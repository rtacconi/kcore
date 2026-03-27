# Adding a New Node to a kcore Cluster

This guide walks through adding a new bare-metal node (agent-only) to an
existing kcore cluster. The new node will run `kcore-node-agent` and join
the controller for VM scheduling.

## Prerequisites

- An existing kcore cluster with a running controller (e.g., on
  `192.168.40.105:9090`).
- The operator machine has `kctl` configured with the cluster context
  (`~/.kcore/<context>/` containing `ca.crt`, `ca.key`, `kctl.crt`,
  `kctl.key`).
- The new node is booted from the kcore ISO (or running the kcore live
  environment) and is network-reachable from the operator machine.

## Step 1: Discover the new node's hardware

Before installing, you can inspect the new node's disks and network
interfaces to confirm which devices to target.

```bash
# List available block devices
kctl --node <NEW_NODE_IP>:9091 node disks

# List network interfaces
kctl --node <NEW_NODE_IP>:9091 node nics
```

Replace `<NEW_NODE_IP>` with the IP address of the new node (visible on
its console or via DHCP lease).

## Step 2: Install kcore to disk

Run the install command from the operator machine. This signs a fresh
node certificate using the cluster CA and pushes it (along with the OS)
to the new node.

```bash
kctl --node <NEW_NODE_IP>:9091 node install \
  --os-disk /dev/sda \
  --join-controller 192.168.40.105:9090 \
  --join-controller 192.168.40.106:9090 \
  --dc-id DC1
```

### Common options

| Flag | Description |
|------|-------------|
| `--os-disk /dev/sda` | Target disk for the OS install (required) |
| `--join-controller <host:port>` | Controller address to join (required for agent-only nodes, repeatable for fallback order) |
| `--dc-id DC1` | Datacenter identity to persist in node config (defaults to `DC1`) |
| `--data-disk /dev/nvme0n1` | Additional data disk(s), can be repeated |
| `--storage-backend filesystem` | Storage backend: `filesystem`, `lvm`, or `zfs` |
| `--lvm-vg-name vg0` | LVM volume group name (when using `lvm` backend) |
| `--zfs-pool-name tank0` | ZFS pool name (when using `zfs` backend) |
| `--disable-vxlan` | Disable VXLAN overlay networking on this node |

### What happens during install

1. `kctl` reads the cluster CA cert and key from
   `~/.kcore/<context>/`.
2. It signs a new node certificate with the new node's IP as the
   Subject Alternative Name (SAN).
3. It sends an `InstallToDiskRequest` via gRPC to the new node's
   agent. The request includes:
   - The CA certificate (so the node trusts the controller)
   - The freshly signed node certificate and private key
   - Ordered controller endpoints (`192.168.40.105:9090`, `192.168.40.106:9090`, ...)
   - Datacenter identity (`dcId`, default `DC1`)
4. The node-agent writes the OS to disk, places the certificates in
   `/etc/kcore/certs/`, and writes `node-agent.yaml` with
   both `controllerAddr` (primary) and `controllers` (fallback list), plus `dcId`.
5. The node reboots into the installed system.

## Step 3: Node self-registration (automatic)

After reboot, the node-agent starts automatically and connects to the
controller over mTLS. It sends a `RegisterNode` RPC with its hostname,
CPU count, and memory capacity.

Because the node is new, the controller records it with
`approval_status: pending`. The node **cannot**:

- Receive VM scheduling decisions
- Have its heartbeats promote it to `ready`
- Receive Nix configuration pushes

This is visible in the controller logs:

```
INFO kcore_controller::grpc::controller: node registered with pending approval
     node_id=kvm-node-02 address=192.168.40.110:9091 approval_status=pending
```

## Step 4: List nodes and see the pending node

From the operator machine:

```bash
kctl get nodes
```

Example output:

```
ID                    HOSTNAME              ADDRESS           CORES      MEMORY  STATUS      STORAGE     APPROVAL
kvm-node-01           kvm-node-01           192.168.40.105        8      32 GiB  ready       filesystem  approved
kvm-node-02           kvm-node-02           192.168.40.110        4      16 GiB  pending     filesystem  pending
```

The new node appears with `STATUS: pending` and `APPROVAL: pending`.

## Step 5: Approve the node

Once you have verified the node is legitimate, approve it:

```bash
kctl node approve kvm-node-02
```

Output:

```
node 'kvm-node-02' approved
```

The controller transitions the node to `approval_status: approved` and
`status: ready`. It also establishes a gRPC connection to the node for
configuration pushes. The node is now eligible for VM scheduling.

