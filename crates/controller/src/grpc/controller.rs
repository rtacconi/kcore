use tonic::{Request, Response, Status};
use tracing::{error, info, warn};

use crate::config::NetworkConfig;
use crate::controller_proto;
use crate::db::{Database, NodeRow, VmRow};
use crate::node_proto;
use crate::{nixgen, node_client::NodeClients, scheduler};

pub struct ControllerService {
    db: Database,
    clients: NodeClients,
    default_network: NetworkConfig,
    #[cfg(test)]
    test_push_hook:
        Option<std::sync::Arc<dyn Fn(&NodeRow) -> Result<(), Status> + Send + Sync + 'static>>,
}

impl ControllerService {
    pub fn new(db: Database, clients: NodeClients, default_network: NetworkConfig) -> Self {
        Self {
            db,
            clients,
            default_network,
            #[cfg(test)]
            test_push_hook: None,
        }
    }

    #[cfg(test)]
    pub fn new_with_test_push_hook(
        db: Database,
        clients: NodeClients,
        default_network: NetworkConfig,
        hook: std::sync::Arc<dyn Fn(&NodeRow) -> Result<(), Status> + Send + Sync + 'static>,
    ) -> Self {
        Self {
            db,
            clients,
            default_network,
            test_push_hook: Some(hook),
        }
    }

    async fn push_config_to_node(&self, node: &NodeRow) -> Result<(), Status> {
        #[cfg(test)]
        if let Some(hook) = &self.test_push_hook {
            return hook(node);
        }

        let vms = self
            .db
            .list_vms_for_node(&node.id)
            .map_err(|e| Status::internal(format!("listing vms: {e}")))?;

        let iface = if node.gateway_interface.is_empty() {
            &self.default_network.gateway_interface
        } else {
            &node.gateway_interface
        };

        let nix_config = nixgen::generate_node_config(&vms, iface, &self.default_network);

        let mut admin = self.clients.get_admin(&node.address).ok_or_else(|| {
            Status::unavailable(format!("no connection to node {}", node.address))
        })?;

        admin
            .apply_nix_config(node_proto::ApplyNixConfigRequest {
                configuration_nix: nix_config,
                rebuild: true,
            })
            .await
            .map_err(|e| {
                error!(node = %node.id, error = %e, "failed to push config to node");
                Status::internal(format!("pushing config to node {}: {e}", node.id))
            })?;

        info!(node = %node.id, "pushed config and triggered rebuild");
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    fn resolve_node_for_vm(&self, vm_id: &str, target_node: &str) -> Result<NodeRow, Status> {
        if !target_node.is_empty() {
            let node = self
                .db
                .get_node_by_address(target_node)
                .map_err(|e| Status::internal(e.to_string()))?
                .or_else(|| self.db.get_node(target_node).ok().flatten())
                .ok_or_else(|| Status::not_found(format!("node {target_node} not found")))?;
            return Ok(node);
        }

        let node_id = self
            .db
            .find_node_for_vm(vm_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("VM {vm_id} not found")))?;

        self.db
            .get_node(&node_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("node {node_id} not found")))
    }

    async fn set_vm_desired_state_internal(
        &self,
        vm_id: &str,
        target_node: &str,
        auto_start: bool,
    ) -> Result<i32, Status> {
        let node = self.resolve_node_for_vm(vm_id, target_node)?;
        let updated = self
            .db
            .set_vm_auto_start(vm_id, auto_start)
            .map_err(|e| Status::internal(format!("updating vm desired state: {e}")))?;
        if !updated {
            return Err(Status::not_found(format!("VM {vm_id} not found")));
        }
        self.push_config_to_node(&node).await?;
        Ok(if auto_start {
            controller_proto::VmState::Running as i32
        } else {
            controller_proto::VmState::Stopped as i32
        })
    }
}

fn short_vm_id_seed() -> String {
    let raw = uuid_v4();
    let start = raw.len().saturating_sub(8);
    raw[start..].to_string()
}

