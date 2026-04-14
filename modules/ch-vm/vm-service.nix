{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.ch-vm.vms;
  helpers = import ./helpers.nix { inherit lib; };
  inherit (helpers) tapName generateMac;

  mkVmService =
    vmName: vmCfg:
    let
      mac = if vmCfg.macAddress != null then vmCfg.macAddress else generateMac vmName;

      socketPath = "${cfg.socketDir}/${vmName}.sock";
      serialSocket = "${cfg.socketDir}/${vmName}.serial.sock";
      seedIso = "/etc/kcore/seeds/${vmName}.iso";
      firmwarePath =
        if cfg.firmwarePath != null then cfg.firmwarePath else "${pkgs.OVMF-cloud-hypervisor.firmware}";
      chBin = "${cfg.cloudHypervisorPackage}/bin/cloud-hypervisor";

      isLvm = vmCfg.storageBackend == "lvm";
      isZfs = vmCfg.storageBackend == "zfs";
      isBlockBackend = isLvm || isZfs;

      lvName = "kcore-${vmName}";
      lvDevice = "/dev/${cfg.lvmVgName}/${lvName}";

      zvolDataset = "${cfg.zfsPoolName}/kcore-${vmName}";
      zvolDevice = "/dev/zvol/${zvolDataset}";

      actualDisk =
        if isLvm then
          lvDevice
        else if isZfs then
          zvolDevice
        else
          toString vmCfg.image;
      actualFormat = if isBlockBackend then "raw" else vmCfg.imageFormat;

      vmDiskArg = "path=${actualDisk},image_type=${actualFormat}";
      seedDiskArg = "path=${seedIso},readonly=on,image_type=raw";

      lvmProvisionScript = pkgs.writeShellScript "lvm-provision-${vmName}" ''
        set -e
        LV_DEVICE="${lvDevice}"
        VG="${cfg.lvmVgName}"
        LV="${lvName}"
        SIZE_BYTES="${toString vmCfg.storageSizeBytes}"

        if [ ! -b "$LV_DEVICE" ]; then
          echo "Creating LV $VG/$LV (''${SIZE_BYTES} bytes)..."
          ${pkgs.lvm2.bin}/bin/lvcreate -y -L "''${SIZE_BYTES}B" -n "$LV" "$VG"
          echo "Converting source image to LV..."
          ${pkgs.qemu-utils}/bin/qemu-img convert \
            -f ${vmCfg.imageFormat} -O raw \
            ${toString vmCfg.image} "$LV_DEVICE"
          echo "LVM volume provisioned: $LV_DEVICE"
        else
          echo "LV $LV_DEVICE already exists, skipping provision"
        fi
      '';

      zfsProvisionScript = pkgs.writeShellScript "zfs-provision-${vmName}" ''
        set -e
        ZVOL_DATASET="${zvolDataset}"
        ZVOL_DEVICE="${zvolDevice}"
        SIZE_BYTES="${toString vmCfg.storageSizeBytes}"

        if ! ${pkgs.zfs}/bin/zfs list -H "$ZVOL_DATASET" >/dev/null 2>&1; then
          echo "Creating zvol $ZVOL_DATASET (''${SIZE_BYTES} bytes)..."
          ${pkgs.zfs}/bin/zfs create -V "''${SIZE_BYTES}" -o volmode=dev "$ZVOL_DATASET"
          # Wait for the device node to appear
          for i in $(seq 1 30); do
            [ -b "$ZVOL_DEVICE" ] && break
            sleep 0.2
          done
          if [ ! -b "$ZVOL_DEVICE" ]; then
            echo "ERROR: zvol device $ZVOL_DEVICE did not appear after create"
            exit 1
          fi
          echo "Converting source image to zvol..."
          ${pkgs.qemu-utils}/bin/qemu-img convert \
            -f ${vmCfg.imageFormat} -O raw \
            ${toString vmCfg.image} "$ZVOL_DEVICE"
          echo "ZFS volume provisioned: $ZVOL_DEVICE"
        else
          echo "zvol $ZVOL_DATASET already exists, skipping provision"
        fi
      '';

      chArgs = lib.concatStringsSep " " (
        [
          "--api-socket ${socketPath}"
          "--cpus boot=${toString vmCfg.cores}"
          "--memory size=${toString vmCfg.memorySize}M"
          "--firmware ${firmwarePath}"
          "--serial socket=${serialSocket}"
          "--disk ${vmDiskArg} ${seedDiskArg}"
          "--net tap=${tapName vmName},mac=${mac}"
        ]
        ++ vmCfg.extraArgs
      );

      basePreChecks = [
        "${pkgs.coreutils}/bin/rm -f ${socketPath} ${serialSocket}"
        "${pkgs.bash}/bin/bash -euc 'test -f ${seedIso} || { echo \"missing cloud-init seed: ${seedIso}\"; exit 1; }'"
        "${pkgs.bash}/bin/bash -euc 'test -f ${firmwarePath} || { echo \"missing firmware: ${firmwarePath}\"; exit 1; }'"
      ];

      sourceImageCheck = "${pkgs.bash}/bin/bash -euc 'test -e ${toString vmCfg.image} || { echo \"missing source image: ${toString vmCfg.image}\"; exit 1; }'";

      lvmPreChecks = [
        sourceImageCheck
        "${lvmProvisionScript}"
      ];
      zfsPreChecks = [
        sourceImageCheck
        "${zfsProvisionScript}"
      ];
      fsPreChecks = [ sourceImageCheck ];

      storagePreChecks =
        if isLvm then
          lvmPreChecks
        else if isZfs then
          zfsPreChecks
        else
          fsPreChecks;
    in
    {
      description = "kcore VM ${vmName}";
      requires = [ "kcore-tap-${vmName}.service" ];
      after = [ "kcore-tap-${vmName}.service" ];
      wantedBy = lib.optionals vmCfg.autoStart [ "multi-user.target" ];
      stopIfChanged = true;

      serviceConfig = {
        Type = "simple";
        ExecStartPre = basePreChecks ++ storagePreChecks;
        ExecStart = "${chBin} ${chArgs}";
        ExecStop = "${pkgs.curl}/bin/curl --unix-socket ${socketPath} -s -X PUT http://localhost/api/v1/vm.power-button";
        TimeoutStopSec = 30;
        Restart = if vmCfg.autoStart then "always" else "no";
        RestartSec = 5;

        Group = "kvm";
        LimitMEMLOCK = "infinity";
      };
    };
  anyVmUsesZfs = lib.any (vm: vm.storageBackend == "zfs") (lib.attrValues cfg.virtualMachines);
  anyVmUsesLvm = lib.any (vm: vm.storageBackend == "lvm") (lib.attrValues cfg.virtualMachines);
in
{
  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.virtualMachines != { } -> cfg.gatewayInterface != "";
        message = "ch-vm.vms.gatewayInterface must be set when virtualMachines are defined.";
      }
    ];

    boot.supportedFilesystems = lib.mkIf anyVmUsesZfs [ "zfs" ];

    services.lvm.enable = lib.mkIf anyVmUsesLvm true;

    systemd.services = lib.mapAttrs' (
      vmName: vmCfg: lib.nameValuePair "kcore-vm-${vmName}" (mkVmService vmName vmCfg)
    ) cfg.virtualMachines;
  };
}
