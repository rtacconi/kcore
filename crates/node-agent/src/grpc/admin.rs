#![allow(clippy::result_large_err)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::process::Command;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::auth::{self, CN_CONTROLLER, CN_KCTL};
use crate::discovery;
use crate::proto;

pub struct AdminService {
    nix_config_path: PathBuf,
}

const BOOTSTRAP_CERT_DIR: &str = "/etc/kcore/certs";
const INSTALL_LOG_DIR: &str = "/var/log/kcore";
const NIXOS_CONFIG_PATH: &str = "/etc/nixos/configuration.nix";
const IMAGE_CACHE_DIR: &str = "/var/lib/kcore/images";

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

fn validate_disk_path(path: &str, field: &str) -> Result<(), Status> {
    if !path.starts_with("/dev/") {
        return Err(Status::invalid_argument(format!(
            "{field}: must start with /dev/, got {path}"
        )));
    }
    if path.contains("..") {
        return Err(Status::invalid_argument(format!(
            "{field}: path traversal not allowed in {path}"
        )));
    }
    if path.contains(char::is_whitespace) {
        return Err(Status::invalid_argument(format!(
            "{field}: whitespace not allowed in {path}"
        )));
    }
    Ok(())
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

async fn log_failed_kcore_units(context: &'static str) {
    let out = match Command::new("systemctl")
        .args([
            "--no-pager",
            "--full",
            "--failed",
            "list-units",
            "kcore-vm-*",
            "kcore-tap-*",
            "kcore-bridge-*",
            "kcore-dhcp-*",
        ])
        .output()
        .await
    {
        Ok(out) => out,
        Err(e) => {
            error!(error = %e, "failed to inspect failed kcore units");
            return;
        }
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    error!(
        context = context,
        exit_code = out.status.code().unwrap_or(-1),
        stdout = %stdout,
        stderr = %stderr,
        "detected failed kcore VM units after nix apply"
    );
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
        log_failed_kcore_units("after_nixos_rebuild_test_failure").await;
        return;
    }

    info!("nixos-rebuild test succeeded; starting nixos-rebuild switch");
    match run_rebuild_mode("switch").await {
        Ok(out) if out.status.success() => {
            info!("nixos-rebuild switch completed");
            log_failed_kcore_units("after_nixos_rebuild_switch_success").await;
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            error!(
                path = %path.display(),
                stderr = %stderr,
                "nixos-rebuild switch failed"
            );
            log_failed_kcore_units("after_nixos_rebuild_switch_failure").await;
        }
        Err(e) => {
            error!(path = %path.display(), error = %e, "failed to run nixos-rebuild switch");
            log_failed_kcore_units("after_nixos_rebuild_switch_spawn_failure").await;
        }
    }
}

fn validate_image_sha256(sha: &str) -> Result<String, Status> {
    let normalized = sha.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Status::invalid_argument(
            "image_sha256 must be exactly 64 hexadecimal characters",
        ));
    }
    Ok(normalized)
}

fn validate_image_url(url: &str) -> Result<String, Status> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument("image_url is required"));
    }
    if !trimmed.starts_with("https://") {
        return Err(Status::invalid_argument(
            "image_url must use https:// scheme",
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_destination_path(path: &str) -> Result<PathBuf, Status> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument("destination_path is required"));
    }
    let p = PathBuf::from(trimmed);
    if !p.is_absolute() {
        return Err(Status::invalid_argument(
            "destination_path must be an absolute path",
        ));
    }
    if !p.starts_with(IMAGE_CACHE_DIR) {
        return Err(Status::invalid_argument(format!(
            "destination_path must be under {IMAGE_CACHE_DIR}"
        )));
    }
    if trimmed.contains("..") {
        return Err(Status::invalid_argument(
            "destination_path must not contain path traversal",
        ));
    }
    Ok(p)
}

