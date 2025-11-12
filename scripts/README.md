# kcore Scripts

Automation scripts for kcore operations.

For complete documentation, see [docs/scripts.md](../docs/scripts.md).

## Quick Reference

### Available Scripts

- `build-iso.sh` - Build bootable kcore ISO
- `create-vm.sh` - Create VM on kcore node
- `delete-vm.sh` - Delete VM from kcore node
- `test-node.sh` - Test node connectivity
- `list-services.sh` - List gRPC services
- `deploy-node.sh` - Deploy node-agent to node
- `write-usb.sh` - Write ISO to USB drive

### Usage

**Via Make (Recommended):**
```bash
make build-iso
NODE_IP=192.168.40.146 make create-vm
```

**Direct Execution:**
```bash
./scripts/build-iso.sh
NODE_IP=192.168.40.146 ./scripts/create-vm.sh
```

For full documentation, standards, and examples, see:
- **[docs/scripts.md](../docs/scripts.md)** - Complete scripts documentation
- **[docs/COMMANDS.md](../docs/COMMANDS.md)** - All commands reference
- **[README.md](../README.md)** - Main project documentation
