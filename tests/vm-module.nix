{pkgs, ...}: let
  testImage = pkgs.runCommand "test-disk.raw" {} ''
    ${pkgs.qemu}/bin/qemu-img create -f raw "$out" 64M
  '';
in
  pkgs.testers.runNixOSTest {
    name = "ctrl-os-vms-basic";

    nodes.machine = {pkgs, ...}: {
      imports = [../modules/ctrl-os-vms];

      ctrl-os.vms = {
        enable = true;
        cloudHypervisorPackage = pkgs.cloud-hypervisor;
        gatewayInterface = "eth0";

        networks.default = {
          externalIP = "10.0.2.15";
          gatewayIP = "192.168.100.1";
        };

        virtualMachines.testvm = {
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
      machine.succeed("ip link show kbr-default")

      status = machine.get_unit_info("kcore-vm-testvm.service")
      assert status["ActiveState"] != "active", "VM should not auto-start"

      machine.succeed("test -d /run/kcore")
      machine.succeed("test -f /etc/kcore/seeds/testvm.iso")
    '';
  }
