//! Reconciles [`ClusterUpdate`](crate::controller_proto::ClusterUpdate) rows by
//! calling [`PrepareSystemUpdate`](crate::node_proto::node_admin_client::NodeAdminClient::prepare_system_update)
//! then [`ActivateSystemUpdate`](crate::node_proto::node_admin_client::NodeAdminClient::activate_system_update)
//! on each target node (serial one-at-a-time MVP).

use std::time::Duration;

use tokio::time;
use tonic::Request;
use tracing::{debug, warn};

use crate::cluster_update_spec::{activation_mode_string, spec_from_json};
use crate::db::{ClusterUpdateNodeRow, ClusterUpdateRow, Database};
use crate::node_client::NodeClients;
use crate::node_proto;

const RECONCILE_TICK: Duration = Duration::from_secs(12);
const NODE_PREPARE_TIMEOUT_SECS: i32 = 3600;
const NODE_ACTIVATE_TIMEOUT_SECS: i32 = 900;

pub fn spawn_cluster_update_reconciler(db: Database, clients: NodeClients) {
    tokio::spawn(async move {
        let mut ticker = time::interval(RECONCILE_TICK);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(e) = reconcile_once(&db, &clients).await {
                warn!(error = %e, "cluster update reconcile tick failed");
            }
        }
    });
}

async fn reconcile_once(db: &Database, clients: &NodeClients) -> Result<(), String> {
    let updates: Vec<ClusterUpdateRow> = db
        .list_cluster_updates_for_reconcile()
        .map_err(|e| format!("list cluster updates: {e}"))?;
    if updates.is_empty() {
        return Ok(());
    }
    debug!(count = updates.len(), "cluster update reconcile");
    for u in updates {
        if let Err(e) = reconcile_update(db, clients, &u).await {
            warn!(
                error = %e,
                name = %u.name,
                "cluster update reconcile failed"
            );
        }
    }
    Ok(())
}

