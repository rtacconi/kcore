use anyhow::Result;

use crate::client::{self, controller_proto as proto};
use crate::config::ConnectionInfo;

pub struct CreateArgs {
    pub name: String,
    pub external_ip: String,
    pub gateway_ip: String,
    pub internal_netmask: String,
    pub target_node: Option<String>,
    pub vlan_id: i32,
    pub network_type: String,
    pub enable_outbound_nat: bool,
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
            allowed_tcp_ports: vec![],
            allowed_udp_ports: vec![],
            vlan_id: args.vlan_id,
            network_type: args.network_type,
            enable_outbound_nat: args.enable_outbound_nat,
        })
        .await?
        .into_inner();

    if resp.success {
        let label = format!("Network '{}' on node {}", args.name, resp.node_id);
        println!(
            "{}",
            crate::apply_summary::render_apply_summary(resp.action, &resp.changed_fields, &label)
        );
        if !resp.message.is_empty() {
            println!("  Info: {}", resp.message);
        }
    } else {
        println!("Network '{}' creation rejected", args.name);
    }
    Ok(())
}

pub async fn create_from_manifest(info: &ConnectionInfo, path: &str) -> Result<()> {
    let data = std::fs::read_to_string(path)?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&data)?;

    let kind = doc["kind"].as_str().unwrap_or("");
    if !kind.eq_ignore_ascii_case("Network") {
        anyhow::bail!("expected kind=Network, got {kind}");
    }

    let name = doc["metadata"]["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("metadata.name is required"))?
        .to_string();

    let spec = &doc["spec"];
    let network_type = spec["type"]
        .as_str()
        .or_else(|| spec["networkType"].as_str())
        .unwrap_or("nat")
        .to_string();
    let external_ip = spec["externalIp"]
        .as_str()
        .or_else(|| spec["external_ip"].as_str())
        .unwrap_or("")
        .to_string();
    let gateway_ip = spec["gatewayIp"]
        .as_str()
        .or_else(|| spec["gateway_ip"].as_str())
        .unwrap_or("")
        .to_string();
    let internal_netmask = spec["internalNetmask"]
        .as_str()
        .or_else(|| spec["internal_netmask"].as_str())
        .or_else(|| spec["netmask"].as_str())
        .unwrap_or("255.255.255.0")
        .to_string();
    let target_node = spec["targetNode"]
        .as_str()
        .or_else(|| spec["target_node"].as_str())
        .unwrap_or("")
        .to_string();
    let vlan_id = spec["vlanId"]
        .as_i64()
        .or_else(|| spec["vlan_id"].as_i64())
        .unwrap_or(0) as i32;
    let enable_outbound_nat = spec["enableOutboundNat"]
        .as_bool()
        .or_else(|| spec["enable_outbound_nat"].as_bool())
        .unwrap_or(network_type != "bridge");
    let allowed_tcp_ports: Vec<i32> = spec["allowedTcpPorts"]
        .as_sequence()
        .or_else(|| spec["allowed_tcp_ports"].as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_i64().map(|p| p as i32))
                .collect()
        })
        .unwrap_or_default();
    let allowed_udp_ports: Vec<i32> = spec["allowedUdpPorts"]
        .as_sequence()
        .or_else(|| spec["allowed_udp_ports"].as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_i64().map(|p| p as i32))
                .collect()
        })
        .unwrap_or_default();

    let mut client = client::controller_client(info).await?;
    let resp = client
        .create_network(proto::CreateNetworkRequest {
            name: name.clone(),
            external_ip,
            gateway_ip,
            internal_netmask,
            target_node,
            allowed_tcp_ports,
            allowed_udp_ports,
            vlan_id,
            network_type,
            enable_outbound_nat,
        })
        .await?
        .into_inner();

    if resp.success {
        let label = format!("Network '{name}' on node {}", resp.node_id);
        println!(
            "{}",
            crate::apply_summary::render_apply_summary(resp.action, &resp.changed_fields, &label)
        );
    } else {
        println!("Network '{name}' creation rejected");
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

    struct NetworkSummary {
        name: String,
        net_type: String,
        gateway_ip: String,
        internal_netmask: String,
        vlan_id: i32,
        node_count: usize,
    }

    let mut grouped: Vec<NetworkSummary> = Vec::new();
    for n in &resp.networks {
        let net_type = if n.network_type.is_empty() {
            "nat".to_string()
        } else {
            n.network_type.clone()
        };
        if let Some(existing) = grouped.iter_mut().find(|g| g.name == n.name) {
            existing.node_count += 1;
        } else {
            grouped.push(NetworkSummary {
                name: n.name.clone(),
                net_type,
                gateway_ip: n.gateway_ip.clone(),
                internal_netmask: n.internal_netmask.clone(),
                vlan_id: n.vlan_id,
                node_count: 1,
            });
        }
    }

    println!(
        "{:<20}  {:<7}  {:<16}  {:<16}  {:>4}  {:<8}  {:<16}  {:<8}",
        "NAME", "TYPE", "GATEWAY", "NETMASK", "VLAN", "OVERLAY", "BRIDGE", "NODES"
    );
    for s in &grouped {
        let vlan = if s.vlan_id > 0 {
            s.vlan_id.to_string()
        } else {
            "-".to_string()
        };
        let overlay = if s.net_type == "vxlan" { "yes" } else { "no" };
        let bridge = compute_bridge_name(&s.name);
        println!(
            "{:<20}  {:<7}  {:<16}  {:<16}  {:>4}  {:<8}  {:<16}  {:<8}",
            s.name,
            s.net_type,
            s.gateway_ip,
            s.internal_netmask,
            vlan,
            overlay,
            bridge,
            s.node_count
        );
    }
    Ok(())
}

