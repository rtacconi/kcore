use anyhow::{Context, Result};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

use crate::config::ConnectionInfo;

pub mod controller_proto {
    tonic::include_proto!("kcore.controller");
}

pub mod node_proto {
    tonic::include_proto!("kcore.node");
}

pub async fn connect(info: &ConnectionInfo) -> Result<Channel> {
    let scheme = if info.insecure { "http" } else { "https" };
    let uri = format!("{scheme}://{}", info.address);

    let mut endpoint = Endpoint::from_shared(uri)?;

    if !info.insecure {
        let ca = info
            .ca
            .as_ref()
            .context("missing CA certificate path for TLS connection")?;
        let cert = info
            .cert
            .as_ref()
            .context("missing client certificate path for mTLS connection")?;
        let key = info
            .key
            .as_ref()
            .context("missing client key path for mTLS connection")?;

        let mut tls = ClientTlsConfig::new();

        let ca_pem =
            std::fs::read_to_string(ca).with_context(|| format!("reading CA cert {ca}"))?;
        tls = tls.ca_certificate(Certificate::from_pem(ca_pem));

        let cert_pem =
            std::fs::read_to_string(cert).with_context(|| format!("reading client cert {cert}"))?;
        let key_pem =
            std::fs::read_to_string(key).with_context(|| format!("reading client key {key}"))?;
        tls = tls.identity(Identity::from_pem(cert_pem, key_pem));

        endpoint = endpoint.tls_config(tls)?;
    }

    let channel = endpoint.connect().await?;
    Ok(channel)
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

/// Parse a human-readable size string (e.g. "4G", "8192M", "1T") into bytes.
pub fn parse_size_bytes(s: &str) -> Result<i64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty size string".to_string());
    }

    let (num_part, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));

    let value: i64 = num_part
        .parse()
        .map_err(|_| format!("invalid number: {num_part}"))?;

    let multiplier: i64 = match unit.to_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024 * 1024,
        "g" | "gb" | "gib" => 1024 * 1024 * 1024,
        "t" | "tb" | "tib" => 1024 * 1024 * 1024 * 1024,
        other => return Err(format!("unknown unit: {other}")),
    };

    Ok(value * multiplier)
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

    fn ensure_crypto_provider() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
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
            insecure: false,
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
    async fn mtls_connect_fails_with_untrusted_client_cert() {
        ensure_crypto_provider();
        let good = tempfile::tempdir().expect("tempdir");
        let good_dir = good.path().join("certs");
        pki::create_cluster_pki(&good_dir, "127.0.0.1", false).expect("create pki");
        let (addr, shutdown) = start_mtls_server(&good_dir).await;

        let bad = tempfile::tempdir().expect("tempdir");
        let bad_dir = bad.path().join("certs");
        pki::create_cluster_pki(&bad_dir, "127.0.0.1", false).expect("create pki");

        let info = ConnectionInfo {
            address: addr,
            insecure: false,
            cert: Some(bad_dir.join("kctl.crt").display().to_string()),
            key: Some(bad_dir.join("kctl.key").display().to_string()),
            ca: Some(good_dir.join("ca.crt").display().to_string()),
        };

        let channel = super::connect(&info).await.expect("channel");
        let mut health = tonic_health::pb::health_client::HealthClient::new(channel);
        let resp = health
            .check(tonic_health::pb::HealthCheckRequest {
                service: String::new(),
            })
            .await;
        let _ = shutdown.send(());
        assert!(resp.is_err());
    }
}
