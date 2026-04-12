use tonic::{Request, Response, Status};

use crate::auth::{self, CN_CONTROLLER_PREFIX, CN_KCTL};
use crate::proto;

pub struct InfoService {
    node_id: String,
}

impl InfoService {
    pub fn new(node_id: String) -> Self {
        Self { node_id }
    }
}

#[tonic::async_trait]
impl proto::node_info_server::NodeInfo for InfoService {
    async fn get_node_info(
        &self,
        request: Request<proto::GetNodeInfoRequest>,
    ) -> Result<Response<proto::GetNodeInfoResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX, CN_KCTL])?;

        let (hostname, cpu_cores, memory_bytes, cpu_used, memory_used) =
            tokio::task::spawn_blocking(|| {
                let hostname = hostname::get()
                    .map(|h| h.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "unknown".into());
                let (cpu, mem, cpu_used, mem_used) = read_capacity_and_usage();
                (hostname, cpu, mem, cpu_used, mem_used)
            })
            .await
            .map_err(|e| Status::internal(format!("task join: {e}")))?;

        Ok(Response::new(proto::GetNodeInfoResponse {
            node_id: self.node_id.clone(),
            hostname,
            capacity: Some(proto::NodeCapacity {
                cpu_cores,
                memory_bytes,
            }),
            usage: Some(proto::NodeUsage {
                cpu_cores_used: cpu_used,
                memory_bytes_used: memory_used,
            }),
            storage_backends: Vec::new(),
        }))
    }
}

fn read_capacity_and_usage() -> (i32, i64, i32, i64) {
    let cpu_cores = std::fs::read_to_string("/proc/cpuinfo")
        .map(|s| s.matches("processor\t:").count() as i32)
        .unwrap_or(0);

    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let parse_kb = |prefix: &str| -> i64 {
        meminfo
            .lines()
            .find(|l| l.starts_with(prefix))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<i64>().ok())
            .map(|kb| kb * 1024)
            .unwrap_or(0)
    };
    let memory_total = parse_kb("MemTotal:");
    let memory_available = parse_kb("MemAvailable:");
    let memory_used = (memory_total - memory_available).max(0);

    let cpu_used = std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .map(|load| load.round() as i32)
        .unwrap_or(0);

    (cpu_cores, memory_total, cpu_used, memory_used)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insecure_mode_denies_node_info_endpoint() {
        let svc = InfoService::new("node-1".to_string());
        let res = <InfoService as proto::node_info_server::NodeInfo>::get_node_info(
            &svc,
            Request::new(proto::GetNodeInfoRequest {}),
        )
        .await;
        let err = res.expect_err("expected permission denied without TLS");
        assert_eq!(err.code(), tonic::Code::PermissionDenied);
    }
}
