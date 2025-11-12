#!/usr/bin/env bash

# full_workflow_test.sh - End-to-end VM lifecycle test

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test_helpers.sh"

# Test configuration
TEST_VM_NAME="test-vm-$(date +%s)"
TEST_VM_CPU=2
TEST_VM_MEMORY=4294967296  # 4GB

echo "═════════════════════════════════════════════════════"
echo "kcore End-to-End Workflow Test"
echo "═════════════════════════════════════════════════════"
echo ""
log_info "Test VM: $TEST_VM_NAME"
echo ""

# Prerequisites check
test_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check required tools
    for tool in grpcurl jq nc; do
        if ! command -v "$tool" &>/dev/null; then
            log_error "$tool is not installed"
            return 1
        fi
    done
    
    # Check controller
    check_controller || return 1
    
    # Check node agent
    check_node_agent || return 1
    
    log_success "All prerequisites met"
    return 0
}

# Test 1: List nodes
test_list_nodes() {
    log_info "Listing registered nodes..."
    
    local nodes
    nodes=$(get_nodes)
    
    if [ -z "$nodes" ]; then
        log_error "No nodes registered"
        return 1
    fi
    
    log_info "Registered nodes:"
    echo "$nodes" | jq -r '.nodeId + " - " + .hostname + " (" + .address + ")"'
    
    return 0
}

# Test 2: Create VM
test_create_vm() {
    log_info "Creating VM: $TEST_VM_NAME..."
    
    local result
    result=$(create_test_vm "$TEST_VM_NAME" "$TEST_VM_CPU" "$TEST_VM_MEMORY" "$NODE_ADDR")
    
    if ! echo "$result" | jq -e '.vmId' &>/dev/null; then
        log_error "VM creation failed: $result"
        return 1
    fi
    
    local vm_id
    vm_id=$(echo "$result" | jq -r '.vmId')
    
    log_success "VM created: $vm_id"
    return 0
}

# Test 3: List VMs
test_list_vms() {
    log_info "Listing VMs..."
    
    local vms
    vms=$(get_vms)
    
    if [ -z "$vms" ]; then
        log_warn "No VMs found"
        return 1
    fi
    
    log_info "Found VMs:"
    echo "$vms" | jq -r '.name + " (" + .id + ") - " + .state'
    
    # Check if our test VM is in the list
    if ! echo "$vms" | jq -e ".name == \"$TEST_VM_NAME\"" &>/dev/null; then
        log_error "Test VM not found in list"
        return 1
    fi
    
    log_success "Test VM found in list"
    return 0
}

# Test 4: Get VM details
test_get_vm_details() {
    log_info "Getting VM details for $TEST_VM_NAME..."
    
    local data
    data=$(jq -n --arg id "$TEST_VM_NAME" '{vm_id: $id}')
    
    local result
    result=$(controller_call "GetVm" "$data")
    
    if ! echo "$result" | jq -e '.spec' &>/dev/null; then
        log_error "Failed to get VM details: $result"
        return 1
    fi
    
    log_info "VM details:"
    echo "$result" | jq '.'
    
    return 0
}

# Test 5: List VMs on specific node
test_list_vms_on_node() {
    log_info "Listing VMs on node $NODE_ADDR..."
    
    local vms
    vms=$(get_vms "$NODE_ADDR")
    
    if [ -z "$vms" ]; then
        log_warn "No VMs found on node"
        # This might be expected if node is empty
        return 0
    fi
    
    log_info "VMs on node:"
    echo "$vms" | jq -r '.name + " (" + .id + ")"'
    
    return 0
}

# Test 6: Delete VM
test_delete_vm() {
    log_info "Deleting VM: $TEST_VM_NAME..."
    
    local result
    result=$(delete_test_vm "$TEST_VM_NAME" "$NODE_ADDR")
    
    if ! echo "$result" | jq -e '.success == true' &>/dev/null; then
        log_error "VM deletion failed: $result"
        return 1
    fi
    
    log_success "VM deleted successfully"
    
    # Verify VM is gone
    sleep 2
    local vms
    vms=$(get_vms 2>/dev/null || echo "")
    
    if echo "$vms" | jq -e ".name == \"$TEST_VM_NAME\"" &>/dev/null; then
        log_error "VM still exists after deletion"
        return 1
    fi
    
    log_success "Verified VM is deleted"
    return 0
}

# Run all tests
main() {
    run_test "Prerequisites Check" test_prerequisites
    run_test "List Nodes" test_list_nodes
    run_test "Create VM" test_create_vm
    run_test "List All VMs" test_list_vms
    run_test "Get VM Details" test_get_vm_details
    run_test "List VMs on Node" test_list_vms_on_node
    run_test "Delete VM" test_delete_vm
    
    print_summary
}

main "$@"

