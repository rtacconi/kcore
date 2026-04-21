# Storage Backends (FS/LVM/ZFS)

This document explains how storage works end-to-end in kcore:

- operator commands (`kcore-kctl`)
- controller API and validation
- node install-time storage configuration
- generated Nix VM configuration fields
- source files involved in the storage flow (with detailed responsibilities)

For a deep operational runbook focused specifically on day-2 filesystem changes, see:

- `docs/day-2-filesystem-operations.md`

## Storage Model

There are two connected storage concepts:

1. **Node storage capability**  
   Each node registers a storage backend capability (`filesystem`, `lvm`, `zfs`) with the controller.

2. **VM storage request**  
   Every VM create request now carries required storage metadata:
   - `storage_backend`
   - `storage_size_bytes`

Controller checks compatibility and rejects VM create if requested backend does not match the selected node capability.

## Command Reference

Use these commands as the canonical storage workflow.

### 1) Install node with storage backend

#### Single-disk install

When a server has only one disk (or you want OS and VMs to share the same
disk), omit `--data-disk`. The filesystem backend stores images and volumes
under `/var/lib/kcore/` on the OS partition:

```bash
kcore-kctl --node 192.168.40.105:9091 node install \
  --os-disk /dev/sda \
  --join-controller 192.168.40.105:9090 \
  --storage-backend filesystem
```

#### Multi-disk install

When dedicated data disk(s) are available, pass them with `--data-disk`:

Filesystem:

```bash
kcore-kctl --node 192.168.40.105:9091 node install \
  --os-disk /dev/sda \
  --data-disk /dev/nvme0n1 \
  --join-controller 192.168.40.105:9090 \
  --storage-backend filesystem
```

LVM:

```bash
kcore-kctl --node 192.168.40.105:9091 node install \
  --os-disk /dev/sda \
  --data-disk /dev/nvme0n1 \
  --join-controller 192.168.40.105:9090 \
  --storage-backend lvm \
  --lvm-vg-name vg_kcore \
  --lvm-lv-prefix kcore-
```

ZFS:

```bash
kcore-kctl --node 192.168.40.105:9091 node install \
  --os-disk /dev/sda \
  --data-disk /dev/nvme0n1 \
  --join-controller 192.168.40.105:9090 \
  --storage-backend zfs \
  --zfs-pool-name tank0 \
  --zfs-dataset-prefix kcore-
```

Notes:

- `--storage-backend` is the typed mode and preferred over legacy `--data-disk-mode`.
- Exactly one of `--join-controller` or `--run-controller` is required.

#### Data disks during `install-to-disk`

