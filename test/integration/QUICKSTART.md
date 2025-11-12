# Integration Tests Quick Start

Get started with kcore integration testing in 5 minutes.

## Prerequisites

```bash
# Install required tools (macOS)
brew install grpcurl jq

# Install required tools (Linux)
apt-get install grpcurl jq
```

## Setup

### 1. Build kcore Components

```bash
# Build everything
make proto
make controller
make kctl
make node-agent-nix
```

### 2. Start Controller

```bash
# Terminal 1: Start controller
./bin/kcore-controller -listen :8080
```

### 3. Deploy Node Agent (Optional for E2E tests)

```bash
# Deploy to your node
NODE_IP=192.168.40.146 make deploy-node

# Or manually
scp bin/kcore-node-agent-linux-amd64 root@192.168.40.146:/usr/local/bin/kcore-node-agent
ssh root@192.168.40.146 '/usr/local/bin/kcore-node-agent &'
```

## Run Tests

### Quick Test (Controller Only)

```bash
# Run controller integration tests
make test-controller
```

Expected output:
```
=== RUN   TestControllerBasicOperations
=== RUN   TestControllerBasicOperations/ListNodes
--- PASS: TestControllerBasicOperations (0.01s)
    --- PASS: TestControllerBasicOperations/ListNodes (0.01s)
...
PASS
```

### Full End-to-End Test

```bash
# Requires controller + node agent running
make test-e2e
```

Expected output:
```
═════════════════════════════════════════════════════════
kcore End-to-End Workflow Test
═════════════════════════════════════════════════════════

[INFO] Running test: Prerequisites Check
[SUCCESS] ✓ Test passed: Prerequisites Check

[INFO] Running test: Create VM
[SUCCESS] VM created: test-vm-1699564800
[SUCCESS] ✓ Test passed: Create VM

...

═════════════════════════════════════════════════════════
Total tests run:    7
Tests passed:       7
Tests failed:       0

[SUCCESS] All tests passed! 🎉
```

### All Integration Tests

```bash
# Run everything
make test-integration
```

## Configuration

Set environment variables if using non-default addresses:

```bash
export KCORE_CONTROLLER_ADDR="localhost:8080"
export KCORE_NODE_ADDR="192.168.40.146:9091"
export KCORE_TEST_SSH_KEY="~/.ssh/id_ed25519_gmail"
```

## Common Issues

### Controller Not Running

```
Error: Controller not running at localhost:8080
```

**Fix**: Start controller in another terminal:
```bash
./bin/kcore-controller -listen :8080
```

### Node Agent Not Accessible

```
Error: Node agent not running at 192.168.40.146:9091
```

**Fix**: Deploy and start node agent:
```bash
NODE_IP=192.168.40.146 make deploy-node
```

### Test Hangs

If a test hangs, it usually means a service is not responding. Press Ctrl+C and check:

```bash
# Check controller
curl -v http://localhost:8080

# Check node agent
nc -zv 192.168.40.146 9091
```

## Next Steps

- Read [Integration Testing Guide](../../docs/INTEGRATION_TESTING.md)
- Review [Testing Status](../../docs/TESTING_STATUS.md)
- Write custom tests using [test_helpers.sh](e2e/test_helpers.sh)

## Test Development

### Add a New E2E Test

```bash
# Create test file
cat > test/integration/e2e/my_test.sh << 'EOF'
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test_helpers.sh"

test_my_feature() {
    log_info "Testing my feature..."
    
    # Your test logic
    local result=$(controller_call "ListNodes")
    
    if [ -n "$result" ]; then
        log_success "Test passed"
        return 0
    else
        log_error "Test failed"
        return 1
    fi
}

main() {
    run_test "My Feature" test_my_feature
    print_summary
}

main "$@"
EOF

# Make executable
chmod +x test/integration/e2e/my_test.sh

# Run it
./test/integration/e2e/my_test.sh
```

### Add a New Go Test

```bash
cat > test/integration/controller/my_test.go << 'EOF'
package controller_test

import (
    "context"
    "testing"
    ctrlpb "github.com/kcore/kcore/api/controller"
)

func TestMyFeature(t *testing.T) {
    // Your test here
}
EOF

# Run it
go test ./test/integration/controller -v -run TestMyFeature
```

## CI/CD

To run in CI:

```bash
# Set CI mode (fail fast)
CI=true make test-integration
```

## Help

For more details:
- `make help` - Show all make targets
- `./scripts/run-integration-tests.sh --help` - Test runner options
- `docs/INTEGRATION_TESTING.md` - Full testing guide

