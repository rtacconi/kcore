# kcore (MVP)

Minimal libvirt-based node-agent with gRPC, plus a NixOS host config and ISO build instructions.

## Layout
- `api/v1/compute.proto`: gRPC API
- `cmd/node-agent`: node-agent main
- `internal/agent`: libvirt implementation
- `flake.nix`: NixOS host config (libvirt, bridge `br0`, node-agent service)
- `Makefile`: proto generation + build
- `scripts/build-iso.sh`: Docker-based ISO build for macOS

## Prerequisites
- Go 1.22+
- `protoc` (protobuf compiler)
- Docker Desktop (for building ISO on macOS)

## Generate protobufs
```bash
make proto
```

## Build node-agent (Darwin/Linux)
```bash
make build
# binary at bin/node-agent
```

## Run node-agent locally (Linux host with libvirt)
```bash
sudo ./bin/node-agent --listen :8443 --data-dir /var/lib/kcore
```

## Build NixOS ISO on macOS Intel
Yes, you can build on macOS Intel using Docker.

```bash
chmod +x scripts/build-iso.sh
./scripts/build-iso.sh
# Result symlink in ./result, ISO under result/iso/*.iso
```

If the container complains about privileges, ensure Docker has "virt" features enabled and try again. The script uses an official Nix image and builds `.#nixosConfigurations.kvm-node.config.system.build.isoImage`.

## Install on Lenovo ThinkCentre
1. Write the ISO to a USB stick from macOS:
   ```bash
   diskutil list
   # Identify your USB (e.g., /dev/disk4). Unmount it:
   diskutil unmountDisk /dev/disk4
   sudo dd if=result/iso/*.iso of=/dev/rdisk4 bs=4m conv=sync progress
   diskutil eject /dev/disk4
   ```
2. Boot the ThinkCentre from USB, run the installer.
3. After install, the system will have libvirt enabled and a `node-agent` service configured. If you want to use your locally built binary instead of the flake package, copy it to `/opt/node-agent` and adjust the service.

## Notes
- Networking uses a Linux bridge `br0` on the primary NIC; ensure your environment supports bridging.
- The agent currently creates qcow2 disks and a cloud-init NoCloud ISO per-VM.
- Add TLS flags (`--tls-cert/--tls-key/--tls-ca`) when you wire mTLS.
