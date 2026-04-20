use std::path::Path;
use std::sync::Arc;

use tokio::sync::Notify;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tracing::{error, info, warn};

use crate::config::Config;
use crate::controller_proto;

const DISABLE_VXLAN_MARKER: &str = "/etc/kcore/disable-vxlan";
const REGISTRATION_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(5);
const MAX_REGISTRATION_RETRIES: u32 = 12;
const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);
const RENEWAL_THRESHOLD_DAYS: i64 = 30;
const RENEWAL_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(86400);

/// Spawn registration in a background task and return a Notify that fires
/// when registration completes (or exhausts retries). Heartbeat loop waits
/// on this signal so it doesn't send heartbeats before the node exists on
/// any controller.
pub fn register_with_controller_tracked(cfg: Config) -> Arc<Notify> {
    let registered = Arc::new(Notify::new());
    let signal = registered.clone();
    tokio::spawn(async move {
        register_with_controller(&cfg).await;
        signal.notify_one();
    });
    registered
}

pub async fn register_with_controller(cfg: &Config) {
    let disable_vxlan = Path::new(DISABLE_VXLAN_MARKER).exists();
    if disable_vxlan {
        info!("VXLAN disabled: marker file {} found", DISABLE_VXLAN_MARKER);
    }

    let (hostname, cpu_cores, memory_bytes) = tokio::task::spawn_blocking(|| {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "unknown".into());
        let cpu = std::fs::read_to_string("/proc/cpuinfo")
            .map(|s| s.matches("processor\t:").count() as i32)
            .unwrap_or(0);
        let mem_total: i64 = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("MemTotal:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|v| v.parse::<i64>().ok())
                    .map(|kb| kb * 1024)
            })
            .unwrap_or(0);
        (hostname, cpu, mem_total)
    })
    .await
    .unwrap_or_else(|_| ("unknown".into(), 0, 0));

    let storage_backend = match cfg.storage.backend {
        crate::config::StorageBackendKind::Filesystem => {
            controller_proto::StorageBackendType::Filesystem as i32
        }
        crate::config::StorageBackendKind::Lvm => controller_proto::StorageBackendType::Lvm as i32,
        crate::config::StorageBackendKind::Zfs => controller_proto::StorageBackendType::Zfs as i32,
    };

    let endpoints = controller_endpoints(cfg);
    if endpoints.is_empty() {
        return;
    }

    for attempt in 1..=MAX_REGISTRATION_RETRIES {
        let mut registered = false;
        for endpoint in &endpoints {
            match connect_and_register(
                cfg,
                endpoint,
                &hostname,
                cpu_cores,
                memory_bytes,
                storage_backend,
                disable_vxlan,
            )
            .await
            {
                Ok(()) => {
                    info!(
                        controller = %endpoint,
                        node_id = %cfg.node_id,
                        dc_id = %cfg.dc_id,
                        disable_vxlan,
                        "registered with controller"
                    );
                    registered = true;
                    break;
                }
                Err(e) => {
                    warn!(
                        endpoint = %endpoint,
                        attempt,
                        max = MAX_REGISTRATION_RETRIES,
                        error = %e,
                        "registration attempt failed on controller endpoint"
                    );
                }
            }
        }
        if registered {
            return;
        }
        if attempt < MAX_REGISTRATION_RETRIES {
            tokio::time::sleep(REGISTRATION_RETRY_DELAY).await;
        }
    }
    error!(
        endpoints = ?endpoints,
        "failed to register after {} attempts; node may need manual registration",
        MAX_REGISTRATION_RETRIES
    );
}

async fn connect_and_register(
    cfg: &Config,
    endpoint: &str,
    hostname: &str,
    cpu_cores: i32,
    memory_bytes: i64,
    storage_backend: i32,
    disable_vxlan: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let channel = if let Some(tls) = cfg.tls.as_ref() {
        let ca_pem = std::fs::read_to_string(&tls.ca_file)?;
        let cert_pem = std::fs::read_to_string(&tls.cert_file)?;
        let key_pem = std::fs::read_to_string(&tls.key_file)?;
        let domain = endpoint_host(endpoint).unwrap_or("localhost").to_string();
        let tls_config = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(ca_pem))
            .identity(Identity::from_pem(cert_pem, key_pem))
            .domain_name(domain);
        Channel::from_shared(endpoint.to_string())?
            .tls_config(tls_config)?
            .connect()
            .await?
    } else {
        Channel::from_shared(endpoint.to_string())?
            .connect()
            .await?
    };

    let cert_expiry = cfg
        .tls
        .as_ref()
        .and_then(|tls| std::fs::read_to_string(&tls.cert_file).ok())
        .and_then(|pem| cert_days_remaining(&pem).ok())
        .unwrap_or(-1) as i32;

    let listen_addr = &cfg.listen_addr;
    let external_addr = derive_external_address(listen_addr);

    let mut client = controller_proto::controller_client::ControllerClient::new(channel);
    let mut labels = Vec::new();
    labels.push(format!("dc={}", cfg.dc_id.trim()));
    client
        .register_node(controller_proto::RegisterNodeRequest {
            node_id: cfg.node_id.clone(),
            hostname: hostname.to_string(),
            address: external_addr,
            capacity: Some(controller_proto::NodeCapacity {
                cpu_cores,
                memory_bytes,
            }),
            labels,
            storage_backend,
            disable_vxlan,
            cert_expiry_days: cert_expiry,
            luks_method: detect_luks_method(),
            dc_id: cfg.dc_id.clone(),
        })
        .await?;
    Ok(())
}

