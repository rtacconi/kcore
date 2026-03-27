{
  description = "kcore - declarative VM management with Cloud Hypervisor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin"];

      perSystem = {
        pkgs,
        system,
        ...
      }: let
        rustOverlay = inputs.rust-overlay.overlays.default;
        pkgsWithRust = import inputs.nixpkgs {
          inherit system;
          overlays = [rustOverlay];
        };
        kcoreVersion = builtins.replaceStrings ["\n"] [""] (builtins.readFile ./VERSION);
        rustToolchain = pkgsWithRust.rust-bin.stable.latest.default.override {
          extensions = ["rust-src" "rust-analyzer"];
        };
        craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            (craneLib.filterCargoSources path type)
            || pkgs.lib.hasPrefix "${toString ./.}/proto/" (toString path);
        };

        commonArgs = {
          inherit src;
          pname = "kcore-workspace";
          version = kcoreVersion;
          strictDeps = true;
          nativeBuildInputs = [pkgs.protobuf pkgs.cmake pkgs.perl];
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        kcore-node-agent = craneLib.buildPackage (commonArgs
          // {
            inherit cargoArtifacts;
            pname = "kcore-node-agent";
            cargoExtraArgs = "-p kcore-node-agent";
          });

        kcore-controller = craneLib.buildPackage (commonArgs
          // {
            inherit cargoArtifacts;
            pname = "kcore-controller";
            cargoExtraArgs = "-p kcore-controller";
          });

        kcore-kctl = craneLib.buildPackage (commonArgs
          // {
            inherit cargoArtifacts;
            pname = "kcore-kctl";
            cargoExtraArgs = "-p kcore-kctl";
          });
      in {
        packages = {
          default = kcore-node-agent;
          inherit kcore-node-agent kcore-controller kcore-kctl;
        };

        checks = {
          inherit kcore-node-agent kcore-controller kcore-kctl;
          clippy = craneLib.cargoClippy (commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            });
          fmt-rust = craneLib.cargoFmt {inherit src;};
          vm-module = import ./tests/vm-module.nix {inherit pkgs;};
          fmt-nix = pkgs.runCommand "check-nix-fmt" {nativeBuildInputs = [pkgs.nixfmt-rfc-style pkgs.findutils];} ''
            find ${./.} -name '*.nix' -exec nixfmt --check {} + 2>&1 || {
              echo "Run 'nixfmt' to fix formatting"
              exit 1
            }
            touch $out
          '';
        };

        devShells.default = craneLib.devShell {
          packages =
            [
              pkgs.cargo-audit
              pkgs.protobuf
              pkgs.grpcurl
              pkgs.cmake
              pkgs.perl
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
              pkgs.cloud-hypervisor
            ];
        };
      };

      flake = let
        kcoreVersion = builtins.replaceStrings ["\n"] [""] (builtins.readFile ./VERSION);
        chVmModule = ./modules/ch-vm;
      in {
        nixosModules = {
          ch-vm = chVmModule;
          default = chVmModule;
        };

        nixosConfigurations.kcore-iso = inputs.nixpkgs.lib.nixosSystem {
          system = "x86_64-linux";
          modules = [
            "${inputs.nixpkgs}/nixos/modules/installer/cd-dvd/iso-image.nix"
            ./modules/kcore-minimal.nix
            ./modules/kcore-branding.nix
            (
              {
                config,
                pkgs,
                lib,
                ...
              }: let
                nodeAgent = inputs.self.packages.x86_64-linux.kcore-node-agent;
                controller = inputs.self.packages.x86_64-linux.kcore-controller;
              in {
                system.stateVersion = "25.05";
                nixpkgs.config.allowUnfree = true;

                boot.loader.timeout = lib.mkForce 0;
                boot.loader.systemd-boot.editor = false;
                boot.kernelParams = ["quiet" "loglevel=3"];
                boot.kernelModules = ["kvm" "kvm-intel" "kvm-amd" "tap" "tun" "br_netfilter"];
                services.qemuGuest.enable = true;

                networking.hostName = "kvm-node";
                networking.useDHCP = true;
                networking.firewall.enable = true;
                networking.firewall.allowedTCPPorts = [22 9090 9091];

                users.users.root.initialPassword = "kcore";
                users.mutableUsers = true;

                services.openssh = {
                  enable = true;
                  listenAddresses = [
                    {
                      addr = "0.0.0.0";
                      port = 22;
                    }
                  ];
                  settings = {
                    PermitRootLogin = "yes";
                    PasswordAuthentication = true;
                  };
                };

                systemd.services.kcore-node-agent = {
                  description = "kcore Node Agent";
                  wantedBy = ["multi-user.target"];
                  after = ["network-online.target"];
                  wants = ["network-online.target"];
                  serviceConfig = {
                    Type = "simple";
                    # Live ISO has no bootstrap TLS yet; allow insecure node admin RPCs.
                    ExecStart = "${nodeAgent}/bin/kcore-node-agent --allow-insecure";
                    Environment = "PATH=/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:/run/wrappers/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
                    Restart = "always";
                    RestartSec = "10s";
                    User = "root";
                    LimitNOFILE = 65536;
                  };
                };

                systemd.tmpfiles.rules = [
                  "d /var/lib/kcore 0755 root root -"
                  "d /var/lib/kcore/images 0755 root root -"
                  "d /opt/kcore 0755 root root -"
                  "d /opt/kcore/bin 0755 root root -"
                  "d /etc/kcore 0755 root root -"
                  "d /run/kcore 0755 root root -"
                ];

                environment.systemPackages = [
                  pkgs.cloud-hypervisor
                  pkgs.qemu-utils
                  pkgs.cloud-utils
                  pkgs.iproute2
                  pkgs.jq
                  pkgs.parted
                  pkgs.lvm2
                  nodeAgent
                  controller
                  (pkgs.writeShellScriptBin "install-to-disk" ''
                    set -euo pipefail

                    DISK=""
                    AUTO_YES="false"
                    NON_INTERACTIVE="false"
                    FORCE_WIPE="false"
                    REBOOT_AFTER_INSTALL="false"
                    CONTROLLER_ENDPOINT=""
                    RUN_CONTROLLER="false"
                    DISABLE_VXLAN="false"
                    DATA_DISKS=()

                    while [[ $# -gt 0 ]]; do
                      case "$1" in
                        --disk)
                          DISK="''${2:-}"
                          shift 2
                          ;;
                        --yes)
                          AUTO_YES="true"
                          shift
                          ;;
                        --non-interactive)
                          NON_INTERACTIVE="true"
                          shift
                          ;;
                        --wipe)
                          FORCE_WIPE="true"
                          shift
                          ;;
                        --reboot)
                          REBOOT_AFTER_INSTALL="true"
                          shift
                          ;;
                        --controller)
                          CONTROLLER_ENDPOINT="''${2:-}"
                          shift 2
                          ;;
                        --run-controller)
                          RUN_CONTROLLER="true"
                          shift
                          ;;
                        --data-disk)
                          DATA_DISKS+=("''${2:-}")
                          shift 2
                          ;;
                        --disable-vxlan)
                          DISABLE_VXLAN="true"
                          shift
                          ;;
                        *)
                          echo "Unknown argument: $1"
                          echo "Usage: install-to-disk [--disk /dev/sda] [--data-disk /dev/nvme0n1] [--controller 192.168.40.135[:9090]] [--run-controller] [--disable-vxlan] [--yes --wipe --non-interactive --reboot]"
                          exit 1
                          ;;
                      esac
                    done

                    if [ "$NON_INTERACTIVE" = "true" ] && [ "$AUTO_YES" != "true" ]; then
                      echo "Error: --non-interactive requires --yes"
                      exit 1
                    fi

                    echo "======================================================"
                    echo "  KCORE Node - Automated Disk Installer"
                    echo "======================================================"
                    echo ""
                    echo "WARNING: This will ERASE the selected disk and install NixOS!"
                    echo ""

                    echo "Available disks:"
                    lsblk -d -o NAME,SIZE,TYPE,MODEL | grep disk
                    echo ""

                    if [ -z "$DISK" ]; then
                      read -p "Enter target disk (e.g., sda, nvme0n1, vda): " DISK
                    fi

                    if [[ "$DISK" == /dev/* ]]; then
                      DISK_PATH="$DISK"
                    else
                      DISK_PATH="/dev/$DISK"
                    fi

                    if [ ! -b "$DISK_PATH" ]; then
                      echo "Error: $DISK_PATH is not a valid block device"
                      exit 1
                    fi

                    echo "Selected: $DISK_PATH"
                    lsblk "$DISK_PATH"
                    echo ""

                    if [ "$NON_INTERACTIVE" = "true" ]; then
                      if [ "$FORCE_WIPE" != "true" ]; then
                        echo "Error: --non-interactive requires --wipe"
                        exit 1
                      fi
                      echo "Non-interactive mode: continuing with forced disk wipe."
                    else
                      read -p "THIS WILL ERASE ALL DATA ON $DISK_PATH! Type 'yes' to continue: " CONFIRM
                      if [ "$CONFIRM" != "yes" ]; then
                        echo "Installation cancelled."
                        exit 0
                      fi
                    fi

                    echo ""
                    echo "Preparing disk..."

                    for vg in $(vgs --noheadings -o vg_name 2>/dev/null || true); do
                      vgchange -an "$vg" 2>/dev/null || true
                    done

                    for part in "$DISK_PATH"*; do
                      if [ -b "$part" ]; then
                        umount "$part" 2>/dev/null || true
                      fi
                    done

                    echo "Partitioning disk..."

                    for i in {1..3}; do
                      wipefs -a -f "$DISK_PATH" && break || sleep 2
                    done

                    parted -s "$DISK_PATH" mklabel gpt
                    parted -s "$DISK_PATH" mkpart ESP fat32 1MiB 512MiB
                    parted -s "$DISK_PATH" set 1 esp on
                    parted -s "$DISK_PATH" mkpart primary ext4 512MiB 100%

                    sleep 2
                    partprobe "$DISK_PATH" || true
                    sleep 2

                    if [[ "$DISK" == *nvme* ]] || [[ "$DISK" == *mmcblk* ]]; then
                      BOOT_PART="''${DISK_PATH}p1"
                      ROOT_PART="''${DISK_PATH}p2"
                    else
                      BOOT_PART="''${DISK_PATH}1"
                      ROOT_PART="''${DISK_PATH}2"
                    fi

                    echo "Formatting partitions..."
                    mkfs.fat -F 32 -n BOOT "$BOOT_PART"
                    mkfs.ext4 -F -L nixos "$ROOT_PART"

                    echo "Mounting partitions..."
                    mkdir -p /mnt
                    mount "$ROOT_PART" /mnt
                    mkdir -p /mnt/boot
                    mount "$BOOT_PART" /mnt/boot

                    echo "Generating NixOS hardware configuration..."
                    nixos-generate-config --root /mnt

                    echo "Copying kcore binaries..."
                    mkdir -p /mnt/opt/kcore/bin
                    cp "${nodeAgent}/bin/kcore-node-agent" /mnt/opt/kcore/bin/kcore-node-agent
                    cp "${controller}/bin/kcore-controller" /mnt/opt/kcore/bin/kcore-controller
                    chmod +x /mnt/opt/kcore/bin/kcore-node-agent
                    chmod +x /mnt/opt/kcore/bin/kcore-controller

                    echo "Copying ch-vm module..."
                    mkdir -p /mnt/etc/nixos/modules/ch-vm
                    cp -r ${chVmModule}/* /mnt/etc/nixos/modules/ch-vm/

                    echo "Copying kcore config and certificates..."
                    mkdir -p /mnt/etc/kcore
                    if [ -d /etc/kcore ]; then
                      cp -r /etc/kcore/* /mnt/etc/kcore/ 2>/dev/null || true
                    fi

                    if [ -n "$CONTROLLER_ENDPOINT" ]; then
                      echo "$CONTROLLER_ENDPOINT" > /mnt/etc/kcore/bootstrap-controller-endpoint
                    fi
                    if [ "''${#DATA_DISKS[@]}" -gt 0 ]; then
                      printf "%s\n" "''${DATA_DISKS[@]}" > /mnt/etc/kcore/data-disks
                    fi

                    if [ "$DISABLE_VXLAN" = "true" ]; then
                      touch /mnt/etc/kcore/disable-vxlan
                    fi

                    SSH_KEYS=""
                    if [ -f /root/.ssh/authorized_keys ]; then
                      SSH_KEYS=$(cat /root/.ssh/authorized_keys | sed 's/^/      "/' | sed 's/$/"/' | paste -sd '\n')
                    fi

                    GATEWAY_INTERFACE=$(ip -4 route show default 2>/dev/null | awk 'NR==1 {print $5}')
                    INTERNAL_GATEWAY_IP="10.240.0.1"
                    EXTERNAL_IP=""
                    if [ -n "$GATEWAY_INTERFACE" ]; then
                      EXTERNAL_IP=$(ip -4 -o addr show dev "$GATEWAY_INTERFACE" scope global 2>/dev/null | awk 'NR==1 {print $4}' | cut -d/ -f1)
                    fi
                    if [ -z "$EXTERNAL_IP" ]; then
                      EXTERNAL_IP=$(hostname -I 2>/dev/null | awk '{print $1}')
                    fi
                    if [ -z "$GATEWAY_INTERFACE" ]; then
                      GATEWAY_INTERFACE="eno1"
                    fi
                    if [[ "$EXTERNAL_IP" == 10.240.* ]]; then
                      INTERNAL_GATEWAY_IP="10.241.0.1"
                    fi
                    if [ -z "$EXTERNAL_IP" ]; then
                      EXTERNAL_IP="127.0.0.1"
                    fi

                    # Controller is opt-in only. By default, install as node-agent that joins an existing controller.
                    CONTROLLER_ADDR="$CONTROLLER_ENDPOINT"
                    if [ "$RUN_CONTROLLER" = "true" ]; then
                      CONTROLLER_ADDR="$EXTERNAL_IP:9090"
                    elif [ -z "$CONTROLLER_ENDPOINT" ]; then
                      echo "Error: provide --controller <host:9090> or pass --run-controller"
                      exit 1
                    fi

                    cat > /mnt/etc/kcore/node-agent.yaml << AGENTEOF
nodeId: kvm-node-01
listenAddr: "0.0.0.0:9091"
controllerAddr: "$CONTROLLER_ADDR"
vmSocketDir: /run/kcore
nixConfigPath: /etc/nixos/kcore-vms.nix
tls:
  caFile: /etc/kcore/certs/ca.crt
  certFile: /etc/kcore/certs/node.crt
  keyFile: /etc/kcore/certs/node.key
AGENTEOF

                    cat > /mnt/etc/kcore/controller.yaml << CTRLEOF
listenAddr: "0.0.0.0:9090"
dbPath: /var/lib/kcore/controller.db
defaultNetwork:
  gatewayInterface: $GATEWAY_INTERFACE
  externalIp: $EXTERNAL_IP
  gatewayIp: $INTERNAL_GATEWAY_IP
tls:
  caFile: /etc/kcore/certs/ca.crt
  certFile: /etc/kcore/certs/controller.crt
  keyFile: /etc/kcore/certs/controller.key
  subCaCertFile: /etc/kcore/certs/sub-ca.crt
  subCaKeyFile: /etc/kcore/certs/sub-ca.key
CTRLEOF

                    cat > /mnt/etc/nixos/kcore-vms.nix << 'VMSEOF'
{ ... }:
{
}
VMSEOF

                    # Build controller service block conditionally
                    CONTROLLER_SERVICE=""
                    if [ "$RUN_CONTROLLER" = "true" ]; then
                      read -r -d "" CONTROLLER_SERVICE << 'CTRLSVC' || true
  systemd.services.kcore-controller = {
    description = "kcore Controller";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    serviceConfig = {
      Type = "simple";
      ExecStart = "/opt/kcore/bin/kcore-controller --config /etc/kcore/controller.yaml";
      Restart = "always";
      RestartSec = "10s";
      User = "root";
      LimitNOFILE = 65536;
    };
  };
CTRLSVC
                    fi

                    echo "Writing NixOS configuration..."
                    cat > /mnt/etc/nixos/configuration.nix << NIXEOF
{ config, pkgs, lib, ... }:
{
  imports = [
    ./hardware-configuration.nix
    ./modules/ch-vm
    ./kcore-vms.nix
  ];

  nix.settings.experimental-features = [ "nix-command" "flakes" ];
  nix.nixPath = [ "nixpkgs=${pkgs.path}" ];

  boot.loader.systemd-boot.enable = true;
  boot.loader.efi.canTouchEfiVariables = true;
  boot.kernelModules = [ "kvm" "kvm-intel" "kvm-amd" "tap" "tun" "br_netfilter" ];
  boot.kernel.sysctl."net.ipv4.ip_forward" = 1;

  networking.hostName = "kvm-node";
  networking.useDHCP = true;
  networking.firewall.enable = true;
  networking.firewall.allowedTCPPorts = [ 22 9090 9091 ];

  users.users.root = {
    initialPassword = "kcore";
    openssh.authorizedKeys.keys = [
$SSH_KEYS
    ];
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

  systemd.services.kcore-node-agent = {
    description = "kcore Node Agent";
    wantedBy = [ "multi-user.target" ];
    after = [ "network-online.target" ];
    wants = [ "network-online.target" ];
    serviceConfig = {
      Type = "simple";
      ExecStart = "/opt/kcore/bin/kcore-node-agent --config /etc/kcore/node-agent.yaml";
      Environment = "PATH=/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:/run/wrappers/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
      Restart = "always";
      RestartSec = "10s";
      User = "root";
      LimitNOFILE = 65536;
    };
  };

$CONTROLLER_SERVICE

  systemd.tmpfiles.rules = [
    "d /var/lib/kcore 0755 root root -"
    "d /var/lib/kcore/images 0755 root root -"
    "d /opt/kcore 0755 root root -"
    "d /opt/kcore/bin 0755 root root -"
    "d /etc/kcore 0755 root root -"
    "d /run/kcore 0755 root root -"
  ];

  environment.systemPackages = with pkgs; [
    vim
    htop
    curl
    wget
    iproute2
    cloud-hypervisor
    qemu-utils
    cloud-utils
    jq
    parted
    lvm2
  ];

  system.stateVersion = "25.05";
}
NIXEOF

                    echo "Configuring Nix with flakes support..."
                    mkdir -p /mnt/etc/nix
                    echo "experimental-features = nix-command flakes" > /mnt/etc/nix/nix.conf

                    echo "Installing NixOS (this will take 10-20 minutes)..."
                    export NIX_CONFIG="experimental-features = nix-command flakes"
                    export NIX_PATH="nixos-config=/mnt/etc/nixos/configuration.nix:nixpkgs=${pkgs.path}"
                    nixos-install

                    echo ""
                    echo "======================================================"
                    echo "  Installation complete!"
                    echo "======================================================"
                    echo ""
                    echo "Login credentials:"
                    echo "  Username: root"
                    echo "  Password: kcore"
                    echo ""
                    if [ "$RUN_CONTROLLER" = "true" ]; then
                      echo "This node is configured as a controller + agent."
                      echo "To add more nodes, use:"
                      echo "  kctl node install --node <new-node-ip>:9091 --os-disk /dev/sda --join-controller <this-ip>"
                    else
                      echo "This node is configured as an agent joining controller at: $CONTROLLER_ENDPOINT"
                    fi
                    echo ""
                    if [ "$REBOOT_AFTER_INSTALL" = "true" ]; then
                      echo "Rebooting now..."
                      sync
                      reboot
                    else
                      echo "Remove the USB drive and type: reboot"
                    fi
                  '')
                ];

                environment.etc."kcore/node-agent.yaml" = {
                  text = builtins.concatStringsSep "\n" [
                    "nodeId: kvm-node-01"
                    ''listenAddr: "0.0.0.0:9091"''
                    "vmSocketDir: /run/kcore"
                    "nixConfigPath: /etc/nixos/kcore-vms.nix"
                    ""
                  ];
                  mode = "0644";
                };

                isoImage.volumeID = "KCORE";
                isoImage.makeUsbBootable = true;
                isoImage.makeEfiBootable = true;
                image.fileName = "nixos-kcore-${kcoreVersion}-x86_64-linux.iso";
              }
            )
          ];
        };
      };
    };
}
