#!/usr/bin/env bash

# multi_node_test.sh - Multi-node scenario tests

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test_helpers.sh"

echo "═════════════════════════════════════════════════════"
echo "kcore Multi-Node Test"
echo "═════════════════════════════════════════════════════"
echo ""

# Test 1: Multiple node registration
test_multiple_nodes() {
    log_info "Testing multiple node scenario..."
    
    local nodes
    nodes=$(get_nodes)
    
    local node_count
    node_count=$(echo "$nodes" | jq -s 'length')
    
    log_info "Found $node_count registered node(s)"
    
    if [ "$node_count" -eq 0 ]; then
        log_warn "No nodes registered - multi-node test skipped"
        return 0
    fi
    
    echo "$nodes" | jq -r '.nodeId + " - " + .address'
    
    return 0
}

# Test 2: VM creation with explicit node targeting
test_explicit_node_targeting() {
    log_info "Testing explicit node targeting..."
    
    local test_vm="test-explicit-$(date +%s)"
    
    # Create VM on specific node
    local result
    result=$(create_test_vm "$test_vm" 2 4294967296 "$NODE_ADDR")
    
    if ! echo "$result" | jq -e '.vmId' &>/dev/null; then
        log_error "Failed to create VM on explicit node"
        return 1
    fi
    
    local vm_id
    vm_id=$(echo "$result" | jq -r '.vmId')
    local node_id
    node_id=$(echo "$result" | jq -r '.nodeId')
    
    log_success "VM $vm_id created on node $node_id"
    
    # Verify VM is on correct node
    local vms
    vms=$(get_vms "$NODE_ADDR")
    
    if ! echo "$vms" | jq -e ".id == \"$vm_id\"" &>/dev/null; then
        log_error "VM not found on target node"
        return 1
    fi
    
    log_success "VM verified on target node"
    
    # Cleanup
    delete_test_vm "$vm_id" "$NODE_ADDR"
    
    return 0
}

# Test 3: VM auto-scheduling (no explicit node)
test_auto_scheduling() {
    log_info "Testing auto-scheduling..."
    
    local nodes
    nodes=$(get_nodes)
    local node_count
    node_count=$(echo "$nodes" | jq -s 'length')
    
    if [ "$node_count" -eq 0 ]; then
        log_warn "No nodes available for auto-scheduling test"
        return 0
    fi
    
    local test_vm="test-auto-$(date +%s)"
    
    # Create VM without specifying node
    local result
    result=$(create_test_vm "$test_vm" 2 4294967296 "")
    
    if ! echo "$result" | jq -e '.vmId' &>/dev/null; then
        log_error "Auto-scheduling failed"
        return 1
    fi
    
    local vm_id
    vm_id=$(echo "$result" | jq -r '.vmId')
    local assigned_node
    assigned_node=$(echo "$result" | jq -r '.nodeId')
    
    log_success "VM $vm_id auto-scheduled to node $assigned_node"
    
    # Cleanup
    delete_test_vm "$vm_id"
    
    return 0
}

# Test 4: List VMs across all nodes
test_list_all_vms() {
    log_info "Testing VM listing across all nodes..."
    
    # Create test VMs on different nodes if multiple available
    local test_vm1="test-multi1-$(date +%s)"
    local test_vm2="test-multi2-$(date +%s)"
    
    create_test_vm "$test_vm1" 1 2147483648 "$NODE_ADDR"
    sleep 1
    create_test_vm "$test_vm2" 1 2147483648 "$NODE_ADDR"
    
    # List all VMs
    local all_vms
    all_vms=$(get_vms)
    
    if [ -z "$all_vms" ]; then
        log_error "No VMs found"
        return 1
    fi
    
    local vm_count
    vm_count=$(echo "$all_vms" | jq -s 'length')
    
    log_info "Found $vm_count VM(s) across all nodes"
    echo "$all_vms" | jq -r '.name + " on node " + .nodeId'
    
    # Verify both test VMs are present
    if ! echo "$all_vms" | jq -e ".name == \"$test_vm1\"" &>/dev/null; then
        log_error "Test VM 1 not found"
        return 1
    fi
    
    if ! echo "$all_vms" | jq -e ".name == \"$test_vm2\"" &>/dev/null; then
        log_error "Test VM 2 not found"
        return 1
    fi
    
    log_success "All test VMs found"
    
    # Cleanup
    delete_test_vm "$test_vm1" "$NODE_ADDR"
    delete_test_vm "$test_vm2" "$NODE_ADDR"
    
    return 0
}

# Run all tests
main() {
    run_test "Multiple Nodes" test_multiple_nodes
    run_test "Explicit Node Targeting" test_explicit_node_targeting
    run_test "Auto-Scheduling" test_auto_scheduling
    run_test "List All VMs" test_list_all_vms
    
    print_summary
}

main "$@"