#[tonic::async_trait]
impl controller_proto::controller_server::Controller for ControllerService {
    async fn register_node(
        &self,
        request: Request<controller_proto::RegisterNodeRequest>,
    ) -> Result<Response<controller_proto::RegisterNodeResponse>, Status> {
        let req = request.into_inner();
        let (cpu, mem) = req
            .capacity
            .map(|c| (c.cpu_cores, c.memory_bytes))
            .unwrap_or((0, 0));

        let node = NodeRow {
            id: req.node_id.clone(),
            hostname: req.hostname.clone(),
            address: req.address.clone(),
            cpu_cores: cpu,
            memory_bytes: mem,
            status: "ready".into(),
            last_heartbeat: String::new(),
            gateway_interface: String::new(),
        };

        self.db
            .upsert_node(&node)
            .map_err(|e| Status::internal(format!("storing node: {e}")))?;

        if let Err(e) = self.clients.connect(&req.address).await {
            warn!(address = %req.address, error = %e, "failed to connect to node");
        }

        info!(node_id = %req.node_id, address = %req.address, "registered node");

        Ok(Response::new(controller_proto::RegisterNodeResponse {
            success: true,
            message: "registered".into(),
        }))
    }

    async fn heartbeat(
        &self,
        request: Request<controller_proto::HeartbeatRequest>,
    ) -> Result<Response<controller_proto::HeartbeatResponse>, Status> {
        let req = request.into_inner();
        let (cpu_used, mem_used) = req
            .usage
            .map(|u| (u.cpu_cores_used, u.memory_bytes_used))
            .unwrap_or((0, 0));

        let found = self
            .db
            .update_heartbeat(&req.node_id, cpu_used, mem_used)
            .map_err(|e| Status::internal(e.to_string()))?;

        if !found {
            return Err(Status::not_found(format!(
                "node {} not registered",
                req.node_id
            )));
        }

        Ok(Response::new(controller_proto::HeartbeatResponse {
            success: true,
        }))
    }

    async fn sync_vm_state(
        &self,
        request: Request<controller_proto::SyncVmStateRequest>,
    ) -> Result<Response<controller_proto::SyncVmStateResponse>, Status> {
        let req = request.into_inner();
        info!(
            node_id = %req.node_id,
            vm_count = req.vms.len(),
            "syncing VM state from node"
        );
        Ok(Response::new(controller_proto::SyncVmStateResponse {
            success: true,
        }))
    }

