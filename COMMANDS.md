# KCORE Command Reference

All commands are available via devbox. Run `devbox run help` for a quick overview.

---

## 📦 Build Commands

### Generate Protobuf Code
```bash
devbox run proto
```
Generates Go code from `.proto` files in `api/`.

### Build Node Agent
```bash
devbox run build-node-agent
```
Builds `kcore-node-agent` for Linux/amd64 using Podman container with libvirt support.

### Build Controller
```bash
devbox run build-controller
```
Builds `controller` binary for your current platform.

### Build ISO
```bash
devbox run build-iso
```
Builds the bootable NixOS ISO with kcore node-agent embedded.
- Takes 10-30 minutes
- Output: `result/iso/*.iso`
- Size: ~1GB

---

## 🚀 Running Services

### Start Controller
```bash
devbox run start-controller
```
Starts the controller using `examples/controller.yaml` config.

**Prerequisites:**
- Controller binary built (`devbox run build-controller`)
- `examples/controller.yaml` configured with correct paths
- Certificates in `certs/` directory

---

## ☁️ Node Management

All node management commands require `NODE_IP` environment variable:

### Test Node Connectivity
```bash
NODE_IP=192.168.40.146 devbox run test-node
```
Tests:
- TCP connection to port 9091
- gRPC service availability
- Certificate authentication

### List Node Services
```bash
NODE_IP=192.168.40.146 devbox run list-node-services
```
Lists all available gRPC services on the node.

### Deploy Node Agent
```bash
NODE_IP=192.168.40.146 devbox run deploy-node
```
Deploys to an existing node:
- Copies node-agent binary to `/opt/kcode/bin/`
- Copies certificates to `/etc/kcode/`
- Copies `node-agent.yaml` config

**Note:** Requires node-agent to be built first.

### Create VM
```bash
NODE_IP=192.168.40.146 devbox run create-vm
```
Creates a test VM with:
- Random UUID
- Name: `test-vm`
- 2 CPUs
- 2GB RAM

Returns VM ID and state.

### Delete VM
```bash
NODE_IP=192.168.40.146 VM_ID=<uuid> devbox run delete-vm
```
Deletes a VM by ID.

**Example:**
```bash
NODE_IP=192.168.40.146 \
  VM_ID=5fc2b3d5-57e0-4991-bc1e-349ee5ec3784 \
  devbox run delete-vm
```

---

## 💾 Installation

### Write ISO to USB
```bash
USB_DEVICE=/dev/disk4 devbox run write-usb
```

**macOS:**
```bash
# List disks
diskutil list

# Unmount (if mounted)
diskutil unmountDisk /dev/disk4

# Write ISO
USB_DEVICE=/dev/disk4 devbox run write-usb
```

**Linux:**
```bash
# List disks
lsblk

# Write ISO
USB_DEVICE=/dev/sdb devbox run write-usb
```

**Safety Features:**
- Checks if ISO exists
- Confirms with user (type "YES")
- Shows progress during write

---

## 📚 Help & Documentation

### Show All Commands
```bash
devbox run help
```

### Documentation Files
- `QUICKSTART.md` - Installation guide
- `ARCHITECTURE.md` - System design and workflow
- `FIXES.md` - Troubleshooting and manual fixes
- `COMMANDS.md` - This file

---

## Common Workflows

### 1. Full Development Setup
```bash
# Enter devbox shell
devbox shell

# Generate protobuf
devbox run proto

# Build components
devbox run build-controller
devbox run build-node-agent

# Build ISO
devbox run build-iso
```

### 2. Deploy to New Node
```bash
# Write ISO to USB
USB_DEVICE=/dev/disk4 devbox run write-usb

# Boot node from USB, run install-to-disk, reboot

# Test connectivity
NODE_IP=192.168.40.146 devbox run test-node

# Deploy if needed (usually automatic after install-to-disk)
NODE_IP=192.168.40.146 devbox run deploy-node
```

### 3. VM Management
```bash
# Set node IP
export NODE_IP=192.168.40.146

# Create VM
devbox run create-vm
# Output: VM ID: 5fc2b3d5-57e0-4991-bc1e-349ee5ec3784

# List VMs (on node)
ssh root@$NODE_IP virsh list --all

# Delete VM
VM_ID=5fc2b3d5-57e0-4991-bc1e-349ee5ec3784 devbox run delete-vm
```

