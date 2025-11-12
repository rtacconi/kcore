# kcore

A modern, minimal virtualization platform for datacenters and home labs.

## 🚀 Quick Start

```bash
# Build the ISO
make build-iso

# Write to USB
USB_DEVICE=/dev/disk4 make write-usb

# Boot, login (root/kcore), run:
install-to-disk

# After reboot, manage VMs with kctl
kctl create vm web-server --cpu 4 --memory 8G
kctl get vms
kctl describe vm web-server
```

**That's it!** Everything else is automated.

---

## 📚 Documentation

### Getting Started
- **[Quick Start Guide](docs/QUICKSTART.md)** - Complete installation walkthrough from USB to running VMs
- **[Introduction](docs/intro.md)** - Project overview, architecture, and manual setup

### User Guides
- **[kctl CLI](docs/KCTL.md)** - User-friendly CLI for managing VMs and resources
- **[Commands Reference](docs/COMMANDS.md)** - All make/devbox commands with examples
- **[Architecture](docs/ARCHITECTURE.md)** - System design, workflows, and component communication

### Operations
- **[Fixes & Troubleshooting](docs/FIXES.md)** - Common issues and solutions
- **[Scripts](docs/scripts.md)** - Automation scripts documentation

### Development
- **[Building ISO](docs/BUILD_ISO.md)** - How to build the kcore ISO
- **[Building Node Agent](docs/BUILD_NODE_AGENT.md)** - How to build node-agent
- **[Cross Compilation](docs/CROSS_COMPILATION.md)** - Cross-compiling node-agent for Linux
- **[Project Structure](docs/PROJECT_STRUCTURE.md)** - Codebase organization

### Additional Resources
- **[Build ISO Remote](docs/BUILD_ISO_REMOTE.md)** - Remote ISO building
- **[Write USB](docs/WRITE_USB.md)** - USB drive preparation
- **[Next Steps](docs/NEXT_STEPS.md)** - Future development roadmap
- **[Step 1 Status](docs/STEP1_STATUS.md)** - Initial development status

---

## 🎯 What is kcore?

kcore is a clustered KVM virtualization platform that provides:

- **Simple VM Management** - Create, manage, and delete VMs via API
- **Automated Deployment** - Boot from USB, run one command, reboot
- **Distributed Architecture** - Controller manages multiple compute nodes
- **Modern Stack** - Go + gRPC + NixOS + KVM/libvirt

### Key Components

- **Controller** - Runs on your Mac/workstation, manages cluster state
- **Node Agent** - Runs on each compute node, manages local VMs
- **kcore ISO** - Bootable installer with everything included

---

## 💻 Development

### Prerequisites

- **Nix** with flakes enabled
- **Go 1.22+**
- **Make**
- **Devbox** (optional but recommended)

### Setup

```bash
# Clone repository
git clone https://github.com/rtacconi/kcore.git
cd kcore

# Enter devbox shell (sets up environment)
devbox shell

# Generate protobuf code
make proto

# Build components
make controller
make node-agent-nix
make build-iso
```

### Available Commands

```bash
make help                           # Show all commands
make proto                          # Generate protobuf code
make controller                     # Build controller
make kctl                           # Build kctl CLI
make node-agent-nix                 # Build node-agent
make build-iso                      # Build kcore ISO
NODE_IP=x.x.x.x make create-vm     # Create VM
NODE_IP=x.x.x.x make test-node     # Test node
```

See [Commands Reference](docs/COMMANDS.md) for full list.

---

## 🏗️ Architecture

```
┌─────────────────────┐
│   Your Mac/Linux    │
│  kcore-controller   │  ← Manages cluster state
│   (port 9090)       │     SQLite database
└──────────┬──────────┘
           │ gRPC (mTLS)
    ┌──────┴───────┬──────────────┐
    │              │              │
┌───▼────────┐ ┌──▼─────────┐ ┌──▼─────────┐
│  kcore     │ │  kcore     │ │  kcore     │
│  Node #1   │ │  Node #2   │ │  Node #N   │
│            │ │            │ │            │
│ node-agent │ │ node-agent │ │ node-agent │
│ (port 9091)│ │ (port 9091)│ │ (port 9091)│
│            │ │            │ │            │
│ libvirtd   │ │ libvirtd   │ │ libvirtd   │
│  VMs...    │ │  VMs...    │ │  VMs...    │
└────────────┘ └────────────┘ └────────────┘
```

- **Controller** receives node registrations and sends VM management commands
- **Node Agents** self-register with controller and manage local VMs via libvirtd
- **Communication** via gRPC with mutual TLS authentication

See [Architecture](docs/ARCHITECTURE.md) for detailed design.

---

## 🌟 Features

✅ **Fully Automated**
- Boot from USB → Run `install-to-disk` → Reboot
- All services start automatically
- Node auto-registers with controller

✅ **Hardware Auto-Detection**
- DHCP on all interfaces
- Works with any NIC
- No manual configuration

✅ **Production Ready**
- libvirtd + virtlogd managed by systemd
- Automatic service restarts
- Comprehensive logging

✅ **Developer Friendly**
- Make-based build system
- Devbox environment management
- Scripts in separate files (not inline)

---

## 📖 Common Tasks

### Build and Deploy

```bash
# Build ISO
make build-iso

# Write to USB
USB_DEVICE=/dev/disk4 make write-usb

# Boot node from USB, then:
ssh root@<node-ip>  # password: kcore
install-to-disk
reboot
```

### VM Management

```bash
# Create VM
kctl create vm web-server --cpu 4 --memory 8G --disk 100G

# List VMs
kctl get vms

# Get VM details
kctl describe vm web-server

# Delete VM
kctl delete vm web-server
```

### Update Node

```bash
# Rebuild node-agent
make node-agent-nix

# Deploy to node
NODE_IP=192.168.40.146 make deploy-node

# Restart service
ssh root@192.168.40.146 systemctl restart kcode-node-agent
```

---

## 🔧 Project Structure

```
kcore/
├── cmd/
│   ├── controller/      # Controller binary
│   └── node-agent/      # Node agent binary
├── pkg/
│   ├── config/          # Configuration parsing
│   ├── controller/      # Controller logic
│   └── sqlite/          # Database
├── node/
│   ├── libvirt/         # Libvirt integration
│   ├── storage/         # Storage drivers
│   └── server.go        # gRPC server
├── api/                 # Generated protobuf code
├── proto/               # Protobuf definitions
├── modules/             # NixOS modules
├── scripts/             # Automation scripts
├── docs/                # Documentation
└── examples/            # Example configs
```

See [Project Structure](docs/PROJECT_STRUCTURE.md) for details.

---

## 🤝 Contributing

Contributions welcome! Please:

1. Read the documentation in `docs/`
2. Follow the script standards in `docs/scripts.md`
3. Test changes on both macOS and Linux where applicable
4. Use kcore branding in user-facing text

---

## 📝 License

[Add your license here]

---

## 🔗 Links

- **Repository**: https://github.com/rtacconi/kcore
- **Issues**: https://github.com/rtacconi/kcore/issues
- **Documentation**: [docs/](docs/)

---

## 📞 Support

For help:
- Read [Quick Start Guide](docs/QUICKSTART.md)
- Check [Fixes & Troubleshooting](docs/FIXES.md)
- See [Commands Reference](docs/COMMANDS.md)
- Review [Architecture](docs/ARCHITECTURE.md)

