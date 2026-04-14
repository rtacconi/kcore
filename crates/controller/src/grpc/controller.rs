use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tonic::{Request, Response, Status};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::auth::{self, CN_CONTROLLER_PREFIX, CN_KCTL, CN_NODE_PREFIX};
use crate::config::{NetworkConfig, ReplicationConfig};
use crate::controller_proto;
use crate::db::{
    Database, NetworkRow, NodeRow, SecurityGroupRow, SecurityGroupRuleRow, VmRow, WorkloadRow,
};
use crate::node_proto;
use crate::{nixgen, node_client::NodeClients, scheduler};
use std::collections::HashMap;

use super::helpers::compute_vni;
use super::helpers::{
    controller_state_from_node_state, parse_datetime_to_timestamp, parse_port_list,
    short_vm_id_seed, state_fallback_without_runtime, vm_backend_handle,
};
use super::signing;
use super::validation::{
    derive_image_format, derive_image_format_from_path, derive_local_image_path,
    normalize_image_format, normalize_storage_backend, storage_backend_to_proto,
    validate_image_path, validate_image_sha256, validate_image_url, validate_ipv4,
    validate_netmask, validate_network_name, validate_network_type, validate_storage_size_bytes,
};

#[cfg(test)]
type PushHook = std::sync::Arc<dyn Fn(&NodeRow) -> Result<(), Status> + Send + Sync + 'static>;
const EVT_NODE_REGISTER: &str = "node.register";
const EVT_NODE_HEARTBEAT: &str = "node.heartbeat";
const EVT_NODE_APPROVE: &str = "node.approve";
const EVT_NODE_REJECT: &str = "node.reject";
const EVT_VM_CREATE: &str = "vm.create";
const EVT_VM_UPDATE: &str = "vm.update";
const EVT_VM_DELETE: &str = "vm.delete";
const EVT_VM_DESIRED_STATE_SET: &str = "vm.desired_state.set";
const EVT_NETWORK_CREATE: &str = "network.create";
const EVT_NETWORK_DELETE: &str = "network.delete";
const EVT_SECURITY_GROUP_CREATE: &str = "security_group.create";
const EVT_SECURITY_GROUP_DELETE: &str = "security_group.delete";
const EVT_SECURITY_GROUP_ATTACH: &str = "security_group.attach";
const EVT_SECURITY_GROUP_DETACH: &str = "security_group.detach";
const EVT_NODE_DRAIN: &str = "node.drain";
const EVT_SSH_KEY_CREATE: &str = "ssh_key.create";
const EVT_SSH_KEY_DELETE: &str = "ssh_key.delete";

fn normalize_sg_protocol(protocol: &str) -> Result<String, Status> {
    let p = protocol.trim().to_ascii_lowercase();
    match p.as_str() {
        "tcp" | "udp" => Ok(p),
        _ => Err(Status::invalid_argument(
            "security group rule protocol must be tcp or udp",
        )),
    }
}

fn validate_port(port: i32, field: &str) -> Result<i32, Status> {
    if (1..=65535).contains(&port) {
        Ok(port)
    } else {
        Err(Status::invalid_argument(format!(
            "{field} must be in range 1-65535"
        )))
    }
}

fn parse_sg_target_kind(kind: i32) -> Result<controller_proto::SecurityGroupTargetKind, Status> {
    let parsed = controller_proto::SecurityGroupTargetKind::try_from(kind)
        .unwrap_or(controller_proto::SecurityGroupTargetKind::Unspecified);
    match parsed {
        controller_proto::SecurityGroupTargetKind::Vm
        | controller_proto::SecurityGroupTargetKind::Network => Ok(parsed),
        controller_proto::SecurityGroupTargetKind::Unspecified => Err(Status::invalid_argument(
            "target_kind must be vm or network",
        )),
    }
}

#[derive(Clone, Default)]
pub struct SubCaState {
    pub cert_pem: String,
    pub key_pem: String,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
}

impl SubCaState {
    pub fn is_available(&self) -> bool {
        !self.cert_pem.is_empty() && !self.key_pem.is_empty()
    }
}

#[derive(Clone, Default)]
pub struct TlsPaths {
    pub cert_file: String,
    pub key_file: String,
}

pub struct ControllerService {
    db: Database,
    clients: NodeClients,
    default_network: NetworkConfig,
    sub_ca: Arc<Mutex<SubCaState>>,
    replication: Option<ReplicationConfig>,
    tls_paths: Option<TlsPaths>,
    require_manual_approval: bool,
    #[cfg(test)]
    test_push_hook: Option<PushHook>,
}

impl ControllerService {
    fn reserve_nat_vm_ip_for_network(
        &self,
        node_id: &str,
        vm_network: &str,
        vm_id: &str,
        gateway_ip: &str,
    ) -> Result<String, Status> {
        let mut octets = gateway_ip.split('.');
        let o0 = octets
            .next()
            .ok_or_else(|| Status::internal(format!("invalid gateway IP '{}'", gateway_ip)))?;
        let o1 = octets
            .next()
            .ok_or_else(|| Status::internal(format!("invalid gateway IP '{}'", gateway_ip)))?;
        let o2 = octets
            .next()
            .ok_or_else(|| Status::internal(format!("invalid gateway IP '{}'", gateway_ip)))?;
        let _o3 = octets
            .next()
            .ok_or_else(|| Status::internal(format!("invalid gateway IP '{}'", gateway_ip)))?;
        let prefix = format!("{o0}.{o1}.{o2}");

        let existing = self
            .db
            .list_vms_for_node(node_id)
            .map_err(|e| Status::internal(format!("listing vms for IP reservation: {e}")))?;
        let mut used_hosts: HashSet<u16> = HashSet::new();
        for vm in existing {
            if vm.network != vm_network || vm.vm_ip.is_empty() {
                continue;
            }
            let mut vm_octets = vm.vm_ip.split('.');
            let Some(v0) = vm_octets.next() else {
                continue;
            };
            let Some(v1) = vm_octets.next() else {
                continue;
            };
            let Some(v2) = vm_octets.next() else {
                continue;
            };
            let Some(v3) = vm_octets.next() else {
                continue;
            };
            if format!("{v0}.{v1}.{v2}") != prefix {
                continue;
            }
            if let Ok(host) = v3.parse::<u16>() {
                used_hosts.insert(host);
            }
        }

        // Keep .1 for gateway and reserve lower addresses for infra.
        const MIN_HOST: u16 = 10;
        const MAX_HOST: u16 = 249;
        const POOL_SIZE: u16 = (MAX_HOST - MIN_HOST) + 1;

        let mut seed: u32 = 0;
        for b in vm_id.as_bytes() {
            seed = seed.wrapping_mul(131).wrapping_add(*b as u32);
        }
        let preferred_offset = (seed % POOL_SIZE as u32) as u16;

        for probe in 0..POOL_SIZE {
            let offset = (preferred_offset + probe) % POOL_SIZE;
            let host = MIN_HOST + offset;
            if !used_hosts.contains(&host) {
                return Ok(format!("{prefix}.{host}"));
            }
        }
        Err(Status::resource_exhausted(format!(
            "no free NAT reservation addresses for network '{}' on node '{}'",
            vm_network, node_id
        )))
    }

    pub fn new(
        db: Database,
        clients: NodeClients,
        default_network: NetworkConfig,
        sub_ca: Arc<Mutex<SubCaState>>,
        replication: Option<ReplicationConfig>,
        require_manual_approval: bool,
    ) -> Self {
        Self {
            db,
            clients,
            default_network,
            sub_ca,
            replication,
            tls_paths: None,
            require_manual_approval,
            #[cfg(test)]
            test_push_hook: None,
        }
    }

    pub fn with_tls_paths(mut self, paths: TlsPaths) -> Self {
        self.tls_paths = Some(paths);
        self
    }

