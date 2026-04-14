//! Pagination test: 15 VMs across 2 pages, multiple networks.
//!
//! Separate binary so `OnceLock` / `set_dashboard_config` doesn't conflict.

mod support;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use kcore_dashboard::app_server::dashboard_router;
use kcore_dashboard::config::DashboardConfig;
use kcore_dashboard::controller_client::controller_proto::controller_server::Controller;
use kcore_dashboard::controller_client::controller_proto::{
    controller_server::ControllerServer, *,
};
use kcore_dashboard::state::set_dashboard_config;
use leptos::config::get_configuration;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request as TRequest, Response, Status};
use tower::ServiceExt;

#[derive(Clone, Default)]
struct ManyVmsController;

#[tonic::async_trait]
impl Controller for ManyVmsController {
    async fn register_node(
        &self,
        _: TRequest<RegisterNodeRequest>,
    ) -> Result<Response<RegisterNodeResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn heartbeat(
        &self,
        _: TRequest<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn sync_vm_state(
        &self,
        _: TRequest<SyncVmStateRequest>,
    ) -> Result<Response<SyncVmStateResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn sync_workload_state(
        &self,
        _: TRequest<SyncWorkloadStateRequest>,
    ) -> Result<Response<SyncWorkloadStateResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn create_vm(
        &self,
        _: TRequest<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn update_vm(
        &self,
        _: TRequest<UpdateVmRequest>,
    ) -> Result<Response<UpdateVmResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn delete_vm(
        &self,
        _: TRequest<DeleteVmRequest>,
    ) -> Result<Response<DeleteVmResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn set_vm_desired_state(
        &self,
        _: TRequest<SetVmDesiredStateRequest>,
    ) -> Result<Response<SetVmDesiredStateResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn get_vm(&self, _: TRequest<GetVmRequest>) -> Result<Response<GetVmResponse>, Status> {
        Err(Status::unimplemented(""))
    }

    async fn list_vms(
        &self,
        _: TRequest<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        let vms: Vec<VmInfo> = (1..=15)
            .map(|i| VmInfo {
                id: format!("vm-pag-{i:03}"),
                name: format!("paginated-vm-{i:03}"),
                state: if i % 3 == 0 {
                    VmState::Stopped as i32
                } else {
                    VmState::Running as i32
                },
                cpu: i % 4 + 1,
                memory_bytes: (i as i64) * 512 * 1024 * 1024,
                node_id: format!("node-{}", if i <= 8 { "a" } else { "b" }),
                created_at: None,
                storage_backend: String::new(),
                storage_size_bytes: 0,
            })
            .collect();
        Ok(Response::new(ListVmsResponse { vms }))
    }

    async fn create_workload(
        &self,
        _: TRequest<CreateWorkloadRequest>,
    ) -> Result<Response<CreateWorkloadResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn delete_workload(
        &self,
        _: TRequest<DeleteWorkloadRequest>,
    ) -> Result<Response<DeleteWorkloadResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn set_workload_desired_state(
        &self,
        _: TRequest<SetWorkloadDesiredStateRequest>,
    ) -> Result<Response<SetWorkloadDesiredStateResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn get_workload(
        &self,
        _: TRequest<GetWorkloadRequest>,
    ) -> Result<Response<GetWorkloadResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn list_workloads(
        &self,
        _: TRequest<ListWorkloadsRequest>,
    ) -> Result<Response<ListWorkloadsResponse>, Status> {
        Err(Status::unimplemented(""))
    }

    async fn create_network(
        &self,
        _: TRequest<CreateNetworkRequest>,
    ) -> Result<Response<CreateNetworkResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn delete_network(
        &self,
        _: TRequest<DeleteNetworkRequest>,
    ) -> Result<Response<DeleteNetworkResponse>, Status> {
        Err(Status::unimplemented(""))
    }

    async fn list_networks(
        &self,
        _: TRequest<ListNetworksRequest>,
    ) -> Result<Response<ListNetworksResponse>, Status> {
        Ok(Response::new(ListNetworksResponse {
            networks: vec![
                NetworkInfo {
                    name: "prod-nat".into(),
                    network_type: "nat".into(),
                    node_id: "node-a".into(),
                    external_ip: "198.51.100.1".into(),
                    gateway_ip: "10.100.0.1".into(),
                    internal_netmask: "255.255.0.0".into(),
                    vlan_id: 100,
                    enable_outbound_nat: true,
                    ..Default::default()
                },
                NetworkInfo {
                    name: "internal-bridge".into(),
                    network_type: "bridge".into(),
                    node_id: "node-b".into(),
                    external_ip: String::new(),
                    gateway_ip: "10.200.0.1".into(),
                    internal_netmask: "255.255.255.0".into(),
                    vlan_id: 200,
                    enable_outbound_nat: false,
                    ..Default::default()
                },
            ],
        }))
    }

    async fn create_security_group(
        &self,
        _: TRequest<CreateSecurityGroupRequest>,
    ) -> Result<Response<CreateSecurityGroupResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn get_security_group(
        &self,
        _: TRequest<GetSecurityGroupRequest>,
    ) -> Result<Response<GetSecurityGroupResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn list_security_groups(
        &self,
        _: TRequest<ListSecurityGroupsRequest>,
    ) -> Result<Response<ListSecurityGroupsResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn delete_security_group(
        &self,
        _: TRequest<DeleteSecurityGroupRequest>,
    ) -> Result<Response<DeleteSecurityGroupResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn attach_security_group(
        &self,
        _: TRequest<AttachSecurityGroupRequest>,
    ) -> Result<Response<AttachSecurityGroupResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn detach_security_group(
        &self,
        _: TRequest<DetachSecurityGroupRequest>,
    ) -> Result<Response<DetachSecurityGroupResponse>, Status> {
        Err(Status::unimplemented(""))
    }

    async fn get_network_overview(
        &self,
        _: TRequest<GetNetworkOverviewRequest>,
    ) -> Result<Response<GetNetworkOverviewResponse>, Status> {
        Ok(Response::new(GetNetworkOverviewResponse {
            default_gateway_interface: "eno1".into(),
            default_external_ip: "198.51.100.254".into(),
            default_gateway_ip: "10.0.0.1".into(),
            default_internal_netmask: "255.255.0.0".into(),
            nodes: vec![
                NodeNetworkInfo {
                    node_id: "node-a".into(),
                    hostname: "pag-host-a".into(),
                    address: "10.0.0.1:9443".into(),
                    gateway_interface: "eno1".into(),
                    disable_vxlan: false,
                    interfaces: vec![NetworkInterfaceDetail {
                        name: "eno1".into(),
                        mac_address: "aa:00:00:00:00:01".into(),
                        state: "UP".into(),
                        mtu: 1500,
                        addresses: vec!["10.0.0.1/24".into()],
                    }],
                },
                NodeNetworkInfo {
                    node_id: "node-b".into(),
                    hostname: "pag-host-b".into(),
                    address: "10.0.0.2:9443".into(),
                    gateway_interface: "eth0".into(),
                    disable_vxlan: true,
                    interfaces: vec![],
                },
            ],
        }))
    }

    async fn list_nodes(
        &self,
        _: TRequest<ListNodesRequest>,
    ) -> Result<Response<ListNodesResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn get_node(
        &self,
        _: TRequest<GetNodeRequest>,
    ) -> Result<Response<GetNodeResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn create_ssh_key(
        &self,
        _: TRequest<CreateSshKeyRequest>,
    ) -> Result<Response<CreateSshKeyResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn delete_ssh_key(
        &self,
        _: TRequest<DeleteSshKeyRequest>,
    ) -> Result<Response<DeleteSshKeyResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn list_ssh_keys(
        &self,
        _: TRequest<ListSshKeysRequest>,
    ) -> Result<Response<ListSshKeysResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn get_ssh_key(
        &self,
        _: TRequest<GetSshKeyRequest>,
    ) -> Result<Response<GetSshKeyResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn drain_node(
        &self,
        _: TRequest<DrainNodeRequest>,
    ) -> Result<Response<DrainNodeResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn approve_node(
        &self,
        _: TRequest<ApproveNodeRequest>,
    ) -> Result<Response<ApproveNodeResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn reject_node(
        &self,
        _: TRequest<RejectNodeRequest>,
    ) -> Result<Response<RejectNodeResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn renew_node_cert(
        &self,
        _: TRequest<RenewNodeCertRequest>,
    ) -> Result<Response<RenewNodeCertResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn issue_node_bootstrap_cert(
        &self,
        _: TRequest<IssueNodeBootstrapCertRequest>,
    ) -> Result<Response<IssueNodeBootstrapCertResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn rotate_sub_ca(
        &self,
        _: TRequest<RotateSubCaRequest>,
    ) -> Result<Response<RotateSubCaResponse>, Status> {
        Err(Status::unimplemented(""))
    }
    async fn reload_tls(
        &self,
        _: TRequest<ReloadTlsRequest>,
    ) -> Result<Response<ReloadTlsResponse>, Status> {
        Err(Status::unimplemented(""))
    }

    async fn get_storage_overview(
        &self,
        _: TRequest<GetStorageOverviewRequest>,
    ) -> Result<Response<GetStorageOverviewResponse>, Status> {
        Ok(Response::new(GetStorageOverviewResponse::default()))
    }

    async fn get_compliance_report(
        &self,
        _: TRequest<GetComplianceReportRequest>,
    ) -> Result<Response<GetComplianceReportResponse>, Status> {
        Ok(Response::new(GetComplianceReportResponse {
            controller_version: "pag-test-0.2".into(),
            crypto_library: "test-crypto".into(),
            total_vms: 15,
            running_vms: 10,
            stopped_vms: 5,
            total_networks: 2,
            nat_networks: 1,
            bridge_networks: 1,
            ..Default::default()
        }))
    }

    async fn list_volumes(
        &self,
        _: TRequest<ListVolumesRequest>,
    ) -> Result<Response<ListVolumesResponse>, Status> {
        Err(Status::unimplemented(""))
    }
}

async fn spawn_many_vms_controller() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let incoming = TcpListenerStream::new(listener);
    tokio::spawn(async move {
        Server::builder()
            .add_service(ControllerServer::new(ManyVmsController))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    addr
}

async fn fetch(app: &axum::Router, path: &str) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 8 * 1024 * 1024).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn pagination_and_multiple_networks() {
    let grpc = spawn_many_vms_controller().await;
    set_dashboard_config(DashboardConfig::insecure_on(format!(
        "127.0.0.1:{}",
        grpc.port()
    )))
    .expect("set config");

    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let conf = get_configuration(Some(manifest)).expect("leptos config");
    let app = dashboard_router(conf.leptos_options);

    // ── Page 1: first 10 of 15 VMs ──────────────────────────────────
    let (st, p1) = fetch(&app, "/vms").await;
    assert_eq!(st, StatusCode::OK);
    assert!(p1.contains("Page 1 of 2"), "15 VMs = 2 pages; got: {}", {
        let idx = p1.find("Page").unwrap_or(0);
        &p1[idx..idx + 40.min(p1.len() - idx)]
    });
    assert!(p1.contains("15 VMs"), "total VM count");
    assert!(p1.contains("Next"), "page 1 must have Next link");

    // ── Page 2: remaining 5 VMs ─────────────────────────────────────
    let (st, p2) = fetch(&app, "/vms?page=2").await;
    assert_eq!(st, StatusCode::OK);
    assert!(p2.contains("Page 2 of 2"), "second page");
    assert!(p2.contains("Previous"), "page 2 must have Previous link");

    // ── Networks: host + 2 SDN networks ────────────────────────────
    let (st, nets) = fetch(&app, "/networks").await;
    assert_eq!(st, StatusCode::OK);

    // Host networking
    assert!(nets.contains("Host networking"), "must have host section");
    assert!(nets.contains("pag-host-a"), "must show node-a hostname");
    assert!(nets.contains("pag-host-b"), "must show node-b hostname");
    assert!(
        nets.contains("198.51.100.254"),
        "must show default external IP"
    );

    // SDN
    assert!(nets.contains("prod-nat"), "must show first network");
    assert!(nets.contains("internal-bridge"), "must show second network");
    assert!(nets.contains("198.51.100.1"), "must show SDN external IP");
    assert!(nets.contains("bridge"), "must show bridge type");
    assert!(nets.contains("node-a"), "must show node-a");
    assert!(nets.contains("node-b"), "must show node-b");

    // ── Compliance shows multi-VM stats ──────────────────────────────
    let (st, compliance) = fetch(&app, "/compliance").await;
    assert_eq!(st, StatusCode::OK);
    assert!(compliance.contains("pag-test-0.2"));
}
