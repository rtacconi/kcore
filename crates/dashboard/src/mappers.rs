//! gRPC proto → dashboard DTO (testable without the network).

use crate::controller_client::controller_proto;
use crate::dto::{
    AccessControlEntryDto, ComplianceDto, HostInterfaceDto, NetworkOverviewDto, NetworkRowDto,
    NodeNetworkDto, NodeStorageDto, NodeSummaryDto, ReplicationStatusDto, StorageDiskRowDto,
    StorageOverviewDto, VmRowDto, VmsPageDto,
};
use crate::format::{self, paginate_by_name, VM_PAGE_SIZE};

pub fn compliance_from_proto(r: controller_proto::GetComplianceReportResponse) -> ComplianceDto {
    ComplianceDto {
        controller_version: r.controller_version,
        crypto_library: r.crypto_library,
        tls13_cipher_suites: r.tls13_cipher_suites,
        tls12_cipher_suites: r.tls12_cipher_suites,
        kx_groups: r.kx_groups,
        excluded_algorithms: r.excluded_algorithms,
        mtls_enabled: r.mtls_enabled,
        access_control: r
            .access_control
            .into_iter()
            .map(|e| AccessControlEntryDto {
                rpc_method: e.rpc_method,
                allowed_identities: e.allowed_identities,
            })
            .collect(),
        total_nodes: r.total_nodes,
        approved_nodes: r.approved_nodes,
        pending_nodes: r.pending_nodes,
        rejected_nodes: r.rejected_nodes,
        total_vms: r.total_vms,
        running_vms: r.running_vms,
        stopped_vms: r.stopped_vms,
        total_networks: r.total_networks,
        nat_networks: r.nat_networks,
        bridge_networks: r.bridge_networks,
        vxlan_networks: r.vxlan_networks,
        sub_ca_enabled: r.sub_ca_enabled,
        cert_auto_renewal_days: r.cert_auto_renewal_days,
        nodes_expiring_30d: r.nodes_expiring_30d,
        nodes_cert_unknown: r.nodes_cert_unknown,
        nodes: r
            .nodes
            .into_iter()
            .map(|n| NodeSummaryDto {
                node_id: n.node_id,
                hostname: n.hostname,
                address: n.address,
                approval_status: n.approval_status,
                cert_expiry_days: n.cert_expiry_days,
                luks_method: n.luks_method,
            })
            .collect(),
        nodes_luks_tpm2: r.nodes_luks_tpm2,
        nodes_luks_keyfile: r.nodes_luks_keyfile,
        nodes_luks_unknown: r.nodes_luks_unknown,
    }
}

pub fn vms_page_from_proto(vms: Vec<controller_proto::VmInfo>, page: u32) -> VmsPageDto {
    let rows: Vec<VmRowDto> = vms
        .into_iter()
        .map(|v| VmRowDto {
            id: v.id,
            name: v.name.clone(),
            state: format::vm_state_label(v.state).to_string(),
            cpu: v.cpu,
            memory: format::memory_mebibytes(v.memory_bytes),
            node_id: v.node_id,
        })
        .collect();
    let page = page.max(1);
    let pv = paginate_by_name(rows, |r| r.name.clone(), page, VM_PAGE_SIZE);
    VmsPageDto {
        total_pages: pv.total_pages(),
        has_prev: pv.has_prev(),
        has_next: pv.has_next(),
        page: pv.page,
        page_size: pv.page_size,
        total: pv.total,
        vms: pv.items,
    }
}

fn classify_interface(name: &str) -> &'static str {
    if name == "lo" {
        "loopback"
    } else if name.starts_with("br-")
        || name.starts_with("kbr-")
        || name.starts_with("virbr")
        || name == "br0"
    {
        "bridge"
    } else if name.starts_with("tap-") || name.starts_with("vnet") {
        "tap"
    } else if name.contains('.') || name.starts_with("vlan") {
        "vlan"
    } else if name.starts_with("vxlan") || name.starts_with("kvx-") {
        "vxlan"
    } else if name.starts_with("eno")
        || name.starts_with("eth")
        || name.starts_with("enp")
        || name.starts_with("ens")
    {
        "physical"
    } else if name.starts_with("bond") {
        "bond"
    } else if name.starts_with("docker") || name.starts_with("veth") {
        "container"
    } else {
        "other"
    }
}