## Step 6: Verify the approved node

```bash
kctl get nodes
```

```
ID                    HOSTNAME              ADDRESS           CORES      MEMORY  STATUS      STORAGE     APPROVAL
kvm-node-01           kvm-node-01           192.168.40.105        8      32 GiB  ready       filesystem  approved
kvm-node-02           kvm-node-02           192.168.40.110        4      16 GiB  ready       filesystem  approved
```

For detailed information about the new node:

```bash
kctl get node kvm-node-02
```

```
ID:        kvm-node-02
Hostname:  kvm-node-02
Address:   192.168.40.110:9091
Status:    ready
Approval:  approved
CPU:       4 cores
Memory:    16 GiB
CPU used:  0 cores
Mem used:  0 B
Storage:   filesystem
```

## Rejecting a node

If a node appears in the pending queue that you do not recognize, reject
it:

```bash
kctl node reject kvm-node-unknown
```

A rejected node has `approval_status: rejected` and `status: rejected`.
It cannot schedule VMs or send heartbeats. If the node attempts to
re-register, it stays rejected.

## Re-registration behavior

- **Approved nodes** that reboot and re-register keep their `approved`
  status automatically. No operator action is needed.
- **Rejected nodes** that re-register stay `rejected`. To allow a
  previously rejected node back in, approve it explicitly.
- **Pending nodes** that re-register (e.g., agent restart) stay
  `pending` until approved.

## Certificate renewal

After installation, node certificates are valid for 1 year. The node-agent
automatically checks certificate expiry at startup and once daily:

- If the certificate expires in more than 30 days, no action is taken.
- If the certificate is within 30 days of expiry, the node-agent calls
  `RenewNodeCert` on the controller. The controller signs a new
  certificate using its **sub-CA** (an intermediate CA deployed during
  cluster creation) and returns the new cert chain + private key.
- The node-agent writes the renewed certificate to disk. The new cert
  takes effect on the next service restart.

This is fully automatic and requires no operator intervention. The
controller log shows:

```
INFO kcore_controller::grpc::controller: renewed node certificate via sub-CA
     node_id=kvm-node-02 host=192.168.40.110
```

The operator can rotate the sub-CA at any time with:

```bash
kctl rotate sub-ca
```

This generates a new sub-CA from the root CA and pushes it to the
controller. Existing node certs remain valid (the root CA is the trust
anchor); future renewals use the new sub-CA.

### Monitoring certificate expiry

`kctl get nodes` includes a `CERT EXPIRY` column showing days until each
node's certificate expires:

```
ID                    HOSTNAME     ADDRESS             CORES    MEMORY  STATUS   STORAGE     APPROVAL    CERT EXPIRY
kvm-node-01           kvm-node     192.168.40.105:9091      6   15.5 GB  ready    filesystem  approved           363d
kvm-node-02           kvm-node-02  192.168.40.110:9091      4   16.0 GB  ready    filesystem  approved            25d ⚠
```

- Certificates expiring within 30 days are flagged with `⚠`.
- `EXPIRED` is shown if the certificate has already expired.
- `unknown` appears if the node has not yet reported its certificate status.

The detailed view (`kctl get node <NODE_ID>`) also includes a `Cert:` line.

## Security model

The approval queue adds a human gate on top of the mTLS trust model. Even
if an attacker obtains a certificate signed by the cluster CA, the node
still needs operator approval before it can participate in the cluster.

Only approved nodes can renew their certificates. Pending or rejected
nodes receive a `PermissionDenied` error from the `RenewNodeCert` RPC.

See [docs/security-k8s-vs-kcore.md](security-k8s-vs-kcore.md) for a
full comparison of kcore's security model with Kubernetes.

## Quick reference

| Action | Command |
|--------|---------|
| List disks on new node | `kctl --node <IP>:9091 node disks` |
| List NICs on new node | `kctl --node <IP>:9091 node nics` |
| Install to disk (join cluster, with fallback) | `kctl --node <IP>:9091 node install --os-disk /dev/sda --join-controller <CTRL1>:9090 --join-controller <CTRL2>:9090 --dc-id DC1` |
| List all nodes | `kctl get nodes` |
| Get node details | `kctl get node <NODE_ID>` |
| Approve pending node | `kctl node approve <NODE_ID>` |
| Reject pending node | `kctl node reject <NODE_ID>` |
| Rotate sub-CA | `kctl rotate sub-ca` |
