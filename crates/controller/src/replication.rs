use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tracing::{info, warn};

use crate::config::{ReplicationConfig, TlsConfig};
use crate::controller_proto;
use crate::db::{
    Database, NetworkRow, NodeRow, ReplicationResourceHeadRow, SecurityGroupRow,
    SecurityGroupRuleRow, VmRow,
};
use crate::replication_policy::{
    compare_rank, loser_terminal_state, parse_safety_class, parse_validity_class, ArbitrationRank,
};

const DEFAULT_PAGE_SIZE: i32 = 500;
const MAX_PAGE_SIZE: i32 = 5_000;

/// Best-effort detection of a non-loopback, non-link-local IPv4 address
/// when the controller is bound to a wildcard address and no `external_ip`
/// is configured. Returns `None` if no suitable address is found.
fn detect_advertisable_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}
const ERROR_BACKOFF_SECS: u64 = 5;
const IDLE_POLL_SECS: u64 = 2;
const CROSS_DC_IDLE_POLL_SECS: u64 = 5;
const CROSS_DC_ERROR_BACKOFF_SECS: u64 = 15;
const CONNECT_TIMEOUT_SECS: u64 = 10;
const RPC_TIMEOUT_SECS: u64 = 30;
const COMPENSATION_IDLE_SECS: u64 = 2;
const MATERIALIZER_IDLE_SECS: u64 = 2;
const MATERIALIZER_BATCH_SIZE: i64 = 256;
const RESERVATION_MAX_RETRIES: i32 = 3;
const RESERVATION_RETRY_IDLE_SECS: u64 = 2;
const RESERVATION_RETRY_COOLDOWN_SECS: i64 = 5;
const RESERVATION_RETRY_BATCH_SIZE: i64 = 32;

/// Emit a `controller.register` event into the replication outbox so that
/// other controllers discover this one via CRDT materialization.
/// Also upserts self into the local `controller_peers` table.
pub fn emit_controller_register(db: &Database, cfg: &crate::config::Config) {
    let Some(replication) = &cfg.replication else {
        return;
    };
    let controller_id = replication.controller_id.trim();
    if controller_id.is_empty() {
        return;
    }

    let listen_addr = cfg.listen_addr.trim();
    let external_ip = cfg.default_network.external_ip.trim();
    let port = listen_addr
        .rsplit_once(':')
        .map(|(_, p)| p)
        .unwrap_or("9090");
    let address = if !external_ip.is_empty() {
        format!("{external_ip}:{port}")
    } else {
        let host = listen_addr
            .rsplit_once(':')
            .map(|(h, _)| h)
            .unwrap_or(listen_addr);
        let is_wildcard = host == "0.0.0.0" || host == "::" || host == "[::]" || host.is_empty();
        if is_wildcard {
            if let Some(ip) = detect_advertisable_ip() {
                format!("{ip}:{port}")
            } else {
                warn!(
                    "controller bound to wildcard address and no external_ip configured; \
                       emitting listen_addr as-is — peers may not be able to dial this endpoint"
                );
                listen_addr.to_string()
            }
        } else {
            listen_addr.to_string()
        }
    };

    let dc_id = &replication.dc_id;
    let resource_key = format!("controller/{controller_id}");

    let logical_ts_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let envelope = serde_json::json!({
        "schemaVersion": 1,
        "opId": uuid::Uuid::new_v4().to_string(),
        "logicalTsUnixMs": logical_ts_unix_ms,
        "controllerId": controller_id,
        "dcId": dc_id,
        "eventType": "controller.register",
        "resourceKey": resource_key,
        "body": {
            "controllerId": controller_id,
            "address": address,
            "dcId": dc_id,
        },
    });

    if let Ok(payload) = serde_json::to_vec(&envelope) {
        if let Err(e) = db.append_replication_outbox("controller.register", &resource_key, &payload)
        {
            warn!(error = %e, "failed to emit controller.register event");
        } else {
            info!(
                controller_id = %controller_id,
                address = %address,
                "emitted controller.register replication event"
            );
        }
    }

    if let Err(e) = db.upsert_controller_peer(controller_id, &address, dc_id) {
        warn!(error = %e, "failed to upsert local controller peer record");
    }
}

pub fn spawn_replication_pollers(
    db: Database,
    replication: Option<ReplicationConfig>,
    tls: Option<TlsConfig>,
    listen_addr: &str,
) {
    let Some(replication) = replication else {
        return;
    };

    let local_controller_id = replication.controller_id.trim().to_string();
    if local_controller_id.is_empty() {
        warn!("replication configured but empty controller_id; pollers disabled");
        return;
    }
    let local_dc_id = replication.dc_id.clone();

    let active_peers: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let listen_addr_owned = listen_addr.to_string();

    for peer in &replication.peers {
        let peer = peer.trim().to_string();
        if peer.is_empty() {
            continue;
        }
        if same_endpoint(&listen_addr_owned, &peer) {
            info!(peer = %peer, "skipping replication peer that resolves to local controller");
            continue;
        }
        {
            let mut set = active_peers.lock().unwrap_or_else(|e| e.into_inner());
            set.insert(endpoint_host_port(&peer).to_ascii_lowercase());
        }
        let db = db.clone();
        let tls = tls.clone();
        let local_controller_id = local_controller_id.clone();
        // Static peers from config: DC unknown at startup, assume same-DC (faster polling).
        // Real DC will be determined by peer discovery once controller.register materializes.
        tokio::spawn(async move {
            replication_peer_loop(db, &peer, &local_controller_id, tls, false).await;
        });
    }

    spawn_peer_discovery(
        db,
        tls,
        listen_addr_owned,
        local_controller_id,
        local_dc_id,
        active_peers,
    );
}

const PEER_DISCOVERY_INTERVAL_SECS: u64 = 10;

fn spawn_peer_discovery(
    db: Database,
    tls: Option<TlsConfig>,
    listen_addr: String,
    local_controller_id: String,
    local_dc_id: String,
    active_peers: Arc<Mutex<HashSet<String>>>,
) {
    tokio::spawn(async move {
        let mut first_run = true;
        loop {
            if first_run {
                first_run = false;
            } else {
                tokio::time::sleep(Duration::from_secs(PEER_DISCOVERY_INTERVAL_SECS)).await;
            }
            let peers = match db.list_controller_peers() {
                Ok(p) => p,
                Err(e) => {
                    warn!(error = %e, "peer discovery: failed to list controller_peers");
                    continue;
                }
            };
            for peer_row in peers {
                if peer_row.controller_id == local_controller_id {
                    continue;
                }
                let addr = peer_row.address.trim().to_string();
                if addr.is_empty() {
                    continue;
                }
                if same_endpoint(&listen_addr, &addr) {
                    continue;
                }
                let key = endpoint_host_port(&addr).to_ascii_lowercase();
                {
                    let mut set = active_peers.lock().unwrap_or_else(|e| e.into_inner());
                    if set.contains(&key) {
                        continue;
                    }
                    set.insert(key);
                }
                let is_cross_dc = peer_row.dc_id != local_dc_id;
                info!(
                    peer = %addr,
                    controller_id = %peer_row.controller_id,
                    cross_dc = is_cross_dc,
                    "peer discovery: starting replication poller for newly discovered controller"
                );
                let db = db.clone();
                let tls = tls.clone();
                let local_controller_id = local_controller_id.clone();
                tokio::spawn(async move {
                    replication_peer_loop(db, &addr, &local_controller_id, tls, is_cross_dc).await;
                });
            }
        }
    });
}

pub fn spawn_compensation_executor(db: Database) {
    tokio::spawn(async move {
        loop {
            match process_compensation_once(&db) {
                Ok(true) => continue,
                Ok(false) => tokio::time::sleep(Duration::from_secs(COMPENSATION_IDLE_SECS)).await,
                Err(e) => {
                    warn!(error = %e, "compensation executor loop failed");
                    tokio::time::sleep(Duration::from_secs(ERROR_BACKOFF_SECS)).await;
                }
            }
        }
    });
}

pub fn spawn_head_materializer(db: Database) {
    tokio::spawn(async move {
        loop {
            match process_materialization_once(&db) {
                Ok(true) => continue,
                Ok(false) => tokio::time::sleep(Duration::from_secs(MATERIALIZER_IDLE_SECS)).await,
                Err(e) => {
                    warn!(error = %e, "replication head materializer loop failed");
                    tokio::time::sleep(Duration::from_secs(ERROR_BACKOFF_SECS)).await;
                }
            }
        }
    });
}

pub fn spawn_reservation_retry_executor(db: Database) {
    tokio::spawn(async move {
        loop {
            match process_reservation_retry_once(&db) {
                Ok(true) => continue,
                Ok(false) => {
                    tokio::time::sleep(Duration::from_secs(RESERVATION_RETRY_IDLE_SECS)).await
                }
                Err(e) => {
                    warn!(error = %e, "reservation retry executor loop failed");
                    tokio::time::sleep(Duration::from_secs(ERROR_BACKOFF_SECS)).await;
                }
            }
        }
    });
}

async fn replication_peer_loop(
    db: Database,
    peer: &str,
    local_controller_id: &str,
    tls: Option<TlsConfig>,
    is_cross_dc: bool,
) {
    let idle_secs = if is_cross_dc {
        CROSS_DC_IDLE_POLL_SECS
    } else {
        IDLE_POLL_SECS
    };
    let error_secs = if is_cross_dc {
        CROSS_DC_ERROR_BACKOFF_SECS
    } else {
        ERROR_BACKOFF_SECS
    };
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
                tokio::time::sleep(Duration::from_secs(idle_secs)).await;
            }
            Err(e) => {
                warn!(peer = %peer, error = %e, "replication poll failed");
                tokio::time::sleep(Duration::from_secs(error_secs)).await;
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

    let mut last_applied_event_id = after_event_id;
    for event in &events {
        apply_replication_event(db, event)?;
        last_applied_event_id = event.event_id;
    }
    db.upsert_replication_ack(&format!("apply/{peer}"), last_applied_event_id)
        .map_err(|e| format!("store local apply frontier: {e}"))?;
    db.upsert_replication_ack(local_frontier_key, last_applied_event_id)
        .map_err(|e| format!("store local frontier: {e}"))?;

    let ack_peer_id = local_peer_identity(local_controller_id, tls)
        .unwrap_or_else(|| local_controller_id.to_string());
    client
        .ack_replication_events(controller_proto::AckReplicationEventsRequest {
            peer_id: ack_peer_id,
            last_event_id: last_applied_event_id,
        })
        .await
        .map_err(|e| format!("ack_replication_events: {e}"))?;

    info!(
        peer = %peer,
        events = events.len(),
        from_event = after_event_id,
        to_event = last_applied_event_id,
        "replication poll advanced frontier"
    );
    Ok(true)
}