fn disk_role_hint(mountpoint: &str, fstype: &str) -> String {
    let mp = mountpoint.trim();
    let fs = fstype.trim();
    if mp == "/" {
        return "OS (root)".into();
    }
    if mp.starts_with("/boot") {
        return "OS (boot)".into();
    }
    if mp.starts_with("/nix") {
        return "OS (Nix store)".into();
    }
    if mp.contains("kcore") {
        return "Data (kcore)".into();
    }
    if !mp.is_empty() {
        return "Mounted".into();
    }
    if fs.eq_ignore_ascii_case("zfs") {
        return "ZFS component".into();
    }
    if fs.contains("LVM") || fs.eq_ignore_ascii_case("lvm2_member") {
        return "LVM PV".into();
    }
    if !fs.is_empty() {
        return "See partitions".into();
    }
    "Block device".into()
}

pub fn storage_overview_from_proto(
    r: controller_proto::GetStorageOverviewResponse,
) -> StorageOverviewDto {
    StorageOverviewDto {
        approved_nodes: r.approved_nodes,
        nodes_disk_inventory_ok: r.nodes_disk_inventory_ok,
        backend_filesystem_nodes: r.backend_filesystem_nodes,
        backend_lvm_nodes: r.backend_lvm_nodes,
        backend_zfs_nodes: r.backend_zfs_nodes,
        backend_unspecified_nodes: r.backend_unspecified_nodes,
        nodes_luks_tpm2: r.nodes_luks_tpm2,
        nodes_luks_keyfile: r.nodes_luks_keyfile,
        nodes_luks_unknown: r.nodes_luks_unknown,
        total_block_devices: r.total_block_devices,
        nodes: r
            .nodes
            .into_iter()
            .map(|n| NodeStorageDto {
                node_id: n.node_id,
                hostname: n.hostname,
                address: n.address,
                storage_backend: format::storage_backend_label(n.storage_backend).to_string(),
                luks_method: n.luks_method,
                disk_inventory_ok: n.disk_inventory_ok,
                disks: n
                    .disks
                    .into_iter()
                    .map(|d| {
                        let role_hint = disk_role_hint(&d.mountpoint, &d.fstype);
                        StorageDiskRowDto {
                            name: d.name,
                            path: d.path,
                            size: d.size,
                            model: d.model,
                            fstype: d.fstype,
                            mountpoint: d.mountpoint,
                            role_hint,
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub fn network_overview_from_proto(
    r: controller_proto::GetNetworkOverviewResponse,
) -> NetworkOverviewDto {
    NetworkOverviewDto {
        default_gateway_interface: r.default_gateway_interface,
        default_external_ip: r.default_external_ip,
        default_gateway_ip: r.default_gateway_ip,
        default_internal_netmask: r.default_internal_netmask,
        nodes: r
            .nodes
            .into_iter()
            .map(|n| NodeNetworkDto {
                node_id: n.node_id,
                hostname: n.hostname,
                address: n.address,
                gateway_interface: n.gateway_interface,
                disable_vxlan: n.disable_vxlan,
                interfaces: n
                    .interfaces
                    .into_iter()
                    .map(|i| {
                        let kind = classify_interface(&i.name).to_string();
                        HostInterfaceDto {
                            name: i.name,
                            mac_address: i.mac_address,
                            state: i.state,
                            mtu: i.mtu,
                            addresses: i.addresses,
                            kind,
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub fn networks_from_proto(nets: Vec<controller_proto::NetworkInfo>) -> Vec<NetworkRowDto> {
    let mut rows: Vec<NetworkRowDto> = nets
        .into_iter()
        .map(|n| NetworkRowDto {
            name: n.name.clone(),
            network_type: n.network_type,
            node_id: n.node_id,
            external_ip: n.external_ip,
            gateway_ip: n.gateway_ip,
            internal_netmask: n.internal_netmask,
            vlan_id: n.vlan_id,
            enable_outbound_nat: n.enable_outbound_nat,
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

pub fn replication_status_from_proto(
    r: controller_proto::GetReplicationStatusResponse,
) -> ReplicationStatusDto {
    ReplicationStatusDto {
        unresolved_conflicts: r.unresolved_conflicts,
        pending_compensation_jobs: r.pending_compensation_jobs,
        failed_compensation_jobs: r.failed_compensation_jobs,
        materialization_backlog: r.materialization_backlog,
        failed_reservations: r.failed_reservations,
        failed_retryable_reservations: r.failed_retryable_reservations,
        failed_non_retryable_reservations: r.failed_non_retryable_reservations,
        retry_exhausted_reservations: r.retry_exhausted_reservations,
        zero_manual_slo_healthy: r.zero_manual_slo_healthy,
        zero_manual_slo_violations: r.zero_manual_slo_violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compliance_maps_core_fields() {
        let r = controller_proto::GetComplianceReportResponse {
            controller_version: "9.9.9".into(),
            crypto_library: "test-lib".into(),
            tls13_cipher_suites: vec!["A".into()],
            tls12_cipher_suites: vec!["B".into()],
            kx_groups: vec!["P-256".into()],
            excluded_algorithms: vec!["X".into()],
            mtls_enabled: true,
            access_control: vec![controller_proto::AccessControlEntry {
                rpc_method: "ListVms".into(),
                allowed_identities: "kcore-kctl".into(),
            }],
            total_nodes: 2,
            approved_nodes: 1,
            pending_nodes: 1,
            rejected_nodes: 0,
            total_vms: 3,
            running_vms: 2,
            stopped_vms: 1,
            total_networks: 4,
            nat_networks: 2,
            bridge_networks: 1,
            vxlan_networks: 1,
            sub_ca_enabled: false,
            cert_auto_renewal_days: 30,
            nodes_expiring_30d: 0,
            nodes_cert_unknown: 0,
            nodes: vec![controller_proto::NodeInfo {
                node_id: "n1".into(),
                hostname: "h1".into(),
                address: "10.0.0.1:9091".into(),
                capacity: None,
                usage: None,
                status: "ready".into(),
                last_heartbeat: None,
                labels: vec![],
                storage_backend: 0,
                disable_vxlan: false,
                approval_status: "approved".into(),
                cert_expiry_days: 90,
                luks_method: "tpm2".into(),
            }],
            nodes_luks_tpm2: 1,
            nodes_luks_keyfile: 0,
            nodes_luks_unknown: 0,
        };
        let d = compliance_from_proto(r);
        assert_eq!(d.controller_version, "9.9.9");
        assert_eq!(d.total_vms, 3);
        assert_eq!(d.nodes.len(), 1);
        assert_eq!(d.nodes[0].hostname, "h1");
        assert_eq!(d.access_control[0].rpc_method, "ListVms");
    }

    #[test]
    fn vms_page_slices_and_sorts() {
        let vms = vec![
            controller_proto::VmInfo {
                id: "1".into(),
                name: "zebra".into(),
                state: 2,
                cpu: 2,
                memory_bytes: 1024 * 1024 * 1024,
                node_id: "n".into(),
                created_at: None,
            },
            controller_proto::VmInfo {
                id: "2".into(),
                name: "alpha".into(),
                state: 1,
                cpu: 1,
                memory_bytes: 512 * 1024 * 1024,
                node_id: "n".into(),
                created_at: None,
            },
        ];
        let p = vms_page_from_proto(vms, 1);
        assert_eq!(p.total, 2);
        assert_eq!(p.vms.len(), 2);
        assert_eq!(p.vms[0].name, "alpha");
        assert_eq!(p.vms[0].state, "Stopped");
        assert_eq!(p.vms[1].state, "Running");
    }

    #[test]
    fn networks_sort_by_name() {
        let nets = vec![
            controller_proto::NetworkInfo {
                name: "b-net".into(),
                external_ip: "".into(),
                gateway_ip: "".into(),
                internal_netmask: "".into(),
                node_id: "n".into(),
                allowed_tcp_ports: vec![],
                allowed_udp_ports: vec![],
                vlan_id: 0,
                network_type: "nat".into(),
                enable_outbound_nat: true,
            },
            controller_proto::NetworkInfo {
                name: "a-net".into(),
                external_ip: "".into(),
                gateway_ip: "".into(),
                internal_netmask: "".into(),
                node_id: "n".into(),
                allowed_tcp_ports: vec![],
                allowed_udp_ports: vec![],
                vlan_id: 0,
                network_type: "bridge".into(),
                enable_outbound_nat: false,
            },
        ];
        let rows = networks_from_proto(nets);
        assert_eq!(rows[0].name, "a-net");
        assert_eq!(rows[1].name, "b-net");
    }

    #[test]
    fn classify_interface_identifies_types() {
        assert_eq!(classify_interface("lo"), "loopback");
        assert_eq!(classify_interface("eno1"), "physical");
        assert_eq!(classify_interface("eth0"), "physical");
        assert_eq!(classify_interface("enp3s0"), "physical");
        assert_eq!(classify_interface("ens5"), "physical");
        assert_eq!(classify_interface("br-default"), "bridge");
        assert_eq!(classify_interface("kbr-net1"), "bridge");
        assert_eq!(classify_interface("virbr0"), "bridge");
        assert_eq!(classify_interface("br0"), "bridge");
        assert_eq!(classify_interface("tap-vm1"), "tap");
        assert_eq!(classify_interface("vnet0"), "tap");
        assert_eq!(classify_interface("eth0.100"), "vlan");
        assert_eq!(classify_interface("vlan42"), "vlan");
        assert_eq!(classify_interface("vxlan100"), "vxlan");
        assert_eq!(classify_interface("kvx-overlay"), "vxlan");
        assert_eq!(classify_interface("bond0"), "bond");
        assert_eq!(classify_interface("docker0"), "container");
        assert_eq!(classify_interface("veth1234"), "container");
        assert_eq!(classify_interface("wlan0"), "other");
    }

    #[test]
    fn network_overview_maps_and_classifies() {
        let r = controller_proto::GetNetworkOverviewResponse {
            default_gateway_interface: "eno1".into(),
            default_external_ip: "203.0.113.10".into(),
            default_gateway_ip: "10.0.0.1".into(),
            default_internal_netmask: "255.255.255.0".into(),
            nodes: vec![controller_proto::NodeNetworkInfo {
                node_id: "n1".into(),
                hostname: "host1".into(),
                address: "10.0.0.1:9443".into(),
                gateway_interface: "eno1".into(),
                disable_vxlan: false,
                interfaces: vec![
                    controller_proto::NetworkInterfaceDetail {
                        name: "eno1".into(),
                        mac_address: "aa:bb:cc:dd:ee:01".into(),
                        state: "UP".into(),
                        mtu: 1500,
                        addresses: vec!["10.0.0.1/24".into()],
                    },
                    controller_proto::NetworkInterfaceDetail {
                        name: "br-net".into(),
                        mac_address: "aa:bb:cc:dd:ee:02".into(),
                        state: "UP".into(),
                        mtu: 1500,
                        addresses: vec![],
                    },
                ],
            }],
        };
        let dto = network_overview_from_proto(r);
        assert_eq!(dto.default_external_ip, "203.0.113.10");
        assert_eq!(dto.nodes.len(), 1);
        assert_eq!(dto.nodes[0].interfaces.len(), 2);
        assert_eq!(dto.nodes[0].interfaces[0].kind, "physical");
        assert_eq!(dto.nodes[0].interfaces[1].kind, "bridge");
    }

    #[test]
    fn storage_overview_maps_nodes_and_hints() {
        let r = controller_proto::GetStorageOverviewResponse {
            approved_nodes: 1,
            nodes_disk_inventory_ok: 1,
            backend_filesystem_nodes: 1,
            backend_lvm_nodes: 0,
            backend_zfs_nodes: 0,
            backend_unspecified_nodes: 0,
            nodes_luks_tpm2: 0,
            nodes_luks_keyfile: 0,
            nodes_luks_unknown: 1,
            total_block_devices: 1,
            nodes: vec![controller_proto::NodeStorageOverview {
                node_id: "n1".into(),
                hostname: "h1".into(),
                address: "10.0.0.1:1".into(),
                storage_backend: controller_proto::StorageBackendType::Zfs as i32,
                luks_method: String::new(),
                disk_inventory_ok: true,
                disks: vec![controller_proto::StorageDiskDetail {
                    name: "sdx".into(),
                    path: "/dev/sdx".into(),
                    size: "1G".into(),
                    model: String::new(),
                    fstype: String::new(),
                    mountpoint: "/".into(),
                }],
            }],
        };
        let dto = storage_overview_from_proto(r);
        assert_eq!(dto.nodes.len(), 1);
        assert_eq!(dto.nodes[0].storage_backend, "ZFS");
        assert_eq!(dto.nodes[0].disks[0].role_hint, "OS (root)");
    }

    #[test]
    fn replication_status_maps_fields() {
        let r = controller_proto::GetReplicationStatusResponse {
            outbox_head_event_id: 10,
            outbox_size: 20,
            outgoing: vec![],
            incoming: vec![],
            unresolved_conflicts: 1,
            pending_compensation_jobs: 2,
            failed_compensation_jobs: 3,
            materialization_backlog: 4,
            oldest_unresolved_conflict_age_seconds: 5,
            failed_reservations: 6,
            zero_manual_slo_healthy: false,
            zero_manual_slo_violations: vec!["failed_reservations=6".into()],
            failed_retryable_reservations: 7,
            failed_non_retryable_reservations: 8,
            retry_exhausted_reservations: 9,
        };
        let dto = replication_status_from_proto(r);
        assert_eq!(dto.unresolved_conflicts, 1);
        assert_eq!(dto.failed_retryable_reservations, 7);
        assert_eq!(dto.retry_exhausted_reservations, 9);
        assert!(!dto.zero_manual_slo_healthy);
    }
}