### 4. Update Existing Node
```bash
# Rebuild node-agent
devbox run build-node-agent

# Deploy to node
NODE_IP=192.168.40.146 devbox run deploy-node

# Restart service on node
ssh root@192.168.40.146 systemctl restart kcode-node-agent

# Verify
NODE_IP=192.168.40.146 devbox run test-node
```

---

## Environment Variables

| Variable | Required By | Description | Example |
|----------|-------------|-------------|---------|
| `NODE_IP` | Most node commands | IP address of KVM node | `192.168.40.146` |
| `VM_ID` | `delete-vm` | UUID of VM to delete | `5fc2b3d5-57e0-4991-bc1e-349ee5ec3784` |
| `USB_DEVICE` | `write-usb` | Device path for USB drive | `/dev/disk4` (macOS), `/dev/sdb` (Linux) |

---

## Tips

### Use Environment Variables
```bash
# Set once, use many times
export NODE_IP=192.168.40.146

devbox run test-node
devbox run create-vm
devbox run list-node-services
```

### Chain Commands
```bash
# Build and deploy in one go
devbox run build-node-agent && \
  NODE_IP=192.168.40.146 devbox run deploy-node
```

### Check Logs on Node
```bash
NODE_IP=192.168.40.146

# node-agent logs
ssh root@$NODE_IP journalctl -u kcode-node-agent -f

# libvirtd logs
ssh root@$NODE_IP journalctl -u libvirtd -f

# All kcore services
ssh root@$NODE_IP systemctl status kcode-node-agent libvirtd virtlogd
```

### Quick VM Status
```bash
# On your Mac
alias kcore-vms='ssh root@192.168.40.146 "virsh list --all"'

kcore-vms
```

---

## Troubleshooting Commands

### Node Not Responding
```bash
NODE_IP=192.168.40.146

# Test basic connectivity
ping $NODE_IP

# Test SSH
ssh root@$NODE_IP 'echo "SSH works"'

# Test TCP port
nc -zv $NODE_IP 9091

# Check services on node
ssh root@$NODE_IP systemctl status kcode-node-agent
```

### Certificate Issues
```bash
NODE_IP=192.168.40.146

# Check certs exist
ssh root@$NODE_IP 'ls -l /etc/kcode/'

# Verify cert details
ssh root@$NODE_IP 'openssl x509 -in /etc/kcode/node.crt -text -noout | grep -A2 "Subject:"'

# Redeploy certs
devbox run deploy-node
```

### Build Issues
```bash
# Clean build
nix-collect-garbage
devbox run build-iso

# Check Nix store
nix-store --verify --check-contents

# Rebuild everything
devbox run proto
devbox run build-controller
devbox run build-node-agent
```

---

## Advanced: Direct gRPC Calls

If you need more control than the devbox scripts provide:

```bash
# Custom VM specs
grpcurl -insecure \
  -cert ./certs/node.crt -key ./certs/node.key \
  -import-path ./proto -proto node.proto \
  -d '{
    "spec": {
      "id": "'$(uuidgen | tr '[:upper:]' '[:lower:]')'",
      "name": "custom-vm",
      "cpu": 8,
      "memory_bytes": 8589934592,
      "disks": [{
        "driver": "local-dir",
        "sizeBytes": 53687091200
      }]
    }
  }' \
  192.168.40.146:9091 kcore.node.NodeCompute/CreateVm

# List all methods
grpcurl -insecure \
  -cert ./certs/node.crt -key ./certs/node.key \
  192.168.40.146:9091 list kcore.node.NodeCompute

# Describe a method
grpcurl -insecure \
  -cert ./certs/node.crt -key ./certs/node.key \
  192.168.40.146:9091 describe kcore.node.NodeCompute.CreateVm
```

---

## Quick Reference Card

```bash
# Most Common Commands
devbox run help                                    # Show help
devbox run build-iso                              # Build ISO
USB_DEVICE=/dev/disk4 devbox run write-usb       # Write to USB
NODE_IP=192.168.40.146 devbox run test-node      # Test node
NODE_IP=192.168.40.146 devbox run create-vm      # Create VM
NODE_IP=192.168.40.146 devbox run deploy-node    # Update node

# Development
devbox run proto                                  # Generate code
devbox run build-controller                       # Build controller
devbox run build-node-agent                       # Build agent
devbox run start-controller                       # Start controller
```

