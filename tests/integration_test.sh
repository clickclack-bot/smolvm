#!/bin/bash
#
# smolvm Integration Tests
#
# This script runs end-to-end integration tests for smolvm.
# It requires a built distribution in dist/smolvm-*/ or a SMOLVM env var.
#
# Usage:
#   ./tests/integration_test.sh           # Auto-detect smolvm binary
#   SMOLVM=/path/to/smolvm ./tests/integration_test.sh  # Use specific binary
#
# Exit codes:
#   0 - All tests passed
#   1 - One or more tests failed
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Find the script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Find smolvm binary
find_smolvm() {
    if [[ -n "${SMOLVM:-}" ]] && [[ -x "$SMOLVM" ]]; then
        echo "$SMOLVM"
        return
    fi

    # Try to find in dist directory
    local dist_dir="$PROJECT_ROOT/dist"
    if [[ -d "$dist_dir" ]]; then
        # Find the extracted distribution directory
        local smolvm_dir=$(find "$dist_dir" -maxdepth 1 -type d -name 'smolvm-*-darwin-*' -o -name 'smolvm-*-linux-*' 2>/dev/null | head -1)
        if [[ -n "$smolvm_dir" ]] && [[ -x "$smolvm_dir/smolvm" ]]; then
            echo "$smolvm_dir/smolvm"
            return
        fi
    fi

    # Try cargo build output
    local target_release="$PROJECT_ROOT/target/release/smolvm"
    if [[ -x "$target_release" ]]; then
        echo "$target_release"
        return
    fi

    echo ""
}

SMOLVM=$(find_smolvm)

if [[ -z "$SMOLVM" ]]; then
    echo -e "${RED}Error: Could not find smolvm binary${NC}"
    echo "Either:"
    echo "  1. Build and extract the distribution: ./scripts/build-dist.sh"
    echo "  2. Set SMOLVM environment variable to the binary path"
    exit 1
fi

echo "Using smolvm: $SMOLVM"
echo ""

# Test helper functions
log_test() {
    echo -e "${YELLOW}[TEST]${NC} $1"
}

log_pass() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((TESTS_PASSED++))
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((TESTS_FAILED++))
}

run_test() {
    local test_name="$1"
    local test_func="$2"

    ((TESTS_RUN++))
    log_test "$test_name"

    if $test_func; then
        log_pass "$test_name"
        return 0
    else
        log_fail "$test_name"
        return 1
    fi
}

# Cleanup function
cleanup() {
    echo ""
    echo "Cleaning up..."

    # Stop any running microvm
    $SMOLVM microvm stop 2>/dev/null || true

    echo "Cleanup complete"
}

# Set up trap for cleanup on exit
trap cleanup EXIT

# =============================================================================
# Test Cases
# =============================================================================

test_version() {
    local output
    output=$($SMOLVM --version 2>&1)
    [[ "$output" == *"smolvm"* ]]
}

test_help() {
    local output
    output=$($SMOLVM --help 2>&1)
    [[ "$output" == *"sandbox"* ]] && [[ "$output" == *"microvm"* ]] && [[ "$output" == *"container"* ]]
}

test_sandbox_run_echo() {
    local output
    output=$($SMOLVM sandbox run alpine:latest -- echo "integration-test-marker" 2>&1)
    [[ "$output" == *"integration-test-marker"* ]]
}

test_sandbox_run_exit_code() {
    # Test that exit codes are propagated correctly
    $SMOLVM sandbox run alpine:latest -- sh -c "exit 0" 2>&1
    local exit_0=$?

    # Capture the exit code without triggering set -e
    local exit_42=0
    $SMOLVM sandbox run alpine:latest -- sh -c "exit 42" 2>&1 || exit_42=$?

    [[ $exit_0 -eq 0 ]] && [[ $exit_42 -eq 42 ]]
}

test_sandbox_run_with_env() {
    local output
    output=$($SMOLVM sandbox run -e TEST_VAR=hello_world alpine:latest -- sh -c 'echo $TEST_VAR' 2>&1)
    [[ "$output" == *"hello_world"* ]]
}

test_microvm_start_stop() {
    # Start the microvm
    $SMOLVM microvm start 2>&1

    # Check status
    local status
    status=$($SMOLVM microvm status 2>&1)
    [[ "$status" == *"running"* ]] || return 1

    # Stop it
    $SMOLVM microvm stop 2>&1

    # Check it's stopped
    status=$($SMOLVM microvm status 2>&1) || true
    [[ "$status" == *"not running"* ]] || [[ "$status" == *"stopped"* ]]
}

test_microvm_exec() {
    # Start the microvm
    $SMOLVM microvm start 2>&1

    # Execute a command
    local output
    output=$($SMOLVM microvm exec -- cat /etc/os-release 2>&1)

    # Verify output contains Alpine
    [[ "$output" == *"Alpine"* ]]
}

