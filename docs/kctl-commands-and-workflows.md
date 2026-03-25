# kctl Commands and Workflows

This guide walks through `kctl` from first-time setup to day-to-day operations, and includes a complete command reference for the current CLI.

## 1) Global flags and connection behavior

These flags are available for most commands:

- `-c, --config <path>`: path to kctl config (default `~/.kcore/config`)
- `-s, --controller <host:port>`: override controller endpoint
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
  --image https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-genericcloud-amd64.qcow2 \
  --image-sha256 <sha256>
```

Create from YAML:

```bash
kctl create vm -f vm.yaml
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

## 5) Node operations

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
  --join-controller 10.0.0.10:9090
```

Apply Nix to a node:

```bash
kctl --node 10.0.0.21:9091 node apply-nix -f ./node-config.nix
```

Skip rebuild (write only):

```bash
kctl --node 10.0.0.21:9091 node apply-nix -f ./node-config.nix --no-rebuild
```

## 6) Image operations

VM image download is controller-managed through VM create (`--image` + `--image-sha256`).

Legacy direct node image commands remain in CLI for compatibility, but the recommended and reproducible flow is:

1. `kctl create vm ... --image <https-url> --image-sha256 <sha256>`
2. controller validates and persists image metadata
3. node-agent downloads/verifies image to `/var/lib/kcore/images/...`
4. controller applies rendered Nix config

## 7) Controller apply

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

## 8) Complete command reference

Top-level commands:

- `kctl create vm ...`
- `kctl create cluster ...`
- `kctl delete vm ...`
- `kctl delete image ...`
- `kctl set vm ... --state <running|stopped>`
- `kctl start vm ...`
- `kctl stop vm ...`
- `kctl get vms [name]`
- `kctl get nodes [name]`
- `kctl node disks`
- `kctl node nics`
- `kctl node install --os-disk ... --join-controller ... [--data-disk ...]`
- `kctl node apply-nix -f ... [--no-rebuild]`
- `kctl pull image <uri>` (legacy/manual path)
- `kctl apply -f ... [--dry-run]`
- `kctl version`

## 9) Common operator patterns

New environment:

1. `kctl create cluster --controller <controller:9090>`
2. Install each node with `kctl --node ... node install ...`
3. Create VMs with `kctl create vm ... --image <https-url> --image-sha256 <sha256>`

Day-2 operations:

1. inspect with `kctl get ...`
2. adjust desired VM running state with `kctl set vm ... --state ...` (or `kctl start/stop vm ...`)
3. update configs with `kctl node apply-nix ...` or `kctl apply ...`
