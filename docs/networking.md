# Networking Model

This project uses declarative virtual networks defined under `ch-vm.vms.networks`.

Key behavior:
- A VM is attached to the network named in `virtualMachines.<name>.network`.
- A network maps to shared host services:
  - `kcore-bridge-<network>.service` (bridge)
  - `kcore-dhcp-<network>.service` (DHCP on that bridge)
  - NAT via nftables for outbound connectivity
- Each VM still gets its own TAP and VM unit:
  - `kcore-tap-<vm>.service`
  - `kcore-vm-<vm>.service`

So, networks are **per named network**, not automatically per VM.

## Example: Two VMs On The Same Network

Both VMs use `network = "default"`, so they share one L2/L3 domain (`kbr-default`):

```nix
{ pkgs, ... }: {
  ch-vm.vms = {
    enable = true;
    cloudHypervisorPackage = pkgs.cloud-hypervisor;
    gatewayInterface = "eno1";

    networks.default = {
      externalIP = "192.168.40.105";
      gatewayIP = "10.240.0.1";
    };

    virtualMachines.web = {
      image = "/var/lib/kcore/images/web.qcow2";
      imageFormat = "qcow2";
      network = "default";
      cores = 2;
      memorySize = 2048;
    };

    virtualMachines.api = {
      image = "/var/lib/kcore/images/api.qcow2";
      imageFormat = "qcow2";
      network = "default";
      cores = 2;
      memorySize = 2048;
    };
  };
}
```

Result:
- One bridge: `kbr-default`
- One DHCP scope for both VMs
- Two TAPs + two VM services

Create these two VMs with `kctl`:

```bash
kcore-kctl create vm web-same-net \
  --image "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2" \
  --image-sha256 "2f8a63ad18962a6413657b82dd016d71604f84a7cd6fdb17e811099d5c88e854" \
  --network default \
  --cpu 2 \
  --memory 2G

kcore-kctl create vm api-same-net \
  --image "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2" \
  --image-sha256 "2f8a63ad18962a6413657b82dd016d71604f84a7cd6fdb17e811099d5c88e854" \
  --network default \
  --cpu 2 \
  --memory 2G
```

## Example: Two VMs On Different Networks

Each VM points to a different network, so each gets a separate bridge and DHCP scope:

```nix
{ pkgs, ... }: {
  ch-vm.vms = {
    enable = true;
    cloudHypervisorPackage = pkgs.cloud-hypervisor;
    gatewayInterface = "eno1";

    networks.frontend = {
      externalIP = "192.168.40.105";
      gatewayIP = "10.240.10.1";
    };

    networks.backend = {
      externalIP = "192.168.40.105";
      gatewayIP = "10.240.20.1";
    };

    virtualMachines.web = {
      image = "/var/lib/kcore/images/web.qcow2";
      imageFormat = "qcow2";
      network = "frontend";
      cores = 2;
      memorySize = 2048;
    };

    virtualMachines.db = {
      image = "/var/lib/kcore/images/db.qcow2";
      imageFormat = "qcow2";
      network = "backend";
      cores = 2;
      memorySize = 2048;
    };
  };
}
```

Result:
- `web` on `kbr-frontend`
- `db` on `kbr-backend`
- Separate DHCP/NAT domains by design

Create these two VMs with `kctl`:

```bash
kcore-kctl create vm web-front-net \
  --image "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2" \
  --image-sha256 "2f8a63ad18962a6413657b82dd016d71604f84a7cd6fdb17e811099d5c88e854" \
  --network frontend \
  --cpu 2 \
  --memory 2G

kcore-kctl create vm db-back-net \
  --image "https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2" \
  --image-sha256 "2f8a63ad18962a6413657b82dd016d71604f84a7cd6fdb17e811099d5c88e854" \
  --network backend \
  --cpu 2 \
  --memory 2G
```

Note:
- `--network` must reference a network that exists in the applied node Nix config under `ch-vm.vms.networks`.

