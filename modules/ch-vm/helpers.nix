{lib}: {
  tapName = vmName: "tap-${builtins.substring 0 8 (builtins.hashString "sha256" vmName)}";
}
