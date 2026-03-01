# kctl - kcore CLI Tool

`kctl` is the command-line interface for managing kcore clusters. Similar to `kubectl` for Kubernetes, `kctl` provides a user-friendly way to create, manage, and delete VMs and other resources.

---

## Installation

### Building from Source

```bash
# Clone repository
git clone https://github.com/rtacconi/kcore.git
cd kcore

# Build kctl (for macOS ARM64)
make kctl

# Binary will be at: bin/kctl
```

### Using devbox

```bash
devbox shell
devbox run build-kctl
```

### Add to PATH

```bash
# Add to your shell profile (~/.zshrc or ~/.bashrc)
export PATH="$PATH:/path/to/kcore/bin"

# Or copy to system path
sudo cp bin/kctl /usr/local/bin/
```

---

## Quick Start

```bash
# Get help
kctl --help

# Create a VM
kctl create vm web-server --cpu 4 --memory 8G --disk 100G

# List all VMs
kctl get vms

# Get VM details
kctl describe vm web-server

# Delete a VM
kctl delete vm web-server
```

---

## Commands Reference

### `kctl create`

Create resources in the kcore cluster.

#### Create VM

```bash
kctl create vm NAME [flags]
```

**Flags:**
- `--cpu` - Number of CPU cores (default: 2)
- `--memory` - Memory size, e.g., 2G, 4096M (default: 2G)
- `--disk` - Disk size, e.g., 100G, 50000M (default: 20G)
- `--image` - OS image to use (optional)
- `--node` - Specific node to run on (optional)
- `--network` - Network to connect to (default: default)
- `--autostart` - Start VM automatically after creation

**Examples:**

```bash
# Create a basic VM
kctl create vm web-01 --cpu 4 --memory 8G

# Create a VM with large disk
kctl create vm db-01 --cpu 8 --memory 16G --disk 500G

# Create a VM on specific node
kctl create vm cache-01 --cpu 2 --memory 4G --node kvm-node-02

# Create a VM with an image
kctl create vm ubuntu-vm --cpu 2 --memory 2G --image ubuntu-22.04

# Create a VM with autostart
kctl create vm web-server --cpu 4 --memory 8G --autostart
```

#### Create Volume

```bash
kctl create volume NAME [flags]
```

**Flags:**
- `--size` - Volume size (default: 10G)
- `--storage-class` - Storage class to use (default: local-dir)
- `--node` - Specific node to create on (optional)

**Examples:**

```bash
# Create a 100GB volume
kctl create volume data-vol --size 100G

# Create a volume with specific storage class
kctl create volume db-vol --size 500G --storage-class local-lvm
```

#### Create Network

```bash
kctl create network NAME [flags]
```

**Flags:**
- `--subnet` - Network subnet, e.g., 192.168.1.0/24
- `--bridge` - Linux bridge to use (default: br0)

**Examples:**

```bash
# Create a network
kctl create network private-net --subnet 192.168.100.0/24

# Create a network with specific bridge
kctl create network dmz-net --bridge br1
```

---

### `kctl get`

Display one or many resources.

```bash
kctl get RESOURCE [NAME] [flags]
```

**Resources:**
- `vms`, `vm` - Virtual machines
- `nodes`, `node` - Cluster nodes
- `volumes`, `volume`, `vol` - Storage volumes
- `networks`, `network`, `net` - Virtual networks

**Flags:**
- `-o, --output` - Output format: json, yaml, wide
- `-l, --selector` - Label selector to filter resources
- `-A, --all-namespaces` - List resources across all namespaces

**Examples:**

```bash
# List all VMs
kctl get vms

# Get specific VM
kctl get vm web-server

# List VMs with wide output
kctl get vms -o wide

# Get VM in JSON format
kctl get vm web-server -o json

# List all nodes
kctl get nodes

# Get specific node
kctl get node kvm-node-01

# List volumes
kctl get volumes

# List networks
kctl get networks
```

