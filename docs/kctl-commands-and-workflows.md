# kctl Commands and Workflows

This guide walks through `kctl` from first-time setup to day-to-day operations, and includes a complete command reference for the current CLI.

## 1) Global flags and connection behavior

These flags are available for most commands:

- `-c, --config <path>`: path to kctl config (default `~/.kcore/config`)
- `-s, --controller <host:port>`: override controller endpoint (repeat to provide ordered fallback endpoints)
- `-k, --insecure`: use plain HTTP and skip TLS client auth
- `--node <host:port>`: target node-agent endpoint for direct node commands

Connection defaults:

- Controller default port: `9090`
- Node-agent default port: `9091`
- Default cert dir: `~/.kcore/certs`

## 2) Initialize cluster PKI and context

Create CA + certificates and write a context:

```bash
kctl create cluster --controller 10.0.0.10:9090
```

Useful options:

- `--certs-dir <path>`: write cert material elsewhere
- `--context <name>`: context name to store
- `--force`: overwrite existing cert files

Result:

- CA and cert/key files are generated
- context is written to config
- mTLS becomes the default transport for that context

## 3) Create and inspect resources

Create a VM from flags (direct URL + SHA256, no manual scp):

```bash
kctl create vm web-01 \
  --cpu 2 \
  --memory 4G \
  --network default \
  --storage-backend filesystem \
  --storage-size-bytes 42949672960 \
  --image https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2 \
  --image-sha256 <sha256>
```

Compliance-first auth (recommended): provide username + SSH public key(s) and keep
`--compliant` at its default `true`:

```bash
kctl create vm web-01 \
  --network default \
  --storage-backend filesystem \
  --storage-size-bytes 42949672960 \
  --image https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2 \
  --image-sha256 <sha256> \
  --username kcore \
  --ssh-public-key "ssh-ed25519 AAAA... user@host"
```

If you explicitly choose password auth, you must acknowledge non-compliance:

```bash
kctl create vm web-01 \
  --network default \
  --storage-backend filesystem \
  --storage-size-bytes 42949672960 \
  --image https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2 \
  --image-sha256 <sha256> \
  --username kcore \
  --password '<temporary-password>' \
  --compliant=false
```

Custom cloud-init user-data is also supported:

```bash
kctl create vm web-01 \
  --network default \
  --storage-backend filesystem \
  --storage-size-bytes 42949672960 \
  --image https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2 \
  --image-sha256 <sha256> \
  --cloud-init-user-data-file ./user-data.yaml
```

Wait until VM is fully ready:

```bash
kctl create vm web-01 \
  --image-path /var/lib/kcore/images/debian12-base.qcow2 \
  --image-format qcow2 \
  --network default \
  --storage-backend filesystem \
  --storage-size-bytes 42949672960 \
  --wait
```

Wait until SSH is reachable from the node host:

```bash
kctl create vm web-01 \
  --image-path /var/lib/kcore/images/debian12-base.qcow2 \
  --image-format qcow2 \
  --network default \
  --storage-backend filesystem \
  --storage-size-bytes 42949672960 \
  --wait-for-ssh \
  --wait-timeout-seconds 300 \
  --ssh-port 22
```

When using `--wait-for-ssh`, failures are surfaced early:

- VM unit crash loops/failures are treated as fatal (wait stops immediately)
- if DHCP lease files are empty, node-agent also attempts ARP/neighbor IP fallback

Create from YAML (idempotent — see §5):

```bash
kctl create vm -f vm.yaml
```

Example VM manifest including `desiredState`:

```yaml
kind: VM
metadata:
  name: web-01
spec:
  cpu: 2
  memoryBytes: 4G
  desiredState: running   # running | stopped (optional; preserves current state if omitted)
  nics:
    - network: default
  disks:
    - image: https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2
      sha256: <sha256>
      format: qcow2
  sshKeys: [deploy-key]
  cloudInitUserData: |
    #cloud-config
    packages: [htop]
```

List and inspect:

```bash
kctl get vms
kctl get vms web-01
kctl get nodes
kctl get nodes node-1
```

Delete:

```bash
kctl delete vm web-01
```

## 4) Declarative VM lifecycle operations

`set vm --state ...` is the canonical declarative lifecycle command. `start` and `stop` are compatibility aliases.

Set desired running state:

```bash
kctl set vm web-01 --state running
```

Set desired stopped state:

```bash
kctl set vm web-01 --state stopped
```

Compatibility aliases:

Start (desired running state):

```bash
kctl start vm web-01
```

Stop (desired stopped state):

```bash
kctl stop vm web-01
```

Both operations update desired state in controller and trigger config apply on the node.