async fn reconcile_update(
    db: &Database,
    clients: &NodeClients,
    update: &ClusterUpdateRow,
) -> Result<(), String> {
    let mut nodes = db
        .list_cluster_update_nodes(&update.name)
        .map_err(|e| format!("list cluster_update_nodes: {e}"))?;
    if nodes.is_empty() {
        return Ok(());
    }

    if update.phase == "ready" {
        db.patch_cluster_update_phase(&update.name, "rolling_out")
            .map_err(|e| format!("patch phase rolling_out: {e}"))?;
    }

    // Terminal cluster phases handled below when all nodes settle.

    if nodes
        .iter()
        .all(|n| n.phase == "succeeded" || n.phase == "cancelled" || n.phase == "failed")
    {
        let any_failed = nodes.iter().any(|n| n.phase == "failed");
        let final_phase = if any_failed {
            "failed"
        } else if nodes.iter().all(|n| n.phase == "cancelled") {
            "cancelled"
        } else {
            "succeeded"
        };
        db.patch_cluster_update_phase(&update.name, final_phase)
            .map_err(|e| format!("finalize cluster update: {e}"))?;
        return Ok(());
    }

    let spec = spec_from_json(&update.spec_json).map_err(|e| format!("parse spec_json: {e}"))?;
    let activation = activation_mode_string(&spec).to_string();

    // Pick the first node still needing work (sorted order preserved from DB).
    for node_row in &mut nodes {
        let node = db
            .get_node(&node_row.node_id)
            .map_err(|e| format!("get node: {e}"))?
            .ok_or_else(|| format!("node {} disappeared", node_row.node_id))?;
        let address = node.address.clone();
        if address.is_empty() {
            warn!(node_id = %node.id, "cluster update: node has no address");
            continue;
        }

        match node_row.phase.as_str() {
            "pending" => {
                let req = node_proto::PrepareSystemUpdateRequest {
                    update_name: update.name.clone(),
                    flake_ref: update.flake_ref.clone(),
                    flake_rev: update.flake_rev.clone(),
                    system_profile: spec
                        .target
                        .as_ref()
                        .map(|t| t.system_profile.clone())
                        .unwrap_or_default(),
                    host_system: String::new(),
                    timeout_seconds: NODE_PREPARE_TIMEOUT_SECS,
                };
                let result = prepare_on_node(clients, &address, req).await;
                match result {
                    Ok(resp) if resp.success => {
                        let closure = resp.prepared_closure.clone();
                        db.upsert_cluster_update_node(&ClusterUpdateNodeRow {
                            update_name: update.name.clone(),
                            node_id: node_row.node_id.clone(),
                            observed_generation: update.generation,
                            phase: "prepared".into(),
                            current_version: String::new(),
                            target_version: update.target_version.clone(),
                            prepared_closure: closure,
                            current_generation: resp.current_generation,
                            target_generation: resp.target_generation,
                            requires_reboot: resp.requires_reboot,
                            last_error: String::new(),
                            last_transition_at: String::new(),
                        })
                        .map_err(|e| format!("upsert node prepared: {e}"))?;
                    }
                    Ok(resp) => {
                        db.upsert_cluster_update_node(&ClusterUpdateNodeRow {
                            update_name: update.name.clone(),
                            node_id: node_row.node_id.clone(),
                            observed_generation: update.generation,
                            phase: "failed".into(),
                            current_version: String::new(),
                            target_version: update.target_version.clone(),
                            prepared_closure: String::new(),
                            current_generation: String::new(),
                            target_generation: String::new(),
                            requires_reboot: false,
                            last_error: resp.message,
                            last_transition_at: String::new(),
                        })
                        .map_err(|e| format!("mark node failed: {e}"))?;
                    }
                    Err(e) => {
                        db.upsert_cluster_update_node(&ClusterUpdateNodeRow {
                            update_name: update.name.clone(),
                            node_id: node_row.node_id.clone(),
                            observed_generation: update.generation,
                            phase: "failed".into(),
                            current_version: String::new(),
                            target_version: update.target_version.clone(),
                            prepared_closure: String::new(),
                            current_generation: String::new(),
                            target_generation: String::new(),
                            requires_reboot: false,
                            last_error: e.clone(),
                            last_transition_at: String::new(),
                        })
                        .map_err(|e2| format!("mark node rpc failed: {e2}"))?;
                    }
                }
                return Ok(());
            }
            "prepared" => {
                let closure = node_row.prepared_closure.clone();
                let req = node_proto::ActivateSystemUpdateRequest {
                    update_name: update.name.clone(),
                    activation_mode: activation.clone(),
                    prepared_closure: closure,
                };
                let result = activate_on_node(clients, &address, req).await;
                match result {
                    Ok(resp) if resp.success => {
                        db.upsert_cluster_update_node(&ClusterUpdateNodeRow {
                            update_name: update.name.clone(),
                            node_id: node_row.node_id.clone(),
                            observed_generation: update.generation,
                            phase: "succeeded".into(),
                            current_version: String::new(),
                            target_version: update.target_version.clone(),
                            prepared_closure: node_row.prepared_closure.clone(),
                            current_generation: node_row.current_generation.clone(),
                            target_generation: node_row.target_generation.clone(),
                            requires_reboot: node_row.requires_reboot,
                            last_error: String::new(),
                            last_transition_at: String::new(),
                        })
                        .map_err(|e| format!("mark node succeeded: {e}"))?;
                    }
                    Ok(resp) => {
                        db.upsert_cluster_update_node(&ClusterUpdateNodeRow {
                            update_name: update.name.clone(),
                            node_id: node_row.node_id.clone(),
                            observed_generation: update.generation,
                            phase: "failed".into(),
                            current_version: String::new(),
                            target_version: update.target_version.clone(),
                            prepared_closure: node_row.prepared_closure.clone(),
                            current_generation: node_row.current_generation.clone(),
                            target_generation: node_row.target_generation.clone(),
                            requires_reboot: node_row.requires_reboot,
                            last_error: resp.message,
                            last_transition_at: String::new(),
                        })
                        .map_err(|e| format!("mark activate failed: {e}"))?;
                    }
                    Err(e) => {
                        db.upsert_cluster_update_node(&ClusterUpdateNodeRow {
                            update_name: update.name.clone(),
                            node_id: node_row.node_id.clone(),
                            observed_generation: update.generation,
                            phase: "failed".into(),
                            current_version: String::new(),
                            target_version: update.target_version.clone(),
                            prepared_closure: node_row.prepared_closure.clone(),
                            current_generation: node_row.current_generation.clone(),
                            target_generation: node_row.target_generation.clone(),
                            requires_reboot: node_row.requires_reboot,
                            last_error: e.clone(),
                            last_transition_at: String::new(),
                        })
                        .map_err(|e2| format!("mark activate rpc failed: {e2}"))?;
                    }
                }
                return Ok(());
            }
            "rolling_back" => {
                let req = node_proto::RollbackSystemUpdateRequest {
                    update_name: update.name.clone(),
                };
                let _ = rollback_on_node(clients, &address, req).await;
                db.upsert_cluster_update_node(&ClusterUpdateNodeRow {
                    update_name: update.name.clone(),
                    node_id: node_row.node_id.clone(),
                    observed_generation: update.generation,
                    phase: "cancelled".into(),
                    current_version: String::new(),
                    target_version: update.target_version.clone(),
                    prepared_closure: String::new(),
                    current_generation: String::new(),
                    target_generation: String::new(),
                    requires_reboot: false,
                    last_error: "rolled back (MVP)".into(),
                    last_transition_at: String::new(),
                })
                .map_err(|e| format!("mark rollback: {e}"))?;
                return Ok(());
            }
            _ => {}
        }
    }
    Ok(())
}

