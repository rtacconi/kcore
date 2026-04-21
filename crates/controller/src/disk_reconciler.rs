//! DiskLayout reconciler.
//!
//! Every tick, list every `disk_layouts` row whose most recent
//! `disk_layout_status.observed_generation` is behind the row's `generation`
//! (or has no status yet), connect to the owning node, and call
//! [`ApplyDiskLayout`] with `apply = true` and `rebuild = true`.
//!
//! The reconciler never touches VMs. If the node-agent refuses the change
//! (classifier says the disk is not idle), the refusal code is recorded in
//! `disk_layout_status` and the operator is expected to drain VMs off the
//! affected disks and resubmit the same manifest — the generation does not
//! bump on unchanged content, so the reconciler will retry the exact same
//! payload until the node accepts or the operator deletes the layout.

use std::time::Duration;

use tokio::time;
use tonic::Request;
use tracing::{debug, error, info, warn};

use crate::db::{Database, DiskLayoutRow, DiskLayoutStatusRow};
use crate::node_client::NodeClients;
use crate::node_proto;

const RECONCILE_TICK: Duration = Duration::from_secs(15);
const NODE_RPC_TIMEOUT_SECS: i32 = 600;

/// Spawn the DiskLayout reconciler loop in the background.
pub fn spawn_disk_layout_reconciler(db: Database, clients: NodeClients) {
    tokio::spawn(async move {
        let mut ticker = time::interval(RECONCILE_TICK);
        // First tick fires immediately, so skip it to avoid a thundering herd
        // on process start.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(e) = reconcile_once(&db, &clients).await {
                warn!(error = %e, "disk layout reconcile tick failed");
            }
        }
    });
}

async fn reconcile_once(db: &Database, clients: &NodeClients) -> Result<(), String> {
    let pending: Vec<DiskLayoutRow> = db
        .list_disk_layouts_needing_reconcile()
        .map_err(|e| format!("list pending disk layouts: {e}"))?;
    if pending.is_empty() {
        return Ok(());
    }
    debug!(count = pending.len(), "reconciling disk layouts");
    for row in pending {
        if let Err(e) = reconcile_row(db, clients, &row).await {
            warn!(
                error = %e,
                name = %row.name,
                node_id = %row.node_id,
                generation = row.generation,
                "disk layout reconcile row failed"
            );
            let _ = db.upsert_disk_layout_status(&DiskLayoutStatusRow {
                name: row.name.clone(),
                observed_generation: 0,
                phase: "failed".to_string(),
                refusal_reason: String::new(),
                message: format!("controller reconcile error: {e}"),
                last_transition_at: String::new(),
            });
        }
    }
    Ok(())
}

async fn reconcile_row(
    db: &Database,
    clients: &NodeClients,
    row: &DiskLayoutRow,
) -> Result<(), String> {
    let node = db
        .get_node(&row.node_id)
        .map_err(|e| format!("lookup node {}: {e}", row.node_id))?
        .ok_or_else(|| format!("node {} not registered", row.node_id))?;
    let address = node.address.clone();
    if address.is_empty() {
        return Err(format!("node {} has no address", row.node_id));
    }

    if clients.get_admin(&address).is_none() {
        clients
            .connect(&address)
            .await
            .map_err(|e| format!("connect to node {address}: {e}"))?;
    }
    let mut admin = clients
        .get_admin(&address)
        .ok_or_else(|| format!("no admin client for {address}"))?;

    let req = node_proto::ApplyDiskLayoutRequest {
        disk_layout_nix: row.layout_nix.clone(),
        apply: true,
        timeout_seconds: NODE_RPC_TIMEOUT_SECS,
        rebuild: true,
    };
    let resp = admin
        .apply_disk_layout(Request::new(req))
        .await
        .map_err(|e| format!("apply_disk_layout rpc: {e}"))?
        .into_inner();

    let phase = if resp.success {
        "applied"
    } else if !resp.refusal_reason.is_empty() {
        "refused"
    } else {
        "failed"
    };
    db.upsert_disk_layout_status(&DiskLayoutStatusRow {
        name: row.name.clone(),
        observed_generation: row.generation,
        phase: phase.to_string(),
        refusal_reason: resp.refusal_reason.clone(),
        message: resp.message.clone(),
        last_transition_at: String::new(),
    })
    .map_err(|e| format!("upsert status for {}: {e}", row.name))?;

    match phase {
        "applied" => info!(
            name = %row.name,
            node_id = %row.node_id,
            generation = row.generation,
            "disk layout applied on node"
        ),
        "refused" => info!(
            name = %row.name,
            node_id = %row.node_id,
            refusal_reason = %resp.refusal_reason,
            "node-agent refused disk layout; operator must drain VMs and resubmit"
        ),
        _ => error!(
            name = %row.name,
            node_id = %row.node_id,
            message = %resp.message,
            "disk layout apply failed on node"
        ),
    }
    Ok(())
}
