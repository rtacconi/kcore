# Building ISO on Remote Ubuntu Server

This document explains how to build the kcore ISO image on a remote Ubuntu server instead of locally on macOS.

## Why Build Remotely?

- **Native AMD64**: No emulation overhead
- **Faster**: Direct compilation without Podman/QEMU emulation
- **More reliable**: Avoids Go runtime crashes in emulated environments
- **Better resource usage**: Uses server's CPU and memory directly

## Prerequisites

1. **SSH access** to Ubuntu server (AMD64)
2. **SSH key** configured: `~/.ssh/id_ed25519_gmail`
3. **Nix** installed on remote server (script will install if missing)

## Usage

### Quick Start

```bash
./build-iso-remote.sh
```

The script will:
1. Test SSH connection
2. Install Nix if needed
3. Transfer project files to server
4. Build ISO natively on AMD64 Linux
5. Download ISO back to local machine

### Manual Steps (if script doesn't work)

1. **SSH to server:**
   ```bash
   ssh -i ~/.ssh/id_ed25519_gmail rtacconi@192.168.40.10
   ```

2. **Install Nix (if not installed):**
   ```bash
   sh <(curl -L https://nixos.org/nix/install) --daemon
   ```

3. **Enable flakes:**
   ```bash
   mkdir -p ~/.config/nix
   echo 'experimental-features = nix-command flakes' >> ~/.config/nix/nix.conf
   ```

4. **Clone/build:**
   ```bash
   git clone <your-repo-url> kcore-build
   cd kcore-build
   export NIXPKGS_ALLOW_UNFREE=1
   nix --extra-experimental-features nix-command \
       --extra-experimental-features flakes \
       build '.#nixosConfigurations.kvm-node-iso.config.system.build.isoImage' \
       -o result-iso
   ```

5. **Download ISO:**
   ```bash
   # On local machine
   scp -i ~/.ssh/id_ed25519_gmail \
       rtacconi@192.168.40.10:~/kcore-build/result-iso/*.iso \
       ./kcore.iso
   ```

## Configuration

Edit `build-iso-remote.sh` to change:
- `SSH_HOST`: Remote server address
- `SSH_KEY`: Path to SSH private key
- `REMOTE_DIR`: Directory on remote server for build

## Troubleshooting

### SSH Connection Issues

```bash
# Test connection
ssh -i ~/.ssh/id_ed25519_gmail rtacconi@192.168.40.10 "echo 'OK'"

# Check SSH key permissions
chmod 600 ~/.ssh/id_ed25519_gmail
```

### Nix Installation Issues

If Nix installation fails, install manually:
```bash
ssh -i ~/.ssh/id_ed25519_gmail rtacconi@192.168.40.10
# Then follow Nix installation instructions
```

### Build Fails

Check remote logs:
```bash
ssh -i ~/.ssh/id_ed25519_gmail rtacconi@192.168.40.10 \
    "tail -100 ~/kcore-build/build.log"
```

### Disk Space

Check available space on remote:
```bash
ssh -i ~/.ssh/id_ed25519_gmail rtacconi@192.168.40.10 "df -h"
```

ISO build requires ~10-20GB free space.

## Advantages Over macOS Build

| Aspect | macOS (Podman) | Remote Ubuntu |
|--------|----------------|---------------|
| Architecture | ARM64 → AMD64 emulation | Native AMD64 |
| Speed | Slow (emulation) | Fast (native) |
| Reliability | Go runtime crashes | Stable |
| Resource Usage | High (emulation overhead) | Efficient |
| Build Time | 60-90+ minutes | 30-45 minutes |

