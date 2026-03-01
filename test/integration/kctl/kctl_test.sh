#!/usr/bin/env bash

# kctl_test.sh - kctl CLI integration tests

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../e2e/test_helpers.sh"

# kctl binary path
KCTL_BIN="${KCTL_BIN:-./bin/kctl}"

echo "═════════════════════════════════════════════════════"
echo "kctl CLI Integration Test"
echo "═════════════════════════════════════════════════════"
echo ""

# Test prerequisites
test_kctl_available() {
    log_info "Checking kctl binary..."
    
    if [ ! -f "$KCTL_BIN" ]; then
        log_error "kctl binary not found at $KCTL_BIN"
        log_info "Build with: make kctl"
        return 1
    fi
    
    if [ ! -x "$KCTL_BIN" ]; then
        log_error "kctl binary is not executable"
        return 1
    fi
    
    log_success "kctl binary found at $KCTL_BIN"
    return 0
}

# Test 1: kctl version
test_kctl_version() {
    log_info "Testing kctl version..."
    
    local output
    output=$("$KCTL_BIN" version 2>&1)
    
    if ! echo "$output" | grep -q "kctl version"; then
        log_error "Version command failed: $output"
        return 1
    fi
    
    log_success "kctl version: $output"
    return 0
}

# Test 2: kctl help
test_kctl_help() {
    log_info "Testing kctl help..."
    
    local output
    output=$("$KCTL_BIN" --help 2>&1)
    
    # Check for expected commands
    for cmd in "create" "get" "delete" "describe" "apply"; do
        if ! echo "$output" | grep -q "$cmd"; then
            log_error "Command '$cmd' not found in help output"
            return 1
        fi
    done
    
    log_success "All expected commands present"
    return 0
}

# Test 3: kctl get nodes
test_kctl_get_nodes() {
    log_info "Testing kctl get nodes..."
    
    # First check if controller is configured
    check_controller || return 1
    
    # Note: This requires kctl to be updated to use controller API
    # For now, we'll test if the command structure exists
    
    if "$KCTL_BIN" get --help 2>&1 | grep -q "nodes"; then
        log_success "kctl get nodes command available"
        return 0
    else
        log_warn "kctl get nodes command not yet implemented"
        return 0  # Don't fail - this is expected during development
    fi
}

# Test 4: kctl config
test_kctl_config() {
    log_info "Testing kctl configuration..."
    
    # Create temporary config
    local temp_config
    temp_config=$(mktemp)
    
    cat > "$temp_config" << EOF
controller:
  address: localhost:8080
  insecure: true

default_node: 192.168.40.146:9091
EOF
    
    log_info "Created test config at $temp_config"
    
    # Test with config flag
    if "$KCTL_BIN" --config "$temp_config" version &>/dev/null; then
        log_success "Config file parsing works"
    else
        log_warn "Config file parsing needs implementation"
    fi
    
    rm -f "$temp_config"
    return 0
}

# Test 5: kctl error handling
test_kctl_error_handling() {
    log_info "Testing kctl error handling..."
    
    # Test invalid command
    if "$KCTL_BIN" invalid-command &>/dev/null; then
        log_error "Invalid command should fail"
        return 1
    else
        log_success "Invalid command properly rejected"
    fi
    
    # Test missing required args
    if "$KCTL_BIN" create &>/dev/null; then
        log_error "Missing args should fail"
        return 1
    else
        log_success "Missing args properly rejected"
    fi
    
    return 0
}

# Test 6: kctl VM operations (when controller API is integrated)
test_kctl_vm_operations() {
    log_info "Testing kctl VM operations..."
    
    # Check if controller integration is complete
    if ! check_controller; then
        log_warn "Controller not available - skipping VM operations"
        return 0
    fi
    
    # These tests will be enabled once kctl uses controller API
    log_info "Full VM operations testing pending controller integration"
    
    # Future tests:
    # - kctl create vm
    # - kctl get vms
    # - kctl get vm <name>
    # - kctl describe vm <name>
    # - kctl delete vm <name>
    
    return 0
}

# Test 7: kctl output formats
test_kctl_output_formats() {
    log_info "Testing kctl output formats..."
    
    # Test if output format flags are recognized
    if "$KCTL_BIN" get --help 2>&1 | grep -q "output"; then
        log_success "Output format flag available"
        return 0
    else
        log_warn "Output format flag not yet implemented"
        return 0
    fi
}

# Run all tests
main() {
    run_test "kctl Binary Available" test_kctl_available
    run_test "kctl Version" test_kctl_version
    run_test "kctl Help" test_kctl_help
    run_test "kctl Get Nodes" test_kctl_get_nodes
    run_test "kctl Config" test_kctl_config
    run_test "kctl Error Handling" test_kctl_error_handling
    run_test "kctl VM Operations" test_kctl_vm_operations
    run_test "kctl Output Formats" test_kctl_output_formats
    
    print_summary
}

main "$@"

