# Isolated OpenTofu VM Test

This project is an isolated OpenTofu smoke test for `kcore_vm`.

- It uses a local provider binary built from this repo.
- It does **not** require installing the provider globally in `~/.terraform.d`.
- State and plugin data stay in this folder.

## Prerequisites

- Either:
  - OpenTofu (`tofu`) in `PATH`, or
  - Nix installed (scripts auto-fallback to repo `./nix_shell` dev shell with `tofu`)
- Access to controller (default `192.168.40.10:9090`)

## Apply (create test VM with OpenTofu)

From repo root:

```bash
./examples/terraform-isolated/apply.sh
```

Optional custom test id:

```bash
./examples/terraform-isolated/apply.sh lab2
```

The created VM name is:

`<vm_name_prefix>-<test_id>` (default prefix: `tf-debian-lab`)

## Destroy (cleanup test VM with OpenTofu)

```bash
./examples/terraform-isolated/destroy.sh
```

You can also pass a specific test id:

```bash
./examples/terraform-isolated/destroy.sh lab2
```

## Notes

- This test uses a minimal VM resource (CPU/memory + NIC on `default` network).
- If needed, edit `variables.tf` defaults (controller address, CPU, memory, network).
- In provider dev-override mode, do not run `tofu init` manually; use `apply.sh` / `destroy.sh`.
- Defaults assume a TLS-enabled controller and use `../../certs/dev/{node.crt,node.key,ca.crt}` for client auth.
- If you see `error reading server preface: EOF`, the client is usually speaking plaintext to a TLS endpoint; verify address/protocol and TLS vars.

## Go Integration Test

Run the apply/create + verify + destroy + verify cycle:

```bash
KCORE_TF_E2E=1 ./nix_shell go test ./examples/terraform-isolated -run TestTerraformIsolatedApplyDestroy -v
```
