# KCORE Quick Start Guide

## From USB to Running VM - Fully Automated

This guide covers the complete process from creating a bootable USB to managing VMs via the kcore API.

---

## Prerequisites

- USB drive (4GB+)
- Target machine with KVM support (Intel VT-x or AMD-V)
- macOS or Linux workstation for controller

---

## Step 1: Build the ISO

```bash
cd /path/to/kcore
nix build '.#nixosConfigurations.kvm-node-iso.config.system.build.isoImage' \
  --extra-experimental-features "nix-command flakes"

# ISO will be in: result/iso/
```

---

## Step 2: Write to USB

```bash
# macOS
sudo dd if=result/iso/*.iso of=/dev/rdiskX bs=4m status=progress

# Linux
sudo dd if=result/iso/*.iso of=/dev/sdX bs=4M status=progress oflag=sync
```

---

## Step 3: (Optional) Add Your SSH Key

Before booting the USB, you can add your SSH public key for passwordless access:

```bash
# Mount the USB
# On macOS: it will auto-mount
# On Linux: mount /dev/sdX1 /mnt

# Add your key to the live ISO (if you can modify the ISO filesystem)
# Or, boot the USB and add your key manually:
ssh root@<node-ip>  # password: kcore
mkdir -p /root/.ssh
echo "your-ssh-public-key" > /root/.ssh/authorized_keys
```

When you run `install-to-disk`, it will automatically copy these keys to the installed system.

---

## Step 4: Boot from USB

1. Insert USB into target machine
2. Boot from USB (usually F12 or DEL to access boot menu)
3. System will boot to a login prompt
4. Login: `root` / Password: `kcore`
5. Network will automatically get an IP via DHCP

Check your IP:
```bash
ip addr show
```

---

## Step 5: Install to Disk

SSH into the node (or use the console):

```bash
ssh root@<node-ip>  # password: kcore
```

Run the automated installer:

```bash
install-to-disk
```

The installer will:
- Show available disks
- Ask you to select a disk (e.g., `sda`, `nvme0n1`, `vda`)
- Confirm the operation
- Automatically:
  - Deactivate any LVM volumes
  - Unmount existing partitions
  - Wipe the disk
  - Create GPT partitions (EFI + root)
  - Format filesystems
  - Install NixOS with:
    - libvirtd enabled and running
    - virtlogd enabled and running
    - SSH enabled (port 22)
    - node-agent port open (9091)
    - All required packages (qemu, libvirt, lvm2, parted)
    - Your SSH keys (if present on live ISO)

After installation completes:
```bash
reboot
```

Remove the USB drive when the system restarts.

---

## Step 6: Setup node-agent

After the system boots from the installed disk:

```bash
# SSH into the node
ssh root@<node-ip>  # password: kcore (or use your SSH key)

# Verify libvirtd is running
systemctl status libvirtd
virsh version

# Copy node-agent binary and config
# (Transfer from your workstation or build on the node)
mkdir -p /root/node-agent-bin/bin /etc/kcode
scp /path/to/node-agent root@<node-ip>:/root/node-agent-bin/bin/
scp /path/to/certs/*.{crt,key} root@<node-ip>:/etc/kcode/
scp /path/to/node-agent.yaml root@<node-ip>:/etc/kcode/

# Start node-agent
cd /root/node-agent-bin/bin
./node-agent > /tmp/node-agent.log 2>&1 &
```

---

## Step 7: Create VMs from Controller

On your Mac/workstation:

```bash
cd /path/to/kcore

# Using grpcurl with nix-shell
VM_ID=$(uuidgen | tr '[:upper:]' '[:lower:]')

nix-shell -p grpcurl --run "grpcurl -insecure \
  -cert ./certs/node.crt -key ./certs/node.key \
  -import-path ./proto -proto node.proto \
  -d '{\"spec\": {\"id\": \"'$VM_ID'\", \"name\": \"my-vm\", \"cpu\": 4, \"memory_bytes\": 4294967296}}' \
  <node-ip>:9091 kcore.node.NodeCompute/CreateVm"
```

Expected output:
```json
{
  "status": {
    "id": "38551bb2-3838-4c04-933b-99465acf34cb",
    "state": "VM_STATE_RUNNING"
  }
}
```

---

## Step 8: Verify VM

SSH into the node and check:

```bash
ssh root@<node-ip>

# List all VMs
virsh list --all

# Get VM details
virsh dominfo <vm-name>

# Delete a VM (via API)
# On your workstation:
nix-shell -p grpcurl --run "grpcurl -insecure \
  -cert ./certs/node.crt -key ./certs/node.key \
  -import-path ./proto -proto node.proto \
  -d '{\"id\": \"'$VM_ID'\"}' \
  <node-ip>:9091 kcore.node.NodeCompute/DeleteVm"
```

---

## Key Features

✅ **Fully Automated Installation**
- No manual package installation
- No manual service configuration
- libvirtd and virtlogd start automatically

✅ **Hardware Auto-Detection**
- DHCP on all interfaces
- Automatic network configuration
- Works with any NIC (no hardcoded interface names)

✅ **SSH Key Support**
- Copy keys to live ISO's `/root/.ssh/authorized_keys`
- Automatically transferred to installed system

✅ **Robust Disk Installation**
- Handles existing LVM volumes
- Unmounts busy partitions
- Retries failed operations

✅ **Ready for Production**
- libvirtd enabled and running
- Firewall configured (SSH + node-agent)
- All dependencies installed

---

## Troubleshooting

### ISO doesn't boot
- Verify USB was written correctly: `diskutil list` (macOS) or `lsblk` (Linux)
- Try booting in UEFI mode (not legacy BIOS)

### install-to-disk fails with "device busy"
- The script now handles this automatically by deactivating LVM and unmounting partitions
- If it still fails, manually run:
  ```bash
  vgchange -an pve  # or your VG name
  umount /dev/sda* # or your disk
  ```

### node-agent fails to start
- Check if libvirtd is running: `systemctl status libvirtd`
- Check if virtlogd is running: `systemctl status virtlogd`
- If not, start them manually: `systemctl start libvirtd virtlogd`

### VM creation fails
- Ensure libvirtd and virtlogd are running
- Check node-agent logs: `tail -f /tmp/node-agent.log`
- Verify certificates are correct
- Use `-insecure` flag if certificates don't have IP SANs

### Certificate errors
- The certificates may not include IP addresses in Subject Alternative Names
- Use `-insecure` flag with grpcurl for now
- TODO: Update certificate generation to include IP SANs

---

## What's Next?

1. **Controller Service**: Run the kcore controller on your Mac to manage multiple nodes
2. **VM Templates**: Create VM images and templates
3. **Storage Classes**: Configure storage drivers (local-lvm, ceph, etc.)
4. **Networking**: Setup bridge networking for VMs
5. **High Availability**: Add multiple nodes for HA

See `examples/` directory for sample configurations.

---

## Files Changed in This Release

- `flake.nix`: ISO and installed system configuration
- `FIXES.md`: Detailed list of all fixes applied
- `QUICKSTART.md`: This file

**Commit**: Fix ISO boot and automate VM creation setup
**Date**: 2025-11-12
**Pushed to**: https://github.com/rtacconi/kcore

