use std::error::Error;

use anyhow::{Context, Result};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};
use x509_parser::pem;

use crate::config::ConnectionInfo;

fn format_transport_error(e: &dyn Error) -> String {
    let mut s = e.to_string();
    let mut cur = e.source();
    while let Some(next) = cur {
        s.push_str(": ");
        s.push_str(&next.to_string());
        cur = next.source();
    }
    s
}

fn tls_domain_name_for_endpoint(address: &str, info: &ConnectionInfo) -> Option<String> {
    if let Some(name) = info.tls_server_name.as_ref() {
        let n = name.trim();
        if !n.is_empty() {
            return Some(n.to_string());
        }
    }
    endpoint_host(address).map(ToString::to_string)
}

pub mod controller_proto {
    tonic::include_proto!("kcore.controller");
}

pub mod node_proto {
    tonic::include_proto!("kcore.node");
}

/// Resolve TLS PEM material from `ConnectionInfo`. Prefers already-decoded inline
/// PEM (`*_pem` fields). Falls back to reading from file paths (`ca`/`cert`/`key`).
/// Returns `(ca_pem, client_cert_pem, client_key_pem)`.
fn resolve_tls_pems(info: &ConnectionInfo) -> Result<(String, String, String)> {
    let ca_pem = if let Some(pem) = &info.ca_pem {
        pem.clone()
    } else {
        let path = info.ca.as_deref().context(
            "no CA certificate configured (set ca-data in config or use `kctl create cluster`)",
        )?;
        crate::path_safety::assert_safe_path(path, "TLS CA certificate path")?;
        std::fs::read_to_string(path)
            .with_context(|| format!("reading TLS CA certificate at {path}"))?
    };

    let client_cert_pem = if let Some(pem) = &info.cert_pem {
        pem.clone()
    } else {
        let path = info
            .cert
            .as_deref()
            .context("no client certificate configured (set cert-data in config or use `kctl create cluster`)")?;
        crate::path_safety::assert_safe_path(path, "TLS client certificate path")?;
        std::fs::read_to_string(path)
            .with_context(|| format!("reading TLS client certificate at {path}"))?
    };

    let client_key_pem = if let Some(pem) = &info.key_pem {
        pem.clone()
    } else {
        let path = info.key.as_deref().context(
            "no client key configured (set key-data in config or use `kctl create cluster`)",
        )?;
        crate::path_safety::assert_safe_path(path, "TLS client private key path")?;
        std::fs::read_to_string(path)
            .with_context(|| format!("reading TLS client private key at {path}"))?
    };

    Ok((ca_pem, client_cert_pem, client_key_pem))
}

/// Short SHA-256 fingerprint of the PEM-encoded CA for error messages.
fn ca_fingerprint_short(ca_pem: &str) -> String {
    use std::fmt::Write;
    let digest = aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, ca_pem.as_bytes());
    digest
        .as_ref()
        .iter()
        .take(4)
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02X}");
            s
        })
}

fn cert_source_label(info: &ConnectionInfo, is_ca: bool) -> String {
    if is_ca {
        if info.ca_pem.is_some() {
            "inline ca-data in config".to_string()
        } else {
            info.ca.as_deref().unwrap_or("<unknown>").to_string()
        }
    } else if info.cert_pem.is_some() {
        "inline cert-data in config".to_string()
    } else {
        info.cert.as_deref().unwrap_or("<unknown>").to_string()
    }
}

