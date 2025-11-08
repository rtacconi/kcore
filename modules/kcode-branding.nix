{ config, lib, pkgs, ... }:

let
  kcodeLogo = pkgs.writeText "kcode-logo.txt" ''
    ██╗  ██╗ ██████╗  ██████╗ ██████╗ ███████╗
    ██║ ██╔╝██╔════╝ ██╔═══██╗██╔══██╗██╔════╝
    █████╔╝ ██║      ██║   ██║██████╔╝█████╗
    ██╔═██╗ ██║      ██║   ██║██╔══██╗██╔══╝
    ██║  ██╗╚██████╗ ╚██████╔╝██║  ██║███████╗
    ╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝
  '';
in
{
  # OS Release branding
  system.nixos.label = "kcode";
  
  # /etc/os-release
  environment.etc."os-release".text = ''
    NAME="kcode"
    PRETTY_NAME="kcode"
    ID=nixos
    VERSION_ID="24.05"
    VERSION="24.05 (kcode)"
    VERSION_CODENAME=kcode
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
  environment.etc."issue".text = ''
    ██╗  ██╗ ██████╗  ██████╗ ██████╗ ███████╗
    ██║ ██╔╝██╔════╝ ██╔═══██╗██╔══██╗██╔════╝
    █████╔╝ ██║      ██║   ██║██████╔╝█████╗
    ██╔═██╗ ██║      ██║   ██║██╔══██╗██╔══╝
    ██║  ██╗╚██████╗ ╚██████╔╝██║  ██║███████╗
    ╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝

    Welcome to kcode - A modern virtualization platform
    Kernel \r on an \m (\l)

  '';

  # SSH banner
  services.openssh.banner = ''
    ██╗  ██╗ ██████╗  ██████╗ ██████╗ ███████╗
    ██║ ██╔╝██╔════╝ ██╔═══██╗██╔══██╗██╔════╝
    █████╔╝ ██║      ██║   ██║██████╔╝█████╗
    ██╔═██╗ ██║      ██║   ██║██╔══██╗██╔══╝
    ██║  ██╗╚██████╗ ╚██████╔╝██║  ██║███████╗
    ╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝

    Welcome to kcode - A modern virtualization platform
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

    kcode - A modern virtualization platform
    Powered by NixOS and kcore

  '';
}
