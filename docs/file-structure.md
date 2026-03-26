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
│   │       ├── scheduler.rs         node selection for ready/capacity-fit placement
│   │       ├── nixgen.rs            generates declarative Nix VM config from DB rows with escaping
│   │       ├── node_client.rs       outbound gRPC client pool to node-agents (TLS or plain)
│   │       ├── auth.rs              CN-based authorization rules for controller RPCs
│   │       └── grpc/
│   │           ├── mod.rs           re-exports gRPC service modules
│   │           ├── controller.rs    Controller service impl (VM/node/network APIs, storage checks, config push)
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
│   │       │   ├── info.rs          NodeInfo: returns hostname, capacity/usage, and backend info surface
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
    ├── networking.md                VM network model, examples, and operational guidance
    ├── migrations.md                DB/API migration notes and rollout guidance
    ├── heartbeat.md                 controller/node heartbeat behavior and liveness semantics
    ├── scheduler.md                 scheduling strategy and node selection behavior
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

- `controller.proto` defines the API that `kctl` calls to manage the cluster (VM CRUD, node listing/heartbeats, node+VM storage fields).
- `node.proto` defines the API that each node-agent exposes (admin ops including streaming image upload, typed install storage, and VM SSH readiness checks; compute/storage/system info).

### NixOS modules (`modules/`)

- `ch-vm/` is the declarative VM module. When the controller pushes a generated Nix config to a node, this module realizes it: it creates bridges, TAP devices, NAT rules, cloud-init ISOs, and systemd services that launch cloud-hypervisor.
- `kcore-branding.nix` sets the OS identity (login banner, MOTD, labels).
- `kcore-minimal.nix` strips the NixOS install to a lean server base.

### Build system (`flake.nix`, `Makefile`)

- `flake.nix` defines the Nix flake with reproducible Rust builds via Crane, development shell, NixOS ISO generation, and VM integration tests.
- `Makefile` wraps common cargo commands (`build`, `test`, `clippy`, `fmt`, `audit`) and ISO build targets.

### Tests (`tests/`)

- `vm-module.nix` is a NixOS VM test that boots an ephemeral test machine with the `ch-vm` module enabled and verifies that bridges, TAP devices, and VM service units are correctly created.

## Storage Backend Implementation Map

This list focuses on files that implement the end-to-end storage backend flow (filesystem/lvm/zfs) across install, controller validation, VM create, and generated Nix.

- `proto/controller.proto` — controller API contract for node storage capability and VM storage backend/size fields.
- `proto/node.proto` — node API contract for typed install storage backend and backend-specific install options.
- `crates/controller/src/db.rs` — persistence for node storage backend and VM storage metadata, including migrations.
- `crates/controller/src/grpc/validation.rs` — request validation/normalization for storage backend enums and storage size constraints.
- `crates/controller/src/grpc/controller.rs` — register/list/get node storage capability and VM create compatibility enforcement.
- `crates/controller/src/scheduler.rs` — capacity-based node selection helper used by storage-compatible placement path.
- `crates/controller/src/nixgen.rs` — renders VM storage backend/size into generated node Nix configuration.
- `crates/controller/src/config.rs` — controller runtime settings loader/validator used by API server startup.
- `crates/controller/src/main.rs` — controller process bootstrap and gRPC server registration.
- `crates/kctl/src/main.rs` — CLI flag definitions for storage backend options on `create vm` and `node install`.
- `crates/kctl/src/commands/vm.rs` — maps storage flags into controller `CreateVmRequest`.
- `crates/kctl/src/commands/node.rs` — maps install storage flags into node `InstallToDiskRequest`.
- `crates/kctl/src/output.rs` — displays node storage backend information in CLI output.
- `crates/node-agent/src/grpc/admin.rs` — install-to-disk argument builder with typed storage/backend-specific options and compatibility fallback.
- `crates/node-agent/src/config.rs` — node-agent storage backend config schema (`filesystem`, `lvm`, `zfs`) and validation.
- `crates/node-agent/src/storage/mod.rs` — storage adapter trait + filesystem/lvm/zfs implementations for volume/image ops.
- `crates/node-agent/src/grpc/info.rs` — node info endpoint implementation (capacity/usage reporting surface).
- `crates/node-agent/src/grpc/compute.rs` — VM/image compute operations (runtime VM/image handling paths).
- `crates/node-agent/src/main.rs` — node-agent bootstrap and storage adapter wiring into gRPC services.
- `modules/ch-vm/options.nix` — Nix option surface extended for storage-enriched VM definitions.
- `docs/images.md` — operator-facing image + VM create guidance, including storage-aware examples.
- `docs/kctl-commands-and-workflows.md` — end-to-end CLI workflows and storage backend command examples.
- `README.md` — top-level operator workflow summary including storage-enriched VM creation.

