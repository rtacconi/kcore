use std::net::Ipv4Addr;
use std::time::Duration;
use tonic::{Request, Response, Status};
use tracing::{error, info, warn};

use crate::auth::{self, CN_KCTL, CN_NODE_PREFIX};
use crate::config::NetworkConfig;
use crate::controller_proto;
use crate::db::{Database, NetworkRow, NodeRow, VmRow};
use crate::node_proto;
use crate::{nixgen, node_client::NodeClients, scheduler};

#[cfg(test)]
type PushHook = std::sync::Arc<dyn Fn(&NodeRow) -> Result<(), Status> + Send + Sync + 'static>;

pub struct ControllerService {
    db: Database,
    clients: NodeClients,
    default_network: NetworkConfig,
    #[cfg(test)]
    test_push_hook: Option<PushHook>,
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
        hook: PushHook,
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
        let networks = self
            .db
            .list_networks_for_node(&node.id)
            .map_err(|e| Status::internal(format!("listing networks: {e}")))?;

        let iface = if node.gateway_interface.is_empty() {
            &self.default_network.gateway_interface
        } else {
            &node.gateway_interface
        };

        let nix_config =
            nixgen::generate_node_config(&vms, iface, &self.default_network, &networks);

        let mut admin = self.clients.get_admin(&node.address).ok_or_else(|| {
            Status::unavailable(format!("no connection to node {}", node.address))
        })?;

        for vm in &vms {
            if vm.image_url.is_empty() {
                continue;
            }
            let ensure = admin
                .ensure_image(node_proto::EnsureImageRequest {
                    image_url: vm.image_url.clone(),
                    image_sha256: vm.image_sha256.clone(),
                    destination_path: vm.image_path.clone(),
                })
                .await
                .map_err(|e| {
                    error!(node = %node.id, vm_id = %vm.id, error = %e, "failed to ensure vm image on node");
                    Status::internal(format!("ensuring image for vm {} on node {}: {e}", vm.id, node.id))
                })?
                .into_inner();
            info!(
                node = %node.id,
                vm_id = %vm.id,
                path = %ensure.path,
                size_bytes = ensure.size_bytes,
                cached = ensure.cached,
                downloaded = ensure.downloaded,
                "ensured vm image on node"
            );
        }

        let apply = admin
            .apply_nix_config(node_proto::ApplyNixConfigRequest {
                configuration_nix: nix_config,
                rebuild: true,
            })
            .await
            .map_err(|e| {
                error!(node = %node.id, error = %e, "failed to push config to node");
                Status::internal(format!("pushing config to node {}: {e}", node.id))
            })?
            .into_inner();
        if !apply.success {
            error!(
                node = %node.id,
                message = %apply.message,
                "node rejected nix config apply request"
            );
            return Err(Status::internal(format!(
                "node {} rejected nix apply: {}",
                node.id, apply.message
            )));
        }

        info!(
            node = %node.id,
            message = %apply.message,
            "node accepted nix config apply request"
        );

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

fn validate_image_sha256(sha: &str) -> Result<String, Status> {
    let normalized = sha.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Status::invalid_argument(
            "image_sha256 must be exactly 64 hexadecimal characters",
        ));
    }
    Ok(normalized)
}

fn validate_image_url(url: &str) -> Result<String, Status> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument("image_url is required"));
    }
    if !trimmed.starts_with("https://") {
        return Err(Status::invalid_argument(
            "image_url must use https:// scheme",
        ));
    }
    Ok(trimmed.to_string())
}

fn sanitize_image_file_name(url: &str) -> String {
    let raw_name = url.rsplit('/').next().unwrap_or("image.raw");
    let cleaned: String = raw_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "image.raw".to_string()
    } else {
        cleaned
    }
}

fn derive_local_image_path(image_url: &str, image_sha256: &str) -> String {
    let file_name = sanitize_image_file_name(image_url);
    format!(
        "/var/lib/kcore/images/{}-{}",
        &image_sha256[..12],
        file_name
    )
}

