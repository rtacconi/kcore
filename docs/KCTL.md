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
