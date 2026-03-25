{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.ch-vm.vms;

  tapName = vmName: "tap-${vmName}";

  generateMac = vmName: let
    hash = builtins.hashString "sha256" vmName;
    hexChars = lib.stringToCharacters hash;
    byte = n: lib.concatStrings (lib.sublist (n * 2) 2 hexChars);
  in "52:54:00:${byte 0}:${byte 1}:${byte 2}";

  mkVmService = vmName: vmCfg: let
    mac =
      if vmCfg.macAddress != null
      then vmCfg.macAddress
      else generateMac vmName;

    socketPath = "${cfg.socketDir}/${vmName}.sock";
    seedIso = "/etc/kcore/seeds/${vmName}.iso";
    chBin = "${cfg.cloudHypervisorPackage}/bin/cloud-hypervisor";

    chArgs = lib.concatStringsSep " " ([
        "--api-socket ${socketPath}"
        "--cpus boot=${toString vmCfg.cores}"
        "--memory size=${toString vmCfg.memorySize}M"
        "--disk path=${vmCfg.image}"
        "--disk path=${seedIso},readonly=on"
        "--net tap=${tapName vmName},mac=${mac}"
      ]
      ++ vmCfg.extraArgs);
  in {
    description = "kcore VM ${vmName}";
    requires = ["kcore-tap-${vmName}.service"];
    after = ["kcore-tap-${vmName}.service"];
    wantedBy = lib.optionals vmCfg.autoStart ["multi-user.target"];

    serviceConfig = {
      Type = "simple";
      ExecStartPre = "${pkgs.coreutils}/bin/rm -f ${socketPath}";
      ExecStart = "${chBin} ${chArgs}";
      ExecStop = "${pkgs.curl}/bin/curl --unix-socket ${socketPath} -s -X PUT http://localhost/api/v1/vm.power-button";
      TimeoutStopSec = 30;
      Restart =
        if vmCfg.autoStart
        then "always"
        else "no";
      RestartSec = 5;

      Group = "kvm";
      LimitMEMLOCK = "infinity";
    };
  };
in {
  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.virtualMachines != {} -> cfg.gatewayInterface != "";
        message = "ch-vm.vms.gatewayInterface must be set when virtualMachines are defined.";
      }
    ];

    systemd.services =
      lib.mapAttrs' (
        vmName: vmCfg:
          lib.nameValuePair "kcore-vm-${vmName}" (mkVmService vmName vmCfg)
      )
      cfg.virtualMachines;
  };
}