fn local_peer_identity(local_controller_id: &str, tls: Option<&TlsConfig>) -> Option<String> {
    let tls = tls?;
    let cert_pem = std::fs::read_to_string(&tls.cert_file).ok()?;
    use x509_parser::prelude::FromDer;
    let cert = x509_parser::pem::parse_x509_pem(cert_pem.as_bytes())
        .ok()?
        .1;
    let (_, cert) =
        x509_parser::certificate::X509Certificate::from_der(cert.contents.as_slice()).ok()?;
    let cn = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(ToString::to_string);
    cn.or_else(|| Some(local_controller_id.to_string()))
}

fn apply_replication_event(
    db: &Database,
    event: &controller_proto::ReplicationEvent,
) -> Result<(), String> {
    let payload: Value = serde_json::from_slice(&event.payload).map_err(|e| {
        format!(
            "invalid replication payload for event {}: {e}",
            event.event_id
        )
    })?;
    let payload_obj = payload.as_object().ok_or_else(|| {
        format!(
            "invalid replication payload type for event {}: expected object",
            event.event_id
        )
    })?;

    let origin_controller_id = payload_obj
        .get("controllerId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("event {} missing controllerId", event.event_id))?;
    let fallback_op_id = format!("legacy:{}:{}", origin_controller_id, event.event_id);
    let op_id = payload_obj
        .get("opId")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback_op_id.as_str());
    let payload_event_type = payload_obj
        .get("eventType")
        .and_then(|v| v.as_str())
        .unwrap_or(event.event_type.as_str());
    let payload_resource_key = payload_obj
        .get("resourceKey")
        .and_then(|v| v.as_str())
        .unwrap_or(event.resource_key.as_str());
    if payload_event_type != event.event_type {
        return Err(format!(
            "event {} eventType mismatch: payload={}, row={}",
            event.event_id, payload_event_type, event.event_type
        ));
    }
    if payload_resource_key != event.resource_key {
        return Err(format!(
            "event {} resourceKey mismatch: payload={}, row={}",
            event.event_id, payload_resource_key, event.resource_key
        ));
    }
    let logical_ts_unix_ms = payload_obj
        .get("logicalTsUnixMs")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let policy_priority = payload_obj
        .get("policyPriority")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    let intent_epoch = payload_obj
        .get("intentEpoch")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let validity = parse_validity_class(payload_obj.get("validity").and_then(|v| v.as_str()));
    let safety_class = parse_safety_class(payload_obj.get("safetyClass").and_then(|v| v.as_str()));
    let body = payload_obj.get("body").cloned().unwrap_or(Value::Null);

    if db
        .replication_received_op_exists(op_id)
        .map_err(|e| format!("check received op for event {}: {e}", event.event_id))?
    {
        return Ok(());
    }
    db.insert_replication_received_op(
        op_id,
        origin_controller_id,
        payload_event_type,
        payload_resource_key,
    )
    .map_err(|e| format!("insert received op for event {}: {e}", event.event_id))?;

    let existing_head = db
        .get_replication_resource_head(payload_resource_key)
        .map_err(|e| format!("get resource head for event {}: {e}", event.event_id))?;
    let mut replace_head = should_replace_head(
        existing_head.as_ref(),
        policy_priority,
        intent_epoch,
        validity,
        safety_class,
        logical_ts_unix_ms,
        origin_controller_id,
        op_id,
    );
    let mut reservation_rejected = false;
    if replace_head {
        let reservation =
            evaluate_reservation(db, &event.event_type, payload_resource_key, op_id, &body)?;
        if !reservation.accepted {
            // Keep vm.create in the replicated head even when reservation cannot be
            // satisfied yet; materialization retries after node state converges.
            if payload_event_type != "vm.create" {
                replace_head = false;
                reservation_rejected = true;
                let (incumbent_op_id, incumbent_controller_id) = existing_head
                    .as_ref()
                    .map(|h| (h.last_op_id.as_str(), h.last_controller_id.as_str()))
                    .unwrap_or((op_id, origin_controller_id));
                let reason = format!("auto-rejected: reservation failed ({})", reservation.reason);
                let _ = db.insert_replication_conflict_with_resolved(
                    payload_resource_key,
                    incumbent_op_id,
                    op_id,
                    incumbent_controller_id,
                    origin_controller_id,
                    &reason,
                    true,
                );
            }
        }
    }

    if let Some(existing) = existing_head.as_ref() {
        if reservation_rejected {
            return Ok(());
        }
        let reason = if replace_head {
            format!(
                "auto-resolved: challenger wins by deterministic rank (incumbent_op={}, challenger_op={})",
                existing.last_op_id, op_id
            )
        } else {
            let loser_state = loser_terminal_state(validity, safety_class);
            format!(
                "auto-resolved: incumbent remains winner; challenger terminal={:?} (incumbent_op={}, challenger_op={})",
                loser_state, existing.last_op_id, op_id
            )
        };
        let loser_state = if replace_head {
            loser_terminal_state(
                parse_validity_class(Some(&existing.last_validity)),
                parse_safety_class(Some(&existing.last_safety_class)),
            )
        } else {
            loser_terminal_state(validity, safety_class)
        };
        let challenger_is_loser = !replace_head;
        let loser_op_id = if challenger_is_loser {
            op_id
        } else {
            existing.last_op_id.as_str()
        };
        let (loser_event_type, loser_body_json) = if challenger_is_loser {
            (
                payload_event_type.to_string(),
                serde_json::to_string(&body).map_err(|e| {
                    format!("serialize loser body for event {}: {e}", event.event_id)
                })?,
            )
        } else {
            (
                existing.last_event_type.clone(),
                existing.last_body_json.clone(),
            )
        };
        let conflict_id = db
            .insert_replication_conflict_with_resolved(
                payload_resource_key,
                &existing.last_op_id,
                op_id,
                &existing.last_controller_id,
                origin_controller_id,
                &reason,
                loser_state != crate::replication_policy::ReconcileTerminalState::AutoCompensated,
            )
            .map_err(|e| format!("insert conflict for event {}: {e}", event.event_id))?;
        if loser_state == crate::replication_policy::ReconcileTerminalState::AutoCompensated {
            db.insert_compensation_job(
                conflict_id,
                payload_resource_key,
                loser_op_id,
                &loser_event_type,
                &loser_body_json,
            )
            .map_err(|e| format!("insert compensation job for event {}: {e}", event.event_id))?;
        }
    }

    if replace_head {
        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: payload_resource_key.to_string(),
            last_op_id: op_id.to_string(),
            last_logical_ts_unix_ms: logical_ts_unix_ms,
            last_policy_priority: policy_priority,
            last_intent_epoch: intent_epoch,
            last_validity: payload_obj
                .get("validity")
                .and_then(|v| v.as_str())
                .unwrap_or("valid")
                .to_string(),
            last_safety_class: payload_obj
                .get("safetyClass")
                .and_then(|v| v.as_str())
                .unwrap_or("safe")
                .to_string(),
            last_controller_id: origin_controller_id.to_string(),
            last_event_id: event.event_id,
            last_event_type: payload_event_type.to_string(),
            last_body_json: serde_json::to_string(&body)
                .map_err(|e| format!("serialize head body for event {}: {e}", event.event_id))?,
        })
        .map_err(|e| format!("upsert resource head for event {}: {e}", event.event_id))?;
    }

    match event.event_type.as_str() {
        "node.register"
        | "node.heartbeat"
        | "node.approve"
        | "node.reject"
        | "node.drain"
        | "vm.create"
        | "vm.update"
        | "vm.delete"
        | "vm.desired_state.set"
        | "network.create"
        | "network.delete"
        | "security_group.create"
        | "security_group.delete"
        | "security_group.attach"
        | "security_group.detach"
        | "ssh_key.create"
        | "ssh_key.delete"
        | "controller.register" => {
            // Phase-2 skeleton: payload validation + typed dispatch point.
            Ok(())
        }
        _ => {
            warn!(
                event_type = %event.event_type,
                resource_key = %event.resource_key,
                "unknown replication event type; skipping in skeleton apply path"
            );
            Ok(())
        }
    }
}

struct ReservationOutcome {
    accepted: bool,
    reason: String,
}

fn evaluate_reservation(
    db: &Database,
    event_type: &str,
    resource_key: &str,
    op_id: &str,
    body: &Value,
) -> Result<ReservationOutcome, String> {
    if event_type != "vm.create" {
        return Ok(ReservationOutcome {
            accepted: true,
            reason: "not-required".to_string(),
        });
    }
    let Some(node_id) = body.get("nodeId").and_then(Value::as_str) else {
        return Ok(ReservationOutcome {
            accepted: false,
            reason: "missing nodeId".to_string(),
        });
    };
    let reservation_key = format!("node-capacity/{node_id}");
    let node = db
        .get_node(node_id)
        .map_err(|e| format!("load node for reservation {node_id}: {e}"))?;
    if let Some(node) = node {
        if node.approval_status != "approved" {
            let error = "node not approved";
            let (status, retry_count) = db
                .record_replication_reservation_failure(
                    &reservation_key,
                    resource_key,
                    op_id,
                    false,
                    error,
                    RESERVATION_MAX_RETRIES,
                )
                .map_err(|e| format!("record reservation failure {reservation_key}: {e}"))?;
            return Ok(ReservationOutcome {
                accepted: false,
                reason: format!(
                    "{reservation_key}: {error}; status={status}; retry_count={retry_count}"
                ),
            });
        }
        if node.status != "ready" {
            let error = "node not ready";
            let (status, retry_count) = db
                .record_replication_reservation_failure(
                    &reservation_key,
                    resource_key,
                    op_id,
                    true,
                    error,
                    RESERVATION_MAX_RETRIES,
                )
                .map_err(|e| format!("record reservation failure {reservation_key}: {e}"))?;
            return Ok(ReservationOutcome {
                accepted: false,
                reason: format!(
                    "{reservation_key}: {error}; status={status}; retry_count={retry_count}"
                ),
            });
        }
        db.upsert_replication_reservation(&reservation_key, resource_key, op_id, "reserved", "")
            .map_err(|e| format!("reserve token {reservation_key}: {e}"))?;
        Ok(ReservationOutcome {
            accepted: true,
            reason: reservation_key,
        })
    } else {
        let error = "node missing";
        let (status, retry_count) = db
            .record_replication_reservation_failure(
                &reservation_key,
                resource_key,
                op_id,
                false,
                error,
                RESERVATION_MAX_RETRIES,
            )
            .map_err(|e| format!("record reservation failure {reservation_key}: {e}"))?;
        Ok(ReservationOutcome {
            accepted: false,
            reason: format!(
                "{reservation_key}: {error}; status={status}; retry_count={retry_count}"
            ),
        })
    }
}

fn process_reservation_retry_once(db: &Database) -> Result<bool, String> {
    process_reservation_retry_once_with_min_age(db, RESERVATION_RETRY_COOLDOWN_SECS)
}

