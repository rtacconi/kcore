#!/usr/bin/env bash

# run-integration-tests.sh - Run all integration tests

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

log_section() {
    echo ""
    echo "═════════════════════════════════════════════════════"
    echo "$1"
    echo "═════════════════════════════════════════════════════"
    echo ""
}

# Configuration
export KCORE_CONTROLLER_ADDR="${KCORE_CONTROLLER_ADDR:-localhost:8080}"
export KCORE_NODE_ADDR="${KCORE_NODE_ADDR:-192.168.40.146:9091}"
export KCORE_TEST_SSH_KEY="${KCORE_TEST_SSH_KEY:-~/.ssh/id_ed25519_gmail}"

# Test suite selection
RUN_UNIT_TESTS="${RUN_UNIT_TESTS:-true}"
RUN_CONTROLLER_TESTS="${RUN_CONTROLLER_TESTS:-true}"
RUN_E2E_TESTS="${RUN_E2E_TESTS:-true}"
RUN_KCTL_TESTS="${RUN_KCTL_TESTS:-true}"

# CI mode (fail fast)
CI_MODE="${CI:-false}"

# Results tracking
declare -A TEST_RESULTS
TOTAL_SUITES=0
PASSED_SUITES=0
FAILED_SUITES=0

# Run a test suite
run_suite() {
    local suite_name="$1"
    local test_command="$2"
    
    TOTAL_SUITES=$((TOTAL_SUITES + 1))
    
    log_section "Test Suite: $suite_name"
    
    if eval "$test_command"; then
        TEST_RESULTS["$suite_name"]="PASSED"
        PASSED_SUITES=$((PASSED_SUITES + 1))
        log_success "✓ $suite_name passed"
    else
        TEST_RESULTS["$suite_name"]="FAILED"
        FAILED_SUITES=$((FAILED_SUITES + 1))
        log_error "✗ $suite_name failed"
        
        if [ "$CI_MODE" = "true" ]; then
            log_error "CI mode: Stopping on first failure"
            return 1
        fi
    fi
    
    return 0
}

# Prerequisites check
check_prerequisites() {
    log_section "Checking Prerequisites"
    
    local missing_tools=()
    
    for tool in go grpcurl jq nc; do
        if ! command -v "$tool" &>/dev/null; then
            missing_tools+=("$tool")
        fi
    done
    
    if [ ${#missing_tools[@]} -gt 0 ]; then
        log_error "Missing required tools: ${missing_tools[*]}"
        return 1
    fi
    
    log_success "All required tools available"
    
    # Check if controller is running
    if [ "$RUN_CONTROLLER_TESTS" = "true" ] || [ "$RUN_E2E_TESTS" = "true" ]; then
        if ! nc -z localhost 8080 2>/dev/null; then
            log_error "Controller not running at localhost:8080"
            log_info "Start with: ./bin/kcore-controller -listen :8080"
            return 1
        fi
        log_success "Controller is running"
    fi
    
    # Check if node agent is running (optional for some tests)
    if [ "$RUN_E2E_TESTS" = "true" ]; then
        local node_ip="${KCORE_NODE_ADDR%:*}"
        local node_port="${KCORE_NODE_ADDR#*:}"
        
        if ! nc -z "$node_ip" "$node_port" 2>/dev/null; then
            log_error "Node agent not running at $KCORE_NODE_ADDR"
            log_info "Some E2E tests will be skipped"
        else
            log_success "Node agent is running"
        fi
    fi
    
    return 0
}

# Main test execution
main() {
    cd "$PROJECT_ROOT"
    
    log_section "kcore Integration Test Suite"
    log_info "Project root: $PROJECT_ROOT"
    log_info "Controller: $KCORE_CONTROLLER_ADDR"
    log_info "Node: $KCORE_NODE_ADDR"
    echo ""
    
    # Prerequisites
    if ! check_prerequisites; then
        log_error "Prerequisites check failed"
        exit 1
    fi
    
    # Run test suites
    if [ "$RUN_UNIT_TESTS" = "true" ]; then
        run_suite "Unit Tests" "go test ./pkg/... -v" || true
    fi
    
    if [ "$RUN_CONTROLLER_TESTS" = "true" ]; then
        run_suite "Controller Integration Tests" \
            "go test ./test/integration/controller/... -v" || true
    fi
    
    if [ "$RUN_E2E_TESTS" = "true" ]; then
        # Make scripts executable
        chmod +x test/integration/e2e/*.sh
        
        run_suite "E2E Full Workflow" \
            "./test/integration/e2e/full_workflow_test.sh" || true
        
        run_suite "E2E Multi-Node" \
            "./test/integration/e2e/multi_node_test.sh" || true
    fi
    
    if [ "$RUN_KCTL_TESTS" = "true" ]; then
        chmod +x test/integration/kctl/*.sh
        
        run_suite "kctl Integration" \
            "./test/integration/kctl/kctl_test.sh" || true
    fi
    
    # Print summary
    log_section "Test Summary"
    
    echo "Test Suites Run: $TOTAL_SUITES"
    echo -e "${GREEN}Passed:          $PASSED_SUITES${NC}"
    echo -e "${RED}Failed:          $FAILED_SUITES${NC}"
    echo ""
    
    # Detailed results
    echo "Detailed Results:"
    for suite in "${!TEST_RESULTS[@]}"; do
        local result="${TEST_RESULTS[$suite]}"
        if [ "$result" = "PASSED" ]; then
            echo -e "  ${GREEN}✓${NC} $suite"
        else
            echo -e "  ${RED}✗${NC} $suite"
        fi
    done
    
    echo ""
    echo "═════════════════════════════════════════════════════"
    
    if [ "$FAILED_SUITES" -eq 0 ]; then
        log_success "All test suites passed! 🎉"
        exit 0
    else
        log_error "Some test suites failed"
        exit 1
    fi
}

# Help message
show_help() {
    cat << EOF
Usage: $0 [OPTIONS]

Run kcore integration tests

OPTIONS:
    -h, --help              Show this help message
    --skip-unit             Skip unit tests
    --skip-controller       Skip controller integration tests
    --skip-e2e              Skip end-to-end tests
    --skip-kctl             Skip kctl tests
    --controller ADDR       Controller address (default: localhost:8080)
    --node ADDR             Node agent address (default: 192.168.40.146:9091)
    --ci                    Run in CI mode (fail fast)

EXAMPLES:
    # Run all tests
    $0

    # Run only E2E tests
    $0 --skip-unit --skip-controller --skip-kctl

    # Run with custom addresses
    $0 --controller localhost:9090 --node 192.168.1.100:9091

    # CI mode
    CI=true $0

ENVIRONMENT VARIABLES:
    KCORE_CONTROLLER_ADDR   Controller address
    KCORE_NODE_ADDR         Node agent address
    KCORE_TEST_SSH_KEY      SSH key for node access
    CI                      Enable CI mode
EOF
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_help
            exit 0
            ;;
        --skip-unit)
            RUN_UNIT_TESTS=false
            shift
            ;;
        --skip-controller)
            RUN_CONTROLLER_TESTS=false
            shift
            ;;
        --skip-e2e)
            RUN_E2E_TESTS=false
            shift
            ;;
        --skip-kctl)
            RUN_KCTL_TESTS=false
            shift
            ;;
        --controller)
            export KCORE_CONTROLLER_ADDR="$2"
            shift 2
            ;;
        --node)
            export KCORE_NODE_ADDR="$2"
            shift 2
            ;;
        --ci)
            CI_MODE=true
            shift
            ;;
        *)
            log_error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

main "$@"

