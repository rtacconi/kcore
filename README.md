<p align="center">
  <img src="assets/kcore-logo.png" alt="KCore" width="300">
</p>

# kcore-rust

`kcore-rust` is a Rust control plane for declarative VM lifecycle management on NixOS hosts.
It is organized as a multi-crate workspace with a controller, node agent, and CLI.

## Current Project State

- Declarative VM lifecycle is implemented end-to-end (`create/delete/set desired state` -> controller DB -> generated Nix -> node apply).
- Runtime transport security uses mTLS between `kctl`, `kcore-controller`, and `kcore-node-agent` when TLS is configured.
- Certificate Common Name (CN)-based authorization is enforced per gRPC method in secure mode.
- Node installation bootstraps cluster PKI material into `/etc/kcore/certs` on installed systems.
- Cloud Hypervisor runtime status is surfaced from node-agent via API sockets under `/run/kcore`.

## Components

- `kcore-controller`: gRPC orchestration API, SQLite-backed desired state, scheduler, and Nix config rendering/push.
- `kcore-node-agent`: node-side gRPC service for admin operations, config apply, install flow, and VM runtime visibility.
- `kctl`: operator CLI for cluster PKI setup, VM lifecycle, node install/admin operations, and image operations.

## Architecture Snapshot

- `kctl` sends intent to controller (and can call node-agent directly for node-scoped operations).
- Controller persists desired state, selects target nodes, and renders `ch-vm.vms` with `nixgen`.
- Node-agent writes generated config, runs `nixos-rebuild`, and reports runtime state back to controller.
- VM networking and units are realized by the `ch-vm` module and executed by `cloud-hypervisor`.

See: [Architecture](docs/Architecture.md)

## Security Snapshot

- mTLS is the default production posture; insecure mode is opt-in via `--allow-insecure` / `--insecure`.
- Cluster PKI is created by `kctl create cluster`.
- Node install flow sends only required cert/key material to target nodes; private key files are written with restricted permissions.
- Known security work still pending: certificate rotation workflows, revocation checks (CRL/OCSP), and finer-grained authorization policies.

See: [Security model](docs/security.md)

## Operator Workflows

- Initialize cluster PKI/context: `kctl create cluster --controller <host:9090>`
- Install nodes from live ISO: `kctl --node <host:9091> node install ...`
- Create VMs with a direct HTTPS image URL + checksum:
  `kctl create vm <name> --storage-backend <filesystem|lvm|zfs> --storage-size-bytes <bytes> --image <https-url> --image-sha256 <sha256>`
- Create VMs from node-local uploaded image path:
  `kctl --node <host:9091> node upload-image -f <local-image.{qcow2,raw}> ...`
  then `kctl create vm <name> --storage-backend <filesystem|lvm|zfs> --storage-size-bytes <bytes> --image-path <node-path> --image-format <qcow2|raw>`
- Wait for VM readiness:
  `kctl create vm <name> ... --wait` or `kctl create vm <name> ... --wait-for-ssh`
- Manage VM desired state declaratively: `kctl set vm <name> --state <running|stopped>`
- Legacy compatibility aliases remain available: `kctl start vm ...`, `kctl stop vm ...`

Cloud Hypervisor console endpoints:
- API socket: `/run/kcore/<vm-name>.sock`
- Serial socket: `/run/kcore/<vm-name>.serial.sock`
- Example attach from node host: `socat -,raw,echo=0,icanon=0 UNIX-CONNECT:/run/kcore/<vm-name>.serial.sock`

Default guest access on generated cloud-init seeds:
- DHCP leases are served by `kcore-dhcp-<network>.service` and written to `/run/kcore/dnsmasq-<network>.leases`.
- Default VM user is `kcore` with password `kcore` (for first-boot/operator debugging).
- Example from node host:
  `ssh kcore@$(awk 'NR==1 {print $3}' /run/kcore/dnsmasq-default.leases)`

See:
- [kctl commands and workflows](docs/kctl-commands-and-workflows.md)
- [VM images workflow](docs/images.md)
- [Networking model](docs/networking.md)
- [Node install bootstrap flow](docs/node-install-bootstrap-flow.md)
- [mTLS bootstrap and authentication](docs/mtls-bootstrap-and-auth.md)
- [Nix VM config generation](docs/nix-vm-config-generation.md)
- [File structure](docs/file-structure.md)

## Developer Workflow

Use the Nix flake development environment for reproducible toolchains:

```bash
nix develop
```

Common checks:

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace
cargo fmt --check
cargo audit
```

## Known Gaps / Next Priorities

1. Add certificate lifecycle management (rotation, expiry handling, revocation strategy).
2. Implement robust state reconciliation for VM runtime sync paths.
3. Improve scheduler policy (capacity/affinity/load-aware placement).
4. Expand integration and failure-mode test coverage across controller/node interactions.