fn process_reservation_retry_once_with_min_age(
    db: &Database,
    min_age_seconds: i64,
) -> Result<bool, String> {
    let rows = db
        .list_retryable_replication_reservations(RESERVATION_RETRY_BATCH_SIZE, min_age_seconds)
        .map_err(|e| format!("list retryable reservations: {e}"))?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(false);
    };

    let Some(node_id) = row.reservation_key.strip_prefix("node-capacity/") else {
        db.record_replication_reservation_failure(
            &row.reservation_key,
            &row.resource_key,
            &row.op_id,
            false,
            "malformed reservation key",
            RESERVATION_MAX_RETRIES,
        )
        .map_err(|e| {
            format!(
                "mark malformed reservation key {}: {e}",
                row.reservation_key
            )
        })?;
        return Ok(true);
    };

    let node = db
        .get_node(node_id)
        .map_err(|e| format!("load node for retry reservation {node_id}: {e}"))?;
    match node {
        None => {
            let _ = db
                .record_replication_reservation_failure(
                    &row.reservation_key,
                    &row.resource_key,
                    &row.op_id,
                    false,
                    "node missing",
                    RESERVATION_MAX_RETRIES,
                )
                .map_err(|e| {
                    format!(
                        "record non-retryable reservation failure {}: {e}",
                        row.resource_key
                    )
                })?;
        }
        Some(node) if node.approval_status != "approved" => {
            let _ = db
                .record_replication_reservation_failure(
                    &row.reservation_key,
                    &row.resource_key,
                    &row.op_id,
                    false,
                    "node not approved",
                    RESERVATION_MAX_RETRIES,
                )
                .map_err(|e| {
                    format!(
                        "record non-retryable reservation failure {}: {e}",
                        row.resource_key
                    )
                })?;
        }
        Some(node) if node.status != "ready" => {
            let _ = db
                .record_replication_reservation_failure(
                    &row.reservation_key,
                    &row.resource_key,
                    &row.op_id,
                    true,
                    "node not ready",
                    RESERVATION_MAX_RETRIES,
                )
                .map_err(|e| {
                    format!(
                        "record retryable reservation failure {}: {e}",
                        row.resource_key
                    )
                })?;
        }
        Some(_) => {
            db.upsert_replication_reservation_with_retry(
                &row.reservation_key,
                &row.resource_key,
                &row.op_id,
                "reserved",
                "",
                row.retry_count,
            )
            .map_err(|e| format!("mark reservation reserved {}: {e}", row.resource_key))?;
            info!(
                reservation_key = %row.reservation_key,
                resource_key = %row.resource_key,
                retry_count = row.retry_count,
                "reservation retry promoted to reserved"
            );
        }
    }
    Ok(true)
}

fn process_compensation_once(db: &Database) -> Result<bool, String> {
    let Some(job) = db
        .claim_next_compensation_job()
        .map_err(|e| format!("claim compensation job: {e}"))?
    else {
        return Ok(false);
    };

    let result: Result<(), String> = (|| {
        apply_domain_compensation(db, &job)?;
        db.resolve_replication_conflict(job.conflict_id)
            .map_err(|e| format!("resolve conflict {}: {e}", job.conflict_id))?;
        db.complete_compensation_job(job.id)
            .map_err(|e| format!("complete job {}: {e}", job.id))?;
        info!(
            conflict_id = job.conflict_id,
            resource_key = %job.resource_key,
            loser_op_id = %job.loser_op_id,
            loser_event_type = %job.loser_event_type,
            "completed compensation job"
        );
        Ok(())
    })();

    if let Err(e) = result {
        let _ = db.fail_compensation_job(job.id, &e);
        return Err(e);
    }
    Ok(true)
}

fn apply_domain_compensation(
    db: &Database,
    job: &crate::db::ReplicationCompensationJobRow,
) -> Result<(), String> {
    let loser_body: Value = serde_json::from_str(&job.loser_body_json).map_err(|e| {
        format!(
            "parse loser body for compensation job {} (op {}): {e}",
            job.id, job.loser_op_id
        )
    })?;

    match job.loser_event_type.as_str() {
        "vm.create" => {
            if let Some(vm_id) = loser_body.get("vmId").and_then(Value::as_str) {
                let _ = db
                    .delete_vm_by_id_or_name(vm_id)
                    .map_err(|e| format!("compensate vm.create delete {vm_id}: {e}"))?;
            }
        }
        "network.create" => {
            if let (Some(node_id), Some(name)) = (
                loser_body.get("nodeId").and_then(Value::as_str),
                loser_body.get("name").and_then(Value::as_str),
            ) {
                let _ = db.delete_network(node_id, name).map_err(|e| {
                    format!("compensate network.create delete {name} on {node_id}: {e}")
                })?;
            }
        }
        "security_group.create" => {
            if let Some(name) = loser_body.get("name").and_then(Value::as_str) {
                let _ = db
                    .delete_security_group(name)
                    .map_err(|e| format!("compensate security_group.create delete {name}: {e}"))?;
            }
        }
        "ssh_key.create" => {
            if let Some(name) = loser_body.get("name").and_then(Value::as_str) {
                let _ = db
                    .delete_ssh_key(name)
                    .map_err(|e| format!("compensate ssh_key.create delete {name}: {e}"))?;
            }
        }
        _ => {}
    }

    reconcile_resource_to_current_head(db, &job.resource_key)
}

fn reconcile_resource_to_current_head(db: &Database, resource_key: &str) -> Result<(), String> {
    if let Some(head) = db
        .get_replication_resource_head(resource_key)
        .map_err(|e| format!("load head for compensation resource {resource_key}: {e}"))?
    {
        apply_head_to_domain(db, &head)?;
    }
    Ok(())
}

fn process_materialization_once(db: &Database) -> Result<bool, String> {
    let heads = db
        .list_replication_resource_heads(MATERIALIZER_BATCH_SIZE)
        .map_err(|e| format!("list replication heads: {e}"))?;
    for head in &heads {
        let already = db
            .get_materialized_replication_head(&head.resource_key)
            .map_err(|e| format!("get materialized head {}: {e}", head.resource_key))?;
        if already
            .as_ref()
            .map(|row| row.last_op_id == head.last_op_id)
            .unwrap_or(false)
        {
            continue;
        }
        apply_head_to_domain(db, head)?;
        db.upsert_materialized_replication_head(
            &head.resource_key,
            &head.last_op_id,
            &head.last_event_type,
        )
        .map_err(|e| format!("upsert materialized head {}: {e}", head.resource_key))?;
        return Ok(true);
    }
    Ok(false)
}

