# KCORE Architecture & Workflow

For Mermaid diagrams of the current architecture, see [ARCHITECTURE_MERMAID.md](ARCHITECTURE_MERMAID.md).

## System Architecture

```
┌─────────────────────┐
│   Your Mac/Linux    │
│                     │
│  kcore-controller   │  ← Manages multiple nodes
│   (port 8080)       │     Receives node registrations
└──────────┬──────────┘     Sends commands to nodes
           │
           │ gRPC (TLS)
           │
    ┌──────┴───────┬──────────────┬───────────────┐
    │              │              │               │
┌───▼────────┐ ┌──▼─────────┐ ┌──▼─────────┐ ┌──▼─────────┐
│  KVM Node  │ │  KVM Node  │ │  KVM Node  │ │  KVM Node  │
│    #1      │ │    #2      │ │    #3      │ │    #N      │
│            │ │            │ │            │ │            │
│ node-agent │ │ node-agent │ │ node-agent │ │ node-agent │
│ (port 9091)│ │ (port 9091)│ │ (port 9091)│ │ (port 9091)│
│            │ │            │ │            │ │            │
│ libvirtd   │ │ libvirtd   │ │ libvirtd   │ │ libvirtd   │
│  ┌──┐ ┌──┐ │ │  ┌──┐ ┌──┐ │ │  ┌──┐ ┌──┐ │ │  ┌──┐ ┌──┐ │
│  │VM│ │VM│ │ │  │VM│ │VM│ │ │  │VM│ │VM│ │ │  │VM│ │VM│ │
│  └──┘ └──┘ │ │  └──┘ └──┘ │ │  └──┘ └──┘ │ │  └──┘ └──┘ │
└────────────┘ └────────────┘ └────────────┘ └────────────┘
```

## Component Roles

### Controller (kcore-controller)
- **Runs on**: Your Mac/Linux workstation
- **Port**: 8080 (gRPC with TLS, default `-listen :8080`)
- **Role**: 
  - Accepts node registrations
  - Receives heartbeats from nodes
  - Maintains cluster state
  - Distributes VM creation/management commands
- **Does NOT**: Start or manage node-agent processes

### Node Agent (kcore-node-agent)
- **Runs on**: Each KVM node (bare metal servers)
- **Port**: 9091 (gRPC with TLS)
- **Role**:
  - **Registers** with controller on startup (outbound connection)
  - Sends heartbeats to controller
  - Receives VM management commands from controller
  - Manages local VMs via libvirtd
  - Reports node status and resource usage
- **Managed by**: systemd (auto-starts on boot)

### libvirtd + virtlogd
- **Runs on**: Each KVM node
- **Role**: Low-level VM/QEMU management
- **Managed by**: systemd (auto-starts on boot)
- **Used by**: node-agent for VM operations

### Networking

kcore uses **libvirt** for VM networking. The default configuration uses libvirt's built-in "default" network:

- **libvirt default network** (recommended for simple setups):
  - NAT with DHCP via `virbr0`
  - VMs get IPs from libvirt's DHCP (typically 192.168.122.0/24)
  - No additional bridge configuration required

- **Custom bridges** (optional):
  - Map network names to Linux bridge interfaces (e.g., `br0`, `br1`)
  - Use when VMs need direct access to a host subnet
  - Configure `networks: default: br0` in node-agent config

---

## Installation Workflow

### 1. Build ISO
```bash
nix build '.#nixosConfigurations.kvm-node-iso.config.system.build.isoImage' \
  --extra-experimental-features "nix-command flakes"
```

### 2. Prepare Certificates (BEFORE writing to USB)

**Option A: Use existing certs**
```bash
# Mount the ISO (or modify it before building)
mkdir -p /mnt/iso/etc/kcore
cp certs/*.{crt,key} /mnt/iso/etc/kcore/
cp examples/node-agent.yaml /mnt/iso/etc/kcore/
```

**Option B: Add certs to live USB after boot**
```bash
# Boot USB, login as root
mkdir -p /etc/kcore

# From your workstation:
scp certs/*.{crt,key} root@<node-ip>:/etc/kcore/
scp examples/node-agent.yaml root@<node-ip>:/etc/kcore/

# Edit config with correct controller IP
vi /etc/kcore/node-agent.yaml
# Change controllerAddr to your controller's IP:port
```

### 3. Boot from USB

System boots to login prompt with:
- ✅ DHCP networking (auto-configured)
- ✅ SSH enabled (port 22)
- ✅ Root password: `kcore`
- ❌ libvirtd NOT running (not needed on live ISO)
- ❌ node-agent NOT running (no certs yet)

### 4. Run install-to-disk

The installer will:
1. ✅ Deactivate LVM volumes
2. ✅ Unmount partitions
3. ✅ Partition and format disk
4. ✅ Install NixOS
5. ✅ Copy node-agent binary to `/opt/kcore/bin/`
6. ✅ Copy `/etc/kcore/*` (certs + config) if present
7. ✅ Copy SSH authorized keys if present
8. ✅ Configure systemd services:
   - `libvirtd.service` (enabled)
   - `virtlogd.service` (enabled)
   - `kcore-node-agent.service` (enabled)

### 5. Reboot into Installed System

System boots with:
- ✅ libvirtd running
- ✅ virtlogd running
- ✅ node-agent running (if `/etc/kcore/node-agent.yaml` exists)
- ✅ node-agent automatically registers with controller
- ✅ SSH enabled with your keys

---

## Communication Flow

### Node Registration (Startup)

