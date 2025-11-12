#!/usr/bin/env bash

# test_helpers.sh - Common utilities for integration tests

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Configuration
CONTROLLER_ADDR="${KCORE_CONTROLLER_ADDR:-localhost:8080}"
NODE_ADDR="${KCORE_NODE_ADDR:-192.168.40.146:9091}"
SSH_KEY="${KCORE_TEST_SSH_KEY:-~/.ssh/id_ed25519_gmail}"

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

# Test assertion functions
assert_success() {
    local cmd="$1"
    local msg="${2:-Command failed}"
    
    if eval "$cmd" &>/dev/null; then
        log_success "$msg"
        return 0
    else
        log_error "$msg"
        return 1
    fi
}

assert_equals() {
    local expected="$1"
    local actual="$2"
    local msg="${3:-Values do not match}"
    
    if [ "$expected" = "$actual" ]; then
        log_success "$msg (expected: $expected, got: $actual)"
        return 0
    else
        log_error "$msg (expected: $expected, got: $actual)"
        return 1
    fi
}

assert_contains() {
    local haystack="$1"
    local needle="$2"
    local msg="${3:-String not found}"
    
    if echo "$haystack" | grep -q "$needle"; then
        log_success "$msg"
        return 0
    else
        log_error "$msg (searching for '$needle' in '$haystack')"
        return 1
    fi
}

# Test execution wrapper
run_test() {
    local test_name="$1"
    local test_func="$2"
    
    TESTS_RUN=$((TESTS_RUN + 1))
    
    echo ""
    log_info "Running test: $test_name"
    echo "─────────────────────────────────────────────────────"
    
    if $test_func; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_success "✓ Test passed: $test_name"
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "✗ Test failed: $test_name"
    fi
    
    echo "─────────────────────────────────────────────────────"
}

# Print test summary
print_summary() {
    echo ""
    echo "═════════════════════════════════════════════════════"
    echo "Test Summary"
    echo "═════════════════════════════════════════════════════"
    echo "Total tests run:    $TESTS_RUN"
    echo -e "${GREEN}Tests passed:       $TESTS_PASSED${NC}"
    echo -e "${RED}Tests failed:       $TESTS_FAILED${NC}"
    echo "═════════════════════════════════════════════════════"
    
    if [ "$TESTS_FAILED" -eq 0 ]; then
        log_success "All tests passed! 🎉"
        return 0
    else
        log_error "Some tests failed"
        return 1
    fi
}

# gRPC helpers
grpc_call() {
    local service="$1"
    local method="$2"
    local data="$3"
    local target="${4:-$CONTROLLER_ADDR}"
    
    # Default to empty JSON if no data provided
    if [ -z "$data" ]; then
        data="{}"
    fi
    
    # Use proto files instead of reflection
    local proto_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../proto" && pwd)"
    local certs_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../certs" && pwd)"
    
    # Check if target is node or controller and use appropriate certs
    if [[ "$target" == *"9091"* ]]; then
        # Node connection - use TLS
        grpcurl -cacert "$certs_dir/ca.crt" \
                -cert "$certs_dir/controller.crt" \
                -key "$certs_dir/controller.key" \
                -import-path "$proto_dir" \
                -proto controller.proto -proto node.proto \
                -d "$data" "$target" "$service/$method"
    else
        # Controller connection - use plaintext for now
        grpcurl -plaintext \
                -import-path "$proto_dir" \
                -proto controller.proto -proto node.proto \
                -d "$data" "$target" "$service/$method"
    fi
}

controller_call() {
    local method="$1"
    local data="${2:-}"
    
    grpc_call "kcore.controller.Controller" "$method" "$data" "$CONTROLLER_ADDR"
}

node_call() {
    local method="$1"
    local data="${2:-}"
    
    grpc_call "kcore.node.NodeCompute" "$method" "$data" "$NODE_ADDR"
}

# Wait for service to be ready
wait_for_service() {
    local service_addr="$1"
    local max_attempts="${2:-30}"
    local attempt=0
    
    log_info "Waiting for service at $service_addr..."
    
    while [ $attempt -lt $max_attempts ]; do
        if nc -z "${service_addr%:*}" "${service_addr#*:}" 2>/dev/null; then
            log_success "Service is ready at $service_addr"
            return 0
        fi
        
        attempt=$((attempt + 1))
        sleep 1
    done
    
    log_error "Service at $service_addr did not become ready"
    return 1
}

# Check if controller is running
check_controller() {
    if ! nc -z localhost 8080 2>/dev/null; then
        log_error "Controller not running at localhost:8080"
        log_info "Start controller with: ./bin/kcore-controller -listen :8080"
        return 1
    fi
    
    log_success "Controller is running"
    return 0
}

# Check if node agent is running
check_node_agent() {
    local node_ip="${NODE_ADDR%:*}"
    local node_port="${NODE_ADDR#*:}"
    
    if ! nc -z "$node_ip" "$node_port" 2>/dev/null; then
        log_error "Node agent not running at $NODE_ADDR"
        log_info "Deploy node agent with: NODE_IP=$node_ip make deploy-node"
        return 1
    fi
    
    log_success "Node agent is running at $NODE_ADDR"
    return 0
}

# Get node list from controller
get_nodes() {
    controller_call "ListNodes" | jq -r '.nodes[]'
}

# Get VM list from controller
get_vms() {
    local target_node="${1:-}"
    local data=""
    
    if [ -n "$target_node" ]; then
        data=$(jq -n --arg node "$target_node" '{target_node: $node}')
    fi
    
    controller_call "ListVms" "$data" | jq -r '.vms[]' 2>/dev/null || echo ""
}

# Create a test VM
create_test_vm() {
    local vm_name="$1"
    local cpu="${2:-2}"
    local memory="${3:-4294967296}"  # 4GB default
    local target_node="${4:-}"
    
    # Generate a proper UUID for the VM
    local vm_id=$(uuidgen | tr '[:upper:]' '[:lower:]')
    
    local spec=$(jq -n \
        --arg id "$vm_id" \
        --arg name "$vm_name" \
        --argjson cpu "$cpu" \
        --arg memory "$memory" \
        --arg target_node "$target_node" \
        '{
            spec: {
                id: $id,
                name: $name,
                cpu: $cpu,
                memory_bytes: $memory
            },
            target_node: $target_node
        }')
    
    controller_call "CreateVm" "$spec"
}

# Delete a test VM
delete_test_vm() {
    local vm_id="$1"
    local target_node="${2:-}"
    
    local data=$(jq -n \
        --arg id "$vm_id" \
        --arg target_node "$target_node" \
        '{vm_id: $id, target_node: $target_node}')
    
    controller_call "DeleteVm" "$data"
}

# Cleanup function for tests
cleanup_test_vms() {
    log_info "Cleaning up test VMs..."
    
    local vms
    vms=$(get_vms 2>/dev/null || echo "")
    
    if [ -n "$vms" ]; then
        echo "$vms" | jq -r '.id' | while read -r vm_id; do
            if [[ "$vm_id" == test-* ]]; then
                log_info "Deleting test VM: $vm_id"
                delete_test_vm "$vm_id" || log_warn "Failed to delete $vm_id"
            fi
        done
    fi
    
    log_success "Cleanup complete"
}

# Trap for cleanup on exit
trap cleanup_test_vms EXIT

# Export functions for use in other scripts
export -f log_info log_success log_error log_warn
export -f assert_success assert_equals assert_contains
export -f run_test print_summary
export -f grpc_call controller_call node_call
export -f wait_for_service check_controller check_node_agent
export -f get_nodes get_vms create_test_vm delete_test_vm cleanup_test_vms

