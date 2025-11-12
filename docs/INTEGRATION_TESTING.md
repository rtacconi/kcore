# Integration Testing Guide

This guide covers the comprehensive integration testing framework for kcore.

## Overview

The integration test suite validates the entire kcore stack:

- **Controller Tests**: Node registration, VM routing, request forwarding
- **End-to-End Tests**: Complete VM lifecycle operations
- **Multi-Node Tests**: Cluster operations with multiple nodes
- **kctl Tests**: CLI functionality and user workflows

## Test Structure

```
test/integration/
├── README.md                    # Test documentation
├── controller/                  # Controller Go tests
│   ├── controller_test.go      # Basic controller tests
│   └── vm_operations_test.go   # VM operation tests (planned)
├── e2e/                        # End-to-end shell tests
│   ├── test_helpers.sh         # Common test utilities
│   ├── full_workflow_test.sh   # Complete VM lifecycle
│   └── multi_node_test.sh      # Multi-node scenarios
├── kctl/                       # CLI integration tests
│   └── kctl_test.sh           # CLI command tests
└── fixtures/                   # Test data
    └── vm-specs/              # Sample VM specifications
```

## Prerequisites

### Required Tools

```bash
# Install required tools
brew install grpcurl jq   # macOS
apt-get install grpcurl jq  # Debian/Ubuntu
```

### Running Services

1. **Controller** (required for most tests):

```bash
./bin/kcore-controller -listen :8080
```

2. **Node Agent** (required for E2E tests):

```bash
# On the node (e.g., 192.168.40.146)
/usr/local/bin/kcore-node-agent
```

### Environment Configuration

```bash
export KCORE_CONTROLLER_ADDR="localhost:8080"
export KCORE_NODE_ADDR="192.168.40.146:9091"
export KCORE_TEST_SSH_KEY="~/.ssh/id_ed25519_gmail"
```

## Running Tests

### Quick Start

```bash
# Run all integration tests
make test-integration

# Run all tests (unit + integration)
make test-all
```

### Individual Test Suites

```bash
# Controller integration tests (Go)
make test-controller

# End-to-end workflow tests
make test-e2e

# Multi-node scenario tests
make test-multi-node

# kctl CLI tests
make test-kctl
```

### Advanced Usage

```bash
# Run with custom configuration
KCORE_NODE_ADDR=192.168.1.100:9091 make test-e2e

# Run specific test
./test/integration/e2e/full_workflow_test.sh

# CI mode (fail fast)
CI=true ./scripts/run-integration-tests.sh

# Skip certain suites
./scripts/run-integration-tests.sh --skip-kctl --skip-multi-node
```

## Test Phases

### Phase 1: Controller Tests ✅

**Status**: Complete

**Tests**:
- ✅ Controller server initialization
- ✅ Node registration
- ✅ Node listing
- ✅ Heartbeat mechanism
- ✅ VM-to-node tracking
- ✅ Request routing logic
- ✅ Auto-scheduling

**Run**:
```bash
make test-controller
```

### Phase 2: End-to-End Tests 🔄

**Status**: In Progress (waiting for node-agent deployment)

**Tests**:
- Create VM via controller → node
- List VMs from all nodes
- Get VM details
- Start/Stop VM operations
- Delete VM
- Explicit node targeting
- Auto-scheduling

**Run**:
```bash
make test-e2e
```

**Current Blocker**: Node agent needs to be deployed and running on test node

### Phase 3: Multi-Node Tests 🔄

**Status**: Pending

**Tests**:
- Multiple node registration
- VM distribution across nodes
- Node-specific VM listing
- Cluster-wide VM listing
- Node failover (planned)
- Load balancing (planned)

**Run**:
```bash
make test-multi-node
```

### Phase 4: kctl Integration 🔄

**Status**: Pending controller integration in kctl

**Tests**:
- CLI command structure
- Config file parsing
- Version and help commands
- VM creation via CLI
- VM listing via CLI
- VM deletion via CLI
- Error handling

**Run**:
```bash
make test-kctl
```

## Writing New Tests

### Go Tests

Create test files in `test/integration/controller/`:

```go
package controller_test

import (
    "testing"
    ctrlpb "github.com/kcore/kcore/api/controller"
)

func TestMyFeature(t *testing.T) {
    // Your test here
}
```

Run with:
```bash
go test ./test/integration/controller/... -v
```

### Shell Tests

Create test scripts in `test/integration/e2e/` or `test/integration/kctl/`:

```bash
#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test_helpers.sh"

test_my_feature() {
    log_info "Testing my feature..."
    
    # Test logic here
    
    return 0
}

main() {
    run_test "My Feature Test" test_my_feature
    print_summary
}

main "$@"
```

Make executable:
```bash
chmod +x test/integration/e2e/my_test.sh
```

### Using Test Helpers

The `test_helpers.sh` provides common utilities:

