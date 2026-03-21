use std::path::PathBuf;

use tokio::process::Command;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::discovery;
use crate::proto;

pub struct AdminService {
    nix_config_path: PathBuf,
}

impl AdminService {
    pub fn new(nix_config_path: String) -> Self {
        Self {
            nix_config_path: PathBuf::from(nix_config_path),
        }
    }
}

#[tonic::async_trait]
impl proto::node_admin_server::NodeAdmin for AdminService {
    async fn list_disks(
        &self,
        _request: Request<proto::ListDisksRequest>,
    ) -> Result<Response<proto::ListDisksResponse>, Status> {
        let disks = discovery::list_disks().map_err(Status::internal)?;
        Ok(Response::new(proto::ListDisksResponse { disks }))
    }

    async fn list_network_interfaces(
        &self,
        _request: Request<proto::ListNetworkInterfacesRequest>,
    ) -> Result<Response<proto::ListNetworkInterfacesResponse>, Status> {
        let interfaces = discovery::list_network_interfaces().map_err(Status::internal)?;
        Ok(Response::new(proto::ListNetworkInterfacesResponse {
            interfaces,
        }))
    }

    async fn apply_nix_config(
        &self,
        request: Request<proto::ApplyNixConfigRequest>,
    ) -> Result<Response<proto::ApplyNixConfigResponse>, Status> {
        let req = request.into_inner();
        let path = &self.nix_config_path;

        std::fs::write(path, &req.configuration_nix).map_err(|e| {
            error!(path = %path.display(), error = %e, "failed to write nix config");
            Status::internal(format!("writing {}: {e}", path.display()))
        })?;

        info!(path = %path.display(), "wrote nix config");

        if !req.rebuild {
            return Ok(Response::new(proto::ApplyNixConfigResponse {
                success: true,
                message: format!("config written to {}", path.display()),
            }));
        }

        let rebuild_path = path.clone();
        tokio::spawn(async move {
            info!("starting nixos-rebuild switch");
            match Command::new("nixos-rebuild").arg("switch").output().await {
                Ok(out) if out.status.success() => {
                    info!("nixos-rebuild switch completed");
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    error!(
                        path = %rebuild_path.display(),
                        stderr = %stderr,
                        "nixos-rebuild switch failed"
                    );
                }
                Err(e) => {
                    error!(error = %e, "failed to run nixos-rebuild");
                }
            }
        });

        Ok(Response::new(proto::ApplyNixConfigResponse {
            success: true,
            message: format!(
                "config written to {}; nixos-rebuild switch started",
                path.display()
            ),
        }))
    }

    async fn install_to_disk(
        &self,
        request: Request<proto::InstallToDiskRequest>,
    ) -> Result<Response<proto::InstallToDiskResponse>, Status> {
        let req = request.into_inner();
        if req.os_disk.is_empty() {
            return Err(Status::invalid_argument("os_disk is required"));
        }
        if !req.os_disk.starts_with("/dev/") || req.os_disk.contains("..") {
            return Err(Status::invalid_argument("invalid os_disk path"));
        }

        let mut args = vec![
            "install-to-disk".to_string(),
            "--disk".to_string(),
            req.os_disk,
            "--yes".to_string(),
            "--wipe".to_string(),
            "--non-interactive".to_string(),
            "--reboot".to_string(),
        ];
        for dd in &req.data_disks {
            args.push("--data-disk".to_string());
            args.push(dd.clone());
        }
        if !req.controller.is_empty() {
            args.push("--controller".to_string());
            args.push(req.controller);
        }

        let cmd_str = args.join(" ");
        let spawn_result = std::process::Command::new("nohup")
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        match spawn_result {
            Ok(_) => Ok(Response::new(proto::InstallToDiskResponse {
                accepted: true,
                message: format!("install started: {cmd_str}"),
            })),
            Err(e) => Err(Status::internal(format!("failed to start install: {e}"))),
        }
    }
}
