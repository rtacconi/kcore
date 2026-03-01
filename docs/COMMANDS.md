# KCORE Command Reference

All commands are available via Make. Run `make help` for a quick overview.

**Note:** You can also use devbox shortcuts: `devbox run <command>` which calls the corresponding make target.

---

## 📦 Build Commands

### Generate Protobuf Code
```bash
make proto
```
Generates Go code from `.proto` files in `api/`.  
Script: `scripts/` (inline in Makefile)

### Build Node Agent
```bash
make node-agent-nix
```
Builds `kcore-node-agent` for kcore nodes using Nix (recommended).  
Script: `scripts/` (inline in Makefile)

**Note:** The node-agent is automatically included in the kcore ISO. This build is only needed for updates/development.

### Run Go Tests (without global Go install)
```bash
nix --extra-experimental-features 'nix-command flakes' shell nixpkgs#go -c go test ./node
```
Use this form instead of plain `go test` on hosts without global Go installed.

### Build Controller
```bash
make controller
```
Builds `controller` binary for your current platform.  
Script: `scripts/` (inline in Makefile)

### Build ISO
```bash
make build-iso
```
Builds the bootable kcore ISO with node-agent embedded.
- Takes 10-30 minutes
- Output: `result/iso/*.iso`
- Size: ~1GB  
- Includes: libvirtd, virtlogd, kcore-node-agent, all dependencies  
Script: `scripts/build-iso.sh`

---

## 🚀 Running Services

### Start Controller
```bash
make start-controller
# Or: ./controller -config examples/controller.yaml
```
Starts the kcore controller using `examples/controller.yaml` config.

**Prerequisites:**
- Controller binary built (`make controller`)
- `examples/controller.yaml` configured with correct paths
- Certificates in `certs/` directory

---

## ☁️ Node Management

All node management commands require `NODE_IP` environment variable:

### Test Node Connectivity
```bash
NODE_IP=192.168.40.146 make test-node
```
Tests:
- TCP connection to port 9091
- gRPC service availability
- Certificate authentication  
Script: `scripts/test-node.sh`

### List Node Services
```bash
NODE_IP=192.168.40.146 make list-services
```
Lists all available gRPC services on the kcore node.  
Script: `scripts/list-services.sh`

### Deploy Node Agent
```bash
NODE_IP=192.168.40.146 make deploy-node
```
Deploys to an existing kcore node (for updates):
- Copies node-agent binary to `/opt/kcore/bin/`
- Copies certificates to `/etc/kcore/`
- Copies `node-agent.yaml` config

**Note:** This is for updating nodes. Fresh installs get node-agent automatically from the ISO.  
Script: `scripts/deploy-node.sh`

### Create VM
```bash
NODE_IP=192.168.40.146 make create-vm
```
Creates a test VM on kcore node with:
- Random UUID
- Name: `test-vm`
- 2 CPUs
- 2GB RAM

Returns VM ID and state.  
Script: `scripts/create-vm.sh`

### Delete VM
```bash
NODE_IP=192.168.40.146 VM_ID=<uuid> make delete-vm
```
Deletes a VM by ID from kcore node.  
Script: `scripts/delete-vm.sh`

**Example:**
```bash
NODE_IP=192.168.40.146 \
  VM_ID=5fc2b3d5-57e0-4991-bc1e-349ee5ec3784 \
  make delete-vm
```

---

## 💾 Installation

### Write ISO to USB
```bash
USB_DEVICE=/dev/disk4 make write-usb
```
Writes the kcore ISO to a USB drive for installation.  
Script: `scripts/write-usb.sh`

**macOS:**
```bash
# List disks
diskutil list

# Unmount (if mounted)
diskutil unmountDisk /dev/disk4

# Write ISO
USB_DEVICE=/dev/disk4 make write-usb
```

**Linux:**
```bash
# List disks
lsblk

# Write ISO
USB_DEVICE=/dev/sdb make write-usb
```

**Safety Features:**
- Checks if ISO exists
- Confirms with user (type "YES")
- Shows progress during write

---

## 📚 Help & Documentation

### Show All Commands
```bash
make help
```