fn apply_head_to_domain(db: &Database, head: &ReplicationResourceHeadRow) -> Result<(), String> {
    let body: Value = serde_json::from_str(&head.last_body_json)
        .map_err(|e| format!("parse head body for {}: {e}", head.resource_key))?;
    match head.last_event_type.as_str() {
        "node.register" => {
            let node_id = required_str(&body, "nodeId", &head.resource_key)?;
            let existing = db
                .get_node(node_id)
                .map_err(|e| format!("get node {node_id}: {e}"))?;
            let approval_status = body
                .get("approvalStatus")
                .and_then(Value::as_str)
                .unwrap_or("pending");
            let status = body.get("status").and_then(Value::as_str).unwrap_or(
                if approval_status == "approved" {
                    "ready"
                } else {
                    "pending"
                },
            );
            let node = NodeRow {
                id: node_id.to_string(),
                hostname: body
                    .get("hostname")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.hostname.as_str()))
                    .unwrap_or("unknown")
                    .to_string(),
                address: body
                    .get("address")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.address.as_str()))
                    .unwrap_or_default()
                    .to_string(),
                cpu_cores: body
                    .get("cpuCores")
                    .and_then(Value::as_i64)
                    .map(|v| v as i32)
                    .or_else(|| existing.as_ref().map(|n| n.cpu_cores))
                    .unwrap_or(0),
                memory_bytes: body
                    .get("memoryBytes")
                    .and_then(Value::as_i64)
                    .or_else(|| existing.as_ref().map(|n| n.memory_bytes))
                    .unwrap_or(0),
                status: status.to_string(),
                last_heartbeat: body
                    .get("lastHeartbeat")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.last_heartbeat.as_str()))
                    .unwrap_or_default()
                    .to_string(),
                gateway_interface: body
                    .get("gatewayInterface")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.gateway_interface.as_str()))
                    .unwrap_or("eth0")
                    .to_string(),
                cpu_used: body
                    .get("cpuUsed")
                    .and_then(Value::as_i64)
                    .map(|v| v as i32)
                    .or_else(|| existing.as_ref().map(|n| n.cpu_used))
                    .unwrap_or(0),
                memory_used: body
                    .get("memoryUsed")
                    .and_then(Value::as_i64)
                    .or_else(|| existing.as_ref().map(|n| n.memory_used))
                    .unwrap_or(0),
                storage_backend: storage_backend_from_i32(
                    body.get("storageBackend")
                        .and_then(Value::as_i64)
                        .unwrap_or_default() as i32,
                )
                .to_string(),
                disable_vxlan: body
                    .get("disableVxlan")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                approval_status: approval_status.to_string(),
                cert_expiry_days: body
                    .get("certExpiryDays")
                    .and_then(Value::as_i64)
                    .map(|v| v as i32)
                    .or_else(|| existing.as_ref().map(|n| n.cert_expiry_days))
                    .unwrap_or(-1),
                luks_method: body
                    .get("luksMethod")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.luks_method.as_str()))
                    .unwrap_or_default()
                    .to_string(),
                dc_id: body
                    .get("dcId")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.dc_id.as_str()))
                    .unwrap_or_default()
                    .to_string(),
            };
            db.upsert_node(&node)
                .map_err(|e| format!("upsert node {node_id}: {e}"))?;
            let labels = body
                .get("labels")
                .and_then(Value::as_array)
                .map(|vals| {
                    vals.iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !labels.is_empty() {
                db.upsert_node_labels(node_id, &labels)
                    .map_err(|e| format!("upsert node labels {node_id}: {e}"))?;
            }
            Ok(())
        }
        "node.heartbeat" => {
            let node_id = required_str(&body, "nodeId", &head.resource_key)?;
            let existing = db
                .get_node(node_id)
                .map_err(|e| format!("get node {node_id}: {e}"))?;
            let node = NodeRow {
                id: node_id.to_string(),
                hostname: body
                    .get("hostname")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.hostname.as_str()))
                    .unwrap_or(node_id)
                    .to_string(),
                address: body
                    .get("address")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.address.as_str()))
                    .unwrap_or_default()
                    .to_string(),
                cpu_cores: body
                    .get("cpuCores")
                    .and_then(Value::as_i64)
                    .map(|v| v as i32)
                    .or_else(|| existing.as_ref().map(|n| n.cpu_cores))
                    .unwrap_or(0),
                memory_bytes: body
                    .get("memoryBytes")
                    .and_then(Value::as_i64)
                    .or_else(|| existing.as_ref().map(|n| n.memory_bytes))
                    .unwrap_or(0),
                status: body
                    .get("status")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.status.as_str()))
                    .unwrap_or("pending")
                    .to_string(),
                last_heartbeat: body
                    .get("lastHeartbeat")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.last_heartbeat.as_str()))
                    .unwrap_or_default()
                    .to_string(),
                gateway_interface: body
                    .get("gatewayInterface")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.gateway_interface.as_str()))
                    .unwrap_or("eth0")
                    .to_string(),
                cpu_used: body
                    .get("cpuUsed")
                    .and_then(Value::as_i64)
                    .map(|v| v as i32)
                    .or_else(|| existing.as_ref().map(|n| n.cpu_used))
                    .unwrap_or(0),
                memory_used: body
                    .get("memoryUsed")
                    .and_then(Value::as_i64)
                    .or_else(|| existing.as_ref().map(|n| n.memory_used))
                    .unwrap_or(0),
                storage_backend: storage_backend_from_i32(
                    body.get("storageBackend")
                        .and_then(Value::as_i64)
                        .unwrap_or_default() as i32,
                )
                .to_string(),
                disable_vxlan: body
                    .get("disableVxlan")
                    .and_then(Value::as_bool)
                    .or_else(|| existing.as_ref().map(|n| n.disable_vxlan))
                    .unwrap_or(false),
                approval_status: body
                    .get("approvalStatus")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.approval_status.as_str()))
                    .unwrap_or("pending")
                    .to_string(),
                cert_expiry_days: body
                    .get("certExpiryDays")
                    .and_then(Value::as_i64)
                    .map(|v| v as i32)
                    .or_else(|| existing.as_ref().map(|n| n.cert_expiry_days))
                    .unwrap_or(-1),
                luks_method: body
                    .get("luksMethod")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.luks_method.as_str()))
                    .unwrap_or_default()
                    .to_string(),
                dc_id: body
                    .get("dcId")
                    .and_then(Value::as_str)
                    .or_else(|| existing.as_ref().map(|n| n.dc_id.as_str()))
                    .unwrap_or_default()
                    .to_string(),
            };
            db.upsert_node(&node)
                .map_err(|e| format!("upsert heartbeat node {node_id}: {e}"))?;
            let labels = body
                .get("labels")
                .and_then(Value::as_array)
                .map(|vals| {
                    vals.iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !labels.is_empty() {
                db.upsert_node_labels(node_id, &labels)
                    .map_err(|e| format!("upsert node labels {node_id}: {e}"))?;
            }
            Ok(())
        }
        "vm.create" => {
            let vm_id = required_str(&body, "vmId", &head.resource_key)?;
            let node_id = body
                .get("nodeId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !node_id.is_empty() {
                ensure_replicated_node_exists(db, &node_id)?;
            }
            let vm = VmRow {
                id: vm_id.to_string(),
                name: body
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(vm_id)
                    .to_string(),
                cpu: body.get("cpu").and_then(Value::as_i64).unwrap_or(2) as i32,
                memory_bytes: body
                    .get("memoryBytes")
                    .and_then(Value::as_i64)
                    .unwrap_or(2 * 1024 * 1024 * 1024),
                image_path: body
                    .get("imagePath")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                image_url: body
                    .get("imageUrl")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                image_sha256: body
                    .get("imageSha256")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                image_format: body
                    .get("imageFormat")
                    .and_then(Value::as_str)
                    .unwrap_or("qcow2")
                    .to_string(),
                image_size: body.get("imageSize").and_then(Value::as_i64).unwrap_or(0),
                network: body
                    .get("network")
                    .and_then(Value::as_str)
                    .unwrap_or("default")
                    .to_string(),
                auto_start: body
                    .get("autoStart")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                node_id,
                created_at: String::new(),
                runtime_state: body
                    .get("runtimeState")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                cloud_init_user_data: body
                    .get("cloudInitUserData")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                storage_backend: body
                    .get("storageBackend")
                    .and_then(Value::as_str)
                    .unwrap_or("filesystem")
                    .to_string(),
                storage_size_bytes: body
                    .get("storageSizeBytes")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                vm_ip: body
                    .get("vmIp")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            };
            let _ = db.delete_vm_by_id_or_name(vm_id);
            db.insert_vm(&vm)
                .map_err(|e| format!("insert vm {vm_id} from replication head: {e}"))?;
            let ssh_keys: Vec<String> = body
                .get("sshKeyNames")
                .and_then(Value::as_array)
                .map(|vals| {
                    vals.iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if !ssh_keys.is_empty() {
                db.associate_vm_ssh_keys(vm_id, &ssh_keys)
                    .map_err(|e| format!("associating SSH keys for vm {vm_id}: {e}"))?;
            }
            Ok(())
        }
        "vm.desired_state.set" => {
            let vm_id = vm_id_from_body_or_resource(&body, &head.resource_key)?;
            let auto_start = body
                .get("autoStart")
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("missing autoStart for {}", head.resource_key))?;
            let _ = db
                .set_vm_auto_start(vm_id, auto_start)
                .map_err(|e| format!("set vm auto_start {vm_id}: {e}"))?;
            Ok(())
        }
        "vm.update" => {
            let vm_id = vm_id_from_body_or_resource(&body, &head.resource_key)?;
            let cpu = body
                .get("cpu")
                .and_then(Value::as_i64)
                .and_then(|v| i32::try_from(v).ok());
            let memory_bytes = body.get("memoryBytes").and_then(Value::as_i64);
            let _ = db
                .update_vm_spec(vm_id, cpu, memory_bytes)
                .map_err(|e| format!("update vm spec {vm_id}: {e}"))?;
            Ok(())
        }
        "vm.delete" => {
            let vm_id = vm_id_from_body_or_resource(&body, &head.resource_key)?;
            let _ = db
                .delete_vm_by_id_or_name(vm_id)
                .map_err(|e| format!("delete vm {vm_id}: {e}"))?;
            Ok(())
        }
        "node.approve" => {
            let node_id = body
                .get("nodeId")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("missing nodeId for {}", head.resource_key))?;
            let _ = db
                .set_node_approval(node_id, "approved")
                .map_err(|e| format!("set node approval approved {node_id}: {e}"))?;
            let _ = db
                .update_node_status(node_id, "ready")
                .map_err(|e| format!("set node status ready {node_id}: {e}"))?;
            Ok(())
        }
        "node.reject" => {
            let node_id = body
                .get("nodeId")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("missing nodeId for {}", head.resource_key))?;
            let _ = db
                .set_node_approval(node_id, "rejected")
                .map_err(|e| format!("set node approval rejected {node_id}: {e}"))?;
            let _ = db
                .update_node_status(node_id, "rejected")
                .map_err(|e| format!("set node status rejected {node_id}: {e}"))?;
            Ok(())
        }
        "node.drain" => {
            let node_id = body
                .get("nodeId")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("missing nodeId for {}", head.resource_key))?;
            let _ = db
                .update_node_status(node_id, "drained")
                .map_err(|e| format!("set node status drained {node_id}: {e}"))?;
            Ok(())
        }
        "network.create" => {
            let node_id = required_str(&body, "nodeId", &head.resource_key)?;
            let name = required_str(&body, "name", &head.resource_key)?;
            ensure_replicated_node_exists(db, node_id)?;
            let network = NetworkRow {
                name: name.to_string(),
                external_ip: body
                    .get("externalIp")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                gateway_ip: body
                    .get("gatewayIp")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                internal_netmask: body
                    .get("internalNetmask")
                    .and_then(Value::as_str)
                    .unwrap_or("255.255.255.0")
                    .to_string(),
                node_id: node_id.to_string(),
                allowed_tcp_ports: body
                    .get("allowedTcpPorts")
                    .map(json_as_compact_string)
                    .unwrap_or_else(|| "[]".to_string()),
                allowed_udp_ports: body
                    .get("allowedUdpPorts")
                    .map(json_as_compact_string)
                    .unwrap_or_else(|| "[]".to_string()),
                vlan_id: body.get("vlanId").and_then(Value::as_i64).unwrap_or(0) as i32,
                network_type: body
                    .get("networkType")
                    .and_then(Value::as_str)
                    .unwrap_or("nat")
                    .to_string(),
                enable_outbound_nat: body
                    .get("enableOutboundNat")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                vni: body.get("vni").and_then(Value::as_i64).unwrap_or(0) as i32,
                next_ip: body.get("nextIp").and_then(Value::as_i64).unwrap_or(2) as i32,
            };
            let _ = db.delete_network(node_id, name);
            db.insert_network(&network)
                .map_err(|e| format!("insert network {name} on {node_id}: {e}"))?;
            Ok(())
        }
        "network.delete" => {
            let node_id = required_str(&body, "nodeId", &head.resource_key)?;
            let name = required_str(&body, "name", &head.resource_key)?;
            let _ = db
                .delete_network(node_id, name)
                .map_err(|e| format!("delete network {name} on {node_id}: {e}"))?;
            Ok(())
        }
        "security_group.create" => {
            let name = required_str(&body, "name", &head.resource_key)?;
            let sg = SecurityGroupRow {
                name: name.to_string(),
                description: body
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                created_at: String::new(),
            };
            db.upsert_security_group(&sg)
                .map_err(|e| format!("upsert security group {name}: {e}"))?;
            if let Some(rule_vals) = body.get("rules").and_then(Value::as_array) {
                let rules = rule_vals
                    .iter()
                    .enumerate()
                    .map(|(idx, rule)| SecurityGroupRuleRow {
                        id: rule
                            .get("id")
                            .and_then(Value::as_str)
                            .map(ToString::to_string)
                            .unwrap_or_else(|| format!("rule-{idx}")),
                        security_group: name.to_string(),
                        protocol: rule
                            .get("protocol")
                            .and_then(Value::as_str)
                            .unwrap_or("tcp")
                            .to_string(),
                        host_port: rule.get("hostPort").and_then(Value::as_i64).unwrap_or(0) as i32,
                        target_port: rule.get("targetPort").and_then(Value::as_i64).unwrap_or(0)
                            as i32,
                        source_cidr: rule
                            .get("sourceCidr")
                            .and_then(Value::as_str)
                            .unwrap_or("0.0.0.0/0")
                            .to_string(),
                        target_vm: rule
                            .get("targetVm")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        enable_dnat: rule
                            .get("enableDnat")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                    .collect::<Vec<_>>();
                db.replace_security_group_rules(name, &rules)
                    .map_err(|e| format!("replace security group rules for {name}: {e}"))?;
            }
            Ok(())
        }
        "security_group.delete" => {
            let name = required_str(&body, "name", &head.resource_key)?;
            let _ = db
                .delete_security_group(name)
                .map_err(|e| format!("delete security group {name}: {e}"))?;
            Ok(())
        }
        "security_group.attach" => {
            let sg = required_str(&body, "securityGroup", &head.resource_key)?;
            let target_id = required_str(&body, "targetId", &head.resource_key)?;
            let target_kind = body
                .get("targetKind")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            match target_kind {
                1 => db
                    .attach_security_group_to_vm(sg, target_id)
                    .map_err(|e| format!("attach security group {sg} to vm {target_id}: {e}"))?,
                2 => {
                    let node_id = required_str(&body, "targetNode", &head.resource_key)?;
                    ensure_replicated_node_exists(db, node_id)?;
                    db.attach_security_group_to_network(sg, target_id, node_id)
                        .map_err(|e| {
                            format!(
                                "attach security group {sg} to network {target_id} on {node_id}: {e}"
                            )
                        })?
                }
                other => {
                    return Err(format!(
                        "unsupported security_group.attach targetKind {other} for {}",
                        head.resource_key
                    ));
                }
            }
            Ok(())
        }
        "security_group.detach" => {
            let sg = required_str(&body, "securityGroup", &head.resource_key)?;
            let target_id = required_str(&body, "targetId", &head.resource_key)?;
            let target_kind = body
                .get("targetKind")
                .and_then(Value::as_i64)
                .unwrap_or_default();
            match target_kind {
                1 => {
                    let _ = db
                        .detach_security_group_from_vm(sg, target_id)
                        .map_err(|e| {
                            format!("detach security group {sg} from vm {target_id}: {e}")
                        })?;
                }
                2 => {
                    let node_id = required_str(&body, "targetNode", &head.resource_key)?;
                    let _ = db
                        .detach_security_group_from_network(sg, target_id, node_id)
                        .map_err(|e| {
                            format!(
                                "detach security group {sg} from network {target_id} on {node_id}: {e}"
                            )
                        })?;
                }
                other => {
                    return Err(format!(
                        "unsupported security_group.detach targetKind {other} for {}",
                        head.resource_key
                    ));
                }
            }
            Ok(())
        }
        "ssh_key.create" => {
            let name = body
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("missing name for {}", head.resource_key))?;
            let public_key = body
                .get("publicKey")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("missing publicKey for {}", head.resource_key))?;
            db.upsert_ssh_key(name, public_key)
                .map_err(|e| format!("upsert ssh key {name}: {e}"))?;
            Ok(())
        }
        "ssh_key.delete" => {
            let name = body
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("missing name for {}", head.resource_key))?;
            db.delete_ssh_key(name)
                .map_err(|e| format!("delete ssh key {name}: {e}"))?;
            Ok(())
        }
        "controller.register" => {
            let controller_id = required_str(&body, "controllerId", &head.resource_key)?;
            let address = required_str(&body, "address", &head.resource_key)?;
            let dc_id = body.get("dcId").and_then(Value::as_str).unwrap_or("DC1");
            db.upsert_controller_peer(controller_id, address, dc_id)
                .map_err(|e| format!("upsert controller peer {controller_id}: {e}"))?;
            Ok(())
        }
        _ => Ok(()),
    }
}

