#![allow(clippy::result_large_err)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value as JsonValue;
use tokio::process::Command;
use tokio::sync::Mutex as AsyncMutex;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::auth::{self, CN_CONTROLLER_PREFIX, CN_KCTL};
use crate::discovery;
use crate::proto;
use crate::storage::{self, StorageAdapter};
pub struct AdminService {
    nix_config_path: PathBuf,
    vm_socket_dir: PathBuf,
    storage: Arc<dyn StorageAdapter>,
    apply_lock: Arc<AsyncMutex<()>>,
}

const BOOTSTRAP_CERT_DIR: &str = "/etc/kcore/certs";
const INSTALL_LOG_DIR: &str = "/var/log/kcore";
const NIXOS_CONFIG_PATH: &str = "/etc/nixos/configuration.nix";
const IMAGE_CACHE_DIR: &str = "/var/lib/kcore/images";
const DISKO_MANAGEMENT_MODE_PATH: &str = "/etc/kcore/disko-management-mode";
const DISKO_MODE_INSTALLER_ONLY: &str = "installer-only";
const DISKO_MODE_CONTROLLER_MANAGED: &str = "controller-managed";

async fn resolve_nixpkgs_path() -> Option<String> {
    for candidate in [
        "/nix/var/nix/profiles/per-user/root/channels/nixos",
        "/run/current-system/sw/share/nixpkgs",
    ] {
        if std::path::Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }

    if let Ok(contents) = tokio::fs::read_to_string(NIXOS_CONFIG_PATH).await {
        for line in contents.lines() {
            if let Some(rest) = line.trim().strip_prefix("nix.nixPath") {
                if let Some(start) = rest.find("nixpkgs=") {
                    let path_start = start + "nixpkgs=".len();
                    let path_end = rest[path_start..]
                        .find(['"', '\'', ']'])
                        .map(|i| path_start + i)
                        .unwrap_or(rest.len());
                    let p = rest[path_start..path_end].trim();
                    if !p.is_empty() && std::path::Path::new(p).exists() {
                        return Some(p.to_string());
                    }
                }
            }
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
        Self::new_with_storage(
            nix_config_path,
            "/run/kcore".to_string(),
            storage::default_adapter(),
        )
    }

    pub fn new_with_storage(
        nix_config_path: String,
        vm_socket_dir: String,
        storage: Arc<dyn StorageAdapter>,
    ) -> Self {
        Self {
            nix_config_path: PathBuf::from(nix_config_path),
            vm_socket_dir: PathBuf::from(vm_socket_dir),
            storage,
            apply_lock: Arc::new(AsyncMutex::new(())),
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

fn normalize_disko_management_mode(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        DISKO_MODE_CONTROLLER_MANAGED => DISKO_MODE_CONTROLLER_MANAGED,
        _ => DISKO_MODE_INSTALLER_ONLY,
    }
}

fn read_disko_management_mode() -> &'static str {
    let raw = std::fs::read_to_string(DISKO_MANAGEMENT_MODE_PATH)
        .unwrap_or_else(|_| DISKO_MODE_INSTALLER_ONLY.to_string());
    normalize_disko_management_mode(&raw)
}

fn validate_disko_timeout_seconds_or_default(timeout_seconds: i32) -> u64 {
    if timeout_seconds <= 0 {
        300
    } else {
        timeout_seconds.min(3600) as u64
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
    let nixpkgs_path = resolve_nixpkgs_path().await;
    if let Some(ref p) = nixpkgs_path {
        info!(nixpkgs_path = %p, "resolved nixpkgs path for rebuild");
    }
    let mut cmd = Command::new("nixos-rebuild");
    cmd.args(rebuild_args(mode));
    if let Some(ref p) = nixpkgs_path {
        cmd.env(
            "NIX_PATH",
            format!("nixos-config={NIXOS_CONFIG_PATH}:nixpkgs={p}"),
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
    if stdout.contains("0 loaded units listed.") {
        info!(
            context = context,
            exit_code = out.status.code().unwrap_or(-1),
            "no failed kcore VM units after nix apply"
        );
        return;
    }
    error!(
        context = context,
        exit_code = out.status.code().unwrap_or(-1),
        stdout = %stdout,
        stderr = %stderr,
        "detected failed kcore VM units after nix apply"
    );
}

fn parse_stopped_vms_from_nix(configuration_nix: &str) -> Vec<String> {
    let mut current_vm: Option<String> = None;
    let mut stopped = Vec::new();
    for raw in configuration_nix.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("virtualMachines.\"") {
            if let Some((vm_name, _)) = rest.split_once("\"") {
                current_vm = Some(vm_name.to_string());
            }
            continue;
        }
        if line.starts_with("};") {
            current_vm = None;
            continue;
        }
        if let Some(vm_name) = current_vm.as_ref() {
            if line == "autoStart = false;" {
                stopped.push(vm_name.clone());
            }
        }
    }
    stopped.sort();
    stopped.dedup();
    stopped
}

async fn enforce_stopped_vm_units(stopped_vms: &[String]) {
    for vm_name in stopped_vms {
        let unit = format!("kcore-vm-{vm_name}.service");
        let out = match Command::new("systemctl")
            .args(["stop", &unit])
            .output()
            .await
        {
            Ok(out) => out,
            Err(e) => {
                error!(vm_name = %vm_name, error = %e, "failed to spawn systemctl stop");
                continue;
            }
        };
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            error!(
                vm_name = %vm_name,
                unit = %unit,
                stderr = %stderr,
                "failed to enforce stopped VM unit after switch"
            );
        } else {
            info!(vm_name = %vm_name, unit = %unit, "enforced stopped VM unit");
        }
    }
}

async fn run_test_then_switch(path: PathBuf, _desired_stopped_vms: Vec<String>) {
    let nixpkgs_path = resolve_nixpkgs_path().await;
    let nix_path_val = nixpkgs_path
        .as_deref()
        .map(|p| format!("nixos-config={NIXOS_CONFIG_PATH}:nixpkgs={p}"))
        .unwrap_or_default();

    let script = format!(
        "set -e; export PATH=\"/run/current-system/sw/bin:$PATH\"; \
         export NIX_PATH='{nix_path_val}'; \
         nixos-rebuild test && nixos-rebuild switch"
    );

    info!(path = %path.display(), "launching nixos-rebuild test+switch via transient systemd unit");

    let _ = Command::new("systemctl")
        .args(["stop", "kcore-nix-rebuild.service"])
        .output()
        .await;
    let _ = Command::new("systemctl")
        .args(["reset-failed", "kcore-nix-rebuild.service"])
        .output()
        .await;

    let out = Command::new("systemd-run")
        .args([
            "--unit=kcore-nix-rebuild",
            "--collect",
            "--property=Type=exec",
            "--",
            "bash",
            "-c",
            &script,
        ])
        .output()
        .await;

    match out {
        Ok(o) if o.status.success() => {
            info!("kcore-nix-rebuild transient unit launched successfully");
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            error!(stderr = %stderr, "failed to launch kcore-nix-rebuild transient unit");
        }
        Err(e) => {
            error!(error = %e, "failed to spawn systemd-run for nix rebuild");
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

fn parse_lease_entry(line: &str) -> Option<(&str, &str, &str)> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 4 {
        return None;
    }
    let mac = fields[1];
    let ip = fields[2];
    let hostname = fields[3];
    Some((mac, ip, hostname))
}

fn find_vm_ip_in_lease_file(path: &Path, vm_name: &str, vm_mac: Option<&str>) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let target_mac = vm_mac.map(|m| m.to_ascii_lowercase());
    let mut matched_ip: Option<String> = None;
    for line in reader.lines().map_while(Result::ok) {
        let Some((mac, ip, hostname)) = parse_lease_entry(&line) else {
            continue;
        };
        let mac_match = target_mac
            .as_ref()
            .map(|target| mac.eq_ignore_ascii_case(target))
            .unwrap_or(false);
        let host_match = hostname == vm_name;
        if mac_match || host_match {
            matched_ip = Some(ip.to_string());
        }
    }
    matched_ip
}

fn vm_primary_mac(info: &crate::vmm::VmInfo) -> Option<String> {
    info.config
        .net
        .iter()
        .find_map(|n| n.mac.as_ref())
        .map(|m| m.to_ascii_lowercase())
}

fn lease_files_for_network(runtime_dir: &Path, network: &str) -> Vec<PathBuf> {
    if !network.trim().is_empty() {
        return vec![runtime_dir.join(format!("dnsmasq-{}.leases", network.trim()))];
    }
    let Ok(entries) = std::fs::read_dir(runtime_dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|name| name.starts_with("dnsmasq-") && name.ends_with(".leases"))
        })
        .collect()
}

fn validate_port_or_default(port: i32) -> u16 {
    if port <= 0 {
        22
    } else {
        port.clamp(1, u16::MAX as i32) as u16
    }
}

fn validate_timeout_ms_or_default(timeout_ms: i32) -> u64 {
    if timeout_ms <= 0 {
        1500
    } else {
        timeout_ms as u64
    }
}

fn parse_i64_field(v: &JsonValue, key: &str) -> i64 {
    if let Some(n) = v.get(key).and_then(|x| x.as_i64()) {
        return n;
    }
    v.get(key)
        .and_then(|x| x.as_str())
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(0)
}

fn parse_string_field(v: &JsonValue, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

async fn lvm_report_json(bin: &str, args: &[&str]) -> Result<JsonValue, String> {
    let resolved = resolve_lvm_bin(bin);
    let out = Command::new(&resolved)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("spawn {resolved} {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "{bin} {} failed: {}",
            args.join(" "),
            stderr.trim()
        ));
    }
    serde_json::from_slice::<JsonValue>(&out.stdout)
        .map_err(|e| format!("parse {resolved} {} json: {e}", args.join(" ")))
}

fn resolve_lvm_bin(bin: &str) -> String {
    let candidates = [
        format!("/run/current-system/sw/bin/{bin}"),
        format!("/nix/var/nix/profiles/default/bin/{bin}"),
        format!("/usr/sbin/{bin}"),
        format!("/usr/bin/{bin}"),
        bin.to_string(),
    ];
    for c in candidates {
        if std::path::Path::new(&c).exists() || !c.starts_with('/') {
            return c;
        }
    }
    bin.to_string()
}

async fn collect_lvm_info() -> proto::GetLvmInfoResponse {
    let vg = lvm_report_json(
        "vgs",
        &[
            "--reportformat",
            "json",
            "--units",
            "b",
            "--nosuffix",
            "-o",
            "vg_name,vg_size,vg_free,vg_attr",
        ],
    )
    .await;
    let lv = lvm_report_json(
        "lvs",
        &[
            "--reportformat",
            "json",
            "--units",
            "b",
            "--nosuffix",
            "-o",
            "lv_name,vg_name,lv_size,lv_attr,lv_path,pool_lv,origin,data_percent,metadata_percent",
        ],
    )
    .await;
    let pv = lvm_report_json(
        "pvs",
        &[
            "--reportformat",
            "json",
            "--units",
            "b",
            "--nosuffix",
            "-o",
            "pv_name,vg_name,pv_size,pv_free,pv_attr",
        ],
    )
    .await;

    if vg.is_err() && lv.is_err() && pv.is_err() {
        return proto::GetLvmInfoResponse {
            available: false,
            message: "lvm tooling unavailable or no readable reports".to_string(),
            volume_groups: vec![],
            logical_volumes: vec![],
            physical_volumes: vec![],
        };
    }

    let mut volume_groups = Vec::new();
    if let Ok(vg_json) = vg {
        if let Some(vgs) = vg_json
            .get("report")
            .and_then(|r| r.as_array())
            .and_then(|r| r.first())
            .and_then(|x| x.get("vg"))
            .and_then(|x| x.as_array())
        {
            for row in vgs {
                volume_groups.push(proto::LvmVolumeGroupInfo {
                    name: parse_string_field(row, "vg_name"),
                    size_bytes: parse_i64_field(row, "vg_size"),
                    free_bytes: parse_i64_field(row, "vg_free"),
                    attr: parse_string_field(row, "vg_attr"),
                });
            }
        }
    }

    let mut logical_volumes = Vec::new();
    if let Ok(lv_json) = lv {
        if let Some(lvs) = lv_json
            .get("report")
            .and_then(|r| r.as_array())
            .and_then(|r| r.first())
            .and_then(|x| x.get("lv"))
            .and_then(|x| x.as_array())
        {
            for row in lvs {
                logical_volumes.push(proto::LvmLogicalVolumeInfo {
                    name: parse_string_field(row, "lv_name"),
                    vg_name: parse_string_field(row, "vg_name"),
                    size_bytes: parse_i64_field(row, "lv_size"),
                    attr: parse_string_field(row, "lv_attr"),
                    path: parse_string_field(row, "lv_path"),
                    pool: parse_string_field(row, "pool_lv"),
                    origin: parse_string_field(row, "origin"),
                    data_percent: parse_string_field(row, "data_percent"),
                    metadata_percent: parse_string_field(row, "metadata_percent"),
                });
            }
        }
    }

    let mut physical_volumes = Vec::new();
    if let Ok(pv_json) = pv {
        if let Some(pvs) = pv_json
            .get("report")
            .and_then(|r| r.as_array())
            .and_then(|r| r.first())
            .and_then(|x| x.get("pv"))
            .and_then(|x| x.as_array())
        {
            for row in pvs {
                physical_volumes.push(proto::LvmPhysicalVolumeInfo {
                    name: parse_string_field(row, "pv_name"),
                    vg_name: parse_string_field(row, "vg_name"),
                    size_bytes: parse_i64_field(row, "pv_size"),
                    free_bytes: parse_i64_field(row, "pv_free"),
                    attr: parse_string_field(row, "pv_attr"),
                });
            }
        }
    }

    proto::GetLvmInfoResponse {
        available: true,
        message: "ok".to_string(),
        volume_groups,
        logical_volumes,
        physical_volumes,
    }
}

#[derive(Debug, Default, Clone)]
struct VmUnitState {
    active_state: String,
    sub_state: String,
    result: String,
    n_restarts: u32,
}

fn vm_unit_name(vm_name: &str) -> String {
    format!("kcore-vm-{vm_name}.service")
}

async fn read_vm_unit_state(vm_name: &str) -> Option<VmUnitState> {
    let unit = vm_unit_name(vm_name);
    let out = Command::new("systemctl")
        .args([
            "show",
            "--property=ActiveState,SubState,Result,NRestarts",
            &unit,
        ])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut state = VmUnitState::default();
    for line in stdout.lines() {
        if let Some(v) = line.strip_prefix("ActiveState=") {
            state.active_state = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("SubState=") {
            state.sub_state = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("Result=") {
            state.result = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("NRestarts=") {
            state.n_restarts = v.trim().parse::<u32>().unwrap_or(0);
        }
    }
    Some(state)
}

async fn vm_recent_failure_hint(vm_name: &str) -> Option<String> {
    let unit = vm_unit_name(vm_name);
    let out = Command::new("journalctl")
        .args(["-u", &unit, "-n", "12", "--no-pager"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|s| s.trim().to_string())
}

fn vm_unit_is_fatal(state: &VmUnitState) -> bool {
    if state.active_state == "failed" {
        return true;
    }
    // auto-restart flapping with repeated exit failures is effectively fatal for readiness.
    state.result == "exit-code"
        && (state.active_state == "activating" || state.sub_state == "auto-restart")
        && state.n_restarts >= 3
}

fn parse_neigh_line(line: &str) -> Option<(&str, &str)> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 5 {
        return None;
    }
    let ip = fields[0];
    let lladdr_idx = fields.iter().position(|f| *f == "lladdr")?;
    if lladdr_idx + 1 >= fields.len() {
        return None;
    }
    let mac = fields[lladdr_idx + 1];
    Some((ip, mac))
}

async fn find_vm_ip_in_neigh(vm_mac: &str, network: &str) -> Option<String> {
    let mut args = vec!["neigh".to_string(), "show".to_string()];
    let net = network.trim();
    if !net.is_empty() {
        args.push("dev".to_string());
        args.push(format!("kbr-{net}"));
    }
    let out = Command::new("ip").args(args).output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut last_match: Option<String> = None;
    for line in stdout.lines() {
        let Some((ip, mac)) = parse_neigh_line(line) else {
            continue;
        };
        if mac.eq_ignore_ascii_case(vm_mac) {
            last_match = Some(ip.to_string());
        }
    }
    last_match
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
        ("sub-ca.crt", &req.sub_ca_cert_pem),
        ("sub-ca.key", &req.sub_ca_key_pem),
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

fn normalize_endpoint(s: &str, default_port: u16) -> String {
    let trimmed = s.trim();
    if trimmed.parse::<std::net::SocketAddr>().is_ok() {
        return trimmed.to_string();
    }
    if trimmed.starts_with('[') {
        return format!("{trimmed}:{default_port}");
    }
    let colon_count = trimmed.chars().filter(|&c| c == ':').count();
    if colon_count > 1 {
        return format!("[{trimmed}]:{default_port}");
    }
    if colon_count == 1 {
        return trimmed.to_string();
    }
    format!("{trimmed}:{default_port}")
}

fn build_install_command_args(req: &proto::InstallToDiskRequest) -> Result<Vec<String>, Status> {
    let mut controllers: Vec<String> = req
        .controllers
        .iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| normalize_endpoint(&v, 9090))
        .collect();
    if controllers.is_empty() && !req.controller.trim().is_empty() {
        let c = req.controller.trim().to_string();
        controllers.push(normalize_endpoint(&c, 9090));
    }
    let has_controller = !controllers.is_empty();
    if !has_controller && !req.run_controller {
        return Err(Status::invalid_argument(
            "provide --controller or --run-controller (or both for a joining controller)",
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
        for controller in &controllers {
            args.push("--controller".to_string());
            args.push(controller.clone());
        }
    } else if !req.controller.trim().is_empty() {
        args.push("--controller".to_string());
        args.push(req.controller.trim().to_string());
    }
    if req.run_controller {
        args.push("--run-controller".to_string());
    }
    let typed_mode = match proto::StorageBackendType::try_from(req.storage_backend)
        .unwrap_or(proto::StorageBackendType::Unspecified)
    {
        proto::StorageBackendType::Filesystem => "filesystem",
        proto::StorageBackendType::Lvm => "lvm",
        proto::StorageBackendType::Zfs => "zfs",
        proto::StorageBackendType::Unspecified => "",
    };
    let mode = if typed_mode.is_empty() {
        req.data_disk_mode.trim()
    } else {
        typed_mode
    };
    if !mode.is_empty() {
        args.push("--data-disk-mode".to_string());
        args.push(mode.to_string());
    }
    if mode == "lvm" {
        if !req.lvm_vg_name.trim().is_empty() {
            args.push("--lvm-vg-name".to_string());
            args.push(req.lvm_vg_name.trim().to_string());
        }
        if !req.lvm_lv_prefix.trim().is_empty() {
            args.push("--lvm-lv-prefix".to_string());
            args.push(req.lvm_lv_prefix.trim().to_string());
        }
    }
    if mode == "zfs" {
        if !req.zfs_pool_name.trim().is_empty() {
            args.push("--zfs-pool-name".to_string());
            args.push(req.zfs_pool_name.trim().to_string());
        }
        if !req.zfs_dataset_prefix.trim().is_empty() {
            args.push("--zfs-dataset-prefix".to_string());
            args.push(req.zfs_dataset_prefix.trim().to_string());
        }
    }
    if req.disable_vxlan {
        args.push("--disable-vxlan".to_string());
    }
    if !req.dc_id.trim().is_empty() {
        args.push("--dc-id".to_string());
        args.push(req.dc_id.trim().to_string());
    }
    if !req.hostname.trim().is_empty() {
        args.push("--hostname".to_string());
        args.push(req.hostname.trim().to_string());
    }
    if !req.node_id.trim().is_empty() {
        args.push("--node-id".to_string());
        args.push(req.node_id.trim().to_string());
    }
    Ok(args)
}

#[tonic::async_trait]
impl proto::node_admin_server::NodeAdmin for AdminService {
    async fn list_disks(
        &self,
        request: Request<proto::ListDisksRequest>,
    ) -> Result<Response<proto::ListDisksResponse>, Status> {
        auth::require_peer_insecure_ok(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
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
        auth::require_peer_insecure_ok(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let interfaces = tokio::task::spawn_blocking(discovery::list_network_interfaces)
            .await
            .map_err(|e| Status::internal(format!("task join: {e}")))?
            .map_err(Status::internal)?;
        Ok(Response::new(proto::ListNetworkInterfacesResponse {
            interfaces,
        }))
    }

    async fn get_lvm_info(
        &self,
        request: Request<proto::GetLvmInfoRequest>,
    ) -> Result<Response<proto::GetLvmInfoResponse>, Status> {
        auth::require_peer_insecure_ok(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        Ok(Response::new(collect_lvm_info().await))
    }

    async fn apply_nix_config(
        &self,
        request: Request<proto::ApplyNixConfigRequest>,
    ) -> Result<Response<proto::ApplyNixConfigResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX])?;
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
        let desired_stopped_vms = parse_stopped_vms_from_nix(&req.configuration_nix);
        let apply_lock = Arc::clone(&self.apply_lock);
        tokio::spawn(async move {
            let _guard = apply_lock.lock().await;
            run_test_then_switch(rebuild_path, desired_stopped_vms).await;
        });

        Ok(Response::new(proto::ApplyNixConfigResponse {
            success: true,
            message: format!(
                "config written to {}; nixos-rebuild test+switch started",
                path.display()
            ),
        }))
    }

    async fn apply_disko_layout(
        &self,
        request: Request<proto::ApplyDiskoLayoutRequest>,
    ) -> Result<Response<proto::ApplyDiskoLayoutResponse>, Status> {
        auth::require_peer_insecure_ok(&request, &[CN_KCTL, CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let mode = read_disko_management_mode();

        if req.disko_nix.trim().is_empty() {
            return Err(Status::invalid_argument("disko_nix cannot be empty"));
        }
        if !req.disko_nix.contains("disko.devices") {
            return Err(Status::invalid_argument(
                "disko_nix must define disko.devices",
            ));
        }
        if req.apply && mode != DISKO_MODE_CONTROLLER_MANAGED {
            return Ok(Response::new(proto::ApplyDiskoLayoutResponse {
                success: false,
                message:
                    "node is in installer-only disko mode; enable controller-managed mode first"
                        .to_string(),
                mode: mode.to_string(),
            }));
        }

        let timeout_seconds = validate_disko_timeout_seconds_or_default(req.timeout_seconds);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Status::internal(format!("system clock error: {e}")))?
            .as_secs();
        let temp_path = PathBuf::from(format!("/tmp/kcore-disko-day2-{timestamp}.nix"));

        let write_path = temp_path.clone();
        let disko_nix = req.disko_nix.clone();
        tokio::task::spawn_blocking(move || std::fs::write(&write_path, disko_nix))
            .await
            .map_err(|e| Status::internal(format!("task join: {e}")))?
            .map_err(|e| {
                Status::internal(format!("writing disko config {}: {e}", temp_path.display()))
            })?;

        let mut cmd = Command::new("timeout");
        cmd.args([format!("{timeout_seconds}s")]);
        if req.apply {
            cmd.arg("disko")
                .arg("--mode")
                .arg("format,mount")
                .arg(temp_path.as_os_str());
        } else {
            cmd.arg("nix-instantiate")
                .arg("--parse")
                .arg(temp_path.as_os_str());
        }
        let output = cmd
            .output()
            .await
            .map_err(|e| Status::internal(format!("running disko command: {e}")))?;
        let _ = tokio::fs::remove_file(&temp_path).await;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if output.status.success() {
            let action = if req.apply { "applied" } else { "validated" };
            let detail = if stdout.is_empty() {
                format!("disko layout {action} successfully")
            } else {
                format!("disko layout {action} successfully: {stdout}")
            };
            return Ok(Response::new(proto::ApplyDiskoLayoutResponse {
                success: true,
                message: detail,
                mode: mode.to_string(),
            }));
        }

        let detail = if stderr.is_empty() {
            format!("disko command failed with status {}", output.status)
        } else {
            format!("disko command failed: {stderr}")
        };
        Ok(Response::new(proto::ApplyDiskoLayoutResponse {
            success: false,
            message: detail,
            mode: mode.to_string(),
        }))
    }

    async fn ensure_image(
        &self,
        request: Request<proto::EnsureImageRequest>,
    ) -> Result<Response<proto::EnsureImageResponse>, Status> {
        auth::require_peer(&request, &[CN_CONTROLLER_PREFIX])?;
        let req = request.into_inner();
        let storage = Arc::clone(&self.storage);
        let resp = tokio::task::spawn_blocking(move || {
            storage
                .ensure_image(storage::EnsureImageRequest {
                    image_url: req.image_url,
                    image_sha256: req.image_sha256,
                    destination_path: req.destination_path,
                })
                .map(storage::ensure_image_response)
        })
        .await
        .map_err(|e| Status::internal(format!("task join: {e}")))??;
        Ok(Response::new(resp))
    }

    async fn upload_image(
        &self,
        request: Request<proto::UploadImageRequest>,
    ) -> Result<Response<proto::UploadImageResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();
        let storage = Arc::clone(&self.storage);
        let resp = tokio::task::spawn_blocking(move || {
            storage
                .upload_image(storage::UploadImageRequest {
                    image_bytes: req.image_bytes,
                    source_name: req.source_name,
                    destination_name: req.destination_name,
                    image_format: req.image_format,
                    image_sha256: req.image_sha256,
                })
                .map(storage::upload_image_response)
        })
        .await
        .map_err(|e| Status::internal(format!("task join: {e}")))??;
        Ok(Response::new(resp))
    }

    async fn upload_image_stream(
        &self,
        request: Request<tonic::Streaming<proto::UploadImageChunk>>,
    ) -> Result<Response<proto::UploadImageResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let mut stream = request.into_inner();
        let first = stream
            .message()
            .await?
            .ok_or_else(|| Status::invalid_argument("upload stream is empty"))?;

        let source_name = first.source_name.clone();
        let destination_name = first.destination_name.clone();
        let image_format = first.image_format.clone();
        let image_sha256 = first.image_sha256.clone();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Status::internal(format!("system clock error: {e}")))?
            .as_millis();
        let tmp_path = PathBuf::from(format!("/tmp/kcore-upload-{timestamp}.part"));
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| Status::internal(format!("creating {}: {e}", tmp_path.display())))?;

        if !first.chunk_data.is_empty() {
            tokio::io::AsyncWriteExt::write_all(&mut file, &first.chunk_data)
                .await
                .map_err(|e| Status::internal(format!("writing {}: {e}", tmp_path.display())))?;
        }

        while let Some(chunk) = stream.message().await? {
            if !chunk.image_format.trim().is_empty()
                && chunk.image_format.trim() != image_format.trim()
            {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(Status::invalid_argument(
                    "image_format must be consistent across stream chunks",
                ));
            }
            if !chunk.image_sha256.trim().is_empty()
                && chunk.image_sha256.trim() != image_sha256.trim()
            {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(Status::invalid_argument(
                    "image_sha256 must be consistent across stream chunks",
                ));
            }
            if !chunk.source_name.trim().is_empty()
                && chunk.source_name.trim() != source_name.trim()
            {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(Status::invalid_argument(
                    "source_name must be consistent across stream chunks",
                ));
            }
            if !chunk.destination_name.trim().is_empty()
                && chunk.destination_name.trim() != destination_name.trim()
            {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                return Err(Status::invalid_argument(
                    "destination_name must be consistent across stream chunks",
                ));
            }
            if !chunk.chunk_data.is_empty() {
                tokio::io::AsyncWriteExt::write_all(&mut file, &chunk.chunk_data)
                    .await
                    .map_err(|e| {
                        Status::internal(format!("writing {}: {e}", tmp_path.display()))
                    })?;
            }
        }

        tokio::io::AsyncWriteExt::flush(&mut file)
            .await
            .map_err(|e| Status::internal(format!("flushing {}: {e}", tmp_path.display())))?;
        drop(file);

        let storage = Arc::clone(&self.storage);
        let tmp_path_for_upload = tmp_path.clone();
        let resp = tokio::task::spawn_blocking(move || {
            storage
                .upload_image_from_path(storage::UploadImageFromPathRequest {
                    source_file_path: tmp_path_for_upload.display().to_string(),
                    source_name,
                    destination_name,
                    image_format,
                    image_sha256,
                })
                .map(storage::upload_image_response)
        })
        .await
        .map_err(|e| Status::internal(format!("task join: {e}")))??;

        let _ = tokio::fs::remove_file(&tmp_path).await;
        Ok(Response::new(resp))
    }

    async fn check_vm_ssh_ready(
        &self,
        request: Request<proto::CheckVmSshReadyRequest>,
    ) -> Result<Response<proto::CheckVmSshReadyResponse>, Status> {
        auth::require_peer(&request, &[CN_KCTL])?;
        let req = request.into_inner();
        let vm_name = req.vm_name.trim();
        if vm_name.is_empty() {
            return Err(Status::invalid_argument("vm_name is required"));
        }
        let port = validate_port_or_default(req.port);
        let timeout_ms = validate_timeout_ms_or_default(req.timeout_ms);
        if let Some(unit_state) = read_vm_unit_state(vm_name).await {
            if vm_unit_is_fatal(&unit_state) {
                let hint = vm_recent_failure_hint(vm_name)
                    .await
                    .unwrap_or_else(|| "see journalctl for VM unit details".to_string());
                return Ok(Response::new(proto::CheckVmSshReadyResponse {
                    ready: false,
                    ip: String::new(),
                    port: port as i32,
                    reason: format!(
                        "VM unit {} is failing (active={}, sub={}, result={}, restarts={}): {}",
                        vm_unit_name(vm_name),
                        unit_state.active_state,
                        unit_state.sub_state,
                        unit_state.result,
                        unit_state.n_restarts,
                        hint
                    ),
                    fatal: true,
                }));
            }
        }
        let vmm = crate::vmm::Client::new(&self.vm_socket_dir.display().to_string());
        let vm_info = vmm.get_vm_info(vm_name).await;
        let vm_mac = vm_info.as_ref().and_then(vm_primary_mac);
        let lease_files = lease_files_for_network(&self.vm_socket_dir, &req.network);
        if lease_files.is_empty() {
            return Ok(Response::new(proto::CheckVmSshReadyResponse {
                ready: false,
                ip: String::new(),
                port: port as i32,
                reason: format!(
                    "no dnsmasq lease files found in {}",
                    self.vm_socket_dir.display()
                ),
                fatal: false,
            }));
        }

        let mut vm_ip = None;
        for lease in &lease_files {
            if let Some(ip) = find_vm_ip_in_lease_file(lease, vm_name, vm_mac.as_deref()) {
                vm_ip = Some(ip);
                break;
            }
        }
        let Some(ip) = vm_ip else {
            if let Some(mac) = vm_mac.as_deref() {
                if let Some(ip) = find_vm_ip_in_neigh(mac, &req.network).await {
                    let connect = tokio::time::timeout(
                        std::time::Duration::from_millis(timeout_ms),
                        tokio::net::TcpStream::connect((ip.as_str(), port)),
                    )
                    .await;
                    return match connect {
                        Ok(Ok(_stream)) => Ok(Response::new(proto::CheckVmSshReadyResponse {
                            ready: true,
                            ip,
                            port: port as i32,
                            reason: "ssh port reachable (ip discovered via arp/neigh)".to_string(),
                            fatal: false,
                        })),
                        Ok(Err(e)) => Ok(Response::new(proto::CheckVmSshReadyResponse {
                            ready: false,
                            ip,
                            port: port as i32,
                            reason: format!("arp/neigh found IP but tcp connect failed: {e}"),
                            fatal: false,
                        })),
                        Err(_) => Ok(Response::new(proto::CheckVmSshReadyResponse {
                            ready: false,
                            ip,
                            port: port as i32,
                            reason: format!(
                                "arp/neigh found IP but tcp connect timed out after {timeout_ms}ms"
                            ),
                            fatal: false,
                        })),
                    };
                }
            }
            return Ok(Response::new(proto::CheckVmSshReadyResponse {
                ready: false,
                ip: String::new(),
                port: port as i32,
                reason: if vm_mac.is_some() {
                    "no DHCP lease found for VM yet (and no arp/neigh match for VM MAC)".to_string()
                } else {
                    "no DHCP lease found for VM yet (VM MAC unavailable)".to_string()
                },
                fatal: false,
            }));
        };

        let connect = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            tokio::net::TcpStream::connect((ip.as_str(), port)),
        )
        .await;
        match connect {
            Ok(Ok(_stream)) => Ok(Response::new(proto::CheckVmSshReadyResponse {
                ready: true,
                ip,
                port: port as i32,
                reason: "ssh port reachable".to_string(),
                fatal: false,
            })),
            Ok(Err(e)) => Ok(Response::new(proto::CheckVmSshReadyResponse {
                ready: false,
                ip,
                port: port as i32,
                reason: format!("tcp connect failed: {e}"),
                fatal: false,
            })),
            Err(_) => Ok(Response::new(proto::CheckVmSshReadyResponse {
                ready: false,
                ip,
                port: port as i32,
                reason: format!("tcp connect timed out after {timeout_ms}ms"),
                fatal: false,
            })),
        }
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

                let mut args = build_install_command_args(&req)?;

                let tpm_present = std::path::Path::new("/sys/class/tpm/tpm0").exists();
                let luks_method = if tpm_present { "tpm2" } else { "key-file" };
                args.push("--luks-method".to_string());
                args.push(luks_method.to_string());

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
                            luks_method: luks_method.to_string(),
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
            data_disk_mode: String::new(),
            storage_backend: proto::StorageBackendType::Unspecified as i32,
            lvm_vg_name: String::new(),
            lvm_lv_prefix: String::new(),
            zfs_pool_name: String::new(),
            zfs_dataset_prefix: String::new(),
            disable_vxlan: false,
            sub_ca_cert_pem: String::new(),
            sub_ca_key_pem: String::new(),
            controllers: Vec::new(),
            dc_id: String::new(),
            hostname: String::new(),
            node_id: String::new(),
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
            data_disk_mode: String::new(),
            storage_backend: proto::StorageBackendType::Unspecified as i32,
            lvm_vg_name: String::new(),
            lvm_lv_prefix: String::new(),
            zfs_pool_name: String::new(),
            zfs_dataset_prefix: String::new(),
            disable_vxlan: false,
            sub_ca_cert_pem: String::new(),
            sub_ca_key_pem: String::new(),
            controllers: Vec::new(),
            dc_id: String::new(),
            hostname: String::new(),
            node_id: String::new(),
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
                data_disk_mode: String::new(),
                storage_backend: proto::StorageBackendType::Unspecified as i32,
                lvm_vg_name: String::new(),
                lvm_lv_prefix: String::new(),
                zfs_pool_name: String::new(),
                zfs_dataset_prefix: String::new(),
                disable_vxlan: false,
                sub_ca_cert_pem: String::new(),
                sub_ca_key_pem: String::new(),
                controllers: Vec::new(),
                dc_id: String::new(),
                hostname: String::new(),
                node_id: String::new(),
            }),
        )
        .await
        .expect_err("invalid disk path should fail after passing auth");
        assert_eq!(install.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn install_rejects_neither_controller_mode() {
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
                data_disk_mode: String::new(),
                storage_backend: proto::StorageBackendType::Unspecified as i32,
                lvm_vg_name: String::new(),
                lvm_lv_prefix: String::new(),
                zfs_pool_name: String::new(),
                zfs_dataset_prefix: String::new(),
                disable_vxlan: false,
                sub_ca_cert_pem: String::new(),
                sub_ca_key_pem: String::new(),
                controllers: Vec::new(),
                dc_id: String::new(),
                hostname: String::new(),
                node_id: String::new(),
            }),
        )
        .await
        .expect_err("missing controller mode should fail");
        assert_eq!(neither.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn build_install_args_accepts_run_controller_with_peers() {
        let req = proto::InstallToDiskRequest {
            os_disk: "/dev/sda".to_string(),
            data_disks: Vec::new(),
            controller: String::new(),
            run_controller: true,
            controllers: vec!["192.168.40.105:9090".to_string()],
            ca_cert_pem: String::new(),
            node_cert_pem: String::new(),
            node_key_pem: String::new(),
            controller_cert_pem: String::new(),
            controller_key_pem: String::new(),
            kctl_cert_pem: String::new(),
            kctl_key_pem: String::new(),
            data_disk_mode: String::new(),
            storage_backend: proto::StorageBackendType::Unspecified as i32,
            lvm_vg_name: String::new(),
            lvm_lv_prefix: String::new(),
            zfs_pool_name: String::new(),
            zfs_dataset_prefix: String::new(),
            disable_vxlan: false,
            sub_ca_cert_pem: String::new(),
            sub_ca_key_pem: String::new(),
            dc_id: String::new(),
            hostname: String::new(),
            node_id: String::new(),
        };
        let args = build_install_command_args(&req).expect("should accept both flags");
        assert!(args.contains(&"--run-controller".to_string()));
        assert!(args.contains(&"--controller".to_string()));
        assert!(args.contains(&"192.168.40.105:9090".to_string()));
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
            data_disk_mode: "filesystem".to_string(),
            storage_backend: proto::StorageBackendType::Filesystem as i32,
            lvm_vg_name: String::new(),
            lvm_lv_prefix: String::new(),
            zfs_pool_name: String::new(),
            zfs_dataset_prefix: String::new(),
            disable_vxlan: false,
            sub_ca_cert_pem: String::new(),
            sub_ca_key_pem: String::new(),
            controllers: vec![
                "192.168.40.10:9090".to_string(),
                "192.168.40.11:9090".to_string(),
            ],
            dc_id: "DC1".to_string(),
            hostname: String::new(),
            node_id: String::new(),
        };
        let args = build_install_command_args(&req).expect("args");
        assert!(args.contains(&"--controller".to_string()));
        assert!(args.contains(&"192.168.40.10:9090".to_string()));
        assert!(args.contains(&"192.168.40.11:9090".to_string()));
        assert_eq!(
            args.iter().filter(|v| v.as_str() == "--controller").count(),
            2
        );
        assert!(!args.contains(&"--run-controller".to_string()));
        assert!(args.contains(&"--data-disk".to_string()));
        assert!(args.contains(&"/dev/nvme0n1".to_string()));
        assert!(args.contains(&"--dc-id".to_string()));
        assert!(args.contains(&"DC1".to_string()));
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
            data_disk_mode: "zfs".to_string(),
            storage_backend: proto::StorageBackendType::Zfs as i32,
            lvm_vg_name: String::new(),
            lvm_lv_prefix: String::new(),
            zfs_pool_name: "tank0".to_string(),
            zfs_dataset_prefix: "kcore-".to_string(),
            disable_vxlan: false,
            sub_ca_cert_pem: String::new(),
            sub_ca_key_pem: String::new(),
            controllers: Vec::new(),
            dc_id: String::new(),
            hostname: String::new(),
            node_id: String::new(),
        };
        let args = build_install_command_args(&req).expect("args");
        assert!(args.contains(&"--run-controller".to_string()));
        assert!(!args.contains(&"--controller".to_string()));
        assert!(args.contains(&"--data-disk-mode".to_string()));
        assert!(args.contains(&"zfs".to_string()));
        assert!(args.contains(&"--zfs-pool-name".to_string()));
        assert!(args.contains(&"tank0".to_string()));
        assert!(args.contains(&"--zfs-dataset-prefix".to_string()));
        assert!(args.contains(&"kcore-".to_string()));
        assert!(!args.contains(&"--disable-vxlan".to_string()));
    }

    #[test]
    fn build_install_command_args_disable_vxlan_flag() {
        let req = proto::InstallToDiskRequest {
            os_disk: "/dev/sda".to_string(),
            data_disks: Vec::new(),
            controller: "192.168.1.10:9090".to_string(),
            run_controller: false,
            ca_cert_pem: String::new(),
            node_cert_pem: String::new(),
            node_key_pem: String::new(),
            controller_cert_pem: String::new(),
            controller_key_pem: String::new(),
            kctl_cert_pem: String::new(),
            kctl_key_pem: String::new(),
            data_disk_mode: String::new(),
            storage_backend: proto::StorageBackendType::Filesystem as i32,
            lvm_vg_name: String::new(),
            lvm_lv_prefix: String::new(),
            zfs_pool_name: String::new(),
            zfs_dataset_prefix: String::new(),
            disable_vxlan: true,
            sub_ca_cert_pem: String::new(),
            sub_ca_key_pem: String::new(),
            controllers: Vec::new(),
            dc_id: String::new(),
            hostname: String::new(),
            node_id: String::new(),
        };
        let args = build_install_command_args(&req).expect("args");
        assert!(args.contains(&"--disable-vxlan".to_string()));
    }

    #[test]
    fn build_install_command_args_single_disk_no_data_disks() {
        let req = proto::InstallToDiskRequest {
            os_disk: "/dev/sda".to_string(),
            data_disks: Vec::new(),
            controller: "192.168.1.10:9090".to_string(),
            run_controller: false,
            ca_cert_pem: String::new(),
            node_cert_pem: String::new(),
            node_key_pem: String::new(),
            controller_cert_pem: String::new(),
            controller_key_pem: String::new(),
            kctl_cert_pem: String::new(),
            kctl_key_pem: String::new(),
            data_disk_mode: String::new(),
            storage_backend: proto::StorageBackendType::Filesystem as i32,
            lvm_vg_name: String::new(),
            lvm_lv_prefix: String::new(),
            zfs_pool_name: String::new(),
            zfs_dataset_prefix: String::new(),
            disable_vxlan: false,
            sub_ca_cert_pem: String::new(),
            sub_ca_key_pem: String::new(),
            controllers: Vec::new(),
            dc_id: String::new(),
            hostname: String::new(),
            node_id: String::new(),
        };
        let args = build_install_command_args(&req).expect("args");
        assert!(!args.contains(&"--data-disk".to_string()));
        assert!(args.contains(&"--data-disk-mode".to_string()));
        assert!(args.contains(&"filesystem".to_string()));
        assert!(args.contains(&"--disk".to_string()));
        assert!(args.contains(&"/dev/sda".to_string()));
    }

    #[test]
    fn build_install_args_normalizes_port() {
        let req = proto::InstallToDiskRequest {
            os_disk: "/dev/sda".to_string(),
            data_disks: Vec::new(),
            storage_backend: 0,
            controllers: vec!["192.168.1.10".to_string(), "10.0.0.5:7777".to_string()],
            controller: String::new(),
            run_controller: false,
            hostname: "test".to_string(),
            node_id: "test".to_string(),
            ca_cert_pem: String::new(),
            node_cert_pem: String::new(),
            node_key_pem: String::new(),
            disable_vxlan: false,
            dc_id: String::new(),
            data_disk_mode: String::new(),
            controller_cert_pem: String::new(),
            controller_key_pem: String::new(),
            kctl_cert_pem: String::new(),
            kctl_key_pem: String::new(),
            lvm_vg_name: String::new(),
            lvm_lv_prefix: String::new(),
            zfs_pool_name: String::new(),
            zfs_dataset_prefix: String::new(),
            sub_ca_cert_pem: String::new(),
            sub_ca_key_pem: String::new(),
        };
        let args = build_install_command_args(&req).expect("args");
        assert!(args.contains(&"192.168.1.10:9090".to_string()));
        assert!(args.contains(&"10.0.0.5:7777".to_string()));
        assert!(!args.contains(&"192.168.1.10".to_string()));
    }

    #[test]
    fn parse_lease_entry_parses_dnsmasq_format() {
        let line = "1711454677 52:54:00:4b:13:d6 10.240.0.113 ubuntu-noble-1 01:52:54:00:4b:13:d6";
        let parsed = parse_lease_entry(line).expect("parse lease");
        assert_eq!(parsed.0, "52:54:00:4b:13:d6");
        assert_eq!(parsed.1, "10.240.0.113");
        assert_eq!(parsed.2, "ubuntu-noble-1");
    }

    #[test]
    fn find_vm_ip_in_lease_file_matches_mac_or_hostname() {
        let temp = tempfile::tempdir().expect("tempdir");
        let lease = temp.path().join("dnsmasq-default.leases");
        std::fs::write(
            &lease,
            "1711454677 52:54:00:aa:bb:cc 10.240.0.50 old-vm *\n\
             1711454678 52:54:00:4b:13:d6 10.240.0.113 ubuntu-noble-1 *\n",
        )
        .expect("write lease");

        let ip_by_host = find_vm_ip_in_lease_file(&lease, "ubuntu-noble-1", None);
        assert_eq!(ip_by_host.as_deref(), Some("10.240.0.113"));

        let ip_by_mac =
            find_vm_ip_in_lease_file(&lease, "different-name", Some("52:54:00:4b:13:d6"));
        assert_eq!(ip_by_mac.as_deref(), Some("10.240.0.113"));
    }

    #[test]
    fn validate_port_and_timeout_defaults_are_applied() {
        assert_eq!(validate_port_or_default(0), 22);
        assert_eq!(validate_port_or_default(-3), 22);
        assert_eq!(validate_port_or_default(2222), 2222);
        assert_eq!(validate_timeout_ms_or_default(0), 1500);
        assert_eq!(validate_timeout_ms_or_default(-1), 1500);
        assert_eq!(validate_timeout_ms_or_default(3000), 3000);
    }

    #[test]
    fn disko_management_mode_defaults_to_installer_only() {
        assert_eq!(
            normalize_disko_management_mode(""),
            DISKO_MODE_INSTALLER_ONLY
        );
        assert_eq!(
            normalize_disko_management_mode("unknown"),
            DISKO_MODE_INSTALLER_ONLY
        );
    }

    #[test]
    fn disko_timeout_defaults_and_caps() {
        assert_eq!(validate_disko_timeout_seconds_or_default(0), 300);
        assert_eq!(validate_disko_timeout_seconds_or_default(-1), 300);
        assert_eq!(validate_disko_timeout_seconds_or_default(120), 120);
        assert_eq!(validate_disko_timeout_seconds_or_default(7200), 3600);
    }

    #[test]
    fn parse_neigh_line_extracts_ip_and_mac() {
        let line = "10.240.0.113 dev kbr-default lladdr 52:54:00:4b:13:d6 REACHABLE";
        let parsed = parse_neigh_line(line).expect("parse neigh");
        assert_eq!(parsed.0, "10.240.0.113");
        assert_eq!(parsed.1, "52:54:00:4b:13:d6");
    }

    #[test]
    fn vm_unit_is_fatal_detects_failed_and_flapping() {
        let failed = VmUnitState {
            active_state: "failed".to_string(),
            sub_state: "failed".to_string(),
            result: "exit-code".to_string(),
            n_restarts: 1,
        };
        assert!(vm_unit_is_fatal(&failed));

        let flapping = VmUnitState {
            active_state: "activating".to_string(),
            sub_state: "auto-restart".to_string(),
            result: "exit-code".to_string(),
            n_restarts: 5,
        };
        assert!(vm_unit_is_fatal(&flapping));

        let transient = VmUnitState {
            active_state: "activating".to_string(),
            sub_state: "start".to_string(),
            result: "success".to_string(),
            n_restarts: 0,
        };
        assert!(!vm_unit_is_fatal(&transient));
    }

    #[test]
    fn normalize_endpoint_ipv4_no_port() {
        assert_eq!(normalize_endpoint("10.0.0.1", 9090), "10.0.0.1:9090");
    }

    #[test]
    fn normalize_endpoint_ipv4_with_port() {
        assert_eq!(normalize_endpoint("10.0.0.1:8080", 9090), "10.0.0.1:8080");
    }

    #[test]
    fn normalize_endpoint_ipv6_bracketed_no_port() {
        assert_eq!(
            normalize_endpoint("[2001:db8::10]", 9090),
            "[2001:db8::10]:9090"
        );
    }

    #[test]
    fn normalize_endpoint_ipv6_bracketed_with_port() {
        assert_eq!(
            normalize_endpoint("[2001:db8::10]:7070", 9090),
            "[2001:db8::10]:7070"
        );
    }

    #[test]
    fn normalize_endpoint_bare_ipv6() {
        assert_eq!(
            normalize_endpoint("2001:db8::10", 9090),
            "[2001:db8::10]:9090"
        );
    }
}

