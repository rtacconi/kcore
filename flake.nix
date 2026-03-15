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
              vendorHash = "sha256-ci/hEqEtBqw3Go6h1IMwxK9PqXqP1CZsYGR2yf8Jn5c=";
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

            controller = (pkgs.buildGoModule.override { go = pkgs.go_1_24; }) {
              pname = "kcore-controller";
              version = "0.1.0";
              src = ./.;
              vendorHash = "sha256-ci/hEqEtBqw3Go6h1IMwxK9PqXqP1CZsYGR2yf8Jn5c=";
              subPackages = [ "cmd/controller" ];
              env.CGO_ENABLED = "1";
              postInstall = ''
                ln -sf controller $out/bin/kcore-controller
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
              export BASH_COMPLETION_COMPAT_DIR="/dev/null"
              export PATH="$PATH:$(go env GOPATH)/bin"

              # Readline / history for interactive shells
              set -o emacs 2>/dev/null
              export HISTFILE="$HOME/.bash_history"
              export HISTSIZE=10000
              export HISTFILESIZE=20000
              export HISTCONTROL="ignoredups:erasedups"
              shopt -s histappend 2>/dev/null
              [ -f /etc/inputrc ] && bind -f /etc/inputrc 2>/dev/null
              [ -f "$HOME/.inputrc" ] && bind -f "$HOME/.inputrc" 2>/dev/null
              export PS1='\[\e[1;34m\][nix-dev]\[\e[0m\] \w \$ '

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
              # Reuse the shared packages from outputs.packages
              # This ensures the ISO contains the exact same binaries that will be installed
              nodeAgent = self.packages.${pkgs.system}.node-agent;
              controllerPkg = self.packages.${pkgs.system}.controller;
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
              networking.firewall.allowedTCPPorts = [ 22 9090 9091 ]; # SSH, controller, node-agent
              
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
              systemd.services.kcore-controller = {
                description = "kcore Controller (bootstrap/insecure)";
                wantedBy = [ "multi-user.target" ];
                after = [ "network-online.target" "kcore-bootstrap-certs.service" ];
                wants = [ "network-online.target" "kcore-bootstrap-certs.service" ];
                serviceConfig = {
                  Type = "simple";
                  ExecStart = "${controllerPkg}/bin/kcore-controller --listen :9090 --insecure --node-insecure --auto-register-local --db /var/lib/kcore/controller.db";
                  Restart = "always";
                  RestartSec = "10s";
                  NoNewPrivileges = true;
                  PrivateTmp = true;
                  ProtectSystem = "strict";
                  ProtectHome = true;
                  ReadWritePaths = [ "/var/lib/kcore" ];
                  ReadOnlyPaths = [ "/etc/kcore" ];
                  User = "root";
                  LimitNOFILE = 65536;
                  LimitNPROC = 4096;
                };
              };
              systemd.services.kcore-node-agent = {
                description = "kcore Node Agent";
                wantedBy = [ "multi-user.target" ];
                after = [ "network-online.target" "kcore-bootstrap-certs.service" ];
                wants = [ "network-online.target" "kcore-bootstrap-certs.service" ];
                serviceConfig = {
                  Type = "simple";
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
                qemu_kvm libvirt lvm2 qemu-utils cloud-utils iproute2 jq nodeAgent controllerPkg openssl
                parted
                (pkgs.writeScriptBin "install-to-disk" ''
                  #!/usr/bin/env bash
                  set -euo pipefail

                  # ── Environment variable defaults ──────────────────────────
                  # When KCORE_OS_DISK is set, the installer runs non-interactively.
                  KCORE_OS_DISK="''${KCORE_OS_DISK:-}"
                  KCORE_STORAGE_DISKS="''${KCORE_STORAGE_DISKS:-}"
                  KCORE_HOSTNAME="''${KCORE_HOSTNAME:-kvm-node}"
                  KCORE_ROOT_PASSWORD="''${KCORE_ROOT_PASSWORD:-kcore}"
                  KCORE_SSH_KEYS="''${KCORE_SSH_KEYS:-}"
                  KCORE_RUN_CONTROLLER="''${KCORE_RUN_CONTROLLER:-false}"
                  KCORE_CONTROLLER_ADDRESS="''${KCORE_CONTROLLER_ADDRESS:-}"
                  KCORE_STATUS_FILE="''${KCORE_STATUS_FILE:-}"

                  # ── Progress reporting ─────────────────────────────────────
                  report_status() {
                    local phase="$1"
                    local message="''${2:-}"
                    if [ -n "$KCORE_STATUS_FILE" ]; then
                      printf '{"phase":"%s","message":"%s","timestamp":"%s"}\n' \
                        "$phase" "$message" "$(date -Iseconds)" > "$KCORE_STATUS_FILE"
                    fi
                    echo "[$phase] $message"
                  }

                  # ── Disk selection (interactive vs. non-interactive) ──────
                  if [ -n "$KCORE_OS_DISK" ]; then
                    DISK_PATH="$KCORE_OS_DISK"
                    DISK=$(basename "$DISK_PATH")
                    if [ ! -b "$DISK_PATH" ]; then
                      report_status "FAILED" "$DISK_PATH is not a valid block device"
                      exit 1
                    fi
                  else
                    echo "╔══════════════════════════════════════════════════════════╗"
                    echo "║        KCORE Node - Automated Disk Installer            ║"
                    echo "╚══════════════════════════════════════════════════════════╝"
                    echo ""
                    echo "⚠️  This will ERASE the selected disk and install NixOS!"
                    echo ""

                    echo "Available disks:"
                    lsblk -d -o NAME,SIZE,TYPE,MODEL | grep disk
                    echo ""

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

                    # Detect SSH keys from live environment when interactive
                    if [ -z "$KCORE_SSH_KEYS" ] && [ -f /root/.ssh/authorized_keys ]; then
                      KCORE_SSH_KEYS=$(cat /root/.ssh/authorized_keys)
                    fi
                  fi

                  # ── Phase: PARTITIONING ────────────────────────────────────
                  report_status "PARTITIONING" "Preparing OS disk $DISK_PATH"

                  for vg in $(vgs --noheadings -o vg_name 2>/dev/null || true); do
                    vgchange -an "$vg" 2>/dev/null || true
                  done

                  for part in "$DISK_PATH"*; do
                    if [ -b "$part" ]; then
                      umount "$part" 2>/dev/null || true
                    fi
                  done

                  for i in {1..3}; do
                    wipefs -af "$DISK_PATH" && break || sleep 2
                  done

                  parted -s "$DISK_PATH" mklabel gpt
                  parted -s "$DISK_PATH" mkpart ESP fat32 1MiB 512MiB
                  parted -s "$DISK_PATH" set 1 esp on
                  parted -s "$DISK_PATH" mkpart primary ext4 512MiB 100%

                  sleep 2
                  partprobe "$DISK_PATH" || true
                  sleep 2

                  if [[ "$DISK" == nvme* ]] || [[ "$DISK" == mmcblk* ]]; then
                    BOOT_PART="''${DISK_PATH}p1"
                    ROOT_PART="''${DISK_PATH}p2"
                  else
                    BOOT_PART="''${DISK_PATH}1"
                    ROOT_PART="''${DISK_PATH}2"
                  fi

                  # ── Phase: FORMATTING ──────────────────────────────────────
                  report_status "FORMATTING" "Formatting OS partitions"

                  mkfs.fat -F 32 -n BOOT "$BOOT_PART"
                  mkfs.ext4 -F -L nixos "$ROOT_PART"

                  mkdir -p /mnt
                  mount "$ROOT_PART" /mnt
                  mkdir -p /mnt/boot
                  mount "$BOOT_PART" /mnt/boot

                  # ── Phase: CONFIGURING_STORAGE (optional) ──────────────────
                  if [ -n "$KCORE_STORAGE_DISKS" ]; then
                    report_status "CONFIGURING_STORAGE" "Preparing storage disks for LVM"

                    IFS=',' read -ra SDISKS <<< "$KCORE_STORAGE_DISKS"
                    PV_DEVS=()
                    for sdisk in "''${SDISKS[@]}"; do
                      sdisk=$(echo "$sdisk" | xargs)
                      if [ ! -b "$sdisk" ]; then
                        report_status "FAILED" "Storage disk $sdisk is not a valid block device"
                        exit 1
                      fi
                      wipefs -af "$sdisk"
                      pvcreate -ff -y "$sdisk"
                      PV_DEVS+=("$sdisk")
                    done

                    vgcreate kcore-storage "''${PV_DEVS[@]}"
                    echo "VG kcore-storage created with: ''${PV_DEVS[*]}"
                  fi

                  # ── Phase: INSTALLING_OS ───────────────────────────────────
                  report_status "INSTALLING_OS" "Generating NixOS configuration"

                  nixos-generate-config --root /mnt

                  # Copy node-agent binary
                  mkdir -p /mnt/opt/kcore/bin
                  NODE_AGENT_BIN="${nodeAgent}/bin/kcore-node-agent"
                  if [ ! -f "$NODE_AGENT_BIN" ] || [ ! -x "$NODE_AGENT_BIN" ]; then
                    if command -v kcore-node-agent >/dev/null 2>&1; then
                      NODE_AGENT_BIN=$(command -v kcore-node-agent)
                    else
                      report_status "FAILED" "kcore-node-agent binary not found"
                      exit 1
                    fi
                  fi
                  cp "$NODE_AGENT_BIN" /mnt/opt/kcore/bin/kcore-node-agent
                  chmod +x /mnt/opt/kcore/bin/kcore-node-agent

                  # Copy controller binary when controller role is enabled
                  if [ "$KCORE_RUN_CONTROLLER" = "true" ]; then
                    CONTROLLER_BIN="${controllerPkg}/bin/kcore-controller"
                    if [ ! -f "$CONTROLLER_BIN" ] || [ ! -x "$CONTROLLER_BIN" ]; then
                      if command -v kcore-controller >/dev/null 2>&1; then
                        CONTROLLER_BIN=$(command -v kcore-controller)
                      else
                        report_status "FAILED" "kcore-controller binary not found"
                        exit 1
                      fi
                    fi
                    cp "$CONTROLLER_BIN" /mnt/opt/kcore/bin/kcore-controller
                    chmod +x /mnt/opt/kcore/bin/kcore-controller
                  fi

                  # Copy kcore config and certs from the live environment
                  if [ -d /etc/kcore ]; then
                    mkdir -p /mnt/etc/kcore
                    cp -r /etc/kcore/* /mnt/etc/kcore/ 2>/dev/null || true
                  fi

                  # ── Phase: CONFIGURING_SERVICES ────────────────────────────
                  report_status "CONFIGURING_SERVICES" "Writing NixOS configuration"

                  # Build SSH authorized-keys block
                  SSH_KEYS_NIX=""
                  if [ -n "$KCORE_SSH_KEYS" ]; then
                    while IFS= read -r _key; do
                      [ -z "$_key" ] && continue
                      SSH_KEYS_NIX="$SSH_KEYS_NIX      \"$_key\"
"
                    done <<< "$KCORE_SSH_KEYS"
                  fi

                  # Firewall ports
                  FW_PORTS="22 9091"
                  if [ "$KCORE_RUN_CONTROLLER" = "true" ]; then
                    FW_PORTS="22 9090 9091"
                  fi

                  # Write base configuration.nix (closing brace added separately)
                  cat > /mnt/etc/nixos/configuration.nix << NIXEOF
{ config, pkgs, ... }:
{
  imports = [ ./hardware-configuration.nix ];

  nix.settings.experimental-features = [ "nix-command" "flakes" ];

  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;

  networking.hostName = "$KCORE_HOSTNAME";
  networking.useDHCP = true;
  networking.firewall.enable = true;
  networking.firewall.allowedTCPPorts = [ $FW_PORTS ];

  users.users.root = {
    initialPassword = "$KCORE_ROOT_PASSWORD";
    openssh.authorizedKeys.keys = [
$SSH_KEYS_NIX    ];
  };
  users.mutableUsers = true;
  users.groups.libvirt = {};

  services.openssh = {
    enable = true;
    listenAddresses = [ { addr = "0.0.0.0"; port = 22; } ];
    settings = {
      PermitRootLogin = "yes";
      PasswordAuthentication = true;
    };
  };

  virtualisation.libvirtd = {
    enable = true;
    qemu.runAsRoot = true;
  };

  systemd.services.virtlogd = {
    wantedBy = [ "multi-user.target" ];
    before = [ "libvirtd.service" ];
  };

  boot.kernelModules = [ "kvm" "kvm-intel" "kvm-amd" "br_netfilter" "tap" ];

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
  };

  systemd.tmpfiles.rules = [
    "d /var/lib/kcore 0755 root root -"
    "d /var/lib/kcore/disks 0755 root root -"
    "d /opt/kcore 0755 root root -"
    "d /opt/kcore/bin 0755 root root -"
    "d /etc/kcore 0755 root root -"
  ];

  environment.systemPackages = with pkgs; [
    vim htop curl wget iproute2
    qemu_kvm libvirt lvm2 parted openssl cloud-utils
  ];

  system.stateVersion = "25.05";
}
NIXEOF

                  # Conditionally append controller service before the closing brace
                  if [ "$KCORE_RUN_CONTROLLER" = "true" ]; then
                    # Remove final closing brace, append controller block, re-close
                    sed -i '$ d' /mnt/etc/nixos/configuration.nix
                    cat >> /mnt/etc/nixos/configuration.nix << 'CTRLEOF'

  systemd.services.kcore-controller = {
    description = "kcore Controller";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    unitConfig = {
      ConditionPathExists = [
        "/etc/kcore/ca.crt"
        "/etc/kcore/node.crt"
        "/etc/kcore/node.key"
      ];
    };
    serviceConfig = {
      Type = "simple";
      ExecStart = "/opt/kcore/bin/kcore-controller --listen :9090 --cert /etc/kcore/node.crt --key /etc/kcore/node.key --ca /etc/kcore/ca.crt --db /var/lib/kcore/controller.db";
      Restart = "always";
      RestartSec = "10s";
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
      ProtectHome = true;
      ReadWritePaths = [ "/var/lib/kcore" ];
      ReadOnlyPaths = [ "/etc/kcore" ];
      User = "root";
      LimitNOFILE = 65536;
      LimitNPROC = 4096;
    };
  };
}
CTRLEOF
                  fi

                  # Configure Nix flakes
                  mkdir -p /mnt/etc/nix
                  echo "experimental-features = nix-command flakes" > /mnt/etc/nix/nix.conf

                  # Node-agent configuration with optional controller registration
                  mkdir -p /mnt/etc/kcore
                  CTRL_ADDR=""
                  if [ -n "$KCORE_CONTROLLER_ADDRESS" ]; then
                    CTRL_ADDR="$KCORE_CONTROLLER_ADDRESS"
                  fi

                  cat > /mnt/etc/kcore/node-agent.yaml << YAMLEOF
nodeId: $KCORE_HOSTNAME
controllerAddr: "$CTRL_ADDR"

tls:
  caFile: /etc/kcore/ca.crt
  certFile: /etc/kcore/node.crt
  keyFile: /etc/kcore/node.key

networks:
  default: default

storage:
  drivers:
    local-dir:
      type: local-dir
      parameters:
        path: /var/lib/kcore/disks
YAMLEOF

                  report_status "INSTALLING_OS" "Running nixos-install (10-20 minutes)..."
                  export NIX_CONFIG="experimental-features = nix-command flakes"
                  nixos-install --no-root-password

                  # Installed marker
                  mkdir -p /mnt/etc/kcore
                  date -Iseconds > /mnt/etc/kcore/installed

                  report_status "COMPLETE" "Installation finished successfully"

                  echo ""
                  echo "╔══════════════════════════════════════════════════════════╗"
                  echo "║  ✅ Installation complete!                               ║"
                  echo "╚══════════════════════════════════════════════════════════╝"
                  echo ""
                  echo "Login credentials:"
                  echo "  Username: root"
                  echo "  Password: $KCORE_ROOT_PASSWORD"
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
