use tokio::process::Command;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::auth::{self, CN_CONTROLLER_PREFIX, CN_KCTL};
use crate::config::ReplicationConfig;
use crate::controller_proto;
use crate::db::Database;

pub struct ControllerAdminService {
    db: Database,
    replication_peers: Vec<String>,
}

impl ControllerAdminService {
    pub fn new(db: Database, replication: Option<ReplicationConfig>) -> Self {
        let replication_peers = replication.map(|r| r.peers).unwrap_or_default();
        Self {
            db,
            replication_peers,
        }
    }
}

#[tonic::async_trait]
impl controller_proto::controller_admin_server::ControllerAdmin for ControllerAdminService {
    async fn apply_nix_config(
        &self,
        request: Request<controller_proto::ApplyNixConfigRequest>,
    ) -> Result<Response<controller_proto::ApplyNixConfigResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();
        let path = "/etc/nixos/configuration.nix";

        let config_nix = req.configuration_nix.clone();
        tokio::task::spawn_blocking(move || std::fs::write(path, &config_nix))
            .await
            .map_err(|e| Status::internal(format!("task join: {e}")))?
            .map_err(|e| {
                error!(error = %e, "failed to write controller nix config");
                Status::internal(format!("writing {path}: {e}"))
            })?;

        info!("wrote controller nix config");

        if !req.rebuild {
            return Ok(Response::new(controller_proto::ApplyNixConfigResponse {
                success: true,
                message: format!("config written to {path}"),
            }));
        }

        tokio::spawn(async move {
            info!("starting nixos-rebuild switch");
            match Command::new("nixos-rebuild").arg("switch").output().await {
                Ok(out) if out.status.success() => {
                    info!("nixos-rebuild switch completed");
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    error!(stderr = %stderr, "nixos-rebuild switch failed");
                }
                Err(e) => {
                    error!(error = %e, "failed to run nixos-rebuild");
                }
            }
        });