```bash
# Logging
log_info "Information message"
log_success "Success message"
log_error "Error message"
log_warn "Warning message"

# Assertions
assert_success "command" "Description"
assert_equals "expected" "actual" "Description"
assert_contains "haystack" "needle" "Description"

# gRPC calls
controller_call "CreateVm" '{"spec": {...}}'
node_call "ListVms" '{}'

# Service checks
check_controller
check_node_agent
wait_for_service "localhost:8080"

# VM operations
create_test_vm "vm-name" 2 4294967296 "node-addr"
delete_test_vm "vm-id" "node-addr"
get_vms
get_nodes
```

## Test Fixtures

Sample VM specifications are in `test/integration/fixtures/vm-specs/`:

```yaml
# basic-vm.yaml
apiVersion: kcore.dev/v1
kind: VirtualMachine
metadata:
  name: basic-vm
spec:
  cpu: 2
  memory: 4Gi
  disks:
    - name: root
      size: 20Gi
      storageClass: local-dir
```

Use in tests:
```bash
VM_SPEC=$(cat test/integration/fixtures/vm-specs/basic-vm.yaml)
```

## CI/CD Integration

### GitHub Actions

```yaml
name: Integration Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup Go
        uses: actions/setup-go@v4
        with:
          go-version: '1.22'
      
      - name: Install dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y grpcurl jq
      
      - name: Build
        run: |
          make proto
          make controller
      
      - name: Start Controller
        run: |
          ./bin/kcore-controller -listen :8080 &
          sleep 2
      
      - name: Run Integration Tests
        env:
          CI: true
          KCORE_CONTROLLER_ADDR: localhost:8080
        run: |
          make test-integration
```

### Local CI Simulation

```bash
CI=true ./scripts/run-integration-tests.sh
```

## Debugging Tests

### Verbose Output

```bash
# Go tests with verbose output
go test ./test/integration/controller/... -v

# Shell tests show detailed output by default
./test/integration/e2e/full_workflow_test.sh
```

### Running Single Test Function

```bash
# Go test
go test ./test/integration/controller -v -run TestNodeRegistration

# Shell test - modify script to run only specific function
test_list_nodes
```

### Inspecting gRPC Calls

```bash
# Enable grpcurl debugging
GRPC_TRACE=all GRPC_VERBOSITY=DEBUG grpcurl ...
```

### Common Issues

#### Controller Not Running

```
Error: Controller not running at localhost:8080
```

**Solution**:
```bash
./bin/kcore-controller -listen :8080
```

#### Node Agent Not Accessible

```
Error: Node agent not running at 192.168.40.146:9091
```

**Solution**:
```bash
# Check node status
ssh root@192.168.40.146 'systemctl status kcode-node-agent'

# Deploy if needed
NODE_IP=192.168.40.146 make deploy-node
```

#### Test VM Cleanup

```bash
# Manual cleanup of test VMs
./test/integration/e2e/test_helpers.sh
cleanup_test_vms
```

## Test Results

### Expected Output

```
═════════════════════════════════════════════════════════
kcore Integration Test Suite
═════════════════════════════════════════════════════════

[INFO] Checking prerequisites...
[SUCCESS] All required tools available
[SUCCESS] Controller is running
[SUCCESS] Node agent is running

═════════════════════════════════════════════════════════
Test Suite: Controller Integration Tests
═════════════════════════════════════════════════════════

[INFO] Running test: List Nodes
─────────────────────────────────────────────────────
[SUCCESS] ✓ Test passed: List Nodes
─────────────────────────────────────────────────────

...

═════════════════════════════════════════════════════════
Test Summary
═════════════════════════════════════════════════════════

Total tests run:    15
Tests passed:       15
Tests failed:       0

[SUCCESS] All tests passed! 🎉
```

## Metrics and Coverage

### Test Coverage

```bash
# Go test coverage
go test ./test/integration/controller/... -coverprofile=coverage.out
go tool cover -html=coverage.out
```

### Test Metrics

Track test execution time:
```bash
time make test-integration
```

## Roadmap

### Short Term
- ✅ Test framework structure
- ✅ Controller integration tests
- ✅ E2E test scripts
- ✅ Test helper utilities
- 🔄 Complete E2E tests (waiting on node-agent)
- 🔄 kctl integration tests

### Medium Term
- Multi-node cluster tests
- Performance tests
- Stress tests
- Failure injection tests

### Long Term
- Automated test scheduling
- Test result dashboards
- Regression test database
- Chaos engineering tests

## Contributing

When adding new features:

1. Write tests first (TDD approach)
2. Add unit tests in `pkg/*/`
3. Add integration tests in `test/integration/`
4. Update this documentation
5. Run full test suite before submitting PR

## Resources

- [Testing Status](TESTING_STATUS.md) - Current test progress
- [Architecture](ARCHITECTURE.md) - System design
- [Commands](COMMANDS.md) - CLI reference
- [Makefile](../Makefile) - Build and test targets

## Support

For issues with tests:

1. Check [TESTING_STATUS.md](TESTING_STATUS.md) for known issues
2. Review logs in test output
3. Run with verbose flags
4. Check prerequisites and service status

