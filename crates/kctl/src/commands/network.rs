use anyhow::Result;

use crate::client::{self, controller_proto as proto};
use crate::config::ConnectionInfo;

pub struct CreateArgs {
    pub name: String,
    pub external_ip: String,
    pub gateway_ip: String,
    pub internal_netmask: String,
    pub target_node: Option<String>,
}

pub async fn create(info: &ConnectionInfo, args: CreateArgs) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .create_network(proto::CreateNetworkRequest {
            name: args.name.clone(),
            external_ip: args.external_ip,
            gateway_ip: args.gateway_ip,
            internal_netmask: args.internal_netmask,
            target_node: args.target_node.unwrap_or_default(),
        })
        .await?
        .into_inner();

    if resp.success {
        println!("Network '{}' created", args.name);
        println!("  Node: {}", resp.node_id);
        if !resp.message.is_empty() {
            println!("  Info: {}", resp.message);
        }
    } else {
        println!("Network '{}' creation rejected", args.name);
    }
    Ok(())
}

pub async fn delete(info: &ConnectionInfo, name: &str, target_node: Option<String>) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    client
        .delete_network(proto::DeleteNetworkRequest {
            name: name.to_string(),
            target_node: target_node.unwrap_or_default(),
        })
        .await?;
    println!("Network '{name}' deleted");
    Ok(())
}

pub async fn list(info: &ConnectionInfo, target_node: Option<String>) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .list_networks(proto::ListNetworksRequest {
            target_node: target_node.unwrap_or_default(),
        })
        .await?
        .into_inner();

    if resp.networks.is_empty() {
        println!("No custom networks found");
        return Ok(());
    }

    println!(
        "{:<20}  {:<16}  {:<16}  {:<15}  {:<16}",
        "NAME", "GATEWAY", "NETMASK", "EXTERNAL_IP", "NODE"
    );
    for n in &resp.networks {
        println!(
            "{:<20}  {:<16}  {:<16}  {:<15}  {:<16}",
            n.name, n.gateway_ip, n.internal_netmask, n.external_ip, n.node_id
        );
    }
    Ok(())
}