    async fn create_vm(
        &self,
        request: Request<controller_proto::CreateVmRequest>,
    ) -> Result<Response<controller_proto::CreateVmResponse>, Status> {
        let req = request.into_inner();
        let spec = req
            .spec
            .ok_or_else(|| Status::invalid_argument("spec is required"))?;

        let node = if !req.target_node.is_empty() {
            self.db
                .get_node_by_address(&req.target_node)
                .map_err(|e| Status::internal(e.to_string()))?
                .or_else(|| self.db.get_node(&req.target_node).ok().flatten())
                .ok_or_else(|| Status::not_found(format!("node {} not found", req.target_node)))?
        } else {
            let nodes = self
                .db
                .list_nodes()
                .map_err(|e| Status::internal(e.to_string()))?;
            scheduler::select_node(&nodes)
                .cloned()
                .ok_or_else(|| Status::unavailable("no ready nodes"))?
        };

        let vm_id = if spec.id.is_empty() {
            let mut selected: Option<String> = None;
            for _ in 0..8 {
                let candidate = format!("vm-{}", short_vm_id_seed());
                let exists = self
                    .db
                    .get_vm(&candidate)
                    .map_err(|e| Status::internal(format!("checking vm id: {e}")))?
                    .is_some();
                if !exists {
                    selected = Some(candidate);
                    break;
                }
            }
            selected.ok_or_else(|| Status::internal("failed to allocate unique vm id"))?
        } else {
            if self
                .db
                .get_vm(&spec.id)
                .map_err(|e| Status::internal(format!("checking vm id: {e}")))?
                .is_some()
            {
                return Err(Status::already_exists(format!("vm {} already exists", spec.id)));
            }
            spec.id.clone()
        };

        let vm_name = if spec.name.is_empty() {
            vm_id.clone()
        } else {
            spec.name.clone()
        };

        if self
            .db
            .find_node_for_vm(&vm_name)
            .map_err(|e| Status::internal(format!("checking vm name: {e}")))?
            .is_some()
        {
            return Err(Status::already_exists(format!("vm name {vm_name} already exists")));
        }

        let image_path = spec
            .disks
            .first()
            .map(|d| d.backend_handle.clone())
            .unwrap_or_else(|| format!("/var/lib/kcore/images/{vm_name}.raw"));

        let vm = VmRow {
            id: vm_id.clone(),
            name: vm_name,
            cpu: spec.cpu,
            memory_bytes: spec.memory_bytes,
            image_path,
            image_size: 8192,
            network: spec
                .nics
                .first()
                .map(|n| n.network.clone())
                .unwrap_or_else(|| "default".into()),
            auto_start: true,
            node_id: node.id.clone(),
            created_at: String::new(),
        };

        self.db
            .insert_vm(&vm)
            .map_err(|e| Status::internal(format!("storing vm: {e}")))?;

        info!(vm_id = %vm_id, node_id = %node.id, "created VM, pushing config");

        self.push_config_to_node(&node).await?;

        Ok(Response::new(controller_proto::CreateVmResponse {
            vm_id,
            node_id: node.id,
            state: controller_proto::VmState::Stopped as i32,
        }))
    }

    async fn delete_vm(
        &self,
        request: Request<controller_proto::DeleteVmRequest>,
    ) -> Result<Response<controller_proto::DeleteVmResponse>, Status> {
        let req = request.into_inner();
        let node = self.resolve_node_for_vm(&req.vm_id, &req.target_node)?;

        self.db
            .delete_vm(&req.vm_id)
            .map_err(|e| Status::internal(format!("deleting vm: {e}")))?;

        info!(vm_id = %req.vm_id, node_id = %node.id, "deleted VM, pushing config");

        self.push_config_to_node(&node).await?;

        Ok(Response::new(controller_proto::DeleteVmResponse {
            success: true,
        }))
    }

    async fn set_vm_desired_state(
        &self,
        request: Request<controller_proto::SetVmDesiredStateRequest>,
    ) -> Result<Response<controller_proto::SetVmDesiredStateResponse>, Status> {
        let req = request.into_inner();
        let auto_start = match controller_proto::VmDesiredState::try_from(req.desired_state)
            .unwrap_or(controller_proto::VmDesiredState::Unspecified)
        {
            controller_proto::VmDesiredState::Running => true,
            controller_proto::VmDesiredState::Stopped => false,
            controller_proto::VmDesiredState::Unspecified => {
                return Err(Status::invalid_argument(
                    "desired_state must be RUNNING or STOPPED",
                ));
            }
        };
        let state = self
            .set_vm_desired_state_internal(&req.vm_id, &req.target_node, auto_start)
            .await?;

        Ok(Response::new(controller_proto::SetVmDesiredStateResponse {
            state,
        }))
    }

