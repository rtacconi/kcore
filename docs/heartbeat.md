# Node Heartbeat

Each node periodically sends a heartbeat RPC to the controller. This is the
mechanism that keeps the controller informed about node liveness and resource
usage.

## Protocol

The heartbeat is a unary gRPC call defined in `proto/controller.proto`:

```protobuf
rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);

message HeartbeatRequest {
  string node_id = 1;
  NodeUsage usage = 2;
}

message NodeUsage {
  int32 cpu_cores_used = 1;
  int64 memory_bytes_used = 2;
}
```

The node agent sends this at a regular interval (typically every 10–30
seconds). The controller processes it synchronously and returns a simple
`{ success: true }` response.

## What the controller does on each heartbeat

1. **Updates `last_heartbeat`** — set to `datetime('now')` (UTC) in the
   `nodes` table.
2. **Sets `status = 'ready'`** — a successful heartbeat proves the node is
   reachable, so the controller marks it ready.
3. **Persists resource usage** — `cpu_used` and `memory_used` are written to
   the `nodes` table. The scheduler reads these values when deciding where to
   place new VMs (see [docs/scheduler.md](scheduler.md)).

The SQL executed on each heartbeat:

```sql
UPDATE nodes
SET last_heartbeat = datetime('now'),
    status = 'ready',
    cpu_used = ?,
    memory_used = ?
WHERE id = ?
```

If the `node_id` doesn't match any registered node, the RPC returns
`NOT_FOUND` and the node should re-register.

## Authentication

The heartbeat RPC requires mTLS with a certificate whose Common Name starts
with `kcore-node-`. This ensures only legitimate node agents can report status.

## How usage data feeds the scheduler

The `NodeRow` in the database carries both capacity (from `RegisterNode`) and
current usage (from heartbeats):

```
capacity:  cpu_cores=8        memory_bytes=34359738368  (32 GiB)
usage:     cpu_used=3         memory_used=12884901888   (12 GiB)
free:      8 - 3 = 5 cores    32 - 12 = 20 GiB
```

When a `CreateVm` request arrives without a `target_node`, the scheduler loads
all nodes and picks the one with the most free capacity. See
[docs/scheduler.md](scheduler.md) for the full algorithm.

## SyncVmState — the companion RPC

Alongside heartbeats, nodes also send `SyncVmState` RPCs that report the
runtime state of each VM on that node (`running`, `stopped`, `paused`,
`error`). This updates the `runtime_state` column in the `vms` table so that
`kctl get vms` can show live state without polling every node.

```protobuf
rpc SyncVmState(SyncVmStateRequest) returns (SyncVmStateResponse);

message SyncVmStateRequest {
  string node_id = 1;
  repeated VmInfo vms = 2;
}
```

VMs reported by a node that the controller doesn't track are logged as orphans
but otherwise ignored.

## Exposing usage in the API

The `ListNodes` and `GetNode` RPCs return `NodeInfo` messages that include:

- **`capacity`** — `{ cpu_cores, memory_bytes }` from registration.
- **`usage`** — `{ cpu_cores_used, memory_bytes_used }` from the last
  heartbeat.
- **`last_heartbeat`** — timestamp of the most recent heartbeat.
- **`labels`** — placement labels set at registration time.

Example `kctl get nodes` output:

```
ID           HOSTNAME     ADDRESS           CORES    MEMORY  STATUS
node-1       node-1       10.0.0.11:9091        8     32 GiB  ready
node-2       node-2       10.0.0.12:9091       16     64 GiB  ready
```

`kctl get node node-1` shows the detailed view including usage:

```
ID:        node-1
Hostname:  node-1
Address:   10.0.0.11:9091
Status:    ready
CPU:       8 cores
Memory:    32 GiB
CPU used:  3 cores
Mem used:  12 GiB
```

## Staleness detection (future)

There is currently no automatic mechanism to mark a node as `not-ready` if
heartbeats stop arriving. This is a planned improvement — a background task in
the controller would periodically check `last_heartbeat` and downgrade nodes
that haven't reported within a configurable timeout (e.g. 90 seconds). Once
marked `not-ready`, the scheduler would skip them for new VM placement.
