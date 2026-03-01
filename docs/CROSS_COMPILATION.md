# Cross-Compilation Setup

## Using Devbox

Devbox is configured to provide a consistent development environment. To use it:

```bash
# Enter devbox shell
devbox shell

# Build controller (macOS)
make controller

# Build node-agent (Linux/amd64) - uses Podman automatically
make node-agent
```

## Manual Podman Build

If you prefer to build manually without devbox:

```bash
# Build node-agent in Podman
podman run --rm \
  -v $(pwd):/work \
  -w /work \
  -e CGO_ENABLED=1 \
  -e GOOS=linux \
  -e GOARCH=amd64 \
  docker.io/golang:1.24 bash -c 'apt-get update -qq && apt-get install -y -qq libvirt-dev pkg-config gcc && go build -o bin/kcore-node-agent-linux-amd64 ./cmd/node-agent'
```

## Why Podman?

The node-agent requires CGO to link against libvirt. Cross-compiling CGO from macOS to Linux requires:
- Linux C libraries and headers
- Linux syscalls (not available in macOS SDK)
- Proper pkg-config configuration

Podman provides a clean Linux environment for building the Linux binary, similar to Docker but rootless and daemonless.

## Alternative: Build on Linux

You can also build directly on a Linux system:

```bash
# On Linux system
sudo apt-get install libvirt-dev pkg-config gcc
make node-agent-podman
```

