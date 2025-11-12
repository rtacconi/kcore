{
  description = "Minimal NixOS for kvm node-agent";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        packages = {
          # Use Go 1.24 from nixos-25.05
          node-agent = (pkgs.buildGoModule.override { go = pkgs.go_1_24; }) {
            pname = "kcore-node-agent";
            version = "0.1.0";
            src = ./.; # Include vendor directory
            vendorHash = null; # Will be computed from vendor directory
            subPackages = [ "cmd/node-agent" ];
            CGO_ENABLED = 1;
            buildFlags = [ "-tags" "libvirt" ];
            # Link against libvirt (runtime)
            buildInputs = with pkgs; [ libvirt ];
            # Build tools (needed during build)
            nativeBuildInputs = with pkgs; [ pkg-config ];
          };
        };
      }
    ) // {
      nixosConfigurations.kvm-node = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          ./modules/kcode-minimal.nix
          ./modules/kcode-branding.nix
          ./modules/kcode-libvirt.nix
          ({ config, pkgs, lib, ... }:
            let
              # Build node agent in the same evaluation
              # Use Go 1.24 from nixos-25.05
              nodeAgent = (pkgs.buildGoModule.override { go = pkgs.go_1_24; }) {
                pname = "kcore-node-agent";
                version = "0.1.0";
                src = pkgs.lib.cleanSourceWith {
                  filter = path: type:
                    let
                      baseName = baseNameOf path;
                    in
                      baseName != "vendor" && baseName != ".git";
                  src = self;
                };
                vendorHash = null; # Will be set after first build
                proxyVendor = true; # Use proxy for vendoring
                subPackages = [ "cmd/node-agent" ];
                CGO_ENABLED = 1;
                buildFlags = [ "-tags" "libvirt" ];
                buildInputs = with pkgs; [ libvirt pkg-config ];
              };
            in
            {
              system.stateVersion = "25.05";
              
              boot.kernelModules = [ "kvm" "kvm_intel" "kvm_amd" "br_netfilter" "tap" ];

              services.qemuGuest.enable = true;

              # Networking: simple bridge for now
              networking = {
                hostName = "kvm-node";
                useDHCP = false;
                interfaces.enp1s0.useDHCP = true;
                bridges.br0.interfaces = [ "enp1s0" ];
                firewall.enable = true;
                firewall.allowedTCPPorts = [ 9091 ]; # node-agent gRPC
              };

              # Virtualization configuration
              virtualisation = {
                kvmgt.enable = false;
                libvirtd.enable = false; # use direct QEMU; can flip to true if you prefer libvirt
                qemu.package = pkgs.qemu_kvm;
              };

              # Node agent service using the built binary
              systemd.services.kcode-node-agent = {
                description = "kcore Node Agent";
                wantedBy = [ "multi-user.target" ];
                after = [ "network-online.target" ] ++ (lib.optional config.virtualisation.libvirtd.enable "libvirtd.service");
                wants = [ "network-online.target" ];

                serviceConfig = {
                  Type = "simple";
                  ExecStart = "${nodeAgent}/bin/kcore-node-agent";
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

                environment = lib.mkForce {
                  PATH = pkgs.lib.makeBinPath [
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

              # Placeholder node agent config (user must update with actual controller IP and certs)
              environment.etc."kcode/node-agent.yaml.example" = {
                text = ''
                  # Node agent configuration
                  # Copy this to /etc/kcode/node-agent.yaml and update with your values
                  nodeId: kvm-node-01
                  controllerAddr: "CHANGE_ME:9090"  # Replace with controller IP

                  tls:
                    caFile: /etc/kcode/ca.crt
                    certFile: /etc/kcode/node.crt
                    keyFile: /etc/kcode/node.key

                  # Network name to bridge name mapping
                  networks:
                    default: br0

                  # Storage driver configuration
                  storage:
                    drivers:
                      local-dir:
                        type: local-dir
                        parameters:
                          path: /var/lib/kcode/disks
                '';
                mode = "0644";
              };

              # System packages
              environment.systemPackages = with pkgs; [
                qemu_kvm
                libvirt
                lvm2
                qemu-utils
                cloud-utils
                iproute2
                jq
                nodeAgent # Include the built binary in system packages
              ];

              # For cloud-init NoCloud seed creation
              services.cloud-init.enable = false; # guests use it; host doesn't need the daemon
            })
        ];
      };

      # ISO image configuration (for USB installation)
      nixosConfigurations.kvm-node-iso = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          "${nixpkgs}/nixos/modules/installer/cd-dvd/iso-image.nix"
          ./modules/kcode-minimal.nix
          ./modules/kcode-branding.nix
          ({ config, pkgs, lib, ... }:
            let
              # Use Go 1.24 from nixos-25.05
              nodeAgent = (pkgs.buildGoModule.override { go = pkgs.go_1_24; }) {
                pname = "kcore-node-agent";
                version = "0.1.0";
                src = self; # Include vendor directory
                vendorHash = null; # Will be computed from vendor directory
                subPackages = [ "cmd/node-agent" ];
                CGO_ENABLED = 1;
                buildFlags = [ "-tags" "libvirt" ];
                # Link against libvirt (runtime)
                buildInputs = with pkgs; [ libvirt ];
                # Build tools (needed during build)
                nativeBuildInputs = with pkgs; [ pkg-config ];
              };
            in
            {
              system.stateVersion = "25.05";
              
              # Allow unfree packages (needed for some firmware)
              nixpkgs.config.allowUnfree = true;
              
              # Don't force GRUB - let iso-image.nix use isolinux for legacy boot
              # GRUB is only needed for installed systems, not ISOs
              # iso-image.nix handles boot configuration automatically
              
              boot.kernelModules = [ "kvm" "kvm_intel" "kvm_amd" "br_netfilter" "tap" ];
              services.qemuGuest.enable = true;
              
              # Simple networking for live ISO - auto-detect all interfaces
              networking.hostName = "kvm-node";
              networking.useDHCP = true;  # Enable DHCP on all interfaces automatically
              networking.firewall.enable = true;
              networking.firewall.allowedTCPPorts = [ 22 9091 ]; # SSH first, then node-agent
              
              # Root user with password for console login
              users.users.root.initialPassword = "kcore";
              users.mutableUsers = true; # Allow changing password after boot
              
              # Enable SSH for remote access
              services.openssh = {
                enable = true;
                settings = {
                  PermitRootLogin = "yes";
                  PasswordAuthentication = true;
                };
              };
              
              virtualisation.kvmgt.enable = false;
              virtualisation.libvirtd.enable = false;
              # QEMU is included in systemPackages below
              systemd.services.kcode-node-agent = {
                description = "kcore Node Agent";
                wantedBy = [ "multi-user.target" ];
                after = [ "network-online.target" ];
                wants = [ "network-online.target" ];
                serviceConfig = {
                  Type = "simple";
                  ExecStart = "${nodeAgent}/bin/kcore-node-agent";
                  Restart = "always";
                  RestartSec = "10s";
                  NoNewPrivileges = true;
                  PrivateTmp = true;
                  ProtectSystem = "strict";
                  ProtectHome = true;
                  ReadWritePaths = [ "/var/lib/kcode" "/var/run/libvirt" ];
                  ReadOnlyPaths = [ "/etc/kcode" ];
                  CapabilityBoundingSet = [ "CAP_SYS_ADMIN" "CAP_NET_ADMIN" ];
                  AmbientCapabilities = [ "CAP_SYS_ADMIN" "CAP_NET_ADMIN" ];
                  User = "root";
                  Group = "libvirt";
                  LimitNOFILE = 65536;
                  LimitNPROC = 4096;
                };
                environment = lib.mkForce {
                  PATH = pkgs.lib.makeBinPath [
                    pkgs.qemu_kvm pkgs.libvirt pkgs.lvm2 pkgs.qemu-utils pkgs.coreutils
                  ];
                };
              };
              systemd.tmpfiles.rules = [
                "d /var/lib/kcode 0755 root root -"
                "d /var/lib/kcode/disks 0755 root root -"
                "d /opt/kcode 0755 root root -"
                "d /etc/kcode 0755 root root -"
              ];
              environment.systemPackages = with pkgs; [
                qemu_kvm libvirt lvm2 qemu-utils cloud-utils iproute2 jq nodeAgent
                # Tools needed for install-to-disk script
                parted util-linux dosfstools e2fsprogs gawk
                (pkgs.writeScriptBin "install-to-disk" ''
                  #!/usr/bin/env bash
                  set -euo pipefail
                  
                  echo "╔══════════════════════════════════════════════════════════╗"
                  echo "║        KCORE Node - Automated Disk Installer            ║"
                  echo "╚══════════════════════════════════════════════════════════╝"
                  echo ""
                  echo "⚠️  This will ERASE the selected disk and install NixOS!"
                  echo ""
                  
                  # Show available disks
                  echo "Available disks:"
                  lsblk -d -o NAME,SIZE,TYPE,MODEL | grep disk
                  echo ""
                  
                  # Ask for target disk
                  read -p "Enter target disk (e.g., sda, nvme0n1, vda): " DISK
                  DISK_PATH="/dev/$DISK"
                  
                  if [ ! -b "$DISK_PATH" ]; then
                    echo "Error: $DISK_PATH is not a valid block device"
                    exit 1
                  fi
                  
                  echo ""
                  echo "Selected: $DISK_PATH"
                  lsblk "$DISK_PATH"
                  echo ""
                  read -p "⚠️  THIS WILL ERASE ALL DATA ON $DISK_PATH! Type 'yes' to continue: " CONFIRM
                  
                  if [ "$CONFIRM" != "yes" ]; then
                    echo "Installation cancelled."
                    exit 0
                  fi
                  
                  echo ""
                  echo "🔧 Partitioning disk..."
                  
                  # Wipe any existing partition table
                  wipefs -a "$DISK_PATH" || true
                  
                  # Create GPT partition table with UEFI + root partitions
                  parted -s "$DISK_PATH" mklabel gpt
                  parted -s "$DISK_PATH" mkpart ESP fat32 1MiB 512MiB
                  parted -s "$DISK_PATH" set 1 esp on
                  parted -s "$DISK_PATH" mkpart primary ext4 512MiB 100%
                  
                  # Wait for kernel to recognize partitions
                  sleep 2
                  partprobe "$DISK_PATH" || true
                  sleep 2
                  
                  # Determine partition names (handle both /dev/sda1 and /dev/nvme0n1p1 styles)
                  if [[ "$DISK" == nvme* ]] || [[ "$DISK" == mmcblk* ]]; then
                    BOOT_PART="''${DISK_PATH}p1"
                    ROOT_PART="''${DISK_PATH}p2"
                  else
                    BOOT_PART="''${DISK_PATH}1"
                    ROOT_PART="''${DISK_PATH}2"
                  fi
                  
                  echo "🔧 Formatting partitions..."
                  mkfs.fat -F 32 -n BOOT "$BOOT_PART"
                  mkfs.ext4 -F -L nixos "$ROOT_PART"
                  
                  echo "🔧 Mounting partitions..."
                  mount "$ROOT_PART" /mnt
                  mkdir -p /mnt/boot
                  mount "$BOOT_PART" /mnt/boot
                  
                  echo "🔧 Generating NixOS configuration..."
                  nixos-generate-config --root /mnt
                  
                  # Copy the current flake configuration
                  echo "🔧 Copying kcore configuration..."
                  cat > /mnt/etc/nixos/configuration.nix << 'EOF'
{ config, pkgs, ... }:
{
  imports = [ ./hardware-configuration.nix ];
  
  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;
  boot.kernelModules = [ "kvm" "kvm_intel" "kvm_amd" "br_netfilter" "tap" ];
  
  # Simple networking - auto-detect all interfaces with DHCP
  networking.hostName = "kvm-node";
  networking.useDHCP = true;
  networking.firewall.enable = true;
  networking.firewall.allowedTCPPorts = [ 22 9091 ];  # SSH + node-agent
  
  users.users.root.initialPassword = "kcore";
  users.mutableUsers = true;
  
  services.openssh = {
    enable = true;
    settings = {
      PermitRootLogin = "yes";
      PasswordAuthentication = true;
    };
  };
  
  virtualisation.libvirtd.enable = false;
  
  environment.systemPackages = with pkgs; [
    vim htop curl wget iproute2 qemu_kvm libvirt lvm2
  ];
  
  system.stateVersion = "25.05";
}
EOF
                  
                  echo "🔧 Installing NixOS (this will take 10-20 minutes)..."
                  nixos-install --no-root-password
                  
                  echo ""
                  echo "╔══════════════════════════════════════════════════════════╗"
                  echo "║  ✅ Installation complete!                               ║"
                  echo "╚══════════════════════════════════════════════════════════╝"
                  echo ""
                  echo "Login credentials:"
                  echo "  Username: root"
                  echo "  Password: kcore"
                  echo ""
                  echo "The system is ready. Remove the USB drive and type:"
                  echo "  reboot"
                  echo ""
                '')
              ];
              environment.etc."kcode/node-agent.yaml.example" = {
                text = ''
                  # Node agent configuration
                  # Copy this to /etc/kcode/node-agent.yaml and update with your values
                  nodeId: kvm-node-01
                  controllerAddr: "CHANGE_ME:9090"

                  tls:
                    caFile: /etc/kcode/ca.crt
                    certFile: /etc/kcode/node.crt
                    keyFile: /etc/kcode/node.key

                  networks:
                    default: br0

                  storage:
                    drivers:
                      local-dir:
                        type: local-dir
                        parameters:
                          path: /var/lib/kcode/disks
                '';
                mode = "0644";
              };
              services.cloud-init.enable = false;
              
              # Ensure libvirt group exists for node agent
              users.groups.libvirt = {};
              
              # Set ISO volume ID
              isoImage.volumeID = "KCORE";
              
              # Ensure the ISO is USB-bootable (hybrid MBR) and UEFI-bootable
              isoImage.makeUsbBootable = true;
              isoImage.makeEfiBootable = true;
            })
        ];
      };
    };
}