---

### `kctl describe`

Show detailed information about a resource.

```bash
kctl describe RESOURCE NAME
```

**Resources:**
- `vm` - Virtual machine
- `node` - Cluster node
- `volume` - Storage volume
- `network` - Virtual network

**Examples:**

```bash
# Describe a VM
kctl describe vm web-server

# Describe a node
kctl describe node kvm-node-01

# Describe a volume
kctl describe volume data-vol

# Describe a network
kctl describe network private-net
```

**Sample Output:**

```
Name:           web-server
ID:             5fc2b3d5-57e0-4991-bc1e-349ee5ec3784
Status:         Running
Node:           kvm-node-01

Resources:
  CPU:          4 cores
  Memory:       8GB
  Disk:         100GB

Network:
  Network:      default
  IP Address:   192.168.1.100
  MAC Address:  52:54:00:12:34:56

Configuration:
  Autostart:    false
  Boot Device:  hd

Timestamps:
  Created:      2025-11-10 14:30:00 UTC
  Started:      2025-11-10 14:32:15 UTC
  Age:          2d

Events:
  2m ago   Normal    Started      VM started successfully
  1h ago   Normal    Migrated     VM migrated from kvm-node-02
```

---

### `kctl delete`

Delete resources by name.

```bash
kctl delete RESOURCE NAME [flags]
```

**Flags:**
- `--force` - Force deletion without confirmation
- `--all` - Delete all resources of the specified type

**Examples:**

```bash
# Delete a VM (with confirmation)
kctl delete vm web-server

# Force delete without confirmation
kctl delete vm web-server --force

# Delete a volume
kctl delete volume data-vol

# Delete a network
kctl delete network private-net
```

---

### `kctl apply`

Apply a configuration from a file.

```bash
kctl apply -f FILENAME [flags]
```

**Flags:**
- `-f, --filename` - Filename to apply (required)
- `--dry-run` - Show what would be applied without applying

**Examples:**

```bash
# Apply configuration from a file
kctl apply -f vm.yaml

# Apply multiple files
kctl apply -f vm1.yaml -f vm2.yaml

# Dry run (show what would be applied)
kctl apply -f vm.yaml --dry-run
```

**Sample YAML:**

```yaml
apiVersion: kcore.io/v1
kind: VirtualMachine
metadata:
  name: web-server
spec:
  cpu: 4
  memory: 8G
  disk: 100G
  network: default
  autostart: false
```

---

### `kctl version`

Print the kctl version.

```bash
kctl version
```

---

## Global Flags

These flags are available for all commands:

- `-c, --config` - Path to kctl config file (default: `$HOME/.kcore/config`)
- `-s, --controller` - Controller address (overrides config)
- `-k, --insecure` - Skip TLS certificate verification
- `-h, --help` - Help for any command

---

## Configuration

kctl looks for configuration in `$HOME/.kcore/config` by default.

**Sample Configuration:**

```yaml
controller:
  address: 192.168.1.100:9090
  tls:
    ca: /path/to/ca.crt
    cert: /path/to/client.crt
    key: /path/to/client.key
    insecure: false

defaults:
  namespace: default
  timeout: 30s
```

You can override the config file location:

```bash
kctl --config /path/to/config get vms
```

Or override the controller address:

```bash
kctl --controller 192.168.1.100:9090 get vms
```

---

## Common Workflows

### Deploy a New VM

```bash
# Method 1: Using create command
kctl create vm web-01 --cpu 4 --memory 8G --disk 100G

# Method 2: Using YAML file
cat > vm.yaml <<EOF
apiVersion: kcore.io/v1
kind: VirtualMachine
metadata:
  name: web-01
spec:
  cpu: 4
  memory: 8G
  disk: 100G
  network: default
EOF

kctl apply -f vm.yaml
```

### List and Inspect Resources

