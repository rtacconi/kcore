{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.ch-vm.vms;

  mkSeedIso = vmName: vmCfg: let
    userData =
      if vmCfg.cloudInitUserConfigFile != null
      then vmCfg.cloudInitUserConfigFile
      else
        pkgs.writeText "${vmName}-user-data" ''
          #cloud-config
          hostname: ${vmName}
        '';

    metaData = pkgs.writeText "${vmName}-meta-data" ''
      instance-id: ${vmName}
      local-hostname: ${vmName}
    '';
  in
    pkgs.runCommand "kcore-seed-${vmName}.iso" {
      nativeBuildInputs = [pkgs.cloud-utils];
    } ''
      cloud-localds \
        ${lib.optionalString (vmCfg.cloudInitNetworkConfigFile != null)
        "--network-config ${vmCfg.cloudInitNetworkConfigFile}"} \
        "$out" ${userData} ${metaData}
    '';
in {
  config = lib.mkIf cfg.enable {
    systemd.tmpfiles.rules = [
      "d ${cfg.socketDir} 0755 root root -"
      "d /var/lib/kcore/seeds 0755 root root -"
    ];

    environment.etc =
      lib.mapAttrs' (
        vmName: vmCfg:
          lib.nameValuePair "kcore/seeds/${vmName}.iso" {
            source = mkSeedIso vmName vmCfg;
          }
      )
      cfg.virtualMachines;
  };
}
