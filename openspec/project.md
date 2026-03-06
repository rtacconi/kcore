# kcore Project Context

## Overview

kcore is a NixOS-based node system for running VMs (KVM/libvirt). Nodes run a node-agent (gRPC) and can be managed by a controller. Installation is done by booting from a kcore ISO and running install-to-disk (interactive or via the install automator API).

## Tech stack

- **OS / image**: NixOS (flake.nix), live ISO and installed system
- **Node agent**: Go, gRPC with mTLS, port 9091
- **Install automator**: HTTP/HTTPS API, port 9092 (Talos-style: bootstrap vs installed)
- **VM storage**: `/var/lib/kcore/disks` on the installed system
- **Certs**: `/etc/kcore/` (ca.crt, node.crt, node.key)

## Conventions

- Specs under `openspec/specs/` describe current or intended behavior; use SHALL/MUST and GIVEN/WHEN/THEN scenarios.
- Install manifest: YAML with `os_disk` (e.g. `sda`, `nvme0n1`); see `examples/install-manifest.yaml`.
