use tonic::{Request, Response, Status};

use crate::auth::{self, CN_CONTROLLER, CN_KCTL};
use crate::proto;

pub struct StorageService;

impl StorageService {
    pub fn new() -> Self {
        Self
    }
}

const DECLARATIVE_MSG: &str = "Storage is managed declaratively via NixOS config (ch-vm.vms). \
    Use `nixos-rebuild switch` to manage volumes.";

#[tonic::async_trait]
impl proto::node_storage_server::NodeStorage for StorageService {
    async fn create_volume(
        &self,
        request: Request<proto::CreateVolumeRequest>,
    ) -> Result<Response<proto::CreateVolumeResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn delete_volume(
        &self,
        request: Request<proto::DeleteVolumeRequest>,
    ) -> Result<Response<proto::DeleteVolumeResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn attach_volume(
        &self,
        request: Request<proto::AttachVolumeRequest>,
    ) -> Result<Response<proto::AttachVolumeResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }

    async fn detach_volume(
        &self,
        request: Request<proto::DetachVolumeRequest>,
    ) -> Result<Response<proto::DetachVolumeResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER, CN_KCTL])?;
        Err(Status::unimplemented(DECLARATIVE_MSG))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_denied(res: Result<impl Sized, Status>) {
        match res {
            Ok(_) => panic!("expected permission denied without TLS"),
            Err(err) => assert_eq!(err.code(), tonic::Code::PermissionDenied),
        }
    }

    #[tokio::test]
    async fn insecure_mode_denies_all_storage_endpoints() {
        let s = StorageService::new();

        assert_denied(
            <StorageService as proto::node_storage_server::NodeStorage>::create_volume(
                &s,
                Request::new(proto::CreateVolumeRequest {
                    volume_id: "vol-1".to_string(),
                    storage_class: "default".to_string(),
                    size_bytes: 1024,
                    parameters: std::collections::HashMap::new(),
                }),
            )
            .await,
        );
        assert_denied(
            <StorageService as proto::node_storage_server::NodeStorage>::delete_volume(
                &s,
                Request::new(proto::DeleteVolumeRequest {
                    backend_handle: "/dev/null".to_string(),
                }),
            )
            .await,
        );
        assert_denied(
            <StorageService as proto::node_storage_server::NodeStorage>::attach_volume(
                &s,
                Request::new(proto::AttachVolumeRequest {
                    backend_handle: "/dev/null".to_string(),
                    vm_id: "vm-1".to_string(),
                    target_device: "vda".to_string(),
                    bus: "virtio".to_string(),
                }),
            )
            .await,
        );
        assert_denied(
            <StorageService as proto::node_storage_server::NodeStorage>::detach_volume(
                &s,
                Request::new(proto::DetachVolumeRequest {
                    backend_handle: "/dev/null".to_string(),
                    vm_id: "vm-1".to_string(),
                }),
            )
            .await,
        );
    }
}