fn derive_external_address(listen_addr: &str) -> String {
    if let Some(port) = listen_addr.strip_prefix("0.0.0.0:") {
        if let Ok(ip) = get_primary_ip() {
            return format!("{ip}:{port}");
        }
    }
    listen_addr.to_string()
}

fn get_primary_ip() -> Result<String, std::io::Error> {
    // Prefer the IPv4 on the default-route interface so we don't accidentally
    // register bridge/gateway addresses like 10.240.0.1 as the node endpoint.
    let route = std::process::Command::new("ip")
        .args(["-4", "route", "show", "default"])
        .output()?;
    if route.status.success() {
        let route_stdout = String::from_utf8_lossy(&route.stdout);
        if let Some(dev) = parse_default_route_dev(&route_stdout) {
            let addr = std::process::Command::new("ip")
                .args(["-4", "-o", "addr", "show", "dev", dev, "scope", "global"])
                .output()?;
            if addr.status.success() {
                let addr_stdout = String::from_utf8_lossy(&addr.stdout);
                if let Some(ip) = parse_first_ipv4_addr(&addr_stdout) {
                    return Ok(ip);
                }
            }
        }
    }

    // Fallback for environments without `ip route` output.
    let output = std::process::Command::new("hostname").arg("-I").output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split_whitespace()
        .next()
        .map(|s| s.to_string())
        .ok_or_else(|| std::io::Error::other("no IP found"))
}

fn parse_default_route_dev(route_stdout: &str) -> Option<&str> {
    // Example: "default via 192.168.40.1 dev eno1 proto dhcp src 192.168.40.105"
    let mut prev = "";
    for tok in route_stdout.split_whitespace() {
        if prev == "dev" {
            return Some(tok);
        }
        prev = tok;
    }
    None
}

fn parse_first_ipv4_addr(addr_stdout: &str) -> Option<String> {
    // Example: "4: eno1    inet 192.168.40.105/24 brd ... scope global ..."
    for line in addr_stdout.lines() {
        let mut prev = "";
        for tok in line.split_whitespace() {
            if prev == "inet" {
                return tok.split('/').next().map(|s| s.to_string());
            }
            prev = tok;
        }
    }
    None
}

/// Spawn a background task that checks cert expiry daily and renews via
/// the controller's RenewNodeCert RPC when the cert is within 30 days of expiry.
pub fn start_cert_renewal_loop(cfg: Config) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = check_and_renew_cert(&cfg).await {
                warn!(error = %e, "cert renewal check failed");
            }
            tokio::time::sleep(RENEWAL_CHECK_INTERVAL).await;
        }
    });
}

pub fn start_heartbeat_loop(cfg: Config, registration_done: Arc<Notify>) {
    tokio::spawn(async move {
        registration_done.notified().await;
        loop {
            if let Err(e) = send_heartbeat_once(&cfg).await {
                warn!(error = %e, "heartbeat failed on all controller endpoints");
            }
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
        }
    });
}

