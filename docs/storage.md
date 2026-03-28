# Storage Backends (FS/LVM/ZFS)

This document explains how storage works end-to-end in kcore:

- operator commands (`kcore-kctl`)
- controller API and validation
- node install-time storage configuration
- generated Nix VM configuration fields
- source files involved in the storage flow (with detailed responsibilities)

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

- `crates/kctl/src/commands/vm.rs`  
  Validates and maps storage CLI arguments into `CreateVmRequest`.

- `crates/kctl/src/commands/node.rs`  
  Maps typed install storage flags into `InstallToDiskRequest`.

- `crates/kctl/src/output.rs`  
  Renders node storage backend in operator output.

### Nix module surface

- `modules/ch-vm/options.nix`  
  Declares VM storage options accepted by generated config (`storageBackend`, `storageSizeBytes`).

- `modules/ch-vm/vm-service.nix`  
  VM runtime service generation. Storage metadata is currently represented as VM config fields and can be consumed by future service-level provisioning logic.

## Compatibility Notes

- Legacy `data_disk_mode` remains supported, but typed `storage_backend` is preferred.
- VM creation now expects storage backend + size metadata and rejects invalid/empty values.
- Backend mismatch errors are surfaced early by controller validation.