    #[cfg(test)]
    pub fn new_with_test_push_hook(
        db: Database,
        clients: NodeClients,
        default_network: NetworkConfig,
        replication: Option<ReplicationConfig>,
        hook: PushHook,
    ) -> Self {
        Self {
            db,
            clients,
            default_network,
            sub_ca: Arc::new(Mutex::new(SubCaState::default())),
            replication,
            tls_paths: None,
            require_manual_approval: false,
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

        let node_ip = node.address.split(':').next().unwrap_or("").to_string();

        let mut vxlan_peers: std::collections::HashMap<String, nixgen::VxlanMeta> =
            std::collections::HashMap::new();
        for net in &networks {
            if net.network_type == "vxlan" {
                let all_with_name = self
                    .db
                    .list_networks_by_name(&net.name)
                    .map_err(|e| Status::internal(format!("listing vxlan peers: {e}")))?;
                let peers: Vec<String> = all_with_name
                    .iter()
                    .filter(|n| n.node_id != node.id)
                    .filter_map(|n| {
                        self.db
                            .get_node(&n.node_id)
                            .ok()
                            .flatten()
                            .map(|nd| nd.address.split(':').next().unwrap_or("").to_string())
                    })
                    .filter(|ip| !ip.is_empty())
                    .collect();
                vxlan_peers.insert(
                    net.name.clone(),
                    nixgen::VxlanMeta {
                        vni: net.vni,
                        peers,
                        local_ip: node_ip.clone(),
                    },
                );
            }
        }

        let mut security_group_rules: HashMap<String, Vec<nixgen::SecurityGroupResolvedRule>> =
            HashMap::new();
        for net in &networks {
            let mut effective_groups = self
                .db
                .list_security_groups_for_network(&net.name, &node.id)
                .map_err(|e| Status::internal(format!("listing network security groups: {e}")))?;
            for vm in vms.iter().filter(|v| v.network == net.name) {
                let vm_groups = self
                    .db
                    .list_security_groups_for_vm(&vm.id)
                    .map_err(|e| Status::internal(format!("listing vm security groups: {e}")))?;
                effective_groups.extend(vm_groups);
            }
            effective_groups.sort();
            effective_groups.dedup();
            let mut rules_for_net = Vec::new();
            for sg_name in effective_groups {
                let sg_rules = self
                    .db
                    .list_security_group_rules(&sg_name)
                    .map_err(|e| Status::internal(format!("listing security group rules: {e}")))?;
                for rule in sg_rules {
                    let target_ip = if !rule.target_vm.trim().is_empty() {
                        if let Some(target_vm) = self
                            .db
                            .get_vm(rule.target_vm.trim())
                            .map_err(|e| Status::internal(e.to_string()))?
                            .or_else(|| {
                                self.db.list_vms_for_node(&node.id).ok().and_then(|vv| {
                                    vv.into_iter().find(|v| v.name == rule.target_vm)
                                })
                            })
                        {
                            target_vm.vm_ip
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    };
                    rules_for_net.push(nixgen::SecurityGroupResolvedRule {
                        protocol: rule.protocol,
                        host_port: rule.host_port,
                        target_port: if rule.target_port <= 0 {
                            rule.host_port
                        } else {
                            rule.target_port
                        },
                        source_cidr: if rule.source_cidr.trim().is_empty() {
                            "0.0.0.0/0".to_string()
                        } else {
                            rule.source_cidr
                        },
                        target_ip,
                        enable_dnat: rule.enable_dnat,
                    });
                }
            }
            if !rules_for_net.is_empty() {
                security_group_rules.insert(net.name.clone(), rules_for_net);
            }
        }

        let nix_config = nixgen::generate_node_config_with_security_groups(
            &vms,
            iface,
            &self.default_network,
            &networks,
            &vm_ssh_keys,
            &vxlan_peers,
            &security_group_rules,
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

    /// Re-push config to all other nodes sharing a VXLAN network so their
    /// FDB peer lists stay current when a node joins or leaves the overlay.
    async fn refresh_vxlan_peers(&self, network_name: &str, skip_node_id: &str) {
        let peer_networks = match self.db.list_networks_by_name(network_name) {
            Ok(nets) => nets,
            Err(e) => {
                warn!(network = %network_name, error = %e, "failed to list vxlan peer networks");
                return;
            }
        };
        for peer_net in &peer_networks {
            if peer_net.node_id == skip_node_id {
                continue;
            }
            let peer_node = match self.db.get_node(&peer_net.node_id) {
                Ok(Some(n)) => n,
                _ => continue,
            };
            if let Err(e) = self.push_config_to_node(&peer_node).await {
                warn!(
                    node = %peer_node.id,
                    network = %network_name,
                    error = %e,
                    "failed to refresh vxlan peers on node"
                );
            } else {
                info!(
                    node = %peer_node.id,
                    network = %network_name,
                    "refreshed vxlan peer list"
                );
            }
        }
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

    fn preflight_vm_create_on_node(
        &self,
        node: &NodeRow,
        spec: &controller_proto::VmSpec,
        requested_storage_backend: &str,
    ) -> Result<(), Status> {
        let alternative_nodes = self.alternative_vm_create_nodes(
            &node.id,
            requested_storage_backend,
            spec.cpu,
            spec.memory_bytes,
        );
        let alternative_ids = alternative_nodes
            .into_iter()
            .map(|n| n.id)
            .take(3)
            .collect::<Vec<_>>();
        let hint = if alternative_ids.is_empty() {
            String::new()
        } else {
            format!("; try target_node one of: {}", alternative_ids.join(", "))
        };

        if node.approval_status != "approved" {
            return Err(Status::failed_precondition(format!(
                "node '{}' is not approved{}",
                node.id, hint
            )));
        }
        if node.status != "ready" {
            return Err(Status::unavailable(format!(
                "node '{}' is not ready{}",
                node.id, hint
            )));
        }

        let available_cpu = node.cpu_cores - node.cpu_used;
        let available_memory = node.memory_bytes - node.memory_used;
        if available_cpu < spec.cpu || available_memory < spec.memory_bytes {
            return Err(Status::unavailable(format!(
                "node '{}' lacks capacity for request (need cpu={} mem={}, available cpu={} mem={}){}",
                node.id, spec.cpu, spec.memory_bytes, available_cpu, available_memory, hint
            )));
        }
        Ok(())
    }

    fn alternative_vm_create_nodes(
        &self,
        exclude_node_id: &str,
        requested_storage_backend: &str,
        cpu: i32,
        memory_bytes: i64,
    ) -> Vec<NodeRow> {
        self.db
            .list_nodes()
            .ok()
            .unwrap_or_default()
            .into_iter()
            .filter(|n| {
                n.id != exclude_node_id
                    && n.storage_backend == requested_storage_backend
                    && n.approval_status == "approved"
                    && n.status == "ready"
                    && (n.cpu_cores - n.cpu_used) >= cpu
                    && (n.memory_bytes - n.memory_used) >= memory_bytes
            })
            .collect()
    }

    async fn set_vm_desired_state_internal(
        &self,
        vm_id: &str,
        target_node: &str,
        auto_start: bool,
    ) -> Result<i32, Status> {
        let node = self.resolve_node_for_vm(vm_id, target_node)?;
        let vm_name = self
            .db
            .get_vm(vm_id)
            .map_err(|e| Status::internal(format!("fetching vm: {e}")))?
            .map(|vm| vm.name)
            .or_else(|| {
                self.db
                    .list_vms_for_node(&node.id)
                    .ok()
                    .and_then(|rows| rows.into_iter().find(|vm| vm.name == vm_id))
                    .map(|vm| vm.name)
            })
            .ok_or_else(|| Status::not_found(format!("VM {vm_id} not found")))?;
        let updated = self
            .db
            .set_vm_auto_start(vm_id, auto_start)
            .map_err(|e| Status::internal(format!("updating vm desired state: {e}")))?;
        if !updated {
            return Err(Status::not_found(format!("VM {vm_id} not found")));
        }
        self.push_config_to_node(&node).await?;
        let desired_state = if auto_start {
            node_proto::VmDesiredState::Running as i32
        } else {
            node_proto::VmDesiredState::Stopped as i32
        };
        match self.ensure_compute_client_for_address(&node.address).await {
            Ok(mut compute) => {
                if let Err(e) = compute
                    .set_vm_desired_state(node_proto::SetVmDesiredStateRequest {
                        vm_id: vm_name,
                        desired_state,
                    })
                    .await
                {
                    warn!(
                        node_id = %node.id,
                        vm_id = %vm_id,
                        error = %e,
                        "failed to apply runtime desired state; declarative config already updated"
                    );
                }
            }
            Err(e) => {
                warn!(
                    node_id = %node.id,
                    vm_id = %vm_id,
                    error = %e,
                    "missing compute client for runtime desired state apply; declarative config already updated"
                );
            }
        }
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

    async fn ensure_container_client_for_address(
        &self,
        address: &str,
    ) -> Result<
        node_proto::node_container_client::NodeContainerClient<tonic::transport::Channel>,
        Status,
    > {
        if let Some(client) = self.clients.get_container(address) {
            return Ok(client);
        }
        self.clients
            .connect(address)
            .await
            .map_err(|e| Status::unavailable(format!("no connection to node {address}: {e}")))?;
        self.clients
            .get_container(address)
            .ok_or_else(|| Status::unavailable(format!("no connection to node {address}")))
    }

    fn log_replication_event(&self, event_type: &str, resource_key: &str, body: serde_json::Value) {
        let Some(rep) = &self.replication else {
            return;
        };
        let logical_ts_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let envelope = serde_json::json!({
            "schemaVersion": 1,
            "opId": Uuid::new_v4().to_string(),
            "logicalTsUnixMs": logical_ts_unix_ms,
            "controllerId": rep.controller_id,
            "dcId": rep.dc_id,
            "eventType": event_type,
            "resourceKey": resource_key,
            "body": body,
        });
        let payload = match serde_json::to_vec(&envelope) {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    error = %e,
                    event_type = %event_type,
                    resource_key = %resource_key,
                    "failed to serialize replication envelope"
                );
                return;
            }
        };
        if let Err(e) = self
            .db
            .append_replication_outbox(event_type, resource_key, &payload)
        {
            warn!(
                error = %e,
                event_type = %event_type,
                resource_key = %resource_key,
                "failed to append replication_outbox row"
            );
        }
    }

    fn log_replication_event_required(
        &self,
        event_type: &str,
        resource_key: &str,
        body: serde_json::Value,
    ) -> Result<(), Status> {
        let Some(rep) = &self.replication else {
            return Ok(());
        };
        let logical_ts_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let envelope = serde_json::json!({
            "schemaVersion": 1,
            "opId": Uuid::new_v4().to_string(),
            "logicalTsUnixMs": logical_ts_unix_ms,
            "controllerId": rep.controller_id,
            "dcId": rep.dc_id,
            "eventType": event_type,
            "resourceKey": resource_key,
            "body": body,
        });
        let payload = serde_json::to_vec(&envelope)
            .map_err(|e| Status::internal(format!("serialize replication envelope: {e}")))?;
        self.db
            .append_replication_outbox(event_type, resource_key, &payload)
            .map_err(|e| Status::internal(format!("append replication outbox row: {e}")))?;
        Ok(())
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

        let existing = self
            .db
            .get_node(&req.node_id)
            .map_err(|e| Status::internal(format!("checking node: {e}")))?;

        let approval_status = match &existing {
            Some(n) => n.approval_status.clone(),
            None => {
                if self.require_manual_approval {
                    "pending".to_string()
                } else {
                    "approved".to_string()
                }
            }
        };

        let dc_id = if req.dc_id.trim().is_empty() {
            existing
                .as_ref()
                .map(|n| n.dc_id.clone())
                .unwrap_or_default()
        } else {
            req.dc_id.clone()
        };

        let node = NodeRow {
            id: req.node_id.clone(),
            hostname: req.hostname.clone(),
            address: req.address.clone(),
            cpu_cores: cpu,
            memory_bytes: mem,
            status: if approval_status == "approved" {
                "ready".into()
            } else {
                "pending".into()
            },
            last_heartbeat: String::new(),
            gateway_interface: String::new(),
            cpu_used: 0,
            memory_used: 0,
            storage_backend,
            disable_vxlan: req.disable_vxlan,
            approval_status: approval_status.clone(),
            cert_expiry_days: req.cert_expiry_days,
            luks_method: req.luks_method.clone(),
            dc_id: dc_id.clone(),
        };

        self.db
            .upsert_node(&node)
            .map_err(|e| Status::internal(format!("storing node: {e}")))?;

        if !req.labels.is_empty() {
            self.db
                .upsert_node_labels(&req.node_id, &req.labels)
                .map_err(|e| Status::internal(format!("storing labels: {e}")))?;
        }

        self.log_replication_event_required(
            EVT_NODE_REGISTER,
            &format!("node/{}", req.node_id),
            serde_json::json!({
                "nodeId": req.node_id,
                "hostname": req.hostname,
                "address": req.address,
                "cpuCores": req.capacity.as_ref().map(|c| c.cpu_cores).unwrap_or(0),
                "memoryBytes": req.capacity.as_ref().map(|c| c.memory_bytes).unwrap_or(0),
                "status": if approval_status == "approved" { "ready" } else { "pending" },
                "gatewayInterface": "",
                "storageBackend": req.storage_backend,
                "disableVxlan": req.disable_vxlan,
                "certExpiryDays": req.cert_expiry_days,
                "approvalStatus": approval_status,
                "labels": req.labels,
                "luksMethod": req.luks_method,
                "dcId": dc_id,
            }),
        )?;

        if approval_status == "approved" {
            if let Err(e) = self.clients.connect(&req.address).await {
                warn!(address = %req.address, error = %e, "failed to connect to node");
            }
            info!(node_id = %req.node_id, address = %req.address, "registered node (approved)");
        } else {
            info!(node_id = %req.node_id, address = %req.address, approval_status = %approval_status, "node registered with pending approval");
        }

        let message = if approval_status == "approved" {
            "registered".to_string()
        } else {
            format!("registered (approval status: {approval_status})")
        };

        Ok(Response::new(controller_proto::RegisterNodeResponse {
            success: true,
            message,
            approval_status,
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
            .update_heartbeat(
                &req.node_id,
                cpu_used,
                mem_used,
                req.cert_expiry_days,
                &req.luks_method,
            )
            .map_err(|e| Status::internal(e.to_string()))?;

        if !found {
            return Err(Status::not_found(format!(
                "node {} not registered",
                req.node_id
            )));
        }

        if let Ok(Some(node)) = self.db.get_node(&req.node_id) {
            let labels = self.db.get_node_labels(&req.node_id).unwrap_or_default();
            self.log_replication_event_required(
                EVT_NODE_HEARTBEAT,
                &format!("node/{}", req.node_id),
                serde_json::json!({
                    "nodeId": node.id,
                    "hostname": node.hostname,
                    "address": node.address,
                    "cpuCores": node.cpu_cores,
                    "memoryBytes": node.memory_bytes,
                    "status": node.status,
                    "gatewayInterface": node.gateway_interface,
                    "storageBackend": match node.storage_backend.as_str() {
                        "lvm" => controller_proto::StorageBackendType::Lvm as i32,
                        "zfs" => controller_proto::StorageBackendType::Zfs as i32,
                        _ => controller_proto::StorageBackendType::Filesystem as i32,
                    },
                    "disableVxlan": node.disable_vxlan,
                    "certExpiryDays": req.cert_expiry_days,
                    "approvalStatus": node.approval_status,
                    "labels": labels,
                    "luksMethod": req.luks_method,
                    "cpuUsed": cpu_used,
                    "memoryUsed": mem_used,
                    "lastHeartbeat": node.last_heartbeat,
                }),
            )?;
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

    async fn sync_workload_state(
        &self,
        request: Request<controller_proto::SyncWorkloadStateRequest>,
    ) -> Result<Response<controller_proto::SyncWorkloadStateResponse>, Status> {
        auth::require_peer(&request, &[CN_NODE_PREFIX])?;
        let req = request.into_inner();
        for workload in &req.workloads {
            let state = match controller_proto::WorkloadKind::try_from(workload.kind)
                .unwrap_or(controller_proto::WorkloadKind::Unspecified)
            {
                controller_proto::WorkloadKind::Vm => {
                    match controller_proto::VmState::try_from(workload.vm_state)
                        .unwrap_or(controller_proto::VmState::Unknown)
                    {
                        controller_proto::VmState::Running => "running",
                        controller_proto::VmState::Stopped => "stopped",
                        controller_proto::VmState::Paused => "paused",
                        controller_proto::VmState::Error => "error",
                        controller_proto::VmState::Unknown => "unknown",
                    }
                }
                controller_proto::WorkloadKind::Container => {
                    match controller_proto::ContainerState::try_from(workload.container_state)
                        .unwrap_or(controller_proto::ContainerState::Unknown)
                    {
                        controller_proto::ContainerState::Created => "created",
                        controller_proto::ContainerState::Running => "running",
                        controller_proto::ContainerState::Stopped => "stopped",
                        controller_proto::ContainerState::Error => "error",
                        controller_proto::ContainerState::Unknown => "unknown",
                    }
                }
                controller_proto::WorkloadKind::Unspecified => "unknown",
            };
            let _ = self.db.update_workload_runtime_state(&workload.id, state);
        }
        Ok(Response::new(controller_proto::SyncWorkloadStateResponse {
            success: true,
        }))
    }

    async fn create_vm(
        &self,
        request: Request<controller_proto::CreateVmRequest>,
    ) -> Result<Response<controller_proto::CreateVmResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let spec = req
            .spec
            .ok_or_else(|| Status::invalid_argument("spec is required"))?;

        let requested_storage_backend = normalize_storage_backend(req.storage_backend, true)?;
        let requested_storage_size_bytes = validate_storage_size_bytes(req.storage_size_bytes)?;

        let target_node_requested = !req.target_node.is_empty();
        let mut node = if target_node_requested {
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
            let target_dc = req.target_dc.trim();
            if target_dc.is_empty() {
                scheduler::select_node_for_vm(&compatible_nodes, spec.cpu, spec.memory_bytes)
            } else {
                scheduler::select_node_for_vm_in_dc(
                    &compatible_nodes,
                    spec.cpu,
                    spec.memory_bytes,
                    target_dc,
                )
            }
            .cloned()
            .ok_or_else(|| {
                if target_dc.is_empty() {
                    Status::unavailable(
                        "no ready node with sufficient capacity matching requested storage backend",
                    )
                } else {
                    Status::unavailable(format!(
                        "no ready node in DC '{}' with sufficient capacity matching requested storage backend",
                        target_dc,
                    ))
                }
            })?
        };
        if target_node_requested {
            let preflight_error = if node.storage_backend != requested_storage_backend {
                Some(Status::failed_precondition(format!(
                    "VM storage backend '{}' does not match node '{}' backend '{}'",
                    requested_storage_backend, node.id, node.storage_backend
                )))
            } else {
                self.preflight_vm_create_on_node(&node, &spec, &requested_storage_backend)
                    .err()
            };
            if let Some(err) = preflight_error {
                if let Some(fallback) = scheduler::select_node_for_vm(
                    &self.alternative_vm_create_nodes(
                        &node.id,
                        &requested_storage_backend,
                        spec.cpu,
                        spec.memory_bytes,
                    ),
                    spec.cpu,
                    spec.memory_bytes,
                )
                .cloned()
                {
                    warn!(
                        vm_name = %spec.name,
                        requested_node = %node.id,
                        fallback_node = %fallback.id,
                        reason = %err.message(),
                        "target node failed preflight; auto-falling back to alternative node"
                    );
                    node = fallback;
                } else {
                    return Err(err);
                }
            }
        } else if node.storage_backend != requested_storage_backend {
            return Err(Status::failed_precondition(format!(
                "VM storage backend '{}' does not match node '{}' backend '{}'",
                requested_storage_backend, node.id, node.storage_backend
            )));
        }
        self.preflight_vm_create_on_node(&node, &spec, &requested_storage_backend)?;

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

        let vm_ip = if vm_network == "default" {
            self.reserve_nat_vm_ip_for_network(
                &node.id,
                &vm_network,
                &vm_id,
                &self.default_network.gateway_ip,
            )?
        } else if let Some(net) = self
            .db
            .get_network_for_node(&node.id, &vm_network)
            .map_err(|e| Status::internal(format!("fetching network: {e}")))?
        {
            match net.network_type.as_str() {
                "vxlan" => self
                    .db
                    .allocate_vm_ip_global(&vm_network)
                    .map_err(|e| Status::internal(format!("allocating VM IP: {e}")))?,
                "nat" => self.reserve_nat_vm_ip_for_network(
                    &node.id,
                    &vm_network,
                    &vm_id,
                    &net.gateway_ip,
                )?,
                _ => String::new(),
            }
        } else {
            String::new()
        };

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
            vm_ip,
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

        let ssh_key_names: Vec<String> = self
            .db
            .get_vm_ssh_key_names(&vm_id)
            .map_err(|e| Status::internal(format!("listing VM SSH keys for replication: {e}")))?;

        self.log_replication_event(
            EVT_VM_CREATE,
            &format!("vm/{vm_id}"),
            serde_json::json!({
                "vmId": vm_id,
                "nodeId": node.id,
                "name": vm.name,
                "cpu": vm.cpu,
                "memoryBytes": vm.memory_bytes,
                "imagePath": vm.image_path,
                "imageUrl": vm.image_url,
                "imageSha256": vm.image_sha256,
                "imageFormat": vm.image_format,
                "imageSize": vm.image_size,
                "network": vm.network,
                "autoStart": vm.auto_start,
                "runtimeState": vm.runtime_state,
                "cloudInitUserData": vm.cloud_init_user_data,
                "storageBackend": vm.storage_backend,
                "storageSizeBytes": vm.storage_size_bytes,
                "vmIp": vm.vm_ip,
                "sshKeyNames": ssh_key_names,
            }),
        );

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
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
        self.log_replication_event(
            EVT_VM_UPDATE,
            &format!("vm/{}", req.vm_id),
            serde_json::json!({
                "vmId": req.vm_id,
                "nodeId": node.id,
                "cpu": cpu,
                "memoryBytes": mem,
            }),
        );

        Ok(Response::new(controller_proto::UpdateVmResponse {
            success: true,
            message: format!("VM '{}' updated", req.vm_id),
        }))
    }

    async fn delete_vm(
        &self,
        request: Request<controller_proto::DeleteVmRequest>,
    ) -> Result<Response<controller_proto::DeleteVmResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
        self.log_replication_event(
            EVT_VM_DELETE,
            &format!("vm/{}", req.vm_id),
            serde_json::json!({
                "vmId": req.vm_id,
                "nodeId": node.id,
            }),
        );

        Ok(Response::new(controller_proto::DeleteVmResponse {
            success: true,
        }))
    }

    async fn set_vm_desired_state(
        &self,
        request: Request<controller_proto::SetVmDesiredStateRequest>,
    ) -> Result<Response<controller_proto::SetVmDesiredStateResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
        self.log_replication_event(
            EVT_VM_DESIRED_STATE_SET,
            &format!("vm/{}", req.vm_id),
            serde_json::json!({
                "vmId": req.vm_id,
                "targetNode": req.target_node,
                "autoStart": auto_start,
            }),
        );

        Ok(Response::new(controller_proto::SetVmDesiredStateResponse {
            state,
        }))
    }

    async fn get_vm(
        &self,
        request: Request<controller_proto::GetVmRequest>,
    ) -> Result<Response<controller_proto::GetVmResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
                        backend_handle: vm_backend_handle(&db_vm),
                        bus: String::new(),
                        device: String::new(),
                    }],
                    nics: vec![controller_proto::Nic {
                        network: db_vm.network.clone(),
                        model: "virtio".to_string(),
                        mac_address: String::new(),
                    }],
                    storage_backend: db_vm.storage_backend.clone(),
                    storage_size_bytes: db_vm.storage_size_bytes,
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
                    assigned_ip: db_vm.vm_ip.clone(),
                }));
            }
        };

        let spec = inner.spec.map(|s| {
            let mut disks: Vec<controller_proto::Disk> = s
                .disks
                .into_iter()
                .map(|d| controller_proto::Disk {
                    name: d.name,
                    backend_handle: d.backend_handle,
                    bus: d.bus,
                    device: d.device,
                })
                .collect();
            if db_vm.storage_backend == "lvm" || db_vm.storage_backend == "zfs" {
                let block_path = vm_backend_handle(&db_vm);
                let single = disks.len() == 1;
                for d in &mut disks {
                    if d.name == "boot" || single {
                        d.backend_handle.clone_from(&block_path);
                    }
                }
            }
            if disks.is_empty() {
                disks.push(controller_proto::Disk {
                    name: "boot".to_string(),
                    backend_handle: vm_backend_handle(&db_vm),
                    bus: String::new(),
                    device: String::new(),
                });
            }

            let mut nics: Vec<controller_proto::Nic> = s
                .nics
                .into_iter()
                .map(|n| controller_proto::Nic {
                    network: {
                        let raw = n.network.trim().to_string();
                        if raw.is_empty()
                            || raw.starts_with("tap-")
                            || raw.starts_with("veth")
                            || raw.contains("[kcore-net:")
                        {
                            db_vm.network.clone()
                        } else {
                            raw
                        }
                    },
                    model: n.model,
                    mac_address: n.mac_address,
                })
                .collect();
            if nics.is_empty() || nics.iter().all(|n| n.network.trim().is_empty()) {
                nics = vec![controller_proto::Nic {
                    network: db_vm.network.clone(),
                    model: "virtio".to_string(),
                    mac_address: nics
                        .first()
                        .map(|n| n.mac_address.clone())
                        .unwrap_or_default(),
                }];
            }

            controller_proto::VmSpec {
                id: if s.id.is_empty() {
                    db_vm.id.clone()
                } else {
                    s.id
                },
                name: if s.name.is_empty() {
                    db_vm.name.clone()
                } else {
                    s.name
                },
                cpu: if s.cpu == 0 { db_vm.cpu } else { s.cpu },
                memory_bytes: if s.memory_bytes == 0 {
                    db_vm.memory_bytes
                } else {
                    s.memory_bytes
                },
                disks,
                nics,
                storage_backend: db_vm.storage_backend.clone(),
                storage_size_bytes: db_vm.storage_size_bytes,
            }
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
            assigned_ip: db_vm.vm_ip,
        }))
    }

    async fn list_vms(
        &self,
        request: Request<controller_proto::ListVmsRequest>,
    ) -> Result<Response<controller_proto::ListVmsResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
                    storage_backend: vm.storage_backend,
                    storage_size_bytes: vm.storage_size_bytes,
                }
            })
            .collect();

        Ok(Response::new(controller_proto::ListVmsResponse {
            vms: infos,
        }))
    }

    async fn create_workload(
        &self,
        request: Request<controller_proto::CreateWorkloadRequest>,
    ) -> Result<Response<controller_proto::CreateWorkloadResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let kind = controller_proto::WorkloadKind::try_from(req.kind)
            .unwrap_or(controller_proto::WorkloadKind::Unspecified);
        match kind {
            controller_proto::WorkloadKind::Vm => {
                let vm_spec = req.vm_spec.ok_or_else(|| {
                    Status::invalid_argument("vm_spec is required for VM workload")
                })?;
                let vm_resp = self
                    .create_vm(Request::new(controller_proto::CreateVmRequest {
                        target_node: req.target_node.clone(),
                        spec: Some(vm_spec),
                        image_url: req.image_url.clone(),
                        image_sha256: req.image_sha256.clone(),
                        cloud_init_user_data: req.cloud_init_user_data.clone(),
                        image_path: req.image_path.clone(),
                        image_format: req.image_format.clone(),
                        ssh_key_names: req.ssh_key_names.clone(),
                        storage_backend: req.storage_backend,
                        storage_size_bytes: req.storage_size_bytes,
                        target_dc: String::new(),
                    }))
                    .await?
                    .into_inner();

                if let Some(vm_row) = self.db.get_vm(&vm_resp.vm_id).map_err(|e| {
                    Status::internal(format!("fetching vm after create workload: {e}"))
                })? {
                    let _ = self.db.upsert_workload(&WorkloadRow {
                        id: vm_row.id.clone(),
                        name: vm_row.name.clone(),
                        kind: "vm".to_string(),
                        node_id: vm_row.node_id.clone(),
                        runtime_state: vm_row.runtime_state.clone(),
                        desired_state: if vm_row.auto_start {
                            "running".to_string()
                        } else {
                            "stopped".to_string()
                        },
                        vm_id: vm_row.id.clone(),
                        container_image: String::new(),
                        network: vm_row.network.clone(),
                        storage_backend: vm_row.storage_backend.clone(),
                        storage_size_bytes: vm_row.storage_size_bytes,
                        created_at: String::new(),
                    });
                }

                Ok(Response::new(controller_proto::CreateWorkloadResponse {
                    kind: controller_proto::WorkloadKind::Vm as i32,
                    workload_id: vm_resp.vm_id,
                    node_id: vm_resp.node_id,
                    vm_state: vm_resp.state,
                    container_state: controller_proto::ContainerState::Unknown as i32,
                }))
            }
            controller_proto::WorkloadKind::Container => {
                let spec = req.container_spec.ok_or_else(|| {
                    Status::invalid_argument("container_spec is required for container workload")
                })?;
                if spec.name.trim().is_empty() || spec.image.trim().is_empty() {
                    return Err(Status::invalid_argument(
                        "container_spec.name and container_spec.image are required",
                    ));
                }
                let node = if !req.target_node.is_empty() {
                    self.db
                        .get_node_by_address(&req.target_node)
                        .map_err(|e| Status::internal(e.to_string()))?
                        .or_else(|| self.db.get_node(&req.target_node).ok().flatten())
                        .ok_or_else(|| {
                            Status::not_found(format!("node {} not found", req.target_node))
                        })?
                } else {
                    let nodes = self
                        .db
                        .list_nodes()
                        .map_err(|e| Status::internal(e.to_string()))?;
                    scheduler::select_node(&nodes)
                        .cloned()
                        .ok_or_else(|| Status::unavailable("no ready nodes"))?
                };

                let mut container = self
                    .ensure_container_client_for_address(&node.address)
                    .await?;
                let created = container
                    .create_container(node_proto::CreateContainerRequest {
                        spec: Some(node_proto::ContainerSpec {
                            name: spec.name.clone(),
                            image: spec.image.clone(),
                            network: spec.network.clone(),
                            command: spec.command.clone(),
                            env: spec.env.clone(),
                            ports: spec.ports.clone(),
                            storage_backend: spec.storage_backend.clone(),
                            storage_size_bytes: spec.storage_size_bytes,
                            mount_target: spec.mount_target.clone(),
                        }),
                    })
                    .await
                    .map_err(|e| Status::internal(format!("creating container on node: {e}")))?
                    .into_inner()
                    .container
                    .ok_or_else(|| Status::internal("missing container response from node"))?;

                let workload_id = if created.id.is_empty() {
                    format!("ctr-{}", Uuid::new_v4())
                } else {
                    created.id.clone()
                };
                self.db
                    .upsert_workload(&WorkloadRow {
                        id: workload_id.clone(),
                        name: created.name.clone(),
                        kind: "container".to_string(),
                        node_id: node.id.clone(),
                        runtime_state: "running".to_string(),
                        desired_state: "running".to_string(),
                        vm_id: String::new(),
                        container_image: created.image.clone(),
                        network: spec.network.clone(),
                        storage_backend: normalize_storage_backend(req.storage_backend, false)?,
                        storage_size_bytes: req.storage_size_bytes.max(0),
                        created_at: String::new(),
                    })
                    .map_err(|e| Status::internal(format!("storing workload row: {e}")))?;

                Ok(Response::new(controller_proto::CreateWorkloadResponse {
                    kind: controller_proto::WorkloadKind::Container as i32,
                    workload_id,
                    node_id: node.id,
                    vm_state: controller_proto::VmState::Unknown as i32,
                    container_state: created.state,
                }))
            }
            controller_proto::WorkloadKind::Unspecified => {
                Err(Status::invalid_argument("kind is required"))
            }
        }
    }

    async fn delete_workload(
        &self,
        request: Request<controller_proto::DeleteWorkloadRequest>,
    ) -> Result<Response<controller_proto::DeleteWorkloadResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let kind = controller_proto::WorkloadKind::try_from(req.kind)
            .unwrap_or(controller_proto::WorkloadKind::Unspecified);
        match kind {
            controller_proto::WorkloadKind::Vm => {
                let _ = self
                    .delete_vm(Request::new(controller_proto::DeleteVmRequest {
                        vm_id: req.workload_id.clone(),
                        target_node: req.target_node.clone(),
                    }))
                    .await?;
                let _ = self.db.delete_workload_by_id_or_name(&req.workload_id);
                Ok(Response::new(controller_proto::DeleteWorkloadResponse {
                    success: true,
                }))
            }
            controller_proto::WorkloadKind::Container => {
                let wl = self
                    .db
                    .get_workload(&req.workload_id)
                    .map_err(|e| Status::internal(format!("fetching workload: {e}")))?;
                let node = if let Some(wl) = &wl {
                    self.db
                        .get_node(&wl.node_id)
                        .map_err(|e| Status::internal(e.to_string()))?
                        .ok_or_else(|| {
                            Status::not_found(format!("node {} not found", wl.node_id))
                        })?
                } else if !req.target_node.is_empty() {
                    self.db
                        .get_node_by_address(&req.target_node)
                        .map_err(|e| Status::internal(e.to_string()))?
                        .or_else(|| self.db.get_node(&req.target_node).ok().flatten())
                        .ok_or_else(|| {
                            Status::not_found(format!("node {} not found", req.target_node))
                        })?
                } else {
                    return Err(Status::not_found(format!(
                        "workload {} not found",
                        req.workload_id
                    )));
                };
                let name = wl
                    .as_ref()
                    .map(|w| w.name.clone())
                    .unwrap_or_else(|| req.workload_id.clone());
                let mut container = self
                    .ensure_container_client_for_address(&node.address)
                    .await?;
                let _ = container
                    .delete_container(node_proto::DeleteContainerRequest { name, force: true })
                    .await
                    .map_err(|e| Status::internal(format!("deleting container on node: {e}")))?;
                let _ = self.db.delete_workload_by_id_or_name(&req.workload_id);
                Ok(Response::new(controller_proto::DeleteWorkloadResponse {
                    success: true,
                }))
            }
            controller_proto::WorkloadKind::Unspecified => {
                Err(Status::invalid_argument("kind is required"))
            }
        }
    }

    async fn set_workload_desired_state(
        &self,
        request: Request<controller_proto::SetWorkloadDesiredStateRequest>,
    ) -> Result<Response<controller_proto::SetWorkloadDesiredStateResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let desired = controller_proto::WorkloadDesiredState::try_from(req.desired_state)
            .unwrap_or(controller_proto::WorkloadDesiredState::Unspecified);
        let kind = controller_proto::WorkloadKind::try_from(req.kind)
            .unwrap_or(controller_proto::WorkloadKind::Unspecified);
        match kind {
            controller_proto::WorkloadKind::Vm => {
                let vm_desired = match desired {
                    controller_proto::WorkloadDesiredState::Running => {
                        controller_proto::VmDesiredState::Running as i32
                    }
                    controller_proto::WorkloadDesiredState::Stopped => {
                        controller_proto::VmDesiredState::Stopped as i32
                    }
                    controller_proto::WorkloadDesiredState::Unspecified => {
                        return Err(Status::invalid_argument("desired_state is required"));
                    }
                };
                let resp = self
                    .set_vm_desired_state(Request::new(
                        controller_proto::SetVmDesiredStateRequest {
                            vm_id: req.workload_id.clone(),
                            desired_state: vm_desired,
                            target_node: req.target_node.clone(),
                        },
                    ))
                    .await?
                    .into_inner();
                let _ = self.db.update_workload_desired_state(
                    &req.workload_id,
                    if desired == controller_proto::WorkloadDesiredState::Running {
                        "running"
                    } else {
                        "stopped"
                    },
                );
                Ok(Response::new(
                    controller_proto::SetWorkloadDesiredStateResponse {
                        kind: controller_proto::WorkloadKind::Vm as i32,
                        vm_state: resp.state,
                        container_state: controller_proto::ContainerState::Unknown as i32,
                    },
                ))
            }
            controller_proto::WorkloadKind::Container => {
                let wl = self
                    .db
                    .get_workload(&req.workload_id)
                    .map_err(|e| Status::internal(format!("fetching workload: {e}")))?
                    .ok_or_else(|| {
                        Status::not_found(format!("workload {} not found", req.workload_id))
                    })?;
                let node = self
                    .db
                    .get_node(&wl.node_id)
                    .map_err(|e| Status::internal(e.to_string()))?
                    .ok_or_else(|| Status::not_found(format!("node {} not found", wl.node_id)))?;
                let mut container = self
                    .ensure_container_client_for_address(&node.address)
                    .await?;
                let state = match desired {
                    controller_proto::WorkloadDesiredState::Running => {
                        let resp = container
                            .start_container(node_proto::StartContainerRequest {
                                name: wl.name.clone(),
                            })
                            .await
                            .map_err(|e| Status::internal(format!("starting container: {e}")))?
                            .into_inner();
                        let _ = self.db.update_workload_runtime_state(&wl.id, "running");
                        let _ = self.db.update_workload_desired_state(&wl.id, "running");
                        resp.container
                            .map(|c| c.state)
                            .unwrap_or(controller_proto::ContainerState::Running as i32)
                    }
                    controller_proto::WorkloadDesiredState::Stopped => {
                        let resp = container
                            .stop_container(node_proto::StopContainerRequest {
                                name: wl.name.clone(),
                            })
                            .await
                            .map_err(|e| Status::internal(format!("stopping container: {e}")))?
                            .into_inner();
                        let _ = self.db.update_workload_runtime_state(&wl.id, "stopped");
                        let _ = self.db.update_workload_desired_state(&wl.id, "stopped");
                        resp.container
                            .map(|c| c.state)
                            .unwrap_or(controller_proto::ContainerState::Stopped as i32)
                    }
                    controller_proto::WorkloadDesiredState::Unspecified => {
                        return Err(Status::invalid_argument("desired_state is required"));
                    }
                };
                Ok(Response::new(
                    controller_proto::SetWorkloadDesiredStateResponse {
                        kind: controller_proto::WorkloadKind::Container as i32,
                        vm_state: controller_proto::VmState::Unknown as i32,
                        container_state: state,
                    },
                ))
            }
            controller_proto::WorkloadKind::Unspecified => {
                Err(Status::invalid_argument("kind is required"))
            }
        }
    }

    async fn get_workload(
        &self,
        request: Request<controller_proto::GetWorkloadRequest>,
    ) -> Result<Response<controller_proto::GetWorkloadResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let kind = controller_proto::WorkloadKind::try_from(req.kind)
            .unwrap_or(controller_proto::WorkloadKind::Unspecified);
        match kind {
            controller_proto::WorkloadKind::Vm => {
                let vm = self
                    .get_vm(Request::new(controller_proto::GetVmRequest {
                        vm_id: req.workload_id,
                        target_node: req.target_node,
                    }))
                    .await?
                    .into_inner();
                Ok(Response::new(controller_proto::GetWorkloadResponse {
                    kind: controller_proto::WorkloadKind::Vm as i32,
                    vm_spec: vm.spec,
                    container_spec: None,
                    vm_status: vm.status,
                    container_info: None,
                    node_id: vm.node_id,
                    assigned_ip: vm.assigned_ip,
                }))
            }
            controller_proto::WorkloadKind::Container => {
                let wl = self
                    .db
                    .get_workload(&req.workload_id)
                    .map_err(|e| Status::internal(format!("fetching workload: {e}")))?
                    .ok_or_else(|| {
                        Status::not_found(format!("workload {} not found", req.workload_id))
                    })?;
                let node = self
                    .db
                    .get_node(&wl.node_id)
                    .map_err(|e| Status::internal(e.to_string()))?
                    .ok_or_else(|| Status::not_found(format!("node {} not found", wl.node_id)))?;
                let mut container = self
                    .ensure_container_client_for_address(&node.address)
                    .await?;
                let info = container
                    .get_container(node_proto::GetContainerRequest {
                        name: wl.name.clone(),
                    })
                    .await
                    .map_err(|e| Status::internal(format!("getting container: {e}")))?
                    .into_inner()
                    .container;
                let c = info.ok_or_else(|| Status::not_found("container not found on node"))?;
                Ok(Response::new(controller_proto::GetWorkloadResponse {
                    kind: controller_proto::WorkloadKind::Container as i32,
                    vm_spec: None,
                    container_spec: Some(controller_proto::ContainerSpec {
                        name: c.name.clone(),
                        image: c.image.clone(),
                        network: wl.network,
                        command: Vec::new(),
                        env: std::collections::HashMap::new(),
                        ports: Vec::new(),
                        storage_backend: wl.storage_backend.clone(),
                        storage_size_bytes: wl.storage_size_bytes,
                        mount_target: "/data".to_string(),
                    }),
                    vm_status: None,
                    container_info: Some(controller_proto::ContainerInfo {
                        id: c.id,
                        name: c.name,
                        image: c.image,
                        state: c.state,
                        status: c.status,
                        node_id: node.id.clone(),
                        created_at: None,
                    }),
                    node_id: node.id.clone(),
                    assigned_ip: String::new(),
                }))
            }
            controller_proto::WorkloadKind::Unspecified => {
                Err(Status::invalid_argument("kind is required"))
            }
        }
    }

    async fn list_workloads(
        &self,
        request: Request<controller_proto::ListWorkloadsRequest>,
    ) -> Result<Response<controller_proto::ListWorkloadsResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let kind = controller_proto::WorkloadKind::try_from(req.kind)
            .unwrap_or(controller_proto::WorkloadKind::Unspecified);

        let mut vms = Vec::new();
        let mut containers = Vec::new();

        if kind == controller_proto::WorkloadKind::Unspecified
            || kind == controller_proto::WorkloadKind::Vm
        {
            vms = self
                .list_vms(Request::new(controller_proto::ListVmsRequest {
                    target_node: req.target_node.clone(),
                }))
                .await?
                .into_inner()
                .vms;
        }

        if kind == controller_proto::WorkloadKind::Unspecified
            || kind == controller_proto::WorkloadKind::Container
        {
            let node_filter = if req.target_node.trim().is_empty() {
                None
            } else {
                self.db
                    .get_node_by_address(&req.target_node)
                    .map_err(|e| Status::internal(e.to_string()))?
                    .or_else(|| self.db.get_node(&req.target_node).ok().flatten())
                    .map(|n| n.id)
            };
            let rows = self
                .db
                .list_workloads(Some("container"), node_filter.as_deref())
                .map_err(|e| Status::internal(format!("listing container workloads: {e}")))?;
            containers = rows
                .into_iter()
                .map(|w| controller_proto::ContainerInfo {
                    id: w.id,
                    name: w.name,
                    image: w.container_image,
                    state: match w.runtime_state.as_str() {
                        "running" => controller_proto::ContainerState::Running as i32,
                        "stopped" => controller_proto::ContainerState::Stopped as i32,
                        "created" => controller_proto::ContainerState::Created as i32,
                        "error" => controller_proto::ContainerState::Error as i32,
                        _ => controller_proto::ContainerState::Unknown as i32,
                    },
                    status: w.runtime_state,
                    node_id: w.node_id,
                    created_at: parse_datetime_to_timestamp(&w.created_at),
                })
                .collect();
        }

        Ok(Response::new(controller_proto::ListWorkloadsResponse {
            vms,
            containers,
        }))
    }

    async fn create_network(
        &self,
        request: Request<controller_proto::CreateNetworkRequest>,
    ) -> Result<Response<controller_proto::CreateNetworkResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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

        let network_type = validate_network_type(&req.network_type)?;

        if network_type == "bridge" && req.vlan_id == 0 {
            return Err(Status::failed_precondition(
                "bridge-mode networks without a VLAN ID enslave the management NIC, \
                 which severs host connectivity. Use --vlan-id to create a VLAN \
                 sub-interface, or use 'nat' or 'vxlan' network types instead."
                    .to_string(),
            ));
        }

        if network_type == "vxlan" && node.disable_vxlan {
            return Err(Status::failed_precondition(format!(
                "VXLAN is disabled on node '{}'; cannot create vxlan network",
                node.id
            )));
        }

        let enable_outbound_nat = match network_type.as_str() {
            "bridge" => false,
            "nat" => true,
            "vxlan" => req.enable_outbound_nat,
            _ => true,
        };

        let vni = if network_type == "vxlan" {
            compute_vni(&name)
        } else {
            0
        };

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
                network_type: network_type.clone(),
                enable_outbound_nat,
                vni,
                next_ip: 2,
            })
            .map_err(|e| Status::internal(format!("storing network: {e}")))?;

        self.push_config_to_node(&node).await?;

        if network_type == "vxlan" {
            self.refresh_vxlan_peers(&name, &node.id).await;
        }

        self.log_replication_event(
            EVT_NETWORK_CREATE,
            &format!("network/{}/{}", node.id, name),
            serde_json::json!({
                "name": name,
                "nodeId": node.id,
                "externalIp": req.external_ip,
                "gatewayIp": req.gateway_ip,
                "internalNetmask": req.internal_netmask,
                "allowedTcpPorts": req.allowed_tcp_ports,
                "allowedUdpPorts": req.allowed_udp_ports,
                "networkType": network_type,
                "vlanId": req.vlan_id,
                "enableOutboundNat": enable_outbound_nat,
                "vni": vni,
                "nextIp": 2,
            }),
        );

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
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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

        let was_vxlan = self
            .db
            .get_network_for_node(&node.id, name)
            .map_err(|e| Status::internal(format!("reading network: {e}")))?
            .map(|n| n.network_type == "vxlan")
            .unwrap_or(false);

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

        if was_vxlan {
            self.refresh_vxlan_peers(name, &node.id).await;
        }

        self.log_replication_event(
            EVT_NETWORK_DELETE,
            &format!("network/{}/{}", node.id, name),
            serde_json::json!({
                "name": name,
                "nodeId": node.id,
            }),
        );
        Ok(Response::new(controller_proto::DeleteNetworkResponse {
            success: true,
        }))
    }

    async fn list_networks(
        &self,
        request: Request<controller_proto::ListNetworksRequest>,
    ) -> Result<Response<controller_proto::ListNetworksResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
                    network_type: n.network_type,
                    enable_outbound_nat: n.enable_outbound_nat,
                })
                .collect(),
        }))
    }

    async fn create_security_group(
        &self,
        request: Request<controller_proto::CreateSecurityGroupRequest>,
    ) -> Result<Response<controller_proto::CreateSecurityGroupResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let sg = req
            .security_group
            .ok_or_else(|| Status::invalid_argument("security_group is required"))?;
        let name = validate_network_name(&sg.name)?;
        let row = SecurityGroupRow {
            name: name.clone(),
            description: sg.description.trim().to_string(),
            created_at: String::new(),
        };
        self.db
            .upsert_security_group(&row)
            .map_err(|e| Status::internal(format!("upserting security group: {e}")))?;

        let mut rules = Vec::new();
        for r in sg.rules {
            let id = if r.id.trim().is_empty() {
                Uuid::new_v4().to_string()
            } else {
                r.id.trim().to_string()
            };
            let protocol = normalize_sg_protocol(&r.protocol)?;
            let host_port = validate_port(r.host_port, "host_port")?;
            let target_port = if r.target_port <= 0 {
                host_port
            } else {
                validate_port(r.target_port, "target_port")?
            };
            rules.push(SecurityGroupRuleRow {
                id,
                security_group: name.clone(),
                protocol,
                host_port,
                target_port,
                source_cidr: r.source_cidr.trim().to_string(),
                target_vm: r.target_vm.trim().to_string(),
                enable_dnat: r.enable_dnat,
            });
        }
        self.db
            .replace_security_group_rules(&name, &rules)
            .map_err(|e| Status::internal(format!("storing security group rules: {e}")))?;

        self.log_replication_event(
            EVT_SECURITY_GROUP_CREATE,
            &format!("security-group/{name}"),
            serde_json::json!({
                "name": name,
                "description": row.description,
                "rules": rules.iter().map(|r| serde_json::json!({
                    "id": r.id,
                    "protocol": r.protocol,
                    "hostPort": r.host_port,
                    "targetPort": r.target_port,
                    "sourceCidr": r.source_cidr,
                    "targetVm": r.target_vm,
                    "enableDnat": r.enable_dnat,
                })).collect::<Vec<_>>(),
            }),
        );

        let created = self
            .db
            .get_security_group(&name)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::internal("security group disappeared after create"))?;
        let stored_rules = self
            .db
            .list_security_group_rules(&name)
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(
            controller_proto::CreateSecurityGroupResponse {
                success: true,
                security_group: Some(controller_proto::SecurityGroup {
                    name: created.name,
                    description: created.description,
                    rules: stored_rules
                        .into_iter()
                        .map(|r| controller_proto::SecurityGroupRule {
                            id: r.id,
                            protocol: r.protocol,
                            host_port: r.host_port,
                            target_port: r.target_port,
                            source_cidr: r.source_cidr,
                            target_vm: r.target_vm,
                            enable_dnat: r.enable_dnat,
                        })
                        .collect(),
                    created_at: parse_datetime_to_timestamp(&created.created_at),
                }),
            },
        ))
    }

    async fn get_security_group(
        &self,
        request: Request<controller_proto::GetSecurityGroupRequest>,
    ) -> Result<Response<controller_proto::GetSecurityGroupResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let name = req.name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }
        let sg = self
            .db
            .get_security_group(name)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("security group '{name}' not found")))?;
        let rules = self
            .db
            .list_security_group_rules(name)
            .map_err(|e| Status::internal(e.to_string()))?;
        let vm_atts = self
            .db
            .list_security_group_vm_attachments(name)
            .map_err(|e| Status::internal(e.to_string()))?;
        let net_atts = self
            .db
            .list_security_group_network_attachments(name)
            .map_err(|e| Status::internal(e.to_string()))?;
        let mut attachments = Vec::new();
        attachments.extend(vm_atts.into_iter().map(|a| {
            controller_proto::SecurityGroupAttachment {
                security_group: a.security_group,
                target_kind: controller_proto::SecurityGroupTargetKind::Vm as i32,
                target_id: a.vm_id,
                target_node: String::new(),
            }
        }));
        attachments.extend(net_atts.into_iter().map(|a| {
            controller_proto::SecurityGroupAttachment {
                security_group: a.security_group,
                target_kind: controller_proto::SecurityGroupTargetKind::Network as i32,
                target_id: a.network_name,
                target_node: a.node_id,
            }
        }));

        Ok(Response::new(controller_proto::GetSecurityGroupResponse {
            security_group: Some(controller_proto::SecurityGroup {
                name: sg.name,
                description: sg.description,
                rules: rules
                    .into_iter()
                    .map(|r| controller_proto::SecurityGroupRule {
                        id: r.id,
                        protocol: r.protocol,
                        host_port: r.host_port,
                        target_port: r.target_port,
                        source_cidr: r.source_cidr,
                        target_vm: r.target_vm,
                        enable_dnat: r.enable_dnat,
                    })
                    .collect(),
                created_at: parse_datetime_to_timestamp(&sg.created_at),
            }),
            attachments,
        }))
    }

    async fn list_security_groups(
        &self,
        request: Request<controller_proto::ListSecurityGroupsRequest>,
    ) -> Result<Response<controller_proto::ListSecurityGroupsResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let groups = self
            .db
            .list_security_groups()
            .map_err(|e| Status::internal(e.to_string()))?;
        let mut out = Vec::new();
        for sg in groups {
            let rules = self
                .db
                .list_security_group_rules(&sg.name)
                .map_err(|e| Status::internal(e.to_string()))?;
            out.push(controller_proto::SecurityGroup {
                name: sg.name,
                description: sg.description,
                rules: rules
                    .into_iter()
                    .map(|r| controller_proto::SecurityGroupRule {
                        id: r.id,
                        protocol: r.protocol,
                        host_port: r.host_port,
                        target_port: r.target_port,
                        source_cidr: r.source_cidr,
                        target_vm: r.target_vm,
                        enable_dnat: r.enable_dnat,
                    })
                    .collect(),
                created_at: parse_datetime_to_timestamp(&sg.created_at),
            });
        }
        Ok(Response::new(
            controller_proto::ListSecurityGroupsResponse {
                security_groups: out,
            },
        ))
    }

    async fn delete_security_group(
        &self,
        request: Request<controller_proto::DeleteSecurityGroupRequest>,
    ) -> Result<Response<controller_proto::DeleteSecurityGroupResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let name = req.name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }
        let deleted = self
            .db
            .delete_security_group(name)
            .map_err(|e| Status::internal(e.to_string()))?;
        if !deleted {
            return Err(Status::not_found(format!(
                "security group '{name}' not found"
            )));
        }
        self.log_replication_event(
            EVT_SECURITY_GROUP_DELETE,
            &format!("security-group/{name}"),
            serde_json::json!({ "name": name }),
        );
        Ok(Response::new(
            controller_proto::DeleteSecurityGroupResponse { success: true },
        ))
    }

    async fn attach_security_group(
        &self,
        request: Request<controller_proto::AttachSecurityGroupRequest>,
    ) -> Result<Response<controller_proto::AttachSecurityGroupResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let sg = req.security_group.trim();
        if sg.is_empty() {
            return Err(Status::invalid_argument("security_group is required"));
        }
        if self
            .db
            .get_security_group(sg)
            .map_err(|e| Status::internal(e.to_string()))?
            .is_none()
        {
            return Err(Status::not_found(format!(
                "security group '{sg}' not found"
            )));
        }
        let kind = parse_sg_target_kind(req.target_kind)?;
        match kind {
            controller_proto::SecurityGroupTargetKind::Vm => {
                let vm = self
                    .db
                    .get_vm(&req.target_id)
                    .map_err(|e| Status::internal(e.to_string()))?
                    .or_else(|| {
                        self.db
                            .list_vms()
                            .ok()
                            .and_then(|vms| vms.into_iter().find(|v| v.name == req.target_id))
                    })
                    .ok_or_else(|| {
                        Status::not_found(format!("vm '{}' not found", req.target_id))
                    })?;
                self.db
                    .attach_security_group_to_vm(sg, &vm.id)
                    .map_err(|e| Status::internal(e.to_string()))?;
                if let Some(node) = self
                    .db
                    .get_node(&vm.node_id)
                    .map_err(|e| Status::internal(e.to_string()))?
                {
                    self.push_config_to_node(&node).await?;
                }
            }
            controller_proto::SecurityGroupTargetKind::Network => {
                let node = if !req.target_node.trim().is_empty() {
                    self.db
                        .get_node_by_address(req.target_node.trim())
                        .map_err(|e| Status::internal(e.to_string()))?
                        .or_else(|| self.db.get_node(req.target_node.trim()).ok().flatten())
                        .ok_or_else(|| {
                            Status::not_found(format!("node '{}' not found", req.target_node))
                        })?
                } else {
                    return Err(Status::invalid_argument(
                        "target_node is required for network attachments",
                    ));
                };
                if self
                    .db
                    .get_network_for_node(&node.id, req.target_id.trim())
                    .map_err(|e| Status::internal(e.to_string()))?
                    .is_none()
                {
                    return Err(Status::not_found(format!(
                        "network '{}' not found on node '{}'",
                        req.target_id.trim(),
                        node.id
                    )));
                }
                self.db
                    .attach_security_group_to_network(sg, req.target_id.trim(), &node.id)
                    .map_err(|e| Status::internal(e.to_string()))?;
                self.push_config_to_node(&node).await?;
            }
            controller_proto::SecurityGroupTargetKind::Unspecified => unreachable!(),
        }
        self.log_replication_event(
            EVT_SECURITY_GROUP_ATTACH,
            &format!("security-group/{sg}"),
            serde_json::json!({
                "securityGroup": sg,
                "targetKind": req.target_kind,
                "targetId": req.target_id,
                "targetNode": req.target_node
            }),
        );
        Ok(Response::new(
            controller_proto::AttachSecurityGroupResponse { success: true },
        ))
    }

    async fn detach_security_group(
        &self,
        request: Request<controller_proto::DetachSecurityGroupRequest>,
    ) -> Result<Response<controller_proto::DetachSecurityGroupResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let sg = req.security_group.trim();
        if sg.is_empty() {
            return Err(Status::invalid_argument("security_group is required"));
        }
        let kind = parse_sg_target_kind(req.target_kind)?;
        match kind {
            controller_proto::SecurityGroupTargetKind::Vm => {
                let vm = self
                    .db
                    .get_vm(&req.target_id)
                    .map_err(|e| Status::internal(e.to_string()))?
                    .or_else(|| {
                        self.db
                            .list_vms()
                            .ok()
                            .and_then(|vms| vms.into_iter().find(|v| v.name == req.target_id))
                    })
                    .ok_or_else(|| {
                        Status::not_found(format!("vm '{}' not found", req.target_id))
                    })?;
                self.db
                    .detach_security_group_from_vm(sg, &vm.id)
                    .map_err(|e| Status::internal(e.to_string()))?;
                if let Some(node) = self
                    .db
                    .get_node(&vm.node_id)
                    .map_err(|e| Status::internal(e.to_string()))?
                {
                    self.push_config_to_node(&node).await?;
                }
            }
            controller_proto::SecurityGroupTargetKind::Network => {
                let node = if !req.target_node.trim().is_empty() {
                    self.db
                        .get_node_by_address(req.target_node.trim())
                        .map_err(|e| Status::internal(e.to_string()))?
                        .or_else(|| self.db.get_node(req.target_node.trim()).ok().flatten())
                        .ok_or_else(|| {
                            Status::not_found(format!("node '{}' not found", req.target_node))
                        })?
                } else {
                    return Err(Status::invalid_argument(
                        "target_node is required for network attachments",
                    ));
                };
                self.db
                    .detach_security_group_from_network(sg, req.target_id.trim(), &node.id)
                    .map_err(|e| Status::internal(e.to_string()))?;
                self.push_config_to_node(&node).await?;
            }
            controller_proto::SecurityGroupTargetKind::Unspecified => unreachable!(),
        }
        self.log_replication_event(
            EVT_SECURITY_GROUP_DETACH,
            &format!("security-group/{sg}"),
            serde_json::json!({
                "securityGroup": sg,
                "targetKind": req.target_kind,
                "targetId": req.target_id,
                "targetNode": req.target_node
            }),
        );
        Ok(Response::new(
            controller_proto::DetachSecurityGroupResponse { success: true },
        ))
    }

    async fn list_nodes(
        &self,
        request: Request<controller_proto::ListNodesRequest>,
    ) -> Result<Response<controller_proto::ListNodesResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
                    disable_vxlan: n.disable_vxlan,
                    approval_status: n.approval_status,
                    cert_expiry_days: n.cert_expiry_days,
                    luks_method: n.luks_method,
                    dc_id: n.dc_id,
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
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
                node_id: node.id.clone(),
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
                disable_vxlan: node.disable_vxlan,
                approval_status: node.approval_status,
                cert_expiry_days: node.cert_expiry_days,
                luks_method: node.luks_method,
                dc_id: node.dc_id,
            }),
        }))
    }

    async fn create_ssh_key(
        &self,
        request: Request<controller_proto::CreateSshKeyRequest>,
    ) -> Result<Response<controller_proto::CreateSshKeyResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
        self.log_replication_event(
            EVT_SSH_KEY_CREATE,
            &format!("ssh-key/{}", req.name.trim()),
            serde_json::json!({
                "name": req.name.trim(),
                "publicKey": req.public_key.trim(),
            }),
        );

        Ok(Response::new(controller_proto::CreateSshKeyResponse {
            success: true,
            message: format!("SSH key '{}' created", req.name),
        }))
    }

    async fn delete_ssh_key(
        &self,
        request: Request<controller_proto::DeleteSshKeyRequest>,
    ) -> Result<Response<controller_proto::DeleteSshKeyResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
        self.log_replication_event(
            EVT_SSH_KEY_DELETE,
            &format!("ssh-key/{}", req.name.trim()),
            serde_json::json!({
                "name": req.name.trim(),
            }),
        );

        Ok(Response::new(controller_proto::DeleteSshKeyResponse {
            success: true,
        }))
    }

    async fn list_ssh_keys(
        &self,
        request: Request<controller_proto::ListSshKeysRequest>,
    ) -> Result<Response<controller_proto::ListSshKeysResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;

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
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
        self.log_replication_event_required(
            EVT_NODE_DRAIN,
            &format!("node/{}", req.node_id),
            serde_json::json!({
                "nodeId": req.node_id,
                "targetNode": req.target_node,
                "migrated": migrated,
                "errors": errors,
            }),
        )?;

        Ok(Response::new(controller_proto::DrainNodeResponse {
            success: errors.is_empty(),
            vms_migrated: migrated,
            message: msg,
        }))
    }

    async fn approve_node(
        &self,
        request: Request<controller_proto::ApproveNodeRequest>,
    ) -> Result<Response<controller_proto::ApproveNodeResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();

        let node = self
            .db
            .get_node(&req.node_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("node '{}' not found", req.node_id)))?;

        if node.approval_status == "approved" {
            return Ok(Response::new(controller_proto::ApproveNodeResponse {
                success: true,
                message: "node is already approved".into(),
            }));
        }

        self.db
            .set_node_approval(&req.node_id, "approved")
            .map_err(|e| Status::internal(format!("approving node: {e}")))?;
        self.db
            .update_node_status(&req.node_id, "ready")
            .map_err(|e| Status::internal(format!("updating node status: {e}")))?;

        if let Err(e) = self.clients.connect(&node.address).await {
            warn!(address = %node.address, error = %e, "failed to connect to approved node");
        }

        self.log_replication_event_required(
            EVT_NODE_APPROVE,
            &format!("node/{}", req.node_id),
            serde_json::json!({
                "nodeId": req.node_id,
                "address": node.address,
            }),
        )?;

        info!(node_id = %req.node_id, "node approved");

        Ok(Response::new(controller_proto::ApproveNodeResponse {
            success: true,
            message: format!("node '{}' approved", req.node_id),
        }))
    }

    async fn reject_node(
        &self,
        request: Request<controller_proto::RejectNodeRequest>,
    ) -> Result<Response<controller_proto::RejectNodeResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();

        let _node = self
            .db
            .get_node(&req.node_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("node '{}' not found", req.node_id)))?;

        self.db
            .set_node_approval(&req.node_id, "rejected")
            .map_err(|e| Status::internal(format!("rejecting node: {e}")))?;
        self.db
            .update_node_status(&req.node_id, "rejected")
            .map_err(|e| Status::internal(format!("updating node status: {e}")))?;

        self.log_replication_event_required(
            EVT_NODE_REJECT,
            &format!("node/{}", req.node_id),
            serde_json::json!({
                "nodeId": req.node_id,
            }),
        )?;

        info!(node_id = %req.node_id, "node rejected");

        Ok(Response::new(controller_proto::RejectNodeResponse {
            success: true,
            message: format!("node '{}' rejected", req.node_id),
        }))
    }

    async fn renew_node_cert(
        &self,
        request: Request<controller_proto::RenewNodeCertRequest>,
    ) -> Result<Response<controller_proto::RenewNodeCertResponse>, Status> {
        auth::require_peer(&request, &[CN_NODE_PREFIX])?;
        let req = request.into_inner();

        let node = self
            .db
            .get_node(&req.node_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("node '{}' not found", req.node_id)))?;

        if node.approval_status != "approved" {
            return Err(Status::permission_denied(format!(
                "node '{}' is not approved (status: {})",
                req.node_id, node.approval_status
            )));
        }

        let sub_ca = self
            .sub_ca
            .lock()
            .map_err(|_| Status::internal("sub-CA lock poisoned"))?
            .clone();

        if !sub_ca.is_available() {
            return Err(Status::unavailable(
                "sub-CA is not configured on this controller; certificate renewal is unavailable",
            ));
        }

        let node_host = node.address.split(':').next().unwrap_or("").to_string();
        if node_host.is_empty() {
            return Err(Status::internal(format!(
                "cannot determine host from node address '{}'",
                node.address
            )));
        }

        let (chain_pem, key_pem) =
            signing::sign_node_cert(&sub_ca.cert_pem, &sub_ca.key_pem, &node_host)
                .map_err(|e| Status::internal(format!("signing node cert: {e}")))?;

        info!(node_id = %req.node_id, host = %node_host, "renewed node certificate via sub-CA");

        Ok(Response::new(controller_proto::RenewNodeCertResponse {
            success: true,
            cert_pem: chain_pem,
            key_pem,
            message: format!("certificate renewed for node '{}'", req.node_id),
        }))
    }

    async fn issue_node_bootstrap_cert(
        &self,
        request: Request<controller_proto::IssueNodeBootstrapCertRequest>,
    ) -> Result<Response<controller_proto::IssueNodeBootstrapCertResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();

        let node_id = req.node_id.trim();
        if node_id.is_empty() {
            return Err(Status::invalid_argument("node_id is required"));
        }
        let node_host = req.node_host.trim();
        if node_host.is_empty() {
            return Err(Status::invalid_argument("node_host is required"));
        }

        let sub_ca = self
            .sub_ca
            .lock()
            .map_err(|_| Status::internal("sub-CA lock poisoned"))?
            .clone();

        if !sub_ca.is_available() {
            return Err(Status::unavailable(
                "sub-CA is not configured on this controller; node bootstrap certificate issuance is unavailable",
            ));
        }

        let (chain_pem, key_pem) =
            signing::sign_node_cert(&sub_ca.cert_pem, &sub_ca.key_pem, node_host)
                .map_err(|e| Status::internal(format!("signing bootstrap node cert: {e}")))?;

        info!(node_id = %node_id, node_host = %node_host, "issued bootstrap node certificate via sub-CA");

        Ok(Response::new(
            controller_proto::IssueNodeBootstrapCertResponse {
                success: true,
                cert_pem: chain_pem,
                key_pem,
                message: format!("bootstrap certificate issued for node '{}'", node_id),
            },
        ))
    }

    async fn rotate_sub_ca(
        &self,
        request: Request<controller_proto::RotateSubCaRequest>,
    ) -> Result<Response<controller_proto::RotateSubCaResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();

        if req.sub_ca_cert_pem.trim().is_empty() || req.sub_ca_key_pem.trim().is_empty() {
            return Err(Status::invalid_argument(
                "sub_ca_cert_pem and sub_ca_key_pem are required",
            ));
        }

        signing::validate_sub_ca_cert(&req.sub_ca_cert_pem)
            .map_err(|e| Status::invalid_argument(format!("invalid sub-CA cert: {e}")))?;

        let mut sub_ca = self
            .sub_ca
            .lock()
            .map_err(|_| Status::internal("sub-CA lock poisoned"))?;

        if let Some(cert_file) = &sub_ca.cert_file {
            std::fs::write(cert_file, &req.sub_ca_cert_pem)
                .map_err(|e| Status::internal(format!("writing sub-CA cert: {e}")))?;
        }
        if let Some(key_file) = &sub_ca.key_file {
            std::fs::write(key_file, &req.sub_ca_key_pem)
                .map_err(|e| Status::internal(format!("writing sub-CA key: {e}")))?;
        }

        sub_ca.cert_pem = req.sub_ca_cert_pem;
        sub_ca.key_pem = req.sub_ca_key_pem;

        info!("sub-CA rotated via kctl");

        Ok(Response::new(controller_proto::RotateSubCaResponse {
            success: true,
            message: "sub-CA rotated successfully".into(),
        }))
    }

    async fn reload_tls(
        &self,
        request: Request<controller_proto::ReloadTlsRequest>,
    ) -> Result<Response<controller_proto::ReloadTlsResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();

        if req.cert_pem.trim().is_empty() || req.key_pem.trim().is_empty() {
            return Err(Status::invalid_argument(
                "cert_pem and key_pem are required",
            ));
        }

        let tls = self.tls_paths.as_ref().ok_or_else(|| {
            Status::failed_precondition("TLS is not configured on this controller")
        })?;

        std::fs::write(&tls.cert_file, &req.cert_pem)
            .map_err(|e| Status::internal(format!("writing cert: {e}")))?;
        std::fs::write(&tls.key_file, &req.key_pem)
            .map_err(|e| Status::internal(format!("writing key: {e}")))?;

        info!(
            cert = %tls.cert_file,
            key = %tls.key_file,
            "controller TLS cert written to disk"
        );

        #[cfg(unix)]
        {
            let pid = std::process::id();
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGHUP);
            }
            info!("SIGHUP sent to self, TLS reload in progress");
        }

        Ok(Response::new(controller_proto::ReloadTlsResponse {
            success: true,
            message: "TLS certificate updated; server reloading".into(),
        }))
    }

    async fn get_network_overview(
        &self,
        request: Request<controller_proto::GetNetworkOverviewRequest>,
    ) -> Result<Response<controller_proto::GetNetworkOverviewResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;

        let node_rows = self
            .db
            .list_nodes()
            .map_err(|e| Status::internal(e.to_string()))?;

        let approved: Vec<_> = node_rows
            .into_iter()
            .filter(|n| n.approval_status == "approved")
            .collect();

        let mut nodes = Vec::with_capacity(approved.len());
        for node in &approved {
            let interfaces = match self.ensure_admin_client_for_node(node).await {
                Ok(mut admin) => {
                    match admin
                        .list_network_interfaces(node_proto::ListNetworkInterfacesRequest {})
                        .await
                    {
                        Ok(resp) => resp
                            .into_inner()
                            .interfaces
                            .into_iter()
                            .map(|iface| controller_proto::NetworkInterfaceDetail {
                                name: iface.name,
                                mac_address: iface.mac_address,
                                state: iface.state,
                                mtu: iface.mtu,
                                addresses: iface.addresses,
                            })
                            .collect(),
                        Err(e) => {
                            warn!(node_id = %node.id, error = %e, "ListNetworkInterfaces failed");
                            vec![]
                        }
                    }
                }
                Err(e) => {
                    warn!(node_id = %node.id, error = %e, "cannot reach node for network overview");
                    vec![]
                }
            };

            nodes.push(controller_proto::NodeNetworkInfo {
                node_id: node.id.clone(),
                hostname: node.hostname.clone(),
                address: node.address.clone(),
                gateway_interface: node.gateway_interface.clone(),
                disable_vxlan: node.disable_vxlan,
                interfaces,
            });
        }

        Ok(Response::new(
            controller_proto::GetNetworkOverviewResponse {
                default_gateway_interface: self.default_network.gateway_interface.clone(),
                default_external_ip: self.default_network.external_ip.clone(),
                default_gateway_ip: self.default_network.gateway_ip.clone(),
                default_internal_netmask: self.default_network.internal_netmask.clone(),
                nodes,
            },
        ))
    }

    async fn get_storage_overview(
        &self,
        request: Request<controller_proto::GetStorageOverviewRequest>,
    ) -> Result<Response<controller_proto::GetStorageOverviewResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let _ = request.into_inner();

        let node_rows = self
            .db
            .list_nodes()
            .map_err(|e| Status::internal(e.to_string()))?;
        let approved: Vec<_> = node_rows
            .into_iter()
            .filter(|n| n.approval_status == "approved")
            .collect();

        let (nodes_luks_tpm2, nodes_luks_keyfile, nodes_luks_unknown) = self
            .db
            .count_nodes_luks_method()
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut backend_filesystem_nodes: i32 = 0;
        let mut backend_lvm_nodes: i32 = 0;
        let mut backend_zfs_nodes: i32 = 0;
        let mut backend_unspecified_nodes: i32 = 0;
        for n in &approved {
            match n.storage_backend.as_str() {
                "filesystem" => backend_filesystem_nodes += 1,
                "lvm" => backend_lvm_nodes += 1,
                "zfs" => backend_zfs_nodes += 1,
                _ => backend_unspecified_nodes += 1,
            }
        }

        let mut nodes_out = Vec::with_capacity(approved.len());
        let mut nodes_disk_inventory_ok: i32 = 0;
        let mut total_block_devices: i32 = 0;

        for node in &approved {
            let (
                disks,
                disk_inventory_ok,
                lvm_inventory_ok,
                lvm_volume_groups,
                lvm_logical_volumes,
                lvm_physical_volumes,
            ) = match self.ensure_admin_client_for_node(node).await {
                Ok(mut admin) => {
                    let (disks, disk_inventory_ok) =
                        match admin.list_disks(node_proto::ListDisksRequest {}).await {
                            Ok(resp) => {
                                let disks: Vec<controller_proto::StorageDiskDetail> = resp
                                    .into_inner()
                                    .disks
                                    .into_iter()
                                    .map(|d| controller_proto::StorageDiskDetail {
                                        name: d.name,
                                        path: d.path,
                                        size: d.size,
                                        model: d.model,
                                        fstype: d.fstype,
                                        mountpoint: d.mountpoint,
                                    })
                                    .collect();
                                (disks, true)
                            }
                            Err(e) => {
                                warn!(
                                    node_id = %node.id,
                                    error = %e,
                                    "ListDisks failed for storage overview"
                                );
                                (vec![], false)
                            }
                        };

                    let (
                        lvm_inventory_ok,
                        lvm_volume_groups,
                        lvm_logical_volumes,
                        lvm_physical_volumes,
                    ) = match admin.get_lvm_info(node_proto::GetLvmInfoRequest {}).await {
                        Ok(resp) => {
                            let inner = resp.into_inner();
                            if inner.available {
                                (
                                    true,
                                    inner
                                        .volume_groups
                                        .into_iter()
                                        .map(|vg| controller_proto::StorageLvmVolumeGroupDetail {
                                            name: vg.name,
                                            size_bytes: vg.size_bytes,
                                            free_bytes: vg.free_bytes,
                                            attr: vg.attr,
                                        })
                                        .collect(),
                                    inner
                                        .logical_volumes
                                        .into_iter()
                                        .map(|lv| controller_proto::StorageLvmLogicalVolumeDetail {
                                            name: lv.name,
                                            vg_name: lv.vg_name,
                                            size_bytes: lv.size_bytes,
                                            attr: lv.attr,
                                            path: lv.path,
                                            pool: lv.pool,
                                            origin: lv.origin,
                                            data_percent: lv.data_percent,
                                            metadata_percent: lv.metadata_percent,
                                        })
                                        .collect(),
                                    inner
                                        .physical_volumes
                                        .into_iter()
                                        .map(|pv| {
                                            controller_proto::StorageLvmPhysicalVolumeDetail {
                                                name: pv.name,
                                                vg_name: pv.vg_name,
                                                size_bytes: pv.size_bytes,
                                                free_bytes: pv.free_bytes,
                                                attr: pv.attr,
                                            }
                                        })
                                        .collect(),
                                )
                            } else {
                                (false, vec![], vec![], vec![])
                            }
                        }
                        Err(e) => {
                            warn!(
                                node_id = %node.id,
                                error = %e,
                                "GetLvmInfo failed for storage overview"
                            );
                            (false, vec![], vec![], vec![])
                        }
                    };

                    (
                        disks,
                        disk_inventory_ok,
                        lvm_inventory_ok,
                        lvm_volume_groups,
                        lvm_logical_volumes,
                        lvm_physical_volumes,
                    )
                }
                Err(e) => {
                    warn!(
                        node_id = %node.id,
                        error = %e,
                        "cannot reach node for storage overview"
                    );
                    (vec![], false, false, vec![], vec![], vec![])
                }
            };

            if disk_inventory_ok {
                nodes_disk_inventory_ok += 1;
                total_block_devices += disks.len() as i32;
            }

            nodes_out.push(controller_proto::NodeStorageOverview {
                node_id: node.id.clone(),
                hostname: node.hostname.clone(),
                address: node.address.clone(),
                storage_backend: storage_backend_to_proto(&node.storage_backend),
                luks_method: node.luks_method.clone(),
                disk_inventory_ok,
                disks,
                lvm_inventory_ok,
                lvm_volume_groups,
                lvm_logical_volumes,
                lvm_physical_volumes,
            });
        }

        Ok(Response::new(
            controller_proto::GetStorageOverviewResponse {
                approved_nodes: approved.len() as i32,
                nodes_disk_inventory_ok,
                backend_filesystem_nodes,
                backend_lvm_nodes,
                backend_zfs_nodes,
                backend_unspecified_nodes,
                nodes_luks_tpm2,
                nodes_luks_keyfile,
                nodes_luks_unknown,
                total_block_devices,
                nodes: nodes_out,
            },
        ))
    }

    async fn list_volumes(
        &self,
        request: Request<controller_proto::ListVolumesRequest>,
    ) -> Result<Response<controller_proto::ListVolumesResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let _ = request.into_inner();

        let vms = self
            .db
            .list_vms()
            .map_err(|e| Status::internal(e.to_string()))?;

        let node_address_by_id: std::collections::HashMap<String, String> = self
            .db
            .list_nodes()
            .map_err(|e| Status::internal(e.to_string()))?
            .into_iter()
            .map(|n| (n.id, n.address))
            .collect();

        let vm_count = vms.len();
        let mut fallback_states: Vec<i32> = Vec::with_capacity(vm_count);
        let mut set = tokio::task::JoinSet::new();

        for (idx, vm) in vms.iter().enumerate() {
            fallback_states.push(state_fallback_without_runtime(vm.auto_start));
            if let Some(node_address) = node_address_by_id.get(&vm.node_id) {
                if self.clients.get_compute(node_address).is_none() {
                    let _ = self.clients.connect(node_address).await;
                }
                if let Some(mut compute) = self.clients.get_compute(node_address) {
                    let vm_name = vm.name.clone();
                    set.spawn(async move {
                        let result = tokio::time::timeout(
                            Duration::from_secs(3),
                            compute.get_vm(node_proto::GetVmRequest {
                                vm_id: vm_name.clone(),
                            }),
                        )
                        .await;
                        (idx, result)
                    });
                }
            }
        }

        let mut live_states: Vec<Option<i32>> = vec![None; vm_count];
        while let Some(Ok((idx, result))) = set.join_next().await {
            if let Ok(Ok(resp)) = result {
                if let Some(status) = resp.into_inner().status {
                    live_states[idx] = Some(controller_state_from_node_state(status.state));
                }
            }
        }

        let volumes: Vec<_> = vms
            .into_iter()
            .enumerate()
            .map(|(i, vm)| {
                let state = live_states[i].unwrap_or(fallback_states[i]);
                controller_proto::VolumeInfo {
                    vm_id: vm.id.clone(),
                    vm_name: vm.name.clone(),
                    node_id: vm.node_id.clone(),
                    storage_backend: vm.storage_backend.clone(),
                    storage_size_bytes: vm.storage_size_bytes,
                    backend_handle: vm_backend_handle(&vm),
                    image_format: if vm.storage_backend == "lvm" || vm.storage_backend == "zfs" {
                        "raw".to_string()
                    } else {
                        vm.image_format.clone()
                    },
                    vm_state: state,
                }
            })
            .collect();

        Ok(Response::new(controller_proto::ListVolumesResponse {
            volumes,
        }))
    }

    // TODO(rbac): restrict to admin role when RBAC is implemented
    async fn get_compliance_report(
        &self,
        request: Request<controller_proto::GetComplianceReportRequest>,
    ) -> Result<Response<controller_proto::GetComplianceReportResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;

        let (approved, pending, rejected) = self
            .db
            .count_nodes_by_approval()
            .map_err(|e| Status::internal(e.to_string()))?;
        let total_nodes = approved + pending + rejected;

        let (total_vms, running_vms) = self
            .db
            .count_vms_by_auto_start()
            .map_err(|e| Status::internal(e.to_string()))?;
        let stopped_vms = total_vms - running_vms;

        let (nat, bridge, vxlan) = self
            .db
            .count_networks_by_type()
            .map_err(|e| Status::internal(e.to_string()))?;
        let total_networks = nat + bridge + vxlan;

        let (expiring_30d, cert_unknown) = self
            .db
            .count_nodes_cert_expiry()
            .map_err(|e| Status::internal(e.to_string()))?;

        let (luks_tpm2, luks_keyfile, luks_unknown) = self
            .db
            .count_nodes_luks_method()
            .map_err(|e| Status::internal(e.to_string()))?;

        let sub_ca_enabled = self.sub_ca.lock().unwrap().is_available();

        let node_rows = self
            .db
            .list_nodes()
            .map_err(|e| Status::internal(e.to_string()))?;
        let all_labels = self.db.get_all_node_labels().unwrap_or_default();
        let nodes: Vec<controller_proto::NodeInfo> = node_rows
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
                    disable_vxlan: n.disable_vxlan,
                    approval_status: n.approval_status,
                    cert_expiry_days: n.cert_expiry_days,
                    luks_method: n.luks_method,
                    dc_id: n.dc_id,
                }
            })
            .collect();

        let access_control = vec![
            acl("RegisterNode", CN_NODE_PREFIX),
            acl("Heartbeat", CN_NODE_PREFIX),
            acl("SyncVmState", CN_NODE_PREFIX),
            acl("CreateVm", CN_KCTL),
            acl("UpdateVm", CN_KCTL),
            acl("DeleteVm", CN_KCTL),
            acl("SetVmDesiredState", CN_KCTL),
            acl("GetVm", CN_KCTL),
            acl("ListVms", CN_KCTL),
            acl("CreateNetwork", CN_KCTL),
            acl("DeleteNetwork", CN_KCTL),
            acl("ListNetworks", CN_KCTL),
            acl("CreateSecurityGroup", CN_KCTL),
            acl("GetSecurityGroup", CN_KCTL),
            acl("ListSecurityGroups", CN_KCTL),
            acl("DeleteSecurityGroup", CN_KCTL),
            acl("AttachSecurityGroup", CN_KCTL),
            acl("DetachSecurityGroup", CN_KCTL),
            acl("ListNodes", CN_KCTL),
            acl("GetNode", CN_KCTL),
            acl("CreateSshKey", CN_KCTL),
            acl("DeleteSshKey", CN_KCTL),
            acl("ListSshKeys", CN_KCTL),
            acl("GetSshKey", CN_KCTL),
            acl("DrainNode", CN_KCTL),
            acl("ApproveNode", CN_KCTL),
            acl("RejectNode", CN_KCTL),
            acl("RenewNodeCert", CN_NODE_PREFIX),
            acl("IssueNodeBootstrapCert", CN_KCTL),
            acl("RotateSubCa", CN_KCTL),
            acl("ReloadTls", CN_KCTL),
            acl("GetComplianceReport", CN_KCTL),
            acl("GetNetworkOverview", CN_KCTL),
            acl("GetStorageOverview", CN_KCTL),
            acl("ApplyNixConfig", CN_KCTL),
        ];

        Ok(Response::new(
            controller_proto::GetComplianceReportResponse {
                controller_version: env!("CARGO_PKG_VERSION").to_string(),
                crypto_library: "aws-lc-rs (AWS-LC, FIPS 140-3 #4816)".into(),
                tls13_cipher_suites: vec![
                    "TLS_AES_256_GCM_SHA384".into(),
                    "TLS_AES_128_GCM_SHA256".into(),
                ],
                tls12_cipher_suites: vec![
                    "TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384".into(),
                    "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256".into(),
                    "TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384".into(),
                    "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256".into(),
                ],
                kx_groups: vec!["secp384r1 (P-384)".into(), "secp256r1 (P-256)".into()],
                excluded_algorithms: vec![
                    "ChaCha20-Poly1305".into(),
                    "X25519".into(),
                    "RSA key exchange".into(),
                ],
                mtls_enabled: self.tls_paths.is_some(),
                access_control,
                total_nodes,
                approved_nodes: approved,
                pending_nodes: pending,
                rejected_nodes: rejected,
                total_vms,
                running_vms,
                stopped_vms,
                total_networks,
                nat_networks: nat,
                bridge_networks: bridge,
                vxlan_networks: vxlan,
                sub_ca_enabled,
                cert_auto_renewal_days: 30,
                nodes_expiring_30d: expiring_30d,
                nodes_cert_unknown: cert_unknown,
                nodes,
                nodes_luks_tpm2: luks_tpm2,
                nodes_luks_keyfile: luks_keyfile,
                nodes_luks_unknown: luks_unknown,
            },
        ))
    }
}

