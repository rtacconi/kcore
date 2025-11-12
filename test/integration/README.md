# Integration Tests

This directory contains integration tests for kcore components.

## Structure

```
test/integration/
├── README.md                    # This file
├── controller/                  # Controller integration tests
│   ├── controller_test.go      # Basic controller tests
│   └── vm_operations_test.go   # VM operation tests through controller
├── e2e/                        # End-to-end tests
│   ├── full_workflow_test.sh   # Complete VM lifecycle test
│   ├── multi_node_test.sh      # Multi-node scenario tests
│   └── test_helpers.sh         # Common test utilities
├── kctl/                       # kctl CLI integration tests
│   ├── kctl_test.sh           # CLI command tests
│   └── config_test.sh         # Configuration tests
└── fixtures/                   # Test fixtures and data
    ├── vm-specs/              # Sample VM specifications
    └── node-configs/          # Sample node configurations
```

## Running Tests

### Prerequisites

1. **Controller running**: `./bin/kcore-controller -listen :8080`
2. **Node agent running**: On node at `192.168.40.146:9091`
3. **Test dependencies**: `go`, `grpcurl`, `jq`, `ssh`

### Run All Tests

```bash
make test-integration
```

### Run Specific Test Suites

```bash
# Controller tests
go test ./test/integration/controller -v

# End-to-end tests
./test/integration/e2e/full_workflow_test.sh

# kctl tests
./test/integration/kctl/kctl_test.sh
```

## Test Phases

### Phase 1: Controller Unit Tests ✅
- Controller server creation
- Node registration
- Node listing
- Request routing logic

### Phase 2: Controller-Node Integration 🔄
- VM creation through controller
- VM listing from nodes
- VM deletion
- Multi-node operations

### Phase 3: End-to-End Workflows 🔄
- Complete VM lifecycle (create → start → stop → delete)
- Multi-VM management
- Node failover scenarios

### Phase 4: kctl Integration 🔄
- CLI command execution
- Config file parsing
- Error handling
- Output formatting

## Environment Variables

```bash
export KCORE_CONTROLLER_ADDR="localhost:8080"
export KCORE_NODE_ADDR="192.168.40.146:9091"
export KCORE_TEST_SSH_KEY="~/.ssh/id_ed25519_gmail"
```

## Writing New Tests

1. **Go tests**: Place in appropriate subdirectory
2. **Shell tests**: Use `test_helpers.sh` for common functions
3. **Fixtures**: Add to `fixtures/` directory
4. **Documentation**: Update this README

## CI/CD Integration

These tests are designed to run in CI pipelines:

```bash
# In CI environment
export CI=true
./scripts/run-integration-tests.sh
```

## Troubleshooting

See `docs/TESTING_STATUS.md` for current test status and known issues.

