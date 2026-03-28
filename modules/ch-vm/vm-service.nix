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
      vmDiskArg = "path=${toString vmCfg.image},image_type=${vmCfg.imageFormat}";
      seedDiskArg = "path=${seedIso},readonly=on,image_type=raw";

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
    in
    {
      description = "kcore VM ${vmName}";
      requires = [ "kcore-tap-${vmName}.service" ];
      after = [ "kcore-tap-${vmName}.service" ];
      wantedBy = lib.optionals vmCfg.autoStart [ "multi-user.target" ];

      serviceConfig = {
        Type = "simple";
        ExecStartPre = [
          "${pkgs.coreutils}/bin/rm -f ${socketPath} ${serialSocket}"
          "${pkgs.bash}/bin/bash -euc 'test -e ${toString vmCfg.image} || { echo \"missing VM image: ${toString vmCfg.image}\"; exit 1; }'"
          "${pkgs.bash}/bin/bash -euc 'test -f ${seedIso} || { echo \"missing cloud-init seed: ${seedIso}\"; exit 1; }'"
          "${pkgs.bash}/bin/bash -euc 'test -f ${firmwarePath} || { echo \"missing firmware: ${firmwarePath}\"; exit 1; }'"
        ];
        ExecStart = "${chBin} ${chArgs}";
        ExecStop = "${pkgs.curl}/bin/curl --unix-socket ${socketPath} -s -X PUT http://localhost/api/v1/vm.power-button";
        TimeoutStopSec = 30;
        Restart = if vmCfg.autoStart then "always" else "no";
        RestartSec = 5;

        Group = "kvm";
        LimitMEMLOCK = "infinity";
      };
    };
in
{
  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.virtualMachines != { } -> cfg.gatewayInterface != "";
        message = "ch-vm.vms.gatewayInterface must be set when virtualMachines are defined.";
      }
    ];

    systemd.services = lib.mapAttrs' (
      vmName: vmCfg: lib.nameValuePair "kcore-vm-${vmName}" (mkVmService vmName vmCfg)
    ) cfg.virtualMachines;
  };
}
