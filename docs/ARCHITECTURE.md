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

## Bootstrap Controller

When a bare-metal node boots from the kcore ISO, there is no TLS infrastructure yet. A **bootstrap controller** handles initial enrollment:

1. The ISO boots with a node-agent that listens without TLS (ephemeral mode).
2. The operator runs `kctl init cluster` on their machine to generate a CA and cluster identity.
3. `kctl get disks` / `kctl get nics` queries the node-agent directly (no TLS) to discover hardware.
4. `kctl install node` (or `kctl apply -f NodeInstall`) triggers OS installation on the target disks.
5. After reboot, the node-agent starts with TLS certificates signed by the cluster CA and registers with the controller.

The bootstrap controller is **ephemeral** -- it exists only during initial provisioning and is not part of the steady-state control plane.

## Automator API (replaces NodeAdmin)

The `NodeAdmin` gRPC service is superseded by the **Automator API**, which covers the full node lifecycle:

| Capability | Old (NodeAdmin) | New (Automator API) |
|---|---|---|
| Hardware discovery | -- | `GetDisks`, `GetNics` |
| OS installation | -- | `InstallNode` (multi-disk, role-based) |
| Network configuration | -- | `ConfigureNetwork` (bridges, bonds, VLANs) |
| NixOS config push | `PushConfig` | `UpdateNixConfig` |
| System update | -- | `UpdateSystem` (channels, rebuild, agent) |
| Controller role | -- | `run_controller` flag on install |

The Automator API is exposed on the same node-agent gRPC port (9091) and uses the same mTLS transport once the node is enrolled.

## CA Management

The cluster CA lives on the **operator's machine**, not on any cluster node:

```
~/.kcore/<cluster>/
├── ca.crt            # Cluster CA certificate
├── ca.key            # CA private key (never leaves operator machine)
├── controller.crt    # Controller certificate
├── controller.key
└── nodes/
    ├── <node-id>.crt
    └── <node-id>.key
```

`kctl init cluster` generates the CA. Node and controller certificates are issued automatically during `kctl install node`. The CA key never needs to be on a cluster node.

## Multi-Disk Install

`InstallNode` supports multiple disks with explicit roles:

| Role | Purpose |
|---|---|
| `os` | Root filesystem (NixOS). Exactly one required. |
| `storage` | LVM thin-pool for VM volumes. Zero or more. |

Example (two disks):
```yaml
disks:
  - device: /dev/sda
    role: os
  - device: /dev/nvme0n1
    role: storage
```

The installer partitions and formats each disk according to its role. Storage disks are initialised as LVM PVs and added to the `kcore-storage` volume group.

## Network Configuration

Post-install network can be configured via `ConfigureNetwork` (or `kctl configure network` / `NodeNetwork` manifest):

- **Bridges** -- attach physical NICs to a Linux bridge for VM connectivity.
- **Bonds** -- aggregate multiple NICs (LACP, active-backup, etc.).
- **VLANs** -- tagged sub-interfaces on physical or bonded NICs.
- **DNS** -- custom DNS server list.

Configuration is rendered into a NixOS networking module and optionally applied immediately (`applyNow` / `--apply`).

## Controller Lifecycle

A node can optionally run the kcore controller by setting `runController: true` during installation (or `--run-controller` CLI flag). When enabled:

- The controller binary is deployed alongside the node-agent.
- A systemd unit `kcore-controller` is enabled and started.
- `controllerAddress` sets the advertised gRPC listen address for the controller.
- Other nodes register against this address.

Only one node in the cluster should run the controller (multi-controller HA is a future goal).

## Auto-Registration Flow

After a node is installed and reboots:

1. Node-agent starts with TLS certs signed by the cluster CA.
2. Node-agent calls `RegisterNode` on the controller address baked into its config.
3. Controller verifies the client certificate against the cluster CA.
4. Controller adds the node to its inventory and begins heartbeat monitoring.
5. The node is now ready to receive VM workloads.

## Day-N Operations

Once a node is enrolled, ongoing management uses the Automator API:

- **NixOS config push** -- `kctl update nixconfig --node <id> --file <path>` sends a new `configuration.nix` to the node and optionally triggers `nixos-rebuild switch`.
- **System update** -- `kctl update system --node <id>` updates NixOS channels, rebuilds the system, and/or upgrades the node-agent binary.
- **Network changes** -- `kctl configure network` modifies bridge/bond/VLAN settings without reinstalling.

All Day-N operations go through the controller, which forwards them to the target node-agent.

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
- `Automator` -- hardware discovery, OS install, network config, NixOS management (replaces NodeAdmin)

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