/// Verify that the client cert chains up to the configured CA.
/// Supports both direct signing (leaf signed by root CA) and intermediate
/// chains (leaf signed by sub-CA, sub-CA signed by root CA, with both
/// PEM-concatenated in `client_cert_pem`).
fn verify_cert_chain(ca_pem: &str, client_cert_pem: &str, info: &ConnectionInfo) -> Result<()> {
    let ca_der = pem::parse_x509_pem(ca_pem.as_bytes())
        .map_err(|e| anyhow::anyhow!("CA certificate is not valid PEM: {e}"))?
        .1;
    let ca_cert = ca_der
        .parse_x509()
        .map_err(|e| anyhow::anyhow!("CA certificate is not valid X.509: {e}"))?;

    let all_pems: Vec<_> = pem::Pem::iter_from_buffer(client_cert_pem.as_bytes())
        .filter_map(|r| r.ok())
        .collect();
    if all_pems.is_empty() {
        anyhow::bail!("client certificate PEM contains no valid certificates");
    }

    let leaf = all_pems[0]
        .parse_x509()
        .map_err(|e| anyhow::anyhow!("client certificate is not valid X.509: {e}"))?;

    if leaf.verify_signature(Some(ca_cert.public_key())).is_ok() {
        return Ok(());
    }

    for intermediate_pem in &all_pems[1..] {
        if let Ok(intermediate) = intermediate_pem.parse_x509() {
            let leaf_ok = leaf
                .verify_signature(Some(intermediate.public_key()))
                .is_ok();
            let chain_ok = intermediate
                .verify_signature(Some(ca_cert.public_key()))
                .is_ok();
            if leaf_ok && chain_ok {
                return Ok(());
            }
        }
    }

    anyhow::bail!(
        "client certificate ({}) was signed by a different CA than \
         the configured trust root ({}) — your config may point to stale \
         certificates from a previous cluster PKI generation. \
         Re-run `kctl create cluster` or update cert-data/ca-data in your config.",
        cert_source_label(info, false),
        cert_source_label(info, true),
    )
}

/// Pre-flight check: parse the CA and client cert PEMs to catch common
/// misconfigurations before the TLS handshake.  Produces actionable errors
/// instead of opaque "transport error" messages.
fn validate_tls_materials(
    ca_pem: &str,
    client_cert_pem: &str,
    info: &ConnectionInfo,
) -> Result<()> {
    verify_cert_chain(ca_pem, client_cert_pem, info)?;

    let ca_der = pem::parse_x509_pem(ca_pem.as_bytes())
        .map_err(|e| anyhow::anyhow!("CA certificate is not valid PEM: {e}"))?
        .1;
    let ca_cert = ca_der
        .parse_x509()
        .map_err(|e| anyhow::anyhow!("CA certificate is not valid X.509: {e}"))?;

    let leaf_der = pem::parse_x509_pem(client_cert_pem.as_bytes())
        .map_err(|e| anyhow::anyhow!("client certificate is not valid PEM: {e}"))?
        .1;
    let leaf = leaf_der
        .parse_x509()
        .map_err(|e| anyhow::anyhow!("client certificate is not valid X.509: {e}"))?;

    let now = ::time::OffsetDateTime::now_utc();

    let not_after_ts = leaf.validity().not_after.timestamp();
    if now.unix_timestamp() > not_after_ts {
        let expiry = leaf.validity().not_after.to_rfc2822().unwrap_or_default();
        anyhow::bail!(
            "client certificate ({}) expired on {expiry} — \
             regenerate with `kctl create cluster` or rotate certs",
            cert_source_label(info, false),
        );
    }

    let ca_not_after_ts = ca_cert.validity().not_after.timestamp();
    if now.unix_timestamp() > ca_not_after_ts {
        let expiry = ca_cert
            .validity()
            .not_after
            .to_rfc2822()
            .unwrap_or_default();
        anyhow::bail!(
            "CA certificate expired on {expiry} — \
             the cluster needs a new CA; re-bootstrap with `kctl create cluster --force`"
        );
    }

    Ok(())
}

