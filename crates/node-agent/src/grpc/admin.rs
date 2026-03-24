use std::path::PathBuf;

use tokio::process::Command;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::discovery;
use crate::proto;

pub struct AdminService {
    nix_config_path: PathBuf,
}

const BOOTSTRAP_CERT_DIR: &str = "/etc/kcore/certs";

impl AdminService {
    pub fn new(nix_config_path: String) -> Self {
        Self {
            nix_config_path: PathBuf::from(nix_config_path),
        }
    }
}

fn write_bootstrap_pki(req: &proto::InstallToDiskRequest) -> Result<(), Status> {
    write_bootstrap_pki_at(req, &PathBuf::from(BOOTSTRAP_CERT_DIR))
}

fn write_bootstrap_pki_at(req: &proto::InstallToDiskRequest, base_dir: &PathBuf) -> Result<(), Status> {
    let certs = [
        ("ca.crt", &req.ca_cert_pem),
        ("node.crt", &req.node_cert_pem),
        ("node.key", &req.node_key_pem),
        ("controller.crt", &req.controller_cert_pem),
        ("controller.key", &req.controller_key_pem),
        ("kctl.crt", &req.kctl_cert_pem),
        ("kctl.key", &req.kctl_key_pem),
    ];

    let has_any = certs.iter().any(|(_, content)| !content.trim().is_empty());
    if !has_any {
        return Ok(());
    }

    std::fs::create_dir_all(base_dir)
        .map_err(|e| Status::internal(format!("creating {}: {e}", base_dir.display())))?;

    for (name, content) in certs {
        if content.trim().is_empty() {
            continue;
        }
        let path = base_dir.join(name);
        std::fs::write(&path, content)
            .map_err(|e| Status::internal(format!("writing {}: {e}", path.display())))?;
    }

    Ok(())
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

        write_bootstrap_pki(&req)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_pki_writes_supplied_materials() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cert_dir = temp.path().join("certs");
        let req = proto::InstallToDiskRequest {
            os_disk: "/dev/sda".to_string(),
            data_disks: vec![],
            controller: String::new(),
            ca_cert_pem: "ca".to_string(),
            node_cert_pem: "node-cert".to_string(),
            node_key_pem: "node-key".to_string(),
            controller_cert_pem: String::new(),
            controller_key_pem: String::new(),
            kctl_cert_pem: String::new(),
            kctl_key_pem: String::new(),
        };

        write_bootstrap_pki_at(&req, &cert_dir).expect("write certs");

        assert_eq!(
            std::fs::read_to_string(cert_dir.join("ca.crt")).expect("ca"),
            "ca"
        );
        assert_eq!(
            std::fs::read_to_string(cert_dir.join("node.crt")).expect("node cert"),
            "node-cert"
        );
        assert_eq!(
            std::fs::read_to_string(cert_dir.join("node.key")).expect("node key"),
            "node-key"
        );
        assert!(!cert_dir.join("controller.crt").exists());
    }
}
