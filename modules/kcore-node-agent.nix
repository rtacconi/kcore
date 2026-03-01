{ config, lib, pkgs, ... }:

let
  # Path to node agent binary (will be provided externally or built)
  nodeAgentPath = "/opt/kcore/kcore-node-agent";
  
  # Node agent configuration
  nodeAgentCfg = config.services.kcore.nodeAgent;
  
  # Generate node agent YAML config
  nodeAgentConfig = pkgs.writeText "node-agent.yaml" ''
    nodeId: ${nodeAgentCfg.nodeId}
    ${lib.optionalString (nodeAgentCfg.listenAddr != null) ''
    listenAddr: "${nodeAgentCfg.listenAddr}"
    ''}
    ${lib.optionalString (nodeAgentCfg.controllerAddr != null) ''
    controllerAddr: "${nodeAgentCfg.controllerAddr}"
    ''}
    tls:
      caFile: ${nodeAgentCfg.tls.caFile}
      certFile: ${nodeAgentCfg.tls.certFile}
      keyFile: ${nodeAgentCfg.tls.keyFile}
    
    networks:
      default: ${nodeAgentCfg.networks.default}
      ${lib.concatStringsSep "\n      " (lib.mapAttrsToList (name: value: "${name}: ${value}") (lib.filterAttrs (n: v: n != "default") nodeAgentCfg.networks))}
    
    storage:
      drivers:
        local-dir:
          type: local-dir
          parameters:
            path: ${nodeAgentCfg.storage.localDirPath}
  '';
in
{
  # Node agent configuration options
  options.services.kcore.nodeAgent = {
    enable = lib.mkEnableOption "kcore node agent";
    
    nodeId = lib.mkOption {
      type = lib.types.str;
      default = config.networking.hostName;
      description = "Node identifier";
    };
    
    listenAddr = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = ":9091";
      description = "Address to listen on (e.g., ':9091' or '192.168.1.100:9091')";
    };
    
    controllerAddr = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Controller address for registration and state sync";
    };
    
    tls = {
      caFile = lib.mkOption {
        type = lib.types.str;
        default = "/etc/kcore/certs/ca.crt";
        description = "CA certificate file";
      };
      
      certFile = lib.mkOption {
        type = lib.types.str;
        default = "/etc/kcore/certs/node.crt";
        description = "Node certificate file";
      };
      
      keyFile = lib.mkOption {
        type = lib.types.str;
        default = "/etc/kcore/certs/node.key";
        description = "Node key file";
      };
    };
    
    networks = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = {
        default = "default";  # Use libvirt's default network (NAT + DHCP)
      };
      description = ''
        Network name to bridge/libvirt network mapping.
        Use "default" for libvirt's default network (NAT with DHCP).
        Use "br0", "br1", etc. for custom bridge interfaces.
      '';
    };
    
    storage = {
      localDirPath = lib.mkOption {
        type = lib.types.str;
        default = "/var/lib/kcore/disks";
        description = "Path for local directory storage driver";
      };
    };
  };

  config = lib.mkIf nodeAgentCfg.enable {
    # Generate node agent config file
    environment.etc."kcore/node-agent.yaml" = {
      source = nodeAgentConfig;
      mode = "0644";
    };
    
    # Node agent systemd service
    systemd.services.kcore-node-agent = {
      description = "kcore Node Agent";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" "libvirtd.service" ] ++ (lib.optional config.services.kcore.ovs.enable "openvswitch.service");
      wants = (lib.optional config.services.kcore.ovs.enable "openvswitch.service");

      serviceConfig = {
        Type = "simple";
        ExecStart = "${nodeAgentPath} -config /etc/kcore/node-agent.yaml";
        Restart = "always";
        RestartSec = "10s";

      # Security hardening
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = true;
      ReadWritePaths = [
        "/var/lib/kcore"
        "/var/run/libvirt"
      ];
      ReadOnlyPaths = [
        "/etc/kcore"
      ];
      CapabilityBoundingSet = [
        "CAP_SYS_ADMIN" # Needed for libvirt/KVM
        "CAP_NET_ADMIN" # Needed for network management
      ];
      AmbientCapabilities = [
        "CAP_SYS_ADMIN"
        "CAP_NET_ADMIN"
      ];
      User = "root"; # libvirt requires root or libvirt group
      Group = "libvirt";

      # Resource limits
      LimitNOFILE = 65536;
      LimitNPROC = 4096;
    };

    environment = {
      PATH = lib.makeBinPath [
        pkgs.qemu_kvm
        pkgs.libvirt
        pkgs.lvm2
        pkgs.qemu-utils
        pkgs.coreutils
      ];
    };
  };

    # Create directories
    systemd.tmpfiles.rules = [
      "d /var/lib/kcore 0755 root root -"
      "d /var/lib/kcore/disks 0755 root root -"
      "d /opt/kcore 0755 root root -"
      "d /etc/kcore 0755 root root -"
    ];

    # Ensure libvirt group exists
    users.groups.libvirt = {};
  };
}

