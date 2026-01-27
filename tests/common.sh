#!/bin/bash
#
# Common test utilities for smolvm integration tests.
#
# Source this file in test scripts:
#   source "$(dirname "$0")/common.sh"

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
        local smolvm_dir=$(find "$dist_dir" -maxdepth 1 -type d \( -name 'smolvm-*-darwin-*' -o -name 'smolvm-*-linux-*' \) 2>/dev/null | head -1)
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

# Initialize SMOLVM variable
init_smolvm() {
    SMOLVM=$(find_smolvm)

    if [[ -z "$SMOLVM" ]]; then
        echo -e "${RED}Error: Could not find smolvm binary${NC}"
        echo "Either:"
        echo "  1. Build and extract the distribution: ./scripts/build-dist.sh"
        echo "  2. Set SMOLVM environment variable to the binary path"
        exit 1
    fi

    echo "Using smolvm: $SMOLVM"
}

# Log helpers
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

log_skip() {
    echo -e "${BLUE}[SKIP]${NC} $1"
}

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

# Track failed test names for summary
FAILED_TESTS=()

# Run a test function
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
        FAILED_TESTS+=("$test_name")
        return 1
    fi
}

# Print test summary
print_summary() {
    local test_suite="${1:-Tests}"

    echo ""
    echo "=========================================="
    echo "  $test_suite Summary"
    echo "=========================================="
    echo ""
    echo "Tests run:    $TESTS_RUN"
    echo -e "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
    echo -e "Tests failed: ${RED}$TESTS_FAILED${NC}"

    if [[ $TESTS_FAILED -gt 0 ]] && [[ ${#FAILED_TESTS[@]} -gt 0 ]]; then
        echo ""
        echo -e "${RED}Failed tests:${NC}"
        for test_name in "${FAILED_TESTS[@]}"; do
            echo -e "  ${RED}✗${NC} $test_name"
        done
    fi

    echo ""

    if [[ $TESTS_FAILED -eq 0 ]]; then
        echo -e "${GREEN}All tests passed!${NC}"
        return 0
    else
        echo -e "${RED}Some tests failed.${NC}"
        return 1
    fi
}

# Cleanup helper - stop microvm
cleanup_microvm() {
    $SMOLVM microvm stop 2>/dev/null || true
}

# Ensure microvm is running
ensure_microvm_running() {
    $SMOLVM microvm start 2>/dev/null || true
}

# Extract container ID from output
extract_container_id() {
    local output="$1"
    echo "$output" | grep -oE 'smolvm-[a-f0-9]+' | head -1
}

# Cleanup container by ID
cleanup_container() {
    local container_id="$1"
    $SMOLVM container rm default "$container_id" -f 2>/dev/null || true
}