## 5) Declarative apply and idempotency (server-side upsert)

`kctl create -f <manifest>` is **idempotent**: the controller performs a
server-side upsert — it fetches the existing resource (if any), diffs it against
the incoming spec, and then:

- **creates** the resource when no match exists,
- **updates** only mutable fields when the spec drifted,
- **rejects** any change to an immutable field with `InvalidArgument`,
- **does nothing** when stored and desired state already match.

This lets you re-apply the same manifest as often as you want — `kubectl apply`
semantics — so tools like **Terraform** and **Crossplane** can drive kcore with
a single verb (`create`) instead of an imperative mix of `create/update/set`.

The server returns an `ApplyAction` (`CREATED` | `UPDATED` | `UNCHANGED`) and
the list of `changed_fields`, which `kctl` prints as a reconcile summary:

```text
$ kctl create vm -f vm.yaml
updated VM 'web-01' (fields: cpu, memory_bytes, desired_state)
  ID:   vm-…
  Node: node-a
  CPU:  4 cores
  Mem:  8.0 GiB

$ kctl create vm -f vm.yaml
unchanged VM 'web-01'
  …
```

### Mutable vs. immutable fields (v1)

| Kind              | Mutable                                   | Immutable (rejects with `InvalidArgument`)                                                                                 | Notes |
| ----------------- | ----------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- | ----- |
| **VM**            | `cpu`, `memoryBytes`, `desiredState`      | `disks`, `nics`, `storageBackend`, `storageSizeBytes`, `targetNode`*, `sshKeys`, `cloudInitUserData`, `image_*`              | *`targetDc` is not diff-rejected (see below). |
| **Container**     | `desiredState`                            | `image`, `command`, `network`, `env`, `ports`, `storageBackend`, `storageSizeBytes`, `mountTarget`                         | — |
| **Network**       | — (all immutable in v1)                   | all fields                                                                                                                 | — |
| **SshKey**        | — (public key immutable)                  | `publicKey`                                                                                                                | — |
| **SecurityGroup** | `description`, `rules`, attachments       | `name`                                                                                                                     | — |

Rationale: v1 rejects any change that would require rebuilding or recreating
the resource, so reconciliation stays predictable and safe. Future versions can
promote more fields to mutable as controlled rebuild paths are added. Today the
remediation for an immutable change is always `kctl delete … && kctl create -f`.

> Note: `targetDc` is **not** rejected by the diff today (the controller has
> no per-VM DC field to compare against); placement is enforced once at
> create time via the placement preflight. Treat changes to `targetDc` on
> an existing VM as a no-op: the VM stays where it was originally placed.

For **SecurityGroup** specifically, attachments are reconciled as a set:
attachments listed in the manifest are added if missing, and attachments
present on the controller but absent from the manifest are detached. The
reconcile summary aggregates rule/description changes with attachment
deltas, e.g. `updated security group 'web' (fields: rules,
attachments(+1,-2))`.

### Terraform / Crossplane integration

Because every `create*` RPC is already a declarative upsert, a Terraform
provider or Crossplane composition needs only **one** verb per resource:

- `Create` (in Terraform) / reconcile loop (in Crossplane) → gRPC `CreateX`
  with the desired spec.
- `Update` (Terraform) / reconcile on drift (Crossplane) → same gRPC
  `CreateX`. The controller handles the diff server-side and either applies
  mutable fields or returns `InvalidArgument`, which the provider surfaces as
  a diagnostic telling the user to destroy + recreate.
- `Delete` → existing gRPC `DeleteX`.

No client-side diff, no client-side state of mutable/immutable fields — the
controller is the single source of truth.

## 6) Node operations

Direct node inspection:

```bash
kctl --node 10.0.0.21:9091 node disks
kctl --node 10.0.0.21:9091 node nics
```

Install KcoreOS to disk:

```bash
kctl --node 10.0.0.21:9091 node install \
  --os-disk /dev/sda \
  --data-disk /dev/nvme0n1 \
  --join-controller 10.0.0.10:9090 \
  --storage-backend filesystem
```

Install with VXLAN disabled (simple networking only):

```bash
kctl --node 10.0.0.21:9091 node install \
  --os-disk /dev/sda \
  --join-controller 10.0.0.10:9090 \
  --join-controller 10.0.0.11:9090 \
  --dc-id DC1 \
  --storage-backend filesystem \
  --disable-vxlan
```

`--join-controller` is repeatable; the first endpoint is treated as primary and the rest are stored in node-agent config for fallback order.

Apply Nix to a node:

```bash
kctl --node 10.0.0.21:9091 node apply-nix -f ./node-config.nix
```

Skip rebuild (write only):