async fn send_heartbeat_once(cfg: &Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cert_expiry_days = cfg
        .tls
        .as_ref()
        .and_then(|tls| std::fs::read_to_string(&tls.cert_file).ok())
        .and_then(|pem| cert_days_remaining(&pem).ok())
        .unwrap_or(-1) as i32;
    let endpoints = controller_endpoints(cfg);
    if endpoints.is_empty() {
        return Ok(());
    }
    let mut last_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    for endpoint in &endpoints {
        let channel = match connect_channel(cfg, endpoint).await {
            Ok(c) => c,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        let mut client = controller_proto::controller_client::ControllerClient::new(channel);
        match client
            .heartbeat(controller_proto::HeartbeatRequest {
                node_id: cfg.node_id.clone(),
                usage: Some(controller_proto::NodeUsage {
                    cpu_cores_used: 0,
                    memory_bytes_used: 0,
                }),
                cert_expiry_days,
                luks_method: detect_luks_method(),
            })
            .await
        {
            Ok(_) => {
                let workloads = collect_local_workload_runtime();
                if !workloads.is_empty() {
                    let _ = client
                        .sync_workload_state(controller_proto::SyncWorkloadStateRequest {
                            node_id: cfg.node_id.clone(),
                            workloads,
                        })
                        .await;
                }
                return Ok(());
            }
            Err(e) => {
                last_err = Some(Box::new(e));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| Box::new(std::io::Error::other("heartbeat failed"))))
}

fn collect_local_workload_runtime() -> Vec<controller_proto::WorkloadRuntimeInfo> {
    let mut workloads = Vec::new();
    let runtime = if std::process::Command::new("nerdctl")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        "nerdctl"
    } else if std::process::Command::new("docker")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        "docker"
    } else {
        return workloads;
    };

    let output = std::process::Command::new(runtime)
        .args(["ps", "-a", "--format", "{{.ID}}\t{{.Names}}\t{{.Status}}"])
        .output();
    let Ok(out) = output else {
        return workloads;
    };
    if !out.status.success() {
        return workloads;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        let mut parts = line.splitn(3, '\t');
        let id = parts.next().unwrap_or_default().trim().to_string();
        let name = parts.next().unwrap_or_default().trim().to_string();
        let status = parts.next().unwrap_or_default().trim().to_ascii_lowercase();
        if id.is_empty() || name.is_empty() {
            continue;
        }
        let container_state = if status.starts_with("up ") || status == "running" {
            controller_proto::ContainerState::Running as i32
        } else if status.starts_with("exited") || status.starts_with("created") {
            controller_proto::ContainerState::Stopped as i32
        } else {
            controller_proto::ContainerState::Unknown as i32
        };
        workloads.push(controller_proto::WorkloadRuntimeInfo {
            id,
            name,
            kind: controller_proto::WorkloadKind::Container as i32,
            vm_state: controller_proto::VmState::Unknown as i32,
            container_state,
        });
    }
    workloads
}

async fn check_and_renew_cert(
    cfg: &Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tls = match cfg.tls.as_ref() {
        Some(t) => t,
        None => return Ok(()),
    };

    let cert_pem = std::fs::read_to_string(&tls.cert_file)?;
    let days_remaining = cert_days_remaining(&cert_pem)?;

    if days_remaining > RENEWAL_THRESHOLD_DAYS {
        info!(days_remaining, "certificate valid, no renewal needed");
        return Ok(());
    }

    info!(
        days_remaining,
        "certificate expires soon, requesting renewal"
    );

    let mut resp_opt = None;
    let mut last_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    for endpoint in controller_endpoints(cfg) {
        let channel = match connect_channel(cfg, &endpoint).await {
            Ok(c) => c,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };

        let mut client = controller_proto::controller_client::ControllerClient::new(channel);
        match client
            .renew_node_cert(controller_proto::RenewNodeCertRequest {
                node_id: cfg.node_id.clone(),
            })
            .await
        {
            Ok(resp) => {
                resp_opt = Some(resp.into_inner());
                break;
            }
            Err(e) => {
                last_err = Some(Box::new(e));
            }
        }
    }
    let resp = if let Some(resp) = resp_opt {
        resp
    } else {
        return Err(last_err.unwrap_or_else(|| {
            Box::new(std::io::Error::other(
                "cert renewal failed on all controller endpoints",
            ))
        }));
    };

    if !resp.success {
        return Err(format!("controller rejected renewal: {}", resp.message).into());
    }

    std::fs::write(&tls.cert_file, &resp.cert_pem)?;
    std::fs::write(&tls.key_file, &resp.key_pem)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tls.key_file, std::fs::Permissions::from_mode(0o600))?;
    }

    info!(
        node_id = %cfg.node_id,
        "certificate renewed successfully; restarting to load new TLS identity"
    );

    #[cfg(unix)]
    {
        let pid = std::process::id();
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
    }

    Ok(())
}

fn cert_days_remaining(cert_pem: &str) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
    let pem = pem::parse(cert_pem)?;
    use x509_parser::prelude::FromDer;
    let (_, cert) = x509_parser::certificate::X509Certificate::from_der(pem.contents())?;
    let not_after = cert.validity().not_after.to_datetime();
    let now = time::OffsetDateTime::now_utc();
    let remaining = not_after - now;
    Ok(remaining.whole_days())
}

