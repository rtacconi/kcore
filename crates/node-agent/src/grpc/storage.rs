use tonic::{Request, Response, Status};

use crate::auth::{self, CN_CONTROLLER_PREFIX, CN_KCTL};
use crate::proto;
use crate::storage::{self, StorageAdapter};
use std::sync::Arc;

pub struct StorageService {
    storage: Arc<dyn StorageAdapter>,
}

impl StorageService {
    pub fn new() -> Self {
        Self {
            storage: storage::default_adapter(),
        }
    }

    pub fn new_with_storage(storage: Arc<dyn StorageAdapter>) -> Self {
        Self { storage }
    }
}

#[tonic::async_trait]
impl proto::node_storage_server::NodeStorage for StorageService {
    async fn create_volume(
        &self,
        request: Request<proto::CreateVolumeRequest>,
    ) -> Result<Response<proto::CreateVolumeResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX, CN_KCTL])?;
        let req = request.into_inner();
        let storage = Arc::clone(&self.storage);
        let resp = tokio::task::spawn_blocking(move || {
            storage
                .create_volume(storage::CreateVolumeRequest {
                    volume_id: req.volume_id,
                    storage_class: req.storage_class,
                    size_bytes: req.size_bytes,
                    parameters: req.parameters,
                })
                .map(|backend_handle| proto::CreateVolumeResponse { backend_handle })
        })
        .await
        .map_err(|e| Status::internal(format!("task join: {e}")))??;
        Ok(Response::new(resp))
    }

    async fn delete_volume(
        &self,
        request: Request<proto::DeleteVolumeRequest>,
    ) -> Result<Response<proto::DeleteVolumeResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX, CN_KCTL])?;
        let req = request.into_inner();
        let storage = Arc::clone(&self.storage);
        tokio::task::spawn_blocking(move || storage.delete_volume(&req.backend_handle))
            .await
            .map_err(|e| Status::internal(format!("task join: {e}")))??;
        Ok(Response::new(proto::DeleteVolumeResponse {}))
    }

    async fn attach_volume(
        &self,
        request: Request<proto::AttachVolumeRequest>,
    ) -> Result<Response<proto::AttachVolumeResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX, CN_KCTL])?;
        let req = request.into_inner();
        let storage = Arc::clone(&self.storage);
        tokio::task::spawn_blocking(move || {
            storage.attach_volume(storage::AttachVolumeRequest {
                backend_handle: req.backend_handle,
                vm_id: req.vm_id,
                target_device: req.target_device,
                bus: req.bus,
            })
        })
        .await
        .map_err(|e| Status::internal(format!("task join: {e}")))??;
        Ok(Response::new(proto::AttachVolumeResponse {}))
    }

    async fn detach_volume(
        &self,
        request: Request<proto::DetachVolumeRequest>,
    ) -> Result<Response<proto::DetachVolumeResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX, CN_KCTL])?;
        let req = request.into_inner();
        let storage = Arc::clone(&self.storage);
        tokio::task::spawn_blocking(move || {
            storage.detach_volume(storage::DetachVolumeRequest {
                backend_handle: req.backend_handle,
                vm_id: req.vm_id,
            })
        })
        .await
        .map_err(|e| Status::internal(format!("task join: {e}")))??;
        Ok(Response::new(proto::DetachVolumeResponse {}))
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
