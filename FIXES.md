# KCORE Manual Fixes Applied During Debugging

## Issue: install-to-disk not working, ISO boot failure, VM creation requiring manual steps

### 1. ISO Networking Configuration (Stage 1 Boot Failure)
**Problem:** ISO failed at stage 1 with squashfs mount errors. System couldn't reach stage 3 login.

**Root Cause:** Bridge configuration (`br0`) and specific interface (`enp1s0`) in ISO caused initrd to fail.

**Fix Applied:**
- Changed `networking.useDHCP = false` → `networking.useDHCP = true` (auto-detect all interfaces)
- Removed `networking.bridges.br0.interfaces = [ "enp1s0" ];`
- Simplified firewall to just SSH and node-agent ports

**Location:** `flake.nix` lines 229-233 (ISO configuration)

---

### 2. Missing parted Binary in ISO
**Problem:** `install-to-disk` script failed with "parted: command not found"

**Fix Applied:**
- Added `parted` to ISO's `environment.systemPackages`

**Location:** `flake.nix` line 289

---

### 3. install-to-disk Script - Device Busy Errors
**Problem:** Script failed with "device is busy" when trying to wipe disk with existing LVM volumes or mounted partitions

**Fix Applied:**
- Added LVM volume group deactivation before disk operations
- Added unmounting of existing partitions on target disk
- Added retries for wipefs operations

**Location:** `flake.nix` lines 327-341 (install-to-disk script)

---

### 4. Installed System - libvirtd Not Enabled
**Problem:** After installation, `libvirtd` was not running, causing node-agent to fail with:
```
Failed to connect socket to '/var/run/libvirt/virtqemud-sock': No such file or directory
```

**Fix Applied:**
- Changed `virtualisation.libvirtd.enable = false;` → `true` in installed system configuration
- Added `qemu.runAsRoot = true` for proper VM management
- virtlogd is automatically managed by libvirtd service

**Location:** `flake.nix` lines 392-396 (embedded configuration.nix)

---

### 5. Installed System - Missing Packages
**Problem:** Installed system missing tools needed for maintenance and debugging

**Fix Applied:**
- Added `parted` to installed system packages (for disk operations)
- Kept `lvm2` (for LVM management)

**Location:** `flake.nix` line 398 (environment.systemPackages in installed config)

---

### 6. Installed System - SSH Authorized Keys
**Problem:** Manual SSH key setup required after installation

**Fix Applied:**
- Added support for authorized SSH keys via `users.users.root.openssh.authorizedKeys.keys`
- Keys can be customized before building ISO or added via cloud-init/firstboot script

**Location:** `flake.nix` line 387 (users.users.root configuration)

---

### 7. gRPC TLS Certificate IP SANs
**Problem:** Certificates didn't include IP addresses in Subject Alternative Names, causing:
```
x509: cannot validate certificate for 192.168.40.146 because it doesn't contain any IP SANs
```

**Workaround:** Use `-insecure` flag with grpcurl for now
**Proper Fix:** Regenerate certificates with IP SANs (requires updating cert generation scripts)

---

## Verification Steps After Fresh Install

After installing from USB with these fixes:

1. **System should boot automatically**
   ```bash
   # No manual intervention needed at stage 1-3
   ```

2. **SSH should be accessible**
   ```bash
   ssh root@<node-ip>  # password: kcore
   ```

3. **libvirtd should be running**
   ```bash
   systemctl status libvirtd
   virsh version
   ```

4. **Node-agent can be started**
   ```bash
   # After copying node-agent and config
   /root/node-agent-bin/bin/node-agent &
   ```

5. **VMs can be created via gRPC**
   ```bash
   grpcurl -insecure -cert ./certs/node.crt -key ./certs/node.key \
     -import-path ./proto -proto node.proto \
     -d '{"spec": {"id": "'$(uuidgen | tr '[:upper:]' '[:lower:]')'", "name": "test-vm", "cpu": 2, "memory_bytes": 2147483648}}' \
     <node-ip>:9091 kcore.node.NodeCompute/CreateVm
   ```

---

## Files Modified
- `flake.nix` - ISO and installed system configuration
- `FIXES.md` (this file) - Documentation of all fixes

## Testing Checklist
- [ ] ISO builds successfully
- [ ] ISO boots to stage 3 login without errors
- [ ] install-to-disk completes without "device busy" errors
- [ ] Installed system boots with libvirtd running
- [ ] SSH access works with password
- [ ] node-agent connects to libvirtd successfully
- [ ] VMs can be created via gRPC API