/// Property-based tests (Phase 2).
#[cfg(test)]
mod proptests {
    use super::{
        normalize_endpoint, parse_lease_entry, parse_neigh_line, parse_stopped_vms_from_nix,
        vm_unit_is_fatal, VmUnitState,
    };
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// Parsers must never panic on arbitrary input. (Both consume
        /// untrusted-ish text from `dnsmasq` lease files and `ip neigh`
        /// output respectively.)
        #[test]
        fn lease_and_neigh_parsers_never_panic(line in ".{0,128}") {
            let _ = parse_lease_entry(&line);
            let _ = parse_neigh_line(&line);
        }

        /// `parse_lease_entry` returns `Some` iff the line has at least
        /// 4 whitespace-delimited tokens.
        #[test]
        fn lease_entry_some_iff_four_fields(line in ".{0,128}") {
            let n = line.split_whitespace().count();
            prop_assert_eq!(parse_lease_entry(&line).is_some(), n >= 4);
        }

        /// On a well-formed lease line, the (mac, ip, hostname) tokens
        /// match field positions 1, 2, 3 (0-indexed).
        #[test]
        fn lease_entry_returns_correct_positions(
            ts in 100u32..=999,
            mac in "[0-9a-f:]{17}",
            ip in "[0-9.]{7,15}",
            hostname in "[a-z0-9-]{1,16}",
            client_id in "[a-z0-9*]{1,8}",
        ) {
            let line = format!("{ts} {mac} {ip} {hostname} {client_id}");
            let parsed = parse_lease_entry(&line).expect("must parse");
            prop_assert_eq!(parsed.0, mac.as_str());
            prop_assert_eq!(parsed.1, ip.as_str());
            prop_assert_eq!(parsed.2, hostname.as_str());
        }

