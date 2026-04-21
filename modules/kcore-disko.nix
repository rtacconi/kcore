{
  config,
  lib,
  ...
}:

let
  cfg = config.kcore.disko;
  inherit (lib)
    mkOption
    mkEnableOption
    mkIf
    mkMerge
    types
    listToAttrs
    imap0
    optionalAttrs
    sort
    foldl'
    recursiveUpdate
    ;

  dataDiskPartitions =
    idx: _device:
    if cfg.storageBackend == "lvm" then
      {
        lvm = {
          size = "100%";
          content = {
            type = "lvm_pv";
            vg = cfg.lvm.vgName;
          };
        };
      }
    else if cfg.storageBackend == "zfs" then
      {
        zfs = {
          size = "100%";
          content = {
            type = "zfs";
            pool = cfg.zfs.poolName;
          };
        };
      }
    else
      {
        data = {
          size = "100%";
          content = {
            type = "filesystem";
            format = "ext4";
            mountpoint = if idx == 0 then "/var/lib/kcore/volumes" else "/var/lib/kcore/volumes${toString idx}";
          };
        };
      };

  mkDataDisk = idx: device: {
    name = "data${toString idx}";
    value = {
      type = "disk";
      inherit device;
      content = {
        type = "gpt";
        partitions = dataDiskPartitions idx device;
      };
    };
  };

  baseDevices = {
    disk = {
      os = {
        type = "disk";
        device = cfg.osDisk;
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
                passwordFile = cfg.luksPasswordFile;
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
      };
    }
    // listToAttrs (imap0 mkDataDisk cfg.dataDisks);
  }
  // optionalAttrs (cfg.storageBackend == "lvm" && cfg.dataDisks != [ ]) {
    lvm_vg = {
      ${cfg.lvm.vgName} = {
        type = "lvm_vg";
        lvs = { };
      };
    };
  }
  // optionalAttrs (cfg.storageBackend == "zfs" && cfg.dataDisks != [ ]) {
    zpool = {
      ${cfg.zfs.poolName} = {
        type = "zpool";
        datasets = { };
      };
    };
  };

  sortedFragments = sort (
    a: b: if a.priority == b.priority then a.name < b.name else a.priority < b.priority
  ) cfg.controllerFragments;

  mergedControllerDevices = foldl' recursiveUpdate { } (map (f: f.devices) sortedFragments);

  # The node-agent atomically writes the most-recent applied DiskLayout to
  # cfg.persistedLayoutPath. When the file exists and the node is in
  # controller-managed mode, we pull its `disko.devices` attrset into the
  # final layout so nixos-rebuild and disko agree on the realised shape.
  persistedAvailable =
    cfg.managementMode == "controller-managed" && builtins.pathExists cfg.persistedLayoutPath;
  persistedImport = if persistedAvailable then import cfg.persistedLayoutPath else { };
  persistedDevices = persistedImport.disko.devices or { };
in
{
  options.kcore.disko = {
    enable = mkEnableOption "kcore declarative disk layout via disko";

    osDisk = mkOption {
      type = types.str;
      description = "Block device path for the OS disk.";
      example = "/dev/sda";
    };

    luksPasswordFile = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Path to file containing LUKS passphrase (format-time only).";
    };

    dataDisks = mkOption {
      type = types.listOf types.str;
      default = [ ];
      description = "Block device paths for data disks.";
      example = [ "/dev/nvme0n1" ];
    };

    storageBackend = mkOption {
      type = types.enum [
        "filesystem"
        "lvm"
        "zfs"
      ];
      default = "filesystem";
      description = "Storage backend for data disks.";
    };

    managementMode = mkOption {
      type = types.enum [
        "installer-only"
        "controller-managed"
      ];
      default = "installer-only";
      description = "Ownership mode for disko changes.";
    };

    persistedLayoutPath = mkOption {
      type = types.path;
      default = /etc/kcore/disk/current.nix;
      description = ''
        Filesystem path of the most-recent DiskLayout the node-agent applied.
        When present and managementMode = "controller-managed", its
        `disko.devices` attrset is merged over the declared base layout so
        NixOS and disko observe the same realised shape.
      '';
    };

    controllerFragments = mkOption {
      type = types.listOf (
        types.submodule {
          options = {
            name = mkOption {
              type = types.str;
              description = "Stable fragment name used for deterministic ordering.";
            };
            priority = mkOption {
              type = types.int;
              default = 100;
              description = "Lower value applies earlier during recursive merge.";
            };
            devices = mkOption {
              type = types.attrs;
              default = { };
              description = "Partial disko.devices fragment merged into base layout.";
            };
          };
        }
      );
      default = [ ];
      description = "Optional controller-managed disko fragments for day-2 operations.";
    };

    lvm = {
      vgName = mkOption {
        type = types.str;
        default = "vg_kcore";
        description = "LVM volume group name for data disks.";
      };
    };

    zfs = {
      poolName = mkOption {
        type = types.str;
        default = "tank0";
        description = "ZFS pool name for data disks.";
      };
    };
  };

  config = mkIf cfg.enable (mkMerge [
    {
      assertions = [
        {
          assertion = cfg.managementMode == "controller-managed" || cfg.controllerFragments == [ ];
          message = "kcore.disko.controllerFragments requires kcore.disko.managementMode = \"controller-managed\".";
        }
      ];
    }
    {
      disko.devices =
        let
          withFragments = recursiveUpdate baseDevices (
            if cfg.managementMode == "controller-managed" then mergedControllerDevices else { }
          );
        in
        recursiveUpdate withFragments persistedDevices;
    }
  ]);
}