    async fn get_vm(
        &self,
        request: Request<controller_proto::GetVmRequest>,
    ) -> Result<Response<controller_proto::GetVmResponse>, Status> {
        let req = request.into_inner();
        let node = self.resolve_node_for_vm(&req.vm_id, &req.target_node)?;

        let mut client = self.clients.get_compute(&node.address).ok_or_else(|| {
            Status::unavailable(format!("no connection to node {}", node.address))
        })?;

        let resp = client
            .get_vm(node_proto::GetVmRequest { vm_id: req.vm_id })
            .await?;

        let inner = resp.into_inner();

        let spec = inner.spec.map(|s| controller_proto::VmSpec {
            id: s.id,
            name: s.name,
            cpu: s.cpu,
            memory_bytes: s.memory_bytes,
            disks: s
                .disks
                .into_iter()
                .map(|d| controller_proto::Disk {
                    name: d.name,
                    backend_handle: d.backend_handle,
                    bus: d.bus,
                    device: d.device,
                })
                .collect(),
            nics: s
                .nics
                .into_iter()
                .map(|n| controller_proto::Nic {
                    network: n.network,
                    model: n.model,
                    mac_address: n.mac_address,
                })
                .collect(),
        });

        let status = inner.status.map(|s| controller_proto::VmStatus {
            id: s.id,
            state: s.state,
            created_at: s.created_at,
            updated_at: s.updated_at,
        });

        Ok(Response::new(controller_proto::GetVmResponse {
            spec,
            status,
            node_id: node.id,
        }))
    }

    async fn list_vms(
        &self,
        request: Request<controller_proto::ListVmsRequest>,
    ) -> Result<Response<controller_proto::ListVmsResponse>, Status> {
        let req = request.into_inner();

        let rows = if !req.target_node.is_empty() {
            let node = self
                .db
                .get_node_by_address(&req.target_node)
                .map_err(|e| Status::internal(e.to_string()))?
                .or_else(|| self.db.get_node(&req.target_node).ok().flatten())
                .ok_or_else(|| Status::not_found(format!("node {} not found", req.target_node)))?;
            self.db
                .list_vms_for_node(&node.id)
                .map_err(|e| Status::internal(e.to_string()))?
        } else {
            self.db
                .list_vms()
                .map_err(|e| Status::internal(e.to_string()))?
        };

        let infos = rows
            .into_iter()
            .map(|vm| controller_proto::VmInfo {
                id: vm.id,
                name: vm.name,
                state: if vm.auto_start {
                    controller_proto::VmState::Running as i32
                } else {
                    controller_proto::VmState::Stopped as i32
                },
                cpu: vm.cpu,
                memory_bytes: vm.memory_bytes,
                node_id: vm.node_id,
                created_at: None,
            })
            .collect();

        Ok(Response::new(controller_proto::ListVmsResponse {
            vms: infos,
        }))
    }

    async fn list_nodes(
        &self,
        _request: Request<controller_proto::ListNodesRequest>,
    ) -> Result<Response<controller_proto::ListNodesResponse>, Status> {
        let nodes = self
            .db
            .list_nodes()
            .map_err(|e| Status::internal(e.to_string()))?;

        let infos = nodes
            .into_iter()
            .map(|n| controller_proto::NodeInfo {
                node_id: n.id,
                hostname: n.hostname,
                address: n.address,
                capacity: Some(controller_proto::NodeCapacity {
                    cpu_cores: n.cpu_cores,
                    memory_bytes: n.memory_bytes,
                }),
                usage: None,
                status: n.status,
                last_heartbeat: None,
            })
            .collect();

        Ok(Response::new(controller_proto::ListNodesResponse {
            nodes: infos,
        }))
    }

    async fn get_node(
        &self,
        request: Request<controller_proto::GetNodeRequest>,
    ) -> Result<Response<controller_proto::GetNodeResponse>, Status> {
        let req = request.into_inner();
        let node = self
            .db
            .get_node(&req.node_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("node {} not found", req.node_id)))?;