        Ok(Response::new(controller_proto::ApplyNixConfigResponse {
            success: true,
            message: format!("config written to {path}; nixos-rebuild switch started"),
        }))
    }

    async fn get_replication_events(
        &self,
        request: Request<controller_proto::GetReplicationEventsRequest>,
    ) -> Result<Response<controller_proto::GetReplicationEventsResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let limit = if req.limit <= 0 {
            500
        } else {
            i64::from(req.limit.min(5_000))
        };
        let after = req.after_event_id.max(0);
        let rows = self
            .db
            .list_replication_outbox_since(after, limit)
            .map_err(|e| Status::internal(format!("listing replication events: {e}")))?;
        let events = rows
            .into_iter()
            .map(|row| controller_proto::ReplicationEvent {
                event_id: row.id,
                created_at: row.created_at,
                event_type: row.event_type,
                resource_key: row.resource_key,
                payload: row.payload,
            })
            .collect();
        Ok(Response::new(
            controller_proto::GetReplicationEventsResponse { events },
        ))
    }

    async fn ack_replication_events(
        &self,
        request: Request<controller_proto::AckReplicationEventsRequest>,
    ) -> Result<Response<controller_proto::AckReplicationEventsResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        if req.peer_id.trim().is_empty() {
            return Err(Status::invalid_argument("peer_id is required"));
        }
        if req.last_event_id < 0 {
            return Err(Status::invalid_argument("last_event_id must be >= 0"));
        }
        self.db
            .upsert_replication_ack(req.peer_id.trim(), req.last_event_id)
            .map_err(|e| Status::internal(format!("upserting replication ack: {e}")))?;
        Ok(Response::new(
            controller_proto::AckReplicationEventsResponse { success: true },
        ))
    }

    async fn get_replication_status(
        &self,
        request: Request<controller_proto::GetReplicationStatusRequest>,
    ) -> Result<Response<controller_proto::GetReplicationStatusResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let outbox_head_event_id = self
            .db
            .replication_outbox_head_id()
            .map_err(|e| Status::internal(format!("reading outbox head: {e}")))?;
        let outbox_size = self
            .db
            .replication_outbox_len()
            .map_err(|e| Status::internal(format!("reading outbox size: {e}")))?;
        let ack_rows = self
            .db
            .list_replication_acks()
            .map_err(|e| Status::internal(format!("listing replication acks: {e}")))?;
        let unresolved_conflicts = self
            .db
            .count_unresolved_replication_conflicts()
            .map_err(|e| Status::internal(format!("counting replication conflicts: {e}")))?;

        let outgoing = ack_rows
            .iter()
            .filter(|row| !row.peer_id.starts_with("pull/") && !row.peer_id.starts_with("apply/"))
            .map(|row| controller_proto::ReplicationOutgoingStatus {
                peer_id: row.peer_id.clone(),
                last_acked_event_id: row.last_event_id,
                lag_events: (outbox_head_event_id - row.last_event_id).max(0),
            })
            .collect();

        let mut incoming = Vec::new();
        for endpoint in &self.replication_peers {
            let pull_key = format!("pull/{endpoint}");
            let apply_key = format!("apply/{endpoint}");
            let last_pulled_event_id = ack_rows
                .iter()
                .find(|row| row.peer_id == pull_key)
                .map(|row| row.last_event_id)
                .unwrap_or(0);
            let last_applied_event_id = ack_rows
                .iter()
                .find(|row| row.peer_id == apply_key)
                .map(|row| row.last_event_id)
                .unwrap_or(0);
            incoming.push(controller_proto::ReplicationIncomingStatus {
                peer_endpoint: endpoint.clone(),
                last_pulled_event_id,
                last_applied_event_id,
            });
        }

        Ok(Response::new(controller_proto::GetReplicationStatusResponse {
            outbox_head_event_id,
            outbox_size,
            outgoing,
            incoming,
            unresolved_conflicts,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_replication_events_returns_outbox_rows() {
        let db = Database::open(":memory:").expect("open db");
        db.append_replication_outbox("node.register", "node/n1", br#"{"a":1}"#)
            .expect("append row");
        let svc = ControllerAdminService::new(db, None);
        let resp = <ControllerAdminService as controller_proto::controller_admin_server::ControllerAdmin>::get_replication_events(
            &svc,
            Request::new(controller_proto::GetReplicationEventsRequest {
                after_event_id: 0,
                limit: 10,
            }),
        )
        .await
        .expect("get replication events")
        .into_inner();
        assert_eq!(resp.events.len(), 1);
        assert_eq!(resp.events[0].event_type, "node.register");
        assert_eq!(resp.events[0].resource_key, "node/n1");
    }

    #[tokio::test]
    async fn ack_replication_events_validates_peer_id() {
        let db = Database::open(":memory:").expect("open db");
        let svc = ControllerAdminService::new(db, None);
        let err = <ControllerAdminService as controller_proto::controller_admin_server::ControllerAdmin>::ack_replication_events(
            &svc,
            Request::new(controller_proto::AckReplicationEventsRequest {
                peer_id: String::new(),
                last_event_id: 1,
            }),
        )
        .await
        .expect_err("missing peer id should fail");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn ack_replication_events_persists_frontier() {
        let db = Database::open(":memory:").expect("open db");
        let svc = ControllerAdminService::new(db.clone(), None);
        <ControllerAdminService as controller_proto::controller_admin_server::ControllerAdmin>::ack_replication_events(
            &svc,
            Request::new(controller_proto::AckReplicationEventsRequest {
                peer_id: "peer-a".to_string(),
                last_event_id: 9,
            }),
        )
        .await
        .expect("ack should succeed");
        assert_eq!(
            db.get_replication_ack("peer-a").expect("get ack"),
            Some(9)
        );
    }

    #[tokio::test]
    async fn get_replication_status_reports_outgoing_and_incoming() {
        let db = Database::open(":memory:").expect("open db");
        db.append_replication_outbox("vm.create", "vm/v1", br#"{}"#)
            .expect("append");
        db.append_replication_outbox("vm.update", "vm/v1", br#"{}"#)
            .expect("append");
        db.upsert_replication_ack("peer-ctrl-b", 1).expect("ack outgoing");
        db.upsert_replication_ack("pull/10.0.0.11:9090", 5)
            .expect("ack pull");
        db.upsert_replication_ack("apply/10.0.0.11:9090", 4)
            .expect("ack apply");

        let svc = ControllerAdminService::new(
            db,
            Some(ReplicationConfig {
                controller_id: "ctrl-a".into(),
                dc_id: "DC1".into(),
                peers: vec!["10.0.0.11:9090".into()],
            }),
        );

        let resp = <ControllerAdminService as controller_proto::controller_admin_server::ControllerAdmin>::get_replication_status(
            &svc,
            Request::new(controller_proto::GetReplicationStatusRequest {}),
        )
        .await
        .expect("status")
        .into_inner();

        assert_eq!(resp.outbox_head_event_id, 2);
        assert_eq!(resp.outbox_size, 2);
        assert_eq!(resp.outgoing.len(), 1);
        assert_eq!(resp.outgoing[0].peer_id, "peer-ctrl-b");
        assert_eq!(resp.outgoing[0].lag_events, 1);
        assert_eq!(resp.incoming.len(), 1);
        assert_eq!(resp.incoming[0].peer_endpoint, "10.0.0.11:9090");
        assert_eq!(resp.incoming[0].last_pulled_event_id, 5);
        assert_eq!(resp.incoming[0].last_applied_event_id, 4);
        assert_eq!(resp.unresolved_conflicts, 0);
    }
}
