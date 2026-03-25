use anyhow::{Context, Result};
use crate::client::{self, controller_proto, node_proto};
use crate::config::ConnectionInfo;
use crate::output;
use crate::pki;
use std::path::Path;

pub async fn list_nodes(info: &ConnectionInfo) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .list_nodes(controller_proto::ListNodesRequest {})
        .await?
        .into_inner();

    if resp.nodes.is_empty() {
        println!("No nodes found");
        return Ok(());
    }

    output::print_node_table(&resp.nodes);
    Ok(())
}

pub async fn get_node(
    info: &ConnectionInfo,
    node_id: &str,
) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .get_node(controller_proto::GetNodeRequest {
            node_id: node_id.to_string(),
        })
        .await?
        .into_inner();

    let node = resp.node.as_ref().context("node not found")?;
    output::print_node_detail(node);
    Ok(())
}

pub async fn disks(info: &ConnectionInfo) -> Result<()> {
    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .list_disks(node_proto::ListDisksRequest {})
        .await?
        .into_inner();

    if resp.disks.is_empty() {
        println!("No disks found");
        return Ok(());
    }

    output::print_disk_table(&resp.disks);
    Ok(())
}

pub async fn nics(info: &ConnectionInfo) -> Result<()> {
    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .list_network_interfaces(node_proto::ListNetworkInterfacesRequest {})
        .await?
        .into_inner();

    if resp.interfaces.is_empty() {
        println!("No network interfaces found");
        return Ok(());
    }

    output::print_nic_table(&resp.interfaces);
    Ok(())
}

pub async fn install(
    info: &ConnectionInfo,
    os_disk: &str,
    data_disks: Vec<String>,
    join_controller: &str,
    certs_dir: &Path,
) -> Result<()> {
    let node_host = pki::host_from_address(&info.address)
        .map_err(|e| anyhow::anyhow!("node address: {e}"))?;

    // Include controller PKI only when the node will also run the controller
    // (i.e., --join-controller points at the node itself or is empty/local).
    let controller_host = if join_controller.is_empty() {
        String::new()
    } else {
        pki::host_from_address(join_controller).unwrap_or_default()
    };
    let node_is_controller = !controller_host.is_empty() && controller_host == node_host;

    let install_pki = pki::load_install_pki(certs_dir, &node_host, node_is_controller)
        .map_err(|e| anyhow::anyhow!("loading PKI: {e}"))?;

    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .install_to_disk(node_proto::InstallToDiskRequest {
            os_disk: os_disk.to_string(),
            data_disks,
            controller: join_controller.to_string(),
            ca_cert_pem: install_pki.ca_cert_pem,
            node_cert_pem: install_pki.node_cert_pem,
            node_key_pem: install_pki.node_key_pem,
            controller_cert_pem: install_pki.controller_cert_pem,
            controller_key_pem: install_pki.controller_key_pem,
            kctl_cert_pem: String::new(),
            kctl_key_pem: String::new(),
        })
        .await?
        .into_inner();

    if resp.accepted {
        println!("Install accepted: {}", resp.message);
    } else {
        eprintln!("Install rejected: {}", resp.message);
    }
    Ok(())
}

pub async fn apply_nix(
    info: &ConnectionInfo,
    file: &str,
    rebuild: bool,
) -> Result<()> {
    let content = std::fs::read_to_string(file)
        .with_context(|| format!("reading {file}"))?;

    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .apply_nix_config(node_proto::ApplyNixConfigRequest {
            configuration_nix: content,
            rebuild,
        })
        .await?
        .into_inner();

    if resp.success {
        println!("{}", resp.message);
    } else {
        eprintln!("Failed: {}", resp.message);
    }
    Ok(())
}
