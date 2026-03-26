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
    ├── storage.md                   storage backend model, commands, generated config, and file map
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

## Full File Catalog

This is a complete source/docs catalog (excluding build artifacts like `target/` and `result-*` symlinks).  
For each file: purpose + where it is used in runtime/operator flows.

### Workspace Root

- `Cargo.toml` — workspace manifest; declares crate members, shared metadata, and central dependency/version policy used by all Rust packages.
- `Cargo.lock` — fully resolved dependency graph; locks transitive crate versions so local/CI/release builds use identical artifacts.
- `flake.nix` — primary Nix entrypoint; defines reproducible package outputs, checks, dev shell tooling, and ISO-related build targets.
- `flake.lock` — pinned revisions for Nix inputs; prevents drift in toolchains and package sets across machines and time.
- `Makefile` — operator/developer convenience layer; wraps common `cargo` and Nix commands into predictable high-level targets.
- `VERSION` — single version source used by packaging/release flows and quick version inspection.
- `README.md` — top-level project guide; explains architecture at a glance and points operators to core workflows.
- `.gitignore` — repository hygiene rules; excludes generated artifacts, local scratch files, and non-source outputs.

### API Contracts

- `proto/controller.proto` — control-plane API contract; defines node registration/heartbeat, VM lifecycle, network APIs, SSH key APIs, and drain orchestration structures.
- `proto/node.proto` — node service API contract; defines admin/compute/storage/info RPCs, install-to-disk requests, image transfer APIs, and readiness probing messages.

### Controller Crate (`crates/controller`)

- `crates/controller/Cargo.toml` — controller crate definition; declares runtime deps (`tonic`, `rusqlite`, etc.), build dependencies, and binary metadata.
- `crates/controller/build.rs` — proto compilation hook; generates Rust bindings for controller server traits and node client stubs at build time.
- `crates/controller/src/main.rs` — controller runtime bootstrap; parses CLI/config, initializes DB/clients/services, configures TLS, and starts gRPC serving loop.
- `crates/controller/src/config.rs` — typed configuration model; handles defaults, deserialization, and validation for listener, network defaults, and certificate file paths.
- `crates/controller/src/db.rs` — persistence core; owns schema migrations, CRUD methods, typed row conversion, and transactional update patterns for cluster state.
- `crates/controller/src/scheduler.rs` — scheduling logic; chooses eligible nodes based on readiness and available capacity for create/drain workflows.
- `crates/controller/src/nixgen.rs` — declarative config renderer; converts DB VM/network/storage state into escaped Nix module configuration text.
- `crates/controller/src/node_client.rs` — node connection manager; builds and caches compute/admin gRPC clients per node endpoint.
- `crates/controller/src/auth.rs` — request authorization helpers; enforces certificate CN-based access control on controller APIs.
- `crates/controller/src/grpc/mod.rs` — grpc module index; re-exports implemented service modules for wiring in `main.rs`.
- `crates/controller/src/grpc/helpers.rs` — grpc utility functions; provides timestamp conversion and shared mapping helpers to reduce handler duplication.
- `crates/controller/src/grpc/validation.rs` — centralized request checks; validates image/network/storage arguments and normalizes enum/string inputs.
- `crates/controller/src/grpc/controller.rs` — primary controller service implementation; contains node registration, VM lifecycle, network CRUD, scheduling, and rollback/error semantics.
- `crates/controller/src/grpc/admin.rs` — admin service implementation for controller host operations (for example applying Nix config to the controller machine).

### Node-Agent Crate (`crates/node-agent`)

- `crates/node-agent/Cargo.toml` — node-agent crate definition; declares dependencies for gRPC serving, host command orchestration, and runtime utilities.
- `crates/node-agent/build.rs` — proto codegen hook; generates server/client bindings required by node services.
- `crates/node-agent/src/main.rs` — node-agent entrypoint; loads config, initializes auth/storage adapters, builds gRPC services, and runs server lifecycle.
- `crates/node-agent/src/config.rs` — node configuration schema; validates node identity, TLS files, and backend-specific storage configuration blocks.
- `crates/node-agent/src/auth.rs` — access control layer for node RPCs; checks calling identity and supports secure/insecure mode constraints.
- `crates/node-agent/src/grpc/mod.rs` — grpc service module index used by server wiring.
- `crates/node-agent/src/grpc/admin.rs` — operational admin surface; handles install orchestration, config apply, image upload streams, and SSH readiness diagnostics.
- `crates/node-agent/src/grpc/compute.rs` — compute/data-plane API handlers; performs VM info/state operations and image runtime actions.
- `crates/node-agent/src/grpc/info.rs` — node facts endpoint; reports node identity/capacity/usage and backend capability data to controller/operators.
- `crates/node-agent/src/grpc/storage.rs` — storage RPC adapter; routes NodeStorage requests to selected backend implementation with validation/error mapping.
- `crates/node-agent/src/discovery/mod.rs` — discovery module aggregator.
- `crates/node-agent/src/discovery/disks.rs` — disk enumerator; gathers block-device metadata for install target selection.
- `crates/node-agent/src/discovery/nics.rs` — NIC enumerator; returns interface state/MAC/address data for diagnostics and setup.
- `crates/node-agent/src/storage/mod.rs` — backend abstraction and implementations; defines storage trait and filesystem/LVM/ZFS logic for volume/image operations.
- `crates/node-agent/src/vmm/mod.rs` — VMM module index for client/types exports.
- `crates/node-agent/src/vmm/client.rs` — cloud-hypervisor socket client; queries VM runtime state/config directly from local API sockets.
- `crates/node-agent/src/vmm/types.rs` — strongly typed VM runtime payload models used by VMM client and grpc handlers.

