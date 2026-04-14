{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.ch-vm.vms;
  helpers = import ./helpers.nix { inherit lib; };

  bridgeName =
    name:
    let
      full = "kbr-${name}";
      hash = builtins.substring 0 8 (builtins.hashString "sha256" name);
      short = "kb-${hash}";
    in
    if builtins.stringLength full <= 15 then full else short;
  inherit (helpers) tapName;
  upstreamIface =
    _netName: netCfg:
    if netCfg.vlanId > 0 then
      "${cfg.gatewayInterface}.${toString netCfg.vlanId}"
    else
      cfg.gatewayInterface;
  subnetPrefix =
    ip:
    let
      match = builtins.match "([0-9]+\\.[0-9]+\\.[0-9]+)\\.[0-9]+" ip;
    in
    if match == null then
      throw "invalid IPv4 address for gatewayIP: ${ip}"
    else
      builtins.elemAt match 0;

  netmaskToCidr =
    mask:
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
    .${mask} or (throw "unsupported netmask: ${mask}");

  hexDigitToInt =
    c:
    {
      "0" = 0;
      "1" = 1;
      "2" = 2;
      "3" = 3;
      "4" = 4;
      "5" = 5;
      "6" = 6;
      "7" = 7;
      "8" = 8;
      "9" = 9;
      "a" = 10;
      "b" = 11;
      "c" = 12;
      "d" = 13;
      "e" = 14;
      "f" = 15;
    }
    .${lib.toLower c} or (throw "invalid hex digit '${c}'");

  hexByteToInt =
    hex:
    let
      chars = lib.stringToCharacters hex;
      hi = hexDigitToInt (builtins.elemAt chars 0);
      lo = hexDigitToInt (builtins.elemAt chars 1);
    in
    (hi * 16) + lo;

  # Keep deterministic host reservation per VM name.
  # We reserve from .10-.249 and use linear probing for rare collisions.
  dhcpReservedHostForName =
    vmName:
    let
      hash = builtins.hashString "sha256" vmName;
      b0 = hexByteToInt (builtins.substring 0 2 hash);
      b1 = hexByteToInt (builtins.substring 2 2 hash);
      b2 = hexByteToInt (builtins.substring 4 2 hash);
      minHost = 10;
      maxHost = 249;
      size = (maxHost - minHost) + 1;
      offset = lib.mod ((((b0 * 256) + b1) * 256) + b2) size;
    in
    minHost + offset;

  assignDhcpReservedHosts =
    vmNames:
    let
      minHost = 10;
      maxHost = 249;
      size = (maxHost - minHost) + 1;
      sorted = lib.sort builtins.lessThan vmNames;
      pickHost =
        used: startOffset: probe:
        if probe >= size then
          throw "unable to allocate deterministic DHCP reservation host (pool exhausted)"
        else
          let
            offset = lib.mod (startOffset + probe) size;
            host = minHost + offset;
          in
          if builtins.elem host used then pickHost used startOffset (probe + 1) else host;
      step =
        acc: vmName:
        let
          preferred = (dhcpReservedHostForName vmName) - minHost;
          selected = pickHost acc.used preferred 0;
        in
        {
          used = acc.used ++ [ selected ];
          hosts = acc.hosts // {
            "${vmName}" = selected;
          };
        };
    in
    (lib.foldl' step {
      used = [ ];
      hosts = { };
    } sorted).hosts;

  vmMacAddress =
    vmName: vmCfg:
    if vmCfg.macAddress != null then
      lib.toLower vmCfg.macAddress
    else
      lib.toLower (helpers.generateMac vmName);

  natVmConfigsForNetwork =
    netName: lib.filterAttrs (_: vmCfg: vmCfg.network == netName) cfg.virtualMachines;