pub async fn connect(info: &ConnectionInfo) -> Result<Channel> {
    let addresses = if info.addresses.is_empty() {
        vec![info.address.clone()]
    } else {
        info.addresses.clone()
    };

    let tls_pems: Option<(String, String, String)> = if info.insecure {
        None
    } else {
        let pems = resolve_tls_pems(info)?;
        validate_tls_materials(&pems.0, &pems.1, info)?;
        Some(pems)
    };

    let ca_fp = tls_pems.as_ref().map(|(ca, _, _)| ca_fingerprint_short(ca));

    let mut errors = Vec::new();
    for address in &addresses {
        let scheme = if info.insecure { "http" } else { "https" };
        let uri = format!("{scheme}://{address}");
        let mut endpoint = Endpoint::from_shared(uri.clone())?
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(30));

        if let Some((ca_pem, client_cert_pem, client_key_pem)) = &tls_pems {
            let mut tls = ClientTlsConfig::new();
            tls = tls.ca_certificate(Certificate::from_pem(ca_pem.as_bytes()));
            tls = tls.identity(Identity::from_pem(
                client_cert_pem.as_bytes(),
                client_key_pem.as_bytes(),
            ));
            if let Some(host) = tls_domain_name_for_endpoint(address, info) {
                tls = tls.domain_name(host);
            }
            endpoint = endpoint.tls_config(tls)?;
        }

        match endpoint.connect().await {
            Ok(channel) => return Ok(channel),
            Err(e) => {
                let hint = if let Some(fp) = &ca_fp {
                    format!(
                        " (local CA fingerprint: {fp}… — if this doesn't match the cluster, \
                         re-run `kctl create cluster` or update your config)"
                    )
                } else {
                    String::new()
                };
                errors.push(format!("{address}: {}{hint}", format_transport_error(&e),));
            }
        }
    }
    anyhow::bail!(
        "failed to connect to any controller endpoint: {}",
        errors.join(" | ")
    )
}

fn endpoint_host(address: &str) -> Option<&str> {
    if let Some(rest) = address.strip_prefix('[') {
        if let Some(end_idx) = rest.find(']') {
            return Some(&rest[..end_idx]);
        }
    }
    address
        .rsplit_once(':')
        .map(|(host, _)| host)
        .or(Some(address))
}

pub async fn controller_client(
    info: &ConnectionInfo,
) -> Result<controller_proto::controller_client::ControllerClient<Channel>> {
    let channel = connect(info).await?;
    Ok(controller_proto::controller_client::ControllerClient::new(
        channel,
    ))
}

pub async fn controller_admin_client(
    info: &ConnectionInfo,
) -> Result<controller_proto::controller_admin_client::ControllerAdminClient<Channel>> {
    let channel = connect(info).await?;
    Ok(controller_proto::controller_admin_client::ControllerAdminClient::new(channel))
}

pub async fn node_compute_client(
    info: &ConnectionInfo,
) -> Result<node_proto::node_compute_client::NodeComputeClient<Channel>> {
    let channel = connect(info).await?;
    Ok(node_proto::node_compute_client::NodeComputeClient::new(
        channel,
    ))
}

pub async fn node_admin_client(
    info: &ConnectionInfo,
) -> Result<node_proto::node_admin_client::NodeAdminClient<Channel>> {
    let channel = connect(info).await?;
    Ok(node_proto::node_admin_client::NodeAdminClient::new(channel)
        .max_encoding_message_size(1024 * 1024 * 1024)
        .max_decoding_message_size(1024 * 1024 * 1024))
}

pub async fn node_container_client(
    info: &ConnectionInfo,
) -> Result<node_proto::node_container_client::NodeContainerClient<Channel>> {
    let channel = connect(info).await?;
    Ok(node_proto::node_container_client::NodeContainerClient::new(
        channel,
    ))
}

/// Parse a human-readable size string (e.g. "4G", "8192M", "1T") into bytes.
///
/// Only integer quantities are accepted. Decimal values like `"1.5G"` are
/// rejected with a clear error rather than silently parsed as `1` plus a
/// nonsense unit (`".5g"`), which is what the previous greedy split did.
pub fn parse_size_bytes(s: &str) -> Result<i64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty size string".to_string());
    }

    let split_at = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num_part, unit) = s.split_at(split_at);

    if num_part.is_empty() {
        return Err(format!("missing numeric value in {s:?}"));
    }
    // Reject decimals up front. The previous split-on-first-non-digit
    // would silently slice "1.5G" into ("1", ".5g") and then fail with a
    // confusing "unknown unit: .5g" message that hid the real mistake.
    if unit.starts_with('.') {
        return Err(format!(
            "decimal sizes are not supported (got {s:?}); use an integer in a smaller unit, e.g. 1500M"
        ));
    }

    let value: i64 = num_part
        .parse()
        .map_err(|_| format!("invalid number: {num_part}"))?;
    if value < 0 {
        return Err(format!("size must be non-negative, got {value}"));
    }

    let multiplier: i64 = match unit.trim().to_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024 * 1024,
        "g" | "gb" | "gib" => 1024 * 1024 * 1024,
        "t" | "tb" | "tib" => 1024i64 * 1024 * 1024 * 1024,
        other => return Err(format!("unknown unit: {other}")),
    };

    value
        .checked_mul(multiplier)
        .ok_or_else(|| format!("size {s:?} overflows i64"))
}

