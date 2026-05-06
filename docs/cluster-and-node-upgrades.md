# Cluster and node upgrades

This guide describes how operators roll **NixOS hosts** (controllers and workers)
forward using a pinned **flake** target and the controller **`ClusterUpdate`**
resource. Guest VMs and containers are not upgraded by this flow; they remain
under normal workload lifecycle operations.

For architecture, schema evolution, and implementation notes, see
[`cluster-updates.md`](./cluster-updates.md).

## Mental model

- **Release target** — You choose a kcore/NixOS revision using `flake_ref`
  (flake URI) and `flake_rev` (immutable Git revision). Optional fields include
  `nixpkgs_rev` and `system_profile`.
- **Who gets upgraded** — The **`selector`** picks nodes: explicit `node_ids`,
  `all_nodes`, `controllers_only`, plus optional `labels` and `datacenters`
  where supported.
- **Single-node upgrade** — Use the same manifest format with a selector that
  lists **one** node id (or a small set). There is no separate “node-only” CLI;
  scope is entirely manifest-driven.
- **Controller-led rollout** — `kctl` submits the manifest to the controller.
  The controller records the update, resolves targets, and reconcilers drive
  **prepare** then **activate** RPCs to each node’s agent. Watch **`phase`** on
  the cluster update and per-node rows.

## CLI workflow (`kctl update cluster`)

Assuming `kctl` points at the controller (TLS context in `~/.kcore/config`):

| Command | Purpose |
|--------|---------|
| `kctl update cluster plan -f rollout.yaml` | Dry-run: resolve selector, viability, likely reboot |
| `kctl update cluster apply -f rollout.yaml` | Create or update the `ClusterUpdate` resource |
| `kctl update cluster get <name>` | Show cluster phase, approval state, per-node phases/errors |
| `kctl update cluster list` | List updates |
| `kctl update cluster approve <name>` | Approve when `approval_policy` requires manual approval |
| `kctl update cluster cancel <name>` | Cancel rollout |
| `kctl update cluster rollback <name>` | Request rollback (best-effort; consult release notes) |

Alias: `kctl update os …` (same subcommands).

## YAML manifest (`kind: ClusterUpdate`)

Apply with **`kctl update cluster apply -f rollout.yaml`** (not `kctl apply`; that
path is for workload resources). Fields mirror
`crates/kctl/src/commands/cluster_update.rs` (camelCase in YAML):

```yaml
kind: ClusterUpdate
metadata:
  name: release-0-4-0
spec:
  target:
    version: "0.4.0"
    flake_ref: github:kcorehypervisor/kcore
    flake_rev: "<full-git-sha>"
    nixpkgs_rev: ""           # optional override
    system_profile: ""        # optional; host profile attribute path if needed
  selector:
    all_nodes: true           # or node_ids: ["node-uuid-…"]
    controllers_only: false
    node_ids: []
    labels: []
    datacenters: []
  strategy:
    strategy_type: one-at-a-time   # canary | batch | per-dc | …
    max_unavailable: 1
    batch_size: 1
  drain_vms: false
  drain_timeout_seconds: 0
  activation:
    mode: auto                   # test | switch | boot | auto
    reboot_policy: ""            # opaque string interpreted by node-agent
  approval_policy: manual        # manual | auto-non-disruptive | auto
  automatic_rollback: false
```

**Approval:** With `approval_policy: manual`, the controller creates the update in
a pending phase until `kctl update cluster approve <name>` runs. Automatic policies
skip that step.

**Rollout shape:** Strategy enums exist on the API; the reconciler may process
nodes serially in simpler deployments — verify behaviour for your release.

## Phases and observability

- Cluster-level **`phase`** progresses through states such as pending → ready →
  rolling_out → succeeded / failed / cancelled (see `controller.proto`).
- Each node has a **phase** (`pending`, `prepared`, `succeeded`, `failed`, …)
  and optional **`last_error`** for debugging.

Use `kctl update cluster get <name>` for detail.

## NixOS rollback on the host

Cluster rollback RPCs coordinate intent at the controller. Operators can still
use normal **NixOS generation** tools on a machine (`nixos-rebuild --rollback`,
boot loader previous generation) if an activation leaves the host in a bad state —
combine with your runbook and support channel.

## Related

- [`cluster-updates.md`](./cluster-updates.md) — design and migration topics.
- [`kctl-commands-and-workflows.md`](./kctl-commands-and-workflows.md) — broader CLI patterns.