fn required_str<'a>(body: &'a Value, key: &str, resource_key: &str) -> Result<&'a str, String> {
    body.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {key} for {resource_key}"))
}

fn ensure_replicated_node_exists(db: &Database, node_id: &str) -> Result<(), String> {
    if db
        .get_node(node_id)
        .map_err(|e| format!("read node {node_id}: {e}"))?
        .is_some()
    {
        return Ok(());
    }

    let placeholder = NodeRow {
        id: node_id.to_string(),
        hostname: node_id.to_string(),
        address: format!("{node_id}.replicated:9091"),
        cpu_cores: 0,
        memory_bytes: 0,
        status: "pending".to_string(),
        last_heartbeat: String::new(),
        gateway_interface: String::new(),
        cpu_used: 0,
        memory_used: 0,
        storage_backend: "filesystem".to_string(),
        disable_vxlan: false,
        approval_status: "pending".to_string(),
        cert_expiry_days: 0,
        luks_method: "unknown".to_string(),
        dc_id: String::new(),
    };
    db.upsert_node(&placeholder)
        .map_err(|e| format!("create placeholder node {node_id}: {e}"))
}

fn vm_id_from_body_or_resource<'a>(
    body: &'a Value,
    resource_key: &'a str,
) -> Result<&'a str, String> {
    if let Some(vm_id) = body.get("vmId").and_then(Value::as_str) {
        return Ok(vm_id);
    }
    resource_key
        .strip_prefix("vm/")
        .ok_or_else(|| format!("missing vmId for {resource_key}"))
}

fn json_as_compact_string(value: &Value) -> String {
    if let Some(as_str) = value.as_str() {
        as_str.to_string()
    } else {
        serde_json::to_string(value).unwrap_or_else(|_| "[]".to_string())
    }
}

fn storage_backend_from_i32(raw: i32) -> &'static str {
    let kind = controller_proto::StorageBackendType::try_from(raw)
        .unwrap_or(controller_proto::StorageBackendType::Unspecified);
    match kind {
        controller_proto::StorageBackendType::Lvm => "lvm",
        controller_proto::StorageBackendType::Zfs => "zfs",
        controller_proto::StorageBackendType::Filesystem
        | controller_proto::StorageBackendType::Unspecified => "filesystem",
    }
}

fn should_replace_head(
    existing: Option<&ReplicationResourceHeadRow>,
    policy_priority: i32,
    intent_epoch: i64,
    validity: crate::replication_policy::ValidityClass,
    safety_class: crate::replication_policy::SafetyClass,
    logical_ts_unix_ms: i64,
    controller_id: &str,
    op_id: &str,
) -> bool {
    let Some(existing) = existing else {
        return true;
    };
    compare_rank(
        ArbitrationRank {
            validity,
            safety: safety_class,
            policy_priority,
            intent_epoch,
            logical_ts_unix_ms,
            controller_id,
            op_id,
        },
        ArbitrationRank {
            validity: parse_validity_class(Some(&existing.last_validity)),
            safety: parse_safety_class(Some(&existing.last_safety_class)),
            policy_priority: existing.last_policy_priority,
            intent_epoch: existing.last_intent_epoch,
            logical_ts_unix_ms: existing.last_logical_ts_unix_ms,
            controller_id: &existing.last_controller_id,
            op_id: &existing.last_op_id,
        },
    )
    .is_gt()
}

