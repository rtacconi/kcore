{ lib, pkgs, ... }:

{
  # Minimal base system configuration
  # Remove unnecessary packages and documentation

  # Disable documentation
  documentation.enable = false;
  documentation.nixos.enable = false;
  documentation.man.enable = false;
  documentation.info.enable = false;

  # Minimal locale support (English only)
  i18n.supportedLocales = [ "en_US.UTF-8/UTF-8" ];
  i18n.defaultLocale = "en_US.UTF-8";

  # Disable unnecessary services
  services.udisks2.enable = false;
  services.upower.enable = false;
  services.accounts-daemon.enable = false;
  services.geoclue2.enable = false;
  services.gnome.gnome-keyring.enable = false;

  # Disable desktop environments
  services.xserver.enable = false;

  # Minimal packages
  environment.defaultPackages = with pkgs; [
    coreutils
    findutils
    gnutar
    gzip
    bzip2
    xz
    zstd
    curl
    wget
    vim
    htop
    iotop
    tcpdump
    iproute2
    iputils
    ethtool
    pciutils
    usbutils
  ];

  # Remove unnecessary kernel modules
  # NOTE: Do NOT blacklist usbhid, hid, hid-generic, usbmouse, usbkbd - they're needed for keyboards/mice!
  boot.blacklistedKernelModules = [
    # Audio (not needed for headless server)
    "snd"
    "soundcore"
    # Uncomment specific vendor HID drivers if you want to save space (but test first!)
    # "hid-apple"
    # "hid-microsoft"
    # "hid-logitech"
    # "hid-sony"
    # "hid-steelseries"
    # "hid-wacom"
  ];

  # Keep essential kernel modules
  boot.initrd.availableKernelModules = [
    "ahci"
    "nvme"
    "sd_mod"
    "sr_mod"
    "virtio_pci"
    "virtio_blk"
    "virtio_net"
    "virtio_scsi"
    "virtio_balloon"
    "virtio_console"
    "virtio_rng"
    "9p"
    "9pnet"
    "9pnet_virtio"
  ];

  # Firmware
  hardware.enableAllFirmware = true;
  hardware.enableRedistributableFirmware = true;

  # Microcode updates
  hardware.cpu.intel.updateMicrocode = lib.mkDefault true;
  hardware.cpu.amd.updateMicrocode = lib.mkDefault true;

  # Systemd optimizations
  systemd.services.systemd-udevd.restartIfChanged = false;
}
