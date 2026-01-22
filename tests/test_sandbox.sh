#!/bin/bash
#
# Sandbox tests for smolvm.
#
# Tests the `smolvm sandbox run` command functionality.
# Requires VM environment.
#
# Usage:
#   ./tests/test_sandbox.sh

source "$(dirname "$0")/common.sh"
init_smolvm

echo ""
echo "=========================================="
echo "  smolvm Sandbox Tests"
echo "=========================================="
echo ""

# =============================================================================
# Basic Execution
# =============================================================================

test_sandbox_run_echo() {
    local output
    output=$($SMOLVM sandbox run alpine:latest -- echo "integration-test-marker" 2>&1)
    [[ "$output" == *"integration-test-marker"* ]]
}

test_sandbox_run_cat() {
    local output
    output=$($SMOLVM sandbox run alpine:latest -- cat /etc/os-release 2>&1)
    [[ "$output" == *"Alpine"* ]]
}

# =============================================================================
# Exit Codes
# =============================================================================

test_sandbox_exit_code_zero() {
    $SMOLVM sandbox run alpine:latest -- sh -c "exit 0" 2>&1
}

test_sandbox_exit_code_nonzero() {
    local exit_code=0
    $SMOLVM sandbox run alpine:latest -- sh -c "exit 42" 2>&1 || exit_code=$?
    [[ $exit_code -eq 42 ]]
}

test_sandbox_exit_code_one() {
    local exit_code=0
    $SMOLVM sandbox run alpine:latest -- sh -c "exit 1" 2>&1 || exit_code=$?
    [[ $exit_code -eq 1 ]]
}

# =============================================================================
# Environment Variables
# =============================================================================

test_sandbox_env_variable() {
    local output
    output=$($SMOLVM sandbox run -e TEST_VAR=hello_world alpine:latest -- sh -c 'echo $TEST_VAR' 2>&1)
    [[ "$output" == *"hello_world"* ]]
}

test_sandbox_multiple_env_variables() {
    local output
    output=$($SMOLVM sandbox run -e VAR1=one -e VAR2=two alpine:latest -- sh -c 'echo $VAR1 $VAR2' 2>&1)
    [[ "$output" == *"one"* ]] && [[ "$output" == *"two"* ]]
}

# =============================================================================
# Timeout
# =============================================================================

test_sandbox_timeout() {
    local start_time end_time elapsed output
    start_time=$(date +%s)

    output=$($SMOLVM sandbox run --timeout 5s alpine:latest -- sleep 60 2>&1 || true)

    end_time=$(date +%s)
    elapsed=$((end_time - start_time))

    # Should complete in much less than 60 seconds
    if [[ $elapsed -ge 60 ]]; then
        echo "Timeout test failed: took $elapsed seconds (expected < 60)"
        return 1
    fi

    # Check for timeout message or that it completed quickly
    [[ "$output" == *"timed out"* ]] || [[ $elapsed -lt 30 ]]
}

# =============================================================================
# Working Directory
# =============================================================================

test_sandbox_workdir() {
    local output
    output=$($SMOLVM sandbox run -w /tmp alpine:latest -- pwd 2>&1)
    [[ "$output" == *"/tmp"* ]]
}

# =============================================================================
# Command Execution
# =============================================================================

test_sandbox_shell_pipeline() {
    local output
    output=$($SMOLVM sandbox run alpine:latest -- sh -c "echo 'hello world' | wc -w" 2>&1)
    [[ "$output" == *"2"* ]]
}

test_sandbox_command_not_found() {
    ! $SMOLVM sandbox run alpine:latest -- nonexistent_command_12345 2>/dev/null
}

# =============================================================================
# Run Tests
# =============================================================================

run_test "Sandbox run echo" test_sandbox_run_echo || true
run_test "Sandbox run cat /etc/os-release" test_sandbox_run_cat || true
run_test "Exit code 0" test_sandbox_exit_code_zero || true
run_test "Exit code 42" test_sandbox_exit_code_nonzero || true
run_test "Exit code 1" test_sandbox_exit_code_one || true
run_test "Environment variable" test_sandbox_env_variable || true
run_test "Multiple environment variables" test_sandbox_multiple_env_variables || true
run_test "Timeout" test_sandbox_timeout || true
run_test "Working directory" test_sandbox_workdir || true
run_test "Shell pipeline" test_sandbox_shell_pipeline || true
run_test "Command not found fails" test_sandbox_command_not_found || true

print_summary "Sandbox Tests"