```bash
# List all VMs
kctl get vms

# Get details of specific VM
kctl describe vm web-01

# List all nodes
kctl get nodes

# Get node details
kctl describe node kvm-node-01
```

### Manage VM Lifecycle

```bash
# Create VM
kctl create vm web-01 --cpu 4 --memory 8G

# Check status
kctl get vm web-01

# Get detailed info
kctl describe vm web-01

# Delete VM
kctl delete vm web-01
```

### Manage Storage

```bash
# Create a volume
kctl create volume data-vol --size 100G

# List volumes
kctl get volumes

# Describe volume
kctl describe volume data-vol

# Delete volume
kctl delete volume data-vol
```

---

## Comparison with grpcurl

**Before (using grpcurl):**

```bash
# Complex command with lots of parameters
VM_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')
grpcurl -insecure \
  -cert ./certs/node.crt -key ./certs/node.key \
  -import-path ./proto -proto node.proto \
  -d '{"spec": {"id": "'$VM_ID'", "name": "test-vm", "cpu": 2, "memory_bytes": 2147483648}}' \
  192.168.40.146:9091 kcore.node.NodeCompute/CreateVm
```

**Now (using kctl):**

```bash
# Simple, user-friendly command
kctl create vm test-vm --cpu 2 --memory 2G
```

---

## Tips

### Auto-completion

Generate shell completion:

```bash
# Bash
kctl completion bash > /usr/local/etc/bash_completion.d/kctl

# Zsh
kctl completion zsh > "${fpath[1]}/_kctl"

# Fish
kctl completion fish > ~/.config/fish/completions/kctl.fish
```

### Aliases

Add to your shell profile:

```bash
# Short aliases
alias k=kctl
alias kg='kctl get'
alias kd='kctl describe'
alias kdel='kctl delete'

# Common commands
alias kvms='kctl get vms'
alias knodes='kctl get nodes'
```

### Output Formatting

```bash
# Table format (default)
kctl get vms

# Wide format (more columns)
kctl get vms -o wide

# JSON format
kctl get vms -o json

# YAML format
kctl get vms -o yaml
```

---

## Troubleshooting

### Connection Issues

```bash
# Test connection to controller
kctl --controller 192.168.1.100:9090 get nodes

# Skip TLS verification (for testing)
kctl --insecure get nodes
```

### Debug Mode

```bash
# Add -v for verbose output (TODO: implement)
kctl -v get vms
```

### Common Errors

**"connection refused"**
- Check controller is running
- Verify controller address in config
- Check firewall rules

**"certificate verify failed"**
- Use `--insecure` flag for testing
- Ensure certificates are valid
- Check CA certificate path in config

---

## Development

kctl is built with:
- **Language**: Go
- **CLI Framework**: [Cobra](https://github.com/spf13/cobra)
- **gRPC**: Communicates with kcore controller

### Building

```bash
# Build for macOS ARM64
make kctl

# Build for specific platform
GOOS=darwin GOARCH=arm64 go build -o bin/kctl ./cmd/kctl

# Build for Linux
GOOS=linux GOARCH=amd64 go build -o bin/kctl-linux ./cmd/kctl
```

### Testing

```bash
# Run tests
go test ./cmd/kctl/...

# Build and test
make kctl && ./bin/kctl --help
```

---

## Roadmap

- [ ] Connect to actual controller gRPC API
- [ ] Implement config file loading
- [ ] Add watch mode (`kctl get vms --watch`)
- [ ] Add logs command (`kctl logs vm-name`)
- [ ] Add exec command (`kctl exec vm-name -- command`)
- [ ] Add port-forward command
- [ ] Add context management (multiple clusters)
- [ ] Add output filtering and sorting
- [ ] Add interactive mode
- [ ] Shell auto-completion improvements

---

## See Also

- [QUICKSTART.md](QUICKSTART.md) - Installation guide
- [COMMANDS.md](COMMANDS.md) - All make commands
- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture

