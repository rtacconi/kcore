{
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.ch-vm.vms;

  bridgeName = name: "kbr-${name}";
  tapName = vmName: "tap-${vmName}";

  netmaskToCidr = mask:
    {
      "255.0.0.0" = "8";
      "255.255.0.0" = "16";
      "255.255.128.0" = "17";
      "255.255.192.0" = "18";
      "255.255.224.0" = "19";
      "255.255.240.0" = "20";
      "255.255.248.0" = "21";
      "255.255.252.0" = "22";
      "255.255.254.0" = "23";
      "255.255.255.0" = "24";
      "255.255.255.128" = "25";
      "255.255.255.192" = "26";
      "255.255.255.224" = "27";
      "255.255.255.240" = "28";
      "255.255.255.248" = "29";
      "255.255.255.252" = "30";
    }
    .${mask}
    or (throw "unsupported netmask: ${mask}");
in {
  config = lib.mkIf cfg.enable {
    assertions =
      lib.mapAttrsToList (vmName: vmCfg: {
        assertion = cfg.networks ? ${vmCfg.network};
        message = "VM '${vmName}' references network '${vmCfg.network}' which is not defined in ch-vm.vms.networks.";
      })
      cfg.virtualMachines;

    boot.kernelModules = ["tun" "tap" "br_netfilter"];

    networking.nftables.enable = true;

    systemd.services =
      lib.mapAttrs' (
        netName: netCfg:
          lib.nameValuePair "kcore-bridge-${netName}" {
            description = "kcore bridge for network ${netName}";
            wantedBy = ["multi-user.target"];
            before =
              lib.mapAttrsToList (
                vmName: vmCfg:
                  "kcore-vm-${vmName}.service"
              ) (lib.filterAttrs (_: vm: vm.network == netName) cfg.virtualMachines);

            serviceConfig = {
              Type = "oneshot";
              RemainAfterExit = true;
            };

            path = [pkgs.iproute2 pkgs.nftables];

            script = ''
              bridge="${bridgeName netName}"
              ip link show "$bridge" >/dev/null 2>&1 && exit 0
              ip link add "$bridge" type bridge
              ip addr add ${netCfg.gatewayIP}/${netmaskToCidr netCfg.internalNetmask} dev "$bridge"
              ip link set "$bridge" up

              nft add table ip kcore-${netName} 2>/dev/null || true
              nft add chain ip kcore-${netName} postrouting '{ type nat hook postrouting priority srcnat; }'
              nft add rule ip kcore-${netName} postrouting oifname "${cfg.gatewayInterface}" masquerade
            '';

            preStop = ''
              bridge="${bridgeName netName}"
              nft delete table ip kcore-${netName} 2>/dev/null || true
              ip link set "$bridge" down 2>/dev/null || true
              ip link delete "$bridge" 2>/dev/null || true
            '';
          }
      )
      cfg.networks
      // lib.mapAttrs' (
        vmName: vmCfg:
          lib.nameValuePair "kcore-tap-${vmName}" {
            description = "TAP interface for VM ${vmName}";
            requires = ["kcore-bridge-${vmCfg.network}.service"];
            after = ["kcore-bridge-${vmCfg.network}.service"];
            before = ["kcore-vm-${vmName}.service"];
            wantedBy = ["kcore-vm-${vmName}.service"];

            serviceConfig = {
              Type = "oneshot";
              RemainAfterExit = true;
            };

            path = [pkgs.iproute2];

            script = ''
              tap="${tapName vmName}"
              ip tuntap add dev "$tap" mode tap
              ip link set "$tap" master "${bridgeName vmCfg.network}"
              ip link set "$tap" up
            '';

            preStop = ''
              ip link delete "${tapName vmName}" 2>/dev/null || true
            '';
          }
      )
      cfg.virtualMachines;

    networking.firewall.trustedInterfaces =
      lib.optional (cfg.networks != {}) "kbr-+";

    boot.kernel.sysctl."net.ipv4.ip_forward" = 1;
  };
}
