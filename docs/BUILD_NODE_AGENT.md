# Building Node Agent

## Current Limitation

**Podman on macOS cannot reliably cross-compile the node-agent** due to:
1. Limited x86_64 emulation support
2. Go compiler crashes in emulated environment
3. CGO requires native Linux environment

## ✅ Recommended Solution: Build on Linux

Since you have ThinkCentre nodes running Linux, build directly on one of them:

```bash
# On your ThinkCentre node (or any Linux system)
cd /path/to/kcore
sudo apt-get install libvirt-dev pkg-config gcc
make node-agent-podman
```

The binary will be in `bin/kcore-node-agent-linux-amd64`.

## Alternative: Remote Build via SSH

From your macOS machine:

```bash
# Sync code to Linux machine
rsync -avz --exclude 'bin/' --exclude '.git/' --exclude 'result*/' \
  ./ user@thinkcentre-node:/tmp/kcore-build/

# Build remotely
ssh user@thinkcentre-node 'cd /tmp/kcore-build && sudo apt-get install -y libvirt-dev pkg-config gcc && make node-agent-podman'

# Copy binary back
scp user@thinkcentre-node:/tmp/kcore-build/bin/kcore-node-agent-linux-amd64 ./bin/
```

## Summary

- ✅ **Controller**: Builds fine on macOS (`make controller`)
- ❌ **Node-agent**: Must be built on Linux (Podman emulation fails on macOS)
- ✅ **Solution**: Build on your ThinkCentre nodes or any Linux system

The node-agent binary is only needed for deployment to Linux nodes anyway, so building it on Linux makes sense!

