# kcore

A modern, minimal virtualization platform focused on **datacenters and home labs**.

## Overview

kcore is a clustered virtualization platform built with:

- **Host OS**: NixOS-based "kcore" image (rebranded, minimal)
- **Hypervisor**: KVM via libvirt
- **Control Plane**: Go-based controller with SQLite state
- **Node Agents**: Go-based agents running on kcore nodes
- **Communication**: gRPC with mTLS

## Architecture

```
┌─────────────────┐
│  Control Plane  │  (macOS, SQLite)
│   (Controller)  │
└────────┬────────┘
         │ gRPC (mTLS)
         │
    ┌────┴────┐
    │        │
┌───▼───┐ ┌──▼───┐
│ Node  │ │ Node │  (kcore/NixOS, libvirt)
│ Agent │ │ Agent│
└───────┘ └──────┘
```

## Quick Start

### Option 1: Using Devbox (Recommended)

Devbox provides a reproducible development environment:

```bash
# Enter devbox shell (installs Go, protobuf, Podman, etc.)
devbox shell

# Generate protobuf code
make proto

# Build controller (macOS)
make controller

# Build node-agent (Linux/amd64) - uses Podman automatically
make node-agent
```

### Option 2: Manual Setup

1. **Install Dependencies**

   - Go 1.22+
   - Protocol Buffers compiler (`protoc`)
   - Go protobuf plugins:
     ```bash
     go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
     go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@latest
     ```
   - Podman (for cross-compiling node-agent)

2. **Generate Protobuf Code**

```bash
# Option 1: Use the script
./scripts/generate-proto.sh

# Option 2: Use make
make proto
```

3. **Build the Project**

   ```bash
   # Build control plane (macOS)
   make controller

   # Build node agent (Linux/amd64) - requires Podman
   make node-agent
   ```

   **Note:** The node-agent requires CGO and libvirt, so cross-compilation from macOS uses Podman. See [CROSS_COMPILATION.md](CROSS_COMPILATION.md) for details.

4. **Set Up Certificates**

Create TLS certificates for mTLS communication:

```bash
mkdir -p certs
# Generate CA, controller cert, and node certs
# (Use your preferred tool: openssl, cfssl, etc.)
```

### 3. Configure Controller

Create `controller.yaml`:

```yaml
databasePath: ./kcore.db
listenAddr: ":9090"
tls:
  caFile: ./certs/ca.crt
  certFile: ./certs/controller.crt
  keyFile: ./certs/controller.key
nodeNetworks:
  default: br0
```

### 5. Start Controller

```bash
./bin/kcore-controller
```

### 6. Deploy Node Agent

On each ThinkCentre node:

1. Install kcore (NixOS-based) using the flake
2. Copy node agent binary to `/opt/kcore/kcore-node-agent`
3. Create `/etc/kcore/node-agent.yaml`
4. Start the service: `systemctl start kcore-node-agent`

Or use the deployment script:

```bash
./scripts/deploy-node-agent.sh <node-ip> root
```

### 7. Apply VM Specs

```bash
# Create storage classes
./bin/kcore-controller -apply-vm examples/storage-classes.yaml

# Create a VM
./bin/kcore-controller -apply-vm examples/vm.yaml
```

## NixOS Flake

Build a kcore node image:

```bash
nix build .#images.kcore-node
```

This produces a bootable ISO/image that can be written to USB.

## Project Structure

```
kcore/
├── cmd/
│   ├── controller/     # Control plane binary
│   └── node-agent/      # Node agent binary
├── pkg/
│   ├── config/          # Configuration types and YAML parsing
│   └── sqlite/          # Database schema and access
├── pkg/controller/      # Control plane reconciliation logic
├── node/
│   ├── libvirt/         # Libvirt integration
│   ├── storage/         # Storage drivers
│   └── server.go         # gRPC server implementation
├── api/                 # Generated protobuf code
├── proto/               # Protobuf definitions
├── modules/             # NixOS modules
│   ├── kcore-minimal.nix
│   ├── kcore-branding.nix
│   ├── kcore-node-agent.nix
│   └── kcore-libvirt.nix
├── examples/            # Example YAML specs
└── scripts/             # Deployment scripts
```

## Storage Drivers

kcore supports multiple storage backends:

- **local-dir**: qcow2 files in a directory
- **local-lvm**: LVM logical volumes
- **linstor**: LINSTOR-backed volumes (planned)
- **san-iscsi/san-fc**: SAN-backed volumes (planned)

## Networking

Currently supports Linux bridges. Each network maps to a bridge name on the node.

## Development

### Prerequisites

- Go 1.22+
- Nix (for building NixOS images)
- protoc (for generating protobuf code)
- libvirt development libraries (for node agent)

### Running Tests

```bash
make test
```

### Cross-compilation

The Makefile includes targets for cross-compiling the node agent:

```bash
make node-agent  # Builds for Linux/amd64
```

## License

[Add your license here]

## Contributing

[Add contribution guidelines here]

