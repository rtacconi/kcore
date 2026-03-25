use tonic::{Request, Response, Status};

use crate::auth::{self, CN_CONTROLLER, CN_KCTL};
use crate::proto;
use crate::vmm;

pub struct ComputeService {
    client: vmm::Client,
}

impl ComputeService {
    pub fn new(client: vmm::Client) -> Self {
        Self { client }
    }
}

fn ch_state_to_proto(state: &str) -> i32 {
    match state {
        "Running" => proto::VmState::Running as i32,
        "Paused" => proto::VmState::Paused as i32,
        "Shutdown" | "Created" => proto::VmState::Stopped as i32,
        _ => proto::VmState::Unknown as i32,
    }
}

const DECLARATIVE_MSG: &str = "VMs are managed declaratively via NixOS config (ch-vm.vms). \
    Use `nixos-rebuild switch` to add, remove, or reconfigure VMs.";

#[tonic::async_trait]
impl proto::node_compute_server::NodeCompute for ComputeService {
    async fn get_vm(
        &self,
        request: Request<proto::GetVmRequest>,
    ) -> Result<Response<proto::GetVmResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        let vm_id = &request.get_ref().vm_id;
        let info = self
            .client
            .get_vm_info(vm_id)
            .await
            .ok_or_else(|| Status::not_found(format!("VM {vm_id} not found")))?;

        let cpu = info.config.cpus.map(|c| c.boot_vcpus as i32).unwrap_or(0);
        let mem = info.config.memory.map(|m| m.size as i64).unwrap_or(0);

        Ok(Response::new(proto::GetVmResponse {
            spec: Some(proto::VmSpec {
                id: vm_id.clone(),
                name: vm_id.clone(),
                cpu,
                memory_bytes: mem,
                disks: Vec::new(),
                nics: Vec::new(),
            }),
            status: Some(proto::VmStatus {
                id: vm_id.clone(),
                state: ch_state_to_proto(&info.state),
                created_at: None,
                updated_at: None,
            }),
        }))
    }

    async fn list_vms(
        &self,
        request: Request<proto::ListVmsRequest>,
    ) -> Result<Response<proto::ListVmsResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        let vms = self.client.list_vms().await;

        let vm_infos = vms
            .into_iter()
            .map(|(name, info)| {
                let cpu = info.config.cpus.map(|c| c.boot_vcpus as i32).unwrap_or(0);
                let mem = info.config.memory.map(|m| m.size as i64).unwrap_or(0);
                proto::VmInfo {
                    id: name.clone(),
                    name,
                    state: ch_state_to_proto(&info.state),
                    cpu,
                    memory_bytes: mem,
                    created_at: None,
                }
            })
            .collect();

        Ok(Response::new(proto::ListVmsResponse { vms: vm_infos }))
    }

    async fn create_vm(
        &self,
        request: Request<proto::CreateVmRequest>,
    ) -> Result<Response<proto::CreateVmResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn update_vm(
        &self,
        request: Request<proto::UpdateVmRequest>,
    ) -> Result<Response<proto::UpdateVmResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn delete_vm(
        &self,
        request: Request<proto::DeleteVmRequest>,
    ) -> Result<Response<proto::DeleteVmResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn set_vm_desired_state(
        &self,
        request: Request<proto::SetVmDesiredStateRequest>,
    ) -> Result<Response<proto::SetVmDesiredStateResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn reboot_vm(
        &self,
        request: Request<proto::RebootVmRequest>,
    ) -> Result<Response<proto::RebootVmResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn pull_image(
        &self,
        request: Request<proto::PullImageRequest>,
    ) -> Result<Response<proto::PullImageResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn list_images(
        &self,
        request: Request<proto::ListImagesRequest>,
    ) -> Result<Response<proto::ListImagesResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn delete_image(
        &self,
        request: Request<proto::DeleteImageRequest>,
    ) -> Result<Response<proto::DeleteImageResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svc() -> ComputeService {
        ComputeService::new(vmm::Client::new("/run/kcore"))
    }

    fn assert_denied(res: Result<impl Sized, Status>) {
        match res {
            Ok(_) => panic!("expected permission denied without TLS"),
            Err(err) => assert_eq!(err.code(), tonic::Code::PermissionDenied),
        }
    }

    #[tokio::test]
    async fn insecure_mode_denies_all_compute_endpoints() {
        let s = svc();

        assert_denied(
            <ComputeService as proto::node_compute_server::NodeCompute>::get_vm(
                &s,
                Request::new(proto::GetVmRequest {
                    vm_id: "vm-1".to_string(),
                }),
            )
            .await,
        );
        assert_denied(
            <ComputeService as proto::node_compute_server::NodeCompute>::list_vms(
                &s,
                Request::new(proto::ListVmsRequest {}),
            )
            .await,
        );
        assert_denied(
            <ComputeService as proto::node_compute_server::NodeCompute>::create_vm(
                &s,
                Request::new(proto::CreateVmRequest {
                    spec: None,
                    image_uri: String::new(),
                    image_path: String::new(),
                    image_format: String::new(),
                }),
            )
            .await,
        );
        assert_denied(
            <ComputeService as proto::node_compute_server::NodeCompute>::update_vm(
                &s,
                Request::new(proto::UpdateVmRequest { spec: None }),
            )
            .await,
        );
        assert_denied(
            <ComputeService as proto::node_compute_server::NodeCompute>::delete_vm(
                &s,
                Request::new(proto::DeleteVmRequest {
                    vm_id: "vm-1".to_string(),
                }),
            )
            .await,
        );
        assert_denied(
            <ComputeService as proto::node_compute_server::NodeCompute>::set_vm_desired_state(
                &s,
                Request::new(proto::SetVmDesiredStateRequest {
                    vm_id: "vm-1".to_string(),
                    desired_state: proto::VmDesiredState::Running as i32,
                }),
            )
            .await,
        );
        assert_denied(
            <ComputeService as proto::node_compute_server::NodeCompute>::reboot_vm(
                &s,
                Request::new(proto::RebootVmRequest {
                    vm_id: "vm-1".to_string(),
                    force: false,
                }),
            )
            .await,
        );
        assert_denied(
            <ComputeService as proto::node_compute_server::NodeCompute>::pull_image(
                &s,
                Request::new(proto::PullImageRequest {
                    uri: "https://example.invalid/img.raw".to_string(),
                    name: String::new(),
                }),
            )
            .await,
        );
        assert_denied(
            <ComputeService as proto::node_compute_server::NodeCompute>::list_images(
                &s,
                Request::new(proto::ListImagesRequest {}),
            )
            .await,
        );
        assert_denied(
            <ComputeService as proto::node_compute_server::NodeCompute>::delete_image(
                &s,
                Request::new(proto::DeleteImageRequest {
                    name: "img.raw".to_string(),
                    force: false,
                }),
            )
            .await,
        );
    }
}
