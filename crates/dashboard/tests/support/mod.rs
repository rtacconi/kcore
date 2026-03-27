//! Shared mock controller for HTTP integration tests.
#![allow(dead_code)]

use std::net::SocketAddr;

use kcore_dashboard::controller_client::controller_proto::controller_server::Controller;
use kcore_dashboard::controller_client::controller_proto::{
    controller_server::ControllerServer, *,
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
            }],
        }))
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
        }))
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
            .serve_with_incoming(incoming)
            .await
            .expect("mock controller serve");
    });
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    addr
}
