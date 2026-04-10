# Security Groups (v1)

Security groups provide ingress policy and host-to-VM exposure rules for NAT
networks.

## Scope

- Supports both attachment models:
  - VM-level attachment (specific VM id/name)
  - Network-level attachment (network + node)
- Supports open-and-NAT rule in a single rule:
  - host `<protocol>/<hostPort>` exposure
  - optional DNAT to `targetVm:targetPort`

## Rule Model

Each rule contains:

- `protocol`: `tcp` or `udp`
- `hostPort`: host ingress port
- `targetPort`: VM port (defaults to `hostPort` if omitted)
- `sourceCidr`: optional CIDR (defaults to `0.0.0.0/0`)
- `targetVm`: optional VM id/name
- `enableDnat`: if true, adds DNAT to VM private IP

## YAML Manifest

```yaml
kind: SecurityGroup
metadata:
  name: web-ingress
spec:
  description: expose web vm
  rules:
    - id: https
      protocol: tcp
      hostPort: 8443
      targetPort: 443
      sourceCidr: 0.0.0.0/0
      targetVm: web-01
      enableDnat: true
  attachments:
    - kind: network
      target: private
      node: node-1
    - kind: vm
      target: web-01
```

## kctl Commands

- `kctl security-group create -f sg.yaml`
- `kctl security-group apply -f sg.yaml` (reconciles attachments)
- `kctl security-group list`
- `kctl security-group get <name>`
- `kctl security-group delete <name>`
- `kctl security-group attach --name <sg> --kind vm --target <vm>`
- `kctl security-group attach --name <sg> --kind network --target <net> --target-node <node>`
- `kctl security-group detach ...`

## Rendering Path

Controller resolves effective SG rules for each network on each node from:

- network attachments
- VM attachments for VMs connected to that network

Resolved rules are rendered into Nix under
`ch-vm.vms.networks.<name>.securityGroupRules`.

The `ch-vm` networking module translates these rules into `nftables` rules in
the existing NAT pipeline.
