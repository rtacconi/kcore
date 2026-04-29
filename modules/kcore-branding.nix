{ lib, pkgs, ... }:
let
  kcoreConsoleExe =
    if pkgs ? kcore-console then
      "${pkgs.kcore-console}/bin/kcore-console"
    else
      "/opt/kcore/bin/kcore-console";
  staticIssue = ''
    kcore hypervisor appliance console
    Local shell login is disabled. Use SSH and kcorectl for administration.

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

  # No local console login: only SSH and kctl operations are supported.
  #
  # NixOS wires the first VT through autovt@tty1 (see systemd.targets.getty in
  # nixpkgs getty.nix). Disabling only getty@tty1 leaves agetty running via
  # autovt@, which still shows login(1). Disable the autovt template and all
  # VT getty instances, and clear installer-style autologin if merged later.
  services.getty.autologinUser = lib.mkForce null;
  services.getty.helpLine = lib.mkForce "";

  systemd.services."autovt@".enable = lib.mkForce false;
  systemd.services."getty@tty1".enable = lib.mkForce false;
  systemd.services."getty@tty2".enable = lib.mkForce false;
  systemd.services."getty@tty3".enable = lib.mkForce false;
  systemd.services."getty@tty4".enable = lib.mkForce false;
  systemd.services."getty@tty5".enable = lib.mkForce false;
  systemd.services."getty@tty6".enable = lib.mkForce false;
  systemd.services."serial-getty@ttyS0".enable = lib.mkForce false;
  systemd.services."serial-getty@hvc0".enable = lib.mkForce false;

  # Ratatui appliance TUI (see `crates/kcore-console` and `docs/appliance-console.md`).
  systemd.services.kcore-console = {
    description = "kcore hypervisor appliance console (TUI)";
    documentation = [ "https://kcore.ai/docs" ];
    after = [
      "local-fs.target"
      "network-online.target"
    ];
    wants = [ "network-online.target" ];
    wantedBy = [ "multi-user.target" ];
    before = [ "getty.target" ];
    serviceConfig = {
      Type = "simple";
      Restart = "always";
      RestartSec = "2s";
      ExecStart = "${kcoreConsoleExe} --tty /dev/tty1";
      User = "root";
      Group = "root";
      # ip, lsblk, systemctl for inventory
      Environment = "PATH=${
        lib.makeBinPath (
          with pkgs;
          [
            coreutils
            iproute2
            util-linux
            systemd
          ]
        )
      }:/run/wrappers/bin";
      UMask = "0077";
      NoNewPrivileges = true;
      ProtectSystem = true;
      ProtectHome = true;
      PrivateTmp = true;
      StandardInput = "tty";
      StandardOutput = "tty";
      StandardError = "journal";
      TTYPath = "/dev/tty1";
      TTYReset = true;
      TTYVHangup = true;
      TTYVTDisallocate = true;
    };
    unitConfig.Conflicts = [
      "getty@tty1.service"
      "autovt@tty1.service"
    ];
  };
}
