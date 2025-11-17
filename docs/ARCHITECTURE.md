# KCORE Architecture & Workflow

## System Architecture

```
┌─────────────────────┐
│   Your Mac/Linux    │
│                     │
│  kcore-controller   │  ← Manages multiple nodes
│   (port 9090)       │     Receives node registrations
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
- **Port**: 9090 (gRPC with TLS)
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

### Controller Config (`controller.yaml`)

```yaml
listenAddr: ":9090"
tls:
  caFile: /path/to/ca.crt
  certFile: /path/to/controller.crt
  keyFile: /path/to/controller.key
database:
  path: /path/to/kcore.db
```

### Node Agent Config (`/etc/kcore/node-agent.yaml`)

```yaml
nodeId: kvm-node-01
controllerAddr: "192.168.1.100:9090"  # Your controller's IP

tls:
  caFile: /etc/kcore/ca.crt
  certFile: /etc/kcore/node.crt
  keyFile: /etc/kcore/node.key

networks:
  default: br0

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
./controller -config controller.yaml
```

Output:
```
Starting kcore controller on :9090
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
Registering with controller at 192.168.1.100:9090
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
# Option A: Via controller's API
curl -X POST http://localhost:9090/api/v1/vms \
  -d '{"nodeId": "kvm-node-01", "spec": {...}}'

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

