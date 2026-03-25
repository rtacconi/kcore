use tokio::process::Command;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::auth::{self, CN_KCTL};
use crate::controller_proto;

pub struct ControllerAdminService;

impl ControllerAdminService {
    pub fn new() -> Self {
        Self
    }
}

#[tonic::async_trait]
impl controller_proto::controller_admin_server::ControllerAdmin for ControllerAdminService {
    async fn apply_nix_config(
        &self,
        request: Request<controller_proto::ApplyNixConfigRequest>,
    ) -> Result<Response<controller_proto::ApplyNixConfigResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();
        let path = "/etc/nixos/configuration.nix";

        let config_nix = req.configuration_nix.clone();
        tokio::task::spawn_blocking(move || std::fs::write(path, &config_nix))
            .await
            .map_err(|e| Status::internal(format!("task join: {e}")))?
            .map_err(|e| {
                error!(error = %e, "failed to write controller nix config");
                Status::internal(format!("writing {path}: {e}"))
            })?;

        info!("wrote controller nix config");

        if !req.rebuild {
            return Ok(Response::new(controller_proto::ApplyNixConfigResponse {
                success: true,
                message: format!("config written to {path}"),
            }));
        }

        tokio::spawn(async move {
            info!("starting nixos-rebuild switch");
            match Command::new("nixos-rebuild").arg("switch").output().await {
                Ok(out) if out.status.success() => {
                    info!("nixos-rebuild switch completed");
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    error!(stderr = %stderr, "nixos-rebuild switch failed");
                }
                Err(e) => {
                    error!(error = %e, "failed to run nixos-rebuild");
                }
            }
        });

        Ok(Response::new(controller_proto::ApplyNixConfigResponse {
            success: true,
            message: format!("config written to {path}; nixos-rebuild switch started"),
        }))
    }
}
