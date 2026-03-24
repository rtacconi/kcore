# kcore-rust Analysis Report

`kcore-rust` is a Rust workspace with a clean early-stage control-plane design: one controller, one node agent, and one CLI. It now includes end-to-end mTLS plumbing for cluster traffic when TLS is configured.

## Documentation

- [mTLS bootstrap and authentication](docs/mtls-bootstrap-and-auth.md)
- [Nix VM config generation](docs/nix-vm-config-generation.md)
- [kctl commands and workflows](docs/kctl-commands-and-workflows.md)

## What It Is

- `kcore-controller`: central orchestration API (gRPC), SQLite-backed state, Nix config generation/push to nodes
- `kcore-node-agent`: node-side gRPC service, Cloud Hypervisor status discovery via Unix sockets, node admin ops
- `kctl`: operator CLI (create/get/start/stop VM, node admin, apply config, image pull/delete)

## Architecture Snapshot

- **Control flow**
  - `kctl` talks to controller (or directly to node for admin/image ops)
  - controller stores nodes/VMs in SQLite and pushes generated Nix (`ctrl-os.vms`) to target node
  - node-agent applies Nix and reports VM state from Cloud Hypervisor sockets
- **Protocols/stack**
  - gRPC via `tonic` + protobuf (`proto/controller.proto`, `proto/node.proto`)
  - async runtime: `tokio`
  - DB: `rusqlite` with WAL
  - config: YAML (`serde_yaml`)
  - transport security: mTLS support across `kctl`, `controller`, and `node-agent`

## Strengths

- Clear separation of responsibilities between control plane, node plane, and CLI
- Declarative VM management model (Nix generation) is consistent with immutable infra patterns
- Workspace compiles and tests pass (`cargo check --workspace`, `cargo test --workspace`)
- `kctl create cluster` generates cluster CA and cert/key pairs for secure bootstrap
- Node installation bootstraps CA and mTLS certificates into installed KcoreOS
- Runtime mTLS authentication is supported for client/server and controller/node links

## Key Risks / Gaps

- **Lifecycle semantics clarity (low/medium):** `start_vm`/`stop_vm` now work declaratively (desired state + config apply), but RPC names/flags (for example `force`) still look imperative and can confuse operators.
- **Security hardening incomplete (medium):** mTLS is implemented and materially lowers network attack risk, but cert rotation/revocation workflows and finer-grained authorization are not yet in place.
- **State sync is stubbed (medium):** `sync_vm_state` logs and returns success without persisting reconciliation.
- **Scheduler is minimal (medium):** first-ready-node selection only; no capacity/affinity/load awareness.
- **Testing depth still limited (medium):** meaningful mTLS/bootstrap tests were added, but broader integration and failure-mode coverage is still needed.

## Practical Priorities

1. Complete lifecycle API cleanup: rename/deprecate imperative-looking start/stop semantics and remove stale flags that do not apply in declarative mode.
2. Add certificate lifecycle operations (rotation, expiry alerts, revocation strategy).
3. Add fine-grained authorization on top of mTLS identity.
4. Implement real VM state reconciliation in `sync_vm_state`.
5. Improve scheduler to include resource checks and placement policy.