fn acl(method: &str, identity: &str) -> controller_proto::AccessControlEntry {
    controller_proto::AccessControlEntry {
        rpc_method: method.to_string(),
        allowed_identities: if identity.ends_with('-') {
            format!("{identity}*")
        } else {
            identity.to_string()
        },
    }
}

#[cfg(test)]
#[allow(clippy::result_large_err)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::config::ReplicationConfig;

    fn empty_sub_ca() -> Arc<Mutex<SubCaState>> {
        Arc::new(Mutex::new(SubCaState::default()))
    }

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
            disable_vxlan: false,
            approval_status: "approved".to_string(),
            cert_expiry_days: -1,
            luks_method: String::new(),
            dc_id: "DC1".to_string(),
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
            vm_ip: String::new(),
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
            None,
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
            None,
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
            None,
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
                storage_backend: String::new(),
                storage_size_bytes: 0,
            }),
            image_url: String::new(),
            image_sha256: String::new(),
            cloud_init_user_data: String::new(),
            image_path: String::new(),
            image_format: String::new(),
            ssh_key_names: vec![],
            storage_backend: controller_proto::StorageBackendType::Filesystem as i32,
            storage_size_bytes: 8 * 1024 * 1024 * 1024,
            target_dc: String::new(),
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
            None,
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
                storage_backend: String::new(),
                storage_size_bytes: 0,
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
            target_dc: String::new(),
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
            None,
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
                storage_backend: String::new(),
                storage_size_bytes: 0,
            }),
            image_url: String::new(),
            image_sha256: String::new(),
            cloud_init_user_data: String::new(),
            image_path: "/var/lib/kcore/images/web-1.raw".to_string(),
            image_format: "raw".to_string(),
            ssh_key_names: vec![],
            storage_backend: controller_proto::StorageBackendType::Filesystem as i32,
            storage_size_bytes: 8 * 1024 * 1024 * 1024,
            target_dc: String::new(),
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
            None,
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
                storage_backend: String::new(),
                storage_size_bytes: 0,
            }),
            image_url: String::new(),
            image_sha256: String::new(),
            cloud_init_user_data: String::new(),
            image_path: "/var/lib/kcore/images/base.raw".to_string(),
            image_format: "raw".to_string(),
            ssh_key_names: vec![],
            storage_backend: controller_proto::StorageBackendType::Filesystem as i32,
            storage_size_bytes: 8 * 1024 * 1024 * 1024,
            target_dc: String::new(),
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
    async fn create_vm_storage_backend_mismatch_auto_falls_back_when_possible() {
        let db = Database::open(":memory:").expect("open db");
        let mut wrong_node = test_node();
        wrong_node.id = "node-fs".to_string();
        wrong_node.storage_backend = "fs".to_string();
        db.upsert_node(&wrong_node).expect("insert wrong node");

        let mut candidate = test_node();
        candidate.id = "node-zfs".to_string();
        candidate.address = "127.0.0.2:9091".to_string();
        candidate.storage_backend = "zfs".to_string();
        db.upsert_node(&candidate).expect("insert candidate");

        let hook: PushHook = Arc::new(|_n: &NodeRow| Ok(()));
        let svc = ControllerService::new_with_test_push_hook(
            db,
            NodeClients::new(None),
            test_network(),
            None,
            hook,
        );

        let req = controller_proto::CreateVmRequest {
            target_node: wrong_node.id.clone(),
            spec: Some(controller_proto::VmSpec {
                id: String::new(),
                name: "vm-zfs-fallback".to_string(),
                cpu: 1,
                memory_bytes: 512 * 1024 * 1024,
                disks: vec![],
                nics: vec![],
                storage_backend: String::new(),
                storage_size_bytes: 0,
            }),
            image_url: String::new(),
            image_sha256: String::new(),
            cloud_init_user_data: String::new(),
            image_path: "/var/lib/kcore/images/base.raw".to_string(),
            image_format: "raw".to_string(),
            ssh_key_names: vec![],
            storage_backend: controller_proto::StorageBackendType::Zfs as i32,
            storage_size_bytes: 8 * 1024 * 1024 * 1024,
            target_dc: String::new(),
        };

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::create_vm(
                &svc,
                Request::new(req),
            )
            .await
            .expect("fallback should choose compatible node")
            .into_inner();
        assert_eq!(resp.node_id, "node-zfs");
    }

    #[tokio::test]
    async fn create_vm_rejects_target_node_without_capacity_in_preflight() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.cpu_used = 4;
        node.memory_used = 8 * 1024 * 1024 * 1024;
        db.upsert_node(&node).expect("insert node");

        let hook: PushHook = Arc::new(|_n: &NodeRow| Ok(()));
        let svc = ControllerService::new_with_test_push_hook(
            db,
            NodeClients::new(None),
            test_network(),
            None,
            hook,
        );

        let req = controller_proto::CreateVmRequest {
            target_node: node.id,
            spec: Some(controller_proto::VmSpec {
                id: String::new(),
                name: "vm-no-capacity".to_string(),
                cpu: 1,
                memory_bytes: 512 * 1024 * 1024,
                disks: vec![],
                nics: vec![],
                storage_backend: String::new(),
                storage_size_bytes: 0,
            }),
            image_url: String::new(),
            image_sha256: String::new(),
            cloud_init_user_data: String::new(),
            image_path: "/var/lib/kcore/images/base.raw".to_string(),
            image_format: "raw".to_string(),
            ssh_key_names: vec![],
            storage_backend: controller_proto::StorageBackendType::Filesystem as i32,
            storage_size_bytes: 8 * 1024 * 1024 * 1024,
            target_dc: String::new(),
        };

        let err =
            <ControllerService as controller_proto::controller_server::Controller>::create_vm(
                &svc,
                Request::new(req),
            )
            .await
            .expect_err("preflight capacity check should fail");
        assert_eq!(err.code(), tonic::Code::Unavailable);
        assert!(err.message().contains("lacks capacity"));
    }

    #[tokio::test]
    async fn create_vm_preflight_auto_falls_back_to_alternative_node() {
        let db = Database::open(":memory:").expect("open db");
        let mut overloaded = test_node();
        overloaded.id = "node-overloaded".to_string();
        overloaded.cpu_used = 4;
        overloaded.memory_used = 8 * 1024 * 1024 * 1024;
        db.upsert_node(&overloaded).expect("insert overloaded node");

        let mut candidate = test_node();
        candidate.id = "node-candidate".to_string();
        candidate.address = "127.0.0.2:9091".to_string();
        db.upsert_node(&candidate).expect("insert candidate node");

        let hook: PushHook = Arc::new(|_n: &NodeRow| Ok(()));
        let svc = ControllerService::new_with_test_push_hook(
            db,
            NodeClients::new(None),
            test_network(),
            None,
            hook,
        );

        let req = controller_proto::CreateVmRequest {
            target_node: "node-overloaded".to_string(),
            spec: Some(controller_proto::VmSpec {
                id: String::new(),
                name: "vm-hint".to_string(),
                cpu: 1,
                memory_bytes: 512 * 1024 * 1024,
                disks: vec![],
                nics: vec![],
                storage_backend: String::new(),
                storage_size_bytes: 0,
            }),
            image_url: String::new(),
            image_sha256: String::new(),
            cloud_init_user_data: String::new(),
            image_path: "/var/lib/kcore/images/base.raw".to_string(),
            image_format: "raw".to_string(),
            ssh_key_names: vec![],
            storage_backend: controller_proto::StorageBackendType::Filesystem as i32,
            storage_size_bytes: 8 * 1024 * 1024 * 1024,
            target_dc: String::new(),
        };

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::create_vm(
                &svc,
                Request::new(req),
            )
            .await
            .expect("preflight should auto-fallback to alternative")
            .into_inner();
        assert_eq!(resp.node_id, "node-candidate");
    }

    #[tokio::test]
    async fn create_vm_rejects_target_node_not_ready_in_preflight() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.status = "not-ready".to_string();
        db.upsert_node(&node).expect("insert node");

        let hook: PushHook = Arc::new(|_n: &NodeRow| Ok(()));
        let svc = ControllerService::new_with_test_push_hook(
            db,
            NodeClients::new(None),
            test_network(),
            None,
            hook,
        );

        let req = controller_proto::CreateVmRequest {
            target_node: node.id,
            spec: Some(controller_proto::VmSpec {
                id: String::new(),
                name: "vm-node-not-ready".to_string(),
                cpu: 1,
                memory_bytes: 512 * 1024 * 1024,
                disks: vec![],
                nics: vec![],
                storage_backend: String::new(),
                storage_size_bytes: 0,
            }),
            image_url: String::new(),
            image_sha256: String::new(),
            cloud_init_user_data: String::new(),
            image_path: "/var/lib/kcore/images/base.raw".to_string(),
            image_format: "raw".to_string(),
            ssh_key_names: vec![],
            storage_backend: controller_proto::StorageBackendType::Filesystem as i32,
            storage_size_bytes: 8 * 1024 * 1024 * 1024,
            target_dc: String::new(),
        };

        let err =
            <ControllerService as controller_proto::controller_server::Controller>::create_vm(
                &svc,
                Request::new(req),
            )
            .await
            .expect_err("preflight readiness check should fail");
        assert_eq!(err.code(), tonic::Code::Unavailable);
        assert!(err.message().contains("not ready"));
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
            None,
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

    #[tokio::test]
    async fn create_network_stores_vxlan_type_and_vni() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");

        let hook: PushHook = Arc::new(|_n: &NodeRow| Ok(()));
        let svc = ControllerService::new_with_test_push_hook(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            None,
            hook,
        );

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::create_network(
                &svc,
                Request::new(controller_proto::CreateNetworkRequest {
                    name: "overlay-1".to_string(),
                    external_ip: "203.0.113.10".to_string(),
                    gateway_ip: "10.250.0.1".to_string(),
                    internal_netmask: "255.255.255.0".to_string(),
                    target_node: node.id.clone(),
                    allowed_tcp_ports: vec![],
                    allowed_udp_ports: vec![],
                    vlan_id: 0,
                    network_type: "vxlan".to_string(),
                    enable_outbound_nat: true,
                }),
            )
            .await
            .expect("create vxlan network")
            .into_inner();

        assert!(resp.success);

        let net = db
            .get_network_for_node(&node.id, "overlay-1")
            .expect("get network")
            .expect("network exists");
        assert_eq!(net.network_type, "vxlan");
        assert!(net.vni >= 10000 && net.vni <= 15999, "vni={}", net.vni);
        assert!(net.enable_outbound_nat);
        assert_eq!(net.next_ip, 2);
    }

    #[tokio::test]
    async fn create_network_rejects_invalid_type() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");

        let hook: PushHook = Arc::new(|_n: &NodeRow| Ok(()));
        let svc = ControllerService::new_with_test_push_hook(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            None,
            hook,
        );

        let err =
            <ControllerService as controller_proto::controller_server::Controller>::create_network(
                &svc,
                Request::new(controller_proto::CreateNetworkRequest {
                    name: "bad-net".to_string(),
                    external_ip: "203.0.113.10".to_string(),
                    gateway_ip: "10.250.0.1".to_string(),
                    internal_netmask: "255.255.255.0".to_string(),
                    target_node: node.id.clone(),
                    allowed_tcp_ports: vec![],
                    allowed_udp_ports: vec![],
                    vlan_id: 0,
                    network_type: "wireguard".to_string(),
                    enable_outbound_nat: false,
                }),
            )
            .await
            .expect_err("invalid type should be rejected");

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn create_vm_allocates_ip_for_vxlan_network() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert node");

        let hook: PushHook = Arc::new(|_n: &NodeRow| Ok(()));
        let svc = ControllerService::new_with_test_push_hook(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            None,
            hook,
        );

        <ControllerService as controller_proto::controller_server::Controller>::create_network(
            &svc,
            Request::new(controller_proto::CreateNetworkRequest {
                name: "vx-net".to_string(),
                external_ip: "203.0.113.10".to_string(),
                gateway_ip: "10.250.0.1".to_string(),
                internal_netmask: "255.255.255.0".to_string(),
                target_node: node.id.clone(),
                allowed_tcp_ports: vec![],
                allowed_udp_ports: vec![],
                vlan_id: 0,
                network_type: "vxlan".to_string(),
                enable_outbound_nat: true,
            }),
        )
        .await
        .expect("create vxlan network");

        let create_resp =
            <ControllerService as controller_proto::controller_server::Controller>::create_vm(
                &svc,
                Request::new(controller_proto::CreateVmRequest {
                    spec: Some(controller_proto::VmSpec {
                        id: String::new(),
                        name: "app-1".to_string(),
                        cpu: 1,
                        memory_bytes: 512 * 1024 * 1024,
                        disks: vec![],
                        nics: vec![controller_proto::Nic {
                            network: "vx-net".to_string(),
                            model: String::new(),
                            mac_address: String::new(),
                        }],
                        storage_backend: String::new(),
                        storage_size_bytes: 0,
                    }),
                    image_url: "https://example.com/img.raw".to_string(),
                    image_sha256:
                        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                            .to_string(),
                    cloud_init_user_data: String::new(),
                    target_node: node.id.clone(),
                    ssh_key_names: vec![],
                    storage_backend: controller_proto::StorageBackendType::Filesystem as i32,
                    storage_size_bytes: 10 * 1024 * 1024 * 1024,
                    image_path: String::new(),
                    image_format: String::new(),
                    target_dc: String::new(),
                }),
            )
            .await
            .expect("create vm on vxlan network")
            .into_inner();

        let vm_id = create_resp.vm_id;
        let vm = db.get_vm(&vm_id).expect("get vm").expect("vm exists");
        assert_eq!(vm.vm_ip, "10.250.0.2");
    }

    #[tokio::test]
    async fn create_network_rejects_vxlan_on_disabled_node() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.disable_vxlan = true;
        db.upsert_node(&node).expect("insert node");

        let hook: PushHook = Arc::new(|_n: &NodeRow| Ok(()));
        let svc = ControllerService::new_with_test_push_hook(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            None,
            hook,
        );

        let err =
            <ControllerService as controller_proto::controller_server::Controller>::create_network(
                &svc,
                Request::new(controller_proto::CreateNetworkRequest {
                    name: "overlay-blocked".to_string(),
                    external_ip: "203.0.113.10".to_string(),
                    gateway_ip: "10.250.0.1".to_string(),
                    internal_netmask: "255.255.255.0".to_string(),
                    target_node: node.id.clone(),
                    allowed_tcp_ports: vec![],
                    allowed_udp_ports: vec![],
                    vlan_id: 0,
                    network_type: "vxlan".to_string(),
                    enable_outbound_nat: false,
                }),
            )
            .await
            .expect_err("vxlan should be rejected on disabled node");

        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
        assert!(err.message().contains("VXLAN is disabled"));
    }

    #[tokio::test]
    async fn create_network_allows_nat_on_vxlan_disabled_node() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.disable_vxlan = true;
        db.upsert_node(&node).expect("insert node");

        let hook: PushHook = Arc::new(|_n: &NodeRow| Ok(()));
        let svc = ControllerService::new_with_test_push_hook(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            None,
            hook,
        );

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::create_network(
                &svc,
                Request::new(controller_proto::CreateNetworkRequest {
                    name: "nat-allowed".to_string(),
                    external_ip: "203.0.113.10".to_string(),
                    gateway_ip: "10.250.0.1".to_string(),
                    internal_netmask: "255.255.255.0".to_string(),
                    target_node: node.id.clone(),
                    allowed_tcp_ports: vec![],
                    allowed_udp_ports: vec![],
                    vlan_id: 0,
                    network_type: "nat".to_string(),
                    enable_outbound_nat: false,
                }),
            )
            .await
            .expect("nat should succeed on vxlan-disabled node")
            .into_inner();

        assert!(resp.success);
    }

    #[tokio::test]
    async fn new_node_auto_approved_by_default() {
        let db = Database::open(":memory:").expect("open db");
        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            empty_sub_ca(),
            None,
            false,
        );

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::register_node(
                &svc,
                Request::new(controller_proto::RegisterNodeRequest {
                    node_id: "new-node".to_string(),
                    hostname: "new-node".to_string(),
                    address: "10.0.0.99:9091".to_string(),
                    capacity: Some(controller_proto::NodeCapacity {
                        cpu_cores: 4,
                        memory_bytes: 8_000_000_000,
                    }),
                    labels: vec![],
                    storage_backend: 1,
                    disable_vxlan: false,
                    cert_expiry_days: 365,
                    luks_method: String::new(),
                    dc_id: String::new(),
                }),
            )
            .await
            .expect("register should succeed")
            .into_inner();

        assert!(resp.success);
        assert_eq!(resp.approval_status, "approved");

        let node = db.get_node("new-node").expect("get").expect("exists");
        assert_eq!(node.approval_status, "approved");
        assert_eq!(node.status, "ready");
    }

    #[tokio::test]
    async fn new_node_pending_when_manual_approval_required() {
        let db = Database::open(":memory:").expect("open db");
        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            empty_sub_ca(),
            None,
            true,
        );

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::register_node(
                &svc,
                Request::new(controller_proto::RegisterNodeRequest {
                    node_id: "new-node".to_string(),
                    hostname: "new-node".to_string(),
                    address: "10.0.0.99:9091".to_string(),
                    capacity: Some(controller_proto::NodeCapacity {
                        cpu_cores: 4,
                        memory_bytes: 8_000_000_000,
                    }),
                    labels: vec![],
                    storage_backend: 1,
                    disable_vxlan: false,
                    cert_expiry_days: 365,
                    luks_method: String::new(),
                    dc_id: String::new(),
                }),
            )
            .await
            .expect("register should succeed")
            .into_inner();

        assert!(resp.success);
        assert!(resp.message.contains("pending"));

        let node = db.get_node("new-node").expect("get").expect("exists");
        assert_eq!(node.approval_status, "pending");
        assert_eq!(node.status, "pending");
    }

    #[tokio::test]
    async fn register_node_appends_replication_outbox_when_configured() {
        let db = Database::open(":memory:").expect("open db");
        let replication = Some(ReplicationConfig {
            controller_id: "ctrl-test".into(),
            dc_id: "DC1".into(),
            peers: vec![],
        });
        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            empty_sub_ca(),
            replication,
            false,
        );

        <ControllerService as controller_proto::controller_server::Controller>::register_node(
            &svc,
            Request::new(controller_proto::RegisterNodeRequest {
                node_id: "repl-node".to_string(),
                hostname: "repl-node".to_string(),
                address: "10.0.0.55:9091".to_string(),
                capacity: Some(controller_proto::NodeCapacity {
                    cpu_cores: 2,
                    memory_bytes: 4_000_000_000,
                }),
                labels: vec!["role=test".to_string()],
                storage_backend: 1,
                disable_vxlan: false,
                cert_expiry_days: 365,
                luks_method: String::new(),
                dc_id: String::new(),
            }),
        )
        .await
        .expect("register should succeed");

        assert_eq!(db.replication_outbox_len().expect("count"), 1);
    }

    #[tokio::test]
    async fn heartbeat_appends_replication_outbox_when_configured() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.approval_status = "approved".to_string();
        node.status = "ready".to_string();
        db.upsert_node(&node).expect("insert");
        let replication = Some(ReplicationConfig {
            controller_id: "ctrl-test".into(),
            dc_id: "DC1".into(),
            peers: vec![],
        });
        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            empty_sub_ca(),
            replication,
            false,
        );
        <ControllerService as controller_proto::controller_server::Controller>::heartbeat(
            &svc,
            Request::new(controller_proto::HeartbeatRequest {
                node_id: node.id.clone(),
                usage: Some(controller_proto::NodeUsage {
                    cpu_cores_used: 1,
                    memory_bytes_used: 1024,
                }),
                cert_expiry_days: 300,
                luks_method: "tpm2".to_string(),
            }),
        )
        .await
        .expect("heartbeat should succeed");

        assert_eq!(db.replication_outbox_len().expect("count"), 1);
        let rows = db
            .list_replication_outbox_since(0, 10)
            .expect("list outbox rows");
        assert_eq!(rows[0].event_type, EVT_NODE_HEARTBEAT);
    }

    #[tokio::test]
    async fn approved_node_re_registers_as_approved() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.approval_status = "approved".to_string();
        db.upsert_node(&node).expect("insert");

        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            empty_sub_ca(),
            None,
            false,
        );

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::register_node(
                &svc,
                Request::new(controller_proto::RegisterNodeRequest {
                    node_id: "node-1".to_string(),
                    hostname: "node-1".to_string(),
                    address: "127.0.0.1:9091".to_string(),
                    capacity: Some(controller_proto::NodeCapacity {
                        cpu_cores: 4,
                        memory_bytes: 8_000_000_000,
                    }),
                    labels: vec![],
                    storage_backend: 1,
                    disable_vxlan: false,
                    cert_expiry_days: 300,
                    luks_method: "tpm2".to_string(),
                    dc_id: String::new(),
                }),
            )
            .await
            .expect("re-register should succeed")
            .into_inner();

        assert!(resp.success);
        assert_eq!(resp.message, "registered");

        let n = db.get_node("node-1").expect("get").expect("exists");
        assert_eq!(n.approval_status, "approved");
        assert_eq!(n.status, "ready");
    }

    #[tokio::test]
    async fn approve_node_transitions_to_ready() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.approval_status = "pending".to_string();
        node.status = "pending".to_string();
        db.upsert_node(&node).expect("insert");

        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            empty_sub_ca(),
            None,
            false,
        );

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::approve_node(
                &svc,
                Request::new(controller_proto::ApproveNodeRequest {
                    node_id: "node-1".to_string(),
                }),
            )
            .await
            .expect("approve should succeed")
            .into_inner();

        assert!(resp.success);

        let n = db.get_node("node-1").expect("get").expect("exists");
        assert_eq!(n.approval_status, "approved");
        assert_eq!(n.status, "ready");
    }

    #[tokio::test]
    async fn reject_node_marks_rejected() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.approval_status = "pending".to_string();
        node.status = "pending".to_string();
        db.upsert_node(&node).expect("insert");

        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            empty_sub_ca(),
            None,
            false,
        );

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::reject_node(
                &svc,
                Request::new(controller_proto::RejectNodeRequest {
                    node_id: "node-1".to_string(),
                }),
            )
            .await
            .expect("reject should succeed")
            .into_inner();

        assert!(resp.success);

        let n = db.get_node("node-1").expect("get").expect("exists");
        assert_eq!(n.approval_status, "rejected");
        assert_eq!(n.status, "rejected");
    }

    #[test]
    fn scheduler_skips_pending_nodes() {
        let mut n = NodeRow {
            id: "pending-node".into(),
            hostname: "pending-node".into(),
            address: "10.0.0.1:9091".into(),
            cpu_cores: 8,
            memory_bytes: 16_000_000_000,
            status: "ready".into(),
            last_heartbeat: String::new(),
            gateway_interface: String::new(),
            cpu_used: 0,
            memory_used: 0,
            storage_backend: "filesystem".into(),
            disable_vxlan: false,
            approval_status: "pending".into(),
            cert_expiry_days: -1,
            luks_method: String::new(),
            dc_id: "DC1".to_string(),
        };
        assert!(
            scheduler::select_node(&[n.clone()]).is_none(),
            "pending node should not be selected"
        );

        n.approval_status = "approved".into();
        assert!(
            scheduler::select_node(&[n]).is_some(),
            "approved node should be selected"
        );
    }

    fn test_sub_ca_state() -> Arc<Mutex<SubCaState>> {
        use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair};
        use time::{Duration, OffsetDateTime};

        let mut ca_params = CertificateParams::default();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "test-ca");
        ca_params.not_before = OffsetDateTime::now_utc();
        ca_params.not_after = OffsetDateTime::now_utc() + Duration::days(3650);
        let ca_key = KeyPair::generate().unwrap();
        let ca_cert = ca_params.self_signed(&ca_key).unwrap();

        let mut sub_params = CertificateParams::default();
        sub_params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        sub_params
            .distinguished_name
            .push(DnType::CommonName, "test-sub-ca");
        sub_params.not_before = OffsetDateTime::now_utc();
        sub_params.not_after = OffsetDateTime::now_utc() + Duration::days(1825);
        let issuer = Issuer::from_ca_cert_pem(&ca_cert.pem(), ca_key).unwrap();
        let sub_key = KeyPair::generate().unwrap();
        let sub_cert = sub_params.signed_by(&sub_key, &issuer).unwrap();

        Arc::new(Mutex::new(SubCaState {
            cert_pem: sub_cert.pem(),
            key_pem: sub_key.serialize_pem(),
            cert_file: None,
            key_file: None,
        }))
    }

    #[tokio::test]
    async fn renew_node_cert_returns_chain() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert");

        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            test_sub_ca_state(),
            None,
            false,
        );

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::renew_node_cert(
                &svc,
                Request::new(controller_proto::RenewNodeCertRequest {
                    node_id: "node-1".to_string(),
                }),
            )
            .await
            .expect("renew should succeed")
            .into_inner();

        assert!(resp.success);
        assert!(resp.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(resp.key_pem.contains("BEGIN PRIVATE KEY"));
        let cert_count = resp.cert_pem.matches("BEGIN CERTIFICATE").count();
        assert_eq!(cert_count, 2, "should contain leaf + sub-CA in chain");
    }

    #[tokio::test]
    async fn renew_node_cert_rejects_unapproved_node() {
        let db = Database::open(":memory:").expect("open db");
        let mut node = test_node();
        node.approval_status = "pending".to_string();
        db.upsert_node(&node).expect("insert");

        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            test_sub_ca_state(),
            None,
            false,
        );

        let err =
            <ControllerService as controller_proto::controller_server::Controller>::renew_node_cert(
                &svc,
                Request::new(controller_proto::RenewNodeCertRequest {
                    node_id: "node-1".to_string(),
                }),
            )
            .await
            .expect_err("should reject unapproved node");

        assert_eq!(err.code(), tonic::Code::PermissionDenied);
    }

    #[tokio::test]
    async fn renew_node_cert_fails_without_sub_ca() {
        let db = Database::open(":memory:").expect("open db");
        let node = test_node();
        db.upsert_node(&node).expect("insert");

        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            empty_sub_ca(),
            None,
            false,
        );

        let err =
            <ControllerService as controller_proto::controller_server::Controller>::renew_node_cert(
                &svc,
                Request::new(controller_proto::RenewNodeCertRequest {
                    node_id: "node-1".to_string(),
                }),
            )
            .await
            .expect_err("should fail without sub-CA");

        assert_eq!(err.code(), tonic::Code::Unavailable);
    }

    #[tokio::test]
    async fn rotate_sub_ca_updates_state() {
        let db = Database::open(":memory:").expect("open db");
        let sub_ca = test_sub_ca_state();

        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            sub_ca.clone(),
            None,
            false,
        );

        let new_state = test_sub_ca_state();
        let new_lock = new_state.lock().unwrap();
        let new_cert = new_lock.cert_pem.clone();
        let new_key = new_lock.key_pem.clone();
        drop(new_lock);

        let resp =
            <ControllerService as controller_proto::controller_server::Controller>::rotate_sub_ca(
                &svc,
                Request::new(controller_proto::RotateSubCaRequest {
                    sub_ca_cert_pem: new_cert.clone(),
                    sub_ca_key_pem: new_key.clone(),
                }),
            )
            .await
            .expect("rotate should succeed")
            .into_inner();

        assert!(resp.success);

        let current = sub_ca.lock().unwrap();
        assert_eq!(current.cert_pem, new_cert);
        assert_eq!(current.key_pem, new_key);
    }

    #[tokio::test]
    async fn rotate_sub_ca_rejects_empty_cert() {
        let db = Database::open(":memory:").expect("open db");
        let svc = ControllerService::new(
            db.clone(),
            NodeClients::new(None),
            test_network(),
            empty_sub_ca(),
            None,
            false,
        );

        let err =
            <ControllerService as controller_proto::controller_server::Controller>::rotate_sub_ca(
                &svc,
                Request::new(controller_proto::RotateSubCaRequest {
                    sub_ca_cert_pem: String::new(),
                    sub_ca_key_pem: String::new(),
                }),
            )
            .await
            .expect_err("should reject empty cert");

        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }
}
