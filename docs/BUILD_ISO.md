# Building and Installing kcore NixOS Image

## Prerequisites

- Nix with flakes enabled (`nix --version` should show 2.4+)
- USB stick (8GB+ recommended)
- ThinkCentre hardware

## Building the ISO

### Option 1: Build on macOS using Podman (Recommended for macOS users)

Since Nix on macOS cannot cross-compile to Linux, use Podman to build in a Linux container:

```bash
./build-iso-podman.sh
```

This script will:
1. Pull a NixOS container image
2. Mount your kcore directory into the container
3. Build the ISO inside the Linux container
4. Output the ISO to `result-iso/iso/`

**Note:** Building in Podman on macOS uses emulation and will be slower (30-60 minutes). For faster builds, use Option 2.

### Option 2: Build on Linux (Fastest)

If you have access to a Linux machine:

```bash
# On Linux machine
cd /path/to/kcore
nix build '.#nixosConfigurations.kvm-node-iso.config.system.build.isoImage' -o result-iso
```

### Option 3: Build directly on macOS (Will fail - for reference only)

**This will NOT work** because Nix on macOS cannot cross-compile the node agent:

```bash
# This will fail with cross-compilation error
nix build '.#nixosConfigurations.kvm-node-iso.config.system.build.isoImage' -o result-iso
```

## Step 1: Build the ISO Image

Choose one of the options above. The ISO will be in `result-iso/iso/kcore-*.iso`

## Step 2: Write ISO to USB Stick

### On macOS:

```bash
# Find your USB device (usually /dev/disk2 or similar)
diskutil list

# Unmount the USB stick
diskutil unmountDisk /dev/diskX

# Write the ISO (replace /dev/diskX with your USB device, use rdiskX for faster writes)
sudo dd if=result-iso/iso/kcore-*.iso of=/dev/rdiskX bs=1m

# Eject when done
diskutil eject /dev/diskX
```

### On Linux:

```bash
# Find your USB device
lsblk

# Write the ISO (replace /dev/sdX with your USB device)
sudo dd if=result-iso/iso/kcore-*.iso of=/dev/sdX bs=4M status=progress oflag=sync
```

## Step 3: Boot and Install

1. Insert USB stick into ThinkCentre
2. Boot from USB (usually F12 or Del to access boot menu)
3. Boot into the NixOS installer
4. Follow the installation prompts
5. After installation, reboot into the installed system

## Step 4: Configure Node Agent

After installation, you need to:

1. **Copy TLS certificates** to `/etc/kcore/`:
   ```bash
   # On your macOS machine, copy certificates:
   scp certs/ca.crt certs/node.crt certs/node.key root@<thinkcentre-ip>:/etc/kcore/
   
   # Set correct permissions
   ssh root@<thinkcentre-ip>
   chmod 644 /etc/kcore/*.crt
   chmod 600 /etc/kcore/*.key
   ```

2. **Create node agent config**:
   ```bash
   # SSH into the ThinkCentre
   ssh root@<thinkcentre-ip>
   
   # Copy example config
   cp /etc/kcore/node-agent.yaml.example /etc/kcore/node-agent.yaml
   
   # Edit config with your controller IP
   nano /etc/kcore/node-agent.yaml
   # Update:
   # - nodeId: unique identifier for this node (e.g., "thinkcentre-01")
   # - controllerAddr: your macOS machine's IP:9090 (e.g., "192.168.1.100:9090")
   ```

3. **Start the node agent service**:
   ```bash
   systemctl enable kcore-node-agent
   systemctl start kcore-node-agent
   systemctl status kcore-node-agent
   ```

## Step 5: Verify Node Registration

On your controller (macOS), you should see logs like:

```
2025/11/07 19:00:13 Node registration request: ID=thinkcentre-01, Hostname=kvm-node, Address=192.168.1.X:9091
2025/11/07 19:00:13 Successfully registered node thinkcentre-01 (kvm-node)
```

## Troubleshooting

- **Node agent won't start**: Check `journalctl -u kcore-node-agent -n 50`
- **Can't connect to controller**: 
  - Verify firewall allows port 9090 on macOS
  - Check controller is running: `./bin/kcore-controller`
  - Verify network connectivity: `ping <controller-ip>`
- **TLS errors**: 
  - Ensure certificates are in `/etc/kcore/` with correct permissions (600 for keys, 644 for certs)
  - Verify certificate CN matches (node cert should have CN=kcore-node-01 or similar)
- **Network interface name**: The config assumes `enp1s0`. If your ThinkCentre uses a different interface name (check with `ip addr`), you'll need to update the flake before building:
  ```nix
  networking.interfaces.YOUR_INTERFACE.useDHCP = true;
  networking.bridges.br0.interfaces = [ "YOUR_INTERFACE" ];
  ```

## What's Included in the Image

- **kcore-node-agent**: Pre-compiled binary with libvirt support
- **KVM/QEMU**: Virtualization stack
- **libvirt**: VM management library
- **Network bridge**: Configured for br0
- **Example config**: `/etc/kcore/node-agent.yaml.example`
- **Systemd service**: Auto-starts node agent on boot (after config is set up)

