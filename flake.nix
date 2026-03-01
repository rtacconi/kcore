{
  description = "Minimal NixOS for kvm node-agent";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    let
      inherit (nixpkgs) lib;

      # Single semantic version for kcore, shared between ISO and Go binaries.
      kcoreVersion = lib.strings.trim (builtins.readFile ./VERSION);

      # Shared libvirt / bridge module used by both kvm-node and (optionally) ISO
      kcoreLibvirtModule = { config, lib, pkgs, ... }:
        let
          bridgeCfg = config.services.kcore.bridge;
        in
        {
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
              description = "Physical NIC bound to bridge (e.g. 'enp1s0d1'), or null.";
            };
            ipAddress = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "Bridge IP in CIDR (e.g. '192.168.1.100/24'), or null.";
            };
          };

          config = lib.mkMerge [
            (lib.mkIf bridgeCfg.enable {
              networking.bridges.${bridgeCfg.name} = {
                interfaces = lib.optional (bridgeCfg.physicalInterface != null) bridgeCfg.physicalInterface;
              };
              networking.interfaces.${bridgeCfg.name} = lib.mkIf (bridgeCfg.ipAddress != null) {
                ipv4.addresses = [{
                  address = lib.head (lib.splitString "/" bridgeCfg.ipAddress);
                  prefixLength = lib.toInt (lib.last (lib.splitString "/" bridgeCfg.ipAddress));
                }];
              };
            })
            {
              virtualisation.libvirtd.enable = lib.mkDefault true;
              virtualisation.libvirtd.qemu = {
                package = pkgs.qemu_kvm;
                runAsRoot = true;
                swtpm.enable = true;
              };

              boot.kernelModules = [ "kvm-intel" "kvm-amd" ];

              users.users.root.extraGroups =
                lib.mkIf config.virtualisation.libvirtd.enable [ "libvirt" "libvirtd" ];
              users.groups.libvirt = {};
              users.groups.libvirtd = {};

              systemd.services.libvirt-default-network =
                lib.mkIf config.virtualisation.libvirtd.enable {
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
        };

      # Shared Open vSwitch module used by host and ISO
      kcoreOvsModule = { config, lib, pkgs, ... }:
        {
          options.services.kcore.ovs = {
            enable = lib.mkEnableOption "Open vSwitch for kcore VM networking";
            datapathType = lib.mkOption {
              type = lib.types.str;
              default = "system";
              description = "OVS datapath type: 'system' (kernel) or 'netdev' (DPDK)";
            };
          };

          config = lib.mkIf config.services.kcore.ovs.enable {
            boot.kernelModules = [ "openvswitch" ];
            virtualisation.vswitch = {
              enable = true;
              package = pkgs.openvswitch;
            };
            environment.systemPackages = with pkgs; [
              openvswitch
              dnsmasq
              iproute2
              iptables
            ];
            systemd.tmpfiles.rules = [
              "d /var/lib/openvswitch 0755 root root -"
            ];

            # Provide a Proxmox-like default NAT network on br-int for VMs.
            # This creates br-int, gives it 192.168.200.1/24, enables IP forwarding,
            # configures iptables MASQUERADE, and runs dnsmasq to hand out DHCP
            # leases on 192.168.200.0/24.
            systemd.services.kcore-ovs-nat-dhcp = {
              description = "kcore: OVS br-int with NAT + DHCP for VMs";
              wantedBy = [ "multi-user.target" ];
              after = [ "network-online.target" "ovs-vswitchd.service" ];
              wants = [ "network-online.target" "ovs-vswitchd.service" ];
              serviceConfig = {
                Type = "simple";
                ExecStartPre = pkgs.writeShellScript "kcore-ovs-nat-pre" ''
                  set -e

                  # Determine the external interface from the default IPv4 route.
                  EXT_IF="$(${pkgs.iproute2}/bin/ip -4 route show default | head -n1 | cut -d' ' -f5 || true)"
                  if [ -z "$EXT_IF" ]; then
                    echo "kcore-ovs-nat-pre: could not determine external interface for NAT" >&2
                    exit 0
                  fi

                  # Ensure br-int exists.
                  ${pkgs.openvswitch}/bin/ovs-vsctl --may-exist add-br br-int

                  # Apply datapath type if configured (system or netdev).
                  ${lib.optionalString (config.services.kcore.ovs.datapathType != "") ''
                    ${pkgs.openvswitch}/bin/ovs-vsctl set bridge br-int datapath_type=${config.services.kcore.ovs.datapathType}
                  ''}

                  # Assign IP and bring br-int up.
                  ${pkgs.iproute2}/bin/ip addr flush dev br-int || true
                  ${pkgs.iproute2}/bin/ip addr add 192.168.200.1/24 dev br-int
                  ${pkgs.iproute2}/bin/ip link set br-int up

                  # Enable IPv4 forwarding.
                  ${pkgs.procps}/bin/sysctl -w net.ipv4.ip_forward=1 >/dev/null

                  # Configure NAT for the VM subnet.
                  ${pkgs.iptables}/bin/iptables -t nat -C POSTROUTING -s 192.168.200.0/24 -o "$EXT_IF" -j MASQUERADE 2>/dev/null || \
                    ${pkgs.iptables}/bin/iptables -t nat -A POSTROUTING -s 192.168.200.0/24 -o "$EXT_IF" -j MASQUERADE
                '';

                ExecStart = "${pkgs.dnsmasq}/bin/dnsmasq "
                  + "--interface=br-int "
                  + "--bind-interfaces "
                  + "--except-interface=lo "
                  + "--dhcp-range=192.168.200.10,192.168.200.200,12h "
                  + "--dhcp-option=3,192.168.200.1 "
                  + "--dhcp-option=6,1.1.1.1 "
                  + "--pid-file=/run/dnsmasq-br-int.pid "
                  + "--log-facility=/var/log/dnsmasq-br-int.log";
              };
            };
          };
        };

      # Shared OVN module used by nodes that participate in advanced networking.
      # This module does NOT enable OVN by default; nodes opt in via
      #   services.kcore.ovn.enable = true;
      # and select a role:
      #   "central" - runs NB/SB ovsdb-server + ovn-northd + ovn-controller
      #   "chassis" - runs only ovn-controller and connects to a remote NB/SB
      kcoreOvnModule = { config, lib, pkgs, ... }:
        let
          cfg = config.services.kcore.ovn;
        in {
          options.services.kcore.ovn = {
            enable = lib.mkEnableOption "OVN integration for kcore";

            role = lib.mkOption {
              type = lib.types.enum [ "central" "chassis" ];
              default = "central";
              description = ''
                OVN role for this node.

                - "central": runs OVN NB/SB ovsdb-server and ovn-northd locally.
                - "chassis": runs only ovn-controller and connects to remote NB/SB.
              '';
            };

            nbConnection = lib.mkOption {
              type = lib.types.str;
              default = "unix:/var/run/ovn/ovnnb_db.sock";
              description = "OVN northbound DB connection string (used by ovn-northd and chassis controllers).";
            };

            sbConnection = lib.mkOption {
              type = lib.types.str;
              default = "unix:/var/run/ovn/ovnsb_db.sock";
              description = "OVN southbound DB connection string (used by ovn-controller).";
            };
          };

          config = lib.mkIf cfg.enable {
            environment.systemPackages = with pkgs; [
              ovn
            ];

            # Ensure OVN runtime and config directories exist.
            systemd.tmpfiles.rules = [
              "d /etc/ovn 0755 root root -"
              "d /var/run/ovn 0755 root root -"
            ];

            # Central role: own NB/SB DB files and ovsdb-server processes.
            systemd.services.kcore-ovn-db-setup = lib.mkIf (cfg.role == "central") {
              description = "kcore: Initialize OVN NB/SB databases";
              wantedBy = [ "multi-user.target" ];
              before = [ "ovn-ovsdb-nb.service" "ovn-ovsdb-sb.service" ];
              serviceConfig = {
                Type = "oneshot";
                RemainAfterExit = true;
                ExecStart = pkgs.writeShellScript "kcore-ovn-db-setup" ''
                  set -e
                  OVN_PREFIX="${pkgs.ovn}"
                  SCHEMA_NB="$OVN_PREFIX/share/ovn/ovn-nb.ovsschema"
                  SCHEMA_SB="$OVN_PREFIX/share/ovn/ovn-sb.ovsschema"

                  if [ ! -f "$SCHEMA_NB" ] || [ ! -f "$SCHEMA_SB" ]; then
                    echo "OVN schema files not found at $SCHEMA_NB / $SCHEMA_SB" >&2
                    exit 1
                  fi

                  mkdir -p /etc/ovn /var/run/ovn

                  if [ ! -f /etc/ovn/ovnnb_db.db ]; then
                    echo "Creating /etc/ovn/ovnnb_db.db..."
                    ${pkgs.ovn}/bin/ovsdb-tool create /etc/ovn/ovnnb_db.db "$SCHEMA_NB"
                  fi

                  if [ ! -f /etc/ovn/ovnsb_db.db ]; then
                    echo "Creating /etc/ovn/ovnsb_db.db..."
                    ${pkgs.ovn}/bin/ovsdb-tool create /etc/ovn/ovnsb_db.db "$SCHEMA_SB"
                  fi
                '';
              };
            };

            systemd.services."ovn-ovsdb-nb" = lib.mkIf (cfg.role == "central") {
              description = "OVN Northbound ovsdb-server";
              wantedBy = [ "multi-user.target" ];
              after = [ "network-online.target" "kcore-ovn-db-setup.service" ];
              serviceConfig = {
                Type = "simple";
                RuntimeDirectory = "ovn";
                ExecStart = "${pkgs.ovn}/bin/ovsdb-server "
                  + "--no-chdir "
                  + "--log-file=/var/log/ovnnb.log "
                  + "--remote=punix:/var/run/ovn/ovnnb_db.sock "
                  + "--pidfile=/run/ovn/ovnnb_db.pid "
                  + "/etc/ovn/ovnnb_db.db";
                Restart = "always";
              };
            };

            systemd.services."ovn-ovsdb-sb" = lib.mkIf (cfg.role == "central") {
              description = "OVN Southbound ovsdb-server";
              wantedBy = [ "multi-user.target" ];
              after = [ "network-online.target" "kcore-ovn-db-setup.service" ];
              serviceConfig = {
                Type = "simple";
                RuntimeDirectory = "ovn";
                ExecStart = "${pkgs.ovn}/bin/ovsdb-server "
                  + "--no-chdir "
                  + "--log-file=/var/log/ovnsb.log "
                  + "--remote=punix:/var/run/ovn/ovnsb_db.sock "
                  + "--pidfile=/run/ovn/ovnsb_db.pid "
                  + "/etc/ovn/ovnsb_db.db";
                Restart = "always";
              };
            };

            systemd.services."ovn-northd" = lib.mkIf (cfg.role == "central") {
              description = "OVN Northbound Daemon";
              wantedBy = [ "multi-user.target" ];
              after = [ "network-online.target" "ovn-ovsdb-nb.service" "ovn-ovsdb-sb.service" ];
              serviceConfig = {
                Type = "simple";
                RuntimeDirectory = "ovn";
                ExecStart = "${pkgs.ovn}/bin/ovn-northd "
                  + "--ovnnb-db=${cfg.nbConnection} "
                  + "--ovnsb-db=${cfg.sbConnection}";
                Restart = "always";
              };
            };

            # ovn-controller runs on both central and chassis nodes as the local
            # chassis agent. For now we point it at the local OVS DB and rely on
            # OVS external_ids for NB/SB connection config.
            systemd.services."ovn-controller" = {
              description = "OVN Controller (kcore node)";
              wantedBy = [ "multi-user.target" ];
              after = [ "network-online.target" "ovs-vswitchd.service" ];
              serviceConfig = {
                Type = "simple";
                RuntimeDirectory = "ovn";
                ExecStart = "${pkgs.ovn}/bin/ovn-controller unix:/run/openvswitch/db.sock";
                Restart = "always";
              };
            };
          };
        };
    in
      flake-utils.lib.eachDefaultSystem (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        {
          packages = {
            # Single canonical Go package definition for node-agent
            # This is reused by both NixOS configurations (kvm-node and kvm-node-iso)
            node-agent = (pkgs.buildGoModule.override { go = pkgs.go_1_24; }) {
              pname = "kcore-node-agent";
              version = "0.1.0";
              src = ./.; # Include vendor directory
              vendorHash = "sha256-K14plyEnAIibeETxGZaQhTasSp2Gw0CCm3IvqGizdDo=";
              subPackages = [ "cmd/node-agent" ];
              env.CGO_ENABLED = "1";
              buildFlags = [ "-tags" "libvirt" ];
              # Link against libvirt (runtime)
              buildInputs = with pkgs; [ libvirt ];
              # Build tools (needed during build)
              nativeBuildInputs = with pkgs; [ pkg-config ];
              
              # The subPackages build creates 'node-agent' binary, but we want 'kcore-node-agent'
              # Create a symlink so both names work (for compatibility)
              postInstall = ''
                ln -sf node-agent $out/bin/kcore-node-agent
              '';
            };
          };
        }
      ) // {
      nixosConfigurations.kvm-node = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          ./modules/kcore-minimal.nix
          ./modules/kcore-branding.nix
          kcoreLibvirtModule
          kcoreOvsModule

          ({ config, pkgs, lib, ... }:
            let
              # Reuse the shared node-agent package from outputs.packages
              # This ensures we use the exact same binary as the ISO
              nodeAgent = self.packages.${pkgs.system}.node-agent;
            in
            {
              system.stateVersion = "25.05";
              services.qemuGuest.enable = true;

              # Networking: bridge-based, DHCP on br0 (not on the enslaved NIC)
              networking = {
                hostName = "kvm-node";
                useDHCP = false;
                interfaces.enp1s0.useDHCP = false;
                bridges.br0.interfaces = [ "enp1s0" ];
                interfaces.br0.useDHCP = true;
                firewall.enable = true;
                firewall.allowedTCPPorts = [ 9091 ]; # node-agent gRPC
              };

              # Virtualization configuration
              virtualisation = {
                kvmgt.enable = false;
                libvirtd.enable = false; # use direct QEMU; can flip to true if you prefer libvirt
                # QEMU package is included in systemPackages below
              };

              # Node agent service using the shared binary
              systemd.services.kcore-node-agent = {
                description = "kcore Node Agent";
                wantedBy = [ "multi-user.target" ];
                after = [ "network-online.target" "ovs-vswitchd.service" ] ++ (lib.optional config.virtualisation.libvirtd.enable "libvirtd.service");
                wants = [ "network-online.target" "ovs-vswitchd.service" ];

                serviceConfig = {
                  Type = "simple";
                  # Use kcore-node-agent (the symlink created by postInstall)
                  ExecStart = "${nodeAgent}/bin/kcore-node-agent";
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

                environment = lib.mkForce {
                  PATH = pkgs.lib.makeBinPath [
                    pkgs.qemu_kvm
                    pkgs.libvirt
                    pkgs.lvm2
                    pkgs.qemu-utils
                    pkgs.coreutils
                    pkgs.openvswitch  # OVS utilities (ovs-vsctl, ovs-ofctl, etc.)
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

              # Placeholder node agent config (user must update with actual controller IP and certs)
              environment.etc."kcore/node-agent.yaml.example" = {
                text = ''
                  # Node agent configuration
                  # Copy this to /etc/kcore/node-agent.yaml and update with your values
                  nodeId: kvm-node-01
                  controllerAddr: "CHANGE_ME:9090"  # Replace with controller IP

                  tls:
                    caFile: /etc/kcore/ca.crt
                    certFile: /etc/kcore/node.crt
                    keyFile: /etc/kcore/node.key

                  # Network name to bridge/OVS mapping
                  # Use OVS bridge names (e.g., "br-int", "br-ex")
                  # OVS will be used instead of libvirt virbr0
                  networks:
                    default: br-int  # OVS integration bridge

                  # Storage driver configuration
                  storage:
                    drivers:
                      local-dir:
                        type: local-dir
                        parameters:
                          path: /var/lib/kcore/disks
                '';
                mode = "0644";
              };

              # System packages - include the shared node-agent package
              environment.systemPackages = with pkgs; [
                qemu_kvm
                libvirt
                lvm2
                qemu-utils
                cloud-utils
                iproute2
                jq
                openvswitch  # Open vSwitch for networking
                nodeAgent # Include the shared binary in system packages (makes it available on PATH)
              ];
              
              # Enable Open vSwitch
              services.kcore.ovs.enable = true;

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
          ./modules/kcore-minimal.nix
          ./modules/kcore-branding.nix
          kcoreOvsModule
          ({ config, pkgs, lib, ... }:
            let
              # Reuse the shared node-agent package from outputs.packages
              # This ensures the ISO contains the exact same binary that will be installed
              nodeAgent = self.packages.${pkgs.system}.node-agent;
            in
            {
              system.stateVersion = "25.05";
              
              # Allow unfree packages (needed for some firmware)
              nixpkgs.config.allowUnfree = true;
              
              # Boot experience: make ISO feel like kcore, skip NixOS boot menu as much as possible
              # NOTE: iso-image.nix still provides the underlying bootloader (isolinux/systemd-boot),
              # but we can minimize the menu / branding here.
              boot.loader.timeout = lib.mkForce 0;            # Auto-boot default entry, no menu delay
              boot.loader.systemd-boot.editor = false;
              # Reduce kernel console noise a bit
              boot.kernelParams = [ "quiet" "loglevel=3" ];
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
                listenAddresses = [ { addr = "0.0.0.0"; port = 22; } ]; # Listen on all interfaces
                settings = {
                  PermitRootLogin = "yes";
                  PasswordAuthentication = true;
                };
              };
              
              virtualisation.kvmgt.enable = false;
              virtualisation.libvirtd.enable = false;
              # QEMU is included in systemPackages below
              systemd.services.kcore-node-agent = {
                description = "kcore Node Agent";
                wantedBy = [ "multi-user.target" ];
                after = [ "network-online.target" "ovs-vswitchd.service" ];
                wants = [ "network-online.target" "ovs-vswitchd.service" ];
                serviceConfig = {
                  Type = "simple";
                  # Use kcore-node-agent (the symlink created by postInstall)
                  ExecStart = "${nodeAgent}/bin/kcore-node-agent";
                  Restart = "always";
                  RestartSec = "10s";
                  NoNewPrivileges = true;
                  PrivateTmp = true;
                  ProtectSystem = "strict";
                  ProtectHome = true;
                  ReadWritePaths = [ "/var/lib/kcore" "/var/run/libvirt" ];
                  ReadOnlyPaths = [ "/etc/kcore" ];
                  CapabilityBoundingSet = [ "CAP_SYS_ADMIN" "CAP_NET_ADMIN" ];
                  AmbientCapabilities = [ "CAP_SYS_ADMIN" "CAP_NET_ADMIN" ];
                  User = "root";
                  Group = "libvirt";
                  LimitNOFILE = 65536;
                  LimitNPROC = 4096;
                };
                environment = lib.mkForce {
                  PATH = pkgs.lib.makeBinPath [
                    pkgs.qemu_kvm pkgs.libvirt pkgs.lvm2 pkgs.qemu-utils pkgs.coreutils pkgs.openvswitch
                  ];
                };
              };
              systemd.tmpfiles.rules = [
                "d /var/lib/kcore 0755 root root -"
                "d /var/lib/kcore/disks 0755 root root -"
                "d /opt/kcore 0755 root root -"
                "d /etc/kcore 0755 root root -"
              ];
              environment.systemPackages = with pkgs; [
                qemu_kvm libvirt lvm2 qemu-utils cloud-utils iproute2 jq openvswitch nodeAgent
                # Tools needed for install-to-disk script (parted is main missing one)
                parted
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
                  echo "🔧 Preparing disk (deactivating LVM and unmounting)..."
                  
                  # Deactivate any LVM volume groups on this disk
                  for vg in $(vgs --noheadings -o vg_name 2>/dev/null || true); do
                    echo "Deactivating volume group: $vg"
                    vgchange -an "$vg" 2>/dev/null || true
                  done
                  
                  # Unmount any partitions on this disk
                  for part in "$DISK_PATH"*; do
                    if [ -b "$part" ]; then
                      umount "$part" 2>/dev/null || true
                    fi
                  done
                  
                  echo "🔧 Partitioning disk..."
                  
                  # Wipe any existing partition table (with retries)
                  for i in {1..3}; do
                    wipefs -a "$DISK_PATH" && break || sleep 2
                  done
                  
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
                  # Ensure /mnt directory exists
                  mkdir -p /mnt
                  mount "$ROOT_PART" /mnt
                  mkdir -p /mnt/boot
                  mount "$BOOT_PART" /mnt/boot
                  
                  echo "🔧 Generating NixOS configuration..."
                  nixos-generate-config --root /mnt
                  
                  # Copy the current flake configuration
                  echo "🔧 Copying kcore configuration..."
                  
                  # Detect SSH authorized keys from live environment
                  SSH_KEYS=""
                  if [ -f /root/.ssh/authorized_keys ]; then
                    echo "📋 Found SSH authorized keys, adding to installed system..."
                    SSH_KEYS=$(cat /root/.ssh/authorized_keys | sed 's/^/      "/' | sed 's/$/"/' | paste -sd '\n')
                  fi
                  
                  # Copy node-agent binary to installed system
                  # The binary is already in the Nix store and available via PATH (from systemPackages)
                  # We use the store path directly (Nix substitutes ${nodeAgent} at build time)
                  echo "📋 Copying node-agent binary..."
                  mkdir -p /mnt/opt/kcore/bin
                  
                  # Primary method: Use the store path directly (Nix substitutes this at ISO build time)
                  # The ${nodeAgent} variable is replaced with the actual Nix store path during ISO build
                  NODE_AGENT_BIN="${nodeAgent}/bin/kcore-node-agent"
                  
                  # Verify the binary exists and is executable
                  if [ ! -f "$NODE_AGENT_BIN" ] || [ ! -x "$NODE_AGENT_BIN" ]; then
                    echo "❌ Error: kcore-node-agent binary not found at expected store path: $NODE_AGENT_BIN"
                    echo ""
                    echo "Debug information:"
                    echo "  Store path: ${nodeAgent}"
                    echo "  Expected binary: $NODE_AGENT_BIN"
                    echo "  PATH: $PATH"
                    echo ""
                    echo "Fallback: Trying to find binary via PATH..."
                    if command -v kcore-node-agent >/dev/null 2>&1; then
                      NODE_AGENT_BIN=$(command -v kcore-node-agent)
                      echo "✅ Found via PATH: $NODE_AGENT_BIN"
                    else
                      echo "❌ Binary not found in PATH either"
                      exit 1
                    fi
                  else
                    echo "✅ Found binary at store path: $NODE_AGENT_BIN"
                  fi
                  
                  # Copy the binary to the installed system
                  cp "$NODE_AGENT_BIN" /mnt/opt/kcore/bin/kcore-node-agent
                  chmod +x /mnt/opt/kcore/bin/kcore-node-agent
                  
                  # Verify it was copied successfully
                  if [ ! -f /mnt/opt/kcore/bin/kcore-node-agent ] || [ ! -x /mnt/opt/kcore/bin/kcore-node-agent ]; then
                    echo "❌ Error: Failed to copy or make node-agent binary executable"
                    exit 1
                  fi
                  echo "✅ Node-agent binary copied successfully ($(du -h /mnt/opt/kcore/bin/kcore-node-agent | cut -f1))"
                  
                  # Copy kcore config and certs if they exist
                  if [ -d /etc/kcore ]; then
                    echo "📋 Copying kcore configuration and certificates..."
                    mkdir -p /mnt/etc/kcore
                    cp -r /etc/kcore/* /mnt/etc/kcore/ 2>/dev/null || true
                  fi
                  
                  cat > /mnt/etc/nixos/configuration.nix << EOF
{ config, pkgs, ... }:
{
  imports = [ ./hardware-configuration.nix ];
  
  # Enable Nix flakes in the installed system
  nix.settings.experimental-features = [ "nix-command" "flakes" ];
  
  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;
  
  # Simple networking - auto-detect all interfaces with DHCP
  networking.hostName = "kvm-node";
  networking.useDHCP = true;
  networking.firewall.enable = true;
  networking.firewall.allowedTCPPorts = [ 22 9091 ];  # SSH + node-agent
  
  users.users.root = {
    initialPassword = "kcore";
    openssh.authorizedKeys.keys = [
$SSH_KEYS
    ];
  };
  users.mutableUsers = true;
  
  services.openssh = {
    enable = true;
    listenAddresses = [ { addr = "0.0.0.0"; port = 22; } ]; # Listen on all interfaces (including br0)
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

  # Enable Open vSwitch using standard NixOS options (no custom services.kcore)
  boot.kernelModules = [ "kvm" "kvm-intel" "kvm-amd" "br_netfilter" "tap" "openvswitch" ];
  virtualisation.vswitch = {
    enable = true;
    package = pkgs.openvswitch;
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
  
  system.stateVersion = "25.05";
}
EOF
                  
                  echo "🔧 Configuring Nix with flakes support..."
                  # Configure flakes for the installed system
                  mkdir -p /mnt/etc/nix
                  echo "experimental-features = nix-command flakes" > /mnt/etc/nix/nix.conf
                  
                  # NOTE: The live ISO root filesystem is read-only, so we cannot
                  # write to /etc/nix/nix.conf here. Instead, we rely on NIX_CONFIG
                  # for nixos-install, and /mnt/etc/nix/nix.conf for the installed system.
                  
                  echo "🔧 Installing NixOS (this will take 10-20 minutes)..."
                  export NIX_CONFIG="experimental-features = nix-command flakes"
                  # Root password is set non-interactively via users.users.root.initialPassword = "kcore"
                  nixos-install
                  
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
              environment.etc."kcore/node-agent.yaml" = {
                text = ''
                  # Node agent configuration
                  # Managed by kcore ISO defaults – adjust values to your environment.
                  nodeId: kvm-node-01
                  controllerAddr: "CHANGE_ME:9090"

                  tls:
                    caFile: /etc/kcore/ca.crt
                    certFile: /etc/kcore/node.crt
                    keyFile: /etc/kcore/node.key

                  networks:
                    default: br-int  # OVS integration bridge

                  storage:
                    drivers:
                      local-dir:
                        type: local-dir
                        parameters:
                          path: /var/lib/kcore/disks
                '';
                mode = "0644";
              };
              services.cloud-init.enable = false;
              
              # Enable Open vSwitch
              services.kcore.ovs.enable = true;
              
              # Ensure libvirt group exists for node agent
              users.groups.libvirt = {};
              
              # Set ISO volume ID and name with semantic version
              isoImage.volumeID = "KCORE";
              isoImage.isoName = "nixos-kcore-" + kcoreVersion + "-x86_64-linux.iso";
              
              # Ensure the ISO is USB-bootable (hybrid MBR) and UEFI-bootable
              isoImage.makeUsbBootable = true;
              isoImage.makeEfiBootable = true;
            })
        ];
      };
    };
}