        /// `parse_neigh_line` returns `None` for any line shorter than
        /// 5 fields, and any line missing the `lladdr` token.
        #[test]
        fn neigh_line_returns_none_when_lladdr_missing(line in "[a-z0-9. ]{0,64}") {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 5 || !fields.contains(&"lladdr") {
                prop_assert!(parse_neigh_line(&line).is_none());
            }
        }

        /// `parse_neigh_line` returns the IP and the token immediately
        /// after `lladdr` for well-formed lines.
        #[test]
        fn neigh_line_returns_ip_and_mac(
            ip in "[0-9.]{7,15}",
            mac in "[0-9a-f:]{17}",
        ) {
            let line = format!("{ip} dev br0 lladdr {mac} REACHABLE");
            let parsed = parse_neigh_line(&line).expect("well-formed");
            prop_assert_eq!(parsed.0, ip.as_str());
            prop_assert_eq!(parsed.1, mac.as_str());
        }

        /// `vm_unit_is_fatal` is **monotone** in `n_restarts` for the
        /// flapping branch: if a state is fatal at N restarts, it is
        /// fatal at N+k restarts (everything else equal).
        #[test]
        fn vm_unit_is_fatal_is_monotone_in_restarts(
            active in prop::sample::select(vec!["active", "activating", "failed", "inactive"]),
            sub in prop::sample::select(vec!["running", "auto-restart", "dead"]),
            result in prop::sample::select(vec!["exit-code", "success", "signal"]),
            n in 0u32..=10,
            extra in 1u32..=10,
        ) {
            let lo = VmUnitState {
                active_state: active.into(),
                sub_state: sub.into(),
                result: result.into(),
                n_restarts: n,
            };
            let hi = VmUnitState {
                active_state: lo.active_state.clone(),
                sub_state: lo.sub_state.clone(),
                result: lo.result.clone(),
                n_restarts: n.saturating_add(extra),
            };
            if vm_unit_is_fatal(&lo) {
                prop_assert!(vm_unit_is_fatal(&hi));
            }
        }

