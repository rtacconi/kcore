//! Leptos server functions — call controller gRPC using [`crate::state::dashboard_config`].

use leptos::prelude::*;

use crate::controller_client;
use crate::dto::{ComplianceDto, NetworkOverviewDto, NetworkRowDto, VmsPageDto};
use crate::mappers::{
    compliance_from_proto, network_overview_from_proto, networks_from_proto, vms_page_from_proto,
};
use crate::state::dashboard_config;

fn map_err(e: anyhow::Error) -> ServerFnError {
    ServerFnError::new(format!("{e:#}"))
}

#[server(GetComplianceReport, "/api")]
pub async fn get_compliance_dto() -> Result<ComplianceDto, ServerFnError> {
    let cfg = dashboard_config();
    let r = controller_client::get_compliance(cfg)
        .await
        .map_err(map_err)?;
    Ok(compliance_from_proto(r))
}

#[server(ListVmsPage, "/api")]
pub async fn list_vms_page(page: u32) -> Result<VmsPageDto, ServerFnError> {
    let cfg = dashboard_config();
    let vms = controller_client::list_vms(cfg).await.map_err(map_err)?;
    Ok(vms_page_from_proto(vms, page))
}

#[server(ListNetworks, "/api")]
pub async fn list_networks_dto() -> Result<Vec<NetworkRowDto>, ServerFnError> {
    let cfg = dashboard_config();
    let nets = controller_client::list_networks(cfg)
        .await
        .map_err(map_err)?;
    Ok(networks_from_proto(nets))
}

#[server(GetNetworkOverview, "/api")]
pub async fn get_network_overview_dto() -> Result<NetworkOverviewDto, ServerFnError> {
    let cfg = dashboard_config();
    let overview = controller_client::get_network_overview(cfg)
        .await
        .map_err(map_err)?;
    Ok(network_overview_from_proto(overview))
}
