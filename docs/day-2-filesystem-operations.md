# Day-2 Filesystem Operations (Technical)

This document is a technical runbook for day-2 filesystem and disk-layout changes on installed kcore nodes.

It complements:

- `docs/storage.md` (backend model and command surface)
- `docs/node-install-bootstrap-flow.md` (install-time flow)
- `docs/kctl-commands-and-workflows.md` (operator command reference)

## Scope

This document covers **post-install** operations for storage backends used by kcore:

- `filesystem` (ext4 mounts generated via disko)
- `lvm` (PV/VG managed by disko; LVs created by node-agent)
- `zfs` (pool created by disko; zvols/datasets used by node-agent)

It focuses on:

- ownership and safety controls
- safe/dangerous classifier and refusal semantics
- idempotent execution model
- operational sequencing
- validation and rollback strategies

> Terminology: the operator surface uses the plain word **"disk"** (`DiskLayout`, `kctl node apply-disk`). The underlying partitioning tool is still [disko](https://github.com/nix-community/disko) and is referenced as such in logs, command invocations, and the `kcore.disko.*` NixOS module.

## Ownership model and safety gates

kcore enforces explicit ownership for day-2 disk changes via:

- `/etc/kcore/disk-management-mode` (new canonical path)
- `/etc/kcore/disko-management-mode` (legacy path — still read as a fallback for one release; `node install` writes the new path and a compatibility symlink)

Valid values:

- `installer-only` (default after install)
- `controller-managed`

### Rules

1. Install-time always leaves nodes in `installer-only`.
2. `kctl node apply-disk --apply` is rejected in `installer-only`.
3. Validation-only (`kctl node apply-disk -f ...` without `--apply`) remains available for syntax/eval checks.
4. Promotion to `controller-managed` is a deliberate operator action.
5. Even in `controller-managed` mode, the node-agent runs a **safe/dangerous classifier** against every apply and refuses requests that would touch a block device currently in use (see below).

This prevents accidental partitioning operations in normal day-2 workflows.

## Safe / dangerous classifier

Before running `disko --mode format,mount`, the node-agent snapshots `lsblk -J -p -o NAME,PATH,FSTYPE,MOUNTPOINTS,PKNAME,TYPE` and checks the proposed layout against live state. The request is **refused** when a target device or any descendant is one of:

- an active kcore volume or image mount (prefixes `/var/lib/kcore/volumes`, `/var/lib/kcore/images`) → `target_device_has_active_kcore_volume`
- an active system mount (`/`, `/boot`, `/boot/efi`, `/nix`, `/nix/store`) → `target_device_has_active_system_mount`
- an active LVM physical volume (`fstype = LVM2_member`) → `target_device_is_active_lvm_pv`
- an active ZFS pool member (`fstype = zfs_member`) → `target_device_is_active_zpool_member`

When a classifier refuses, `ApplyDiskLayoutResponse.refusal_reason` contains the stable code above and `message` contains the human-readable detail. If lsblk itself cannot be inspected, the agent fails closed with `refusal_reason = "lsblk_probe_failed"`.

The controller is never expected to drain, stop, migrate, or reboot VMs. The operator quiesces workloads (manually today; via live migration once that lands) before submitting a `DiskLayout`.

## Recommended workflow: declarative `DiskLayout` resource

Day-2 disk changes are best driven through the controller as a `kind: DiskLayout` resource instead of direct node pushes. The controller persists the manifest in its replicated DB, classifier-pre-flights it, and the controller-side reconciler dispatches `ApplyDiskLayout` to the owning node and writes the result back into `status`.

```yaml
kind: DiskLayout
metadata:
  name: prod-data-pool
spec:
  nodeId: node-prod-01
  layoutNixFile: ./day2-disk.nix   # or inline `layoutNix: |`
```

```bash
kctl diff   -f day2-disk-layout.yaml         # controller pre-flight (no writes)
kctl apply  -f day2-disk-layout.yaml         # creates / updates the DiskLayout
kctl get    disk-layouts                     # list with phase + refusal_reason
kctl describe disk-layout prod-data-pool     # full body + status
kctl delete disk-layout prod-data-pool       # removes from controller DB; node is untouched
```

`kctl diff` calls the controller's read-only `ClassifyDiskLayout` RPC, which extracts the target devices from the Nix body and runs the controller-side pre-flight (structural checks today; live inventory once the replicated block-device table lands). The node-agent classifier is still the authoritative gate on every apply.

The reconciler retries refused layouts on every tick using the same generation, so the operator drains affected VMs and re-checks `kctl describe disk-layout <name>` until `phase = applied`. Editing `spec.layoutNix` (or its referenced file) bumps the generation; resubmitting the identical content does not.

`kctl node apply-disk -f …` remains available for one-off operator pushes and for validation flows where there is no controller (or for nodes still in `installer-only` mode that haven't been registered as DiskLayout targets).

## RPC and CLI execution path

Day-2 disk operations flow through:

1. `kctl` sends `ApplyDiskLayoutRequest` to `NodeAdmin.ApplyDiskLayout`.
2. Node-agent validates the Nix text contains `disko.devices`.
3. Node-agent runs the safe/dangerous classifier against live `lsblk`.
4. Node-agent stages the expression under `/etc/kcore/disk/` and executes a bounded command via `timeout`:
   - validate path: `nix-instantiate --parse <file>`
   - apply path: `disko --mode format,mount <file>`
5. On successful apply, node-agent atomically promotes the staged file to `/etc/kcore/disk/current.nix` so NixOS (via `modules/kcore-disko.nix`) and the controller reconciler observe the realised shape.
6. When `rebuild = true` (the default from `kctl`), node-agent chains `nixos-rebuild test` then `nixos-rebuild switch` via a transient systemd unit (`kcore-nix-rebuild.service`), matching the `ApplyNixConfig` flow.
7. Node-agent returns:
   - `success`
   - `message`
   - `mode` (current ownership mode)
   - `refusal_reason` (empty on success)

Timeout behavior:

- `timeout_seconds <= 0` defaults to `300`
- timeout is capped server-side (`3600`)

> Back-compat: the `NodeAdmin.ApplyDiskoLayout` RPC and the `kctl node apply-disko` alias still work. They are thin shims that forward to the new RPC/command and will be removed one release later. New manifests and docs should use the `apply-disk` names.

## Filesystem backend invariants

For the `filesystem` backend, current generated layouts use ext4 partitions mounted under:

- `/var/lib/kcore/volumes`
- `/var/lib/kcore/volumesN` for additional disks

### Invariants

- existing mount points must remain stable unless migration is planned
- new day-2 data disks should introduce new mount points, not replace active mounts in-place
- VM image cache path (`/var/lib/kcore/images`) must remain accessible
- node-agent process must retain write access to volume roots

## LVM backend invariants

For `lvm`, disko creates PVs and VG; node-agent creates/deletes LVs dynamically.

### Invariants

- VG name in runtime config must match on-disk VG
- PV additions must not remove or rename existing PVs backing active LVs
- thin-pool/data LV topology changes require dedicated maintenance planning

### Practical checks

- `vgs`, `pvs`, `lvs -a -o +devices`
- verify `node-agent.yaml` LVM names before and after apply

## ZFS backend invariants

For `zfs`, disko creates pool; node-agent consumes pool for zvol/dataset operations.

### Invariants

- pool name must remain stable (`tank0` or configured value)
- do not import pool under alternate name
- avoid day-2 operations that modify ashift/feature compatibility without maintenance window

### Practical checks

- `zpool status`
- `zpool list`
- verify pool name in runtime config remains unchanged

## Safe day-2 workflow (recommended)

### 0) Preflight

- capture current state:
  - `lsblk -f`
  - `findmnt -R /var/lib/kcore`
  - backend-specific health (`vgs/pvs/lvs` or `zpool status`)
- ensure target disks are correct devices
- verify maintenance window and backup posture
- stop / migrate any workload whose backing storage lives on the target devices (the classifier will refuse otherwise)

### 1) Confirm ownership mode

```bash
cat /etc/kcore/disk-management-mode
```

If still `installer-only`, promote intentionally:

```bash
echo controller-managed | sudo tee /etc/kcore/disk-management-mode
```

### 2) Validate layout only

```bash
kcore-kctl --node <node:9091> node apply-disk -f ./day2-layout.nix
```

Do not proceed until validation returns success.

### 3) Apply with bounded timeout

```bash
kcore-kctl --node <node:9091> node apply-disk \
  -f ./day2-layout.nix \
  --apply \
  --timeout-seconds 600
```

The node-agent:

- runs the classifier and refuses if any target device is in use;
- persists the applied Nix to `/etc/kcore/disk/current.nix`;
- chains `nixos-rebuild test` → `switch` so mount units and services stay consistent.

Pass `--no-rebuild` only when you are explicitly orchestrating the rebuild yourself (e.g. as part of a controller-driven reconciliation).

### 4) Post-apply verification

- `findmnt -R /var/lib/kcore`
- verify backend health
- verify node-agent active and volume operations still succeed
- `systemctl status kcore-nix-rebuild.service` to confirm the rebuild landed

## Idempotence and repeatability expectations

Day-2 operations should be authored so re-running does not introduce drift:

- avoid random/non-deterministic naming in disko definitions
- keep stable identifiers (VG/pool names, mountpoint paths)
- prefer additive changes over in-place destructive replacement

If a change is not naturally idempotent, treat it as a one-time migration step and document a guard condition.

## Failure handling

### Validation failure

- fix layout file
- rerun validation
- no disk mutation expected

### Classifier refusal

- `refusal_reason` explains which invariant was violated
- stop / migrate the listed workload and resubmit; never disable the classifier

### Apply failure

- inspect returned stderr in RPC response
- the staged Nix file is **not** promoted to `/etc/kcore/disk/current.nix` when disko fails, so NixOS evaluation is unaffected
- verify partial state with `lsblk/findmnt` and backend tools
- do not retry blindly; first confirm whether partial partitioning occurred

### Timeout

- command was bounded; inspect actual system state before re-attempt
- increase timeout only when operation is known to be long-running and safe

### Rebuild failure

- `nixos-rebuild test+switch` runs as `kcore-nix-rebuild.service`; use `journalctl -u kcore-nix-rebuild.service` for logs
- re-applying the same `DiskLayout` is safe (disko is idempotent on matching layout) and re-triggers the rebuild chain

## Concurrency and maintenance considerations

- avoid concurrent day-2 apply operations on the same node
- avoid VM placement churn during partitioning operations
- for topology-changing operations (VG/pool structural edits), prefer temporary workload quiesce

## Example: additive filesystem disk

Minimal additive pattern for `filesystem` backend:

1. Add new disk entry producing `/var/lib/kcore/volumes1`.
2. Validate only.
3. Apply — classifier sees the new disk is idle, rebuild chain activates the mount.

This keeps existing `volumes` mount untouched and reduces blast radius.

## What is intentionally not covered

- repartitioning OS/root disk in-place on production nodes
- automatic migration of existing volumes between backends
- backend conversion (`filesystem -> lvm -> zfs`) without explicit migration tooling
- `--force` overrides of classifier refusals (there are none by design; stop the workload instead)

Those are maintenance/reinstall-class workflows and should be handled with dedicated migration procedures.
