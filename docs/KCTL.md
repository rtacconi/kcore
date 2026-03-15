# kctl - kcore CLI Tool

`kctl` is the command-line interface for managing kcore clusters. VMs are created declaratively from YAML manifests.

**Important:** kctl communicates with the **controller**, not with node-agents directly. The controller forwards VM operations to the appropriate node-agent. The default controller port is **9090**.

---

## Installation

### Building from Source

```bash
cd kcore

# Build kctl for macOS ARM64
make kctl

# Build kctl for Linux
CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -o bin/kctl-linux ./cmd/kctl

# Binary will be at: bin/kctl or bin/kctl-linux
```

### Add to PATH

```bash
export PATH="$PATH:/path/to/kcore/bin"

# Or copy to system path
sudo cp bin/kctl /usr/local/bin/
```

---

## Quick Start

```bash
# Create a VM from a YAML manifest
kctl apply -f examples/vm.yaml

# Dry-run (preview without applying)
kctl apply -f examples/vm.yaml --dry-run

# List all VMs
kctl get vms

# Get VM details
kctl describe vm debian12-test

# Delete a VM
kctl delete vm debian12-test
```

---

## Creating VMs

VMs are created **declaratively** via YAML manifests. There is no `kctl create vm` CLI command.

### VM Manifest Format

```yaml
apiVersion: kcore.io/v1
kind: VM
metadata:
  name: debian12-test
spec:
  cpu: 1
  memory: 2G
  image: https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2
  enableKcoreLogin: true
  nics:
    - network: default
      model: virtio
```

### Manifest Fields

| Field | Required | Description |
|---|---|---|
| `metadata.name` | Yes | VM name |
| `spec.cpu` | Yes | Number of CPU cores |
| `spec.memory` | Yes | Memory size (e.g. `2G`, `4096M`) |
| `spec.image` | No | Cloud image URL or local path |
| `spec.enableKcoreLogin` | No | Enable kcore default credentials (default: true) |
| `spec.cloudInit` | No | Custom #cloud-config YAML (overrides default) |
| `spec.nics` | No | Network interfaces (defaults to `default` network) |

### Apply a Manifest

```bash
# Apply to node configured in ~/.kcore/config
kctl apply -f vm.yaml

# Dry-run to preview
kctl apply -f vm.yaml --dry-run

# With explicit controller address
kctl apply -f vm.yaml --controller 192.168.40.107:9090

# Skip TLS verification
kctl apply -f vm.yaml --insecure
```

### Custom Cloud-Init

```yaml
apiVersion: kcore.io/v1
kind: VM
metadata:
  name: custom-vm
spec:
  cpu: 2
  memory: 4G
  image: https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2
  cloudInit: |
    #cloud-config
    users:
      - name: admin
        sudo: ALL=(ALL) NOPASSWD:ALL
        shell: /bin/bash
        lock_passwd: false
        plain_text_passwd: s3cret
    ssh_pwauth: true
  nics:
    - network: default
```

### Cloud-Init Default Behavior

When `enableKcoreLogin: true` (default), cloud-init configures:
- User `kcore` with password `kcore` and sudo
- Distro default user (e.g. `debian`, `ubuntu`) with password `kcore`
- Root password `kcore`
- SSH password auth enabled
- Serial console enabled

---

## Commands Reference

### `kctl apply`

Apply a configuration from a YAML file.

```bash
kctl apply -f FILENAME [flags]
```

**Flags:**
- `-f, --filename` - Filename to apply (required)
- `--dry-run` - Show what would be applied without applying

### `kctl get`

Display resources.

```bash
kctl get RESOURCE [NAME]
```

**Resources:** `vms`, `vm`, `nodes`, `node`, `volumes`, `networks`

### `kctl describe`

Show detailed information about a resource.

```bash
kctl describe RESOURCE NAME
```

### `kctl delete`

Delete resources by name.

```bash
kctl delete RESOURCE NAME [flags]
```

### `kctl version`

Print the kctl version.

---

## Global Flags