pub async fn describe(
    info: &ConnectionInfo,
    name: &str,
    _target_node: Option<String>,
) -> Result<()> {
    let mut client = client::controller_client(info).await?;
    let resp = client
        .list_networks(proto::ListNetworksRequest {
            target_node: String::new(),
        })
        .await?
        .into_inner();

    let matches: Vec<_> = resp
        .networks
        .into_iter()
        .filter(|n| n.name == name)
        .collect();
    if matches.is_empty() {
        anyhow::bail!("network '{name}' not found");
    }

    let first = &matches[0];
    let net_type = if first.network_type.is_empty() {
        "nat".to_string()
    } else {
        first.network_type.clone()
    };
    let is_overlay = net_type == "vxlan";
    let vlan = if first.vlan_id > 0 {
        first.vlan_id.to_string()
    } else {
        "-".to_string()
    };

    println!("Name:              {}", first.name);
    println!("Type:              {net_type}");
    println!(
        "Overlay:           {}",
        if is_overlay { "yes" } else { "no" }
    );
    println!("Bridge:            {}", compute_bridge_name(name));
    println!("Gateway IP:        {}", first.gateway_ip);
    println!("Internal netmask:  {}", first.internal_netmask);
    if let Some(cidr) = ipv4_subnet_from_gateway_mask(&first.gateway_ip, &first.internal_netmask) {
        println!("Network CIDR:      {cidr}");
    }
    println!("VLAN:              {vlan}");
    println!(
        "Outbound NAT:      {}",
        if first.enable_outbound_nat {
            "enabled"
        } else {
            "disabled"
        }
    );
    let tcp_ports = &first.allowed_tcp_ports;
    let udp_ports = &first.allowed_udp_ports;
    println!(
        "Allowed TCP ports: {}",
        if tcp_ports.is_empty() {
            "(none)".to_string()
        } else {
            tcp_ports
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    println!(
        "Allowed UDP ports: {}",
        if udp_ports.is_empty() {
            "(none)".to_string()
        } else {
            udp_ports
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );

    println!("Nodes:             {} participating", matches.len());
    for m in &matches {
        if is_overlay {
            println!("  - {}  (vtep: {})", m.node_id, m.external_ip);
        } else {
            println!("  - {}  (external: {})", m.node_id, m.external_ip);
        }
    }
    Ok(())
}

fn compute_bridge_name(network_name: &str) -> String {
    let full = format!("kbr-{network_name}");
    if full.len() <= 15 {
        return full;
    }
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    network_name.hash(&mut hasher);
    let hash = format!("{:016x}", hasher.finish());
    format!("kb-{}", &hash[..8])
}

fn ipv4_subnet_from_gateway_mask(gateway_ip: &str, netmask: &str) -> Option<String> {
    fn parse_ipv4(ip: &str) -> Option<[u8; 4]> {
        let mut parts = ip.split('.');
        let a = parts.next()?.parse::<u8>().ok()?;
        let b = parts.next()?.parse::<u8>().ok()?;
        let c = parts.next()?.parse::<u8>().ok()?;
        let d = parts.next()?.parse::<u8>().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some([a, b, c, d])
    }

    let ip = parse_ipv4(gateway_ip)?;
    let mask = parse_ipv4(netmask)?;
    let network = [
        ip[0] & mask[0],
        ip[1] & mask[1],
        ip[2] & mask[2],
        ip[3] & mask[3],
    ];
    let prefix = mask.iter().map(|b| b.count_ones()).sum::<u32>();
    Some(format!(
        "{}.{}.{}.{}/{}",
        network[0], network[1], network[2], network[3], prefix
    ))
}

/// Property-based tests (Phase 2) — local network helpers.
#[cfg(test)]
mod proptests {
    use super::{compute_bridge_name, ipv4_subnet_from_gateway_mask};
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// `compute_bridge_name` always produces a name <= 15 bytes
        /// (the Linux IFNAMSIZ - 1 limit) and is **deterministic**
        /// for the same input.
        #[test]
        fn compute_bridge_name_is_short_and_deterministic(name in ".{0,64}") {
            let a = compute_bridge_name(&name);
            let b = compute_bridge_name(&name);
            prop_assert_eq!(&a, &b);
            prop_assert!(a.len() <= 15, "bridge name {a:?} ({} bytes) > 15", a.len());
        }

        /// Short names retain the `kbr-` prefix; long names use the
        /// hashed `kb-` prefix.
        #[test]
        fn compute_bridge_name_uses_correct_prefix(name in "[a-z0-9]{0,32}") {
            let bridge = compute_bridge_name(&name);
            let full = format!("kbr-{name}");
            if full.len() <= 15 {
                prop_assert_eq!(bridge, full);
            } else {
                prop_assert!(bridge.starts_with("kb-"));
                prop_assert_eq!(bridge.len(), 11);
            }
        }

        /// `ipv4_subnet_from_gateway_mask` returns `None` for
        /// non-parseable inputs and never panics.
        #[test]
        fn ipv4_subnet_never_panics(g in ".{0,32}", m in ".{0,32}") {
            let _ = ipv4_subnet_from_gateway_mask(&g, &m);
        }

        /// **Soundness**: when both are valid IPv4 quads, the prefix
        /// length is in `[0, 32]` and the network address equals
        /// `gateway AND mask` byte-wise.
        #[test]
        fn ipv4_subnet_prefix_in_range_and_network_correct(
            ga in 0u8..=255, gb in 0u8..=255, gc in 0u8..=255, gd in 0u8..=255,
            ma in 0u8..=255, mb in 0u8..=255, mc in 0u8..=255, md in 0u8..=255,
        ) {
            let g = format!("{ga}.{gb}.{gc}.{gd}");
            let m = format!("{ma}.{mb}.{mc}.{md}");
            let result = ipv4_subnet_from_gateway_mask(&g, &m).expect("valid quads parse");
            // result is "<a>.<b>.<c>.<d>/<p>"
            let (net, prefix) = result.split_once('/').expect("has prefix");
            let p: u8 = prefix.parse().expect("prefix is integer");
            prop_assert!(p <= 32, "prefix {p} > 32");
            let expected_net = format!("{}.{}.{}.{}", ga & ma, gb & mb, gc & mc, gd & md);
            prop_assert_eq!(net, expected_net);
        }
    }
}
