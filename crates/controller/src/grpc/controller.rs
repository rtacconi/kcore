use std::time::Duration;
use tonic::{Request, Response, Status};
use tracing::{error, info, warn};

use crate::auth::{self, CN_KCTL, CN_NODE_PREFIX};
use crate::config::NetworkConfig;
use crate::controller_proto;
use crate::db::{Database, NetworkRow, NodeRow, VmRow};
use crate::node_proto;
use crate::{nixgen, node_client::NodeClients, scheduler};

use super::helpers::{
    controller_state_from_node_state, parse_datetime_to_timestamp, parse_port_list,
    short_vm_id_seed, state_fallback_without_runtime,
};
use super::validation::{
    derive_image_format, derive_image_format_from_path, derive_local_image_path,
    normalize_image_format, normalize_storage_backend, storage_backend_to_proto,
    validate_image_path, validate_image_sha256, validate_image_url, validate_ipv4,
    validate_netmask, validate_network_name, validate_storage_size_bytes,
};

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

        let mut vm_ssh_keys: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for vm in &vms {
            match self.db.get_vm_ssh_keys(&vm.id) {
                Ok(keys) if !keys.is_empty() => {
                    vm_ssh_keys.insert(vm.id.clone(), keys);
                }
                _ => {}
            }
        }

        let nix_config = nixgen::generate_node_config(
            &vms,
            iface,
            &self.default_network,
            &networks,
            &vm_ssh_keys,
        );

        let mut admin = self.ensure_admin_client_for_node(node).await?;

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

    async fn ensure_admin_client_for_node(
        &self,
        node: &NodeRow,
    ) -> Result<node_proto::node_admin_client::NodeAdminClient<tonic::transport::Channel>, Status>
    {
        if let Some(client) = self.clients.get_admin(&node.address) {
            return Ok(client);
        }
        self.clients.connect(&node.address).await.map_err(|e| {
            Status::unavailable(format!("no connection to node {}: {e}", node.address))
        })?;
        self.clients
            .get_admin(&node.address)
            .ok_or_else(|| Status::unavailable(format!("no connection to node {}", node.address)))
    }

    async fn ensure_compute_client_for_address(
        &self,
        address: &str,
    ) -> Result<node_proto::node_compute_client::NodeComputeClient<tonic::transport::Channel>, Status>
    {
        if let Some(client) = self.clients.get_compute(address) {
            return Ok(client);
        }
        self.clients
            .connect(address)
            .await
            .map_err(|e| Status::unavailable(format!("no connection to node {address}: {e}")))?;
        self.clients
            .get_compute(address)
            .ok_or_else(|| Status::unavailable(format!("no connection to node {address}")))
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
        let storage_backend = normalize_storage_backend(req.storage_backend, false)?;

        let node = NodeRow {
            id: req.node_id.clone(),
            hostname: req.hostname.clone(),
            address: req.address.clone(),
            cpu_cores: cpu,
            memory_bytes: mem,
            status: "ready".into(),
            last_heartbeat: String::new(),
            gateway_interface: String::new(),
            cpu_used: 0,
            memory_used: 0,
            storage_backend,
        };

        self.db
            .upsert_node(&node)
            .map_err(|e| Status::internal(format!("storing node: {e}")))?;

        if !req.labels.is_empty() {
            self.db
                .upsert_node_labels(&req.node_id, &req.labels)
                .map_err(|e| Status::internal(format!("storing labels: {e}")))?;
        }

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

        for vm in &req.vms {
            let state_str = match controller_proto::VmState::try_from(vm.state) {
                Ok(controller_proto::VmState::Running) => "running",
                Ok(controller_proto::VmState::Stopped) => "stopped",
                Ok(controller_proto::VmState::Paused) => "paused",
                Ok(controller_proto::VmState::Error) => "error",
                _ => "unknown",
            };
            match self
                .db
                .update_vm_runtime_state(&req.node_id, &vm.name, state_str)
            {
                Ok(true) => {}
                Ok(false) => {
                    warn!(
                        node_id = %req.node_id,
                        vm_name = %vm.name,
                        "node reported VM not tracked by controller (orphan)"
                    );
                }
                Err(e) => {
                    error!(
                        node_id = %req.node_id,
                        vm_name = %vm.name,
                        error = %e,
                        "failed to update VM runtime state"
                    );
                }
            }
        }

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

        let requested_storage_backend = normalize_storage_backend(req.storage_backend, true)?;
        let requested_storage_size_bytes = validate_storage_size_bytes(req.storage_size_bytes)?;

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
            let compatible_nodes: Vec<NodeRow> = nodes
                .into_iter()
                .filter(|n| n.storage_backend == requested_storage_backend)
                .collect();
            scheduler::select_node_for_vm(&compatible_nodes, spec.cpu, spec.memory_bytes)
                .cloned()
                .ok_or_else(|| {
                    Status::unavailable(
                        "no ready node with sufficient capacity matching requested storage backend",
                    )
                })?
        };
        if node.storage_backend != requested_storage_backend {
            return Err(Status::failed_precondition(format!(
                "VM storage backend '{}' does not match node '{}' backend '{}'",
                requested_storage_backend, node.id, node.storage_backend
            )));
        }

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

        let image_url_input = req.image_url.trim();
        let image_path_input = req.image_path.trim();
        if image_url_input.is_empty() && image_path_input.is_empty() {
            return Err(Status::invalid_argument(
                "either image_url or image_path is required",
            ));
        }
        if !image_url_input.is_empty() && !image_path_input.is_empty() {
            return Err(Status::invalid_argument(
                "image_url and image_path are mutually exclusive",
            ));
        }

        let (image_url, image_sha256, image_path, image_format) = if !image_url_input.is_empty() {
            let image_url = validate_image_url(image_url_input)?;
            let image_sha256 = validate_image_sha256(&req.image_sha256)?;
            let image_path = derive_local_image_path(&image_url, &image_sha256);
            let image_format = derive_image_format(&image_url);
            (image_url, image_sha256, image_path, image_format)
        } else {
            let image_path = validate_image_path(image_path_input)?;
            let image_format = if req.image_format.trim().is_empty() {
                derive_image_format_from_path(&image_path)
            } else {
                normalize_image_format(&req.image_format)?
            };
            (String::new(), String::new(), image_path, image_format)
        };
        let existing_on_node = self
            .db
            .list_vms_for_node(&node.id)
            .map_err(|e| Status::internal(format!("listing vms for image collision check: {e}")))?;
        if let Some(conflict) = existing_on_node
            .into_iter()
            .find(|existing| existing.image_path == image_path)
        {
            return Err(Status::failed_precondition(format!(
                "image path '{}' is already used by VM '{}' on node '{}'; duplicate writable disk usage is not supported",
                image_path, conflict.name, node.id
            )));
        }
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
            runtime_state: "unknown".to_string(),
            cloud_init_user_data: req.cloud_init_user_data,
            storage_backend: requested_storage_backend,
            storage_size_bytes: requested_storage_size_bytes,
        };

        self.db
            .insert_vm(&vm)
            .map_err(|e| Status::internal(format!("storing vm: {e}")))?;

        if !req.ssh_key_names.is_empty() {
            for key_name in &req.ssh_key_names {
                if self
                    .db
                    .get_ssh_key(key_name)
                    .map_err(|e| Status::internal(format!("checking ssh key: {e}")))?
                    .is_none()
                {
                    self.db.delete_vm_by_id_or_name(&vm_id).ok();
                    return Err(Status::not_found(format!(
                        "SSH key '{}' not found",
                        key_name
                    )));
                }
            }
            self.db
                .associate_vm_ssh_keys(&vm_id, &req.ssh_key_names)
                .map_err(|e| {
                    self.db.delete_vm_by_id_or_name(&vm_id).ok();
                    Status::internal(format!("associating ssh keys: {e}"))
                })?;
        }

        info!(vm_id = %vm_id, node_id = %node.id, "created VM, pushing config");

        if let Err(push_err) = self.push_config_to_node(&node).await {
            warn!(
                vm_id = %vm_id,
                node_id = %node.id,
                error = %push_err,
                "failed to push config after VM insert; rolling back VM row"
            );
            if let Err(db_err) = self.db.delete_vm_by_id_or_name(&vm_id) {
                error!(
                    vm_id = %vm_id,
                    node_id = %node.id,
                    error = %db_err,
                    "rollback failed after push error"
                );
                return Err(Status::internal(format!(
                    "failed to apply VM config and rollback VM {}: push error: {}; rollback error: {}",
                    vm_id,
                    push_err.message(),
                    db_err
                )));
            }
            return Err(Status::aborted(format!(
                "failed to apply VM {} on node {}: {}",
                vm_id,
                node.id,
                push_err.message()
            )));
        }

        Ok(Response::new(controller_proto::CreateVmResponse {
            vm_id,
            node_id: node.id,
            state: controller_proto::VmState::Stopped as i32,
        }))
    }

    async fn update_vm(
        &self,
        request: Request<controller_proto::UpdateVmRequest>,
    ) -> Result<Response<controller_proto::UpdateVmResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();

        if req.vm_id.is_empty() {
            return Err(Status::invalid_argument("vm_id is required"));
        }

        let node = self.resolve_node_for_vm(&req.vm_id, &req.target_node)?;

        let cpu = if req.cpu > 0 { Some(req.cpu) } else { None };
        let mem = if req.memory_bytes > 0 {
            Some(req.memory_bytes)
        } else {
            None
        };

        if cpu.is_none() && mem.is_none() {
            return Err(Status::invalid_argument(
                "at least one of cpu or memory_bytes must be set",
            ));
        }

        let updated = self
            .db
            .update_vm_spec(&req.vm_id, cpu, mem)
            .map_err(|e| Status::internal(format!("updating vm: {e}")))?;
        if !updated {
            return Err(Status::not_found(format!("VM '{}' not found", req.vm_id)));
        }

        info!(vm_id = %req.vm_id, cpu = ?cpu, memory_bytes = ?mem, "updated VM spec, pushing config");
        self.push_config_to_node(&node).await?;

        Ok(Response::new(controller_proto::UpdateVmResponse {
            success: true,
            message: format!("VM '{}' updated", req.vm_id),
        }))
    }

    async fn delete_vm(
        &self,
        request: Request<controller_proto::DeleteVmRequest>,
    ) -> Result<Response<controller_proto::DeleteVmResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();
        let node = self.resolve_node_for_vm(&req.vm_id, &req.target_node)?;

        let deleted = self
            .db
            .delete_vm_by_id_or_name(&req.vm_id)
            .map_err(|e| Status::internal(format!("deleting vm: {e}")))?;
        if !deleted {
            return Err(Status::not_found(format!("VM '{}' not found", req.vm_id)));
        }

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

        let mut client = self
            .ensure_compute_client_for_address(&node.address)
            .await?;

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

        let vm_count = rows.len();
        let mut fallback_states: Vec<i32> = Vec::with_capacity(vm_count);
        let mut set = tokio::task::JoinSet::new();

        for (idx, vm) in rows.iter().enumerate() {
            fallback_states.push(state_fallback_without_runtime(vm.auto_start));
            if let Some(node_address) = node_address_by_id.get(&vm.node_id) {
                if self.clients.get_compute(node_address).is_none() {
                    if let Err(err) = self.clients.connect(node_address).await {
                        warn!(address = %node_address, error = %err, "failed to refresh node compute client");
                    }
                }
                if let Some(mut compute) = self.clients.get_compute(node_address) {
                    let vm_name = vm.name.clone();
                    let node_id = vm.node_id.clone();
                    let addr = node_address.clone();
                    set.spawn(async move {
                        let result = tokio::time::timeout(
                            Duration::from_secs(3),
                            compute.get_vm(node_proto::GetVmRequest {
                                vm_id: vm_name.clone(),
                            }),
                        )
                        .await;
                        (idx, vm_name, node_id, addr, result)
                    });
                }
            }
        }

        let mut live_states: Vec<Option<i32>> = vec![None; vm_count];
        while let Some(Ok((idx, vm_name, node_id, addr, result))) = set.join_next().await {
            match result {
                Ok(Ok(resp)) => {
                    if let Some(status) = resp.into_inner().status {
                        live_states[idx] = Some(controller_state_from_node_state(status.state));
                    }
                }
                Ok(Err(err)) => {
                    warn!(node_id = %node_id, vm_name = %vm_name, address = %addr, error = %err, "failed to fetch runtime VM state");
                }
                Err(_) => {
                    warn!(node_id = %node_id, vm_name = %vm_name, address = %addr, "timed out fetching runtime VM state");
                }
            }
        }

        let infos: Vec<_> = rows
            .into_iter()
            .enumerate()
            .map(|(i, vm)| {
                let state = live_states[i].unwrap_or(fallback_states[i]);
                controller_proto::VmInfo {
                    id: vm.id,
                    name: vm.name,
                    state,
                    cpu: vm.cpu,
                    memory_bytes: vm.memory_bytes,
                    node_id: vm.node_id,
                    created_at: None,
                }
            })
            .collect();

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
                allowed_tcp_ports: req
                    .allowed_tcp_ports
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                allowed_udp_ports: req
                    .allowed_udp_ports
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                vlan_id: req.vlan_id,
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
                    allowed_tcp_ports: parse_port_list(&n.allowed_tcp_ports),
                    allowed_udp_ports: parse_port_list(&n.allowed_udp_ports),
                    vlan_id: n.vlan_id,
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

        let all_labels = self.db.get_all_node_labels().unwrap_or_default();

        let infos = nodes
            .into_iter()
            .map(|n| {
                let labels: Vec<String> = all_labels
                    .iter()
                    .filter(|(nid, _)| nid == &n.id)
                    .map(|(_, l)| l.clone())
                    .collect();
                let hb = if n.last_heartbeat.is_empty() {
                    None
                } else {
                    parse_datetime_to_timestamp(&n.last_heartbeat)
                };
                controller_proto::NodeInfo {
                    node_id: n.id,
                    hostname: n.hostname,
                    address: n.address,
                    capacity: Some(controller_proto::NodeCapacity {
                        cpu_cores: n.cpu_cores,
                        memory_bytes: n.memory_bytes,
                    }),
                    usage: Some(controller_proto::NodeUsage {
                        cpu_cores_used: n.cpu_used,
                        memory_bytes_used: n.memory_used,
                    }),
                    status: n.status,
                    last_heartbeat: hb,
                    labels,
                    storage_backend: storage_backend_to_proto(&n.storage_backend),
                }
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

        let labels = self.db.get_node_labels(&node.id).unwrap_or_default();
        let hb = if node.last_heartbeat.is_empty() {
            None
        } else {
            parse_datetime_to_timestamp(&node.last_heartbeat)
        };

        Ok(Response::new(controller_proto::GetNodeResponse {
            node: Some(controller_proto::NodeInfo {
                node_id: node.id,
                hostname: node.hostname,
                address: node.address,
                capacity: Some(controller_proto::NodeCapacity {
                    cpu_cores: node.cpu_cores,
                    memory_bytes: node.memory_bytes,
                }),
                usage: Some(controller_proto::NodeUsage {
                    cpu_cores_used: node.cpu_used,
                    memory_bytes_used: node.memory_used,
                }),
                status: node.status,
                last_heartbeat: hb,
                labels,
                storage_backend: storage_backend_to_proto(&node.storage_backend),
            }),
        }))
    }

    async fn create_ssh_key(
        &self,
        request: Request<controller_proto::CreateSshKeyRequest>,
    ) -> Result<Response<controller_proto::CreateSshKeyResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();

        if req.name.trim().is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }
        if req.public_key.trim().is_empty() {
            return Err(Status::invalid_argument("public_key is required"));
        }
        if !req.public_key.starts_with("ssh-") && !req.public_key.starts_with("ecdsa-") {
            return Err(Status::invalid_argument(
                "public_key must start with ssh- or ecdsa- (OpenSSH format)",
            ));
        }

        self.db
            .insert_ssh_key(req.name.trim(), req.public_key.trim())
            .map_err(|e| {
                if e.to_string().contains("UNIQUE constraint") {
                    Status::already_exists(format!("SSH key '{}' already exists", req.name))
                } else {
                    Status::internal(format!("storing ssh key: {e}"))
                }
            })?;

        info!(name = %req.name, "created SSH key");

        Ok(Response::new(controller_proto::CreateSshKeyResponse {
            success: true,
            message: format!("SSH key '{}' created", req.name),
        }))
    }

    async fn delete_ssh_key(
        &self,
        request: Request<controller_proto::DeleteSshKeyRequest>,
    ) -> Result<Response<controller_proto::DeleteSshKeyResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();

        let deleted = self
            .db
            .delete_ssh_key(&req.name)
            .map_err(|e| Status::internal(format!("deleting ssh key: {e}")))?;

        if !deleted {
            return Err(Status::not_found(format!(
                "SSH key '{}' not found",
                req.name
            )));
        }

        info!(name = %req.name, "deleted SSH key");

        Ok(Response::new(controller_proto::DeleteSshKeyResponse {
            success: true,
        }))
    }

    async fn list_ssh_keys(
        &self,
        request: Request<controller_proto::ListSshKeysRequest>,
    ) -> Result<Response<controller_proto::ListSshKeysResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;

        let keys = self
            .db
            .list_ssh_keys()
            .map_err(|e| Status::internal(format!("listing ssh keys: {e}")))?;

        let infos = keys
            .into_iter()
            .map(|(name, public_key, created_at)| {
                let ts = parse_datetime_to_timestamp(&created_at);
                controller_proto::SshKeyInfo {
                    name,
                    public_key,
                    created_at: ts,
                }
            })
            .collect();

        Ok(Response::new(controller_proto::ListSshKeysResponse {
            keys: infos,
        }))
    }

    async fn get_ssh_key(
        &self,
        request: Request<controller_proto::GetSshKeyRequest>,
    ) -> Result<Response<controller_proto::GetSshKeyResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();

        let (name, public_key, created_at) = self
            .db
            .get_ssh_key(&req.name)
            .map_err(|e| Status::internal(format!("getting ssh key: {e}")))?
            .ok_or_else(|| Status::not_found(format!("SSH key '{}' not found", req.name)))?;

        let ts = parse_datetime_to_timestamp(&created_at);

        Ok(Response::new(controller_proto::GetSshKeyResponse {
            key: Some(controller_proto::SshKeyInfo {
                name,
                public_key,
                created_at: ts,
            }),
        }))
    }

    async fn drain_node(
        &self,
        request: Request<controller_proto::DrainNodeRequest>,
    ) -> Result<Response<controller_proto::DrainNodeResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();

        let source_node = self
            .db
            .get_node(&req.node_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("node '{}' not found", req.node_id)))?;

        self.db
            .update_node_status(&req.node_id, "draining")
            .map_err(|e| Status::internal(format!("updating node status: {e}")))?;

        let vms = self
            .db
            .list_vms_for_node(&req.node_id)
            .map_err(|e| Status::internal(format!("listing vms: {e}")))?;

        if vms.is_empty() {
            self.db
                .update_node_status(&req.node_id, "drained")
                .map_err(|e| Status::internal(format!("updating node status: {e}")))?;
            return Ok(Response::new(controller_proto::DrainNodeResponse {
                success: true,
                vms_migrated: 0,
                message: "node has no VMs, marked as drained".into(),
            }));
        }

        let all_nodes = self
            .db
            .list_nodes()
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut migrated = 0i32;
        let mut errors = Vec::new();
        let eligible_nodes: Vec<NodeRow> = all_nodes
            .iter()
            .filter(|n| n.id != req.node_id)
            .cloned()
            .collect();

        let mut destination_node_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for vm in &vms {
            let target = if !req.target_node.is_empty() {
                eligible_nodes
                    .iter()
                    .find(|n| n.id == req.target_node || n.address == req.target_node)
                    .ok_or_else(|| {
                        Status::not_found(format!("target node '{}' not found", req.target_node))
                    })?
            } else {
                match scheduler::select_node_for_vm(&eligible_nodes, vm.cpu, vm.memory_bytes) {
                    Some(n) => n,
                    None => {
                        errors.push(format!("no node with capacity for VM '{}'", vm.name));
                        continue;
                    }
                }
            };

            let deleted = self
                .db
                .delete_vm_by_id_or_name(&vm.id)
                .map_err(|e| Status::internal(format!("deleting vm: {e}")))?;
            if !deleted {
                continue;
            }

            let mut new_vm = vm.clone();
            new_vm.node_id = target.id.clone();
            if let Err(e) = self.db.insert_vm(&new_vm) {
                errors.push(format!("re-inserting VM '{}': {e}", vm.name));
                continue;
            }

            let ssh_keys = self.db.get_vm_ssh_keys(&vm.id).unwrap_or_default();
            if !ssh_keys.is_empty() {
                let key_names: Vec<String> = self
                    .db
                    .list_ssh_keys()
                    .unwrap_or_default()
                    .iter()
                    .filter(|(_, pk, _)| ssh_keys.contains(pk))
                    .map(|(name, _, _)| name.clone())
                    .collect();
                let _ = self.db.associate_vm_ssh_keys(&new_vm.id, &key_names);
            }

            migrated += 1;
            destination_node_ids.insert(target.id.clone());
        }

        if let Err(e) = self.push_config_to_node(&source_node).await {
            warn!(node = %req.node_id, error = %e, "failed to push config to drained node");
        }

        for target_id in &destination_node_ids {
            if let Ok(Some(target_node)) = self.db.get_node(target_id) {
                if let Err(e) = self.push_config_to_node(&target_node).await {
                    warn!(node = %target_id, error = %e, "failed to push config to target node");
                }
            }
        }

        self.db
            .update_node_status(&req.node_id, "drained")
            .map_err(|e| Status::internal(format!("updating node status: {e}")))?;

        let msg = if errors.is_empty() {
            format!("{migrated} VMs migrated successfully")
        } else {
            format!(
                "{migrated} VMs migrated, {} errors: {}",
                errors.len(),
                errors.join("; ")
            )
        };

        Ok(Response::new(controller_proto::DrainNodeResponse {
            success: errors.is_empty(),
            vms_migrated: migrated,
            message: msg,
        }))
    }
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
            cpu_used: 0,
            memory_used: 0,
            storage_backend: "filesystem".to_string(),
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
            runtime_state: "unknown".to_string(),
            cloud_init_user_data: String::new(),
            storage_backend: "filesystem".to_string(),
            storage_size_bytes: 10 * 1024 * 1024 * 1024,
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
            cloud_init_user_data: String::new(),
            image_path: String::new(),
            image_format: String::new(),
            ssh_key_names: vec![],
            storage_backend: controller_proto::StorageBackendType::Filesystem as i32,
            storage_size_bytes: 8 * 1024 * 1024 * 1024,
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

    #[tokio::test]
    async fn create_vm_rolls_back_when_push_fails() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");

        let hook: PushHook =
            Arc::new(|_n: &NodeRow| Err(Status::internal("simulated push failure for test")));
        let svc = ControllerService::new_with_test_push_hook(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            hook,
        );

        let req = controller_proto::CreateVmRequest {
            target_node: node.id.clone(),
            spec: Some(controller_proto::VmSpec {
                id: String::new(),
                name: "vm-rollback".to_string(),
                cpu: 1,
                memory_bytes: 512 * 1024 * 1024,
                disks: vec![],
                nics: vec![],
            }),
            image_url: "https://example.com/debian.raw".to_string(),
            image_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            cloud_init_user_data: String::new(),
            image_path: String::new(),
            image_format: String::new(),
            ssh_key_names: vec![],
            storage_backend: controller_proto::StorageBackendType::Filesystem as i32,
            storage_size_bytes: 8 * 1024 * 1024 * 1024,
        };

        let err =
            <ControllerService as controller_proto::controller_server::Controller>::create_vm(
                &svc,
                Request::new(req),
            )
            .await
            .expect_err("create should fail when push fails");
        assert_eq!(err.code(), tonic::Code::Aborted);

        let found = db
            .find_node_for_vm("vm-rollback")
            .expect("query vm by name after failed create");
        assert!(found.is_none(), "failed create should be rolled back");
    }

    #[tokio::test]
    async fn create_vm_rejects_image_path_already_in_use() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");
        db.insert_vm(&test_vm(&node.id))
            .expect("insert existing vm");

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
                name: "vm-path-conflict".to_string(),
                cpu: 1,
                memory_bytes: 512 * 1024 * 1024,
                disks: vec![],
                nics: vec![],
            }),
            image_url: String::new(),
            image_sha256: String::new(),
            cloud_init_user_data: String::new(),
            image_path: "/var/lib/kcore/images/web-1.raw".to_string(),
            image_format: "raw".to_string(),
            ssh_key_names: vec![],
            storage_backend: controller_proto::StorageBackendType::Filesystem as i32,
            storage_size_bytes: 8 * 1024 * 1024 * 1024,
        };

        let err =
            <ControllerService as controller_proto::controller_server::Controller>::create_vm(
                &svc,
                Request::new(req),
            )
            .await
            .expect_err("duplicate image path should be rejected");
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("already used"));
    }

    #[tokio::test]
    async fn create_vm_rejects_storage_backend_mismatch() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.storage_backend = "zfs".to_string();
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
                name: "vm-storage-mismatch".to_string(),
                cpu: 1,
                memory_bytes: 512 * 1024 * 1024,
                disks: vec![],
                nics: vec![],
            }),
            image_url: String::new(),
            image_sha256: String::new(),
            cloud_init_user_data: String::new(),
            image_path: "/var/lib/kcore/images/base.raw".to_string(),
            image_format: "raw".to_string(),
            ssh_key_names: vec![],
            storage_backend: controller_proto::StorageBackendType::Filesystem as i32,
            storage_size_bytes: 8 * 1024 * 1024 * 1024,
        };

        let err =
            <ControllerService as controller_proto::controller_server::Controller>::create_vm(
                &svc,
                Request::new(req),
            )
            .await
            .expect_err("mismatched storage backend should fail");
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("does not match"));
    }

    #[tokio::test]
    async fn drain_node_moves_vms_to_target_and_pushes_config() {
        let db = Database::open(":memory:").expect("open db");

        let mut node_a = test_node();
        node_a.id = "node-a".to_string();
        node_a.hostname = "node-a".to_string();
        db.upsert_node(&node_a).expect("insert node-a");

        let mut node_b = test_node();
        node_b.id = "node-b".to_string();
        node_b.hostname = "node-b".to_string();
        node_b.address = "127.0.0.2:9091".to_string();
        db.upsert_node(&node_b).expect("insert node-b");

        let mut vm1 = test_vm("node-a");
        vm1.id = "vm-drain-1".to_string();
        vm1.name = "drain-web-1".to_string();
        db.insert_vm(&vm1).expect("insert vm1");

        let mut vm2 = test_vm("node-a");
        vm2.id = "vm-drain-2".to_string();
        vm2.name = "drain-web-2".to_string();
        db.insert_vm(&vm2).expect("insert vm2");

        let pushed_nodes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let pushed_clone = Arc::clone(&pushed_nodes);
        let hook: PushHook = Arc::new(move |n: &NodeRow| {
            pushed_clone.lock().expect("lock").push(n.id.clone());
            Ok(())
        });

        let svc = ControllerService::new_with_test_push_hook(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            hook,
        );

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::drain_node(
                &svc,
                Request::new(controller_proto::DrainNodeRequest {
                    node_id: "node-a".to_string(),
                    target_node: "node-b".to_string(),
                }),
            )
            .await
            .expect("drain should succeed")
            .into_inner();

        assert!(resp.success, "drain should succeed: {}", resp.message);
        assert_eq!(resp.vms_migrated, 2);

        let node_a_vms = db.list_vms_for_node("node-a").expect("list vms node-a");
        assert!(
            node_a_vms.is_empty(),
            "node-a should have no VMs after drain"
        );

        let node_b_vms = db.list_vms_for_node("node-b").expect("list vms node-b");
        assert_eq!(node_b_vms.len(), 2, "node-b should have 2 VMs after drain");

        let pushed = pushed_nodes.lock().expect("lock");
        assert!(
            pushed.contains(&"node-a".to_string()),
            "should push config to drained node: {:?}",
            *pushed
        );
        assert!(
            pushed.contains(&"node-b".to_string()),
            "should push config to target node: {:?}",
            *pushed
        );

        let node_a_status = db
            .get_node("node-a")
            .expect("get node-a")
            .expect("node-a exists");
        assert_eq!(node_a_status.status, "drained");
    }
}
