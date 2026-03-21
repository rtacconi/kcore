use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

use crate::config::ConnectionInfo;

pub mod controller_proto {
    tonic::include_proto!("kcore.controller");
}

pub mod node_proto {
    tonic::include_proto!("kcore.node");
}

pub async fn connect(info: &ConnectionInfo) -> Result<Channel, Box<dyn std::error::Error>> {
    let scheme = if info.insecure { "http" } else { "https" };
    let uri = format!("{scheme}://{}", info.address);

    let mut endpoint = Endpoint::from_shared(uri)?;

    if !info.insecure {
        let mut tls = ClientTlsConfig::new();

        if let Some(ca) = &info.ca {
            let ca_pem =
                std::fs::read_to_string(ca).map_err(|e| format!("reading CA cert {ca}: {e}"))?;
            tls = tls.ca_certificate(Certificate::from_pem(ca_pem));
        }

        if let (Some(cert), Some(key)) = (&info.cert, &info.key) {
            let cert_pem = std::fs::read_to_string(cert)
                .map_err(|e| format!("reading client cert {cert}: {e}"))?;
            let key_pem = std::fs::read_to_string(key)
                .map_err(|e| format!("reading client key {key}: {e}"))?;
            tls = tls.identity(Identity::from_pem(cert_pem, key_pem));
        }

        endpoint = endpoint.tls_config(tls)?;
    }

    let channel = endpoint.connect().await?;
    Ok(channel)
}

pub async fn controller_client(
    info: &ConnectionInfo,
) -> Result<
    controller_proto::controller_client::ControllerClient<Channel>,
    Box<dyn std::error::Error>,
> {
    let channel = connect(info).await?;
    Ok(controller_proto::controller_client::ControllerClient::new(
        channel,
    ))
}

pub async fn controller_admin_client(
    info: &ConnectionInfo,
) -> Result<
    controller_proto::controller_admin_client::ControllerAdminClient<Channel>,
    Box<dyn std::error::Error>,
> {
    let channel = connect(info).await?;
    Ok(controller_proto::controller_admin_client::ControllerAdminClient::new(channel))
}

pub async fn node_compute_client(
    info: &ConnectionInfo,
) -> Result<node_proto::node_compute_client::NodeComputeClient<Channel>, Box<dyn std::error::Error>>
{
    let channel = connect(info).await?;
    Ok(node_proto::node_compute_client::NodeComputeClient::new(
        channel,
    ))
}

pub async fn node_admin_client(
    info: &ConnectionInfo,
) -> Result<node_proto::node_admin_client::NodeAdminClient<Channel>, Box<dyn std::error::Error>> {
    let channel = connect(info).await?;
    Ok(node_proto::node_admin_client::NodeAdminClient::new(channel))
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
