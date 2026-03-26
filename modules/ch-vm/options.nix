{lib, ...}: let
  networkSubmodule = lib.types.submodule {
    options = {
      externalIP = lib.mkOption {
        type = lib.types.str;
        description = "External IP address for this network (used in NAT).";
      };

      gatewayIP = lib.mkOption {
        type = lib.types.str;
        description = "Gateway IP for the internal bridge (host-side address).";
      };

      internalNetmask = lib.mkOption {
        type = lib.types.str;
        default = "255.255.255.0";
        description = "Netmask for the internal bridge network.";
      };

      allowedTCPPorts = lib.mkOption {
        type = lib.types.listOf lib.types.port;
        default = [];
        description = "TCP ports to forward from external IP to VMs.";
      };

      allowedUDPPorts = lib.mkOption {
        type = lib.types.listOf lib.types.port;
        default = [];
        description = "UDP ports to forward from external IP to VMs.";
      };

      vlanId = lib.mkOption {
        type = lib.types.int;
        default = 0;
        description = "802.1Q VLAN tag. When > 0, a VLAN sub-interface is created on gatewayInterface and the bridge is placed on top of it instead of the physical NIC.";
      };

      networkType = lib.mkOption {
        type = lib.types.enum ["nat" "bridge" "vxlan"];
        default = "nat";
        description = "Network mode: nat (default, masquerade+DNAT), bridge (passthrough to physical NIC), or vxlan (overlay).";
      };

      vni = lib.mkOption {
        type = lib.types.int;
        default = 0;
        description = "VXLAN Network Identifier (used when networkType = vxlan).";
      };

      vxlanPeers = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [];
        description = "IP addresses of peer hosts for VXLAN FDB flooding.";
      };

      vxlanLocalIp = lib.mkOption {
        type = lib.types.str;
        default = "";
        description = "This node's IP address used as VXLAN tunnel source.";
      };

      enableOutboundNat = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to add masquerade rules for outbound internet access. Set to false for fully isolated overlays.";
      };
    };
  };

  vmSubmodule = lib.types.submodule {
    options = {
      image = lib.mkOption {
        type = lib.types.path;
        description = "Path to the VM disk image.";
      };

      imageFormat = lib.mkOption {
        type = lib.types.enum ["raw" "qcow2"];
        default = "raw";
        description = "Disk image format passed to Cloud Hypervisor (image_type).";
      };

      imageSize = lib.mkOption {
        type = lib.types.int;
        default = 8192;
        description = "Disk image size in MiB (used when auto-creating).";
      };

      storageBackend = lib.mkOption {
        type = lib.types.enum ["filesystem" "lvm" "zfs"];
        default = "filesystem";
        description = "Storage backend requested for VM data volume provisioning.";
      };

      storageSizeBytes = lib.mkOption {
        type = lib.types.ints.positive;
        default = 10737418240;
        description = "Requested VM storage size in bytes for backend provisioning metadata.";
      };

      cores = lib.mkOption {
        type = lib.types.ints.positive;
        default = 2;
        description = "Number of vCPUs.";
      };

      memorySize = lib.mkOption {
        type = lib.types.ints.positive;
        default = 4096;
        description = "Memory size in MiB.";
      };

      network = lib.mkOption {
        type = lib.types.str;
        default = "default";
        description = "Name of the network (must match a key in ch-vm.vms.networks).";
      };

      cloudInitUserConfigFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Path to cloud-init user-data YAML file.";
      };

      cloudInitNetworkConfigFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "Path to cloud-init network-config YAML file.";
      };

      autoStart = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Whether to start this VM automatically on boot.";
      };

      macAddress = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "MAC address for the VM NIC. Auto-generated if null.";
      };

      extraArgs = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [];
        description = "Extra command-line arguments passed to cloud-hypervisor.";
      };
    };
  };
in {
  options.ch-vm.vms = {
    enable = lib.mkEnableOption "kcore declarative VM management";

    cloudHypervisorPackage = lib.mkOption {
      type = lib.types.package;
      description = "Cloud Hypervisor package to use.";
    };

    socketDir = lib.mkOption {
      type = lib.types.str;
      default = "/run/kcore";
      description = "Directory for Cloud Hypervisor API sockets.";
    };

    firmwarePath = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Optional firmware path passed to Cloud Hypervisor. Defaults to OVMF-cloud-hypervisor firmware from nixpkgs.";
    };

    gatewayInterface = lib.mkOption {
      type = lib.types.str;
      description = "Host network interface used as the upstream gateway.";
      example = "eno1";
    };

    networks = lib.mkOption {
      type = lib.types.attrsOf networkSubmodule;
      default = {};
      description = "Named networks, each backed by a bridge with NAT.";
    };

    virtualMachines = lib.mkOption {
      type = lib.types.attrsOf vmSubmodule;
      default = {};
      description = "Named virtual machines managed by Cloud Hypervisor.";
    };
  };
}