async fn prepare_on_node(
    clients: &NodeClients,
    address: &str,
    req: node_proto::PrepareSystemUpdateRequest,
) -> Result<node_proto::PrepareSystemUpdateResponse, String> {
    if clients.get_admin(address).is_none() {
        clients
            .connect(address)
            .await
            .map_err(|e| format!("connect node {address}: {e}"))?;
    }
    let mut admin = clients
        .get_admin(address)
        .ok_or_else(|| format!("no admin client for {address}"))?;
    Ok(admin
        .prepare_system_update(Request::new(req))
        .await
        .map_err(|e| format!("prepare_system_update: {e}"))?
        .into_inner())
}

async fn activate_on_node(
    clients: &NodeClients,
    address: &str,
    req: node_proto::ActivateSystemUpdateRequest,
) -> Result<node_proto::ActivateSystemUpdateResponse, String> {
    if clients.get_admin(address).is_none() {
        clients
            .connect(address)
            .await
            .map_err(|e| format!("connect node {address}: {e}"))?;
    }
    let mut admin = clients
        .get_admin(address)
        .ok_or_else(|| format!("no admin client for {address}"))?;
    Ok(admin
        .activate_system_update(Request::new(req))
        .await
        .map_err(|e| format!("activate_system_update: {e}"))?
        .into_inner())
}

async fn rollback_on_node(
    clients: &NodeClients,
    address: &str,
    req: node_proto::RollbackSystemUpdateRequest,
) -> Result<node_proto::RollbackSystemUpdateResponse, String> {
    if clients.get_admin(address).is_none() {
        clients
            .connect(address)
            .await
            .map_err(|e| format!("connect node {address}: {e}"))?;
    }
    let mut admin = clients
        .get_admin(address)
        .ok_or_else(|| format!("no admin client for {address}"))?;
    Ok(admin
        .rollback_system_update(Request::new(req))
        .await
        .map_err(|e| format!("rollback_system_update: {e}"))?
        .into_inner())
}