### Documentation Files
- [QUICKSTART.md](QUICKSTART.md) - Installation guide
- [ARCHITECTURE.md](ARCHITECTURE.md) - System design and workflow
- [FIXES.md](FIXES.md) - Troubleshooting and manual fixes
- [COMMANDS.md](COMMANDS.md) - This file
- [scripts.md](scripts.md) - Scripts documentation

---

## Common Workflows

### 1. Full Development Setup
```bash
# Enter devbox shell (sets up environment)
devbox shell

# Generate protobuf
make proto

# Build components
make controller
make node-agent-nix

# Build ISO
make build-iso
```

### 2. Deploy New kcore Node
```bash
# Write ISO to USB
USB_DEVICE=/dev/disk4 make write-usb

# Boot node from USB, login (root/kcore), run:
install-to-disk

# After reboot, test connectivity
NODE_IP=192.168.40.146 make test-node
```

### 3. VM Management
```bash
# Set node IP
export NODE_IP=192.168.40.146

# Create VM
make create-vm
# Output: VM ID: 5fc2b3d5-57e0-4991-bc1e-349ee5ec3784

# List VMs on kcore node
ssh root@$NODE_IP virsh list --all

# Delete VM
VM_ID=5fc2b3d5-57e0-4991-bc1e-349ee5ec3784 make delete-vm
```

### 3b. Cloud Image Login Policy
```bash
# Secure default (no known guest password injected)
kctl create vm ubuntu-secure \
  --cpu 2 --memory 4G --disk 20G \
  --image https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img

# Explicitly enable lab credentials for console/SSH
kctl create vm ubuntu-lab \
  --cpu 2 --memory 4G --disk 20G \
  --image https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img \
  --enable-kcore-login
```

When `--enable-kcore-login` is used, cloud-init adds known credentials:
- `kcore/kcore`
- distro default user (`ubuntu` for Ubuntu images, `debian` for Debian images)

### 4. Update Existing kcore Node
```bash
# Rebuild node-agent
make node-agent-nix

# Deploy to node
NODE_IP=192.168.40.146 make deploy-node

# Restart service on kcore node
ssh root@192.168.40.146 systemctl restart kcore-node-agent

# Verify
NODE_IP=192.168.40.146 make test-node
```

---

## Environment Variables

| Variable | Required By | Description | Example |
|----------|-------------|-------------|---------|
| `NODE_IP` | Most node commands | IP address of kcore node | `192.168.40.146` |
| `VM_ID` | `delete-vm` | UUID of VM to delete | `5fc2b3d5-57e0-4991-bc1e-349ee5ec3784` |
| `USB_DEVICE` | `write-usb` | Device path for USB drive | `/dev/disk4` (macOS), `/dev/sdb` (Linux) |

---

## Tips

### Use Environment Variables
```bash
# Set once, use many times
export NODE_IP=192.168.40.146

make test-node
make create-vm
make list-services
```

### Chain Commands
```bash
# Build and deploy in one go
make node-agent-nix && \
  NODE_IP=192.168.40.146 make deploy-node
```

### Check Logs on kcore Node
```bash
NODE_IP=192.168.40.146

# node-agent logs
ssh root@$NODE_IP journalctl -u kcore-node-agent -f

# libvirtd logs
ssh root@$NODE_IP journalctl -u libvirtd -f

# All kcore services
ssh root@$NODE_IP systemctl status kcore-node-agent libvirtd virtlogd
```

### Quick VM Status
```bash
# On your Mac/workstation
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
ssh root@$NODE_IP systemctl status kcore-node-agent
```

### Certificate Issues
```bash
NODE_IP=192.168.40.146

# Check certs exist
ssh root@$NODE_IP 'ls -l /etc/kcore/'

# Verify cert details
ssh root@$NODE_IP 'openssl x509 -in /etc/kcore/node.crt -text -noout | grep -A2 "Subject:"'

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
make help                                    # Show help
make build-iso                              # Build ISO
USB_DEVICE=/dev/disk4 make write-usb       # Write to USB
NODE_IP=192.168.40.146 make test-node      # Test node
NODE_IP=192.168.40.146 make create-vm      # Create VM
NODE_IP=192.168.40.146 make deploy-node    # Update node

# Development
make proto                                  # Generate code
make controller                             # Build controller
make node-agent-nix                         # Build agent
./controller -config examples/controller.yaml  # Start controller
```

