use crate::config::NetworkConfig;
use crate::db::VmRow;

pub fn generate_node_config(
    vms: &[VmRow],
    gateway_interface: &str,
    network: &NetworkConfig,
) -> String {
    let mut out = String::from("{ pkgs, ... }: {\n");
    out.push_str("  ctrl-os.vms = {\n");
    out.push_str("    enable = true;\n");
    out.push_str("    cloudHypervisorPackage = pkgs.cloud-hypervisor;\n");
    out.push_str(&format!(
        "    gatewayInterface = \"{gateway_interface}\";\n"
    ));

    out.push_str("    networks.default = {\n");
    out.push_str(&format!(
        "      externalIP = \"{}\";\n",
        network.external_ip
    ));
    out.push_str(&format!("      gatewayIP = \"{}\";\n", network.gateway_ip));
    if network.internal_netmask != "255.255.255.0" {
        out.push_str(&format!(
            "      internalNetmask = \"{}\";\n",
            network.internal_netmask
        ));
    }
    out.push_str("    };\n");

    for vm in vms {
        let nix_name = vm.name.replace(' ', "-");
        out.push_str(&format!("    virtualMachines.\"{nix_name}\" = {{\n"));
        out.push_str(&format!("      image = {};\n", vm.image_path));
        out.push_str(&format!("      imageSize = {};\n", vm.image_size));
        out.push_str(&format!("      cores = {};\n", vm.cpu));
        out.push_str(&format!(
            "      memorySize = {};\n",
            vm.memory_bytes / (1024 * 1024)
        ));
        out.push_str(&format!("      network = \"{}\";\n", vm.network));
        out.push_str(&format!(
            "      autoStart = {};\n",
            if vm.auto_start { "true" } else { "false" }
        ));
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
            image_size: 8192,
            network: "default".into(),
            auto_start,
            node_id: "node-1".into(),
            created_at: String::new(),
        }
    }

    #[test]
    fn generates_valid_nix() {
        let net = NetworkConfig {
            gateway_interface: "eno1".into(),
            external_ip: "203.0.113.10".into(),
            gateway_ip: "10.0.0.1".into(),
            internal_netmask: "255.255.255.0".into(),
        };
        let vms = vec![vm(true, "web-01")];

        let config = generate_node_config(&vms, "eno1", &net);
        assert!(config.contains("ctrl-os.vms"));
        assert!(config.contains("web-01"));
        assert!(config.contains("cores = 2"));
        assert!(config.contains("memorySize = 4096"));
        assert!(config.contains("gatewayInterface = \"eno1\""));
    }

    #[test]
    fn includes_non_default_netmask_and_stopped_state() {
        let net = NetworkConfig {
            gateway_interface: "eno1".into(),
            external_ip: "203.0.113.10".into(),
            gateway_ip: "10.0.0.1".into(),
            internal_netmask: "255.255.255.128".into(),
        };
        let config = generate_node_config(&[vm(false, "web-01")], "eno1", &net);
        assert!(config.contains("internalNetmask = \"255.255.255.128\""));
        assert!(config.contains("autoStart = false;"));
    }

    #[test]
    fn sanitizes_vm_name_spaces_for_nix_attr_key() {
        let net = NetworkConfig {
            gateway_interface: "eno1".into(),
            external_ip: "203.0.113.10".into(),
            gateway_ip: "10.0.0.1".into(),
            internal_netmask: "255.255.255.0".into(),
        };
        let config = generate_node_config(&[vm(true, "db node 01")], "eno1", &net);
        assert!(config.contains("virtualMachines.\"db-node-01\""));
    }
}
