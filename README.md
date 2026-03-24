# kcore-rust Analysis Report

`kcore-rust` is a Rust workspace with a clean early-stage control-plane design: one controller, one node agent, and one CLI.

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

## Strengths

- Clear separation of responsibilities between control plane, node plane, and CLI
- Declarative VM management model (Nix generation) is consistent with immutable infra patterns
- Workspace compiles and tests pass (`cargo check --workspace`, `cargo test --workspace`)
- CLI supports TLS client config and insecure mode for dev workflows

## Key Risks / Gaps

- **Lifecycle mismatch (high):** controller forwards `start_vm`/`stop_vm` to node compute RPC, but node-agent currently returns `unimplemented` for those mutating VM methods.
- **Security exposure (high):** `NodeAdmin` is explicitly unauthenticated in proto comments.
- **TLS inconsistency (high):** TLS fields exist in configs and CLI supports TLS, but controller->node client currently dials `http://...` and servers are started without TLS wiring.
- **State sync is stubbed (medium):** `sync_vm_state` logs and returns success without persisting reconciliation.
- **Scheduler is minimal (medium):** first-ready-node selection only; no capacity/affinity/load awareness.
- **Testing is sparse (medium):** only one unit test exists (Nix generator).

## Practical Priorities

1. Align VM lifecycle semantics: either implement node mutating RPCs or remove/deprecate start/stop RPC path in controller.
2. Add authn/authz and transport hardening for `NodeAdmin` and controller<->node links.
3. Implement real VM state reconciliation in `sync_vm_state`.
4. Improve scheduler to include resource checks and placement policy.
5. Expand tests around controller DB logic, RPC behavior, and failure paths.