fn sha256sum_file(path: &std::path::Path) -> Result<String, Status> {
    let out = std::process::Command::new("sha256sum")
        .arg(path)
        .output()
        .map_err(|e| Status::internal(format!("running sha256sum on {}: {e}", path.display())))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(Status::internal(format!(
            "sha256sum failed for {}: {}",
            path.display(),
            stderr.trim()
        )));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let digest = stdout
        .split_whitespace()
        .next()
        .ok_or_else(|| Status::internal("invalid sha256sum output"))?;
    Ok(digest.to_ascii_lowercase())
}

fn ensure_image_cached(
    req: proto::EnsureImageRequest,
) -> Result<proto::EnsureImageResponse, Status> {
    let image_url = validate_image_url(&req.image_url)?;
    let image_sha256 = validate_image_sha256(&req.image_sha256)?;
    let destination = validate_destination_path(&req.destination_path)?;

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| Status::internal(format!("creating {}: {e}", parent.display())))?;
    }

    if destination.exists() {
        let existing_sha = sha256sum_file(&destination)?;
        if existing_sha == image_sha256 {
            let size_bytes = std::fs::metadata(&destination)
                .map_err(|e| Status::internal(format!("stat {}: {e}", destination.display())))?
                .len() as i64;
            return Ok(proto::EnsureImageResponse {
                path: destination.display().to_string(),
                size_bytes,
                cached: true,
                downloaded: false,
            });
        }
        std::fs::remove_file(&destination)
            .map_err(|e| Status::internal(format!("removing {}: {e}", destination.display())))?;
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| Status::internal(format!("system clock before UNIX_EPOCH: {e}")))?
        .as_millis();
    let tmp_path = PathBuf::from(format!("{}.part-{timestamp}", destination.display()));

    let status = std::process::Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--output",
            tmp_path.to_string_lossy().as_ref(),
            image_url.as_str(),
        ])
        .status()
        .map_err(|e| Status::internal(format!("starting curl: {e}")))?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(Status::internal(format!(
            "curl download failed for {}",
            image_url
        )));
    }

    let downloaded_sha = sha256sum_file(&tmp_path)?;
    if downloaded_sha != image_sha256 {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(Status::failed_precondition(format!(
            "sha256 mismatch for {} (expected {}, got {})",
            image_url, image_sha256, downloaded_sha
        )));
    }

    std::fs::rename(&tmp_path, &destination).map_err(|e| {
        Status::internal(format!(
            "moving {} to {}: {e}",
            tmp_path.display(),
            destination.display()
        ))
    })?;

    let size_bytes = std::fs::metadata(&destination)
        .map_err(|e| Status::internal(format!("stat {}: {e}", destination.display())))?
        .len() as i64;

    Ok(proto::EnsureImageResponse {
        path: destination.display().to_string(),
        size_bytes,
        cached: false,
        downloaded: true,
    })
}

fn is_private_key(filename: &str) -> bool {
    filename.ends_with(".key")
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

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = if is_private_key(name) { 0o600 } else { 0o644 };
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                .map_err(|e| Status::internal(format!("chmod {}: {e}", path.display())))?;
        }
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

fn build_install_command_args(req: &proto::InstallToDiskRequest) -> Result<Vec<String>, Status> {
    let has_controller = !req.controller.trim().is_empty();
    if has_controller == req.run_controller {
        return Err(Status::invalid_argument(
            "provide exactly one of controller or run_controller",
        ));
    }

    let mut args = vec![
        "--disk".to_string(),
        req.os_disk.clone(),
        "--yes".to_string(),
        "--wipe".to_string(),
        "--non-interactive".to_string(),
        "--reboot".to_string(),
    ];
    for dd in &req.data_disks {
        args.push("--data-disk".to_string());
        args.push(dd.clone());
    }
    if has_controller {
        args.push("--controller".to_string());
        args.push(req.controller.clone());
    }
    if req.run_controller {
        args.push("--run-controller".to_string());
    }
    Ok(args)
}

#[tonic::async_trait]
impl proto::node_admin_server::NodeAdmin for AdminService {
    async fn list_disks(
        &self,
        request: Request<proto::ListDisksRequest>,
    ) -> Result<Response<proto::ListDisksResponse>, Status> {
        auth::require_peer_insecure_ok(&request, &[CN_KCTL, CN_CONTROLLER])?;
        let disks = tokio::task::spawn_blocking(discovery::list_disks)
            .await
            .map_err(|e| Status::internal(format!("task join: {e}")))?
            .map_err(Status::internal)?;
        Ok(Response::new(proto::ListDisksResponse { disks }))
    }

