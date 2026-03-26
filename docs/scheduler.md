# VM Scheduler

The scheduler decides which node should host a new VM when the user doesn't
explicitly specify a `--target-node`.

Source: `crates/controller/src/scheduler.rs`

## How it works

When `CreateVm` is called without a `target_node`, the controller loads every
registered node from the database and passes the list to the scheduler.

The scheduler does two things:

1. **Filters** — only nodes with `status == "ready"` are considered. Nodes
   that have never heartbeated or are marked unhealthy are excluded.
2. **Ranks** — among the eligible nodes it picks the one with the **most free
   resources**, using `(free_memory, free_cpu)` as a composite sort key.
   Memory is the primary factor because it's usually the bottleneck for VMs.

### Capacity-aware placement

The scheduler computes free resources from values stored on each `NodeRow`:

```
free_memory = memory_bytes - memory_used
free_cpu    = cpu_cores    - cpu_used
```

These values come from two sources:

- **`memory_bytes` / `cpu_cores`** — total node capacity, reported once during
  `RegisterNode`.
- **`memory_used` / `cpu_used`** — current usage, updated on every heartbeat
  (see [docs/heartbeat.md](heartbeat.md)).

The `select_node_for_vm` function also enforces a **hard capacity check**: a
node is only eligible if it has enough free CPU *and* free memory to fit the
requested VM. If no node can fit, the RPC returns `UNAVAILABLE` with the
message *"no ready node with sufficient capacity"*.

## Functions

### `select_node(nodes) -> Option<&NodeRow>`

Returns the ready node with the most free resources. Used when no specific VM
size is known (general-purpose node selection).

### `select_node_for_vm(nodes, requested_cpu, requested_memory) -> Option<&NodeRow>`

Returns the ready node that can fit the requested VM and has the most remaining
capacity after placement. This is the function used by `CreateVm`.

## Algorithm: most-free-first (spread)

The current strategy is **spread scheduling** — new VMs land on the node with
the most headroom. This avoids hot spots and gives each VM the best chance of
having resources available for bursting.

An alternative would be **bin-packing** (fill nodes tightly before spilling to
the next), which is better for power savings on large clusters. The scheduler
can be swapped to that strategy by changing `max_by_key` to `min_by_key` on
the free-resource tuple.

## Edge cases

| Situation | Behaviour |
|-----------|-----------|
| No nodes registered | Returns `None` → `UNAVAILABLE` |
| All nodes `status != "ready"` | Returns `None` → `UNAVAILABLE` |
| No node has enough capacity | `select_node_for_vm` returns `None` → `UNAVAILABLE` |
| Single node with capacity | That node is selected |
| Tie (equal free resources) | Deterministic but arbitrary (depends on DB row order) |

## Future improvements

- **Label-based affinity** — filter nodes by labels before ranking (e.g.
  `--label dc=dc-a` to restrict placement to a specific datacenter).
- **Anti-affinity** — avoid placing two VMs with the same name prefix on the
  same node for HA.
- **Overcommit ratio** — allow scheduling beyond physical capacity with a
  configurable multiplier.