in
{
  config = lib.mkIf cfg.enable {
    assertions =
      lib.mapAttrsToList (vmName: vmCfg: {
        assertion = cfg.networks ? ${vmCfg.network};
        message = "VM '${vmName}' references network '${vmCfg.network}' which is not defined in ch-vm.vms.networks.";
      }) cfg.virtualMachines
      ++ lib.mapAttrsToList (netName: netCfg: {
        assertion = !(netCfg.networkType == "bridge" && netCfg.vlanId == 0);
        message = "Network '${netName}' uses bridge mode without a VLAN ID. This would enslave the management NIC (${cfg.gatewayInterface}) and sever host connectivity. Set vlanId > 0 or use nat/vxlan instead.";
      }) cfg.networks;

    boot.kernelModules = [
      "tun"
      "tap"
      "br_netfilter"
      "vxlan"
    ];

    networking.nftables.enable = true;

    systemd.services =
      lib.mapAttrs' (
        netName: netCfg:
        lib.nameValuePair "kcore-bridge-${netName}" {
          description = "kcore bridge for network ${netName}";
          wantedBy = [ "multi-user.target" ];
          before = lib.mapAttrsToList (vmName: _vmCfg: "kcore-vm-${vmName}.service") (
            lib.filterAttrs (_: vm: vm.network == netName) cfg.virtualMachines
          );

          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
          };

          path = [
            pkgs.iproute2
            pkgs.nftables
          ];

          script =
            let
              isNat = netCfg.networkType == "nat";
              isBridge = netCfg.networkType == "bridge";
              isVxlan = netCfg.networkType == "vxlan";
            in
            ''
              bridge="${bridgeName netName}"
              ip link show "$bridge" >/dev/null 2>&1 && exit 0

              ${lib.optionalString isNat ''
                # Safety guard: prevent bridge subnet from hijacking the host LAN.
                ext_ip=$(ip -4 -o addr show dev "${cfg.gatewayInterface}" scope global 2>/dev/null | awk 'NR==1 {print $4}' | cut -d/ -f1)
                if [ -n "$ext_ip" ]; then
                  gw_ip="${netCfg.gatewayIP}"
                  ext_prefix="''${ext_ip%.*}"
                  gw_prefix="''${gw_ip%.*}"
                  if [ "$ext_prefix" = "$gw_prefix" ]; then
                    echo "Refusing ch-vm network '${netName}': gatewayIP ${netCfg.gatewayIP} overlaps external subnet on ${cfg.gatewayInterface} ($ext_ip)"
                    exit 1
                  fi
                fi
              ''}

              ${lib.optionalString (netCfg.vlanId > 0) ''
                vlan_if="${cfg.gatewayInterface}.${toString netCfg.vlanId}"
                if ! ip link show "$vlan_if" >/dev/null 2>&1; then
                  ip link add link "${cfg.gatewayInterface}" name "$vlan_if" type vlan id ${toString netCfg.vlanId}
                  ip link set "$vlan_if" up
                fi
              ''}

              ip link add "$bridge" type bridge
              ip link set "$bridge" up

              ${lib.optionalString isBridge ''
                # Bridge mode: attach physical NIC (or VLAN sub-if) directly to bridge.
                # VMs obtain IPs from the upstream DHCP server.
                ip link set "${upstreamIface netName netCfg}" master "$bridge"
              ''}

              ${lib.optionalString isNat ''
                ip addr add ${netCfg.gatewayIP}/${netmaskToCidr netCfg.internalNetmask} dev "$bridge"

                nft add table ip kcore-${netName} 2>/dev/null || true
                nft add chain ip kcore-${netName} postrouting '{ type nat hook postrouting priority srcnat; }'
                nft add rule ip kcore-${netName} postrouting oifname "${upstreamIface netName netCfg}" masquerade
                nft add chain ip kcore-${netName} prerouting '{ type nat hook prerouting priority dstnat; }'
                nft add chain ip kcore-${netName} forward '{ type filter hook forward priority 0; }'
                ${lib.concatMapStringsSep "\n              "
                  (port: ''
                    nft add rule ip kcore-${netName} prerouting ip daddr ${netCfg.externalIP} tcp dport ${toString port} dnat to ${netCfg.gatewayIP}
                                  nft add rule ip kcore-${netName} forward iifname "${upstreamIface netName netCfg}" tcp dport ${toString port} accept'')
                  netCfg.allowedTCPPorts
                }
                ${lib.concatMapStringsSep "\n              "
                  (port: ''
                    nft add rule ip kcore-${netName} prerouting ip daddr ${netCfg.externalIP} udp dport ${toString port} dnat to ${netCfg.gatewayIP}
                                  nft add rule ip kcore-${netName} forward iifname "${upstreamIface netName netCfg}" udp dport ${toString port} accept'')
                  netCfg.allowedUDPPorts
                }
                ${lib.concatMapStringsSep "\n              " (rule: ''
                  ${
                    if rule.enableDnat && rule.targetIp != "" then
                      ''
                        nft add rule ip kcore-${netName} prerouting ip daddr ${netCfg.externalIP} ${rule.protocol} dport ${toString rule.hostPort} ip saddr ${rule.sourceCidr} dnat to ${rule.targetIp}:${toString rule.targetPort}
                        nft add rule ip kcore-${netName} forward iifname "${upstreamIface netName netCfg}" ip daddr ${rule.targetIp} ${rule.protocol} dport ${toString rule.targetPort} ip saddr ${rule.sourceCidr} accept
                      ''
                    else
                      ''
                        nft add rule ip kcore-${netName} forward iifname "${upstreamIface netName netCfg}" ${rule.protocol} dport ${toString rule.hostPort} ip saddr ${rule.sourceCidr} accept
                      ''
                  }
                '') netCfg.securityGroupRules}
              ''}

              ${lib.optionalString isVxlan ''
                # VXLAN overlay: create VXLAN interface, add FDB entries, attach to bridge.
                ip addr add ${netCfg.gatewayIP}/${netmaskToCidr netCfg.internalNetmask} dev "$bridge"

                ip link add vxlan${toString netCfg.vni} type vxlan id ${toString netCfg.vni} dstport 4789 local ${netCfg.vxlanLocalIp}
                ${lib.concatMapStringsSep "\n              " (peer: ''
                  bridge fdb append 00:00:00:00:00:00 dev vxlan${toString netCfg.vni} dst ${peer}
                '') netCfg.vxlanPeers}
                ip link set vxlan${toString netCfg.vni} master "$bridge"
                ip link set vxlan${toString netCfg.vni} up

                ${lib.optionalString netCfg.enableOutboundNat ''
                  nft add table ip kcore-${netName} 2>/dev/null || true
                  nft add chain ip kcore-${netName} postrouting '{ type nat hook postrouting priority srcnat; }'
                  nft add rule ip kcore-${netName} postrouting oifname "${cfg.gatewayInterface}" masquerade
                ''}
              ''}
            '';

          preStop =
            let
              isVxlan = netCfg.networkType == "vxlan";
              isBridge = netCfg.networkType == "bridge";
            in
            ''
              bridge="${bridgeName netName}"
              nft delete table ip kcore-${netName} 2>/dev/null || true
              ${lib.optionalString isVxlan ''
                ip link delete vxlan${toString netCfg.vni} 2>/dev/null || true
              ''}
              ${lib.optionalString isBridge ''
                ip link set "${upstreamIface netName netCfg}" nomaster 2>/dev/null || true
              ''}
              ip link set "$bridge" down 2>/dev/null || true
              ip link delete "$bridge" 2>/dev/null || true
              ${lib.optionalString (netCfg.vlanId > 0) ''
                ip link delete "${cfg.gatewayInterface}.${toString netCfg.vlanId}" 2>/dev/null || true
              ''}
            '';
        }
      ) cfg.networks
      // lib.mapAttrs' (
        netName: netCfg:
        lib.nameValuePair "kcore-dhcp-${netName}" {
          description = "kcore dnsmasq DHCP for network ${netName}";
          requires = [ "kcore-bridge-${netName}.service" ];
          after = [ "kcore-bridge-${netName}.service" ];
          wantedBy = [ "multi-user.target" ];
          serviceConfig = {
            Type = "simple";
            Restart = "always";
            RestartSec = 2;
            ExecStartPre = "${pkgs.coreutils}/bin/mkdir -p /run/kcore";
            ExecStart =
              let
                netVms = natVmConfigsForNetwork netName;
                vmNames = lib.attrNames netVms;
                reservedHosts = assignDhcpReservedHosts vmNames;
                dhcpHostArgs = lib.concatMapStringsSep " " (
                  vmName:
                  let
                    vmCfg = netVms.${vmName};
                    mac = vmMacAddress vmName vmCfg;
                    hostOctet = toString reservedHosts.${vmName};
                    fallbackIp = "${subnetPrefix netCfg.gatewayIP}.${hostOctet}";
                    fixedIp = if vmCfg.dhcpReservedIPv4 != null then vmCfg.dhcpReservedIPv4 else fallbackIp;
                  in
                  "--dhcp-host=${mac},${fixedIp},${vmName},infinite"
                ) vmNames;
              in
              "${pkgs.dnsmasq}/bin/dnsmasq --keep-in-foreground --bind-interfaces --interface=${bridgeName netName} --except-interface=lo --dhcp-authoritative --dhcp-range=${subnetPrefix netCfg.gatewayIP}.100,${subnetPrefix netCfg.gatewayIP}.199,${netCfg.internalNetmask},12h --dhcp-option=option:router,${netCfg.gatewayIP} --dhcp-option=option:dns-server,1.1.1.1,8.8.8.8 --dhcp-leasefile=/run/kcore/dnsmasq-${netName}.leases --pid-file=/run/kcore/dnsmasq-${netName}.pid ${dhcpHostArgs}";
          };
        }
      ) (lib.filterAttrs (_: netCfg: netCfg.networkType == "nat") cfg.networks)
      // lib.mapAttrs' (
        vmName: vmCfg:
        lib.nameValuePair "kcore-tap-${vmName}" {
          description = "TAP interface for VM ${vmName}";
          requires = [ "kcore-bridge-${vmCfg.network}.service" ];
          after = [ "kcore-bridge-${vmCfg.network}.service" ];
          before = [ "kcore-vm-${vmName}.service" ];
          wantedBy = [ "kcore-vm-${vmName}.service" ];

          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = true;
          };

          path = [ pkgs.iproute2 ];

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
      ) cfg.virtualMachines;

    # NixOS firewall trustedInterfaces expects explicit interface names, not globs.
    # Build the exact bridge interface list so DHCP/DNS traffic from VM bridges
    # reaches host services like dnsmasq.
    networking.firewall.trustedInterfaces = lib.mapAttrsToList (
      netName: _netCfg: bridgeName netName
    ) cfg.networks;

    networking.firewall.allowedUDPPorts = lib.optional (lib.any (n: n.networkType == "vxlan") (
      lib.attrValues cfg.networks
    )) 4789;

    boot.kernel.sysctl."net.ipv4.ip_forward" = lib.mkDefault 1;
  };
}