| Flag | Description |
|---|---|
| `-c, --config` | Path to kctl config file (default: `~/.kcore/config`) |
| `-s, --controller` | Controller address (overrides config, default port 9090) |
| `-k, --insecure` | Skip TLS certificate verification |

---

## Configuration

kctl reads configuration from `~/.kcore/config`:

```yaml
current-context: lenovo
contexts:
  lenovo:
    controller: "192.168.40.107:9090"
    insecure: false
    cert: "/path/to/client.crt"
    key: "/path/to/client.key"
    ca: "/path/to/ca.crt"
  dev:
    controller: "localhost:9090"
    insecure: true
```

---

## Node Provisioning Manifests

In addition to `VM`, kctl supports manifest kinds for node lifecycle management via the Automator API.

### NodeInstall

Triggers OS installation on a bare-metal node with multi-disk support.

```yaml
apiVersion: kcore.io/v1
kind: NodeInstall
metadata:
  name: install-kcore01
spec:
  nodeId: "node-abc123"
  hostname: kcore01
  rootPassword: s3cret
  disks:
    - device: /dev/sda
      role: os
    - device: /dev/nvme0n1
      role: storage
  runController: true
  controllerAddress: "10.0.0.1:9090"
```

| Field | Required | Description |
|---|---|---|
| `spec.nodeId` | Yes | Target node identifier |
| `spec.hostname` | No | Hostname for the installed system |
| `spec.rootPassword` | No | Root password (default: `kcore`) |
| `spec.disks` | Yes | List of disks with `device` and `role` (`os` or `storage`) |
| `spec.runController` | No | Deploy and enable the controller on this node (default: false) |
| `spec.controllerAddress` | No | Advertised controller gRPC address (required if `runController` is true) |

### NodeNetwork

Configures networking on an installed node.

```yaml
apiVersion: kcore.io/v1
kind: NodeNetwork
metadata:
  name: network-kcore01
spec:
  nodeId: "node-abc123"
  bridges:
    - name: br0
      ports:
        - enp1s0
  bonds:
    - name: bond0
      mode: 802.3ad
      interfaces:
        - enp2s0
        - enp3s0
  vlans:
    - name: vlan100
      id: 100
      parent: bond0
  dnsServers:
    - "1.1.1.1"
    - "8.8.8.8"
  applyNow: true
```

| Field | Required | Description |
|---|---|---|
| `spec.nodeId` | Yes | Target node identifier |
| `spec.bridges` | No | Bridge definitions (`name`, `ports[]`) |
| `spec.bonds` | No | Bond definitions (`name`, `mode`, `interfaces[]`) |
| `spec.vlans` | No | VLAN definitions (`name`, `id`, `parent`) |
| `spec.dnsServers` | No | List of DNS server addresses |
| `spec.applyNow` | No | Apply configuration immediately (default: false) |

### NodeConfig

Pushes a NixOS configuration to a node.

```yaml
apiVersion: kcore.io/v1
kind: NodeConfig
metadata:
  name: config-kcore01
spec:
  nodeId: "node-abc123"
  configurationNix: |
    { config, pkgs, ... }:
    {
      services.openssh.enable = true;
      networking.firewall.allowedTCPPorts = [ 22 9091 ];
    }
  rebuild: true
```

| Field | Required | Description |
|---|---|---|
| `spec.nodeId` | Yes | Target node identifier |
| `spec.configurationNix` | No | Inline NixOS configuration (mutually exclusive with `configFile`) |
| `spec.configFile` | No | Path to a local `.nix` file to upload (mutually exclusive with `configurationNix`) |
| `spec.rebuild` | No | Run `nixos-rebuild switch` after pushing (default: false) |

---

## Testing

```bash
# Run kctl unit tests
go test ./cmd/kctl/... -count=1

# Run all project tests
go test ./cmd/kctl/... ./node/... ./pkg/... ./test/... -count=1
```

---

## See Also

- [QUICKSTART.md](QUICKSTART.md) - Installation guide
- [COMMANDS.md](COMMANDS.md) - All make commands
- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture
- [TERRAFORM_PROVIDER.md](TERRAFORM_PROVIDER.md) - Terraform provider docs
