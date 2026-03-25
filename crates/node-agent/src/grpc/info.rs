use tonic::{Request, Response, Status};

use crate::auth::{self, CN_CONTROLLER, CN_KCTL};
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
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;

        let (hostname, cpu_cores, memory_bytes) = tokio::task::spawn_blocking(|| {
            let hostname = hostname::get()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "unknown".into());
            let (cpu, mem) = read_capacity();
            (hostname, cpu, mem)
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
                cpu_cores_used: 0,
                memory_bytes_used: 0,
            }),
            storage_backends: Vec::new(),
        }))
    }
}

fn read_capacity() -> (i32, i64) {
    let cpu_cores = std::fs::read_to_string("/proc/cpuinfo")
        .map(|s| s.matches("processor\t:").count() as i32)
        .unwrap_or(0);

    let memory_bytes = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| {
                    l.split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse::<i64>().ok())
                })
                .map(|kb| kb * 1024)
        })
        .unwrap_or(0);

    (cpu_cores, memory_bytes)
}
