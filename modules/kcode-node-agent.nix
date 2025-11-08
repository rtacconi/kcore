{ config, lib, pkgs, ... }:

let
  # Path to node agent binary (will be provided externally or built)
  nodeAgentPath = "/opt/kcode/kcore-node-agent";
in
{
  # Node agent systemd service
  systemd.services.kcode-node-agent = {
    description = "kcore Node Agent";
    wantedBy = [ "multi-user.target" ];
    after = [ "network.target" "libvirtd.service" ];

    serviceConfig = {
      Type = "simple";
      ExecStart = "${nodeAgentPath}";
      Restart = "always";
      RestartSec = "10s";

      # Security hardening
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = true;
      ReadWritePaths = [
        "/var/lib/kcode"
        "/var/run/libvirt"
      ];
      ReadOnlyPaths = [
        "/etc/kcode"
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
    "d /var/lib/kcode 0755 root root -"
    "d /var/lib/kcode/disks 0755 root root -"
    "d /opt/kcode 0755 root root -"
    "d /etc/kcode 0755 root root -"
  ];

  # Ensure libvirt group exists
  users.groups.libvirt = {};
}

