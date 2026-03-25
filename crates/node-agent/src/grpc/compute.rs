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

const DECLARATIVE_MSG: &str = "VMs are managed declaratively via NixOS config (ctrl-os.vms). \
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
        _request: Request<proto::CreateVmRequest>,
    ) -> Result<Response<proto::CreateVmResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn update_vm(
        &self,
        _request: Request<proto::UpdateVmRequest>,
    ) -> Result<Response<proto::UpdateVmResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn delete_vm(
        &self,
        _request: Request<proto::DeleteVmRequest>,
    ) -> Result<Response<proto::DeleteVmResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn set_vm_desired_state(
        &self,
        _request: Request<proto::SetVmDesiredStateRequest>,
    ) -> Result<Response<proto::SetVmDesiredStateResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn reboot_vm(
        &self,
        _request: Request<proto::RebootVmRequest>,
    ) -> Result<Response<proto::RebootVmResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn pull_image(
        &self,
        _request: Request<proto::PullImageRequest>,
    ) -> Result<Response<proto::PullImageResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn list_images(
        &self,
        _request: Request<proto::ListImagesRequest>,
    ) -> Result<Response<proto::ListImagesResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn delete_image(
        &self,
        _request: Request<proto::DeleteImageRequest>,
    ) -> Result<Response<proto::DeleteImageResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }
}
