//! gRPC client for the controller (kctl-equivalent transport).

use anyhow::{Context, Result};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity};

use crate::config::DashboardConfig;

pub mod controller_proto {
    tonic::include_proto!("kcore.controller");
}

pub fn endpoint_host(address: &str) -> Option<&str> {
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

pub async fn connect_channel(cfg: &DashboardConfig) -> Result<Channel> {
    let scheme = if cfg.insecure { "http" } else { "https" };
    let uri = format!("{scheme}://{}", cfg.controller_addr);
    let mut endpoint = Endpoint::from_shared(uri.clone()).context("invalid controller URI")?;

    if !cfg.insecure {
        let (ca_pem, cert_pem, key_pem) = cfg.tls_pem_strings()?;
        let mut tls = ClientTlsConfig::new();
        tls = tls.ca_certificate(Certificate::from_pem(ca_pem));
        tls = tls.identity(Identity::from_pem(cert_pem, key_pem));
        let verify_name = cfg
            .tls_domain
            .clone()
            .or_else(|| endpoint_host(&cfg.controller_addr).map(str::to_string));
        if let Some(host) = verify_name {
            tls = tls.domain_name(host);
        }
        endpoint = endpoint
            .tls_config(tls)
            .context("TLS config for controller endpoint")?;
    }

    endpoint.connect().await.with_context(|| {
        format!(
            "gRPC to {} (mTLS: set {} if the cert is for a hostname, not the connection IP)",
            cfg.controller_addr,
            crate::config::ENV_TLS_DOMAIN
        )
    })
}

pub async fn get_compliance(
    cfg: &DashboardConfig,
) -> Result<controller_proto::GetComplianceReportResponse> {
    let channel = connect_channel(cfg).await?;
    let mut client = controller_proto::controller_client::ControllerClient::new(channel);
    let resp = client
        .get_compliance_report(controller_proto::GetComplianceReportRequest {})
        .await
        .context("GetComplianceReport RPC")?;
    Ok(resp.into_inner())
}

pub async fn list_vms(cfg: &DashboardConfig) -> Result<Vec<controller_proto::VmInfo>> {
    let channel = connect_channel(cfg).await?;
    let mut client = controller_proto::controller_client::ControllerClient::new(channel);
    let resp = client
        .list_vms(controller_proto::ListVmsRequest {
            target_node: String::new(),
        })
        .await
        .context("ListVms RPC")?;
    Ok(resp.into_inner().vms)
}

pub async fn list_networks(cfg: &DashboardConfig) -> Result<Vec<controller_proto::NetworkInfo>> {
    let channel = connect_channel(cfg).await?;
    let mut client = controller_proto::controller_client::ControllerClient::new(channel);
    let resp = client
        .list_networks(controller_proto::ListNetworksRequest {
            target_node: String::new(),
        })
        .await
        .context("ListNetworks RPC")?;
    Ok(resp.into_inner().networks)
}

pub async fn get_network_overview(
    cfg: &DashboardConfig,
) -> Result<controller_proto::GetNetworkOverviewResponse> {
    let channel = connect_channel(cfg).await?;
    let mut client = controller_proto::controller_client::ControllerClient::new(channel);
    let resp = client
        .get_network_overview(controller_proto::GetNetworkOverviewRequest {})
        .await
        .context("GetNetworkOverview RPC")?;
    Ok(resp.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_host_parsing() {
        assert_eq!(endpoint_host("10.0.0.1:9090"), Some("10.0.0.1"));
        assert_eq!(endpoint_host("[::1]:9090"), Some("::1"));
    }
}
