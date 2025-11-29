{ config, pkgs, ... }:
{
  imports = [ ./hardware-configuration.nix ];

  # Enable Nix flakes in the installed system
  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;

  # Basic networking: DHCP on all interfaces (node itself)
  networking.hostName = "kvm-node";
  networking.useDHCP = true;
  networking.firewall.enable = true;
  networking.firewall.allowedTCPPorts = [ 22 9091 ]; # SSH + node-agent

  users.users.root = {
    initialPassword = "kcore";
  };
  users.mutableUsers = true;

  services.openssh = {
    enable = true;
    listenAddresses = [ { addr = "0.0.0.0"; port = 22; } ];
    settings = {
      PermitRootLogin = "yes";
      PasswordAuthentication = true;
    };
  };

  # Enable libvirtd for VM management
  virtualisation.libvirtd = {
    enable = true;
    qemu.runAsRoot = true;
  };

  # Ensure virtlogd starts with libvirtd
  systemd.services.virtlogd = {
    wantedBy = [ "multi-user.target" ];
    before = [ "libvirtd.service" ];
  };

  # Open vSwitch kernel modules and vswitch
  boot.kernelModules = [ "kvm" "kvm-intel" "kvm-amd" "br_netfilter" "tap" "openvswitch" ];
  virtualisation.vswitch = {
    enable = true;
    package = pkgs.openvswitch;
  };

  # System packages, including OVN for advanced networking
  environment.systemPackages = with pkgs; [
    vim
    htop
    curl
    wget
    iproute2
    qemu_kvm
    libvirt
    lvm2
    parted
    openvswitch
    ovn
    cloud-utils
  ];

  # OVN northd (NB+SB DB + control plane)
  systemd.services."ovn-northd" = {
    description = "OVN Northbound Daemon";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    serviceConfig = {
      ExecStart = "${pkgs.ovn}/bin/ovn-northd --ovnnb-db=unix:/var/run/ovn/ovnnb_db.sock --ovnsb-db=unix:/var/run/ovn/ovnsb_db.sock";
      Restart = "always";
    };
  };

  # OVN controller on this node, bound to the integration bridge br-int.
  systemd.services."ovn-controller" = {
    description = "OVN Controller (kcore node)";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" "ovs-vswitchd.service" ];
    serviceConfig = {
      ExecStart = "${pkgs.ovn}/bin/ovn-controller --ovnnb-db=unix:/var/run/ovn/ovnnb_db.sock --ovnsb-db=unix:/var/run/ovn/ovnsb_db.sock --local=br-int";
      Restart = "always";
    };
  };

  # kcore node-agent service
  systemd.services.kcore-node-agent = {
    description = "kcore Node Agent";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" "libvirtd.service" "virtlogd.service" "ovs-vswitchd.service" ];
    wants = [ "network-online.target" "ovs-vswitchd.service" ];
    requires = [ "libvirtd.service" ];

    serviceConfig = {
      Type = "simple";
      ExecStart = "/opt/kcore/bin/kcore-node-agent";
      Restart = "always";
      RestartSec = "10s";

      # Security hardening
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = true;
      ReadWritePaths = [ "/var/lib/kcore" "/var/run/libvirt" ];
      ReadOnlyPaths = [ "/etc/kcore" ];

      # Capabilities for libvirt/KVM and networking
      CapabilityBoundingSet = [ "CAP_SYS_ADMIN" "CAP_NET_ADMIN" ];
      AmbientCapabilities = [ "CAP_SYS_ADMIN" "CAP_NET_ADMIN" ];

      User = "root";
      Group = "libvirt";

      # Resource limits
      LimitNOFILE = 65536;
      LimitNPROC = 4096;
    };
  };

  # Create required directories
  systemd.tmpfiles.rules = [
    "d /var/lib/kcore 0755 root root -"
    "d /var/lib/kcore/disks 0755 root root -"
    "d /opt/kcore 0755 root root -"
    "d /opt/kcore/bin 0755 root root -"
    "d /etc/kcore 0755 root root -"
  ];

  system.stateVersion = "25.05";
}


