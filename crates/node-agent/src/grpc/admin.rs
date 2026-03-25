use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::process::Command;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::discovery;
use crate::proto;

pub struct AdminService {
    nix_config_path: PathBuf,
}

const BOOTSTRAP_CERT_DIR: &str = "/etc/kcore/certs";
const INSTALL_LOG_DIR: &str = "/var/log/kcore";
const NIXOS_CONFIG_PATH: &str = "/etc/nixos/configuration.nix";

async fn resolve_nixpkgs_path() -> Option<String> {
    for candidate in [
        "/nix/var/nix/profiles/per-user/root/channels/nixos",
        "/run/current-system/sw/share/nixpkgs",
    ] {
        if std::path::Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }

    let out = Command::new("nix")
        .args(["eval", "--raw", "nixpkgs#path"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if p.is_empty() {
        None
    } else {
        Some(p)
    }
}

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

fn rebuild_args(mode: &'static str) -> Vec<&'static str> {
    vec![mode]
}

fn rebuild_sequence(test_success: bool) -> Vec<&'static str> {
    if test_success {
        vec!["test", "switch"]
    } else {
        vec!["test"]
    }
}

async fn run_rebuild_mode(mode: &'static str) -> Result<std::process::Output, std::io::Error> {
    let mut cmd = Command::new("nixos-rebuild");
    cmd.args(rebuild_args(mode));
    if let Some(nixpkgs_path) = resolve_nixpkgs_path().await {
        cmd.env(
            "NIX_PATH",
            format!("nixos-config={NIXOS_CONFIG_PATH}:nixpkgs={nixpkgs_path}"),
        );
    }
    cmd.output().await
}

async fn run_test_then_switch(path: PathBuf) {
    info!("starting nixos-rebuild test");
    let test_out = match run_rebuild_mode("test").await {
        Ok(out) => out,
        Err(e) => {
            error!(path = %path.display(), error = %e, "failed to run nixos-rebuild test");
            return;
        }
    };

    if !test_out.status.success() {
        let stderr = String::from_utf8_lossy(&test_out.stderr);
        error!(
            path = %path.display(),
            stderr = %stderr,
            "nixos-rebuild test failed; skipping switch"
        );
        return;
    }

    info!("nixos-rebuild test succeeded; starting nixos-rebuild switch");
    match run_rebuild_mode("switch").await {
        Ok(out) if out.status.success() => {
            info!("nixos-rebuild switch completed");
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            error!(
                path = %path.display(),
                stderr = %stderr,
                "nixos-rebuild switch failed"
            );
        }
        Err(e) => {
            error!(path = %path.display(), error = %e, "failed to run nixos-rebuild switch");
        }
    }
}

fn write_bootstrap_pki_at(
    req: &proto::InstallToDiskRequest,
    base_dir: &PathBuf,
) -> Result<(), Status> {
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

fn prepare_install_log() -> Result<(std::fs::File, PathBuf), Status> {
    std::fs::create_dir_all(INSTALL_LOG_DIR)
        .map_err(|e| Status::internal(format!("creating {INSTALL_LOG_DIR}: {e}")))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| Status::internal(format!("system clock before UNIX_EPOCH: {e}")))?
        .as_secs();
    let log_path = PathBuf::from(INSTALL_LOG_DIR).join(format!("install-to-disk-{timestamp}.log"));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| Status::internal(format!("opening {}: {e}", log_path.display())))?;
    Ok((file, log_path))
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

        let planned_steps = rebuild_sequence(true).join(" -> ");
        info!(path = %path.display(), steps = %planned_steps, "starting background nix apply flow");
        let rebuild_path = path.clone();
        tokio::spawn(async move {
            run_test_then_switch(rebuild_path).await;
        });

        Ok(Response::new(proto::ApplyNixConfigResponse {
            success: true,
            message: format!(
                "config written to {}; nixos-rebuild test+switch started",
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

        let cmd_str = format!("install-to-disk {}", args.join(" "));
        let (mut log_file, log_path) = prepare_install_log()?;
        use std::io::Write as _;
        writeln!(log_file, "Starting install command: {cmd_str}")
            .map_err(|e| Status::internal(format!("writing {}: {e}", log_path.display())))?;
        let stderr_log = log_file
            .try_clone()
            .map_err(|e| Status::internal(format!("cloning {}: {e}", log_path.display())))?;

        let spawn_result = std::process::Command::new("install-to-disk")
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(log_file))
            .stderr(std::process::Stdio::from(stderr_log))
            .spawn();

        match spawn_result {
            Ok(child) => {
                let pid = child.id();
                info!(pid, log_path = %log_path.display(), "started install-to-disk");
                Ok(Response::new(proto::InstallToDiskResponse {
                accepted: true,
                message: format!(
                    "install started (pid {pid}): {cmd_str}; logs: {}",
                    log_path.display()
                ),
                }))
            }
            Err(e) => Err(Status::internal(format!("failed to start install: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::Request;

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

    #[test]
    fn bootstrap_pki_no_materials_is_noop() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cert_dir = temp.path().join("certs");
        let req = proto::InstallToDiskRequest {
            os_disk: "/dev/sda".to_string(),
            data_disks: vec![],
            controller: String::new(),
            ca_cert_pem: String::new(),
            node_cert_pem: String::new(),
            node_key_pem: String::new(),
            controller_cert_pem: String::new(),
            controller_key_pem: String::new(),
            kctl_cert_pem: String::new(),
            kctl_key_pem: String::new(),
        };
        write_bootstrap_pki_at(&req, &cert_dir).expect("noop cert write");
        assert!(
            !cert_dir.exists(),
            "no certificate directory should be created when payload is empty"
        );
    }

    #[test]
    fn rebuild_args_uses_requested_mode() {
        assert_eq!(rebuild_args("test"), vec!["test"]);
        assert_eq!(rebuild_args("switch"), vec!["switch"]);
    }

    #[test]
    fn rebuild_sequence_skips_switch_on_test_failure() {
        assert_eq!(rebuild_sequence(false), vec!["test"]);
        assert_eq!(rebuild_sequence(true), vec!["test", "switch"]);
    }

    #[tokio::test]
    async fn apply_nix_config_without_rebuild_writes_file_only() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nix_path = temp.path().join("kcore-vms.nix");
        let svc = AdminService::new(nix_path.display().to_string());
        let req = proto::ApplyNixConfigRequest {
            configuration_nix: "{ ... }: { test = true; }\n".to_string(),
            rebuild: false,
        };

        let resp = <AdminService as proto::node_admin_server::NodeAdmin>::apply_nix_config(
            &svc,
            Request::new(req),
        )
        .await
        .expect("apply without rebuild")
        .into_inner();

        assert!(resp.success);
        assert!(resp.message.contains("config written to"));
        let written = std::fs::read_to_string(&nix_path).expect("read written nix file");
        assert_eq!(written, "{ ... }: { test = true; }\n");
    }
}
