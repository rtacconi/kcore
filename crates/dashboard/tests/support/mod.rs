//! Shared mock controller for HTTP integration tests.
#![allow(dead_code)]

use std::net::SocketAddr;

use kcore_dashboard::controller_client::controller_proto::controller_admin_server::ControllerAdmin;
use kcore_dashboard::controller_client::controller_proto::controller_server::Controller;
use kcore_dashboard::controller_client::controller_proto::{
    controller_admin_server::ControllerAdminServer, controller_server::ControllerServer, *,
};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

#[derive(Clone, Default)]
pub struct MockController;

fn unimp(name: &'static str) -> Status {
    Status::unimplemented(name)
}

#[tonic::async_trait]
impl Controller for MockController {
    async fn register_node(
        &self,
        _: Request<RegisterNodeRequest>,
    ) -> Result<Response<RegisterNodeResponse>, Status> {
        Err(unimp("register_node"))
    }

    async fn heartbeat(
        &self,
        _: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        Err(unimp("heartbeat"))
    }

    async fn sync_vm_state(
        &self,
        _: Request<SyncVmStateRequest>,
    ) -> Result<Response<SyncVmStateResponse>, Status> {
        Err(unimp("sync_vm_state"))
    }

    async fn sync_workload_state(
        &self,
        _: Request<SyncWorkloadStateRequest>,
    ) -> Result<Response<SyncWorkloadStateResponse>, Status> {
        Err(unimp("sync_workload_state"))
    }

    async fn create_vm(
        &self,
        _: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        Err(unimp("create_vm"))
    }

    async fn update_vm(
        &self,
        _: Request<UpdateVmRequest>,
    ) -> Result<Response<UpdateVmResponse>, Status> {
        Err(unimp("update_vm"))
    }

    async fn delete_vm(
        &self,
        _: Request<DeleteVmRequest>,
    ) -> Result<Response<DeleteVmResponse>, Status> {
        Err(unimp("delete_vm"))
    }

    async fn set_vm_desired_state(
        &self,
        _: Request<SetVmDesiredStateRequest>,
    ) -> Result<Response<SetVmDesiredStateResponse>, Status> {
        Err(unimp("set_vm_desired_state"))
    }

    async fn get_vm(&self, _: Request<GetVmRequest>) -> Result<Response<GetVmResponse>, Status> {
        Err(unimp("get_vm"))
    }

    async fn list_vms(
        &self,
        _: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        Ok(Response::new(ListVmsResponse {
            vms: vec![VmInfo {
                id: "vm-mock-1".into(),
                name: "mock-vm-alpha".into(),
                state: VmState::Running as i32,
                cpu: 2,
                memory_bytes: 512 * 1024 * 1024,
                node_id: "node-mock-a".into(),
                created_at: None,
                storage_backend: String::new(),
                storage_size_bytes: 0,
            }],
        }))
    }

    async fn create_workload(
        &self,
        _: Request<CreateWorkloadRequest>,
    ) -> Result<Response<CreateWorkloadResponse>, Status> {
        Err(unimp("create_workload"))
    }

    async fn delete_workload(
        &self,
        _: Request<DeleteWorkloadRequest>,
    ) -> Result<Response<DeleteWorkloadResponse>, Status> {
        Err(unimp("delete_workload"))
    }

    async fn set_workload_desired_state(
        &self,
        _: Request<SetWorkloadDesiredStateRequest>,
    ) -> Result<Response<SetWorkloadDesiredStateResponse>, Status> {
        Err(unimp("set_workload_desired_state"))
    }

    async fn get_workload(
        &self,
        _: Request<GetWorkloadRequest>,
    ) -> Result<Response<GetWorkloadResponse>, Status> {
        Err(unimp("get_workload"))
    }

    async fn list_workloads(
        &self,
        _: Request<ListWorkloadsRequest>,
    ) -> Result<Response<ListWorkloadsResponse>, Status> {
        Err(unimp("list_workloads"))
    }

    async fn create_network(
        &self,
        _: Request<CreateNetworkRequest>,
    ) -> Result<Response<CreateNetworkResponse>, Status> {
        Err(unimp("create_network"))
    }

    async fn delete_network(
        &self,
        _: Request<DeleteNetworkRequest>,
    ) -> Result<Response<DeleteNetworkResponse>, Status> {
        Err(unimp("delete_network"))
    }