        Ok(Response::new(controller_proto::GetNodeResponse {
            node: Some(controller_proto::NodeInfo {
                node_id: node.id,
                hostname: node.hostname,
                address: node.address,
                capacity: Some(controller_proto::NodeCapacity {
                    cpu_cores: node.cpu_cores,
                    memory_bytes: node.memory_bytes,
                }),
                usage: None,
                status: node.status,
                last_heartbeat: None,
            }),
        }))
    }
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{t:032x}")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::*;

    fn test_network() -> NetworkConfig {
        NetworkConfig {
            gateway_interface: "eno1".to_string(),
            external_ip: "203.0.113.10".to_string(),
            gateway_ip: "10.0.0.1".to_string(),
            internal_netmask: "255.255.255.0".to_string(),
        }
    }

    fn test_node() -> NodeRow {
        NodeRow {
            id: "node-1".to_string(),
            hostname: "node-1".to_string(),
            address: "127.0.0.1:9091".to_string(),
            cpu_cores: 4,
            memory_bytes: 8 * 1024 * 1024 * 1024,
            status: "ready".to_string(),
            last_heartbeat: String::new(),
            gateway_interface: "eno1".to_string(),
        }
    }

    fn test_vm(node_id: &str) -> VmRow {
        VmRow {
            id: "vm-1".to_string(),
            name: "web-1".to_string(),
            cpu: 2,
            memory_bytes: 2 * 1024 * 1024 * 1024,
            image_path: "/var/lib/kcore/images/web-1.raw".to_string(),
            image_size: 8192,
            network: "default".to_string(),
            auto_start: true,
            node_id: node_id.to_string(),
            created_at: String::new(),
        }
    }

    #[tokio::test]
    async fn set_vm_desired_state_updates_db_and_invokes_push_hook() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");
        db.insert_vm(&test_vm(&node.id)).expect("insert vm");

        let push_count = Arc::new(AtomicUsize::new(0));
        let pushed_node = Arc::new(Mutex::new(String::new()));
        let count_clone = Arc::clone(&push_count);
        let node_clone = Arc::clone(&pushed_node);
        let hook = Arc::new(move |n: &NodeRow| {
            count_clone.fetch_add(1, Ordering::SeqCst);
            *node_clone.lock().expect("lock pushed node") = n.id.clone();
            Ok(())
        });

        let svc = ControllerService::new_with_test_push_hook(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            hook,
        );

        let req = controller_proto::SetVmDesiredStateRequest {
            vm_id: "vm-1".to_string(),
            desired_state: controller_proto::VmDesiredState::Stopped as i32,
            target_node: node.id.clone(),
        };

        let resp = <ControllerService as controller_proto::controller_server::Controller>::set_vm_desired_state(
            &svc,
            Request::new(req),
        )
        .await
        .expect("set desired state")
        .into_inner();

        assert_eq!(resp.state, controller_proto::VmState::Stopped as i32);
        let vm = db.get_vm("vm-1").expect("get vm").expect("vm exists");
        assert!(
            !vm.auto_start,
            "desired stopped state should set auto_start=false"
        );
        assert_eq!(push_count.load(Ordering::SeqCst), 1);
        assert_eq!(*pushed_node.lock().expect("lock pushed node"), "node-1");
    }

    #[tokio::test]
    async fn set_vm_desired_state_rejects_unspecified_without_push() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");
        db.insert_vm(&test_vm(&node.id)).expect("insert vm");

        let push_count = Arc::new(AtomicUsize::new(0));
        let count_clone = Arc::clone(&push_count);
        let hook = Arc::new(move |_n: &NodeRow| {
            count_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        let svc = ControllerService::new_with_test_push_hook(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            hook,
        );

        let req = controller_proto::SetVmDesiredStateRequest {
            vm_id: "vm-1".to_string(),
            desired_state: controller_proto::VmDesiredState::Unspecified as i32,
            target_node: String::new(),
        };

        let err = <ControllerService as controller_proto::controller_server::Controller>::set_vm_desired_state(
            &svc,
            Request::new(req),
        )
        .await
        .expect_err("unspecified should fail");

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        let vm = db.get_vm("vm-1").expect("get vm").expect("vm exists");
        assert!(
            vm.auto_start,
            "invalid request should not mutate desired state"
        );
        assert_eq!(push_count.load(Ordering::SeqCst), 0);
    }
}