pub fn format_bytes(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    for unit in &UNITS {
        if value < 1024.0 {
            return if value.fract() == 0.0 {
                format!("{value:.0} {unit}")
            } else {
                format!("{value:.1} {unit}")
            };
        }
        value /= 1024.0;
    }
    format!("{value:.1} PB")
}

#[cfg(test)]
mod size_tests {
    use super::{format_bytes, parse_size_bytes};

    #[test]
    fn parse_size_bytes_basic_units() {
        assert_eq!(parse_size_bytes("0").unwrap(), 0);
        assert_eq!(parse_size_bytes("0b").unwrap(), 0);
        assert_eq!(parse_size_bytes("512").unwrap(), 512);
        assert_eq!(parse_size_bytes("1K").unwrap(), 1024);
        assert_eq!(parse_size_bytes("1KB").unwrap(), 1024);
        assert_eq!(parse_size_bytes("1KiB").unwrap(), 1024);
        assert_eq!(parse_size_bytes("4M").unwrap(), 4 * 1024 * 1024);
        assert_eq!(parse_size_bytes("8G").unwrap(), 8i64 * 1024 * 1024 * 1024);
        assert_eq!(
            parse_size_bytes("1T").unwrap(),
            1024i64 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn parse_size_bytes_is_case_insensitive_and_trims() {
        assert_eq!(parse_size_bytes(" 4g ").unwrap(), 4i64 * 1024 * 1024 * 1024);
        assert_eq!(
            parse_size_bytes("16Gb").unwrap(),
            16i64 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn parse_size_bytes_rejects_decimals_with_clear_message() {
        // Regression: the old greedy split sliced "1.5G" into ("1", ".5g")
        // and failed with "unknown unit: .5g" — operators couldn't tell
        // whether the unit or the number was wrong.
        let err = parse_size_bytes("1.5G").expect_err("decimals must be rejected");
        assert!(
            err.contains("decimal sizes are not supported"),
            "should explain decimals are not supported, got: {err}"
        );
    }

    #[test]
    fn parse_size_bytes_rejects_unknown_unit() {
        let err = parse_size_bytes("4Q").expect_err("unknown unit");
        assert!(err.contains("unknown unit"), "got: {err}");
    }

    #[test]
    fn parse_size_bytes_rejects_empty_and_whitespace() {
        assert!(parse_size_bytes("").is_err());
        assert!(parse_size_bytes("   ").is_err());
    }

    #[test]
    fn parse_size_bytes_rejects_missing_number() {
        let err = parse_size_bytes("MB").expect_err("number required");
        assert!(err.contains("missing numeric value"), "got: {err}");
    }

    #[test]
    fn parse_size_bytes_rejects_overflow() {
        // 10 EiB-worth of GiB will overflow i64 multiplication.
        let err = parse_size_bytes("99999999999G").expect_err("must overflow");
        assert!(err.contains("overflow"), "got: {err}");
    }

    #[test]
    fn format_bytes_renders_human_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1 GB");
        assert_eq!(format_bytes(1024i64 * 1024 * 1024 * 1024), "1 TB");
    }

    #[test]
    fn format_bytes_falls_through_to_pb() {
        let pb = 1024i64 * 1024 * 1024 * 1024 * 1024;
        let s = format_bytes(pb);
        assert!(s.ends_with("PB"), "expected PB suffix, got {s}");
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use tokio_stream::wrappers::TcpListenerStream;
    use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};

    use crate::config::ConnectionInfo;
    use crate::pki;

    async fn start_mtls_server(certs_dir: &Path) -> (String, oneshot::Sender<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");

        let cert_pem =
            std::fs::read_to_string(certs_dir.join("controller.crt")).expect("controller cert");
        let key_pem =
            std::fs::read_to_string(certs_dir.join("controller.key")).expect("controller key");
        let ca_pem = std::fs::read_to_string(certs_dir.join("ca.crt")).expect("ca cert");

        let tls = ServerTlsConfig::new()
            .identity(Identity::from_pem(cert_pem, key_pem))
            .client_ca_root(Certificate::from_pem(ca_pem));

        let (_reporter, service) = tonic_health::server::health_reporter();
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            Server::builder()
                .tls_config(tls)
                .expect("tls config")
                .add_service(service)
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                    let _ = rx.await;
                })
                .await
                .expect("serve");
        });

