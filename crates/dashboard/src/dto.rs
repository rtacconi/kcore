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
    pub approval_status: String,
    pub cert_expiry_days: i32,
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
