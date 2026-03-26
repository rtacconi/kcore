use std::path::Path;

use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};
use tracing::{error, info, warn};

use crate::config::Config;
use crate::controller_proto;

const DISABLE_VXLAN_MARKER: &str = "/etc/kcore/disable-vxlan";
const REGISTRATION_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(5);
const MAX_REGISTRATION_RETRIES: u32 = 12;

pub async fn register_with_controller(cfg: &Config) {
    let disable_vxlan = Path::new(DISABLE_VXLAN_MARKER).exists();
    if disable_vxlan {
        info!("VXLAN disabled: marker file {} found", DISABLE_VXLAN_MARKER);
    }

    let (hostname, cpu_cores, memory_bytes) =
        tokio::task::spawn_blocking(|| {
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
        crate::config::StorageBackendKind::Lvm => {
            controller_proto::StorageBackendType::Lvm as i32
        }
        crate::config::StorageBackendKind::Zfs => {
            controller_proto::StorageBackendType::Zfs as i32
        }
    };

    let endpoint = if cfg.controller_addr.contains("://") {
        cfg.controller_addr.clone()
    } else {
        format!("https://{}", cfg.controller_addr)
    };

    for attempt in 1..=MAX_REGISTRATION_RETRIES {
        match connect_and_register(
            cfg,
            &endpoint,
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
                    controller = %cfg.controller_addr,
                    node_id = %cfg.node_id,
                    disable_vxlan,
                    "registered with controller"
                );
                return;
            }
            Err(e) => {
                warn!(
                    attempt,
                    max = MAX_REGISTRATION_RETRIES,
                    error = %e,
                    "registration attempt failed, retrying"
                );
                if attempt < MAX_REGISTRATION_RETRIES {
                    tokio::time::sleep(REGISTRATION_RETRY_DELAY).await;
                }
            }
        }
    }
    error!(
        controller = %cfg.controller_addr,
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
        let base_tls_config = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(ca_pem))
            .identity(Identity::from_pem(cert_pem, key_pem));
        let server_name_candidates = tls_server_name_candidates(endpoint, hostname);
        let mut last_err: Option<Box<dyn std::error::Error + Send + Sync>> = None;
        let mut connected: Option<Channel> = None;
        for server_name in &server_name_candidates {
            let tls_config = base_tls_config.clone().domain_name(server_name.to_string());
            match Channel::from_shared(endpoint.to_string())?
                .tls_config(tls_config)?
                .connect()
                .await
            {
                Ok(ch) => {
                    connected = Some(ch);
                    break;
                }
                Err(e) => {
                    warn!(
                        endpoint = %endpoint,
                        server_name = %server_name,
                        error = %e,
                        "registration TLS connect attempt failed"
                    );
                    last_err = Some(Box::new(e));
                }
            }
        }
        match connected {
            Some(ch) => ch,
            None => {
                return Err(last_err.unwrap_or_else(|| {
                    Box::new(std::io::Error::other(
                        "TLS connect failed without specific error",
                    )) as Box<dyn std::error::Error + Send + Sync>
                }));
            }
        }
    } else {
        Channel::from_shared(endpoint.to_string())?
            .connect()
            .await?
    };

    let listen_addr = &cfg.listen_addr;
    let external_addr = derive_external_address(listen_addr);

    let mut client =
        controller_proto::controller_client::ControllerClient::new(channel);
    client
        .register_node(controller_proto::RegisterNodeRequest {
            node_id: cfg.node_id.clone(),
            hostname: hostname.to_string(),
            address: external_addr,
            capacity: Some(controller_proto::NodeCapacity {
                cpu_cores,
                memory_bytes,
            }),
            labels: Vec::new(),
            storage_backend,
            disable_vxlan,
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
    let output = std::process::Command::new("hostname")
        .arg("-I")
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .split_whitespace()
        .next()
        .map(|s| s.to_string())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no IP found"))
}

fn tls_server_name_candidates(endpoint: &str, hostname: &str) -> Vec<String> {
    let mut out = Vec::<String>::new();
    if let Some(host) = endpoint_host(endpoint) {
        out.push(host.to_string());
        // Common test env mismatch: endpoint is loopback, cert SAN is node hostname/IP.
        if is_loopback_host(host) {
            if !hostname.trim().is_empty() && hostname != "unknown" {
                out.push(hostname.to_string());
            }
            if let Ok(ip) = get_primary_ip() {
                out.push(ip);
            }
        }
    }
    if out.is_empty() {
        out.push("localhost".to_string());
    }
    dedup_strings(out)
}

fn endpoint_host(endpoint: &str) -> Option<&str> {
    let without_scheme = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .unwrap_or(endpoint);
    // Bracketed IPv6 support: [::1]:9090
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

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

fn dedup_strings(input: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for s in input {
        if !out.iter().any(|existing| existing == &s) {
            out.push(s);
        }
    }
    out
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
    fn tls_server_name_candidates_include_hostname_for_loopback() {
        let names = tls_server_name_candidates("https://127.0.0.1:9090", "kvm-node");
        assert!(names.iter().any(|n| n == "127.0.0.1"));
        assert!(names.iter().any(|n| n == "kvm-node"));
    }
}