fn derive_image_format(image_url: &str) -> String {
    let lower = image_url.to_ascii_lowercase();
    if lower.ends_with(".qcow2") || lower.ends_with(".qcow") {
        "qcow2".to_string()
    } else {
        "raw".to_string()
    }
}

fn validate_network_name(name: &str) -> Result<String, Status> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument("network name is required"));
    }
    if trimmed == "default" {
        return Err(Status::invalid_argument(
            "network name 'default' is reserved; configure it via controller defaultNetwork",
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Status::invalid_argument(
            "network name must contain only letters, digits, '-' or '_'",
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_ipv4(value: &str, field: &str) -> Result<String, Status> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument(format!("{field} is required")));
    }
    trimmed
        .parse::<Ipv4Addr>()
        .map_err(|_| Status::invalid_argument(format!("{field} must be a valid IPv4 address")))?;
    Ok(trimmed.to_string())
}

fn validate_netmask(value: &str) -> Result<String, Status> {
    let parsed = validate_ipv4(value, "internal_netmask")?;
    // Keep validation simple and explicit: only contiguous masks are accepted.
    let bits =
        u32::from(parsed.parse::<Ipv4Addr>().map_err(|_| {
            Status::invalid_argument("internal_netmask must be a valid IPv4 address")
        })?);
    let mut seen_zero = false;
    for i in 0..32 {
        let bit = (bits >> (31 - i)) & 1;
        if bit == 0 {
            seen_zero = true;
        } else if seen_zero {
            return Err(Status::invalid_argument(
                "internal_netmask must be contiguous (for example 255.255.255.0)",
            ));
        }
    }
    Ok(parsed)
}

fn controller_state_from_node_state(state: i32) -> i32 {
    match crate::node_proto::VmState::try_from(state).unwrap_or(crate::node_proto::VmState::Unknown)
    {
        crate::node_proto::VmState::Unknown => controller_proto::VmState::Unknown as i32,
        crate::node_proto::VmState::Stopped => controller_proto::VmState::Stopped as i32,
        crate::node_proto::VmState::Running => controller_proto::VmState::Running as i32,
        crate::node_proto::VmState::Paused => controller_proto::VmState::Paused as i32,
        crate::node_proto::VmState::Error => controller_proto::VmState::Error as i32,
    }
}

fn state_fallback_without_runtime(auto_start: bool) -> i32 {
    if auto_start {
        controller_proto::VmState::Unknown as i32
    } else {
        controller_proto::VmState::Stopped as i32
    }
}

#[tonic::async_trait]
impl controller_proto::controller_server::Controller for ControllerService {
    async fn register_node(
        &self,
        request: Request<controller_proto::RegisterNodeRequest>,
    ) -> Result<Response<controller_proto::RegisterNodeResponse>, Status> {
        auth::require_peer(&request, &[CN_NODE_PREFIX])?;
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
        auth::require_peer(&request, &[CN_NODE_PREFIX])?;
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
        auth::require_peer(&request, &[CN_NODE_PREFIX])?;
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
        auth::require_peer(&request, &[CN_KCTL])?;
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
                return Err(Status::already_exists(format!(
                    "vm {} already exists",
                    spec.id
                )));
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
            return Err(Status::already_exists(format!(
                "vm name {vm_name} already exists"
            )));
        }

        let image_url = validate_image_url(&req.image_url)?;
        let image_sha256 = validate_image_sha256(&req.image_sha256)?;
        let image_path = derive_local_image_path(&image_url, &image_sha256);
        let image_format = derive_image_format(&image_url);
        let vm_network = spec
            .nics
            .first()
            .map(|n| n.network.clone())
            .unwrap_or_else(|| "default".into());
        if vm_network != "default"
            && self
                .db
                .get_network_for_node(&node.id, &vm_network)
                .map_err(|e| Status::internal(format!("checking network: {e}")))?
                .is_none()
        {
            return Err(Status::failed_precondition(format!(
                "network '{}' is not configured on node '{}'",
                vm_network, node.id
            )));
        }

        let vm = VmRow {
            id: vm_id.clone(),
            name: vm_name,
            cpu: spec.cpu,
            memory_bytes: spec.memory_bytes,
            image_path,
            image_url,
            image_sha256,
            image_format,
            image_size: 8192,
            network: vm_network,
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
        auth::require_peer(&request, &[CN_KCTL])?;
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
        auth::require_peer(&request, &[CN_KCTL])?;
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
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();
        let node = self.resolve_node_for_vm(&req.vm_id, &req.target_node)?;
        let db_vm = self
            .db
            .get_vm(&req.vm_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .or_else(|| {
                self.db
                    .list_vms_for_node(&node.id)
                    .ok()
                    .and_then(|rows| rows.into_iter().find(|vm| vm.name == req.vm_id))
            })
            .ok_or_else(|| Status::not_found(format!("VM {} not found", req.vm_id)))?;

        let mut client = self.clients.get_compute(&node.address).ok_or_else(|| {
            Status::unavailable(format!("no connection to node {}", node.address))
        })?;

        let resp = client
            .get_vm(node_proto::GetVmRequest {
                vm_id: db_vm.name.clone(),
            })
            .await;

        let inner = match resp {
            Ok(resp) => resp.into_inner(),
            Err(err) => {
                warn!(
                    vm_id = %db_vm.id,
                    vm_name = %db_vm.name,
                    node_id = %node.id,
                    error = %err,
                    "runtime VM lookup failed; returning database-backed VM details"
                );
                let spec = Some(controller_proto::VmSpec {
                    id: db_vm.id.clone(),
                    name: db_vm.name.clone(),
                    cpu: db_vm.cpu,
                    memory_bytes: db_vm.memory_bytes,
                    disks: vec![controller_proto::Disk {
                        name: "boot".to_string(),
                        backend_handle: db_vm.image_path.clone(),
                        bus: String::new(),
                        device: String::new(),
                    }],
                    nics: vec![controller_proto::Nic {
                        network: db_vm.network.clone(),
                        model: "virtio".to_string(),
                        mac_address: String::new(),
                    }],
                });
                let status = Some(controller_proto::VmStatus {
                    id: db_vm.id.clone(),
                    state: state_fallback_without_runtime(db_vm.auto_start),
                    created_at: None,
                    updated_at: None,
                });
                return Ok(Response::new(controller_proto::GetVmResponse {
                    spec,
                    status,
                    node_id: node.id,
                }));
            }
        };

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
            state: controller_state_from_node_state(s.state),
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
        auth::require_peer(&request, &[CN_KCTL])?;
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

        let node_address_by_id = self
            .db
            .list_nodes()
            .map_err(|e| Status::internal(e.to_string()))?
            .into_iter()
            .map(|n| (n.id, n.address))
            .collect::<std::collections::HashMap<_, _>>();

        let mut infos = Vec::with_capacity(rows.len());
        for vm in rows {
            let mut state = state_fallback_without_runtime(vm.auto_start);
            if let Some(node_address) = node_address_by_id.get(&vm.node_id) {
                if let Some(mut compute) = self.clients.get_compute(node_address) {
                    match tokio::time::timeout(
                        Duration::from_secs(3),
                        compute.get_vm(node_proto::GetVmRequest {
                            vm_id: vm.name.clone(),
                        }),
                    )
                    .await
                    {
                        Ok(Ok(resp)) => {
                            if let Some(status) = resp.into_inner().status {
                                state = controller_state_from_node_state(status.state);
                            }
                        }
                        Ok(Err(err)) => {
                            warn!(node_id = %vm.node_id, vm_name = %vm.name, address = %node_address, error = %err, "failed to fetch runtime VM state");
                        }
                        Err(_) => {
                            warn!(node_id = %vm.node_id, vm_name = %vm.name, address = %node_address, "timed out fetching runtime VM state");
                        }
                    }
                }
            }
            infos.push(controller_proto::VmInfo {
                id: vm.id,
                name: vm.name,
                state,
                cpu: vm.cpu,
                memory_bytes: vm.memory_bytes,
                node_id: vm.node_id,
                created_at: None,
            });
        }

        Ok(Response::new(controller_proto::ListVmsResponse {
            vms: infos,
        }))
    }

    async fn create_network(
        &self,
        request: Request<controller_proto::CreateNetworkRequest>,
    ) -> Result<Response<controller_proto::CreateNetworkResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();
        let name = validate_network_name(&req.name)?;
        let external_ip = validate_ipv4(&req.external_ip, "external_ip")?;
        let gateway_ip = validate_ipv4(&req.gateway_ip, "gateway_ip")?;
        let internal_netmask = if req.internal_netmask.trim().is_empty() {
            "255.255.255.0".to_string()
        } else {
            validate_netmask(&req.internal_netmask)?
        };

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

        if self
            .db
            .get_network_for_node(&node.id, &name)
            .map_err(|e| Status::internal(format!("checking existing network: {e}")))?
            .is_some()
        {
            return Err(Status::already_exists(format!(
                "network '{}' already exists on node '{}'",
                name, node.id
            )));
        }

        self.db
            .insert_network(&NetworkRow {
                name: name.clone(),
                external_ip,
                gateway_ip,
                internal_netmask,
                node_id: node.id.clone(),
            })
            .map_err(|e| Status::internal(format!("storing network: {e}")))?;

        self.push_config_to_node(&node).await?;

        Ok(Response::new(controller_proto::CreateNetworkResponse {
            success: true,
            message: format!("created network '{name}' on node '{}'", node.id),
            node_id: node.id,
        }))
    }

    async fn delete_network(
        &self,
        request: Request<controller_proto::DeleteNetworkRequest>,
    ) -> Result<Response<controller_proto::DeleteNetworkResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();
        let name = req.name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("network name is required"));
        }
        if name == "default" {
            return Err(Status::invalid_argument(
                "cannot delete reserved network 'default'",
            ));
        }

        let node = if !req.target_node.is_empty() {
            self.db
                .get_node_by_address(&req.target_node)
                .map_err(|e| Status::internal(e.to_string()))?
                .or_else(|| self.db.get_node(&req.target_node).ok().flatten())
                .ok_or_else(|| Status::not_found(format!("node {} not found", req.target_node)))?
        } else {
            let matches = self
                .db
                .list_networks()
                .map_err(|e| Status::internal(format!("listing networks: {e}")))?
                .into_iter()
                .filter(|n| n.name == name)
                .collect::<Vec<_>>();
            if matches.is_empty() {
                return Err(Status::not_found(format!("network '{name}' not found")));
            }
            if matches.len() > 1 {
                return Err(Status::failed_precondition(format!(
                    "network '{name}' exists on multiple nodes; pass target_node"
                )));
            }
            self.db
                .get_node(&matches[0].node_id)
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| {
                    Status::not_found(format!("node '{}' not found", matches[0].node_id))
                })?
        };

        let in_use = self
            .db
            .list_vms_for_node(&node.id)
            .map_err(|e| Status::internal(format!("listing vms: {e}")))?
            .into_iter()
            .any(|vm| vm.network == name);
        if in_use {
            return Err(Status::failed_precondition(format!(
                "network '{name}' is still in use by at least one VM on node '{}'",
                node.id
            )));
        }

        let deleted = self
            .db
            .delete_network(&node.id, name)
            .map_err(|e| Status::internal(format!("deleting network: {e}")))?;
        if !deleted {
            return Err(Status::not_found(format!(
                "network '{name}' not found on node '{}'",
                node.id
            )));
        }

        self.push_config_to_node(&node).await?;
        Ok(Response::new(controller_proto::DeleteNetworkResponse {
            success: true,
        }))
    }

    async fn list_networks(
        &self,
        request: Request<controller_proto::ListNetworksRequest>,
    ) -> Result<Response<controller_proto::ListNetworksResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();
        let rows = if req.target_node.is_empty() {
            self.db
                .list_networks()
                .map_err(|e| Status::internal(format!("listing networks: {e}")))?
        } else {
            let node = self
                .db
                .get_node_by_address(&req.target_node)
                .map_err(|e| Status::internal(e.to_string()))?
                .or_else(|| self.db.get_node(&req.target_node).ok().flatten())
                .ok_or_else(|| Status::not_found(format!("node {} not found", req.target_node)))?;
            self.db
                .list_networks_for_node(&node.id)
                .map_err(|e| Status::internal(format!("listing networks for node: {e}")))?
        };

        Ok(Response::new(controller_proto::ListNetworksResponse {
            networks: rows
                .into_iter()
                .map(|n| controller_proto::NetworkInfo {
                    name: n.name,
                    external_ip: n.external_ip,
                    gateway_ip: n.gateway_ip,
                    internal_netmask: n.internal_netmask,
                    node_id: n.node_id,
                })
                .collect(),
        }))
    }

    async fn list_nodes(
        &self,
        request: Request<controller_proto::ListNodesRequest>,
    ) -> Result<Response<controller_proto::ListNodesResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
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
        auth::require_peer(&request, &[CN_KCTL])?;
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
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
#[allow(clippy::result_large_err)]
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
            image_url: "https://example.com/web-1.raw".to_string(),
            image_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            image_format: "raw".to_string(),
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
        let hook: PushHook = Arc::new(move |n: &NodeRow| {
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
        let hook: PushHook = Arc::new(move |_n: &NodeRow| {
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

    #[test]
    fn validate_image_url_requires_https() {
        let err = validate_image_url("http://example.com/debian.raw").expect_err("must fail");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn validate_image_sha256_requires_hex_len_64() {
        let err = validate_image_sha256("1234").expect_err("must fail");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn runtime_state_mapping_never_assumes_running() {
        assert_eq!(
            state_fallback_without_runtime(true),
            controller_proto::VmState::Unknown as i32
        );
        assert_eq!(
            state_fallback_without_runtime(false),
            controller_proto::VmState::Stopped as i32
        );
        assert_eq!(
            controller_state_from_node_state(crate::node_proto::VmState::Running as i32),
            controller_proto::VmState::Running as i32
        );
        assert_eq!(
            controller_state_from_node_state(crate::node_proto::VmState::Unknown as i32),
            controller_proto::VmState::Unknown as i32
        );
    }

    #[test]
    fn derive_local_image_path_is_deterministic() {
        let p1 = derive_local_image_path(
            "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        let p2 = derive_local_image_path(
            "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        assert_eq!(p1, p2);
        assert!(p1.starts_with("/var/lib/kcore/images/aaaaaaaaaaaa-"));
    }

    #[test]
    fn derive_image_format_uses_qcow2_extension() {
        assert_eq!(
            derive_image_format("https://example.com/debian-12-genericcloud-amd64.qcow2"),
            "qcow2"
        );
        assert_eq!(derive_image_format("https://example.com/rootfs.raw"), "raw");
    }

    #[test]
    fn validate_network_inputs_reject_bad_values() {
        let reserved = validate_network_name("default").expect_err("default is reserved");
        assert_eq!(reserved.code(), tonic::Code::InvalidArgument);
        let invalid_ip = validate_ipv4("10.0.0", "gateway_ip").expect_err("invalid ip");
        assert_eq!(invalid_ip.code(), tonic::Code::InvalidArgument);
        let invalid_mask =
            validate_netmask("255.0.255.0").expect_err("non-contiguous mask should fail");
        assert_eq!(invalid_mask.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn create_vm_rejects_missing_image_url_and_sha() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");

        let hook: PushHook = Arc::new(|_n: &NodeRow| Ok(()));
        let svc = ControllerService::new_with_test_push_hook(
            db,
            NodeClients::new(None),
            test_network(),
            hook,
        );

        let req = controller_proto::CreateVmRequest {
            target_node: node.id,
            spec: Some(controller_proto::VmSpec {
                id: String::new(),
                name: "vm-a".to_string(),
                cpu: 1,
                memory_bytes: 512 * 1024 * 1024,
                disks: vec![],
                nics: vec![],
            }),
            image_url: String::new(),
            image_sha256: String::new(),
        };

        let err =
            <ControllerService as controller_proto::controller_server::Controller>::create_vm(
                &svc,
                Request::new(req),
            )
            .await
            .expect_err("missing image_url should be rejected");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
        assert!(err.message().contains("image_url"));
    }
}