test_container_create_and_list() {
    # Ensure microvm is running
    $SMOLVM microvm start 2>&1 || true

    # Create a container
    local create_output
    create_output=$($SMOLVM container create default alpine:latest -- sleep 300 2>&1)

    # Extract container ID
    local container_id
    container_id=$(echo "$create_output" | grep -oE 'smolvm-[a-f0-9]+' | head -1)

    if [[ -z "$container_id" ]]; then
        echo "Failed to extract container ID from: $create_output"
        return 1
    fi

    # List containers and verify it exists
    local list_output
    list_output=$($SMOLVM container ls default 2>&1)

    # Clean up
    $SMOLVM container rm default "$container_id" -f 2>&1 || true

    [[ "$list_output" == *"$container_id"* ]] || [[ "$list_output" == *"${container_id:0:12}"* ]]
}

test_container_exec() {
    # Ensure microvm is running
    $SMOLVM microvm start 2>&1 || true

    # Create a container
    local create_output
    create_output=$($SMOLVM container create default alpine:latest -- sleep 300 2>&1)

    local container_id
    container_id=$(echo "$create_output" | grep -oE 'smolvm-[a-f0-9]+' | head -1)

    if [[ -z "$container_id" ]]; then
        return 1
    fi

    # Execute a command inside the container
    local exec_output
    exec_output=$($SMOLVM container exec default "$container_id" -- echo "exec-test-marker" 2>&1)

    # Clean up
    $SMOLVM container rm default "$container_id" -f 2>&1 || true

    [[ "$exec_output" == *"exec-test-marker"* ]]
}

test_container_stop_start() {
    # Ensure microvm is running
    $SMOLVM microvm start 2>&1 || true

    # Create a container
    local create_output
    create_output=$($SMOLVM container create default alpine:latest -- sleep 300 2>&1)

    local container_id
    container_id=$(echo "$create_output" | grep -oE 'smolvm-[a-f0-9]+' | head -1)

    if [[ -z "$container_id" ]]; then
        return 1
    fi

    # Stop the container
    $SMOLVM container stop default "$container_id" 2>&1

    # Verify it's stopped
    local list_output
    list_output=$($SMOLVM container ls default -a 2>&1)
    if [[ "$list_output" != *"stopped"* ]]; then
        $SMOLVM container rm default "$container_id" -f 2>&1 || true
        return 1
    fi

    # Start it again (restart)
    $SMOLVM container start default "$container_id" 2>&1

    # Verify it's running
    list_output=$($SMOLVM container ls default 2>&1)

    # Clean up
    $SMOLVM container rm default "$container_id" -f 2>&1 || true

    [[ "$list_output" == *"running"* ]]
}

