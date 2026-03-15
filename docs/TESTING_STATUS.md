# Testing Status

## Test Suite Overview

All tests pass as of March 2026. VMs are created declaratively via `kctl apply -f` or Terraform only.

---

## Unit Tests

### kctl CLI (`cmd/kctl/`)

| Test File | Tests | Description |
|---|---|---|
| `manifest_test.go` | 6 tests | VM YAML manifest parsing, validation, kind detection |
| `config_test.go` | 5 tests | Config loading/saving, address normalization, insecure mode |
| `client_test.go` | 2 tests | Memory size parsing, byte formatting |

```bash
go test ./cmd/kctl/... -count=1
```

### Node Agent (`node/`)

| Test File | Tests | Description |
|---|---|---|
| `server_cloudinit_test.go` | 2 tests | Cloud-init user-data generation, image flavor detection |

```bash
go test ./node/... -count=1
```

### Control Plane (`pkg/controlplane/`)

| Test File | Tests | Description |
|---|---|---|
| `service_test.go` | 1 test | Enrollment token create/list/revoke lifecycle |

```bash
go test ./pkg/controlplane/... -count=1
```

### Terraform Provider (`terraform-provider-kcore/`)

| Test File | Tests | Description |
|---|---|---|
| `provider_test.go` | 2 tests | Provider schema validation |
| `resource_vm_test.go` | 1 test | Acceptance test for `kcore_vm` resource (requires live controller) |

```bash
cd terraform-provider-kcore && go test ./internal/provider/... -run TestProvider -count=1
```

---

## Integration Tests

### Controller (`test/integration/controller/`)

| Test | Description |
|---|---|
| `TestControllerBasicOperations` | gRPC ListNodes (skips if controller not reachable) |
| `TestNodeRegistration` | Node registration via in-process server |
| `TestNodeListing` | Empty node listing |
| `TestVmToNodeTracking` | VM lookup for nonexistent VM |
| `TestControllerScheduling` | CreateVm with no nodes available |
| `TestControllerHeartbeat` | Heartbeat from unknown node |

```bash
go test ./test/integration/controller/... -count=1
```

### Terraform E2E (`examples/terraform-isolated/`)

Requires `KCORE_TF_E2E=1` and a live controller at `KCORE_CONTROLLER_ADDRESS`.

```bash
KCORE_TF_E2E=1 KCORE_CONTROLLER_ADDRESS=192.168.40.10:9090 \
  go test ./examples/terraform-isolated/... -count=1
```

---

## Running All Tests

```bash
# Unit + integration (no live services required)
cd /path/to/kcore
go test ./cmd/kctl/... ./node/... ./pkg/... ./test/... -count=1

# Terraform provider unit tests
cd terraform-provider-kcore && go test ./internal/provider/... -run TestProvider -count=1
```

---

## What Was Removed

- `kctl create vm` CLI command and all related tests (replaced by `kctl apply -f`)
- CLI-option VM creation tests

---

## Cursor Rules

A cursor rule at `.cursor/rules/go-tests.mdc` ensures tests are run after any `.go` file change in the `kcore/` tree. See that file for per-package test commands.
