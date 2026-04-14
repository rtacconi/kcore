{
  description = "kcore - declarative VM management with Cloud Hypervisor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane";
    disko = {
      url = "github:nix-community/disko";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-darwin"
      ];

      perSystem =
        {
          pkgs,
          system,
          ...
        }:
        let
          rustOverlay = inputs.rust-overlay.overlays.default;
          pkgsWithRust = import inputs.nixpkgs {
            inherit system;
            overlays = [ rustOverlay ];
          };
          kcoreVersion = builtins.replaceStrings [ "\n" ] [ "" ] (builtins.readFile ./VERSION);
          rustToolchain = pkgsWithRust.rust-bin.stable.latest.default.override {
            extensions = [
              "rust-src"
              "rust-analyzer"
            ];
          };
          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              (craneLib.filterCargoSources path type)
              || pkgs.lib.hasPrefix "${toString ./.}/proto/" (toString path)
              || pkgs.lib.hasPrefix "${toString ./.}/crates/dashboard/assets/" (toString path);
          };

          commonArgs = {
            inherit src;
            pname = "kcore-workspace";
            version = kcoreVersion;
            strictDeps = true;
            nativeBuildInputs = [
              pkgs.protobuf
              pkgs.cmake
              pkgs.perl
            ];
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          kcore-node-agent = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              pname = "kcore-node-agent";
              cargoExtraArgs = "-p kcore-node-agent";
            }
          );

          kcore-controller = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              pname = "kcore-controller";
              cargoExtraArgs = "-p kcore-controller";
            }
          );

          kcore-kctl = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              pname = "kcore-kctl";
              cargoExtraArgs = "-p kcore-kctl";
            }
          );

          kcore-dashboard = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              pname = "kcore-dashboard";
              cargoExtraArgs = "-p kcore-dashboard";
            }
          );
        in
        {
          packages = {
            default = kcore-node-agent;
            inherit
              kcore-node-agent
              kcore-controller
              kcore-kctl
              kcore-dashboard
              ;
          };

          checks = {
            inherit
              kcore-node-agent
              kcore-controller
              kcore-kctl
              kcore-dashboard
              ;
            clippy = craneLib.cargoClippy (
              commonArgs
              // {
                inherit cargoArtifacts;
                cargoClippyExtraArgs = "--all-targets -- --deny warnings";
              }
            );
            fmt-rust = craneLib.cargoFmt { inherit src; };
            vm-module = import ./tests/vm-module.nix { inherit pkgs; };
            disko-module = import ./tests/disko-module.nix { inherit pkgs; };
            fmt-nix =
              pkgs.runCommand "check-nix-fmt"
                {
                  nativeBuildInputs = [
                    pkgs.nixfmt
                    pkgs.findutils
                  ];
                }
                ''
                  find ${./.} -name '*.nix' -exec nixfmt --check {} + 2>&1 || {
                    echo "Run 'nixfmt' to fix formatting"
                    exit 1
                  }
                  touch $out
                '';
          };

          devShells.default = craneLib.devShell {
            packages = [
              pkgs.cargo-audit
              pkgs.protobuf
              pkgs.grpcurl
              pkgs.cmake
              pkgs.perl
              pkgs.statix
              pkgs.deadnix
              pkgs.nixfmt
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
              pkgs.cloud-hypervisor
            ];
            shellHook = ''
              if [ -d .git ] && [ -d scripts/hooks ]; then
                for hook in scripts/hooks/*; do
                  name="$(basename "$hook")"
                  target=".git/hooks/$name"
                  if [ ! -L "$target" ] || [ "$(readlink "$target")" != "../../$hook" ]; then
                    ln -sf "../../$hook" "$target"
                  fi
                done
              fi
            '';
          };
        };

      flake =
        let
          kcoreVersion = builtins.replaceStrings [ "\n" ] [ "" ] (builtins.readFile ./VERSION);
          chVmModule = ./modules/ch-vm;
          dashboardModule = ./modules/kcore-dashboard.nix;
        in
        {
          nixosModules = {
            ch-vm = chVmModule;
            default = chVmModule;
            kcore-disko = ./modules/kcore-disko.nix;
            kcore-dashboard = dashboardModule;
          };

          nixosConfigurations.kcore-iso = inputs.nixpkgs.lib.nixosSystem {
            system = "x86_64-linux";
            modules = [
              "${inputs.nixpkgs}/nixos/modules/installer/cd-dvd/iso-image.nix"
              ./modules/kcore-minimal.nix
              ./modules/kcore-branding.nix
              (
                {
                  pkgs,
                  lib,
                  ...
                }:
                let
                  nodeAgent = inputs.self.packages.x86_64-linux.kcore-node-agent;
                  controller = inputs.self.packages.x86_64-linux.kcore-controller;
                  kctl = inputs.self.packages.x86_64-linux.kcore-kctl;
                  dashboard = inputs.self.packages.x86_64-linux.kcore-dashboard;
                  diskoPackage = inputs.disko.packages.x86_64-linux.default;
                  kcoreDiskoModule = ./modules/kcore-disko.nix;
                in
                {
                  system.stateVersion = "25.05";
                  nixpkgs.config.allowUnfree = true;

                  boot.loader.timeout = lib.mkForce 0;
                  boot.loader.systemd-boot.editor = false;
                  boot.kernelParams = [
                    "quiet"
                    "loglevel=3"
                  ];
                  boot.kernelModules = [
                    "kvm"
                    "kvm-intel"
                    "kvm-amd"
                    "tap"
                    "tun"
                    "br_netfilter"
                  ];
                  services.qemuGuest.enable = true;

                  networking.hostName = "kvm-node";
                  networking.useDHCP = true;
                  networking.firewall.enable = true;
                  networking.firewall.allowedTCPPorts = [
                    22
                    9090
                    9091
                    8080
                  ];

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
                    wantedBy = [ "multi-user.target" ];
                    after = [ "network-online.target" ];
                    wants = [ "network-online.target" ];
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
                    "d /var/lib/kcore/volumes 0755 root root -"
                    "d /opt/kcore 0755 root root -"
                    "d /opt/kcore/bin 0755 root root -"
                    "d /etc/kcore 0755 root root -"
                    "d /run/kcore 0755 root root -"
                  ];

                  environment.systemPackages = [
                    pkgs.nixos-install-tools
                    pkgs.dosfstools
                    pkgs.e2fsprogs
                    pkgs.cloud-hypervisor
                    pkgs.qemu-utils
                    pkgs.cloud-utils
                    pkgs.iproute2
                    pkgs.jq
                    pkgs.parted
                    pkgs.lvm2
                    pkgs.cryptsetup
                    pkgs.tpm2-tools
                    pkgs.openssl
                    nodeAgent
                    controller
                    kctl
                    dashboard
                    diskoPackage
                    (pkgs.writeShellScriptBin "install-to-disk" ''
                                                                set -euo pipefail

                                                                DISK=""
                                                                AUTO_YES="false"
                                                                NON_INTERACTIVE="false"
                                                                FORCE_WIPE="false"
                                                                REBOOT_AFTER_INSTALL="false"
                                                                CONTROLLER_ENDPOINTS=()
                                                                RUN_CONTROLLER="false"
                                                                DISABLE_VXLAN="false"
                                                                DC_ID="DC1"
                                                                LUKS_METHOD=""
                                                                DATA_DISKS=()
                                                                INSTALL_HOSTNAME=""
                                                                INSTALL_NODE_ID=""
                                                                DATA_DISK_MODE="filesystem"
                                                                LVM_VG_NAME=""
                                                                LVM_LV_PREFIX="kcore-"
                                                                ZFS_POOL_NAME=""
                                                                ZFS_DATASET_PREFIX="kcore-"
                                                                RECOVERY_KEY_OUTPUT=""

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
                                                                      _CTRL_VAL="''${2:-}"
                                                                      if [[ -n "$_CTRL_VAL" && "$_CTRL_VAL" != *:* ]]; then
                                                                        _CTRL_VAL="''${_CTRL_VAL}:9090"
                                                                      fi
                                                                      CONTROLLER_ENDPOINTS+=("$_CTRL_VAL")
                                                                      shift 2
                                                                      ;;
                                                                    --dc-id)
                                                                      DC_ID="''${2:-}"
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
                                                                    --luks-method)
                                                                      LUKS_METHOD="''${2:-}"
                                                                      shift 2
                                                                      ;;
                                                                    --hostname)
                                                                      INSTALL_HOSTNAME="''${2:-}"
                                                                      shift 2
                                                                      ;;
                                                                    --node-id)
                                                                      INSTALL_NODE_ID="''${2:-}"
                                                                      shift 2
                                                                      ;;
                                                                    --data-disk-mode)
                                                                      DATA_DISK_MODE="''${2:-}"
                                                                      shift 2
                                                                      ;;
                                                                    --lvm-vg-name)
                                                                      LVM_VG_NAME="''${2:-}"
                                                                      shift 2
                                                                      ;;
                                                                    --lvm-lv-prefix)
                                                                      LVM_LV_PREFIX="''${2:-}"
                                                                      shift 2
                                                                      ;;
                                                                    --zfs-pool-name)
                                                                      ZFS_POOL_NAME="''${2:-}"
                                                                      shift 2
                                                                      ;;
                                                                    --zfs-dataset-prefix)
                                                                      ZFS_DATASET_PREFIX="''${2:-}"
                                                                      shift 2
                                                                      ;;
                                                                    --recovery-key-output)
                                                                      RECOVERY_KEY_OUTPUT="''${2:-}"
                                                                      shift 2
                                                                      ;;
                                                                    *)
                                                                      echo "Unknown argument: $1"
                                                                      echo "Usage: install-to-disk [--disk /dev/sda] [--data-disk /dev/nvme0n1] [--hostname HOSTNAME] [--node-id ID] [--data-disk-mode filesystem|lvm|zfs] [--lvm-vg-name VG] [--lvm-lv-prefix PREFIX] [--zfs-pool-name POOL] [--zfs-dataset-prefix PREFIX] [--recovery-key-output /path/file.txt] [--controller 192.168.40.135[:9090]]... [--dc-id DC1] [--run-controller] [--disable-vxlan] [--luks-method tpm2|key-file] [--yes --wipe --non-interactive --reboot]"
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

                                                                # Best-effort cleanup from prior interrupted runs so retries
                                                                # remain idempotent in automation environments.
                                                                swapoff -a 2>/dev/null || true
                                                                umount -R /mnt 2>/dev/null || true
                                                                if [ -e /dev/mapper/cryptroot ]; then
                                                                  cryptsetup close cryptroot 2>/dev/null || true
                                                                fi

                                                                # Non-interactive destructive wipe for install targets.
                                                                # This intentionally removes signatures and common residual
                                                                # metadata regions so downstream disko/LVM steps never prompt.
                                                                deep_wipe_device() {
                                                                  local target="$1"
                                                                  if [ ! -b "$target" ]; then
                                                                    echo "skip wipe: not a block device: $target"
                                                                    return 0
                                                                  fi
                                                                  echo "==> deep wipe: $target"
                                                                  wipefs -a "$target" 2>/dev/null || true
                                                                  timeout 180s blkdiscard -f "$target" 2>/dev/null || true
                                                                  dd if=/dev/zero of="$target" bs=1M count=128 conv=fsync,notrunc status=none || true
                                                                  local total_sectors tail_sectors
                                                                  total_sectors=$(blockdev --getsz "$target" 2>/dev/null || echo 0)
                                                                  tail_sectors=$((128 * 1024 * 1024 / 512))
                                                                  if [ "$total_sectors" -gt "$tail_sectors" ]; then
                                                                    dd if=/dev/zero of="$target" bs=512 seek=$((total_sectors - tail_sectors)) count="$tail_sectors" conv=fsync,notrunc status=none || true
                                                                  fi
                                                                  partprobe "$target" 2>/dev/null || true
                                                                }

                                                                for vg in $(vgs --noheadings -o vg_name 2>/dev/null || true); do
                                                                  vgchange -an "$vg" 2>/dev/null || true
                                                                done

                                                                for part in "$DISK_PATH"*; do
                                                                  if [ -b "$part" ]; then
                                                                    umount "$part" 2>/dev/null || true
                                                                  fi
                                                                done

                                                                # Always wipe signatures on target disks for clean re-installs.
                                                                # This prevents stale LUKS/LVM metadata from previous attempts.
                                                                if [ "$FORCE_WIPE" = "true" ]; then
                                                                  deep_wipe_device "$DISK_PATH"
                                                                  for dd in "''${DATA_DISKS[@]}"; do
                                                                    deep_wipe_device "$dd"
                                                                  done
                                                                fi

                                                                # Auto-detect LUKS method if not provided
                                                                if [ -z "$LUKS_METHOD" ]; then
                                                                  if [ -d /sys/class/tpm/tpm0 ]; then
                                                                    LUKS_METHOD="tpm2"
                                                                  else
                                                                    LUKS_METHOD="key-file"
                                                                  fi
                                                                fi

                                                                echo "Disk encryption method: $LUKS_METHOD"

                                                                # Generate LUKS passphrase for disko (hex avoids shell-special chars)
                                                                LUKS_PASSPHRASE=$(${pkgs.openssl}/bin/openssl rand -hex 32)
                                                                mkdir -p /tmp/luks
                                                                printf "%s" "$LUKS_PASSPHRASE" > /tmp/luks/password
                                                                chmod 0400 /tmp/luks/password

                                                                # Compute ROOT_PART path for post-install TPM enrollment
                                                                if [[ "$DISK" == *nvme* ]] || [[ "$DISK" == *mmcblk* ]]; then
                                                                  ROOT_PART="''${DISK_PATH}p2"
                                                                else
                                                                  ROOT_PART="''${DISK_PATH}2"
                                                                fi

                                                                # --- Build disko device configuration ---
                                                                DATA_DISK_NIX=""
                                                                EXTRA_DEVICES_NIX=""
                                                                for i in "''${!DATA_DISKS[@]}"; do
                                                                  dd="''${DATA_DISKS[$i]}"
                                                                  case "$DATA_DISK_MODE" in
                                                                    filesystem)
                                                                      MOUNT="/var/lib/kcore/volumes"
                                                                      if [ "$i" -gt 0 ]; then MOUNT="/var/lib/kcore/volumes$i"; fi
                                                                      DATA_DISK_NIX="$DATA_DISK_NIX
                            data$i = {
                              type = \"disk\";
                              device = \"$dd\";
                              content = {
                                type = \"gpt\";
                                partitions = {
                                  data = {
                                    size = \"100%\";
                                    content = {
                                      type = \"filesystem\";
                                      format = \"ext4\";
                                      mountpoint = \"$MOUNT\";
                                    };
                                  };
                                };
                              };
                            };"
                                                                      ;;
                                                                    lvm)
                                                                      DATA_DISK_NIX="$DATA_DISK_NIX
                            data$i = {
                              type = \"disk\";
                              device = \"$dd\";
                              content = {
                                type = \"gpt\";
                                partitions = {
                                  lvm = {
                                    size = \"100%\";
                                    content = {
                                      type = \"lvm_pv\";
                                      vg = \"$LVM_VG_NAME\";
                                    };
                                  };
                                };
                              };
                            };"
                                                                      if [ -z "$EXTRA_DEVICES_NIX" ]; then
                                                                        EXTRA_DEVICES_NIX="
                          lvm_vg = {
                            $LVM_VG_NAME = {
                              type = \"lvm_vg\";
                              lvs = {};
                            };
                          };"
                                                                      fi
                                                                      ;;
                                                                    zfs)
                                                                      DATA_DISK_NIX="$DATA_DISK_NIX
                            data$i = {
                              type = \"disk\";
                              device = \"$dd\";
                              content = {
                                type = \"gpt\";
                                partitions = {
                                  zfs = {
                                    size = \"100%\";
                                    content = {
                                      type = \"zfs\";
                                      pool = \"$ZFS_POOL_NAME\";
                                    };
                                  };
                                };
                              };
                            };"
                                                                      if [ -z "$EXTRA_DEVICES_NIX" ]; then
                                                                        EXTRA_DEVICES_NIX="
                          zpool = {
                            $ZFS_POOL_NAME = {
                              type = \"zpool\";
                              datasets = {};
                            };
                          };"
                                                                      fi
                                                                      ;;
                                                                  esac
                                                                done

                                                                echo "Generating disko configuration..."
                                                                cat > /tmp/disko-config.nix << DISKOEOF
                      {
                        disko.devices = {
                          disk = {
                            os = {
                              type = "disk";
                              device = "$DISK_PATH";
                              content = {
                                type = "gpt";
                                partitions = {
                                  ESP = {
                                    size = "512M";
                                    type = "EF00";
                                    content = {
                                      type = "filesystem";
                                      format = "vfat";
                                      mountpoint = "/boot";
                                      mountOptions = [ "umask=0077" ];
                                    };
                                  };
                                  root = {
                                    size = "100%";
                                    content = {
                                      type = "luks";
                                      name = "cryptroot";
                                      passwordFile = "/tmp/luks/password";
                                      settings = {
                                        allowDiscards = true;
                                      };
                                      content = {
                                        type = "filesystem";
                                        format = "ext4";
                                        mountpoint = "/";
                                      };
                                    };
                                  };
                                };
                              };
                            };$DATA_DISK_NIX
                          };$EXTRA_DEVICES_NIX
                        };
                      }
                      DISKOEOF

                                                                echo "Running disko (partition, format, mount)..."
                                                                disko --mode format,mount --root-mountpoint /mnt /tmp/disko-config.nix

                                                                # For key-file method, copy passphrase to /boot for initrd unlock
                                                                if [ "$LUKS_METHOD" = "key-file" ]; then
                                                                  cp /tmp/luks/password /mnt/boot/crypto_keyfile.bin
                                                                  chmod 0400 /mnt/boot/crypto_keyfile.bin
                                                                fi

                                                                if [ "''${#DATA_DISKS[@]}" -gt 0 ]; then
                                                                  echo "Data disks formatted via disko (backend: $DATA_DISK_MODE)."
                                                                fi

                                                                echo "Generating NixOS hardware configuration..."
                                                                nixos-generate-config --root /mnt

                                                                echo "Copying kcore binaries..."
                                                                mkdir -p /mnt/opt/kcore/bin
                                                                cp "${nodeAgent}/bin/kcore-node-agent" /mnt/opt/kcore/bin/kcore-node-agent
                                                                cp "${controller}/bin/kcore-controller" /mnt/opt/kcore/bin/kcore-controller
                                                                cp "${kctl}/bin/kcore-kctl" /mnt/opt/kcore/bin/kcore-kctl
                                                                cp "${dashboard}/bin/kcore-dashboard" /mnt/opt/kcore/bin/kcore-dashboard
                                                                chmod +x /mnt/opt/kcore/bin/kcore-node-agent
                                                                chmod +x /mnt/opt/kcore/bin/kcore-controller
                                                                chmod +x /mnt/opt/kcore/bin/kcore-kctl
                                                                chmod +x /mnt/opt/kcore/bin/kcore-dashboard

                                                                echo "Copying ch-vm module..."
                                                                mkdir -p /mnt/etc/nixos/modules/ch-vm
                                                                cp -r ${chVmModule}/* /mnt/etc/nixos/modules/ch-vm/

                                                                echo "Copying disko configuration..."
                                                                cp ${kcoreDiskoModule} /mnt/etc/nixos/modules/kcore-disko.nix
                                                                cp /tmp/disko-config.nix /mnt/etc/nixos/disko-config.nix

                                                                echo "Copying kcore config and certificates..."
                                                                mkdir -p /mnt/etc/kcore
                                                                if [ -d /etc/kcore ]; then
                                                                  cp -r /etc/kcore/* /mnt/etc/kcore/ 2>/dev/null || true
                                                                fi
                                                                # Ensure correct permissions on copied cert material
                                                                if [ -d /mnt/etc/kcore/certs ]; then
                                                                  find /mnt/etc/kcore/certs -name '*.key' -exec chmod 0600 {} + 2>/dev/null || true
                                                                  find /mnt/etc/kcore/certs -name '*.crt' -exec chmod 0644 {} + 2>/dev/null || true
                                                                fi
                                                                if [ "''${#DATA_DISKS[@]}" -gt 0 ]; then
                                                                  printf "%s\n" "''${DATA_DISKS[@]}" > /mnt/etc/kcore/data-disks
                                                                fi
                                                                # Safe split default: installer owns disk layout until explicitly promoted.
                                                                echo "installer-only" > /mnt/etc/kcore/disko-management-mode

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
                                                                  echo "WARNING: no default route detected; defaulting gateway interface to eno1"
                                                                  GATEWAY_INTERFACE="eno1"
                                                                fi
                                                                if [[ "$EXTERNAL_IP" == 10.240.* ]]; then
                                                                  INTERNAL_GATEWAY_IP="10.241.0.1"
                                                                fi
                                                                if [ -z "$EXTERNAL_IP" ]; then
                                                                  echo "ERROR: could not detect external IP address."
                                                                  echo "  No default route, no addresses from 'hostname -I'."
                                                                  echo "  Fix networking before installing, or set the IP manually."
                                                                  exit 1
                                                                fi

                                                                # Auto-generate hostname and nodeId if not provided
                                                                if [ -z "$INSTALL_HOSTNAME" ]; then
                                                                  IP_SUFFIX="''${EXTERNAL_IP//./-}"
                                                                  INSTALL_HOSTNAME="kvm-node-$IP_SUFFIX"
                                                                fi
                                                                if [ -z "$INSTALL_NODE_ID" ]; then
                                                                  INSTALL_NODE_ID="$INSTALL_HOSTNAME"
                                                                fi
                                                                if [ -z "$RECOVERY_KEY_OUTPUT" ]; then
                                                                  RECOVERY_KEY_OUTPUT="/var/log/kcore/recovery-keys/$INSTALL_NODE_ID-$(date +%Y%m%d%H%M%S).txt"
                                                                fi

                                                                # Controller is opt-in only. By default, install as node-agent that joins an existing controller.
                                                                CONTROLLER_ADDR=""
                                                                if [ "$RUN_CONTROLLER" = "true" ]; then
                                                                  CONTROLLER_ADDR="$EXTERNAL_IP:9090"
                                                                  if [ "''${#CONTROLLER_ENDPOINTS[@]}" -eq 0 ]; then
                                                                    CONTROLLER_ENDPOINTS=("$CONTROLLER_ADDR")
                                                                  else
                                                                    CONTROLLER_ENDPOINTS=("$CONTROLLER_ADDR" "''${CONTROLLER_ENDPOINTS[@]}")
                                                                  fi
                                                                elif [ "''${#CONTROLLER_ENDPOINTS[@]}" -eq 0 ]; then
                                                                  echo "Error: provide --controller <host:9090> or pass --run-controller"
                                                                  exit 1
                                                                else
                                                                  CONTROLLER_ADDR="''${CONTROLLER_ENDPOINTS[0]}"
                                                                fi

                                                                # Deduplicate endpoints (preserving order, first occurrence wins)
                                                                _SEEN=""
                                                                _DEDUPED=()
                                                                for ctrl in "''${CONTROLLER_ENDPOINTS[@]}"; do
                                                                  if [[ "|$_SEEN|" != *"|$ctrl|"* ]]; then
                                                                    _DEDUPED+=("$ctrl")
                                                                    _SEEN="$_SEEN|$ctrl"
                                                                  fi
                                                                done
                                                                CONTROLLER_ENDPOINTS=("''${_DEDUPED[@]}")

                                                                # Write bootstrap controller endpoint (after merge, so correct for all modes)
                                                                echo "''${CONTROLLER_ENDPOINTS[0]}" > /mnt/etc/kcore/bootstrap-controller-endpoint

                                                                CONTROLLERS_YAML=""
                                                                for ctrl in "''${CONTROLLER_ENDPOINTS[@]}"; do
                                                                  CONTROLLERS_YAML="$CONTROLLERS_YAML
                                              - \"$ctrl\""
                                                                done

                                                                STORAGE_YAML="storage:
                                              backend: $DATA_DISK_MODE
                                              imageCacheDir: /var/lib/kcore/images"
                                                                case "$DATA_DISK_MODE" in
                                                                  filesystem)
                                                                    STORAGE_YAML="$STORAGE_YAML
                                              filesystemVolumeDir: /var/lib/kcore/volumes"
                                                                    ;;
                                                                  lvm)
                                                                    STORAGE_YAML="$STORAGE_YAML
                                              lvm:
                                                vgName: $LVM_VG_NAME
                                                lvPrefix: $LVM_LV_PREFIX"
                                                                    ;;
                                                                  zfs)
                                                                    STORAGE_YAML="$STORAGE_YAML
                                              zfs:
                                                poolName: $ZFS_POOL_NAME
                                                datasetPrefix: $ZFS_DATASET_PREFIX"
                                                                    ;;
                                                                  *)
                                                                    echo "Error: unknown --data-disk-mode: $DATA_DISK_MODE (expected filesystem, lvm, or zfs)"
                                                                    exit 1
                                                                    ;;
                                                                esac

                                                                cat > /mnt/etc/kcore/node-agent.yaml <<AGENTEOF
                      nodeId: $INSTALL_NODE_ID
                      listenAddr: "0.0.0.0:9091"
                      controllerAddr: "$CONTROLLER_ADDR"
                      controllers:$CONTROLLERS_YAML
                      dcId: "$DC_ID"
                      vmSocketDir: /run/kcore
                      nixConfigPath: /etc/nixos/kcore-vms.nix
                      tls:
                        caFile: /etc/kcore/certs/ca.crt
                        certFile: /etc/kcore/certs/node.crt
                        keyFile: /etc/kcore/certs/node.key
                      $STORAGE_YAML
                      AGENTEOF

                                                                if [ "$RUN_CONTROLLER" = "true" ]; then
                                                                  cat > /mnt/etc/kcore/controller.yaml <<CTRLEOF
                      listenAddr: "0.0.0.0:9090"
                      dbPath: /var/lib/kcore/controller.db
                      defaultNetwork:
                        gatewayInterface: br0
                        externalIp: $EXTERNAL_IP
                        gatewayIp: $INTERNAL_GATEWAY_IP
                      tls:
                        caFile: /etc/kcore/certs/ca.crt
                        certFile: /etc/kcore/certs/controller.crt
                        keyFile: /etc/kcore/certs/controller.key
                        subCaCertFile: /etc/kcore/certs/sub-ca.crt
                        subCaKeyFile: /etc/kcore/certs/sub-ca.key
                      CTRLEOF

                                                                  # Always write replication config so the controller emits
                                                                  # controller.register and runs peer discovery even when
                                                                  # starting as the first (solo) controller.
                                                                  REPL_PEERS_YAML=""
                                                                  for ctrl in "''${CONTROLLER_ENDPOINTS[@]}"; do
                                                                    if [ "$ctrl" != "$CONTROLLER_ADDR" ]; then
                                                                      REPL_PEERS_YAML="''${REPL_PEERS_YAML}
                        - \"$ctrl\""
                                                                    fi
                                                                  done
                                                                  if [ -z "$REPL_PEERS_YAML" ]; then
                                                                    REPL_PEERS_YAML=" []"
                                                                  fi
                                                                  cat >> /mnt/etc/kcore/controller.yaml <<REPLEOF
                      replication:
                        controllerId: "kcore-controller-$EXTERNAL_IP"
                        dcId: "$DC_ID"
                        peers:$REPL_PEERS_YAML
                      REPLEOF
                                                                fi

                                                                if [ "$RUN_CONTROLLER" = "true" ]; then
                                                                  cat > /mnt/etc/kcore/dashboard.env <<DASHENV
                      KCORE_CONTROLLER=$EXTERNAL_IP:9090
                      KCORE_CA_FILE=/etc/kcore/certs/ca.crt
                      KCORE_CERT_FILE=/etc/kcore/certs/controller.crt
                      KCORE_KEY_FILE=/etc/kcore/certs/controller.key
                      LEPTOS_SITE_ADDR=0.0.0.0:8080
                      LEPTOS_ENV=PROD
                      DASHENV
                                                                  chmod 0644 /mnt/etc/kcore/dashboard.env
                                                                fi

                                                                cat > /mnt/etc/nixos/kcore-vms.nix <<'VMSEOF'
                      { ... }:
                      {
                      }
                      VMSEOF

                                                                # Build firewall and ordering blocks before configuration.nix heredoc
                                                                EXTRA_TCP_PORTS=""
                                                                EXTRA_UDP_PORTS=""
                                                                NODE_AGENT_AFTER='"network-online.target"'
                                                                if [ "$RUN_CONTROLLER" = "true" ]; then
                                                                  EXTRA_TCP_PORTS="9090 8080"
                                                                  NODE_AGENT_AFTER='"network-online.target" "kcore-controller.service"'
                                                                fi
                                                                if [ "$DISABLE_VXLAN" != "true" ]; then
                                                                  EXTRA_UDP_PORTS="4789"
                                                                fi

                                                                # Build controller service block conditionally
                                                                CONTROLLER_SERVICE=""
                                                                if [ "$RUN_CONTROLLER" = "true" ]; then
                                                                  CONTROLLER_SERVICE=$(cat <<'CTRLSVC'
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
                                                                  )
                                                                fi

                                                                DASHBOARD_SERVICE=""
                                                                if [ "$RUN_CONTROLLER" = "true" ]; then
                                                                  DASHBOARD_SERVICE=$(cat <<'DASHSVC'
                                              systemd.services.kcore-dashboard = {
                                                description = "kcore Dashboard";
                                                wantedBy = [ "multi-user.target" ];
                                                after = [ "network-online.target" "kcore-controller.service" ];
                                                wants = [ "network-online.target" ];
                                                serviceConfig = {
                                                  Type = "simple";
                                                  EnvironmentFile = "/etc/kcore/dashboard.env";
                                                  ExecStart = "/opt/kcore/bin/kcore-dashboard";
                                                  Restart = "always";
                                                  RestartSec = "10s";
                                                  User = "root";
                                                  LimitNOFILE = 65536;
                                                };
                                              };
                      DASHSVC
                                                                  )
                                                                fi

                                                                # Build LUKS boot config for NixOS
                                                                ROOT_PART_UUID=$(blkid -s UUID -o value "$ROOT_PART")
                                                                LUKS_BOOT_CONFIG=""
                                                                if [ "$LUKS_METHOD" = "tpm2" ]; then
                                                                  LUKS_BOOT_CONFIG=$(cat <<'LUKSEOF'
                                              boot.initrd.luks.devices.cryptroot = {
                                                device = "/dev/disk/by-uuid/ROOT_PART_UUID_PLACEHOLDER";
                                                preLVM = true;
                                                crypttabExtraOpts = [ "tpm2-device=auto" ];
                                              };
                                              boot.initrd.systemd.enable = true;
                      LUKSEOF
                                                                  )
                                                                  LUKS_BOOT_CONFIG="''${LUKS_BOOT_CONFIG//ROOT_PART_UUID_PLACEHOLDER/$ROOT_PART_UUID}"
                                                                else
                                                                  LUKS_BOOT_CONFIG=$(cat <<'LUKSEOF'
                                              boot.initrd.luks.devices.cryptroot = {
                                                device = "/dev/disk/by-uuid/ROOT_PART_UUID_PLACEHOLDER";
                                                preLVM = true;
                                                keyFile = "/crypto_keyfile.bin";
                                              };
                                              boot.initrd.secrets."/crypto_keyfile.bin" = "/boot/crypto_keyfile.bin";
                      LUKSEOF
                                                                  )
                                                                  LUKS_BOOT_CONFIG="''${LUKS_BOOT_CONFIG//ROOT_PART_UUID_PLACEHOLDER/$ROOT_PART_UUID}"
                                                                fi

                                                                # --- Storage-specific NixOS configuration ---
                                                                STORAGE_BOOT_CONFIG=""
                                                                HOST_ID=$(echo "$INSTALL_HOSTNAME" | md5sum | head -c 8)
                                                                case "$DATA_DISK_MODE" in
                                                                  lvm)
                                                                    STORAGE_BOOT_CONFIG=$(cat <<'STOREOF'
                        services.lvm.enable = true;
                      STOREOF
                                                                    )
                                                                    ;;
                                                                  zfs)
                                                                    STORAGE_BOOT_CONFIG=$(cat <<STOREOF
                        boot.supportedFilesystems = [ "zfs" ];
                        boot.zfs.extraPools = [ "$ZFS_POOL_NAME" ];
                        networking.hostId = "$HOST_ID";
                      STOREOF
                                                                    )
                                                                    ;;
                                                                esac

                                                                echo "Writing NixOS configuration..."
                                                                cat > /mnt/etc/nixos/configuration.nix <<NIXEOF
                      { config, pkgs, lib, ... }:
                      {
                        # disko-config.nix and modules/kcore-disko.nix are saved for
                        # reference and day-2 data-disk operations (see docs/storage.md).
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

                      $STORAGE_BOOT_CONFIG

                      $LUKS_BOOT_CONFIG

                        networking.hostName = "$INSTALL_HOSTNAME";
                        networking.useDHCP = false;
                        networking.bridges.br0.interfaces = [ "$GATEWAY_INTERFACE" ];
                        networking.interfaces.br0.useDHCP = true;
                        networking.interfaces."$GATEWAY_INTERFACE".useDHCP = false;
                        networking.firewall.enable = true;
                        networking.firewall.allowedTCPPorts = [ 22 9091 $EXTRA_TCP_PORTS ];
                        networking.firewall.allowedUDPPorts = [ $EXTRA_UDP_PORTS ];

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
                          after = [ $NODE_AGENT_AFTER ];
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

                      $DASHBOARD_SERVICE

                        systemd.tmpfiles.rules = [
                          "d /var/lib/kcore 0755 root root -"
                          "d /var/lib/kcore/images 0755 root root -"
                          "d /var/lib/kcore/volumes 0755 root root -"
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
                          cryptsetup
                          tpm2-tools
                          disko
                        ];

                        system.activationScripts.kcore-kctl-path.text = "mkdir -p /usr/local/bin\nln -sfn /opt/kcore/bin/kcore-kctl /usr/local/bin/kctl\nln -sfn /opt/kcore/bin/kcore-kctl /usr/local/bin/kcore-kctl\n";

                        system.stateVersion = "25.05";
                      }
                      NIXEOF

                                                                echo "Installing NixOS (this will take 10-20 minutes)..."
                                                                export NIX_CONFIG="experimental-features = nix-command flakes"
                                                                export NIX_PATH="nixos-config=/mnt/etc/nixos/configuration.nix:nixpkgs=${pkgs.path}"
                                                                nixos-install --no-root-passwd

                                                                # Enroll TPM2 and add recovery key
                                                                if [ "$LUKS_METHOD" = "tpm2" ]; then
                                                                  echo "Enrolling TPM2 for LUKS..."
                                                                  if ! timeout 120s systemd-cryptenroll --unlock-key-file /tmp/luks/password --tpm2-device=auto --tpm2-pcrs=7 "$ROOT_PART"; then
                                                                    echo "Error: TPM2 enrollment failed or timed out"
                                                                    exit 1
                                                                  fi
                                                                  echo ""
                                                                  echo "Adding and persisting LUKS recovery key..."
                                                                  if ! RECOVERY_ENROLL_OUTPUT=$(timeout 120s systemd-cryptenroll --unlock-key-file /tmp/luks/password --recovery-key "$ROOT_PART" 2>&1); then
                                                                    echo "$RECOVERY_ENROLL_OUTPUT"
                                                                    echo "Error: failed to add LUKS recovery key"
                                                                    exit 1
                                                                  fi
                                                                  RECOVERY_KEY=$(printf "%s\n" "$RECOVERY_ENROLL_OUTPUT" | grep -Eo '[a-z0-9]{8}(-[a-z0-9]{8}){7}' | head -n 1 || true)
                                                                  if [ -z "$RECOVERY_KEY" ]; then
                                                                    echo "Warning: could not parse recovery key from systemd-cryptenroll output."
                                                                    echo "$RECOVERY_ENROLL_OUTPUT"
                                                                    RECOVERY_KEY="UNAVAILABLE"
                                                                    RECOVERY_KEY_FINGERPRINT="unavailable"
                                                                  else
                                                                    RECOVERY_KEY_FINGERPRINT=$(printf "%s" "$RECOVERY_KEY" | sha256sum | awk '{print $1}')
                                                                  fi
                                                                  RECOVERY_KEY_TMP=$(mktemp)
                                                                  cat > "$RECOVERY_KEY_TMP" <<EOF
                      nodeId: $INSTALL_NODE_ID
                      hostname: $INSTALL_HOSTNAME
                      disk: $DISK_PATH
                      rootPart: $ROOT_PART
                      rootUuid: $ROOT_PART_UUID
                      luksMethod: tpm2
                      createdAtUtc: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
                      recoveryKey: $RECOVERY_KEY
                      recoveryKeySha256: $RECOVERY_KEY_FINGERPRINT
                      recoveryEnrollOutput: |
                      $(printf "%s\n" "$RECOVERY_ENROLL_OUTPUT" | sed 's/^/  /')
                      EOF
                                                                  chmod 0400 "$RECOVERY_KEY_TMP"
                                                                  install -d -m 0700 /mnt/etc/kcore/recovery
                                                                  install -m 0400 "$RECOVERY_KEY_TMP" /mnt/etc/kcore/recovery/luks-recovery-key.txt
                                                                  install -d -m 0700 "$(dirname "$RECOVERY_KEY_OUTPUT")"
                                                                  install -m 0400 "$RECOVERY_KEY_TMP" "$RECOVERY_KEY_OUTPUT"
                                                                  rm -f "$RECOVERY_KEY_TMP"
                                                                  if ! timeout 120s systemd-cryptenroll --unlock-key-file /tmp/luks/password --wipe-slot=password "$ROOT_PART"; then
                                                                    echo "Warning: failed to remove temporary LUKS password slot"
                                                                  fi
                                                                  rm -f /tmp/luks/password
                                                                  unset LUKS_PASSPHRASE
                                                                  echo "TPM2 enrolled, temporary passphrase replaced with recovery key."
                                                                  echo "Recovery key artifact saved on installed node: /etc/kcore/recovery/luks-recovery-key.txt"
                                                                  echo "Recovery key artifact saved on live env: $RECOVERY_KEY_OUTPUT"
                                                                  echo "Recovery key fingerprint (sha256): $RECOVERY_KEY_FINGERPRINT"
                                                                fi

                                                                echo ""
                                                                echo "======================================================"
                                                                echo "  Installation complete!"
                                                                echo "======================================================"
                                                                echo ""
                                                                echo "Login credentials:"
                                                                echo "  Username: root"
                                                                echo "  Password: kcore"
                                                                echo ""
                                                                echo "Disk encryption: LUKS2 ($LUKS_METHOD)"
                                                                echo ""
                                                                if [ "$RUN_CONTROLLER" = "true" ]; then
                                                                  echo "This node is configured as a controller + agent."
                                                                  echo "To add more nodes, use:"
                                                                  echo "  kctl node install --node <new-node-ip>:9091 --os-disk /dev/sda --join-controller <this-ip>"
                                                                else
                                                                  echo "This node is configured as an agent joining controller at: $CONTROLLER_ADDR"
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
