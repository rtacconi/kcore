use std::time::Duration;

use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tracing::{info, warn};

use crate::config::{ReplicationConfig, TlsConfig};
use crate::controller_proto;
use crate::db::Database;

const DEFAULT_PAGE_SIZE: i32 = 500;
const MAX_PAGE_SIZE: i32 = 5_000;
const ERROR_BACKOFF_SECS: u64 = 5;
const IDLE_POLL_SECS: u64 = 2;

pub fn spawn_replication_pollers(
    db: Database,
    replication: Option<ReplicationConfig>,
    tls: Option<TlsConfig>,
) {
    let Some(replication) = replication else {
        return;
    };

    if replication.peers.is_empty() {
        return;
    }

    let local_controller_id = replication.controller_id.trim().to_string();
    if local_controller_id.is_empty() {
        warn!("replication configured with peers but empty controller_id; pollers disabled");
        return;
    }

    for peer in replication.peers {
        let peer = peer.trim().to_string();
        if peer.is_empty() {
            continue;
        }
        let db = db.clone();
        let tls = tls.clone();
        let local_controller_id = local_controller_id.clone();
        tokio::spawn(async move {
            replication_peer_loop(db, &peer, &local_controller_id, tls).await;
        });
    }
}

async fn replication_peer_loop(
    db: Database,
    peer: &str,
    local_controller_id: &str,
    tls: Option<TlsConfig>,
) {
    let local_frontier_key = format!("pull/{peer}");
    loop {
        match poll_once(
            &db,
            peer,
            local_controller_id,
            &local_frontier_key,
            tls.as_ref(),
        )
        .await
        {
            Ok(did_work) => {
                if did_work {
                    continue;
                }
                tokio::time::sleep(Duration::from_secs(IDLE_POLL_SECS)).await;
            }
            Err(e) => {
                warn!(peer = %peer, error = %e, "replication poll failed");
                tokio::time::sleep(Duration::from_secs(ERROR_BACKOFF_SECS)).await;
            }
        }
    }
}

async fn poll_once(
    db: &Database,
    peer: &str,
    local_controller_id: &str,
    local_frontier_key: &str,
    tls: Option<&TlsConfig>,
) -> Result<bool, String> {
    let mut client = connect_admin(peer, tls)
        .await
        .map_err(|e| format!("connect admin client: {e}"))?;

    let after_event_id = db
        .get_replication_ack(local_frontier_key)
        .map_err(|e| format!("load local frontier: {e}"))?
        .unwrap_or(0);

    let events = client
        .get_replication_events(controller_proto::GetReplicationEventsRequest {
            after_event_id,
            limit: DEFAULT_PAGE_SIZE.min(MAX_PAGE_SIZE),
        })
        .await
        .map_err(|e| format!("get_replication_events: {e}"))?
        .into_inner()
        .events;

    if events.is_empty() {
        return Ok(false);
    }

    let last_event_id = events.last().map(|e| e.event_id).unwrap_or(after_event_id);
    db.upsert_replication_ack(local_frontier_key, last_event_id)
        .map_err(|e| format!("store local frontier: {e}"))?;

    client
        .ack_replication_events(controller_proto::AckReplicationEventsRequest {
            peer_id: local_controller_id.to_string(),
            last_event_id,
        })
        .await
        .map_err(|e| format!("ack_replication_events: {e}"))?;

    info!(
        peer = %peer,
        events = events.len(),
        from_event = after_event_id,
        to_event = last_event_id,
        "replication poll advanced frontier"
    );
    Ok(true)
}

async fn connect_admin(
    endpoint: &str,
    tls: Option<&TlsConfig>,
) -> Result<controller_proto::controller_admin_client::ControllerAdminClient<Channel>, String> {
    let endpoint = normalize_endpoint(endpoint, tls.is_some());
    let mut ep = Channel::from_shared(endpoint).map_err(|e| e.to_string())?;
    if let Some(tls) = tls {
        let ca_pem = std::fs::read_to_string(&tls.ca_file).map_err(|e| e.to_string())?;
        let cert_pem = std::fs::read_to_string(&tls.cert_file).map_err(|e| e.to_string())?;
        let key_pem = std::fs::read_to_string(&tls.key_file).map_err(|e| e.to_string())?;
        ep = ep.tls_config(
            ClientTlsConfig::new()
                .ca_certificate(Certificate::from_pem(ca_pem))
                .identity(Identity::from_pem(cert_pem, key_pem)),
        )
        .map_err(|e| e.to_string())?;
    }
    let channel = ep.connect().await.map_err(|e| e.to_string())?;
    Ok(controller_proto::controller_admin_client::ControllerAdminClient::new(channel))
}

fn normalize_endpoint(endpoint: &str, use_tls: bool) -> String {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        endpoint.to_string()
    } else if use_tls {
        format!("https://{endpoint}")
    } else {
        format!("http://{endpoint}")
    }
}