        /// `failed` active state is **always** fatal regardless of
        /// other fields.
        #[test]
        fn vm_unit_is_fatal_failed_is_always_fatal(
            sub in "[a-z-]{0,16}",
            result in "[a-z-]{0,16}",
            n in any::<u32>(),
        ) {
            let s = VmUnitState {
                active_state: "failed".into(),
                sub_state: sub,
                result,
                n_restarts: n,
            };
            prop_assert!(vm_unit_is_fatal(&s));
        }

        /// `normalize_endpoint` never panics on arbitrary input.
        #[test]
        fn normalize_endpoint_never_panics(s in ".{0,64}") {
            let _ = normalize_endpoint(&s, 9091);
        }

        /// `normalize_endpoint` is idempotent on inputs that already
        /// parse as `SocketAddr` (it should hand them straight back).
        #[test]
        fn normalize_endpoint_idempotent_on_socket_addr(
            a in 0u8..=255, b in 0u8..=255, c in 0u8..=255, d in 0u8..=255,
            port in 1u16..=65_535,
        ) {
            let addr = format!("{a}.{b}.{c}.{d}:{port}");
            prop_assert_eq!(normalize_endpoint(&addr, 9091), addr);
        }

        /// `parse_stopped_vms_from_nix` always returns a sorted,
        /// de-duplicated list.
        #[test]
        fn parse_stopped_vms_from_nix_is_sorted_and_deduped(s in ".{0,256}") {
            let out = parse_stopped_vms_from_nix(&s);
            let mut sorted = out.clone();
            sorted.sort();
            sorted.dedup();
            prop_assert_eq!(out, sorted);
        }
    }
}
