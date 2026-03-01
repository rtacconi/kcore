# Unified ControlPlane API

This document defines the migration from split APIs (`Controller` + `ControllerAdmin` + ad-hoc install flow) to a single `ControlPlane` API surface.

## Goals

- One API for orchestration, admin operations, and day-0 automation.
- Remove dependency on baked private certificates in the ISO.
- Enable fully remote bootstrap/enrollment from `kctl` and Terraform.

## Service

- gRPC service: `kcore.controlplane.ControlPlane`
- Proto: `proto/controlplane.proto`
- Go package: `api/controlplane`

## Coverage

- Existing RPCs (delegated): node registration, heartbeat, sync, VM operations, node listing, controller config apply.
- New RPCs (automation): token lifecycle, bootstrap config, node enrollment, cert rotation, install status.

## kctl command mapping

Scaffolded command tree:

- `kctl controlplane` (alias: `kctl cp`)
  - `kctl cp config apply-controller --file <configuration.nix>`
  - `kctl cp config apply-node --node <node-id> --file <configuration.nix>`
  - `kctl cp enroll token create`
  - `kctl cp enroll token revoke --id <token-id>`
  - `kctl cp enroll token list`
  - `kctl cp enroll bootstrap-config --token <token> --hostname <node-hostname>`
  - `kctl cp install status get --node <node-id>`
  - `kctl cp install status list`

Current status: command shape is in place; execution handlers are explicitly marked not implemented and are intended to be wired to `pkg/controlplaneclient`.

## Terraform mapping

Scaffolded provider objects:

- `resource "kcore_enrollment_token"`
- `resource "kcore_node_enrollment"`
- `resource "kcore_node_wait_ready"`
- `data "kcore_bootstrap_config"`

Current status: schema and registration exist; handlers return explicit not-implemented errors until server-side automation RPCs are fully implemented.

## Security model

- Node private keys are generated on-node.
- Enrollment uses short-lived/revocable tokens.
- `EnrollNode` accepts CSR and returns signed cert + CA bundle.
- `RotateNodeCertificate` supports renewal without host SSH.

## Migration plan

1. Add unified proto + generated stubs.
2. Add unified server skeleton and register it in controller.
3. Add reusable unified client package.
4. Add CLI command mapping (`kctl cp ...`).
5. Add Terraform resource/data-source mapping.
6. Implement automation RPC storage/signing and wire clients end-to-end.
