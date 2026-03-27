use std::time::Duration;

use serde_json::Value;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tracing::{info, warn};

use crate::config::{ReplicationConfig, TlsConfig};
use crate::controller_proto;
use crate::db::{Database, ReplicationResourceHeadRow};
use crate::replication_policy::{
    compare_rank, loser_terminal_state, parse_safety_class, parse_validity_class, ArbitrationRank,
};

const DEFAULT_PAGE_SIZE: i32 = 500;
const MAX_PAGE_SIZE: i32 = 5_000;
const ERROR_BACKOFF_SECS: u64 = 5;
const IDLE_POLL_SECS: u64 = 2;

pub fn spawn_replication_pollers(
    db: Database,
    replication: Option<ReplicationConfig>,
    tls: Option<TlsConfig>,
    listen_addr: &str,
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
        if same_endpoint(listen_addr, &peer) {
            info!(peer = %peer, "skipping replication peer that resolves to local controller");
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

    let mut last_applied_event_id = after_event_id;
    for event in &events {
        apply_replication_event(db, event)?;
        last_applied_event_id = event.event_id;
    }
    db.upsert_replication_ack(&format!("apply/{peer}"), last_applied_event_id)
        .map_err(|e| format!("store local apply frontier: {e}"))?;
    db.upsert_replication_ack(local_frontier_key, last_applied_event_id)
        .map_err(|e| format!("store local frontier: {e}"))?;

    client
        .ack_replication_events(controller_proto::AckReplicationEventsRequest {
            peer_id: local_controller_id.to_string(),
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

fn apply_replication_event(db: &Database, event: &controller_proto::ReplicationEvent) -> Result<(), String> {
    let payload: Value = serde_json::from_slice(&event.payload)
        .map_err(|e| format!("invalid replication payload for event {}: {e}", event.event_id))?;
    let payload_obj = payload
        .as_object()
        .ok_or_else(|| {
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
    let safety_class =
        parse_safety_class(payload_obj.get("safetyClass").and_then(|v| v.as_str()));
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
    let replace_head = should_replace_head(
        existing_head.as_ref(),
        policy_priority,
        intent_epoch,
        validity,
        safety_class,
        logical_ts_unix_ms,
        origin_controller_id,
        op_id,
    );

    if let Some(existing) = existing_head.as_ref() {
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
        let _ = db.insert_replication_conflict_with_resolved(
            payload_resource_key,
            &existing.last_op_id,
            op_id,
            &existing.last_controller_id,
            origin_controller_id,
            &reason,
            true,
        );
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
        | "node.approve"
        | "node.reject"
        | "node.drain"
        | "vm.create"
        | "vm.update"
        | "vm.delete"
        | "vm.desired_state.set"
        | "network.create"
        | "network.delete" => {
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

    #[test]
    fn normalize_endpoint_adds_scheme() {
        assert_eq!(normalize_endpoint("10.0.0.10:9090", true), "https://10.0.0.10:9090");
        assert_eq!(normalize_endpoint("10.0.0.10:9090", false), "http://10.0.0.10:9090");
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
}
