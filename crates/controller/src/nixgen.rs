use crate::config::NetworkConfig;
use crate::db::{NetworkRow, VmRow};

#[derive(Debug, Clone)]
pub struct VxlanMeta {
    pub vni: i32,
    pub peers: Vec<String>,
    pub local_ip: String,
}

#[derive(Debug, Clone)]
pub struct SecurityGroupResolvedRule {
    pub protocol: String,
    pub host_port: i32,
    pub target_port: i32,
    pub source_cidr: String,
    pub target_ip: String,
    pub enable_dnat: bool,
}

fn netmask_to_cidr(mask: &str) -> u8 {
    mask.split('.')
        .filter_map(|o| o.parse::<u8>().ok())
        .map(|o| o.count_ones() as u8)
        .sum()
}

/// Escape a string for use inside a Nix double-quoted string literal.
/// Handles `\` → `\\`, `"` → `\"`, and `${` → `\${` (prevents interpolation).
fn nix_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'$' if bytes.get(i + 1) == Some(&b'{') => {
                out.push_str("\\${");
                i += 1;
            }
            _ => out.push(bytes[i] as char),
        }
        i += 1;
    }
    out
}

/// Strip a Nix attribute key to only safe characters (alphanumeric, dash, underscore).
fn sanitize_nix_attr_key(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

pub fn generate_node_config(
    vms: &[VmRow],
    gateway_interface: &str,
    network: &NetworkConfig,
    networks: &[NetworkRow],
    vm_ssh_keys: &std::collections::HashMap<String, Vec<String>>,
    vxlan_peers: &std::collections::HashMap<String, VxlanMeta>,
) -> String {
    generate_node_config_with_security_groups(
        vms,
        gateway_interface,
        network,
        networks,
        vm_ssh_keys,
        vxlan_peers,
        &std::collections::HashMap::new(),
    )
}

pub fn generate_node_config_with_security_groups(
    vms: &[VmRow],
    gateway_interface: &str,
    network: &NetworkConfig,
    networks: &[NetworkRow],
    vm_ssh_keys: &std::collections::HashMap<String, Vec<String>>,
    vxlan_peers: &std::collections::HashMap<String, VxlanMeta>,
    security_group_rules: &std::collections::HashMap<String, Vec<SecurityGroupResolvedRule>>,
) -> String {
    let mut out = String::from("{ pkgs, ... }: {\n");
    out.push_str("  ch-vm.vms = {\n");
    out.push_str("    enable = true;\n");
    out.push_str("    cloudHypervisorPackage = pkgs.cloud-hypervisor;\n");
    out.push_str(&format!(
        "    gatewayInterface = \"{}\";\n",
        nix_escape(gateway_interface)
    ));

    if vms.iter().any(|v| v.storage_backend == "lvm") {
        out.push_str("    lvmVgName = \"vg_kcore\";\n");
    }
    if vms.iter().any(|v| v.storage_backend == "zfs") {
        out.push_str("    zfsPoolName = \"tank0\";\n");
    }

    out.push_str("    networks.default = {\n");
    out.push_str(&format!(
        "      externalIP = \"{}\";\n",
        nix_escape(&network.external_ip)
    ));
    out.push_str(&format!(
        "      gatewayIP = \"{}\";\n",
        nix_escape(&network.gateway_ip)
    ));
    if network.internal_netmask != "255.255.255.0" {
        out.push_str(&format!(
            "      internalNetmask = \"{}\";\n",
            nix_escape(&network.internal_netmask)
        ));
    }
    out.push_str("    };\n");

    for net in networks {
        if net.name == "default" {
            continue;
        }
        out.push_str(&format!(
            "    networks.\"{}\" = {{\n",
            nix_escape(&net.name)
        ));
        out.push_str(&format!(
            "      externalIP = \"{}\";\n",
            nix_escape(&net.external_ip)
        ));
        out.push_str(&format!(
            "      gatewayIP = \"{}\";\n",
            nix_escape(&net.gateway_ip)
        ));
        if net.internal_netmask != "255.255.255.0" {
            out.push_str(&format!(
                "      internalNetmask = \"{}\";\n",
                nix_escape(&net.internal_netmask)
            ));
        }
        if !net.allowed_tcp_ports.is_empty() {
            let ports: Vec<&str> = net
                .allowed_tcp_ports
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if !ports.is_empty() {
                out.push_str(&format!(
                    "      allowedTCPPorts = [ {} ];\n",
                    ports.join(" ")
                ));
            }
        }
        if !net.allowed_udp_ports.is_empty() {
            let ports: Vec<&str> = net
                .allowed_udp_ports
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if !ports.is_empty() {
                out.push_str(&format!(
                    "      allowedUDPPorts = [ {} ];\n",
                    ports.join(" ")
                ));
            }
        }
        if net.vlan_id > 0 {
            out.push_str(&format!("      vlanId = {};\n", net.vlan_id));
        }
        if net.network_type != "nat" {
            out.push_str(&format!(
                "      networkType = \"{}\";\n",
                nix_escape(&net.network_type)
            ));
        }
        if !net.enable_outbound_nat {
            out.push_str("      enableOutboundNat = false;\n");
        }
        if let Some(vxlan) = vxlan_peers.get(&net.name) {
            out.push_str(&format!("      vni = {};\n", vxlan.vni));
            let peer_list: Vec<String> = vxlan
                .peers
                .iter()
                .map(|p| format!("\"{}\"", nix_escape(p)))
                .collect();
            out.push_str(&format!(
                "      vxlanPeers = [ {} ];\n",
                peer_list.join(" ")
            ));
            out.push_str(&format!(
                "      vxlanLocalIp = \"{}\";\n",
                nix_escape(&vxlan.local_ip)
            ));
        }
        if let Some(rules) = security_group_rules.get(&net.name) {
            if !rules.is_empty() {
                out.push_str("      securityGroupRules = [\n");
                for rule in rules {
                    out.push_str("        {\n");
                    out.push_str(&format!(
                        "          protocol = \"{}\";\n",
                        nix_escape(&rule.protocol)
                    ));
                    out.push_str(&format!("          hostPort = {};\n", rule.host_port));
                    out.push_str(&format!("          targetPort = {};\n", rule.target_port));
                    out.push_str(&format!(
                        "          sourceCidr = \"{}\";\n",
                        nix_escape(&rule.source_cidr)
                    ));
                    out.push_str(&format!(
                        "          targetIp = \"{}\";\n",
                        nix_escape(&rule.target_ip)
                    ));
                    out.push_str(&format!(
                        "          enableDnat = {};\n",
                        if rule.enable_dnat { "true" } else { "false" }
                    ));
                    out.push_str("        }\n");
                }
                out.push_str("      ];\n");
            }
        }
        out.push_str("    };\n");
    }

    for vm in vms {
        let nix_name = sanitize_nix_attr_key(&vm.name);
        out.push_str(&format!("    virtualMachines.\"{nix_name}\" = {{\n"));
        out.push_str(&format!(
            "      image = \"{}\";\n",
            nix_escape(&vm.image_path)
        ));
        out.push_str(&format!(
            "      imageFormat = \"{}\";\n",
            nix_escape(&vm.image_format)
        ));
        out.push_str(&format!(
            "      storageBackend = \"{}\";\n",
            nix_escape(&vm.storage_backend)
        ));
        out.push_str(&format!(
            "      storageSizeBytes = {};\n",
            vm.storage_size_bytes
        ));
        out.push_str(&format!("      imageSize = {};\n", vm.image_size));
        out.push_str(&format!("      cores = {};\n", vm.cpu));
        out.push_str(&format!(
            "      memorySize = {};\n",
            vm.memory_bytes / (1024 * 1024)
        ));
        out.push_str(&format!(
            "      network = \"{}\";\n",
            nix_escape(&vm.network)
        ));
        out.push_str(&format!(
            "      cloudInitInstanceId = \"{}\";\n",
            nix_escape(&vm.id)
        ));
        if !vm.vm_ip.is_empty() {
            out.push_str(&format!(
                "      dhcpReservedIPv4 = \"{}\";\n",
                nix_escape(&vm.vm_ip)
            ));
        }
        out.push_str(&format!(
            "      autoStart = {};\n",
            if vm.auto_start { "true" } else { "false" }
        ));
        let ssh_keys = vm_ssh_keys.get(&vm.id).cloned().unwrap_or_default();
        if !vm.cloud_init_user_data.is_empty() {
            let escaped = nix_escape(&vm.cloud_init_user_data);
            out.push_str(&format!(
                "      cloudInitUserConfigFile = pkgs.writeText \"{nix_name}-cloud-init.yaml\" \"{escaped}\";\n"
            ));
        } else if !ssh_keys.is_empty() {
            let mut ci = String::from("#cloud-config\n");
            ci.push_str(&format!("hostname: {}\n", nix_name));
            ci.push_str("users:\n");
            ci.push_str("  - default\n");
            ci.push_str("  - name: kcore\n");
            ci.push_str("    gecos: kcore default user\n");
            ci.push_str("    groups: [sudo]\n");
            ci.push_str("    shell: /bin/bash\n");
            ci.push_str("    lock_passwd: true\n");
            ci.push_str("    ssh_authorized_keys:\n");
            for key in &ssh_keys {
                ci.push_str(&format!(
                    "      - \"{}\"\n",
                    key.replace('"', "\\\"").replace('\\', "\\\\")
                ));
            }
            ci.push_str("ssh_pwauth: false\n");
            let escaped = nix_escape(&ci);
            out.push_str(&format!(
                "      cloudInitUserConfigFile = pkgs.writeText \"{nix_name}-cloud-init.yaml\" \"{escaped}\";\n"
            ));
        }

        if !vm.vm_ip.is_empty() {
            let vm_net = networks.iter().find(|n| n.name == vm.network);
            if let Some(net) = vm_net {
                if net.network_type == "vxlan" {
                    let cidr = netmask_to_cidr(&net.internal_netmask);
                    let mut net_cfg = String::from(
                        "version: 2\nethernets:\n  kcore0:\n    match:\n      name: \"e*\"\n    dhcp4: false\n",
                    );
                    net_cfg.push_str(&format!("    addresses: [\"{}/{}\"]\n", vm.vm_ip, cidr));
                    net_cfg.push_str(&format!("    gateway4: \"{}\"\n", net.gateway_ip));
                    net_cfg.push_str("    nameservers:\n      addresses: [1.1.1.1, 8.8.8.8]\n");
                    let escaped = nix_escape(&net_cfg);
                    out.push_str(&format!(
                        "      cloudInitNetworkConfigFile = pkgs.writeText \"{nix_name}-network-config.yaml\" \"{escaped}\";\n"
                    ));
                }
            }
        }

        out.push_str("    };\n");
    }

    out.push_str("  };\n");
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm(auto_start: bool, name: &str) -> VmRow {
        VmRow {
            id: "vm-1".into(),
            name: name.into(),
            cpu: 2,
            memory_bytes: 4096 * 1024 * 1024,
            image_path: "/var/lib/kcore/images/debian.raw".into(),
            image_url: "https://example.com/debian.raw".into(),
            image_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            image_format: "raw".into(),
            image_size: 8192,
            network: "default".into(),
            auto_start,
            node_id: "node-1".into(),
            created_at: String::new(),
            runtime_state: "unknown".into(),
            cloud_init_user_data: String::new(),
            storage_backend: "filesystem".into(),
            storage_size_bytes: 10 * 1024 * 1024 * 1024,
            vm_ip: String::new(),
        }
    }

    fn default_net() -> NetworkConfig {
        NetworkConfig {
            gateway_interface: "eno1".into(),
            external_ip: "203.0.113.10".into(),
            gateway_ip: "10.0.0.1".into(),
            internal_netmask: "255.255.255.0".into(),
        }
    }

    #[test]
    fn generates_valid_nix() {
        let config = generate_node_config(
            &[vm(true, "web-01")],
            "eno1",
            &default_net(),
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(config.contains("ch-vm.vms"));
        assert!(config.contains("web-01"));
        assert!(config.contains("cores = 2"));
        assert!(config.contains("memorySize = 4096"));
        assert!(config.contains("gatewayInterface = \"eno1\""));
        assert!(config.contains("image = \"/var/lib/kcore/images/debian.raw\""));
        assert!(config.contains("imageFormat = \"raw\""));
        assert!(config.contains("storageBackend = \"filesystem\""));
        assert!(config.contains("storageSizeBytes = 10737418240"));
        assert!(config.contains("cloudInitInstanceId = \"vm-1\";"));
    }

    #[test]
    fn includes_non_default_netmask_and_stopped_state() {
        let net = NetworkConfig {
            internal_netmask: "255.255.255.128".into(),
            ..default_net()
        };
        let config = generate_node_config(
            &[vm(false, "web-01")],
            "eno1",
            &net,
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(config.contains("internalNetmask = \"255.255.255.128\""));
        assert!(config.contains("autoStart = false;"));
    }

    #[test]
    fn sanitizes_vm_name_for_nix_attr_key() {
        let config = generate_node_config(
            &[vm(true, "db node 01")],
            "eno1",
            &default_net(),
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(config.contains("virtualMachines.\"db-node-01\""));
    }

    #[test]
    fn sanitizes_special_chars_in_vm_name() {
        let config = generate_node_config(
            &[vm(true, "web\";inject")],
            "eno1",
            &default_net(),
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(config.contains("virtualMachines.\"web--inject\""));
        assert!(!config.contains("\";inject"));
    }

    #[test]
    fn nix_escape_handles_quotes_and_backslashes() {
        assert_eq!(nix_escape(r#"a"b"#), r#"a\"b"#);
        assert_eq!(nix_escape(r"a\b"), r"a\\b");
        assert_eq!(nix_escape("a${b}"), "a\\${b}");
    }

    #[test]
    fn image_path_with_special_chars_is_escaped() {
        let mut v = vm(true, "evil");
        v.image_path = r#"/images/foo"${bar}.raw"#.into();
        let config = generate_node_config(
            &[v],
            "eno1",
            &default_net(),
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(config.contains(r#"image = "/images/foo\"\${bar}.raw";"#));
        // The raw `${` is escaped to `\${`, preventing Nix interpolation.
        assert!(!config.contains("image = \"/images/foo\"${bar}.raw\";"));
    }

    #[test]
    fn image_format_is_rendered_for_qcow2() {
        let mut v = vm(true, "qcow");
        v.image_format = "qcow2".into();
        let config = generate_node_config(
            &[v],
            "eno1",
            &default_net(),
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(config.contains("imageFormat = \"qcow2\";"));
    }

    #[test]
    fn network_values_are_escaped() {
        let net = NetworkConfig {
            gateway_interface: "eno1\"".into(),
            external_ip: "1.2.3.4\"".into(),
            gateway_ip: "10.0.0.1\\".into(),
            internal_netmask: "255.255.255.0".into(),
        };
        let config = generate_node_config(
            &[],
            "eno1\"",
            &net,
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(config.contains(r#"gatewayInterface = "eno1\"";"#));
        assert!(config.contains(r#"externalIP = "1.2.3.4\"";"#));
        assert!(config.contains(r#"gatewayIP = "10.0.0.1\\";"#));
    }

    #[test]
    fn renders_custom_networks() {
        let networks = vec![NetworkRow {
            name: "frontend".into(),
            external_ip: "198.51.100.5".into(),
            gateway_ip: "10.240.10.1".into(),
            internal_netmask: "255.255.255.0".into(),
            node_id: "node-1".into(),
            allowed_tcp_ports: String::new(),
            allowed_udp_ports: String::new(),
            vlan_id: 0,
            network_type: "nat".into(),
            enable_outbound_nat: true,
            vni: 0,
            next_ip: 2,
        }];
        let config = generate_node_config(
            &[],
            "eno1",
            &default_net(),
            &networks,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(config.contains("networks.\"frontend\""));
        assert!(config.contains("gatewayIP = \"10.240.10.1\";"));
    }

    #[test]
    fn renders_network_port_forwarding() {
        let networks = vec![NetworkRow {
            name: "frontend".into(),
            external_ip: "198.51.100.5".into(),
            gateway_ip: "10.240.10.1".into(),
            internal_netmask: "255.255.255.0".into(),
            node_id: "node-1".into(),
            allowed_tcp_ports: "80,443,8080".into(),
            allowed_udp_ports: "53".into(),
            vlan_id: 0,
            network_type: "nat".into(),
            enable_outbound_nat: true,
            vni: 0,
            next_ip: 2,
        }];
        let config = generate_node_config(
            &[],
            "eno1",
            &default_net(),
            &networks,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(config.contains("allowedTCPPorts = [ 80 443 8080 ];"));
        assert!(config.contains("allowedUDPPorts = [ 53 ];"));
    }

    #[test]
    fn renders_network_with_vlan_id() {
        let v = vm(true, "web-01");
        let net = NetworkRow {
            name: "production".to_string(),
            external_ip: "203.0.113.10".to_string(),
            gateway_ip: "10.100.0.1".to_string(),
            internal_netmask: "255.255.255.0".to_string(),
            node_id: "node-1".to_string(),
            allowed_tcp_ports: String::new(),
            allowed_udp_ports: String::new(),
            vlan_id: 100,
            network_type: "nat".into(),
            enable_outbound_nat: true,
            vni: 0,
            next_ip: 2,
        };
        let config = generate_node_config(
            &[v],
            "eno1",
            &default_net(),
            &[net],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(config.contains("vlanId = 100"), "should contain vlanId");
        assert!(config.contains("production"), "should contain network name");
    }

    #[test]
    fn injects_ssh_keys_into_cloud_init() {
        let v = vm(true, "web-01");
        let mut keys = std::collections::HashMap::new();
        keys.insert(
            "vm-1".to_string(),
            vec!["ssh-rsa AAAAB3... user@host".to_string()],
        );
        let config = generate_node_config(
            &[v],
            "eno1",
            &default_net(),
            &[],
            &keys,
            &std::collections::HashMap::new(),
        );
        assert!(config.contains("cloudInitUserConfigFile"));
        assert!(config.contains("ssh_authorized_keys"));
        assert!(config.contains("lock_passwd: true"));
        assert!(config.contains("ssh_pwauth: false"));
    }

    #[test]
    fn renders_bridge_network_without_masquerade() {
        let networks = vec![NetworkRow {
            name: "bridged".into(),
            external_ip: "0.0.0.0".into(),
            gateway_ip: "0.0.0.0".into(),
            internal_netmask: "255.255.255.0".into(),
            node_id: "node-1".into(),
            allowed_tcp_ports: String::new(),
            allowed_udp_ports: String::new(),
            vlan_id: 0,
            network_type: "bridge".into(),
            enable_outbound_nat: false,
            vni: 0,
            next_ip: 2,
        }];
        let config = generate_node_config(
            &[],
            "eno1",
            &default_net(),
            &networks,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(config.contains("networkType = \"bridge\""));
        assert!(config.contains("enableOutboundNat = false"));
        assert!(!config.contains("vni ="));
    }

    #[test]
    fn renders_vxlan_network_with_peers() {
        let networks = vec![NetworkRow {
            name: "overlay".into(),
            external_ip: "0.0.0.0".into(),
            gateway_ip: "10.200.0.1".into(),
            internal_netmask: "255.255.255.0".into(),
            node_id: "node-1".into(),
            allowed_tcp_ports: String::new(),
            allowed_udp_ports: String::new(),
            vlan_id: 0,
            network_type: "vxlan".into(),
            enable_outbound_nat: true,
            vni: 10042,
            next_ip: 2,
        }];
        let mut vxlan = std::collections::HashMap::new();
        vxlan.insert(
            "overlay".to_string(),
            VxlanMeta {
                vni: 10042,
                peers: vec!["192.168.1.20".into(), "192.168.1.30".into()],
                local_ip: "192.168.1.10".into(),
            },
        );
        let config = generate_node_config(
            &[],
            "eno1",
            &default_net(),
            &networks,
            &std::collections::HashMap::new(),
            &vxlan,
        );
        assert!(config.contains("networkType = \"vxlan\""));
        assert!(config.contains("vni = 10042"));
        assert!(config.contains("\"192.168.1.20\""));
        assert!(config.contains("\"192.168.1.30\""));
        assert!(config.contains("vxlanLocalIp = \"192.168.1.10\""));
    }

    #[test]
    fn renders_vxlan_vm_with_static_ip() {
        let mut v = vm(true, "vxvm");
        v.network = "overlay".into();
        v.vm_ip = "10.200.0.5".into();

        let networks = vec![NetworkRow {
            name: "overlay".into(),
            external_ip: "0.0.0.0".into(),
            gateway_ip: "10.200.0.1".into(),
            internal_netmask: "255.255.255.0".into(),
            node_id: "node-1".into(),
            allowed_tcp_ports: String::new(),
            allowed_udp_ports: String::new(),
            vlan_id: 0,
            network_type: "vxlan".into(),
            enable_outbound_nat: true,
            vni: 10042,
            next_ip: 6,
        }];
        let mut vxlan = std::collections::HashMap::new();
        vxlan.insert(
            "overlay".to_string(),
            VxlanMeta {
                vni: 10042,
                peers: vec![],
                local_ip: "192.168.1.10".into(),
            },
        );
        let config = generate_node_config(
            &[v],
            "eno1",
            &default_net(),
            &networks,
            &std::collections::HashMap::new(),
            &vxlan,
        );
        assert!(
            config.contains("cloudInitNetworkConfigFile"),
            "should have static network config"
        );
        assert!(config.contains("name: \\\"e*\\\""));
        assert!(config.contains("dhcp4: false"));
        assert!(config.contains("10.200.0.5/24"));
        assert!(config.contains("gateway4"));
    }

    #[test]
    fn emits_lvm_vg_name_when_vm_uses_lvm() {
        let mut v = vm(true, "lvm-vm");
        v.storage_backend = "lvm".into();
        let config = generate_node_config(
            &[v],
            "eno1",
            &default_net(),
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(
            config.contains("lvmVgName = \"vg_kcore\";"),
            "should contain lvmVgName"
        );
    }

    #[test]
    fn omits_lvm_vg_name_for_filesystem_backend() {
        let config = generate_node_config(
            &[vm(true, "fs-vm")],
            "eno1",
            &default_net(),
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(
            !config.contains("lvmVgName"),
            "should not contain lvmVgName"
        );
    }

    #[test]
    fn emits_zfs_pool_name_when_vm_uses_zfs() {
        let mut v = vm(true, "zfs-vm");
        v.storage_backend = "zfs".into();
        let config = generate_node_config(
            &[v],
            "eno1",
            &default_net(),
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(
            config.contains("zfsPoolName = \"tank0\";"),
            "should contain zfsPoolName"
        );
        assert!(
            !config.contains("lvmVgName"),
            "ZFS VM should not contain lvmVgName"
        );
    }

    #[test]
    fn omits_zfs_pool_name_for_filesystem_backend() {
        let config = generate_node_config(
            &[vm(true, "fs-vm")],
            "eno1",
            &default_net(),
            &[],
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        );
        assert!(
            !config.contains("zfsPoolName"),
            "should not contain zfsPoolName"
        );
    }

    #[test]
    fn netmask_to_cidr_converts_common_masks() {
        assert_eq!(netmask_to_cidr("255.255.255.0"), 24);
        assert_eq!(netmask_to_cidr("255.255.0.0"), 16);
        assert_eq!(netmask_to_cidr("255.255.255.128"), 25);
    }

    #[test]
    fn renders_security_group_rules_for_network() {
        let networks = vec![NetworkRow {
            name: "private".into(),
            external_ip: "198.51.100.5".into(),
            gateway_ip: "10.240.10.1".into(),
            internal_netmask: "255.255.255.0".into(),
            node_id: "node-1".into(),
            allowed_tcp_ports: String::new(),
            allowed_udp_ports: String::new(),
            vlan_id: 0,
            network_type: "nat".into(),
            enable_outbound_nat: true,
            vni: 0,
            next_ip: 2,
        }];
        let mut sg_rules = std::collections::HashMap::new();
        sg_rules.insert(
            "private".to_string(),
            vec![SecurityGroupResolvedRule {
                protocol: "tcp".to_string(),
                host_port: 8443,
                target_port: 443,
                source_cidr: "0.0.0.0/0".to_string(),
                target_ip: "10.240.10.22".to_string(),
                enable_dnat: true,
            }],
        );
        let config = generate_node_config_with_security_groups(
            &[],
            "eno1",
            &default_net(),
            &networks,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
            &sg_rules,
        );
        assert!(config.contains("securityGroupRules = ["));
        assert!(config.contains("hostPort = 8443;"));
        assert!(config.contains("targetIp = \"10.240.10.22\";"));
        assert!(config.contains("enableDnat = true;"));
    }
}

/// Property-based tests (Phase 2) — Nix string escaping and sanitization.
///
/// `nix_escape` is the trust boundary for embedding controller-stored
/// strings (VM names, descriptions, etc.) into rendered Nix
/// configuration. A bug here could let a malicious VM name execute
/// arbitrary Nix code at evaluation time, so we want strong
/// post-condition guarantees against ALL inputs, not just the handful
/// covered by example tests.
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Returns true iff `s`, when wrapped in `"…"`, is safe to embed
    /// in a Nix double-quoted string literal: no unescaped `"`, no
    /// unescaped `\`, and no unescaped `${` interpolation marker.
    fn is_safely_escaped(s: &str) -> bool {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                // A backslash MUST be followed by another byte (escape pair).
                b'\\' => {
                    if i + 1 >= bytes.len() {
                        return false;
                    }
                    i += 2;
                    continue;
                }
                // A bare `"` is never allowed.
                b'"' => return false,
                // A bare `${` is never allowed.
                b'$' if bytes.get(i + 1) == Some(&b'{') => return false,
                _ => i += 1,
            }
        }
        true
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2_000,
            .. ProptestConfig::default()
        })]

        /// `nix_escape` never panics for any UTF-8 input.
        #[test]
        fn nix_escape_never_panics(s in ".{0,128}") {
            let _ = nix_escape(&s);
        }

        /// **Soundness**: the output of `nix_escape` is always safe to
        /// embed inside a Nix double-quoted string literal. This is the
        /// security-critical post-condition every caller relies on.
        #[test]
        fn nix_escape_output_is_always_safe(s in ".{0,128}") {
            let escaped = nix_escape(&s);
            prop_assert!(
                is_safely_escaped(&escaped),
                "nix_escape({s:?}) = {escaped:?} is not safely escaped"
            );
        }

        /// **Idempotence on safe input**: a string that contains no
        /// `\`, `"`, or `${` is left unchanged.
        #[test]
        fn nix_escape_is_identity_on_safe_input(s in "[a-zA-Z0-9 _\\-./]{0,32}") {
            prop_assert_eq!(nix_escape(&s), s);
        }

        /// `nix_escape` is **idempotent**: escaping an already-escaped
        /// string and then escaping again is the same as escaping the
        /// already-escaped string only once. (Necessary for any future
        /// pipeline that may re-process strings.)
        ///
        /// Note: this property is intentionally weaker than
        /// `nix_escape(nix_escape(s)) == nix_escape(s)`, which does NOT
        /// hold (because escaping `\` produces `\\`, which itself
        /// contains `\`). The realistic invariant is that two passes
        /// are equivalent to one pass on the once-escaped output —
        /// which is trivially true. We assert the safe-output post-
        /// condition recursively instead.
        #[test]
        fn nix_escape_repeated_application_stays_safe(s in ".{0,64}") {
            let once = nix_escape(&s);
            let twice = nix_escape(&once);
            prop_assert!(is_safely_escaped(&once));
            prop_assert!(is_safely_escaped(&twice));
        }

        /// `sanitize_nix_attr_key` always returns a string of the same
        /// `chars().count()` whose every character is in
        /// `[A-Za-z0-9_-]`.
        #[test]
        fn sanitize_nix_attr_key_charset(s in ".{0,32}") {
            let out = sanitize_nix_attr_key(&s);
            prop_assert_eq!(out.chars().count(), s.chars().count());
            for c in out.chars() {
                prop_assert!(
                    c.is_ascii_alphanumeric() || c == '-' || c == '_',
                    "sanitize_nix_attr_key produced unsafe char {c:?}"
                );
            }
        }

        /// `netmask_to_cidr` for any well-formed IPv4 dotted netmask
        /// returns the popcount of the underlying 32-bit value, so it
        /// is always in `[0, 32]`.
        #[test]
        fn netmask_to_cidr_is_in_range(
            a in 0u8..=255, b in 0u8..=255, c in 0u8..=255, d in 0u8..=255,
        ) {
            let s = format!("{a}.{b}.{c}.{d}");
            let n = netmask_to_cidr(&s);
            prop_assert!(n <= 32, "netmask_to_cidr({s:?}) = {n} > 32");
            // Must also equal sum of octet popcounts.
            let expected = (a.count_ones() + b.count_ones() + c.count_ones() + d.count_ones()) as u8;
            prop_assert_eq!(n, expected);
        }

        /// `netmask_to_cidr` on garbage input never panics; it just
        /// drops un-parseable octets and returns the popcount of the
        /// remainder.
        #[test]
        fn netmask_to_cidr_never_panics_on_garbage(s in ".{0,32}") {
            let _ = netmask_to_cidr(&s);
        }
    }
}