```
KVM Node                          Controller
   │                                  │
   │  1. node-agent starts            │
   │                                  │
   │  2. Reads /etc/kcore/node-agent.yaml
   │     Gets controller address      │
   │                                  │
   │  3. gRPC: RegisterNode() ───────>│
   │     { nodeId, capabilities }     │
   │                                  │
   │  <────── { success }             │
   │                                  │
   │  4. Start heartbeat timer        │
   │                                  │
   │  5. gRPC: Heartbeat() ──────────>│
   │     (every 30s)                  │
   │                                  │
   │  <────── { commands }            │
   │                                  │
```

### VM Creation (Controller → Node)

```
Your Workstation       Controller              Node Agent        libvirtd
      │                    │                       │                │
      │ API: CreateVM      │                       │                │
      ├───────────────────>│                       │                │
      │                    │                       │                │
      │                    │ gRPC: CreateVm()      │                │
      │                    ├──────────────────────>│                │
      │                    │                       │                │
      │                    │                       │ virsh create   │
      │                    │                       ├───────────────>│
      │                    │                       │                │
      │                    │                       │ <─── success ──┤
      │                    │                       │                │
      │                    │ <── { vmStatus } ─────┤                │
      │                    │                       │                │
      │ <── { success } ───┤                       │                │
      │                    │                       │                │
```

---

## Configuration Files

### Controller

The controller binary uses **flags** (not a YAML file): `-listen :8080`, `-cert`, `-key`, `-ca`. Example:

```bash
./bin/kcore-controller -listen :8080 -cert certs/controller.crt -key certs/controller.key -ca certs/ca.crt
```

It keeps **in-memory** state (node registry, VM-to-node mapping). A separate SQLite-based reconciler exists in the codebase but is not used by the default binary.

### Node Agent Config (`/etc/kcore/node-agent.yaml`)

```yaml
nodeId: kvm-node-01
controllerAddr: "192.168.1.100:8080"  # Your controller's IP

tls:
  caFile: /etc/kcore/ca.crt
  certFile: /etc/kcore/node.crt
  keyFile: /etc/kcore/node.key

networks:
  default: default  # libvirt default network (NAT + DHCP)

storage:
  drivers:
    local-dir:
      type: local-dir
      parameters:
        path: /var/lib/kcore/disks
```

---

## Systemd Services on Installed Node

### Service Dependencies

```
multi-user.target
    │
    ├── network-online.target
    │       │
    ├── libvirtd.service ←── virtlogd.service
    │       │
    └── kcore-node-agent.service
            │
            └── (requires: libvirtd.service)
```

### Service Status

```bash
# Check all kcore services
systemctl status libvirtd
systemctl status virtlogd
systemctl status kcore-node-agent

# View logs
journalctl -u kcore-node-agent -f
journalctl -u libvirtd -f
```

---

## Questions Answered

### Q1: Will systemd start libvirtd?
**YES!** The installed system has:
```nix
virtualisation.libvirtd = {
  enable = true;
  qemu.runAsRoot = true;
};
```
This creates a systemd service that starts on boot.

### Q2: Can I send a command to controller to run the node?
**NO!** The architecture is **agent-based**:
- Node-agent runs independently (started by systemd)
- Node-agent **REGISTERS** with controller (agent → controller)
- Controller then **SENDS COMMANDS** to registered nodes (controller → agent)

You cannot "start" a node from the controller. The node must be running and will self-register.

### Q3: Is node-agent code in flake.nix?
**YES!** (After this fix):
- `install-to-disk` copies node-agent binary to `/opt/kcore/bin/`
- `install-to-disk` generates systemd service in `configuration.nix`
- Node-agent auto-starts on boot via systemd

---

## Testing the Full Stack

### 1. Start Controller (on your Mac)

```bash
cd /path/to/kcore
./bin/kcore-controller -listen :8080 -cert certs/controller.crt -key certs/controller.key -ca certs/ca.crt
```

Output:
```
Starting kcore controller on :8080
Waiting for nodes to register...
```

### 2. Install and Boot Node

Follow QUICKSTART.md - after reboot, node-agent auto-starts.

Check logs on node:
```bash
journalctl -u kcore-node-agent -f
```

Output:
```
Starting kcore node agent (node ID: kvm-node-01)
Registering with controller at 192.168.1.100:8080
Successfully registered with controller
Starting heartbeat loop
```

### 3. Controller Sees Node

Controller logs:
```
Node registered: kvm-node-01
Heartbeat received from kvm-node-01
```

### 4. Create VM via Controller

```bash
# Option A: Use kctl (talks to controller via gRPC)
kctl create vm my-vm --cpu 2 --memory 4G --node <node-address>:9091

# Or call controller gRPC (e.g. with grpcurl) at localhost:8080

# Option B: Direct to node (for testing)
nix-shell -p grpcurl --run "grpcurl -insecure \
  -cert ./certs/node.crt -key ./certs/node.key \
  -import-path ./proto -proto node.proto \
  -d '{\"spec\": {...}}' \
  <node-ip>:9091 kcore.node.NodeCompute/CreateVm"
```

### 5. Verify VM Running

```bash
ssh root@<node-ip>
virsh list --all
```

---

## Summary

✅ **Fully Automated After USB Install**
- libvirtd auto-starts
- virtlogd auto-starts
- node-agent auto-starts
- node-agent auto-registers with controller

✅ **No Manual Intervention Required**
- Just add certs to live USB before running install-to-disk
- Everything else is automatic

✅ **Proper Architecture**
- Controller doesn't start nodes
- Nodes register themselves
- Controller sends commands to registered nodes

✅ **Production Ready**
- systemd manages all services
- Automatic restarts on failure
- Proper dependency ordering