```bash
kctl --node 10.0.0.21:9091 node apply-nix -f ./node-config.nix --no-rebuild
```

Validate a day-2 disk layout on a node (no writes, structural + classifier check only):

```bash
kctl --node 10.0.0.21:9091 node apply-disk -f ./day2-disk.nix
```

Apply a day-2 disk layout to a single node (requires `controller-managed`
disk-management mode on the node). The node-agent persists the layout to
`/etc/kcore/disk/current.nix` and chains `nixos-rebuild test` then
`nixos-rebuild switch` automatically; pass `--no-rebuild` to skip:

```bash
kctl --node 10.0.0.21:9091 node apply-disk \
  -f ./day2-disk.nix \
  --apply \
  --timeout-seconds 600
```

The legacy `node apply-disko` spelling stays as an alias for one release.

### Declarative DiskLayout via the controller (preferred)

Submit the same Nix body as a controller-managed `DiskLayout` resource so it
is replicated across controllers and reconciled to the target node:

```yaml
kind: DiskLayout
metadata:
  name: prod-data-pool
spec:
  nodeId: node-prod-01
  layoutNixFile: ./day2-disk.nix
```

```bash
kctl diff   -f day2-disk-layout.yaml
kctl apply  -f day2-disk-layout.yaml
kctl get    disk-layouts
kctl describe disk-layout prod-data-pool
kctl delete disk-layout prod-data-pool   # removes it from the controller; does NOT touch the node
```

The controller never touches VMs. If the node-agent's classifier refuses the
layout (the targeted disk currently backs an active kcore volume, an active
LVM PV, or a ZFS pool member), the refusal code surfaces on
`status.refusalReason`. Drain the affected VMs and resubmit the same
manifest — the reconciler retries until the node accepts.

## 7) Image operations

There are two supported VM image flows:

1. URL-backed flow (controller-managed download):
   - `kctl create vm ... --image <https-url> --image-sha256 <sha256>`
2. Node-local upload flow (two-step):
   - `kctl --node <node:9091> node upload-image -f ./disk.qcow2`
   - `kctl create vm ... --image-path /var/lib/kcore/images/<uploaded-name> --image-format qcow2`

Upload flow details:

- `node upload-image` accepts only `raw` and `qcow2`.
- You can force format with `--format raw|qcow2`; if omitted, kctl infers from filename.
- Optional integrity check: `--image-sha256 <hex>`.
- Upload uses chunked gRPC streaming, so large images (for example multi-GB raw disks) are supported.
- Response includes final node-local path, format, size, and computed SHA256.
- ISO uploads are not supported in this workflow.

Example:

```bash
kctl --node 10.0.0.21:9091 node upload-image \
  -f ./debian-12-genericcloud-amd64.qcow2 \
  --name debian12-base.qcow2

kctl create vm web-01 \
  --cpu 2 \
  --memory 4G \
  --network default \
  --storage-backend filesystem \
  --storage-size-bytes 42949672960 \
  --image-path /var/lib/kcore/images/debian12-base.qcow2 \
  --image-format qcow2
```

For a full image-centric guide (including large raw upload and wait-for-ssh flow), see:

- `docs/images.md`

## 8) Network operations

### Create a network

```bash
kctl create network <name> \
  --external-ip <ip> \
  --gateway-ip <ip> \
  [--type <nat|bridge|vxlan>] \
  [--internal-netmask <mask>] \
  [--vlan-id <id>] \
  [--no-outbound-nat] \
  [--target-node <node-addr-or-id>]
```

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `<name>` | yes | — | Network name |
| `--external-ip` | yes | — | Public IP for NAT/DNAT |
| `--gateway-ip` | yes | — | Bridge gateway IP |
| `--type` | no | `nat` | Network type: `nat`, `bridge`, or `vxlan` |
| `--internal-netmask` | no | `255.255.255.0` | Subnet mask |
| `--vlan-id` | no | `0` | 802.1Q VLAN tag |
| `--no-outbound-nat` | no | `false` | Disable masquerade (vxlan only) |
| `--target-node` | no | auto | Target node |

Examples:

```bash
# NAT network (default)
kctl create network frontend \
  --external-ip 203.0.113.10 \
  --gateway-ip 10.240.10.1

# Bridge network (VMs on physical LAN)
kctl create network lan \
  --type bridge \
  --external-ip 192.168.1.100 \
  --gateway-ip 192.168.1.1

# VXLAN overlay (cross-host L2)
kctl create network cluster \
  --type vxlan \
  --external-ip 203.0.113.10 \
  --gateway-ip 10.250.0.1

# VXLAN without outbound NAT
kctl create network internal \
  --type vxlan \
  --external-ip 203.0.113.10 \
  --gateway-ip 10.251.0.1 \
  --no-outbound-nat
```

