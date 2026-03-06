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

          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              go_1_24
              gopls
              protobuf
              protoc-gen-go
              protoc-gen-go-grpc
              opentofu
              git
              jq
              yq-go
              openssl
              pkg-config
            ] ++ lib.optionals pkgs.stdenv.isLinux [
              libvirt
              qemu_kvm
            ];

            shellHook = ''
              export PATH="$PATH:$(go env GOPATH)/bin"
              echo "kcore dev shell ready (go/protobuf/opentofu)."
            '';
          };
        }
      ) // {
      nixosConfigurations.kvm-node = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          ./modules/kcore-minimal.nix
          ./modules/kcore-branding.nix
          kcoreLibvirtModule

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
                after = [ "network-online.target" ] ++ (lib.optional config.virtualisation.libvirtd.enable "libvirtd.service");
                wants = [ "network-online.target" ];

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
                  controllerAddr: ""  # Set to "<controller-ip>:9090" when controller is ready

                  tls:
                    caFile: /etc/kcore/ca.crt
                    certFile: /etc/kcore/node.crt
                    keyFile: /etc/kcore/node.key

                  # Network name to bridge/libvirt network mapping
                  # Use "default" for libvirt default network (NAT + DHCP)
                  networks:
                    default: default

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
                nodeAgent # Include the shared binary in system packages (makes it available on PATH)
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
          ./modules/kcore-minimal.nix
          ./modules/kcore-branding.nix
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
              systemd.services.kcore-bootstrap-certs = {
                description = "Generate bootstrap TLS certificates for kcore node-agent";
                wantedBy = [ "multi-user.target" ];
                before = [ "kcore-node-agent.service" ];
                serviceConfig = {
                  Type = "oneshot";
                  RemainAfterExit = true;
                };
                script = ''
                  set -euo pipefail
                  mkdir -p /etc/kcore

                  if [ -s /etc/kcore/ca.crt ] && [ -s /etc/kcore/node.crt ] && [ -s /etc/kcore/node.key ]; then
                    exit 0
                  fi

                  tmp="$(mktemp -d)"
                  cleanup() {
                    rm -rf "$tmp"
                  }
                  trap cleanup EXIT

                  cat > "$tmp/node.cnf" <<'EOF'
                  [ req ]
                  default_bits = 2048
                  prompt = no
                  default_md = sha256
                  distinguished_name = dn
                  req_extensions = req_ext
                  [ dn ]
                  CN = kvm-node
                  [ req_ext ]
                  subjectAltName = @alt_names
                  extendedKeyUsage = serverAuth,clientAuth
                  [ alt_names ]
                  DNS.1 = kvm-node
                  DNS.2 = localhost
                  IP.1 = 127.0.0.1
                  EOF

                  ${pkgs.openssl}/bin/openssl req -x509 -newkey rsa:4096 -sha256 -days 3650 -nodes \
                    -subj "/CN=kcore-local-ca" \
                    -keyout /etc/kcore/ca.key \
                    -out /etc/kcore/ca.crt

                  ${pkgs.openssl}/bin/openssl genrsa -out /etc/kcore/node.key 2048
                  ${pkgs.openssl}/bin/openssl req -new \
                    -key /etc/kcore/node.key \
                    -out "$tmp/node.csr" \
                    -config "$tmp/node.cnf"
                  ${pkgs.openssl}/bin/openssl x509 -req \
                    -in "$tmp/node.csr" \
                    -CA /etc/kcore/ca.crt \
                    -CAkey /etc/kcore/ca.key \
                    -CAcreateserial \
                    -out /etc/kcore/node.crt \
                    -days 825 \
                    -sha256 \
                    -extensions req_ext \
                    -extfile "$tmp/node.cnf"

                  chmod 0644 /etc/kcore/ca.crt /etc/kcore/node.crt
                  chmod 0600 /etc/kcore/ca.key /etc/kcore/node.key
                '';
              };
              systemd.services.kcore-node-agent = {
                description = "kcore Node Agent";
                wantedBy = [ "multi-user.target" ];
                after = [ "network-online.target" "kcore-bootstrap-certs.service" ];
                wants = [ "network-online.target" "kcore-bootstrap-certs.service" ];
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
                    pkgs.qemu_kvm pkgs.libvirt pkgs.lvm2 pkgs.qemu-utils pkgs.coreutils
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
                qemu_kvm libvirt lvm2 qemu-utils cloud-utils iproute2 jq nodeAgent openssl
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
                  
                  # Wipe any existing partition table (with retries). -f = force, no interactive prompt (for API/manifest-driven install)
                  for i in {1..3}; do
                    wipefs -af "$DISK_PATH" && break || sleep 2
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
  users.groups.libvirt = {};
  
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

  boot.kernelModules = [ "kvm" "kvm-intel" "kvm-amd" "br_netfilter" "tap" ];
  
  # kcore node-agent service
  systemd.services.kcore-node-agent = {
    description = "kcore Node Agent";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" "libvirtd.service" "virtlogd.service" ];
    wants = [ "network-online.target" ];
    requires = [ "libvirtd.service" ];
    unitConfig = {
      ConditionPathExists = [
        "/etc/kcore/ca.crt"
        "/etc/kcore/node.crt"
        "/etc/kcore/node.key"
      ];
    };
    
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
                    openssl
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
                  controllerAddr: ""

                  tls:
                    caFile: /etc/kcore/ca.crt
                    certFile: /etc/kcore/node.crt
                    keyFile: /etc/kcore/node.key

                  networks:
                    default: default  # libvirt default network (NAT + DHCP)

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