The installer uses [disko](https://github.com/nix-community/disko) to declaratively partition, format, and mount both the OS disk and any data disks at install time. The generated `disko-config.nix` includes data disk entries matching the requested `--storage-backend`:

- **filesystem**: GPT + ext4 partition mounted at `/var/lib/kcore/volumes`
- **lvm**: GPT + LVM PV added to a VG named per `--lvm-vg-name` (default `vg_kcore`)
- **zfs**: GPT + ZFS partition in a zpool named per `--zfs-pool-name` (default `tank0`)

VGs and zpools are **created at install time** by disko. The installer also writes the matching storage backend and parameters into `/etc/kcore/node-agent.yaml`. LVs / zvols are created later by `node-agent` on demand once the backing VG or pool exists.

Device paths are recorded in `/etc/kcore/data-disks` for reference. The authoritative disk layout is the `disko-config.nix` saved at `/etc/nixos/disko-config.nix`.

### Declarative LVM and ZFS on NixOS (day-2)

For day-2 additions of new data disks, the `disko` CLI is available on installed nodes. Typical patterns:

- **[disko](https://github.com/nix-community/disko)** — declarative partition / LUKS / LVM / ZFS / btrfs layout; applies layout from Nix (often run once at install or via a dedicated activation).
- **`fileSystems` + `swapDevices`** — mount ext4/xfs partitions or ZFS datasets **after** the underlying block device or pool already exists (`fsType = "zfs"` expects an **importable** pool).
- **ZFS** — enable `boot.supportedFilesystems = [ "zfs" ];` (and usually `boot.initrd.supportedFilesystems = [ "zfs" ];` when pools must be available in initrd). Pool and dataset **creation** is usually done by **disko**, **manual** `zpool create` / `zfs create`, or a **one-shot** `systemd` service guarded by a state file.
- **LVM** — VG/LV creation is usually **disko**, **imperative** `pvcreate`/`vgcreate`/`lvcreate` once, or a **one-shot** unit; then use `fileSystems` for mount points if needed.

For kcore, the backing VG or pool is created by disko at install time. VM **volumes** are then created at runtime by the node agent (`lvcreate`, `zfs create -V`, etc.) against the VG or pool named in `node-agent.yaml`.

## Disk management modes (safe installer/controller split)

> Naming note. The user-facing surface is **disk** (`DiskLayout`,
> `kctl ... disk-layout`, `/etc/kcore/disk-management-mode`). Disko is still
> the underlying tool that performs the partitioning; we just don't lead with
> the brand name on operator-facing surfaces.

## Disko Ownership Modes (safe installer/controller split)

kcore uses an explicit ownership split for disk partitioning state:

- `installer-only` (default): install-time layout is authoritative; controller / `kctl` day-2 disk apply is blocked.
- `controller-managed`: controller / `kctl` day-2 disk apply is allowed.

Install-time behavior writes the default mode marker:

- `/etc/kcore/disk-management-mode` contains `installer-only` after `node install` (the legacy `/etc/kcore/disko-management-mode` path is still read as a fallback for one release; a symlink is created for backwards compatibility).

This prevents accidental day-2 destructive layout changes on newly installed nodes until operators explicitly promote ownership.

### Promote a node to controller-managed mode

Operators can promote a node explicitly once runbooks and maintenance windows are in place:

```bash
echo controller-managed | sudo tee /etc/kcore/disk-management-mode
```

### Apply mode safety gate

`kcore-kctl node apply-disk --apply` (alias `apply-disko` kept for one release)
succeeds only in `controller-managed` mode. In `installer-only`, the RPC
returns a clear rejection and reports the active mode.

## Day-2 disk workflows

There are two equivalent ways to drive a day-2 disk change.

### A) Declarative `kctl apply -f` (recommended)

Submit a `DiskLayout` manifest to the controller; the controller persists it,
replicates it, and the controller-side reconciler pushes it to the target
node:

```yaml
kind: DiskLayout
metadata:
  name: prod-data-pool
spec:
  nodeId: node-prod-01
  layoutNixFile: ./day2-disk.nix   # or inline `layoutNix: |`
```

```bash
kcore-kctl diff   -f day2-disk-layout.yaml   # controller pre-flight, no writes
kcore-kctl apply  -f day2-disk-layout.yaml   # creates/updates DiskLayout
kcore-kctl get disk-layouts                  # see status (pending / applied / refused)
kcore-kctl describe disk-layout prod-data-pool
```

The node-agent classifier (live `lsblk` based) is the authoritative gate.
If the node-agent refuses (e.g. the targeted disk currently backs an active
kcore volume), the refusal code lands on the layout's `status.refusalReason`
and the operator can drain the affected VMs and resubmit the same manifest —
the generation does not bump on unchanged content, so the reconciler will
retry until the node accepts.

### B) Direct node-agent push (`kctl node apply-disk`)

For one-off operator workflows (validation, manual apply), you can still push
straight to a single node-agent:

```bash
kcore-kctl --node 192.168.40.105:9091 node apply-disk \
  -f ./day2-disk.nix
```

Validation parses/evaluates layout input without partitioning disks.

### 3) Apply with bounded timeout (controller-managed mode only)

```bash
kcore-kctl --node 192.168.40.105:9091 node apply-disk \
  -f ./day2-disk.nix \
  --apply \
  --timeout-seconds 600
```

Runtime is bounded server-side (`timeout`), and command output includes success/failure detail plus active ownership mode.

### 4) Reconcile mounts/services

The node-agent persists the applied layout to `/etc/kcore/disk/current.nix`
and chains `nixos-rebuild test` followed by `nixos-rebuild switch`
automatically. There is no longer a separate manual `apply-nix` step —
operators pass `--no-rebuild` only for validation flows or local tests.

## Controller-side disko fragments (multi-node consistency)

`modules/kcore-disko.nix` now supports optional controller fragments to keep day-2 layouts consistent across nodes while still allowing deterministic overrides.

New options:

- `kcore.disko.managementMode = "installer-only" | "controller-managed"`
- `kcore.disko.controllerFragments = [ { name; priority; devices; } ... ]`

Merge behavior:

- fragments are sorted by `(priority, name)`
- merged into base layout using recursive update
- fragments are rejected in `installer-only` mode via Nix assertion

This gives deterministic composition for fleet-wide storage profiles with explicit opt-in safety.

### 2) Create VM with required storage metadata

Create a VM using a node-local image with filesystem backend request:

```bash
kcore-kctl create vm app-fs-01 \
  --image-path /var/lib/kcore/images/ubuntu-24.04.raw \
  --image-format raw \
  --network default \
  --cpu 2 \
  --memory 4G \
  --storage-backend filesystem \
  --storage-size-bytes 42949672960
```

Create a VM with ZFS backend request:

```bash
kcore-kctl create vm app-zfs-01 \
  --image-path /var/lib/kcore/images/ubuntu-24.04.raw \
  --image-format raw \
  --network default \
  --cpu 2 \
  --memory 4G \
  --storage-backend zfs \
  --storage-size-bytes 42949672960
```

If node/backend mismatch occurs, create fails with `FailedPrecondition`.

### 3) Verify node storage capability

```bash
kcore-kctl get nodes
kcore-kctl get nodes <node-id>
```

`kctl` output includes node storage backend information so operators can confirm placement expectations.

## Nix Configuration Shape

The controller-generated node Nix now carries VM storage metadata fields.

Example generated VM block:

```nix
{ pkgs, ... }: {
  ch-vm.vms = {
    enable = true;
    cloudHypervisorPackage = pkgs.cloud-hypervisor;
    gatewayInterface = "eno1";

    virtualMachines."app-zfs-01" = {
      image = "/var/lib/kcore/images/ubuntu-24.04.raw";
      imageFormat = "raw";
      storageBackend = "zfs";
      storageSizeBytes = 42949672960;
      imageSize = 8192;
      cores = 2;
      memorySize = 4096;
      network = "default";
      autoStart = true;
    };
  };
}
```

`modules/ch-vm/options.nix` defines these fields under `ch-vm.vms.virtualMachines.<name>`:

- `storageBackend = "filesystem" | "lvm" | "zfs"`
- `storageSizeBytes = <positive integer>`

## Node-Agent YAML Storage Config

The node-agent runtime storage adapter is configured in `/etc/kcore/node-agent.yaml`.

Filesystem example:

```yaml
storage:
  backend: filesystem
  imageCacheDir: /var/lib/kcore/images
  filesystemVolumeDir: /var/lib/kcore/volumes
```

LVM example:

```yaml
storage:
  backend: lvm
  imageCacheDir: /var/lib/kcore/images
  lvm:
    vgName: vg_kcore
    lvPrefix: kcore-
```

ZFS example:

```yaml
storage:
  backend: zfs
  imageCacheDir: /var/lib/kcore/images
  zfs:
    poolName: tank0
    datasetPrefix: kcore-
```

## Detailed Storage File Map

### API contracts

- `proto/controller.proto`  
  Defines controller-side storage fields:
  - `RegisterNodeRequest.storage_backend`
  - `NodeInfo.storage_backend`
  - `CreateVmRequest.storage_backend`
  - `CreateVmRequest.storage_size_bytes`  
  This is the contract that `kctl` and controller both compile against.

- `proto/node.proto`  
  Defines typed install-time storage fields:
  - `InstallToDiskRequest.storage_backend`
  - `InstallToDiskRequest.lvm_vg_name`
  - `InstallToDiskRequest.lvm_lv_prefix`
  - `InstallToDiskRequest.zfs_pool_name`
  - `InstallToDiskRequest.zfs_dataset_prefix`  
  Also keeps legacy `data_disk_mode` for compatibility.

### Controller implementation

- `crates/controller/src/grpc/validation.rs`  
  Normalizes and validates storage backend enum values and validates positive storage sizes.

- `crates/controller/src/db.rs`  
  Persists storage metadata:
  - nodes: `storage_backend`
  - vms: `storage_backend`, `storage_size_bytes`  
  Includes schema migration and round-trip tests.

- `crates/controller/src/grpc/controller.rs`  
  Implements storage behavior in live RPCs:
  - reads/stores node backend on register
  - returns node backend in get/list node
  - requires VM storage backend+size on create
  - enforces backend compatibility before placement/apply
  - records VM storage metadata

- `crates/controller/src/nixgen.rs`  
  Injects storage fields into generated Nix VM definitions:
  - `storageBackend`
  - `storageSizeBytes`

- `crates/controller/src/scheduler.rs`  
  Capacity-fit helper used by create flow after backend filtering.

### Node-agent implementation

- `crates/node-agent/src/config.rs`  
  Runtime storage config schema and backend-specific requirements.

- `crates/node-agent/src/grpc/admin.rs`  
  Builds `install-to-disk` command arguments from typed request fields; applies compatibility fallback from legacy mode when needed.  
  Also enforces day-2 disko safety and execution via:
  - `ApplyDiskoLayout` RPC
  - ownership mode gate (`installer-only` vs `controller-managed`)
  - bounded command timeout

- `crates/node-agent/src/storage/mod.rs`  
  Storage adapter trait and concrete backends:
  - filesystem adapter
  - LVM adapter
  - ZFS adapter  
  Handles volume create/delete and image ensure/upload logic.

- `crates/node-agent/src/main.rs`  
  Initializes the adapter from config and injects it into admin/storage services.

- `crates/node-agent/src/grpc/storage.rs`  
  Exposes NodeStorage RPCs backed by the configured adapter.

### kctl implementation

- `crates/kctl/src/main.rs`  
  CLI flag surface:
  - `create vm --storage-backend --storage-size-bytes`
  - `node install --storage-backend` and backend-specific LVM/ZFS flags
  - `node apply-disko -f <file> [--apply] [--timeout-seconds N]`

- `crates/kctl/src/commands/vm.rs`  
  Validates and maps storage CLI arguments into `CreateVmRequest`.

- `crates/kctl/src/commands/node.rs`  
  Maps typed install storage flags into `InstallToDiskRequest`.  
  Implements day-2 disko RPC client call via `apply_disko_layout`.

- `crates/kctl/src/output.rs`  
  Renders node storage backend in operator output.

### Nix module surface

- `modules/kcore-disko.nix`  
  NixOS module that generates `disko.devices` from `kcore.disko.*` options (OS disk with LUKS, data disks with LVM/ZFS/filesystem). Used at install time via the generated `disko-config.nix` and available on installed nodes for day-2 operations.  
  Includes management mode and optional controller fragments for deterministic multi-node day-2 layout composition.

- `modules/ch-vm/options.nix`  
  Declares VM storage options accepted by generated config (`storageBackend`, `storageSizeBytes`).

- `modules/ch-vm/vm-service.nix`  
  VM runtime service generation. Storage metadata is currently represented as VM config fields and can be consumed by future service-level provisioning logic.

## Compatibility Notes

- Legacy `data_disk_mode` remains supported, but typed `storage_backend` is preferred.
- VM creation now expects storage backend + size metadata and rejects invalid/empty values.
- Backend mismatch errors are surfaced early by controller validation.