### List and delete networks

```bash
kctl get networks [--target-node <node>]
kctl delete network <name> [--target-node <node>]
```

The list output includes a `TYPE` column showing the network type.

For detailed networking documentation, see `docs/networking.md`.

## 9) Controller apply

Apply a NixOS configuration to the controller:

```bash
kctl apply -f ./controller-config.nix
```

Targeting behavior:

- `kctl apply` always targets the **controller endpoint**, selected by:
  - `--controller <host:port>`, or
  - current context in `~/.kcore/config`.
- It does **not** select arbitrary nodes.
- For node-specific apply, use `kctl --node <host:9091> node apply-nix -f ...`.

Preview only:

```bash
kctl apply -f ./controller-config.nix --dry-run
```

## 10) Complete command reference

Top-level commands:

- `kctl create vm ... --storage-backend <filesystem|lvm|zfs> --storage-size-bytes <bytes>`
- `kctl create cluster ...`
- `kctl create network <name> --external-ip ... --gateway-ip ... [--type nat|bridge|vxlan] [--no-outbound-nat] [--vlan-id ...] [--target-node ...]`
- `kctl create ssh-key <name> --public-key "ssh-rsa ..."`
- `kctl delete vm ...`
- `kctl delete network <name> [--target-node ...]`
- `kctl delete image ...`
- `kctl delete ssh-key <name>`
- `kctl set vm ... --state <running|stopped>`
- `kctl start vm ...`
- `kctl stop vm ...`
- `kctl get vms [name]`
- `kctl get nodes [name]`
- `kctl get networks [--target-node ...]`
- `kctl get ssh-keys`
- `kctl get compliance-report` (full compliance report with per-node breakdown)
- `kctl node disks`
- `kctl node nics`
- `kctl node install --os-disk ... --join-controller ... [--data-disk ...] [--storage-backend filesystem|lvm|zfs] [--disable-vxlan]`
- `kctl node apply-nix -f ... [--no-rebuild]`
- `kctl node apply-disk -f ... [--apply] [--timeout-seconds N] [--no-rebuild]` (alias `apply-disko`)
- `kctl apply -f <DiskLayout>.yaml`, `kctl diff -f <DiskLayout>.yaml`, `kctl get disk-layouts`, `kctl describe disk-layout <name>`, `kctl delete disk-layout <name>`
- `kctl pull image <uri>` (legacy/manual path)
- `kctl node approve <NODE_ID>`
- `kctl node reject <NODE_ID>`
- `kctl rotate certs --controller <host:port>` (rotate controller cert and push to controller)
- `kctl rotate sub-ca` (generate and push new sub-CA to controller)
- `kctl apply -f ... [--dry-run]`
- `kctl version`

## 11) Common operator patterns


New environment:

1. `kctl create cluster --controller <controller:9090>`
2. Install each node with `kctl --node ... node install ...`
3. Create VMs with `kctl create vm ... --storage-backend ... --storage-size-bytes ... --image <https-url> --image-sha256 <sha256>`

Day-2 operations:

1. inspect with `kctl get ...` (the nodes table includes `CERT EXPIRY` and `LUKS` columns)
2. review compliance posture with `kctl get compliance-report` (crypto, mTLS, access control, encryption at rest, per-node cert and LUKS status)
3. adjust desired VM running state with `kctl set vm ... --state ...` (or `kctl start/stop vm ...`)
4. update configs with `kctl node apply-nix ...` or `kctl apply ...`
5. for day-2 disk layout changes, prefer declarative `kctl apply -f <DiskLayout>.yaml` (use `kctl diff -f` first); for one-off pushes, `kctl node apply-disk ...` (validate first, then `--apply`)
6. rotate controller cert with `kctl rotate certs --controller <host:port>`
7. rotate sub-CA with `kctl rotate sub-ca`

## 12) Storage backend examples

Install node with LVM data disk mode:

```bash
kcore-kctl --node 192.168.40.105:9091 node install \
  --os-disk /dev/sda \
  --data-disk /dev/nvme0n1 \
  --join-controller 192.168.40.105:9090 \
  --data-disk-mode lvm
```

Create VM with ZFS storage settings:

```bash
kcore-kctl create vm app-zfs-01 \
  --image-path /var/lib/kcore/images/ubuntu-24.04.raw \
  --image-format raw \
  --cpu 2 \
  --memory 4G \
  --network default \
  --storage-backend zfs \
  --storage-size-bytes 42949672960 \
  --target-node 192.168.40.105:9091
```
