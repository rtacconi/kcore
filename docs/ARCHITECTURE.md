# KCORE Architecture

## Controller-First Architecture

All external clients (`kctl`, Terraform) communicate exclusively with the **controller**. Only the controller talks to node-agents. This is a hard rule -- kctl and Terraform never create direct connections to node-agents for VM operations.

```
                    ┌──────────┐   ┌───────────┐
                    │   kctl   │   │ Terraform │
                    └────┬─────┘   └─────┬─────┘
                         │               │
                         │  gRPC (mTLS)  │
                         └───────┬───────┘
                                 │
                    ┌────────────▼─────────────┐
                    │       Controller         │
                    │   (port 9090, default)    │
                    │                          │
                    │   ┌──────────────────┐   │
                    │   │  SQLite DB       │   │
                    │   │  desired state   │   │
                    │   │  actual state    │   │
                    │   └──────────────────┘   │
                    └──────┬──────────┬────────┘
                           │          │
                     gRPC mTLS   gRPC mTLS
                           │          │
                    ┌──────▼──┐  ┌────▼──────┐
                    │ Node    │  │ Node      │
                    │ Agent 1 │  │ Agent 2   │
                    │ :9091   │  │ :9091     │
                    │         │  │           │
                    │ SQLite  │  │ SQLite    │
                    │ libvirt │  │ libvirt   │
                    │ ┌──┐┌──┐│  │ ┌──┐┌──┐ │
                    │ │VM││VM││  │ │VM││VM│ │
                    │ └──┘└──┘│  │ └──┘└──┘ │
                    └─────────┘  └──────────┘
```

### Key Principles

1. **Controller-first.** kctl and Terraform talk only to the controller (port 9090). The controller forwards operations to the appropriate node-agent (port 9091).
2. **One controller commands multiple node-agents.** A node-agent registers with exactly one controller. The controller owns all scheduling decisions.
3. **SQLite persistence.** Both the controller and node-agent persist state in SQLite using versioned migrations (see [DATABASE_MIGRATIONS.md](DATABASE_MIGRATIONS.md)).
4. **Desired state vs actual state.** The controller separates what the user wants (desired state) from what actually exists (actual state). A reconciliation loop drives actual state toward desired state.

### Multi-Controller HA (future)

- Multiple controllers can be deployed for high availability.
- Each controller is a full peer -- any can accept requests.
- Controllers can reside in different datacenters.
- State replication uses CRDTs (Conflict-free Replicated Data Types) for eventual consistency without distributed locking.

## Desired State / Actual State

When a user submits a YAML manifest (`kctl apply`) or a Terraform resource, the controller stores it as **desired state** in SQLite. The controller then works to make reality match the intent.

**Desired state** (set by user actions only):
- The full resource spec as declared by the user (JSON in `vms.desired_spec`)
- The target lifecycle state (`vms.desired_state`: running, stopped, deleted)
- Never modified by the controller autonomously

**Actual state** (reported by node-agents):
- VM power state (running/stopped/error)
- Actual node placement
- Updated via node-agent heartbeats and `SyncVmState`

**Reconciliation loop** (`pkg/controller/reconciler.go`):
- Runs on a periodic tick
- Compares desired to actual for every VM
- Takes action to converge: create, start, stop, delete, migrate

## Component Overview

| Component | Port | Database | Purpose |
|-----------|------|----------|---------|
| Controller | 9090 | SQLite (desired + actual state) | Accepts client requests, schedules VMs, reconciles state |
| Node-agent | 9091 | SQLite (VM metadata, cached images) | Manages libvirt domains on a single host |
| kctl | - | - | CLI client, talks to controller only |
| Terraform | - | - | Infrastructure-as-code provider, talks to controller only |

## gRPC Services

### Controller (proto/controller.proto)
- `RegisterNode` -- node-agent registers itself
- `Heartbeat` -- periodic health check from node-agent
- `SyncVmState` -- node-agent reports its current VMs
- `CreateVm`, `DeleteVm`, `StartVm`, `StopVm`, `GetVm`, `ListVms` -- VM lifecycle
- `ListNodes`, `GetNode` -- node inventory

### Node-agent (proto/node.proto)
- `NodeCompute` -- VM lifecycle (called only by the controller)
- `NodeStorage` -- volume operations
- `NodeInfo` -- capacity reporting
- `NodeAdmin` -- NixOS config management

## Data Flow

```
User                Controller              Node-Agent          libvirt
 │                      │                       │                  │
 │  kctl apply -f vm.yaml                       │                  │
 ├──────────────────────►│                       │                  │
 │                      │  Store desired state   │                  │
 │                      │  in SQLite             │                  │
 │                      │                       │                  │
 │                      │  CreateVm (gRPC)       │                  │
 │                      ├───────────────────────►│                  │
 │                      │                       │  Define domain   │
 │                      │                       ├─────────────────►│
 │                      │                       │  Start domain    │
 │                      │                       ├─────────────────►│
 │                      │       response        │                  │
 │                      │◄───────────────────────│                  │
 │                      │  Update actual state   │                  │
 │       response       │  in SQLite             │                  │
 │◄──────────────────────│                       │                  │
```

## See Also

- [KCTL.md](KCTL.md) -- CLI reference
- [DATABASE_MIGRATIONS.md](DATABASE_MIGRATIONS.md) -- SQLite migration pattern
- [TERRAFORM_PROVIDER.md](TERRAFORM_PROVIDER.md) -- Terraform provider
- [COMMANDS.md](COMMANDS.md) -- Make targets
