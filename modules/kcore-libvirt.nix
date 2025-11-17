{ config, lib, pkgs, ... }:

let
  bridgeCfg = config.services.kcore.bridge;
in
{
  # Bridge configuration for host subnet networking
  options.services.kcore.bridge = {
    enable = lib.mkEnableOption "bridge for host subnet VM networking";
    
    name = lib.mkOption {
      type = lib.types.str;
      default = "br0";
      description = "Bridge interface name";
    };
    
    physicalInterface = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Physical network interface to bind to bridge (e.g., 'enp1s0d1'). If null, bridge is created without physical interface.";
    };
    
    ipAddress = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "IP address for bridge (CIDR notation, e.g., '192.168.1.100/24'). If null, no IP is assigned.";
    };
  };

  config = lib.mkMerge [
    (lib.mkIf bridgeCfg.enable {
      # Create bridge interface with physical NIC bound to it
      # This allows VMs to get IPs from the real subnet
      networking.bridges.${bridgeCfg.name} = {
        interfaces = lib.optional (bridgeCfg.physicalInterface != null) bridgeCfg.physicalInterface;
      };
      
      # Configure bridge IP if specified
      networking.interfaces.${bridgeCfg.name} = lib.mkIf (bridgeCfg.ipAddress != null) {
        ipv4.addresses = [{
          address = lib.head (lib.splitString "/" bridgeCfg.ipAddress);
          prefixLength = lib.toInt (lib.last (lib.splitString "/" bridgeCfg.ipAddress));
        }];
      };
    })
    {
      # Enable libvirt (can be disabled in main config for direct QEMU mode)
      virtualisation.libvirtd = lib.mkIf (config.virtualisation.libvirtd.enable != false) {
        enable = true;
        qemu = {
          package = pkgs.qemu_kvm;
          runAsRoot = true;
          swtpm.enable = true;
        };
      };

      # Enable KVM
      boot.kernelModules = [ "kvm-intel" "kvm-amd" ];

      # Permissions for libvirt (only if libvirtd is enabled)
      users.users.root.extraGroups = lib.mkIf (config.virtualisation.libvirtd.enable != false) [ "libvirt" "libvirtd" ];

      # Ensure libvirt group exists
      users.groups.libvirt = {};
      users.groups.libvirtd = {};

      # Ensure libvirt default network is started and autostarted
      # This provides NAT networking with DHCP for VMs
      systemd.services.libvirt-default-network = lib.mkIf (config.virtualisation.libvirtd.enable != false) {
        description = "Ensure libvirt default network is active";
        wantedBy = [ "multi-user.target" ];
        after = [ "libvirtd.service" ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          ExecStart = pkgs.writeShellScript "start-default-network" ''
            ${pkgs.libvirt}/bin/virsh -c qemu:///system net-start default || true
            ${pkgs.libvirt}/bin/virsh -c qemu:///system net-autostart default || true
          '';
        };
      };
    }
  ];
}