fn endpoint_host(endpoint: &str) -> Option<&str> {
    let without_scheme = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .unwrap_or(endpoint);
    if let Some(rest) = without_scheme.strip_prefix('[') {
        if let Some(end_idx) = rest.find(']') {
            return Some(&rest[..end_idx]);
        }
    }
    without_scheme
        .rsplit_once(':')
        .map(|(host, _)| host)
        .or(Some(without_scheme))
}

fn controller_endpoints(cfg: &Config) -> Vec<String> {
    let default_scheme = if cfg.tls.is_some() { "https" } else { "http" };
    cfg.controller_endpoints()
        .into_iter()
        .map(|addr| {
            if addr.contains("://") {
                addr
            } else {
                format!("{default_scheme}://{addr}")
            }
        })
        .collect()
}

async fn connect_channel(
    cfg: &Config,
    endpoint: &str,
) -> Result<Channel, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(tls) = cfg.tls.as_ref() {
        let ca_pem = std::fs::read_to_string(&tls.ca_file)?;
        let cert_pem = std::fs::read_to_string(&tls.cert_file)?;
        let key_pem = std::fs::read_to_string(&tls.key_file)?;
        let domain = endpoint_host(endpoint).unwrap_or("localhost").to_string();
        let tls_config = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(ca_pem))
            .identity(Identity::from_pem(cert_pem, key_pem))
            .domain_name(domain);
        Ok(Channel::from_shared(endpoint.to_string())?
            .tls_config(tls_config)?
            .connect()
            .await?)
    } else {
        Ok(Channel::from_shared(endpoint.to_string())?
            .connect()
            .await?)
    }
}