test_container_id_format() {
    # Ensure microvm is running
    $SMOLVM microvm start 2>&1 || true

    # Create a container
    local create_output
    create_output=$($SMOLVM container create default alpine:latest -- sleep 10 2>&1)

    local container_id
    container_id=$(echo "$create_output" | grep -oE 'smolvm-[a-f0-9]+' | head -1)

    # Clean up
    $SMOLVM container rm default "$container_id" -f 2>&1 || true

    # Verify the ID format: smolvm-{12 or 16 hex chars}
    # Old format: 7 (smolvm-) + 12 = 19
    # New format: 7 (smolvm-) + 16 = 23
    local id_len=${#container_id}
    if [[ $id_len -ne 19 ]] && [[ $id_len -ne 23 ]]; then
        echo "Container ID has wrong length: $id_len (expected 19 or 23)"
        return 1
    fi

    # Verify it matches the pattern (12 or 16 hex chars)
    if [[ ! "$container_id" =~ ^smolvm-[a-f0-9]{12,16}$ ]]; then
        echo "Container ID doesn't match expected pattern: $container_id"
        return 1
    fi

    return 0
}

test_timeout() {
    # Test that timeout works (command should be killed after timeout)
    local start_time
    start_time=$(date +%s)

    # Run a command with a 5 second timeout that would otherwise run for 60 seconds
    local output
    output=$($SMOLVM sandbox run --timeout 5s alpine:latest -- sleep 60 2>&1 || true)

    local end_time
    end_time=$(date +%s)
    local elapsed=$((end_time - start_time))

    # Should complete in less than 60 seconds (the original sleep duration)
    # Allow generous time for VM startup and image pull overhead
    # The key test is that it doesn't wait the full 60 seconds
    if [[ $elapsed -ge 60 ]]; then
        echo "Timeout test failed: took $elapsed seconds (expected < 60)"
        return 1
    fi

    # Additionally verify the timeout message appears
    if [[ "$output" == *"timed out"* ]]; then
        return 0
    fi

    # If no timeout message but completed quickly, still pass
    [[ $elapsed -lt 30 ]]
}

# =============================================================================
# Database Persistence Tests
# =============================================================================

test_db_persistence_across_restart() {
    # Test that VM configuration persists across process restarts

    # Create a named VM with specific configuration
    local vm_name="db-test-vm-$$"
    $SMOLVM microvm create "$vm_name" --cpus 2 --mem 1024 2>&1

    # Verify it was created
    local list_output
    list_output=$($SMOLVM microvm ls --json 2>&1)
    if [[ "$list_output" != *"$vm_name"* ]]; then
        echo "VM was not created"
        return 1
    fi

    # Verify the configuration is correct (this exercises the database read)
    if [[ "$list_output" != *'"cpus": 2'* ]] || [[ "$list_output" != *'"memory_mib": 1024'* ]]; then
        echo "VM configuration not persisted correctly"
        $SMOLVM microvm delete "$vm_name" -f 2>/dev/null || true
        return 1
    fi

    # List again (simulates a "restart" by re-reading from database)
    local list_output2
    list_output2=$($SMOLVM microvm ls --json 2>&1)

    # Clean up
    $SMOLVM microvm delete "$vm_name" -f 2>&1

    # Verify the data was consistent
    [[ "$list_output" == "$list_output2" ]]
}

test_db_vm_state_update() {
    # Test that VM state updates are persisted to database

    # Create a named VM
    local vm_name="db-state-test-$$"
    $SMOLVM microvm create "$vm_name" 2>&1

    # Check initial state is "created"
    local initial_state
    initial_state=$($SMOLVM microvm ls --json 2>&1)
    if [[ "$initial_state" != *'"state": "created"'* ]]; then
        echo "Initial state should be 'created'"
        $SMOLVM microvm delete "$vm_name" -f 2>/dev/null || true
        return 1
    fi

    # Start the VM
    $SMOLVM microvm start "$vm_name" 2>&1

    # Check state changed to "running"
    local running_state
    running_state=$($SMOLVM microvm ls --json 2>&1)
    if [[ "$running_state" != *'"state": "running"'* ]]; then
        echo "State should be 'running' after start"
        $SMOLVM microvm stop "$vm_name" 2>/dev/null || true
        $SMOLVM microvm delete "$vm_name" -f 2>/dev/null || true
        return 1
    fi

    # Stop the VM
    $SMOLVM microvm stop "$vm_name" 2>&1

    # Check state changed to "stopped"
    local stopped_state
    stopped_state=$($SMOLVM microvm ls --json 2>&1)

    # Clean up
    $SMOLVM microvm delete "$vm_name" -f 2>&1

    [[ "$stopped_state" == *'"state": "stopped"'* ]]
}

test_db_delete_removes_from_db() {
    # Test that deleting a VM removes it from the database

    # Create a VM
    local vm_name="db-delete-test-$$"
    $SMOLVM microvm create "$vm_name" 2>&1

    # Verify it exists
    local before_delete
    before_delete=$($SMOLVM microvm ls --json 2>&1)
    if [[ "$before_delete" != *"$vm_name"* ]]; then
        echo "VM should exist before delete"
        return 1
    fi

    # Delete it
    $SMOLVM microvm delete "$vm_name" -f 2>&1

    # Verify it's gone
    local after_delete
    after_delete=$($SMOLVM microvm ls --json 2>&1)

    [[ "$after_delete" != *"$vm_name"* ]]
}

# =============================================================================
# Run Tests
# =============================================================================

echo "=========================================="
echo "  smolvm Integration Tests"
echo "=========================================="
echo ""

# Basic tests
run_test "Version command" test_version || true
run_test "Help command" test_help || true

# Sandbox tests
run_test "Sandbox run echo" test_sandbox_run_echo || true
run_test "Sandbox run exit code" test_sandbox_run_exit_code || true
run_test "Sandbox run with env" test_sandbox_run_with_env || true

# Microvm tests
run_test "Microvm start/stop" test_microvm_start_stop || true
run_test "Microvm exec" test_microvm_exec || true

# Container lifecycle tests
run_test "Container create and list" test_container_create_and_list || true
run_test "Container exec" test_container_exec || true
run_test "Container stop/start (restart)" test_container_stop_start || true
run_test "Container ID format" test_container_id_format || true

# Timeout test
run_test "Command timeout" test_timeout || true

# Database persistence tests
run_test "DB persistence across restart" test_db_persistence_across_restart || true
run_test "DB VM state update" test_db_vm_state_update || true
run_test "DB delete removes from database" test_db_delete_removes_from_db || true

# =============================================================================
# Summary
# =============================================================================

echo ""
echo "=========================================="
echo "  Test Summary"
echo "=========================================="
echo ""
echo "Tests run:    $TESTS_RUN"
echo -e "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests failed: ${RED}$TESTS_FAILED${NC}"
echo ""

if [[ $TESTS_FAILED -eq 0 ]]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi
