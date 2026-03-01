# kcore Project Structure

This document provides an overview of the kcore project structure.

## Core Components

### Control Plane (`cmd/controller/`)
- Main entry point for the control plane
- Runs on macOS, uses SQLite for state
- Handles VM spec application and reconciliation

### Node Agent (`cmd/node-agent/`)
- Main entry point for node agents
- Runs on kcore (NixOS) nodes
- Communicates with control plane via gRPC

## Packages

### `pkg/config/`
- YAML spec parsing (VM, Volume, StorageClass, Network)
- Configuration loading (ControllerConfig, NodeAgentConfig)
- Size parsing utilities (GiB, MiB, etc.)

### `pkg/sqlite/`
- Database schema and migrations
- CRUD operations for:
  - Nodes
  - VMs and VM placement
  - Volumes
  - Storage classes
  - Networks

### `pkg/controller/`
- Reconciliation logic
- Node selection/scheduling
- VM lifecycle management
- Volume provisioning and attachment

### `node/libvirt/`
- Libvirt connection management
- Domain XML generation from VM specs
- Domain lifecycle operations (create, start, stop, delete)

### `node/storage/`
- Storage driver interface
- Implementations:
  - `local-dir`: qcow2 files
  - `local-lvm`: LVM logical volumes
  - `linstor`: Placeholder
  - `san-*`: Placeholder

### `node/server.go`
- gRPC server implementation
- Implements NodeCompute, NodeStorage, NodeInfo services

## API Definitions (`proto/`)
- `node.proto`: Node agent gRPC services
- `controller.proto`: Controller gRPC services (for future use)

## NixOS Modules (`modules/`)
- `kcore-minimal.nix`: Minimal base system
- `kcore-branding.nix`: kcore branding (GRUB, MOTD, etc.)
- `kcore-node-agent.nix`: Node agent systemd service
- `kcore-libvirt.nix`: Libvirt/KVM configuration

## Examples (`examples/`)
- `vm.yaml`: Example VM specification
- `storage-classes.yaml`: Example storage class definitions
- `controller.yaml`: Controller configuration example
- `node-agent.yaml`: Node agent configuration example

## Scripts (`scripts/`)
- `generate-proto.sh`: Generate protobuf code
- `deploy-node-agent.sh`: Deploy node agent to remote node

## Build System

- `Makefile`: Build targets for controller, node-agent, proto generation
- `flake.nix`: NixOS flake for building kcore node images
- `go.mod`: Go module dependencies

## Next Steps

1. Generate protobuf code: `make proto`
2. Build binaries: `make build`
3. Set up TLS certificates
4. Configure and start controller
5. Deploy node agents to ThinkCentre nodes
6. Apply VM specs via controller

