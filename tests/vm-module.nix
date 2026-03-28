{ pkgs, ... }:
let
  testImage = pkgs.runCommand "test-disk.raw" { } ''
    ${pkgs.qemu}/bin/qemu-img create -f raw "$out" 64M
  '';
in
pkgs.testers.runNixOSTest {
  name = "ch-vm-basic";

  nodes.machine =
    { pkgs, ... }:
    {
      imports = [ ../modules/ch-vm ];

      ch-vm.vms = {
        enable = true;
        cloudHypervisorPackage = pkgs.cloud-hypervisor;
        gatewayInterface = "eth0";

        networks.default = {
          externalIP = "10.0.2.15";
          gatewayIP = "192.168.100.1";
        };

        virtualMachines."this-name-is-way-too-long" = {
          image = testImage;
          cores = 1;
          memorySize = 256;
          network = "default";
          autoStart = false;
        };
      };

      virtualisation.memorySize = 2048;
    };

  testScript = ''
    machine.wait_for_unit("kcore-bridge-default.service")
    machine.wait_for_unit("kcore-dhcp-default.service")
    machine.succeed("ip link show kbr-default")

    status = machine.get_unit_info("kcore-vm-this-name-is-way-too-long.service")
    assert status["ActiveState"] != "active", "VM should not auto-start"

    machine.succeed("systemctl start kcore-tap-this-name-is-way-too-long.service")
    machine.wait_for_unit("kcore-tap-this-name-is-way-too-long.service")
    machine.succeed("test $(ip -o link show | awk -F': ' '/tap-[0-9a-f]{8}/ {print $2; exit}' | wc -c) -le 16")
    machine.succeed("systemctl cat kcore-vm-this-name-is-way-too-long.service | grep -E 'tap-[0-9a-f]{8}'")
    machine.succeed("systemctl cat kcore-vm-this-name-is-way-too-long.service | grep -F -- '--firmware /nix/store/'")
    machine.succeed("systemctl cat kcore-vm-this-name-is-way-too-long.service | grep -F 'CLOUDHV.fd'")
    machine.succeed("systemctl cat kcore-vm-this-name-is-way-too-long.service | grep -F 'image_type=raw'")

    machine.succeed("test -d /run/kcore")
    machine.succeed("test -f /etc/kcore/seeds/this-name-is-way-too-long.iso")
  '';
}
