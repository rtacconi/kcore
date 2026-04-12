//! Serializable view models for server functions and tests.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessControlEntryDto {
    pub rpc_method: String,
    pub allowed_identities: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeSummaryDto {
    pub node_id: String,
    pub hostname: String,
    pub address: String,
    pub dc_id: String,
    pub approval_status: String,
    pub cert_expiry_days: i32,
    pub luks_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComplianceDto {
    pub controller_version: String,
    pub crypto_library: String,
    pub tls13_cipher_suites: Vec<String>,
    pub tls12_cipher_suites: Vec<String>,
    pub kx_groups: Vec<String>,
    pub excluded_algorithms: Vec<String>,
    pub mtls_enabled: bool,
    pub access_control: Vec<AccessControlEntryDto>,
    pub total_nodes: i32,
    pub approved_nodes: i32,
    pub pending_nodes: i32,
    pub rejected_nodes: i32,
    pub total_vms: i32,
    pub running_vms: i32,
    pub stopped_vms: i32,
    pub total_networks: i32,
    pub nat_networks: i32,
    pub bridge_networks: i32,
    pub vxlan_networks: i32,
    pub sub_ca_enabled: bool,
    pub cert_auto_renewal_days: i32,
    pub nodes_expiring_30d: i32,
    pub nodes_cert_unknown: i32,
    pub nodes: Vec<NodeSummaryDto>,
    pub nodes_luks_tpm2: i32,
    pub nodes_luks_keyfile: i32,
    pub nodes_luks_unknown: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VmRowDto {
    pub id: String,
    pub name: String,
    pub state: String,
    pub cpu: i32,
    pub memory: String,
    pub node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VmsPageDto {
    pub vms: Vec<VmRowDto>,
    pub page: u32,
    pub page_size: usize,
    pub total: usize,
    pub total_pages: u32,
    pub has_prev: bool,
    pub has_next: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkRowDto {
    pub name: String,
    pub network_type: String,
    pub node_id: String,
    pub external_ip: String,
    pub gateway_ip: String,
    pub internal_netmask: String,
    pub vlan_id: i32,
    pub enable_outbound_nat: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostInterfaceDto {
    pub name: String,
    pub mac_address: String,
    pub state: String,
    pub mtu: i32,
    pub addresses: Vec<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeNetworkDto {
    pub node_id: String,
    pub hostname: String,
    pub address: String,
    pub gateway_interface: String,
    pub disable_vxlan: bool,
    pub interfaces: Vec<HostInterfaceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkOverviewDto {
    pub default_gateway_interface: String,
    pub default_external_ip: String,
    pub default_gateway_ip: String,
    pub default_internal_netmask: String,
    pub nodes: Vec<NodeNetworkDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageDiskRowDto {
    pub name: String,
    pub path: String,
    pub size: String,
    pub model: String,
    pub fstype: String,
    pub mountpoint: String,
    pub role_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeStorageDto {
    pub node_id: String,
    pub hostname: String,
    pub address: String,
    pub storage_backend: String,
    pub luks_method: String,
    pub disk_inventory_ok: bool,
    pub disks: Vec<StorageDiskRowDto>,
    pub lvm_inventory_ok: bool,
    pub lvm_volume_groups: Vec<LvmVolumeGroupDto>,
    pub lvm_logical_volumes: Vec<LvmLogicalVolumeDto>,
    pub lvm_physical_volumes: Vec<LvmPhysicalVolumeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageOverviewDto {
    pub approved_nodes: i32,
    pub nodes_disk_inventory_ok: i32,
    pub backend_filesystem_nodes: i32,
    pub backend_lvm_nodes: i32,
    pub backend_zfs_nodes: i32,
    pub backend_unspecified_nodes: i32,
    pub nodes_luks_tpm2: i32,
    pub nodes_luks_keyfile: i32,
    pub nodes_luks_unknown: i32,
    pub total_block_devices: i32,
    pub nodes: Vec<NodeStorageDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplicationOutgoingDto {
    pub peer_id: String,
    pub last_acked_event_id: i64,
    pub lag_events: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplicationIncomingDto {
    pub peer_endpoint: String,
    pub last_pulled_event_id: i64,
    pub last_applied_event_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplicationStatusDto {
    pub outbox_head_event_id: i64,
    pub outbox_size: i64,
    pub outgoing: Vec<ReplicationOutgoingDto>,
    pub incoming: Vec<ReplicationIncomingDto>,
    pub unresolved_conflicts: i64,
    pub pending_compensation_jobs: i64,
    pub failed_compensation_jobs: i64,
    pub materialization_backlog: i64,
    pub failed_reservations: i64,
    pub failed_retryable_reservations: i64,
    pub failed_non_retryable_reservations: i64,
    pub retry_exhausted_reservations: i64,
    pub zero_manual_slo_healthy: bool,
    pub zero_manual_slo_violations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplicationConflictDto {
    pub id: i64,
    pub resource_key: String,
    pub incumbent_op_id: String,
    pub challenger_op_id: String,
    pub incumbent_controller_id: String,
    pub challenger_controller_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LvmVolumeGroupDto {
    pub name: String,
    pub size: String,
    pub free: String,
    pub attr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LvmLogicalVolumeDto {
    pub name: String,
    pub vg_name: String,
    pub size: String,
    pub attr: String,
    pub path: String,
    pub pool: String,
    pub data_percent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LvmPhysicalVolumeDto {
    pub name: String,
    pub vg_name: String,
    pub size: String,
    pub free: String,
    pub attr: String,
}