    async fn list_network_interfaces(
        &self,
        request: Request<proto::ListNetworkInterfacesRequest>,
    ) -> Result<Response<proto::ListNetworkInterfacesResponse>, Status> {
        auth::require_peer_insecure_ok(&request, &[CN_KCTL, CN_CONTROLLER])?;
        let interfaces = tokio::task::spawn_blocking(discovery::list_network_interfaces)
            .await
            .map_err(|e| Status::internal(format!("task join: {e}")))?
            .map_err(Status::internal)?;
        Ok(Response::new(proto::ListNetworkInterfacesResponse {
            interfaces,
        }))
    }

    async fn apply_nix_config(
        &self,
        request: Request<proto::ApplyNixConfigRequest>,
    ) -> Result<Response<proto::ApplyNixConfigResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER])?;
        let req = request.into_inner();
        let path = self.nix_config_path.clone();

        let write_path = path.clone();
        let config_nix = req.configuration_nix.clone();
        tokio::task::spawn_blocking(move || std::fs::write(&write_path, &config_nix))
            .await
            .map_err(|e| Status::internal(format!("task join: {e}")))?
            .map_err(|e| {
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

    async fn ensure_image(
        &self,
        request: Request<proto::EnsureImageRequest>,
    ) -> Result<Response<proto::EnsureImageResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER])?;
        let req = request.into_inner();
        let resp = tokio::task::spawn_blocking(move || ensure_image_cached(req))
            .await
            .map_err(|e| Status::internal(format!("task join: {e}")))??;
        Ok(Response::new(resp))
    }

    async fn install_to_disk(
        &self,
        request: Request<proto::InstallToDiskRequest>,
    ) -> Result<Response<proto::InstallToDiskResponse>, Status> {
        auth::require_peer_insecure_ok(&request, &[CN_KCTL])?;
        let req = request.into_inner();
        if req.os_disk.is_empty() {
            return Err(Status::invalid_argument("os_disk is required"));
        }
        validate_disk_path(&req.os_disk, "os_disk")?;
        for (i, dd) in req.data_disks.iter().enumerate() {
            validate_disk_path(dd, &format!("data_disks[{i}]"))?;
        }

        let resp =
            tokio::task::spawn_blocking(move || -> Result<proto::InstallToDiskResponse, Status> {
                write_bootstrap_pki(&req)?;

                let args = build_install_command_args(&req)?;

                let cmd_str = format!("install-to-disk {}", args.join(" "));
                let (mut log_file, log_path) = prepare_install_log()?;
                use std::io::Write as _;
                writeln!(log_file, "Starting install command: {cmd_str}").map_err(|e| {
                    Status::internal(format!("writing {}: {e}", log_path.display()))
                })?;
                let stderr_log = log_file.try_clone().map_err(|e| {
                    Status::internal(format!("cloning {}: {e}", log_path.display()))
                })?;

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
                        Ok(proto::InstallToDiskResponse {
                            accepted: true,
                            message: format!(
                                "install started (pid {pid}): {cmd_str}; logs: {}",
                                log_path.display()
                            ),
                        })
                    }
                    Err(e) => Err(Status::internal(format!("failed to start install: {e}"))),
                }
            })
            .await
            .map_err(|e| Status::internal(format!("task join: {e}")))?;

        resp.map(Response::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::Request;

    #[test]
    fn bootstrap_pki_writes_supplied_materials_with_correct_permissions() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cert_dir = temp.path().join("certs");
        let req = proto::InstallToDiskRequest {
            os_disk: "/dev/sda".to_string(),
            data_disks: vec![],
            controller: String::new(),
            run_controller: false,
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

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let ca_mode = std::fs::metadata(cert_dir.join("ca.crt"))
                .expect("ca meta")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(ca_mode, 0o644, "certs should be world-readable");

            let key_mode = std::fs::metadata(cert_dir.join("node.key"))
                .expect("key meta")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(key_mode, 0o600, "private keys should be owner-only");

            let cert_mode = std::fs::metadata(cert_dir.join("node.crt"))
                .expect("cert meta")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(cert_mode, 0o644, "certs should be world-readable");
        }
    }

    #[test]
    fn bootstrap_pki_no_materials_is_noop() {
        let temp = tempfile::tempdir().expect("tempdir");
        let cert_dir = temp.path().join("certs");
        let req = proto::InstallToDiskRequest {
            os_disk: "/dev/sda".to_string(),
            data_disks: vec![],
            controller: String::new(),
            run_controller: false,
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
    fn validate_disk_path_accepts_valid_devices() {
        validate_disk_path("/dev/sda", "os_disk").expect("sda");
        validate_disk_path("/dev/nvme0n1", "os_disk").expect("nvme");
        validate_disk_path("/dev/disk/by-id/scsi-0", "d").expect("by-id");
    }

    #[test]
    fn validate_disk_path_rejects_invalid() {
        validate_disk_path("/tmp/sda", "d").expect_err("not /dev/");
        validate_disk_path("/dev/../etc/passwd", "d").expect_err("traversal");
        validate_disk_path("/dev/sd a", "d").expect_err("whitespace");
    }

    #[test]
    fn validate_image_inputs_reject_invalid_values() {
        validate_image_url("http://example.com/a.raw").expect_err("must be https");
        validate_image_sha256("1234").expect_err("sha must be 64 hex");
        validate_destination_path("/tmp/evil.raw").expect_err("must be under image cache dir");
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
    async fn apply_nix_config_requires_mtls_in_insecure_mode() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nix_path = temp.path().join("kcore-vms.nix");
        let svc = AdminService::new(nix_path.display().to_string());
        let req = proto::ApplyNixConfigRequest {
            configuration_nix: "{ ... }: { test = true; }\n".to_string(),
            rebuild: false,
        };

        let status = <AdminService as proto::node_admin_server::NodeAdmin>::apply_nix_config(
            &svc,
            Request::new(req),
        )
        .await
        .expect_err("apply should be denied without mTLS");

        assert_eq!(status.code(), tonic::Code::PermissionDenied);
        assert!(!nix_path.exists(), "denied request should not write config");
    }

    #[tokio::test]
    async fn insecure_mode_allows_only_discovery_and_install_admin_endpoints() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nix_path = temp.path().join("kcore-vms.nix");
        let svc = AdminService::new(nix_path.display().to_string());

        let apply = <AdminService as proto::node_admin_server::NodeAdmin>::apply_nix_config(
            &svc,
            Request::new(proto::ApplyNixConfigRequest {
                configuration_nix: "{ ... }: {}\n".to_string(),
                rebuild: false,
            }),
        )
        .await
        .expect_err("apply should require mTLS");
        assert_eq!(apply.code(), tonic::Code::PermissionDenied);

        let ensure_image = <AdminService as proto::node_admin_server::NodeAdmin>::ensure_image(
            &svc,
            Request::new(proto::EnsureImageRequest {
                image_url: "https://example.com/debian.raw".to_string(),
                image_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_string(),
                destination_path: "/var/lib/kcore/images/debian.raw".to_string(),
            }),
        )
        .await
        .expect_err("ensure_image should require mTLS");
        assert_eq!(ensure_image.code(), tonic::Code::PermissionDenied);

        let disks = <AdminService as proto::node_admin_server::NodeAdmin>::list_disks(
            &svc,
            Request::new(proto::ListDisksRequest {}),
        )
        .await;
        match disks {
            Ok(_) => {}
            Err(status) => {
                assert_ne!(
                    status.code(),
                    tonic::Code::PermissionDenied,
                    "list_disks should be allowed without TLS"
                );
            }
        }

        let nics = <AdminService as proto::node_admin_server::NodeAdmin>::list_network_interfaces(
            &svc,
            Request::new(proto::ListNetworkInterfacesRequest {}),
        )
        .await;
        match nics {
            Ok(_) => {}
            Err(status) => {
                assert_ne!(
                    status.code(),
                    tonic::Code::PermissionDenied,
                    "list_network_interfaces should be allowed without TLS"
                );
            }
        }

        let install = <AdminService as proto::node_admin_server::NodeAdmin>::install_to_disk(
            &svc,
            Request::new(proto::InstallToDiskRequest {
                os_disk: "/tmp/not-a-device".to_string(),
                data_disks: Vec::new(),
                controller: String::new(),
                run_controller: false,
                ca_cert_pem: String::new(),
                node_cert_pem: String::new(),
                node_key_pem: String::new(),
                controller_cert_pem: String::new(),
                controller_key_pem: String::new(),
                kctl_cert_pem: String::new(),
                kctl_key_pem: String::new(),
            }),
        )
        .await
        .expect_err("invalid disk path should fail after passing auth");
        assert_eq!(install.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn install_requires_exactly_one_controller_mode() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nix_path = temp.path().join("kcore-vms.nix");
        let svc = AdminService::new(nix_path.display().to_string());

        let neither = <AdminService as proto::node_admin_server::NodeAdmin>::install_to_disk(
            &svc,
            Request::new(proto::InstallToDiskRequest {
                os_disk: "/dev/sda".to_string(),
                data_disks: Vec::new(),
                controller: String::new(),
                run_controller: false,
                ca_cert_pem: String::new(),
                node_cert_pem: String::new(),
                node_key_pem: String::new(),
                controller_cert_pem: String::new(),
                controller_key_pem: String::new(),
                kctl_cert_pem: String::new(),
                kctl_key_pem: String::new(),
            }),
        )
        .await
        .expect_err("missing controller mode should fail");
        assert_eq!(neither.code(), tonic::Code::InvalidArgument);

        let both = <AdminService as proto::node_admin_server::NodeAdmin>::install_to_disk(
            &svc,
            Request::new(proto::InstallToDiskRequest {
                os_disk: "/dev/sda".to_string(),
                data_disks: Vec::new(),
                controller: "127.0.0.1:9090".to_string(),
                run_controller: true,
                ca_cert_pem: String::new(),
                node_cert_pem: String::new(),
                node_key_pem: String::new(),
                controller_cert_pem: String::new(),
                controller_key_pem: String::new(),
                kctl_cert_pem: String::new(),
                kctl_key_pem: String::new(),
            }),
        )
        .await
        .expect_err("ambiguous controller mode should fail");
        assert_eq!(both.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn build_install_command_args_join_controller_mode() {
        let req = proto::InstallToDiskRequest {
            os_disk: "/dev/sda".to_string(),
            data_disks: vec!["/dev/nvme0n1".to_string()],
            controller: "192.168.40.10:9090".to_string(),
            run_controller: false,
            ca_cert_pem: String::new(),
            node_cert_pem: String::new(),
            node_key_pem: String::new(),
            controller_cert_pem: String::new(),
            controller_key_pem: String::new(),
            kctl_cert_pem: String::new(),
            kctl_key_pem: String::new(),
        };
        let args = build_install_command_args(&req).expect("args");
        assert!(args.contains(&"--controller".to_string()));
        assert!(args.contains(&"192.168.40.10:9090".to_string()));
        assert!(!args.contains(&"--run-controller".to_string()));
        assert!(args.contains(&"--data-disk".to_string()));
        assert!(args.contains(&"/dev/nvme0n1".to_string()));
    }

    #[test]
    fn build_install_command_args_run_controller_mode() {
        let req = proto::InstallToDiskRequest {
            os_disk: "/dev/sda".to_string(),
            data_disks: Vec::new(),
            controller: String::new(),
            run_controller: true,
            ca_cert_pem: String::new(),
            node_cert_pem: String::new(),
            node_key_pem: String::new(),
            controller_cert_pem: String::new(),
            controller_key_pem: String::new(),
            kctl_cert_pem: String::new(),
            kctl_key_pem: String::new(),
        };
        let args = build_install_command_args(&req).expect("args");
        assert!(args.contains(&"--run-controller".to_string()));
        assert!(!args.contains(&"--controller".to_string()));
    }
}
