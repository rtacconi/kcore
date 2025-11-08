{ config, lib, pkgs, ... }:

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
}

