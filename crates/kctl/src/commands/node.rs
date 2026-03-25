use crate::client::{self, controller_proto, node_proto};
use crate::config::ConnectionInfo;
use crate::output;
use crate::pki;
use anyhow::{Context, Result};
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

pub async fn get_node(info: &ConnectionInfo, node_id: &str) -> Result<()> {
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
    join_controller: Option<&str>,
    run_controller: bool,
    certs_dir: &Path,
) -> Result<()> {
    let join_controller = validate_install_controller_mode(join_controller, run_controller)?;

    let node_host =
        pki::host_from_address(&info.address).map_err(|e| anyhow::anyhow!("node address: {e}"))?;

    let node_is_controller = run_controller;

    let install_pki = pki::load_install_pki(certs_dir, &node_host, node_is_controller)
        .map_err(|e| anyhow::anyhow!("loading PKI: {e}"))?;

    let mut client = client::node_admin_client(info).await?;
    let resp = client
        .install_to_disk(node_proto::InstallToDiskRequest {
            os_disk: os_disk.to_string(),
            data_disks,
            controller: join_controller.to_string(),
            run_controller,
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

fn validate_install_controller_mode(
    join_controller: Option<&str>,
    run_controller: bool,
) -> Result<String> {
    let join = join_controller.map(str::trim).unwrap_or("");
    let has_join = !join.is_empty();
    if has_join == run_controller {
        anyhow::bail!("provide exactly one of --join-controller <host:port> or --run-controller");
    }
    Ok(join.to_string())
}

pub async fn apply_nix(info: &ConnectionInfo, file: &str, rebuild: bool) -> Result<()> {
    let content = std::fs::read_to_string(file).with_context(|| format!("reading {file}"))?;

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

#[cfg(test)]
mod tests {
    use super::validate_install_controller_mode;

    #[test]
    fn validate_install_mode_rejects_neither() {
        let err = validate_install_controller_mode(None, false).expect_err("should fail");
        assert!(err
            .to_string()
            .contains("provide exactly one of --join-controller"));
    }

    #[test]
    fn validate_install_mode_rejects_both() {
        let err = validate_install_controller_mode(Some("192.168.1.10:9090"), true)
            .expect_err("should fail");
        assert!(err
            .to_string()
            .contains("provide exactly one of --join-controller"));
    }

    #[test]
    fn validate_install_mode_accepts_join_only() {
        let join = validate_install_controller_mode(Some(" 192.168.1.10:9090 "), false)
            .expect("should pass");
        assert_eq!(join, "192.168.1.10:9090");
    }

    #[test]
    fn validate_install_mode_accepts_run_controller_only() {
        let join = validate_install_controller_mode(None, true).expect("should pass");
        assert!(join.is_empty());
    }
}
