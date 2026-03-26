# File Structure

This document maps every file in the kcore-rust repository and explains what it does.

## Project tree

```
kcore-rust/
├── Cargo.toml                       workspace manifest
├── Cargo.lock                       pinned dependency versions
├── flake.nix                        Nix flake: packages, dev shell, ISO, tests
├── flake.lock                       pinned Nix inputs
├── Makefile                         convenience targets (build, check, fmt, clippy, audit, ISO)
├── VERSION                          single-line semantic version string
├── README.md                        project overview and quick-start
├── .gitignore                       ignored paths (target, result-*, ISOs)
│
├── proto/
│   ├── controller.proto             gRPC API for the controller (node reg+storage capability, heartbeats, VM CRUD)
│   └── node.proto                   gRPC API for nodes (admin, compute, storage, info, typed install storage)
│
├── crates/
│   ├── controller/                  kcore-controller crate
│   │   ├── Cargo.toml               dependencies (tonic, rusqlite, serde, rcgen)
│   │   ├── build.rs                 tonic-build: compiles controller server + node client protos
│   │   └── src/
│   │       ├── main.rs              CLI entry: loads config, sets up TLS, starts gRPC server
│   │       ├── config.rs            YAML config model (listen addr, DB path, TLS, network defaults)
│   │       ├── db.rs                SQLite layer: nodes/VMs tables, WAL mode, CRUD helpers
│   │       ├── scheduler.rs         node selection (first-ready-node policy)
│   │       ├── nixgen.rs            generates declarative Nix VM config from DB rows with escaping
│   │       ├── node_client.rs       outbound gRPC client pool to node-agents (TLS or plain)
│   │       ├── auth.rs              CN-based authorization rules for controller RPCs
│   │       └── grpc/
│   │           ├── mod.rs           re-exports gRPC service modules
│   │           ├── controller.rs    Controller service impl (create/delete/list VMs, nodes, config push)
│   │           └── admin.rs         controller-side admin RPCs (apply-nix to controller node)
│   │
│   ├── node-agent/                  kcore-node-agent crate
│   │   ├── Cargo.toml               dependencies (tonic, hyper, hyperlocal, serde)
│   │   ├── build.rs                 tonic-build: compiles node server protos
│   │   └── src/
│   │       ├── main.rs              CLI entry: loads config, sets up TLS, starts gRPC server
│   │       ├── config.rs            YAML config model (node ID, listen addr, TLS, socket/nix/storage paths)
│   │       ├── auth.rs              CN-based authorization rules for node RPCs
│   │       ├── grpc/
│   │       │   ├── mod.rs           re-exports gRPC service modules
│   │       │   ├── admin.rs         NodeAdmin: apply nix config, install-to-disk, image upload (unary+stream), VM SSH readiness checks
│   │       │   ├── compute.rs       NodeCompute: VM status queries via cloud-hypervisor sockets
│   │       │   ├── info.rs          NodeInfo: returns hostname, CPU count, memory
│   │       │   └── storage.rs       NodeStorage: volume/image RPCs (stub, declarative guidance)
│   │       ├── storage/
│   │       │   └── mod.rs           storage adapter interface + filesystem/lvm/zfs implementations
│   │       ├── discovery/
│   │       │   ├── mod.rs           re-exports discovery modules
│   │       │   ├── disks.rs         enumerates block devices from /sys/block for install flow
│   │       │   └── nics.rs          enumerates network interfaces from /sys/class/net
│   │       └── vmm/
│   │           ├── mod.rs           re-exports VMM client modules
│   │           ├── client.rs        reads cloud-hypervisor API sockets in /run/kcore for VM state
│   │           └── types.rs         deserialization types for cloud-hypervisor vm.info responses
│   │
│   └── kctl/                        kctl CLI crate
│       ├── Cargo.toml               dependencies (tonic, clap, rcgen, serde)
│       ├── build.rs                 tonic-build: compiles controller + node client protos
│       └── src/
│           ├── main.rs              CLI entry: global flags, subcommand dispatch
│           ├── config.rs            multi-context config model (~/.kcore/config, cluster cert dirs)
│           ├── client.rs            gRPC channel builder with TLS/insecure support
│           ├── output.rs            table formatting for VM and node listings
│           ├── pki.rs               cluster PKI generation (CA, controller, node, kctl certs)
│           └── commands/
│               ├── mod.rs           re-exports command modules
│               ├── vm.rs            VM commands: create (flags/YAML, storage backend+size), delete, get, list, set state, wait/wait-for-ssh readiness
│               ├── node.rs          node commands: disks, nics, install (typed storage opts), apply-nix, upload-image, list, get
│               ├── cluster.rs       cluster commands: create cluster (PKI + context setup)
│               ├── apply.rs         apply commands: push nix config to controller
│               └── image.rs         image commands: pull/delete images on nodes
│
├── modules/
│   ├── ch-vm/                       NixOS module: declarative VM lifecycle on cloud-hypervisor
│   │   ├── default.nix              module entry point, imports all submodules
│   │   ├── options.nix              option declarations (networks, VMs, sockets, images, ports)
│   │   ├── networking.nix           per-network bridges, TAP devices, firewall/NAT, port forwarding
│   │   ├── vm-service.nix           per-VM systemd services (cloud-hypervisor invocation, sockets)
│   │   ├── cloud-init.nix           generates cloud-init seed ISOs (user-data/meta-data) per VM
│   │   └── helpers.nix              utility functions (deterministic TAP name generation)
│   ├── kcore-branding.nix           OS branding: login banner, MOTD, NixOS label, issue text
│   └── kcore-minimal.nix            minimal base config: no docs, en_US locale, lean package set
│
├── tests/
│   └── vm-module.nix                NixOS VM test: imports ch-vm, exercises network/VM config
│
├── scripts/
│   └── build-iso-remote.sh          SSH helper to build the kcore ISO on a remote Linux host
│
└── docs/
    ├── Architecture.md              high-level flow diagrams (Mermaid) and component responsibilities
    ├── security.md                  PKI, CN authorization, input validation, async safety, auditing
    ├── kctl-commands-and-workflows.md   full kctl command reference and operator patterns
    ├── images.md                    VM image workflows: upload, create by path/URL, wait-for-ssh troubleshooting
    ├── node-install-bootstrap-flow.md   node install procedure with cert handoff flowchart
    ├── nix-vm-config-generation.md      when/how Nix VM configs are generated and applied
    ├── mtls-bootstrap-and-auth.md       certificate creation, node bootstrap, runtime mTLS
    ├── formal-methods-and-verification.md   notes on formal verification approaches
    └── file-structure.md            this file
```

