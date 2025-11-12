# kctl Quick Reference

All `kctl` commands are now fully working! 🎉

## Configuration

Create `~/.kcore/config`:
```yaml
current-context: dev-node

contexts:
  dev-node:
    controller: "192.168.40.146:9091"
    insecure: true
```

## Working Commands

### List VMs
```bash
./bin/kctl get vms
```
Shows full UUIDs for easy copy-paste:
```
ID                                    NAME                 STATUS       CPU    MEMORY    
0981839e-0f0e-4c7f-bfe0-35bc00167a8a  test-vm-1762987501   RUNNING      2      4.0 GB    
```

### Create VM
```bash
./bin/kctl create vm my-vm --cpu 2 --memory 4G --disk 50G
```

Example output:
```
✅ VM 'my-vm' created successfully
  ID: 660030eb-2c56-4b38-9673-5924ed3a0374
  Status: VM_STATE_RUNNING
  CPU: 2 cores
  Memory: 4G (4.0 GB)
```

### Describe VM
```bash
./bin/kctl describe vm 660030eb-2c56-4b38-9673-5924ed3a0374
```

Shows complete VM details:
```
Name:           my-vm
ID:             660030eb-2c56-4b38-9673-5924ed3a0374
Status:         VM_STATE_RUNNING

Resources:
  CPU:          2 cores
  Memory:       4.0 GB

Disks:          (none)
Network:        (none)
```

### Delete VM
```bash
./bin/kctl delete vm 660030eb-2c56-4b38-9673-5924ed3a0374 --force
```

Output:
```
Deleting VM '660030eb-2c56-4b38-9673-5924ed3a0374'...
✅ VM '660030eb-2c56-4b38-9673-5924ed3a0374' deleted successfully
```

## Complete Workflow Test

```bash
# 1. Create
./bin/kctl create vm test-vm --cpu 1 --memory 512M

# 2. List (copy UUID)
./bin/kctl get vms

# 3. Describe
./bin/kctl describe vm <UUID>

# 4. Delete
./bin/kctl delete vm <UUID> --force

# 5. Verify
./bin/kctl get vms  # VM should be gone
```

## Development

### Deploy Updated Node Agent (Non-blocking!)
```bash
# Using make
make deploy-node-agent NODE_HOST=root@192.168.40.146

# Using devbox
devbox run deploy-node-agent

# Direct script
./scripts/deploy-node-agent.sh root@192.168.40.146 /tmp/kcore-node-agent-fixed
```

### Build kctl
```bash
make kctl
# Or
devbox run build-kctl
```

## Node Agent

The node agent now creates **persistent VMs** using `DomainDefineXML` instead of transient ones.

VMs survive:
- ✅ Node reboots
- ✅ Agent restarts  
- ✅ libvirtd restarts

## Verified Working

- ✅ `kctl get vms` - Full UUID display
- ✅ `kctl create vm` - Creates persistent VMs
- ✅ `kctl describe vm <UUID>` - Shows complete info
- ✅ `kctl delete vm <UUID>` - Deletes from libvirt
- ✅ Non-blocking agent deployment

All operations verified with libvirt directly!