    async fn list_networks(
        &self,
        _: Request<ListNetworksRequest>,
    ) -> Result<Response<ListNetworksResponse>, Status> {
        Ok(Response::new(ListNetworksResponse {
            networks: vec![NetworkInfo {
                name: "mock-net-stub".into(),
                external_ip: "203.0.113.1".into(),
                gateway_ip: "10.0.0.1".into(),
                internal_netmask: "255.255.255.0".into(),
                node_id: "node-mock-a".into(),
                allowed_tcp_ports: vec![],
                allowed_udp_ports: vec![],
                vlan_id: 0,
                network_type: "nat".into(),
                enable_outbound_nat: true,
            }],
        }))
    }

    async fn create_security_group(
        &self,
        _: Request<CreateSecurityGroupRequest>,
    ) -> Result<Response<CreateSecurityGroupResponse>, Status> {
        Err(unimp("create_security_group"))
    }

    async fn get_security_group(
        &self,
        _: Request<GetSecurityGroupRequest>,
    ) -> Result<Response<GetSecurityGroupResponse>, Status> {
        Err(unimp("get_security_group"))
    }

    async fn list_security_groups(
        &self,
        _: Request<ListSecurityGroupsRequest>,
    ) -> Result<Response<ListSecurityGroupsResponse>, Status> {
        Err(unimp("list_security_groups"))
    }

    async fn delete_security_group(
        &self,
        _: Request<DeleteSecurityGroupRequest>,
    ) -> Result<Response<DeleteSecurityGroupResponse>, Status> {
        Err(unimp("delete_security_group"))
    }

    async fn attach_security_group(
        &self,
        _: Request<AttachSecurityGroupRequest>,
    ) -> Result<Response<AttachSecurityGroupResponse>, Status> {
        Err(unimp("attach_security_group"))
    }

    async fn detach_security_group(
        &self,
        _: Request<DetachSecurityGroupRequest>,
    ) -> Result<Response<DetachSecurityGroupResponse>, Status> {
        Err(unimp("detach_security_group"))
    }

    async fn list_nodes(
        &self,
        _: Request<ListNodesRequest>,
    ) -> Result<Response<ListNodesResponse>, Status> {
        Err(unimp("list_nodes"))
    }

    async fn get_node(
        &self,
        _: Request<GetNodeRequest>,
    ) -> Result<Response<GetNodeResponse>, Status> {
        Err(unimp("get_node"))
    }

    async fn create_ssh_key(
        &self,
        _: Request<CreateSshKeyRequest>,
    ) -> Result<Response<CreateSshKeyResponse>, Status> {
        Err(unimp("create_ssh_key"))
    }

    async fn delete_ssh_key(
        &self,
        _: Request<DeleteSshKeyRequest>,
    ) -> Result<Response<DeleteSshKeyResponse>, Status> {
        Err(unimp("delete_ssh_key"))
    }

    async fn list_ssh_keys(
        &self,
        _: Request<ListSshKeysRequest>,
    ) -> Result<Response<ListSshKeysResponse>, Status> {
        Err(unimp("list_ssh_keys"))
    }

    async fn get_ssh_key(
        &self,
        _: Request<GetSshKeyRequest>,
    ) -> Result<Response<GetSshKeyResponse>, Status> {
        Err(unimp("get_ssh_key"))
    }

    async fn drain_node(
        &self,
        _: Request<DrainNodeRequest>,
    ) -> Result<Response<DrainNodeResponse>, Status> {
        Err(unimp("drain_node"))
    }

    async fn approve_node(
        &self,
        _: Request<ApproveNodeRequest>,
    ) -> Result<Response<ApproveNodeResponse>, Status> {
        Err(unimp("approve_node"))
    }

    async fn reject_node(
        &self,
        _: Request<RejectNodeRequest>,
    ) -> Result<Response<RejectNodeResponse>, Status> {
        Err(unimp("reject_node"))
    }

    async fn renew_node_cert(
        &self,
        _: Request<RenewNodeCertRequest>,
    ) -> Result<Response<RenewNodeCertResponse>, Status> {
        Err(unimp("renew_node_cert"))
    }

    async fn issue_node_bootstrap_cert(
        &self,
        _: Request<IssueNodeBootstrapCertRequest>,
    ) -> Result<Response<IssueNodeBootstrapCertResponse>, Status> {
        Err(unimp("issue_node_bootstrap_cert"))
    }

    async fn rotate_sub_ca(
        &self,
        _: Request<RotateSubCaRequest>,
    ) -> Result<Response<RotateSubCaResponse>, Status> {
        Err(unimp("rotate_sub_ca"))
    }

    async fn reload_tls(
        &self,
        _: Request<ReloadTlsRequest>,
    ) -> Result<Response<ReloadTlsResponse>, Status> {
        Err(unimp("reload_tls"))
    }

