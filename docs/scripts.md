# kcore Scripts Directory

All automation scripts for kcore operations.

## Architecture

```
User Commands
     ↓
  devbox (environment + shortcuts)
     ↓
  Makefile (build system)
     ↓
  scripts/ (implementation)
```

## Scripts

### ISO Building
- **`build-iso.sh`** - Build bootable kcore ISO with all components

### Node Management  
- **`test-node.sh`** - Test kcore node connectivity and gRPC services
- **`list-services.sh`** - List available gRPC services on node
- **`deploy-node.sh`** - Deploy node-agent and certs to node (for updates)

### VM Operations
- **`create-vm.sh`** - Create test VM on kcore node
- **`delete-vm.sh`** - Delete VM from kcore node

### Installation
- **`write-usb.sh`** - Write kcore ISO to USB drive (macOS + Linux)

## Usage

### Via Make (Recommended)
```bash
make build-iso
NODE_IP=192.168.40.146 make create-vm
USB_DEVICE=/dev/disk4 make write-usb
```

### Via devbox
```bash
devbox run build-iso
NODE_IP=192.168.40.146 devbox run create-vm
```

### Direct Execution
```bash
./scripts/build-iso.sh
NODE_IP=192.168.40.146 ./scripts/create-vm.sh
```

## Script Standards

All scripts follow these standards:

1. **Shebang**: `#!/usr/bin/env bash`
2. **Safety**: `set -euo pipefail`
3. **Help**: Usage instructions and examples
4. **Validation**: Check required environment variables
5. **Feedback**: Clear progress indicators (🔨, ✅, ❌)
6. **Error handling**: Meaningful error messages
7. **Documentation**: Inline comments for complex operations

## Environment Variables

| Variable | Used By | Description |
|----------|---------|-------------|
| `NODE_IP` | Most node scripts | IP address of kcore node |
| `VM_ID` | `delete-vm.sh` | UUID of VM to delete |
| `USB_DEVICE` | `write-usb.sh` | Device path for USB drive |

## Adding New Scripts

When adding new scripts:

1. Create script in `scripts/` directory
2. Add shebang and safety flags
3. Make executable: `chmod +x scripts/your-script.sh`
4. Add target to `Makefile`
5. Add shortcut to `devbox.json`
6. Document in [COMMANDS.md](COMMANDS.md)
7. Update this file

Example Makefile target:
```makefile
# Your new feature
# Usage: make your-feature
your-feature:
	@./scripts/your-script.sh
```

Example devbox shortcut:
```json
{
  "scripts": {
    "your-feature": "make your-feature"
  }
}
```

## Testing Scripts

Test scripts manually before committing:

```bash
# Test with required env vars
NODE_IP=192.168.40.146 ./scripts/test-node.sh

# Test error handling (missing env var)
./scripts/test-node.sh
# Should show: ❌ Error: NODE_IP not set

# Test via Make
make test-node  # Should also show error

# Test via devbox
devbox run test-node  # Should also show error
```

## Maintenance

- Keep scripts focused on single responsibility
- Extract common functions if needed
- Update documentation when changing behavior
- Test on both macOS and Linux when applicable
- Use kcore terminology in output (not NixOS)