async fn connect_admin(
    endpoint: &str,
    tls: Option<&TlsConfig>,
) -> Result<controller_proto::controller_admin_client::ControllerAdminClient<Channel>, String> {
    let endpoint = normalize_endpoint(endpoint, tls.is_some());
    let mut ep = Channel::from_shared(endpoint).map_err(|e| e.to_string())?;
    ep = ep
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(RPC_TIMEOUT_SECS));
    if let Some(tls) = tls {
        let ca_pem = std::fs::read_to_string(&tls.ca_file).map_err(|e| e.to_string())?;
        let cert_pem = std::fs::read_to_string(&tls.cert_file).map_err(|e| e.to_string())?;
        let key_pem = std::fs::read_to_string(&tls.key_file).map_err(|e| e.to_string())?;
        ep = ep
            .tls_config(
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

fn endpoint_host_port(endpoint: &str) -> &str {
    if endpoint.len() >= 7 && endpoint[..7].eq_ignore_ascii_case("http://") {
        &endpoint[7..]
    } else if endpoint.len() >= 8 && endpoint[..8].eq_ignore_ascii_case("https://") {
        &endpoint[8..]
    } else {
        endpoint
    }
}

fn same_endpoint(a: &str, b: &str) -> bool {
    endpoint_host_port(a).eq_ignore_ascii_case(endpoint_host_port(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{NodeRow, VmRow};
    use std::{env, fs};

    fn test_node(id: &str) -> NodeRow {
        NodeRow {
            id: id.to_string(),
            hostname: "n1".to_string(),
            address: "10.0.0.10:9443".to_string(),
            cpu_cores: 4,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            status: "ready".to_string(),
            last_heartbeat: String::new(),
            gateway_interface: "eth0".to_string(),
            cpu_used: 0,
            memory_used: 0,
            storage_backend: "fs".to_string(),
            disable_vxlan: false,
            approval_status: "approved".to_string(),
            cert_expiry_days: -1,
            luks_method: String::new(),
            dc_id: "DC1".to_string(),
        }
    }

    fn test_vm(id: &str, node_id: &str) -> VmRow {
        VmRow {
            id: id.to_string(),
            name: "vm-name".to_string(),
            cpu: 1,
            memory_bytes: 512 * 1024 * 1024,
            image_path: "/var/lib/kcore/images/vm-name.qcow2".to_string(),
            image_url: String::new(),
            image_sha256: String::new(),
            image_format: "qcow2".to_string(),
            image_size: 0,
            network: "default".to_string(),
            auto_start: false,
            node_id: node_id.to_string(),
            created_at: String::new(),
            runtime_state: "stopped".to_string(),
            cloud_init_user_data: String::new(),
            storage_backend: "fs".to_string(),
            storage_size_bytes: 0,
            vm_ip: String::new(),
        }
    }

    #[test]
    fn normalize_endpoint_adds_scheme() {
        assert_eq!(
            normalize_endpoint("10.0.0.10:9090", true),
            "https://10.0.0.10:9090"
        );
        assert_eq!(
            normalize_endpoint("10.0.0.10:9090", false),
            "http://10.0.0.10:9090"
        );
        assert_eq!(
            normalize_endpoint("https://10.0.0.10:9090", true),
            "https://10.0.0.10:9090"
        );
    }

    #[test]
    fn same_endpoint_ignores_scheme() {
        assert!(same_endpoint("10.0.0.10:9090", "https://10.0.0.10:9090"));
        assert!(same_endpoint("HTTP://10.0.0.10:9090", "10.0.0.10:9090"));
        assert!(!same_endpoint("10.0.0.10:9090", "10.0.0.11:9090"));
    }

    #[test]
    fn apply_replication_event_rejects_non_object_payload() {
        let event = controller_proto::ReplicationEvent {
            event_id: 7,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            event_type: "vm.create".to_string(),
            resource_key: "vm/v1".to_string(),
            payload: br#"[1,2,3]"#.to_vec(),
        };
        let db = Database::open(":memory:").expect("open db");
        let err = apply_replication_event(&db, &event).expect_err("must fail");
        assert!(err.contains("expected object") || err.contains("missing opId"));
    }

    #[test]
    fn apply_replication_event_dedupes_by_op_id() {
        let db = Database::open(":memory:").expect("open db");
        let payload = br#"{"opId":"op-1","controllerId":"ctrl-a","eventType":"vm.create","resourceKey":"vm/v1","body":{}}"#.to_vec();
        let event = controller_proto::ReplicationEvent {
            event_id: 1,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            event_type: "vm.create".to_string(),
            resource_key: "vm/v1".to_string(),
            payload: payload.clone(),
        };
        apply_replication_event(&db, &event).expect("first apply");
        apply_replication_event(&db, &event).expect("duplicate apply should be noop");
        assert!(db
            .replication_received_op_exists("op-1")
            .expect("received op exists"));
    }

    #[test]
    fn apply_replication_event_updates_lww_resource_head() {
        let db = Database::open(":memory:").expect("open db");
        let older = controller_proto::ReplicationEvent {
            event_id: 1,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            event_type: "vm.update".to_string(),
            resource_key: "vm/v1".to_string(),
            payload: br#"{"opId":"op-1","controllerId":"ctrl-a","logicalTsUnixMs":100,"eventType":"vm.update","resourceKey":"vm/v1","body":{"cpu":1}}"#.to_vec(),
        };
        let newer = controller_proto::ReplicationEvent {
            event_id: 2,
            created_at: "2026-01-01T00:00:01Z".to_string(),
            event_type: "vm.update".to_string(),
            resource_key: "vm/v1".to_string(),
            payload: br#"{"opId":"op-2","controllerId":"ctrl-b","logicalTsUnixMs":200,"eventType":"vm.update","resourceKey":"vm/v1","body":{"cpu":2}}"#.to_vec(),
        };
        apply_replication_event(&db, &newer).expect("apply newer");
        apply_replication_event(&db, &older).expect("apply older");
        let head = db
            .get_replication_resource_head("vm/v1")
            .expect("get head")
            .expect("head exists");
        assert_eq!(head.last_op_id, "op-2");
        assert_eq!(head.last_logical_ts_unix_ms, 200);
    }

    #[test]
    fn apply_replication_event_records_conflict_on_equal_logical_ts() {
        let db = Database::open(":memory:").expect("open db");
        let first = controller_proto::ReplicationEvent {
            event_id: 1,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            event_type: "vm.update".to_string(),
            resource_key: "vm/v2".to_string(),
            payload: br#"{"opId":"op-a","controllerId":"ctrl-a","logicalTsUnixMs":500,"eventType":"vm.update","resourceKey":"vm/v2","body":{"cpu":1}}"#.to_vec(),
        };
        let second = controller_proto::ReplicationEvent {
            event_id: 2,
            created_at: "2026-01-01T00:00:01Z".to_string(),
            event_type: "vm.update".to_string(),
            resource_key: "vm/v2".to_string(),
            payload: br#"{"opId":"op-b","controllerId":"ctrl-b","logicalTsUnixMs":500,"eventType":"vm.update","resourceKey":"vm/v2","body":{"cpu":2}}"#.to_vec(),
        };
        apply_replication_event(&db, &first).expect("first");
        apply_replication_event(&db, &second).expect("second");
        assert_eq!(
            db.count_unresolved_replication_conflicts()
                .expect("count conflicts"),
            0
        );
    }

    #[test]
    fn apply_replication_event_creates_compensation_job_for_unsafe_loser() {
        let db = Database::open(":memory:").expect("open db");
        let winner = controller_proto::ReplicationEvent {
            event_id: 1,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            event_type: "vm.update".to_string(),
            resource_key: "vm/v3".to_string(),
            payload: br#"{"opId":"op-w","controllerId":"ctrl-a","logicalTsUnixMs":600,"eventType":"vm.update","resourceKey":"vm/v3","safetyClass":"safe","body":{"cpu":1}}"#.to_vec(),
        };
        let unsafe_loser = controller_proto::ReplicationEvent {
            event_id: 2,
            created_at: "2026-01-01T00:00:01Z".to_string(),
            event_type: "vm.update".to_string(),
            resource_key: "vm/v3".to_string(),
            payload: br#"{"opId":"op-l","controllerId":"ctrl-b","logicalTsUnixMs":500,"eventType":"vm.update","resourceKey":"vm/v3","safetyClass":"unsafe","body":{"cpu":8}}"#.to_vec(),
        };
        apply_replication_event(&db, &winner).expect("winner");
        apply_replication_event(&db, &unsafe_loser).expect("loser");
        assert_eq!(
            db.count_unresolved_replication_conflicts()
                .expect("count conflicts"),
            1
        );
        assert_eq!(
            db.count_pending_compensation_jobs().expect("pending jobs"),
            1
        );
        process_compensation_once(&db).expect("process one");
        assert_eq!(
            db.count_unresolved_replication_conflicts()
                .expect("count conflicts"),
            0
        );
    }

    #[test]
    fn materializer_applies_vm_desired_state_and_tracks_frontier() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node("node-1");
        db.upsert_node(&node).expect("insert node");
        db.insert_vm(&test_vm("vm-1", &node.id)).expect("insert vm");
        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "vm/vm-1".to_string(),
            last_op_id: "op-1".to_string(),
            last_logical_ts_unix_ms: 100,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 1,
            last_event_type: "vm.desired_state.set".to_string(),
            last_body_json: r#"{"vmId":"vm-1","autoStart":true}"#.to_string(),
        })
        .expect("upsert head");

        assert!(process_materialization_once(&db).expect("materialize"));
        let vm = db.get_vm("vm-1").expect("get vm").expect("vm exists");
        assert!(vm.auto_start);

        // Replay-safe: same winner op should not produce further work.
        assert!(!process_materialization_once(&db).expect("materialize no-op"));
    }

    #[test]
    fn materializer_applies_node_register_with_labels() {
        let db = Database::open(":memory:").expect("open db");
        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "node/node-2".to_string(),
            last_op_id: "op-node-2".to_string(),
            last_logical_ts_unix_ms: 100,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 11,
            last_event_type: "node.register".to_string(),
            last_body_json: r#"{"nodeId":"node-2","hostname":"node2","address":"10.0.0.2:9091","cpuCores":8,"memoryBytes":17179869184,"status":"pending","gatewayInterface":"","storageBackend":1,"disableVxlan":false,"approvalStatus":"pending","certExpiryDays":30,"luksMethod":"tpm2","labels":["dc=DC1","role=edge"]}"#.to_string(),
        })
        .expect("upsert head");

        assert!(process_materialization_once(&db).expect("materialize"));
        let node = db.get_node("node-2").expect("db").expect("node");
        assert_eq!(node.hostname, "node2");
        let labels = db.get_node_labels("node-2").expect("labels");
        assert_eq!(labels.len(), 2);
    }

    #[test]
    fn materializer_applies_node_heartbeat_snapshot() {
        let db = Database::open(":memory:").expect("open db");
        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "node/node-hb".to_string(),
            last_op_id: "op-node-hb".to_string(),
            last_logical_ts_unix_ms: 100,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 12,
            last_event_type: "node.heartbeat".to_string(),
            last_body_json: r#"{"nodeId":"node-hb","hostname":"node-hb","address":"10.0.0.9:9091","cpuCores":8,"memoryBytes":17179869184,"status":"ready","gatewayInterface":"eno1","storageBackend":1,"disableVxlan":false,"approvalStatus":"approved","certExpiryDays":300,"luksMethod":"tpm2","cpuUsed":2,"memoryUsed":4096,"lastHeartbeat":"2026-04-12 00:00:00","labels":["dc=DC1","role=edge"]}"#.to_string(),
        })
        .expect("upsert head");

        assert!(process_materialization_once(&db).expect("materialize"));
        let node = db.get_node("node-hb").expect("db").expect("node");
        assert_eq!(node.status, "ready");
        assert_eq!(node.approval_status, "approved");
        assert_eq!(node.cpu_used, 2);
        assert_eq!(node.memory_used, 4096);
        assert!(!node.last_heartbeat.is_empty());
        let labels = db.get_node_labels("node-hb").expect("labels");
        assert_eq!(labels.len(), 2);
    }

    #[test]
    fn materializer_applies_network_create_and_delete() {
        let db = Database::open(":memory:").expect("open db");
        db.upsert_node(&test_node("node-net")).expect("insert node");

        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "network/node-net/vxlan-test".to_string(),
            last_op_id: "op-net-create".to_string(),
            last_logical_ts_unix_ms: 100,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 12,
            last_event_type: "network.create".to_string(),
            last_body_json: r#"{"name":"vxlan-test","nodeId":"node-net","externalIp":"192.168.40.105","gatewayIp":"10.240.0.1","internalNetmask":"255.255.255.0","allowedTcpPorts":[22,80],"allowedUdpPorts":[],"networkType":"vxlan","vlanId":0,"enableOutboundNat":true,"vni":4200,"nextIp":2}"#.to_string(),
        })
        .expect("upsert network head");
        assert!(process_materialization_once(&db).expect("materialize create"));
        let net = db
            .get_network_for_node("node-net", "vxlan-test")
            .expect("network lookup")
            .expect("network exists");
        assert_eq!(net.network_type, "vxlan");

        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "network/node-net/vxlan-test".to_string(),
            last_op_id: "op-net-delete".to_string(),
            last_logical_ts_unix_ms: 101,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 13,
            last_event_type: "network.delete".to_string(),
            last_body_json: r#"{"name":"vxlan-test","nodeId":"node-net"}"#.to_string(),
        })
        .expect("upsert network delete head");
        assert!(process_materialization_once(&db).expect("materialize delete"));
        assert!(db
            .get_network_for_node("node-net", "vxlan-test")
            .expect("network lookup")
            .is_none());
    }

    #[test]
    fn materializer_network_create_creates_placeholder_node_when_missing() {
        let db = Database::open(":memory:").expect("open db");
        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "network/node-missing/repl-net".to_string(),
            last_op_id: "op-net-create-missing-node".to_string(),
            last_logical_ts_unix_ms: 100,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 20,
            last_event_type: "network.create".to_string(),
            last_body_json: r#"{"name":"repl-net","nodeId":"node-missing","externalIp":"192.168.40.105","gatewayIp":"10.240.0.1","internalNetmask":"255.255.255.0","allowedTcpPorts":[],"allowedUdpPorts":[],"networkType":"vxlan","vlanId":0,"enableOutboundNat":true,"vni":4201,"nextIp":2}"#.to_string(),
        })
        .expect("upsert network head");

        assert!(process_materialization_once(&db).expect("materialize create"));
        assert!(db.get_node("node-missing").expect("node lookup").is_some());
        assert!(db
            .get_network_for_node("node-missing", "repl-net")
            .expect("network lookup")
            .is_some());
    }

    #[test]
    fn materializer_vm_create_creates_placeholder_node_when_missing() {
        let db = Database::open(":memory:").expect("open db");
        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "vm/v-missing-node".to_string(),
            last_op_id: "op-vm-missing-node".to_string(),
            last_logical_ts_unix_ms: 100,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 30,
            last_event_type: "vm.create".to_string(),
            last_body_json: r#"{"vmId":"v-missing-node","name":"v-missing-node","nodeId":"node-missing-vm","cpu":2,"memoryBytes":2147483648,"imagePath":"","imageUrl":"","imageSha256":"","imageFormat":"qcow2","imageSize":0,"network":"default","autoStart":true,"runtimeState":"unknown","cloudInitUserData":"","storageBackend":"filesystem","storageSizeBytes":0,"vmIp":""}"#.to_string(),
        })
        .expect("upsert vm head");

        assert!(process_materialization_once(&db).expect("materialize create"));
        assert!(db
            .get_node("node-missing-vm")
            .expect("node lookup")
            .is_some());
        assert!(db.get_vm("v-missing-node").expect("vm lookup").is_some());
    }

    #[test]
    fn materializer_applies_security_group_create_attach_detach_delete() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node("node-sg");
        db.upsert_node(&node).expect("insert node");
        db.insert_vm(&test_vm("vm-sg", &node.id))
            .expect("insert vm");
        db.insert_network(&NetworkRow {
            name: "vxlan-sg".to_string(),
            external_ip: "192.168.40.105".to_string(),
            gateway_ip: "10.240.0.1".to_string(),
            internal_netmask: "255.255.255.0".to_string(),
            node_id: node.id.clone(),
            allowed_tcp_ports: "[]".to_string(),
            allowed_udp_ports: "[]".to_string(),
            vlan_id: 0,
            network_type: "vxlan".to_string(),
            enable_outbound_nat: true,
            vni: 4200,
            next_ip: 2,
        })
        .expect("insert network");

        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "security-group/web".to_string(),
            last_op_id: "op-sg-create".to_string(),
            last_logical_ts_unix_ms: 100,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 14,
            last_event_type: "security_group.create".to_string(),
            last_body_json: r#"{"name":"web","description":"web ingress","rules":[{"id":"allow-http","protocol":"tcp","hostPort":80,"targetPort":80,"sourceCidr":"0.0.0.0/0","targetVm":"","enableDnat":false}]}"#.to_string(),
        })
        .expect("upsert sg create");
        assert!(process_materialization_once(&db).expect("sg create"));
        assert!(db.get_security_group("web").expect("db").is_some());
        assert_eq!(db.list_security_group_rules("web").expect("rules").len(), 1);

        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "security-group/web".to_string(),
            last_op_id: "op-sg-attach-vm".to_string(),
            last_logical_ts_unix_ms: 101,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 15,
            last_event_type: "security_group.attach".to_string(),
            last_body_json:
                r#"{"securityGroup":"web","targetKind":1,"targetId":"vm-sg","targetNode":""}"#
                    .to_string(),
        })
        .expect("upsert sg attach vm");
        assert!(process_materialization_once(&db).expect("sg attach vm"));
        assert_eq!(
            db.list_security_groups_for_vm("vm-sg")
                .expect("vm attachments")
                .len(),
            1
        );

        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "security-group/web".to_string(),
            last_op_id: "op-sg-attach-net".to_string(),
            last_logical_ts_unix_ms: 102,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 16,
            last_event_type: "security_group.attach".to_string(),
            last_body_json: r#"{"securityGroup":"web","targetKind":2,"targetId":"vxlan-sg","targetNode":"node-sg"}"#.to_string(),
        })
        .expect("upsert sg attach network");
        assert!(process_materialization_once(&db).expect("sg attach network"));
        assert_eq!(
            db.list_security_groups_for_network("vxlan-sg", "node-sg")
                .expect("network attachments")
                .len(),
            1
        );

        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "security-group/web".to_string(),
            last_op_id: "op-sg-detach".to_string(),
            last_logical_ts_unix_ms: 103,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 17,
            last_event_type: "security_group.detach".to_string(),
            last_body_json:
                r#"{"securityGroup":"web","targetKind":1,"targetId":"vm-sg","targetNode":""}"#
                    .to_string(),
        })
        .expect("upsert sg detach");
        assert!(process_materialization_once(&db).expect("sg detach"));
        assert!(db
            .list_security_groups_for_vm("vm-sg")
            .expect("vm attachments")
            .is_empty());

        db.upsert_replication_resource_head(&ReplicationResourceHeadRow {
            resource_key: "security-group/web".to_string(),
            last_op_id: "op-sg-delete".to_string(),
            last_logical_ts_unix_ms: 104,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 18,
            last_event_type: "security_group.delete".to_string(),
            last_body_json: r#"{"name":"web"}"#.to_string(),
        })
        .expect("upsert sg delete");
        assert!(process_materialization_once(&db).expect("sg delete"));
        assert!(db.get_security_group("web").expect("db").is_none());
    }

    #[test]
    fn vm_create_reservation_failure_keeps_head_for_eventual_materialization() {
        let db = Database::open(":memory:").expect("open db");
        let ev = controller_proto::ReplicationEvent {
            event_id: 10,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            event_type: "vm.create".to_string(),
            resource_key: "vm/v9".to_string(),
            payload: br#"{"opId":"op-9","controllerId":"ctrl-a","logicalTsUnixMs":1000,"eventType":"vm.create","resourceKey":"vm/v9","body":{"vmId":"v9","nodeId":"missing-node","name":"v9"}}"#.to_vec(),
        };
        apply_replication_event(&db, &ev).expect("apply");
        assert!(db
            .get_replication_resource_head("vm/v9")
            .expect("get head")
            .is_some());
        let reservation = db
            .get_replication_reservation("node-capacity/missing-node", "vm/v9")
            .expect("reservation read")
            .expect("reservation row");
        assert_eq!(reservation.status, "failed_non_retryable");
    }

    #[test]
    fn apply_replication_event_ignores_out_of_order_event_ids() {
        let db = Database::open(":memory:").expect("open db");
        let newer = controller_proto::ReplicationEvent {
            event_id: 20,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            event_type: "vm.update".to_string(),
            resource_key: "vm/v-order".to_string(),
            payload: br#"{"opId":"op-new","controllerId":"ctrl-a","logicalTsUnixMs":2000,"eventType":"vm.update","resourceKey":"vm/v-order","body":{"vmId":"v-order","cpu":4}}"#.to_vec(),
        };
        let older = controller_proto::ReplicationEvent {
            event_id: 5,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            event_type: "vm.update".to_string(),
            resource_key: "vm/v-order".to_string(),
            payload: br#"{"opId":"op-old","controllerId":"ctrl-a","logicalTsUnixMs":1000,"eventType":"vm.update","resourceKey":"vm/v-order","body":{"vmId":"v-order","cpu":2}}"#.to_vec(),
        };

        apply_replication_event(&db, &newer).expect("apply newer first");
        apply_replication_event(&db, &older).expect("apply older second");
        let head = db
            .get_replication_resource_head("vm/v-order")
            .expect("read head")
            .expect("head row");
        assert_eq!(head.last_op_id, "op-new");
        assert_eq!(head.last_event_id, 20);
    }

    #[test]
    fn vm_create_reservation_retryable_failure_tracks_budget() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node("node-r");
        node.status = "not-ready".to_string();
        db.upsert_node(&node).expect("insert node");
        let body = serde_json::json!({
            "vmId": "v-retry",
            "nodeId": "node-r",
            "name": "v-retry"
        });

        for _ in 0..3 {
            let out = evaluate_reservation(&db, "vm.create", "vm/v-retry", "op-r", &body)
                .expect("evaluate");
            assert!(!out.accepted);
        }

        let reservation = db
            .get_replication_reservation("node-capacity/node-r", "vm/v-retry")
            .expect("reservation read")
            .expect("reservation row");
        assert_eq!(reservation.status, "retry_exhausted");
        assert_eq!(reservation.retry_count, 3);
    }

    #[test]
    fn reservation_retry_executor_promotes_to_reserved_when_node_recovers() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node("node-r1");
        node.status = "not-ready".to_string();
        db.upsert_node(&node).expect("insert node");
        db.upsert_replication_reservation_with_retry(
            "node-capacity/node-r1",
            "vm/v-r1",
            "op-r1",
            "failed_retryable",
            "node not ready",
            1,
        )
        .expect("insert reservation");

        node.status = "ready".to_string();
        db.upsert_node(&node).expect("update node");
        assert!(process_reservation_retry_once_with_min_age(&db, 0).expect("retry once"));
        let row = db
            .get_replication_reservation("node-capacity/node-r1", "vm/v-r1")
            .expect("get reservation")
            .expect("row");
        assert_eq!(row.status, "reserved");
        assert_eq!(row.retry_count, 1);
    }

    #[test]
    fn reservation_retry_executor_exhausts_budget_for_persistent_not_ready() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node("node-r2");
        node.status = "not-ready".to_string();
        db.upsert_node(&node).expect("insert node");
        db.upsert_replication_reservation_with_retry(
            "node-capacity/node-r2",
            "vm/v-r2",
            "op-r2",
            "failed_retryable",
            "node not ready",
            1,
        )
        .expect("insert reservation");

        assert!(process_reservation_retry_once_with_min_age(&db, 0).expect("retry once"));
        assert!(process_reservation_retry_once_with_min_age(&db, 0).expect("retry once"));
        let row = db
            .get_replication_reservation("node-capacity/node-r2", "vm/v-r2")
            .expect("get reservation")
            .expect("row");
        assert_eq!(row.status, "retry_exhausted");
        assert_eq!(row.retry_count, 3);
    }

    #[test]
    fn apply_head_to_domain_upserts_ssh_key() {
        let db = Database::open(":memory:").expect("open db");
        let head = ReplicationResourceHeadRow {
            resource_key: "ssh-key/operator-key".to_string(),
            last_op_id: "op-ssh-1".to_string(),
            last_logical_ts_unix_ms: 1,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 1,
            last_event_type: "ssh_key.create".to_string(),
            last_body_json:
                r#"{"name":"operator-key","publicKey":"ssh-ed25519 AAAA test@example"}"#.to_string(),
        };
        apply_head_to_domain(&db, &head).expect("apply");
        let row = db
            .get_ssh_key("operator-key")
            .expect("db")
            .expect("key row");
        assert_eq!(row.0, "operator-key");
        assert_eq!(row.1, "ssh-ed25519 AAAA test@example");
    }

    #[test]
    fn apply_head_to_domain_deletes_ssh_key_and_vm_associations() {
        let db = Database::open(":memory:").expect("open db");
        db.upsert_node(&test_node("node-1"))
            .expect("insert backing node");
        db.insert_ssh_key("operator-key", "ssh-ed25519 AAAA test@example")
            .expect("insert key");

        let vm = VmRow {
            id: "vm-ssh".to_string(),
            name: "vm-ssh".to_string(),
            cpu: 2,
            memory_bytes: 1024 * 1024 * 1024,
            image_path: "/var/lib/kcore/images/test.qcow2".to_string(),
            image_url: "".to_string(),
            image_sha256: "".to_string(),
            image_format: "qcow2".to_string(),
            image_size: 8192,
            network: "default".to_string(),
            auto_start: true,
            node_id: "node-1".to_string(),
            created_at: String::new(),
            runtime_state: "unknown".to_string(),
            cloud_init_user_data: String::new(),
            storage_backend: "filesystem".to_string(),
            storage_size_bytes: 10 * 1024 * 1024 * 1024,
            vm_ip: String::new(),
        };
        db.insert_vm(&vm).expect("insert vm");
        db.associate_vm_ssh_keys("vm-ssh", &[String::from("operator-key")])
            .expect("associate key");

        let head = ReplicationResourceHeadRow {
            resource_key: "ssh-key/operator-key".to_string(),
            last_op_id: "op-ssh-del-1".to_string(),
            last_logical_ts_unix_ms: 2,
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
            last_controller_id: "ctrl-a".to_string(),
            last_event_id: 2,
            last_event_type: "ssh_key.delete".to_string(),
            last_body_json: r#"{"name":"operator-key"}"#.to_string(),
        };
        apply_head_to_domain(&db, &head).expect("apply");
        assert!(db.get_ssh_key("operator-key").expect("db").is_none());
    }

    #[test]
    fn export_replication_trace_fixture() {
        let Ok(output_path) = env::var("KCORE_REPLICATION_TRACE_OUT") else {
            return;
        };

        let db = Database::open(":memory:").expect("open db");
        let mut retry_node = test_node("node-retry-trace");
        retry_node.status = "not-ready".to_string();
        db.upsert_node(&retry_node)
            .expect("insert retry trace node");
        let events = vec![
            (
                controller_proto::ReplicationEvent {
                    event_id: 1,
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                    event_type: "vm.update".to_string(),
                    resource_key: "vm/v-trace".to_string(),
                    payload: br#"{"opId":"op-trace-a","controllerId":"ctrl-a","logicalTsUnixMs":1000,"eventType":"vm.update","resourceKey":"vm/v-trace","safetyClass":"safe","body":{"cpu":1}}"#.to_vec(),
                },
                100,
            ),
            (
                controller_proto::ReplicationEvent {
                    event_id: 2,
                    created_at: "2026-01-01T00:00:01Z".to_string(),
                    event_type: "vm.update".to_string(),
                    resource_key: "vm/v-trace".to_string(),
                    payload: br#"{"opId":"op-trace-b","controllerId":"ctrl-b","logicalTsUnixMs":900,"eventType":"vm.update","resourceKey":"vm/v-trace","safetyClass":"unsafe","body":{"cpu":2}}"#.to_vec(),
                },
                50,
            ),
            (
                controller_proto::ReplicationEvent {
                    event_id: 3,
                    created_at: "2026-01-01T00:00:01.500Z".to_string(),
                    event_type: "vm.update".to_string(),
                    resource_key: "vm/v-trace-2".to_string(),
                    payload: br#"{"opId":"op-trace-c","controllerId":"ctrl-c","logicalTsUnixMs":2000,"eventType":"vm.update","resourceKey":"vm/v-trace-2","safetyClass":"safe","body":{"cpu":4}}"#.to_vec(),
                },
                200,
            ),
            (
                controller_proto::ReplicationEvent {
                    event_id: 4,
                    created_at: "2026-01-01T00:00:02Z".to_string(),
                    event_type: "vm.update".to_string(),
                    resource_key: "vm/v-trace-2".to_string(),
                    payload: br#"{"opId":"op-trace-d","controllerId":"ctrl-d","logicalTsUnixMs":1900,"eventType":"vm.update","resourceKey":"vm/v-trace-2","safetyClass":"unsafe","body":{"cpu":2}}"#.to_vec(),
                },
                150,
            ),
            (
                controller_proto::ReplicationEvent {
                    event_id: 5,
                    created_at: "2026-01-01T00:00:02.500Z".to_string(),
                    event_type: "vm.create".to_string(),
                    resource_key: "vm/v-trace-reservation".to_string(),
                    payload: br#"{"opId":"op-trace-r","controllerId":"ctrl-r","logicalTsUnixMs":1500,"eventType":"vm.create","resourceKey":"vm/v-trace-reservation","body":{"vmId":"v-trace-reservation","nodeId":"missing-node","name":"vtrace"}}"#.to_vec(),
                },
                80,
            ),
            (
                controller_proto::ReplicationEvent {
                    event_id: 6,
                    created_at: "2026-01-01T00:00:03Z".to_string(),
                    event_type: "vm.create".to_string(),
                    resource_key: "vm/v-trace-retry".to_string(),
                    payload: br#"{"opId":"op-trace-r1","controllerId":"ctrl-r","logicalTsUnixMs":1600,"eventType":"vm.create","resourceKey":"vm/v-trace-retry","body":{"vmId":"v-trace-retry","nodeId":"node-retry-trace","name":"vtrace-retry"}}"#.to_vec(),
                },
                70,
            ),
            (
                controller_proto::ReplicationEvent {
                    event_id: 7,
                    created_at: "2026-01-01T00:00:03.500Z".to_string(),
                    event_type: "vm.create".to_string(),
                    resource_key: "vm/v-trace-retry".to_string(),
                    payload: br#"{"opId":"op-trace-r2","controllerId":"ctrl-r","logicalTsUnixMs":1700,"eventType":"vm.create","resourceKey":"vm/v-trace-retry","body":{"vmId":"v-trace-retry","nodeId":"node-retry-trace","name":"vtrace-retry"}}"#.to_vec(),
                },
                80,
            ),
            (
                controller_proto::ReplicationEvent {
                    event_id: 8,
                    created_at: "2026-01-01T00:00:04Z".to_string(),
                    event_type: "vm.create".to_string(),
                    resource_key: "vm/v-trace-retry".to_string(),
                    payload: br#"{"opId":"op-trace-r3","controllerId":"ctrl-r","logicalTsUnixMs":1800,"eventType":"vm.create","resourceKey":"vm/v-trace-retry","body":{"vmId":"v-trace-retry","nodeId":"node-retry-trace","name":"vtrace-retry"}}"#.to_vec(),
                },
                90,
            ),
        ];

        let mut trace_rows = Vec::new();
        for (event, rank) in events {
            let payload: Value = serde_json::from_slice(&event.payload).expect("payload json");
            let op_id = payload
                .get("opId")
                .and_then(Value::as_str)
                .expect("opId")
                .to_string();
            let resource_key = payload
                .get("resourceKey")
                .and_then(Value::as_str)
                .expect("resourceKey")
                .to_string();
            apply_replication_event(&db, &event).expect("apply event");
            if event.event_id == 2 {
                assert!(
                    process_compensation_once(&db).expect("process compensation"),
                    "expected one compensation job to process"
                );
            }
            let head_op = db
                .get_replication_resource_head(&resource_key)
                .expect("read head")
                .map(|h| h.last_op_id);
            let reservation_status = payload
                .get("body")
                .and_then(Value::as_object)
                .and_then(|b| b.get("nodeId"))
                .and_then(Value::as_str)
                .and_then(|node_id| {
                    db.get_replication_reservation(
                        &format!("node-capacity/{node_id}"),
                        &resource_key,
                    )
                    .ok()
                    .flatten()
                })
                .map(|r| r.status)
                .unwrap_or_else(|| "not_applicable".to_string());
            let reservation_failed = reservation_status.starts_with("failed_")
                || reservation_status == "retry_exhausted";
            let compensation_status = db
                .get_compensation_job_status_for_loser_op(&op_id)
                .ok()
                .flatten()
                .map(|s| {
                    if s == "completed" {
                        "completed"
                    } else {
                        "queued"
                    }
                    .to_string()
                })
                .unwrap_or_else(|| "not_applicable".to_string());
            let terminal_state = if head_op.as_deref() == Some(op_id.as_str()) {
                "auto_accepted"
            } else if reservation_failed {
                "auto_rejected"
            } else if payload
                .get("safetyClass")
                .and_then(Value::as_str)
                .unwrap_or("safe")
                == "unsafe"
            {
                "auto_compensated"
            } else {
                "auto_rejected"
            };
            let expected_winner = if let Some(w) = head_op {
                w
            } else {
                op_id.clone()
            };
            trace_rows.push(serde_json::json!({
                "resource_key": resource_key,
                "op_id": op_id,
                "rank": rank,
                "logical_ts_unix_ms": payload
                    .get("logicalTsUnixMs")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                "controller_id": payload
                    .get("controllerId")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                "terminal_state": terminal_state,
                "reservation_status": reservation_status,
                "compensation_status": compensation_status,
                "expected_winner_op_id": expected_winner,
            }));
        }

        let as_json = serde_json::to_vec_pretty(&trace_rows).expect("serialize trace");
        let output = std::path::Path::new(&output_path);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).expect("create trace parent");
        }
        fs::write(output, as_json).expect("write trace");
    }

    #[test]
    fn materializer_applies_controller_register() {
        let db = Database::open(":memory:").expect("open db");
        let head = ReplicationResourceHeadRow {
            resource_key: "controller/kcore-controller-192-168-40-151".to_string(),
            last_op_id: "op-ctrl-reg".to_string(),
            last_logical_ts_unix_ms: 1000,
            last_controller_id: "kcore-controller-192-168-40-151".to_string(),
            last_event_id: 1,
            last_event_type: "controller.register".to_string(),
            last_body_json: serde_json::json!({
                "controllerId": "kcore-controller-192-168-40-151",
                "address": "192.168.40.151:9090",
                "dcId": "DC1"
            })
            .to_string(),
            last_policy_priority: 0,
            last_intent_epoch: 0,
            last_validity: "valid".to_string(),
            last_safety_class: "safe".to_string(),
        };
        apply_head_to_domain(&db, &head).expect("materialize controller.register");

        let peers = db.list_controller_peers().expect("list peers");
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].controller_id, "kcore-controller-192-168-40-151");
        assert_eq!(peers[0].address, "192.168.40.151:9090");
        assert_eq!(peers[0].dc_id, "DC1");
    }

    #[test]
    fn emit_controller_register_writes_outbox_and_local_peer() {
        let db = Database::open(":memory:").expect("open db");
        let cfg = crate::config::Config {
            listen_addr: "0.0.0.0:9090".to_string(),
            db_path: ":memory:".to_string(),
            tls: None,
            default_network: crate::config::NetworkConfig {
                gateway_interface: "eth0".to_string(),
                external_ip: "192.168.40.105".to_string(),
                gateway_ip: "10.0.0.1".to_string(),
                internal_netmask: "255.255.255.0".to_string(),
            },
            replication: Some(ReplicationConfig {
                controller_id: "kcore-controller-192-168-40-105".to_string(),
                dc_id: "DC1".to_string(),
                peers: vec![],
            }),
            require_manual_approval: false,
        };
        emit_controller_register(&db, &cfg);

        let peers = db.list_controller_peers().expect("list peers");
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].controller_id, "kcore-controller-192-168-40-105");
        assert_eq!(peers[0].address, "192.168.40.105:9090");

        let outbox = db
            .list_replication_outbox_since(0, 10)
            .expect("list outbox");
        assert!(
            outbox.iter().any(|r| r.event_type == "controller.register"),
            "controller.register event should be in outbox"
        );
    }

    #[test]
    fn emit_controller_register_skips_without_replication_config() {
        let db = Database::open(":memory:").expect("open db");
        let cfg = crate::config::Config {
            listen_addr: "0.0.0.0:9090".to_string(),
            db_path: ":memory:".to_string(),
            tls: None,
            default_network: crate::config::NetworkConfig {
                gateway_interface: "eth0".to_string(),
                external_ip: "192.168.40.105".to_string(),
                gateway_ip: "10.0.0.1".to_string(),
                internal_netmask: "255.255.255.0".to_string(),
            },
            replication: None,
            require_manual_approval: false,
        };
        emit_controller_register(&db, &cfg);

        let peers = db.list_controller_peers().expect("list peers");
        assert!(peers.is_empty());
    }
}
