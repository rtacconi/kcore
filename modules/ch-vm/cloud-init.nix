{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.ch-vm.vms;
  helpers = import ./helpers.nix { inherit lib; };
  inherit (helpers) generateMac;

  mkSeedIso =
    vmName: vmCfg:
    let
      userData =
        if vmCfg.cloudInitUserConfigFile != null then
          vmCfg.cloudInitUserConfigFile
        else
          pkgs.writeText "${vmName}-user-data" ''
            #cloud-config
            hostname: ${vmName}
            users:
              - default
              - name: kcore
                gecos: kcore default user
                groups: [sudo]
                shell: /bin/bash
                lock_passwd: false
            ssh_pwauth: true
            chpasswd:
              expire: false
              users:
                - name: kcore
                  password: kcore
          '';
      networkConfig =
        if vmCfg.cloudInitNetworkConfigFile != null then
          vmCfg.cloudInitNetworkConfigFile
        else
          pkgs.writeText "${vmName}-network-config" ''
            version: 2
            ethernets:
              vmnic0:
                match:
                  macaddress: "${generateMac vmName}"
                set-name: eth0
                dhcp4: true
          '';

      metaData = pkgs.writeText "${vmName}-meta-data" ''
        instance-id: ${vmName}
        local-hostname: ${vmName}
      '';
    in
    pkgs.runCommand "kcore-seed-${vmName}.iso"
      {
        nativeBuildInputs = [ pkgs.cloud-utils ];
      }
      ''
        cloud-localds \
          --network-config ${networkConfig} \
          "$out" ${userData} ${metaData}
      '';
in
{
  config = lib.mkIf cfg.enable {
    systemd.tmpfiles.rules = [
      "d ${cfg.socketDir} 0755 root root -"
      "d /var/lib/kcore/seeds 0755 root root -"
    ];

    environment.etc = lib.mapAttrs' (
      vmName: vmCfg:
      lib.nameValuePair "kcore/seeds/${vmName}.iso" {
        source = mkSeedIso vmName vmCfg;
      }
    ) cfg.virtualMachines;
  };
}
