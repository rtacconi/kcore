{ lib, ... }:
let
  staticIssue = ''
    ██╗  ██╗ ██████╗  ██████╗ ██████╗ ███████╗
    ██║ ██╔╝██╔════╝ ██╔═══██╗██╔══██╗██╔════╝
    █████╔╝ ██║      ██║   ██║██████╔╝█████╗
    ██╔═██╗ ██║      ██║   ██║██╔══██╗██╔══╝
    ██║  ██╗╚██████╗ ╚██████╔╝██║  ██║███████╗
    ╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝

    Welcome to kcoreOS
    Kernel \r on an \m (\l)

  '';
in
{
  # OS Release branding
  system.nixos.label = "kcoreOS";

  # /etc/os-release
  environment.etc."os-release".text = ''
    NAME="kcoreOS"
    PRETTY_NAME="kcoreOS"
    ID=nixos
    VERSION_ID="25.05"
    VERSION="25.05 (kcoreOS)"
    VERSION_CODENAME=kcoreos
    HOME_URL="https://github.com/kcore/kcore"
    SUPPORT_URL="https://github.com/kcore/kcore"
    BUG_REPORT_URL="https://github.com/kcore/kcore/issues"
  '';

  # GRUB theme (simple text-based for now)
  boot.loader.grub.splashImage = null;
  boot.loader.grub.theme = null;
  boot.loader.grub.extraConfig = ''
    set timeout=5
    set default=0
  '';

  # Plymouth splash (if enabled)
  boot.plymouth = {
    enable = lib.mkDefault false;
    theme = "bgrt";
  };

  # TTY greeting
  environment.etc."issue".text = staticIssue;
  environment.etc."issue.kcore-static".text = staticIssue;

  # SSH banner
  services.openssh.banner = ''
    ██╗  ██╗ ██████╗  ██████╗ ██████╗ ███████╗
    ██║ ██╔╝██╔════╝ ██╔═══██╗██╔══██╗██╔════╝
    █████╔╝ ██║      ██║   ██║██████╔╝█████╗
    ██╔═██╗ ██║      ██║   ██║██╔══██╗██╔══╝
    ██║  ██╗╚██████╗ ╚██████╔╝██║  ██║███████╗
    ╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝

    Welcome to kcoreOS
    This system is managed by kcore.
  '';

  # MOTD
  environment.etc."motd".text = ''
    ██╗  ██╗ ██████╗  ██████╗ ██████╗ ███████╗
    ██║ ██╔╝██╔════╝ ██╔═══██╗██╔══██╗██╔════╝
    █████╔╝ ██║      ██║   ██║██████╔╝█████╗
    ██╔═██╗ ██║      ██║   ██║██╔══██╗██╔══╝
    ██║  ██╗╚██████╗ ╚██████╔╝██║  ██║███████╗
    ╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝

    kcoreOS - A modern virtualization platform
    Powered by kcoreOS

  '';

  systemd.services.kcore-issue-refresh = {
    description = "Render dynamic kcoreOS TTY issue screen";
    after = [
      "local-fs.target"
      "network-online.target"
      "kcore-node-agent.service"
    ];
    wants = [ "network-online.target" ];
    wantedBy = [ "multi-user.target" ];
    serviceConfig = {
      Type = "oneshot";
      TimeoutStartSec = "30s";
    };
    script = ''
      set -euo pipefail
      tmp_issue="$(mktemp)"
      cleanup() { rm -f "$tmp_issue"; }
      trap cleanup EXIT

      agent_bin=""
      if [ -x /opt/kcore/bin/kcore-node-agent ]; then
        agent_bin="/opt/kcore/bin/kcore-node-agent"
      elif [ -x /run/current-system/sw/bin/kcore-node-agent ]; then
        agent_bin="/run/current-system/sw/bin/kcore-node-agent"
      fi

      if [ -n "$agent_bin" ] && timeout 20s "$agent_bin" render-issue --output "$tmp_issue"; then
        rm -f /etc/issue
        install -m 0644 "$tmp_issue" /etc/issue
      else
        echo "kcore-issue-refresh: renderer failed, restoring static issue" >&2
        rm -f /etc/issue
        install -m 0644 /etc/issue.kcore-static /etc/issue
      fi
    '';
  };

  systemd.timers.kcore-issue-refresh = {
    description = "Periodic kcoreOS issue refresh";
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnBootSec = "20s";
      OnUnitActiveSec = "5min";
      Unit = "kcore-issue-refresh.service";
    };
  };
}