## Full File Catalog

This is a complete source/docs catalog (excluding build artifacts like `target/` and `result-*` symlinks).  
For each file: purpose + where it is used in runtime/operator flows.

### Workspace Root

- `Cargo.toml` — workspace manifest; defines member crates and shared dependency policy.
- `Cargo.lock` — exact dependency resolution used by CI and local reproducible builds.
- `flake.nix` — Nix build graph (packages/checks/dev shell/ISO outputs); canonical reproducible entrypoint.
- `flake.lock` — pinned flake input revisions to keep builds deterministic across machines.
- `Makefile` — convenience wrappers for `cargo`/Nix workflows (build, test, lint, ISO automation).
- `VERSION` — version marker consumed by release/build flows.
- `README.md` — operator-facing quickstart and command workflows; first-stop onboarding doc.
- `.gitignore` — excludes build artifacts, result symlinks, and local transient files from VCS.

### API Contracts

- `proto/controller.proto` — control-plane API (`RegisterNode`, `CreateVm`, networks, SSH keys, drain); source-of-truth contract for controller behavior.
- `proto/node.proto` — node API (`NodeAdmin`, `NodeCompute`, `NodeStorage`, `NodeInfo`); defines install/upload/readiness/storage RPC contracts.

### Controller Crate (`crates/controller`)

- `crates/controller/Cargo.toml` — controller crate deps and compile features.
- `crates/controller/build.rs` — protobuf build step generating Rust stubs for controller and node client usage.
- `crates/controller/src/main.rs` — process bootstrap: config loading, TLS setup, gRPC service registration, shutdown handling.
- `crates/controller/src/config.rs` — YAML schema/defaults/validation for listen address, DB path, TLS material, network defaults.
- `crates/controller/src/db.rs` — SQLite schema + migrations + typed row mapping; central persistence layer for nodes/VMs/networks/keys.
- `crates/controller/src/scheduler.rs` — placement heuristics (`select_node*`) used by VM creation/drain.
- `crates/controller/src/nixgen.rs` — renders node Nix config from DB state (VMs/networks/storage/cloud-init/SSH key injection).
- `crates/controller/src/node_client.rs` — connection pool and typed gRPC clients to node-agent admin/compute services.
- `crates/controller/src/auth.rs` — peer identity checks (CN-based authorization gates on controller RPCs).
- `crates/controller/src/grpc/mod.rs` — module aggregator for grpc service implementations.
- `crates/controller/src/grpc/helpers.rs` — shared conversion/time/parsing helpers used by grpc handlers.
- `crates/controller/src/grpc/validation.rs` — input validation and normalization (image, network, storage); enforces API invariants before DB writes.
- `crates/controller/src/grpc/controller.rs` — main controller business logic for node register/heartbeat, VM lifecycle, network CRUD, scheduling, and push/rollback behavior.
- `crates/controller/src/grpc/admin.rs` — controller-side admin API (apply nix on controller host).

### Node-Agent Crate (`crates/node-agent`)

- `crates/node-agent/Cargo.toml` — node-agent crate deps and compile features.
- `crates/node-agent/build.rs` — protobuf build step generating node server stubs.
- `crates/node-agent/src/main.rs` — node-agent bootstrap: config/auth/TLS/server wiring and storage adapter injection.
- `crates/node-agent/src/config.rs` — node runtime config model, including storage backend settings and backend-specific required fields.
- `crates/node-agent/src/auth.rs` — RPC authorization checks for kctl/controller peer identities.
- `crates/node-agent/src/grpc/mod.rs` — module aggregator for grpc services.
- `crates/node-agent/src/grpc/admin.rs` — admin control surface: apply nix, install-to-disk orchestration, image upload stream handling, VM SSH readiness probes.
- `crates/node-agent/src/grpc/compute.rs` — runtime VM/image operations exposed to controller (list/get/set/delete/pull image flows).
- `crates/node-agent/src/grpc/info.rs` — node telemetry endpoint (capacity/usage + backend surface for discovery/placement).
- `crates/node-agent/src/grpc/storage.rs` — NodeStorage RPC facade mapping volume calls to storage adapter implementation.
- `crates/node-agent/src/discovery/mod.rs` — discovery module exports.
- `crates/node-agent/src/discovery/disks.rs` — host block-device discovery used by install workflows.
- `crates/node-agent/src/discovery/nics.rs` — NIC/address discovery used by operator inspection.
- `crates/node-agent/src/storage/mod.rs` — storage abstraction + FS/LVM/ZFS adapters; image ensure/upload and volume create/delete logic.
- `crates/node-agent/src/vmm/mod.rs` — VMM module exports.
- `crates/node-agent/src/vmm/client.rs` — cloud-hypervisor socket API client for VM status/config introspection.
- `crates/node-agent/src/vmm/types.rs` — typed deserialization structs for cloud-hypervisor payloads.