        (addr.to_string(), tx)
    }

    async fn start_insecure_server() -> (String, oneshot::Sender<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let (_reporter, service) = tonic_health::server::health_reporter();
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            Server::builder()
                .add_service(service)
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async move {
                    let _ = rx.await;
                })
                .await
                .expect("serve");
        });
        (addr.to_string(), tx)
    }

    fn ensure_crypto_provider() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }

    #[tokio::test]
    async fn connect_rejects_path_traversal_in_tls_paths() {
        ensure_crypto_provider();
        let info = ConnectionInfo {
            address: "127.0.0.1:9090".to_string(),
            addresses: vec![],
            insecure: false,
            tls_server_name: None,
            cert_pem: None,
            key_pem: None,
            ca_pem: None,
            cert: Some("/tmp/fake.crt".to_string()),
            key: Some("/tmp/fake.key".to_string()),
            ca: Some("../../../etc/passwd".to_string()),
        };
        let err = super::connect(&info).await.expect_err("traversal");
        let s = format!("{err:#}");
        assert!(
            s.contains("TLS CA certificate path") && s.contains(".."),
            "unexpected error: {s}"
        );
    }

    #[tokio::test]
    async fn connect_rejects_path_traversal_in_client_key_path() {
        ensure_crypto_provider();
        let temp = tempfile::tempdir().expect("tempdir");
        let ca_path = temp.path().join("ca.crt");
        let cert_path = temp.path().join("kctl.crt");
        std::fs::write(&ca_path, "fake-ca").expect("write ca");
        std::fs::write(&cert_path, "fake-cert").expect("write cert");
        let info = ConnectionInfo {
            address: "127.0.0.1:9090".to_string(),
            addresses: vec![],
            insecure: false,
            tls_server_name: None,
            cert_pem: None,
            key_pem: None,
            ca_pem: None,
            cert: Some(cert_path.display().to_string()),
            key: Some("../../etc/passwd".to_string()),
            ca: Some(ca_path.display().to_string()),
        };
        let err = super::connect(&info).await.expect_err("traversal");
        let s = format!("{err:#}");
        assert!(
            s.contains("TLS client private key path") && s.contains(".."),
            "unexpected error: {s}"
        );
    }

    #[tokio::test]
    async fn mtls_connect_succeeds_with_valid_client_cert() {
        ensure_crypto_provider();
        let temp = tempfile::tempdir().expect("tempdir");
        let certs_dir = temp.path().join("certs");
        pki::create_cluster_pki(&certs_dir, "127.0.0.1", false).expect("create pki");
        let (addr, shutdown) = start_mtls_server(&certs_dir).await;

        let info = ConnectionInfo {
            address: addr,
            addresses: vec![],
            insecure: false,
            tls_server_name: None,
            cert_pem: None,
            key_pem: None,
            ca_pem: None,
            cert: Some(certs_dir.join("kctl.crt").display().to_string()),
            key: Some(certs_dir.join("kctl.key").display().to_string()),
            ca: Some(certs_dir.join("ca.crt").display().to_string()),
        };

        let channel = super::connect(&info).await.expect("channel");
        let mut health = tonic_health::pb::health_client::HealthClient::new(channel);
        let resp = health
            .check(tonic_health::pb::HealthCheckRequest {
                service: String::new(),
            })
            .await;
        let _ = shutdown.send(());
        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn mtls_connect_succeeds_with_inline_pem_data() {
        ensure_crypto_provider();
        let temp = tempfile::tempdir().expect("tempdir");
        let certs_dir = temp.path().join("certs");
        pki::create_cluster_pki(&certs_dir, "127.0.0.1", false).expect("create pki");
        let (addr, shutdown) = start_mtls_server(&certs_dir).await;

        let ca_pem = std::fs::read_to_string(certs_dir.join("ca.crt")).expect("ca");
        let cert_pem = std::fs::read_to_string(certs_dir.join("kctl.crt")).expect("cert");
        let key_pem = std::fs::read_to_string(certs_dir.join("kctl.key")).expect("key");

        let info = ConnectionInfo {
            address: addr,
            addresses: vec![],
            insecure: false,
            tls_server_name: None,
            cert_pem: Some(cert_pem),
            key_pem: Some(key_pem),
            ca_pem: Some(ca_pem),
            cert: None,
            key: None,
            ca: None,
        };

        let channel = super::connect(&info).await.expect("channel");
        let mut health = tonic_health::pb::health_client::HealthClient::new(channel);
        let resp = health
            .check(tonic_health::pb::HealthCheckRequest {
                service: String::new(),
            })
            .await;
        let _ = shutdown.send(());
        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn mtls_connect_fails_with_untrusted_client_cert() {
        ensure_crypto_provider();
        let good = tempfile::tempdir().expect("tempdir");
        let good_dir = good.path().join("certs");
        pki::create_cluster_pki(&good_dir, "127.0.0.1", false).expect("create pki");
        let (_addr, shutdown) = start_mtls_server(&good_dir).await;

        let bad = tempfile::tempdir().expect("tempdir");
        let bad_dir = bad.path().join("certs");
        pki::create_cluster_pki(&bad_dir, "127.0.0.1", false).expect("create pki");

        let info = ConnectionInfo {
            address: _addr,
            addresses: vec![],
            insecure: false,
            tls_server_name: None,
            cert_pem: None,
            key_pem: None,
            ca_pem: None,
            cert: Some(bad_dir.join("kctl.crt").display().to_string()),
            key: Some(bad_dir.join("kctl.key").display().to_string()),
            ca: Some(good_dir.join("ca.crt").display().to_string()),
        };

        let err = super::connect(&info)
            .await
            .expect_err("preflight should catch mismatch");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("signed by a different CA"),
            "expected CA mismatch, got: {msg}"
        );
        let _ = shutdown.send(());
    }

    #[tokio::test]
    async fn connect_falls_back_to_second_controller_endpoint() {
        let (addr, shutdown) = start_insecure_server().await;
        let info = ConnectionInfo {
            address: "127.0.0.1:1".to_string(),
            addresses: vec!["127.0.0.1:1".to_string(), addr],
            insecure: true,
            tls_server_name: None,
            cert_pem: None,
            key_pem: None,
            ca_pem: None,
            cert: None,
            key: None,
            ca: None,
        };
        let channel = super::connect(&info).await.expect("fallback connect");
        let mut health = tonic_health::pb::health_client::HealthClient::new(channel);
        let resp = health
            .check(tonic_health::pb::HealthCheckRequest {
                service: String::new(),
            })
            .await;
        let _ = shutdown.send(());
        assert!(resp.is_ok());
    }

    #[tokio::test]
    async fn preflight_detects_ca_cert_mismatch() {
        ensure_crypto_provider();
        let good = tempfile::tempdir().expect("tempdir");
        let good_dir = good.path().join("certs");
        pki::create_cluster_pki(&good_dir, "127.0.0.1", false).expect("create pki");

        let bad = tempfile::tempdir().expect("tempdir");
        let bad_dir = bad.path().join("certs");
        pki::create_cluster_pki(&bad_dir, "127.0.0.1", false).expect("create pki");

        let info = ConnectionInfo {
            address: "127.0.0.1:9090".to_string(),
            addresses: vec![],
            insecure: false,
            tls_server_name: None,
            cert_pem: None,
            key_pem: None,
            ca_pem: None,
            cert: Some(bad_dir.join("kctl.crt").display().to_string()),
            key: Some(bad_dir.join("kctl.key").display().to_string()),
            ca: Some(good_dir.join("ca.crt").display().to_string()),
        };
        let err = super::connect(&info)
            .await
            .expect_err("should detect mismatch");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("signed by a different CA"),
            "expected CA mismatch error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn preflight_detects_ca_cert_mismatch_inline() {
        ensure_crypto_provider();
        let good = tempfile::tempdir().expect("tempdir");
        let good_dir = good.path().join("certs");
        pki::create_cluster_pki(&good_dir, "127.0.0.1", false).expect("create pki");

        let bad = tempfile::tempdir().expect("tempdir");
        let bad_dir = bad.path().join("certs");
        pki::create_cluster_pki(&bad_dir, "127.0.0.1", false).expect("create pki");

        let ca_pem = std::fs::read_to_string(good_dir.join("ca.crt")).expect("ca");
        let cert_pem = std::fs::read_to_string(bad_dir.join("kctl.crt")).expect("cert");
        let key_pem = std::fs::read_to_string(bad_dir.join("kctl.key")).expect("key");

        let info = ConnectionInfo {
            address: "127.0.0.1:9090".to_string(),
            addresses: vec![],
            insecure: false,
            tls_server_name: None,
            cert_pem: Some(cert_pem),
            key_pem: Some(key_pem),
            ca_pem: Some(ca_pem),
            cert: None,
            key: None,
            ca: None,
        };
        let err = super::connect(&info)
            .await
            .expect_err("should detect mismatch");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("signed by a different CA"),
            "expected CA mismatch error, got: {msg}"
        );
    }

    #[test]
    fn preflight_accepts_chain_cert_via_sub_ca() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let temp = tempfile::tempdir().expect("tempdir");
        let certs_dir = temp.path().join("certs");
        pki::create_cluster_pki(&certs_dir, "127.0.0.1", false).expect("create pki");

        let ca_pem = std::fs::read_to_string(certs_dir.join("ca.crt")).expect("ca");
        let sub_ca_pem =
            std::fs::read_to_string(certs_dir.join("sub-ca.crt")).expect("sub-ca cert");
        let sub_ca_key = std::fs::read_to_string(certs_dir.join("sub-ca.key")).expect("sub-ca key");

        let (leaf_chain, _key) =
            pki::sign_node_cert_with_sub_ca(&sub_ca_pem, &sub_ca_key, "10.0.0.1")
                .expect("sign node cert with sub-ca");

        let info = ConnectionInfo {
            address: "127.0.0.1:9090".to_string(),
            addresses: vec![],
            insecure: false,
            tls_server_name: None,
            cert_pem: Some(leaf_chain.clone()),
            key_pem: None,
            ca_pem: Some(ca_pem.clone()),
            cert: None,
            key: None,
            ca: None,
        };
        super::validate_tls_materials(&ca_pem, &leaf_chain, &info)
            .expect("chain cert signed by sub-CA should pass validation");
    }

    #[test]
    fn preflight_accepts_matching_ca_and_cert() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let temp = tempfile::tempdir().expect("tempdir");
        let certs_dir = temp.path().join("certs");
        pki::create_cluster_pki(&certs_dir, "127.0.0.1", false).expect("create pki");

        let ca_pem = std::fs::read_to_string(certs_dir.join("ca.crt")).expect("ca");
        let cert_pem = std::fs::read_to_string(certs_dir.join("kctl.crt")).expect("cert");

        let info = ConnectionInfo {
            address: "127.0.0.1:9090".to_string(),
            addresses: vec![],
            insecure: false,
            tls_server_name: None,
            cert_pem: Some(cert_pem.clone()),
            key_pem: None,
            ca_pem: Some(ca_pem.clone()),
            cert: None,
            key: None,
            ca: None,
        };
        super::validate_tls_materials(&ca_pem, &cert_pem, &info)
            .expect("matching CA and cert should pass validation");
    }
}
