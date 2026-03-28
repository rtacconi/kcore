{ lib }:
{
  tapName = vmName: "tap-${builtins.substring 0 8 (builtins.hashString "sha256" vmName)}";

  generateMac =
    vmName:
    let
      hash = builtins.hashString "sha256" vmName;
      hexChars = lib.stringToCharacters hash;
      byte = n: lib.concatStrings (lib.sublist (n * 2) 2 hexChars);
    in
    "52:54:00:${byte 0}:${byte 1}:${byte 2}";
}