### kctl Crate (`crates/kctl`)

- `crates/kctl/Cargo.toml` — kctl crate metadata; defines CLI/build dependencies and compile features for operator tooling.
- `crates/kctl/build.rs` — protobuf code generation; emits client bindings for controller and node APIs consumed by command handlers.
- `crates/kctl/src/main.rs` — command-line interface model; defines all flags/subcommands and dispatches to command modules.
- `crates/kctl/src/config.rs` — context management; resolves controller/node endpoints and certificate paths from local config state.
- `crates/kctl/src/client.rs` — gRPC client factory; creates TLS/insecure channels and configures per-client message limits/timeouts.
- `crates/kctl/src/output.rs` — output formatter; standardizes human-readable tables/details for resources and diagnostics.
- `crates/kctl/src/pki.rs` — PKI helper library; generates certs/keys and loads install/bootstrap certificate bundles.
- `crates/kctl/src/commands/mod.rs` — command namespace index.
- `crates/kctl/src/commands/apply.rs` — controller apply flow; sends Nix content to controller admin endpoint.
- `crates/kctl/src/commands/cluster.rs` — bootstrap flow; generates local PKI/context and initial controller targeting config.
- `crates/kctl/src/commands/image.rs` — image command handlers for node pull/list/delete scenarios.
- `crates/kctl/src/commands/network.rs` — network command handlers for create/list/delete against controller APIs.
- `crates/kctl/src/commands/node.rs` — node command handlers; includes install mapping, disk/nic inspection, upload, and node-level operations.
- `crates/kctl/src/commands/ssh_key.rs` — SSH key lifecycle handlers (create/list/get/delete) against controller key APIs.
- `crates/kctl/src/commands/vm.rs` — VM command handlers; includes create/update/list/get/delete plus wait and SSH readiness polling behavior.

### Nix Modules

- `modules/ch-vm/default.nix` — composition entrypoint; imports networking/service/cloud-init submodules into one operator-facing module.
- `modules/ch-vm/options.nix` — module contract; defines typed options for networks, VM specs, and storage-related metadata fields.
- `modules/ch-vm/networking.nix` — host network realization logic; creates bridges/taps/DHCP/NAT/forwarding rules per configured network.
- `modules/ch-vm/vm-service.nix` — VM runtime units generator; builds per-VM systemd services and cloud-hypervisor argument strings.
- `modules/ch-vm/cloud-init.nix` — cloud-init artifact generator; renders user-data/meta-data/network configs into seed ISOs.
- `modules/ch-vm/helpers.nix` — deterministic helper functions for naming, tap derivation, and MAC generation.
- `modules/kcore-branding.nix` — branding layer; defines MOTD, labels, and identity-related presentation settings.
- `modules/kcore-minimal.nix` — baseline hardening/minimal profile; trims nonessential packages/features for appliance-style use.

### Tests & Scripts

- `tests/vm-module.nix` — NixOS integration test; validates module wiring, generated units, and essential network/service behavior.
- `scripts/build-iso-remote.sh` — remote build automation script; orchestrates ISO build steps on a remote Linux builder host.

### Documentation

- `docs/Architecture.md` — architecture narrative; explains control/data-plane boundaries and component interaction patterns.
- `docs/networking.md` — operator networking guide; covers network creation, topology examples, and runtime behavior details.
- `docs/migrations.md` — migration operations guide; tracks schema/API changes and upgrade-safe rollout sequencing.
- `docs/heartbeat.md` — liveness model documentation; explains heartbeat update rules and stale-node handling semantics.
- `docs/scheduler.md` — scheduler behavior guide; documents placement criteria and expected selection outcomes.
- `docs/security.md` — security model reference; describes mTLS identity, authorization checks, and threat-boundary assumptions.
- `docs/kctl-commands-and-workflows.md` — CLI usage manual; command syntax, examples, and operational playbooks.
- `docs/images.md` — image workflow guide; upload/pull/create/wait paths with constraints and troubleshooting.
- `docs/storage.md` — storage backend deep dive; includes command examples, generated config shape, and implementation file mapping.
- `docs/node-install-bootstrap-flow.md` — bootstrap sequence doc; details install-to-disk flow and certificate handoff lifecycle.
- `docs/nix-vm-config-generation.md` — config generation internals; explains how DB state is transformed into node-applied Nix.
- `docs/mtls-bootstrap-and-auth.md` — certificate lifecycle guide; bootstrapping, cert roles, and auth usage in runtime calls.
- `docs/formal-methods-and-verification.md` — verification strategy notes; current checks and future formalization directions.
- `docs/file-structure.md` — repository map and deep catalog; explains file responsibilities and runtime/operator relevance.