## How the pieces fit together

### Control plane (Rust crates)

| Crate | Binary | Role |
|---|---|---|
| `controller` | `kcore-controller` | Central API server. Stores nodes and VMs in SQLite, schedules VMs to nodes, generates Nix config, and pushes it to node-agents via gRPC. |
| `node-agent` | `kcore-node-agent` | Runs on every node. Receives Nix config from controller, writes it to disk, triggers `nixos-rebuild`, discovers VM runtime state from cloud-hypervisor API sockets. |
| `kctl` | `kctl` | Operator CLI. Generates cluster PKI, creates/manages VMs, installs nodes from ISO, and performs admin operations. |

### Protobuf contracts (`proto/`)

- `controller.proto` defines the API that `kctl` calls to manage the cluster (VM CRUD, node listing, heartbeats).
- `node.proto` defines the API that each node-agent exposes (admin ops including streaming image upload and VM SSH readiness checks, compute status, storage, system info).

### NixOS modules (`modules/`)

- `ch-vm/` is the declarative VM module. When the controller pushes a generated Nix config to a node, this module realizes it: it creates bridges, TAP devices, NAT rules, cloud-init ISOs, and systemd services that launch cloud-hypervisor.
- `kcore-branding.nix` sets the OS identity (login banner, MOTD, labels).
- `kcore-minimal.nix` strips the NixOS install to a lean server base.

### Build system (`flake.nix`, `Makefile`)

- `flake.nix` defines the Nix flake with reproducible Rust builds via Crane, development shell, NixOS ISO generation, and VM integration tests.
- `Makefile` wraps common cargo commands (`build`, `test`, `clippy`, `fmt`, `audit`) and ISO build targets.

### Tests (`tests/`)

- `vm-module.nix` is a NixOS VM test that boots an ephemeral test machine with the `ch-vm` module enabled and verifies that bridges, TAP devices, and VM service units are correctly created.
