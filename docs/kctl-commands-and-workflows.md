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

Create a VM from flags:

```bash
kctl create vm web-01 --cpu 2 --memory 4G --network default
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

`start` and `stop` are reconciliation requests (desired-state updates), not direct runtime hypervisor calls.

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

Pull image to node:

```bash
kctl --node 10.0.0.21:9091 pull image https://example.com/debian.raw
```

Delete image from node:

```bash
kctl --node 10.0.0.21:9091 delete image debian.raw --force
```

## 7) Controller apply

Apply a NixOS configuration to the controller:

```bash
kctl apply -f ./controller-config.nix
```

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
- `kctl start vm ...`
- `kctl stop vm ...`
- `kctl get vms [name]`
- `kctl get nodes [name]`
- `kctl node disks`
- `kctl node nics`
- `kctl node install --os-disk ... --join-controller ... [--data-disk ...]`
- `kctl node apply-nix -f ... [--no-rebuild]`
- `kctl pull image <uri>`
- `kctl apply -f ... [--dry-run]`
- `kctl version`

## 9) Common operator patterns

New environment:

1. `kctl create cluster --controller <controller:9090>`
2. Install each node with `kctl --node ... node install ...`
3. Create VMs with `kctl create vm ...`

Day-2 operations:

1. inspect with `kctl get ...`
2. adjust desired VM running state with `kctl start/stop vm ...`
3. update configs with `kctl node apply-nix ...` or `kctl apply ...`