### kctl Crate (`crates/kctl`)

- `crates/kctl/Cargo.toml` — CLI crate dependencies and features.
- `crates/kctl/build.rs` — protobuf client stub generation for controller/node services.
- `crates/kctl/src/main.rs` — command model and top-level dispatch; maps CLI flags into command handlers.
- `crates/kctl/src/config.rs` — context resolution and cert path lookup (`~/.kcore/config` and context directories).
- `crates/kctl/src/client.rs` — gRPC channel/client construction with TLS/insecure modes and message size tuning.
- `crates/kctl/src/output.rs` — standardized tabular/detail rendering for nodes/VMs/disks/NICs.
- `crates/kctl/src/pki.rs` — certificate generation/loading utilities for bootstrap and install flows.
- `crates/kctl/src/commands/mod.rs` — command module exports.
- `crates/kctl/src/commands/apply.rs` — `kctl apply` controller admin workflow implementation.
- `crates/kctl/src/commands/cluster.rs` — cluster bootstrap workflow (PKI + context generation).
- `crates/kctl/src/commands/image.rs` — image pull/delete node operations.
- `crates/kctl/src/commands/network.rs` — controller-backed network create/list/delete operations.
- `crates/kctl/src/commands/node.rs` — node commands (disks/nics/install/apply/upload/readiness helpers).
- `crates/kctl/src/commands/ssh_key.rs` — SSH key lifecycle commands against controller API.
- `crates/kctl/src/commands/vm.rs` — VM create/update/get/list/set/delete + wait/SSH readiness polling logic.

### Nix Modules

- `modules/ch-vm/default.nix` — entrypoint importing VM/network/cloud-init/service submodules.
- `modules/ch-vm/options.nix` — typed option schema for VM/network/storage and module-level settings.
- `modules/ch-vm/networking.nix` — realizes bridges, TAPs, firewall/NAT, and per-network forwarding.
- `modules/ch-vm/vm-service.nix` — generates systemd unit definitions and cloud-hypervisor launch arguments per VM.
- `modules/ch-vm/cloud-init.nix` — creates cloud-init seed artifacts per VM from module options.
- `modules/ch-vm/helpers.nix` — helper functions for deterministic naming and MAC generation.
- `modules/kcore-branding.nix` — system identity/branding (MOTD/banner/labels).
- `modules/kcore-minimal.nix` — minimal base profile used for lean appliance-like systems.

### Tests & Scripts

- `tests/vm-module.nix` — integration NixOS test asserting module wiring and generated runtime units/networking.
- `scripts/build-iso-remote.sh` — helper script to build the ISO on a remote host.

### Documentation

- `docs/Architecture.md` — system architecture and high-level component interactions.
- `docs/networking.md` — network model, topology examples, and operator workflows.
- `docs/migrations.md` — migration/versioning guidance for schema/API changes.
- `docs/heartbeat.md` — heartbeat semantics and node liveness behavior.
- `docs/scheduler.md` — scheduler policy, inputs, and expected placement behavior.
- `docs/security.md` — mTLS/CN auth model, trust boundaries, and security notes.
- `docs/kctl-commands-and-workflows.md` — command-by-command operator reference.
- `docs/images.md` — image upload/download/create/wait flows and troubleshooting.
- `docs/node-install-bootstrap-flow.md` — install bootstrap sequence and cert handoff process.
- `docs/nix-vm-config-generation.md` — how controller-generated Nix config is built and applied.
- `docs/mtls-bootstrap-and-auth.md` — certificate bootstrap and auth flow details.
- `docs/formal-methods-and-verification.md` — verification concepts and future hardening notes.
- `docs/file-structure.md` — this catalog + architecture-oriented file map.