    async fn get_network_overview(
        &self,
        _: Request<GetNetworkOverviewRequest>,
    ) -> Result<Response<GetNetworkOverviewResponse>, Status> {
        Ok(Response::new(GetNetworkOverviewResponse {
            default_gateway_interface: "eno1".into(),
            default_external_ip: "203.0.113.10".into(),
            default_gateway_ip: "10.0.0.1".into(),
            default_internal_netmask: "255.255.255.0".into(),
            nodes: vec![NodeNetworkInfo {
                node_id: "node-mock-a".into(),
                hostname: "mock-host-alpha".into(),
                address: "10.0.0.10:9443".into(),
                gateway_interface: "eno1".into(),
                disable_vxlan: false,
                interfaces: vec![
                    NetworkInterfaceDetail {
                        name: "eno1".into(),
                        mac_address: "aa:bb:cc:dd:ee:01".into(),
                        state: "UP".into(),
                        mtu: 1500,
                        addresses: vec!["10.0.0.10/24".into()],
                    },
                    NetworkInterfaceDetail {
                        name: "br-default".into(),
                        mac_address: "aa:bb:cc:dd:ee:02".into(),
                        state: "UP".into(),
                        mtu: 1500,
                        addresses: vec!["10.100.0.1/24".into()],
                    },
                    NetworkInterfaceDetail {
                        name: "tap-vm1".into(),
                        mac_address: "aa:bb:cc:dd:ee:03".into(),
                        state: "UP".into(),
                        mtu: 1500,
                        addresses: vec![],
                    },
                    NetworkInterfaceDetail {
                        name: "lo".into(),
                        mac_address: "00:00:00:00:00:00".into(),
                        state: "UP".into(),
                        mtu: 65536,
                        addresses: vec!["127.0.0.1/8".into()],
                    },
                ],
            }],
        }))
    }

    async fn get_storage_overview(
        &self,
        _: Request<GetStorageOverviewRequest>,
    ) -> Result<Response<GetStorageOverviewResponse>, Status> {
        Ok(Response::new(GetStorageOverviewResponse {
            approved_nodes: 1,
            nodes_disk_inventory_ok: 1,
            backend_filesystem_nodes: 0,
            backend_lvm_nodes: 1,
            backend_zfs_nodes: 0,
            backend_unspecified_nodes: 0,
            nodes_luks_tpm2: 0,
            nodes_luks_keyfile: 1,
            nodes_luks_unknown: 0,
            total_block_devices: 2,
            nodes: vec![NodeStorageOverview {
                node_id: "node-mock-a".into(),
                hostname: "mock-host-alpha".into(),
                address: "10.0.0.10:9443".into(),
                storage_backend: StorageBackendType::Lvm as i32,
                luks_method: "key-file".into(),
                disk_inventory_ok: true,
                disks: vec![
                    StorageDiskDetail {
                        name: "sda".into(),
                        path: "/dev/sda".into(),
                        size: "500G".into(),
                        model: "Mock SSD".into(),
                        fstype: String::new(),
                        mountpoint: String::new(),
                    },
                    StorageDiskDetail {
                        name: "nvme0n1".into(),
                        path: "/dev/nvme0n1".into(),
                        size: "2T".into(),
                        model: "Mock NVMe".into(),
                        fstype: "zfs".into(),
                        mountpoint: "/var/lib/kcore".into(),
                    },
                ],
                lvm_inventory_ok: false,
                lvm_volume_groups: vec![],
                lvm_logical_volumes: vec![],
                lvm_physical_volumes: vec![],
            }],
        }))
    }

    async fn get_compliance_report(
        &self,
        _: Request<GetComplianceReportRequest>,
    ) -> Result<Response<GetComplianceReportResponse>, Status> {
        Ok(Response::new(GetComplianceReportResponse {
            controller_version: "mock-controller-0.1-test".into(),
            crypto_library: "rustls (integration test)".into(),
            tls13_cipher_suites: vec!["TEST_SUITE_A".into()],
            tls12_cipher_suites: vec![],
            kx_groups: vec!["TEST_KX".into()],
            excluded_algorithms: vec![],
            mtls_enabled: true,
            access_control: vec![AccessControlEntry {
                rpc_method: "ListVms".into(),
                allowed_identities: "role:node".into(),
            }],
            total_nodes: 1,
            approved_nodes: 1,
            pending_nodes: 0,
            rejected_nodes: 0,
            total_vms: 1,
            running_vms: 1,
            stopped_vms: 0,
            total_networks: 1,
            nat_networks: 1,
            bridge_networks: 0,
            vxlan_networks: 0,
            sub_ca_enabled: false,
            cert_auto_renewal_days: 30,
            nodes_expiring_30d: 0,
            nodes_cert_unknown: 0,
            nodes: vec![],
            nodes_luks_tpm2: 0,
            nodes_luks_keyfile: 0,
            nodes_luks_unknown: 0,
        }))
    }

