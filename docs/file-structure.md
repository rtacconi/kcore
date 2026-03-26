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

- `Cargo.toml` — workspace manifest defining member crates and shared settings.
- `Cargo.lock` — locked Rust dependency graph for reproducible builds.
- `flake.nix` — Nix flake entrypoint for packages, checks, dev shell, and ISO builds.
- `flake.lock` — pinned Nix inputs for reproducible flake resolution.
- `Makefile` — convenience targets for common build/test/lint workflows.
- `VERSION` — repository version marker.
- `README.md` — project overview, quick-start, and operator workflow summary.
- `.gitignore` — ignored files and directories for git.

- `proto/controller.proto` — controller public gRPC API.
- `proto/node.proto` — node-agent public gRPC API.

- `crates/controller/Cargo.toml` — controller crate manifest and dependencies.
- `crates/controller/build.rs` — protobuf code generation for controller build.
- `crates/controller/src/main.rs` — controller process startup and gRPC server wiring.
- `crates/controller/src/config.rs` — controller runtime config schema and validation.
- `crates/controller/src/db.rs` — SQLite schema, migrations, and persistence helpers.
- `crates/controller/src/scheduler.rs` — scheduling and capacity-based node selection logic.
- `crates/controller/src/nixgen.rs` — generated node Nix config renderer from DB state.
- `crates/controller/src/node_client.rs` — outbound controller-side clients to node-agent endpoints.
- `crates/controller/src/auth.rs` — controller RPC peer identity authorization checks.
- `crates/controller/src/grpc/mod.rs` — grpc module re-exports.
- `crates/controller/src/grpc/helpers.rs` — grpc helper utilities and transformations.
- `crates/controller/src/grpc/validation.rs` — request validation and normalization helpers.
- `crates/controller/src/grpc/controller.rs` — main controller service RPC implementation.
- `crates/controller/src/grpc/admin.rs` — controller admin RPC implementation.

- `crates/node-agent/Cargo.toml` — node-agent crate manifest and dependencies.
- `crates/node-agent/build.rs` — protobuf code generation for node-agent build.
- `crates/node-agent/src/main.rs` — node-agent process startup and service registration.
- `crates/node-agent/src/config.rs` — node-agent runtime config schema and validation.
- `crates/node-agent/src/auth.rs` — node-agent RPC peer identity authorization checks.
- `crates/node-agent/src/grpc/mod.rs` — grpc module re-exports.
- `crates/node-agent/src/grpc/admin.rs` — admin endpoints (apply nix, install, upload, readiness checks).
- `crates/node-agent/src/grpc/compute.rs` — compute endpoints for VM/image runtime operations.
- `crates/node-agent/src/grpc/info.rs` — node info endpoint implementation.
- `crates/node-agent/src/grpc/storage.rs` — storage service RPC implementation facade.
- `crates/node-agent/src/discovery/mod.rs` — discovery module re-exports.
- `crates/node-agent/src/discovery/disks.rs` — disk discovery helpers.
- `crates/node-agent/src/discovery/nics.rs` — network interface discovery helpers.
- `crates/node-agent/src/storage/mod.rs` — storage adapter abstraction and FS/LVM/ZFS backends.
- `crates/node-agent/src/vmm/mod.rs` — VMM module re-exports.
- `crates/node-agent/src/vmm/client.rs` — cloud-hypervisor socket client.
- `crates/node-agent/src/vmm/types.rs` — deserialization types for VMM API payloads.

- `crates/kctl/Cargo.toml` — kctl crate manifest and dependencies.
- `crates/kctl/build.rs` — protobuf code generation for kctl client stubs.
- `crates/kctl/src/main.rs` — CLI entrypoint, argument model, and dispatch.
- `crates/kctl/src/config.rs` — context/certificate config resolution.
- `crates/kctl/src/client.rs` — gRPC client/channel construction helpers.
- `crates/kctl/src/output.rs` — table/detail output formatting helpers.
- `crates/kctl/src/pki.rs` — PKI generation and loading helpers.
- `crates/kctl/src/commands/mod.rs` — command module re-exports.
- `crates/kctl/src/commands/apply.rs` — controller apply command logic.
- `crates/kctl/src/commands/cluster.rs` — cluster bootstrap command logic.
- `crates/kctl/src/commands/image.rs` — image management command logic.
- `crates/kctl/src/commands/network.rs` — network create/list/delete command logic.
- `crates/kctl/src/commands/node.rs` — node inspection/install/apply/upload command logic.
- `crates/kctl/src/commands/ssh_key.rs` — SSH key management command logic.
- `crates/kctl/src/commands/vm.rs` — VM lifecycle/create/wait command logic.

- `modules/ch-vm/default.nix` — module entrypoint importing VM submodules.
- `modules/ch-vm/options.nix` — user-facing options for VM/network/storage settings.
- `modules/ch-vm/networking.nix` — bridge, tap, DHCP, and NAT orchestration.
- `modules/ch-vm/vm-service.nix` — per-VM systemd service generation and CH invocation.
- `modules/ch-vm/cloud-init.nix` — cloud-init seed ISO generation.
- `modules/ch-vm/helpers.nix` — helper utilities for deterministic naming/mac.
- `modules/kcore-branding.nix` — OS branding and identity settings.
- `modules/kcore-minimal.nix` — minimal baseline system module.

- `tests/vm-module.nix` — integration-style NixOS VM module test.
- `scripts/build-iso-remote.sh` — remote ISO build helper script.

- `docs/Architecture.md` — architecture overview and diagrams.
- `docs/networking.md` — networking behavior and operator examples.
- `docs/migrations.md` — migration/upgrade notes.
- `docs/heartbeat.md` — heartbeat model and timing semantics.
- `docs/scheduler.md` — scheduling model and decision criteria.
- `docs/security.md` — authn/authz and security model details.
- `docs/kctl-commands-and-workflows.md` — CLI command reference and workflows.
- `docs/images.md` — image upload/create/wait guidance.
- `docs/node-install-bootstrap-flow.md` — node install and bootstrap sequence.
- `docs/nix-vm-config-generation.md` — Nix generation internals and flow.
- `docs/mtls-bootstrap-and-auth.md` — mTLS bootstrap and certificate usage.
- `docs/formal-methods-and-verification.md` — verification notes and future work.
- `docs/file-structure.md` — file map and implementation catalog (this document).
