use tonic::{Request, Response, Status};

use crate::proto;

pub struct StorageService;

impl StorageService {
    pub fn new() -> Self {
        Self
    }
}

const DECLARATIVE_MSG: &str = "Storage is managed declaratively via NixOS config (ctrl-os.vms). \
    Use `nixos-rebuild switch` to manage volumes.";

#[tonic::async_trait]
impl proto::node_storage_server::NodeStorage for StorageService {
    async fn create_volume(
        &self,
        _request: Request<proto::CreateVolumeRequest>,
    ) -> Result<Response<proto::CreateVolumeResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn delete_volume(
        &self,
        _request: Request<proto::DeleteVolumeRequest>,
    ) -> Result<Response<proto::DeleteVolumeResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn attach_volume(
        &self,
        _request: Request<proto::AttachVolumeRequest>,
    ) -> Result<Response<proto::AttachVolumeResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn detach_volume(
        &self,
        _request: Request<proto::DetachVolumeRequest>,
    ) -> Result<Response<proto::DetachVolumeResponse>, Status> {
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }
}