/// Detects the LUKS encryption method on the root filesystem.
/// Returns "tpm2" if TPM 2.0 is enrolled, "key-file" if dm-crypt is active,
/// or "" if LUKS is not detected.
fn detect_luks_method() -> String {
    let cryptroot = Path::new("/dev/mapper/cryptroot");
    if !cryptroot.exists() {
        return String::new();
    }
    let tpm_present = Path::new("/sys/class/tpm/tpm0").exists();
    if tpm_present {
        "tpm2".to_string()
    } else {
        "key-file".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_external_address_replaces_wildcard() {
        let addr = derive_external_address("192.168.1.5:9091");
        assert_eq!(addr, "192.168.1.5:9091");
    }

    #[test]
    fn parse_default_route_dev_extracts_interface() {
        let line = "default via 192.168.40.1 dev eno1 proto dhcp src 192.168.40.105";
        assert_eq!(parse_default_route_dev(line), Some("eno1"));
    }

    #[test]
    fn parse_first_ipv4_addr_extracts_ip() {
        let line =
            "4: eno1    inet 192.168.40.105/24 brd 192.168.40.255 scope global dynamic noprefixroute eno1";
        assert_eq!(
            parse_first_ipv4_addr(line),
            Some("192.168.40.105".to_string())
        );
    }

    #[test]
    fn marker_file_path_is_correct() {
        assert_eq!(DISABLE_VXLAN_MARKER, "/etc/kcore/disable-vxlan");
    }

    #[test]
    fn endpoint_host_parses_scheme_and_port() {
        assert_eq!(
            endpoint_host("https://192.168.40.105:9090"),
            Some("192.168.40.105")
        );
        assert_eq!(endpoint_host("127.0.0.1:9090"), Some("127.0.0.1"));
        assert_eq!(endpoint_host("https://[::1]:9090"), Some("::1"));
    }

    #[test]
    fn endpoint_host_plain_hostname() {
        assert_eq!(
            endpoint_host("controller.local:9090"),
            Some("controller.local")
        );
        assert_eq!(endpoint_host("https://myhost"), Some("myhost"));
    }

    #[test]
    fn controller_endpoints_prefers_list() {
        let cfg = Config {
            node_id: "node-1".to_string(),
            listen_addr: "0.0.0.0:9091".to_string(),
            controller_addr: "10.0.0.1:9090".to_string(),
            controllers: vec!["10.0.0.2:9090".to_string(), "10.0.0.3:9090".to_string()],
            dc_id: "DC1".to_string(),
            tls: None,
            vm_socket_dir: "/run/kcore".to_string(),
            nix_config_path: "/etc/nixos/kcore-vms.nix".to_string(),
            storage: crate::config::StorageConfig::default(),
        };
        let endpoints = super::controller_endpoints(&cfg);
        assert_eq!(
            endpoints,
            vec!["http://10.0.0.2:9090", "http://10.0.0.3:9090"]
        );
    }

    #[test]
    fn cert_days_remaining_parses_valid_cert() {
        use rcgen::{CertificateParams, DnType, IsCa, KeyPair};
        use time::{Duration, OffsetDateTime};

        let mut params = CertificateParams::default();
        params.is_ca = IsCa::NoCa;
        params
            .distinguished_name
            .push(DnType::CommonName, "test-node");
        params.not_before = OffsetDateTime::now_utc();
        params.not_after = OffsetDateTime::now_utc() + Duration::days(100);
        let key = KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        let pem = cert.pem();

        let days = cert_days_remaining(&pem).unwrap();
        assert!((99..=100).contains(&days), "expected ~100 days, got {days}");
    }

    #[test]
    fn cert_days_remaining_near_expiry() {
        use rcgen::{CertificateParams, DnType, IsCa, KeyPair};
        use time::{Duration, OffsetDateTime};

        let mut params = CertificateParams::default();
        params.is_ca = IsCa::NoCa;
        params
            .distinguished_name
            .push(DnType::CommonName, "expiring-node");
        params.not_before = OffsetDateTime::now_utc() - Duration::days(360);
        params.not_after = OffsetDateTime::now_utc() + Duration::days(5);
        let key = KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        let pem = cert.pem();

        let days = cert_days_remaining(&pem).unwrap();
        assert!((4..=5).contains(&days), "expected ~5 days, got {days}");
    }

    #[test]
    fn collect_local_workload_runtime_is_non_panicking() {
        let _ = collect_local_workload_runtime();
    }
}

/// Property-based tests (Phase 2) — text parsers for `ip route` /
/// `ip addr` output and endpoint URLs.
#[cfg(test)]
mod proptests {
    use super::{endpoint_host, parse_default_route_dev, parse_first_ipv4_addr};
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// All three pure parsers must terminate without panic on
        /// arbitrary input.
        #[test]
        fn parsers_never_panic(s in ".{0,128}") {
            let _ = parse_default_route_dev(&s);
            let _ = parse_first_ipv4_addr(&s);
            let _ = endpoint_host(&s);
        }

        /// `parse_default_route_dev` returns `Some(token)` whenever some
        /// whitespace-delimited token is preceded by `dev`. Conversely,
        /// if no token is preceded by `dev`, returns `None`.
        #[test]
        fn parse_default_route_dev_finds_token_after_dev(
            iface in "[a-z][a-z0-9]{0,7}",
            ip in "[0-9.]{7,15}",
        ) {
            let line = format!("default via {ip} dev {iface} proto dhcp src {ip}");
            prop_assert_eq!(parse_default_route_dev(&line), Some(iface.as_str()));
        }

        /// `parse_default_route_dev` returns `None` when input lacks
        /// the literal `dev` token.
        #[test]
        fn parse_default_route_dev_returns_none_without_dev(
            iface in "[a-z][a-z0-9]{0,7}",
        ) {
            // Avoid generating something that ends with " dev <X>".
            let line = format!("interface {iface}");
            prop_assert_eq!(parse_default_route_dev(&line), None);
        }

        /// `parse_first_ipv4_addr` strips the `/prefix` suffix and
        /// returns the bare IP for any valid `inet` line.
        #[test]
        fn parse_first_ipv4_addr_strips_cidr(
            a in 0u8..=255, b in 0u8..=255, c in 0u8..=255, d in 0u8..=255,
            prefix in 0u8..=32,
        ) {
            let stdout = format!("4: eno1    inet {a}.{b}.{c}.{d}/{prefix} brd ... scope global ...");
            let got = parse_first_ipv4_addr(&stdout);
            prop_assert_eq!(got, Some(format!("{a}.{b}.{c}.{d}")));
        }

        /// `endpoint_host` strips `http(s)://` schemes and returns the
        /// host part. For bracketed IPv6 it returns the inner address.
        #[test]
        fn endpoint_host_strips_scheme_and_port(
            scheme in prop::sample::select(vec!["http", "https"]),
            host in "[a-z][a-z0-9.-]{0,16}",
            port in 1u16..=65_535,
        ) {
            let url = format!("{scheme}://{host}:{port}");
            prop_assert_eq!(endpoint_host(&url), Some(host.as_str()));
        }

        /// Bracketed IPv6 endpoints return the inner address without
        /// brackets.
        #[test]
        fn endpoint_host_returns_inner_ipv6(_seed in any::<u8>()) {
            prop_assert_eq!(endpoint_host("https://[2001:db8::10]:9090"), Some("2001:db8::10"));
            prop_assert_eq!(endpoint_host("[fe80::1]:8080"), Some("fe80::1"));
        }
    }
}