    async fn list_volumes(
        &self,
        _: Request<ListVolumesRequest>,
    ) -> Result<Response<ListVolumesResponse>, Status> {
        Err(unimp("list_volumes"))
    }

    async fn create_disk_layout(
        &self,
        _: Request<CreateDiskLayoutRequest>,
    ) -> Result<Response<CreateDiskLayoutResponse>, Status> {
        Err(unimp("create_disk_layout"))
    }

    async fn get_disk_layout(
        &self,
        _: Request<GetDiskLayoutRequest>,
    ) -> Result<Response<GetDiskLayoutResponse>, Status> {
        Err(unimp("get_disk_layout"))
    }

    async fn list_disk_layouts(
        &self,
        _: Request<ListDiskLayoutsRequest>,
    ) -> Result<Response<ListDiskLayoutsResponse>, Status> {
        Err(unimp("list_disk_layouts"))
    }

    async fn delete_disk_layout(
        &self,
        _: Request<DeleteDiskLayoutRequest>,
    ) -> Result<Response<DeleteDiskLayoutResponse>, Status> {
        Err(unimp("delete_disk_layout"))
    }

    async fn classify_disk_layout(
        &self,
        _: Request<ClassifyDiskLayoutRequest>,
    ) -> Result<Response<ClassifyDiskLayoutResponse>, Status> {
        Err(unimp("classify_disk_layout"))
    }
}

#[tonic::async_trait]
impl ControllerAdmin for MockController {
    async fn apply_nix_config(
        &self,
        _: Request<ApplyNixConfigRequest>,
    ) -> Result<Response<ApplyNixConfigResponse>, Status> {
        Err(unimp("apply_nix_config"))
    }

    async fn get_replication_events(
        &self,
        _: Request<GetReplicationEventsRequest>,
    ) -> Result<Response<GetReplicationEventsResponse>, Status> {
        Err(unimp("get_replication_events"))
    }

    async fn ack_replication_events(
        &self,
        _: Request<AckReplicationEventsRequest>,
    ) -> Result<Response<AckReplicationEventsResponse>, Status> {
        Err(unimp("ack_replication_events"))
    }

    async fn get_replication_status(
        &self,
        _: Request<GetReplicationStatusRequest>,
    ) -> Result<Response<GetReplicationStatusResponse>, Status> {
        Ok(Response::new(GetReplicationStatusResponse {
            outbox_head_event_id: 42,
            outbox_size: 100,
            outgoing: vec![ReplicationOutgoingStatus {
                peer_id: "dc-west".into(),
                last_acked_event_id: 40,
                lag_events: 2,
            }],
            incoming: vec![ReplicationIncomingStatus {
                peer_endpoint: "10.0.1.1:9090".into(),
                last_pulled_event_id: 38,
                last_applied_event_id: 37,
            }],
            unresolved_conflicts: 0,
            pending_compensation_jobs: 0,
            failed_compensation_jobs: 0,
            materialization_backlog: 0,
            oldest_unresolved_conflict_age_seconds: 0,
            failed_reservations: 0,
            zero_manual_slo_healthy: true,
            zero_manual_slo_violations: vec![],
            failed_retryable_reservations: 0,
            failed_non_retryable_reservations: 0,
            retry_exhausted_reservations: 0,
        }))
    }

    async fn list_replication_conflicts(
        &self,
        _: Request<ListReplicationConflictsRequest>,
    ) -> Result<Response<ListReplicationConflictsResponse>, Status> {
        Ok(Response::new(ListReplicationConflictsResponse {
            conflicts: vec![],
        }))
    }

    async fn resolve_replication_conflict(
        &self,
        _: Request<ResolveReplicationConflictRequest>,
    ) -> Result<Response<ResolveReplicationConflictResponse>, Status> {
        Err(unimp("resolve_replication_conflict"))
    }
}

/// Binds an ephemeral port and serves the mock controller until the process exits.
pub async fn spawn_mock_controller() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock controller bind");
    let addr = listener.local_addr().expect("local_addr");
    let incoming = TcpListenerStream::new(listener);
    tokio::spawn(async move {
        Server::builder()
            .add_service(ControllerServer::new(MockController))
            .add_service(ControllerAdminServer::new(MockController))
            .serve_with_incoming(incoming)
            .await
            .expect("mock controller serve");
    });
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    addr
}
