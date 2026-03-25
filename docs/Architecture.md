# kcore-rust Architecture

This document shows how `kctl`, `kcore-controller`, Nix config generation, and `kcore-node-agent` work together to manage VMs declaratively.

## High-level flow

```mermaid
%%{init: {"flowchart": {"nodeSpacing": 25, "rankSpacing": 120}} }%%
flowchart TB
  U[Operator] --> K[kctl CLI]
  K -->|gRPC API calls| C[kcore-controller]

  C -->|Read/Write desired state| DB[(SQLite DB)]
  C -->|Select node| S[Scheduler]
  C -->|Generate Nix text<br/>ch-vm.vms| NIXGEN[nixgen::generate_node_config]

  NIXGEN -->|ApplyNixConfig rebuild=true| A[kcore-node-agent<br/>NodeAdmin]
  A -->|write file| CFG["/etc/nixos/kcore-vms.nix"]

  subgraph APPLY_PATH["Node apply + runtime (right lane)"]
    direction TB
    REBUILD[nixos-rebuild switch]
    MOD[ch-vm Nix module]
    NET[bridge/tap + NAT systemd services]
    VMUNIT[kcore-vm-*.service]
    CH[cloud-hypervisor]
    REBUILD --> MOD
    MOD --> NET
    MOD --> VMUNIT
    VMUNIT --> CH
  end

  A -->|trigger| REBUILD

  CH --> SOCK["/run/kcore/*.sock"]
  A -->|NodeCompute reads VM status| SOCK
  A -->|heartbeat / VM info| C
```

## Component responsibilities

- `kctl` sends user intent (create/delete/start/stop/get/list).
- `kcore-controller` stores desired state, picks a target node, and renders declarative Nix VM config.
- `nixgen::generate_node_config` produces the `ch-vm.vms` block (networks + virtualMachines).
- `kcore-node-agent` writes the config file and applies it via `nixos-rebuild switch`.
- `ch-vm` module realizes networking, TAP devices, seed ISOs, and VM systemd services.
- `cloud-hypervisor` runs VMs; node-agent queries runtime state from API sockets.

## Create/Delete VM lifecycle

```mermaid
sequenceDiagram
  participant K as kctl
  participant C as kcore-controller
  participant D as SQLite
  participant G as nixgen
  participant A as node-agent (NodeAdmin)
  participant N as NixOS switch
  participant H as cloud-hypervisor

  K->>C: CreateVm / DeleteVm
  C->>D: persist desired VM state
  C->>G: generate_node_config(vms, gateway, network)
  G-->>C: configuration_nix (ch-vm.vms)
  C->>A: ApplyNixConfig(configuration_nix, rebuild=true)
  A->>A: write /etc/nixos/kcore-vms.nix
  A->>N: nixos-rebuild switch
  N->>H: start/stop/update kcore-vm-*.service
  H-->>A: runtime state via API socket
  A-->>C: node heartbeat / VM state visibility
```
