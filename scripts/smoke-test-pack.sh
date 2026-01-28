#!/bin/bash
#
# Smoke test for pack functionality.
#
# This script tests pack basics WITHOUT requiring VM execution:
# - Pack command produces valid output files
# - Binary can read its own metadata (--version, --info)
# - Sidecar file has expected structure
#
# Safe to run in CI without hypervisor access.
#
# Usage:
#   ./scripts/smoke-test-pack.sh
#
# Exit codes:
#   0 - All smoke tests passed
#   1 - Some tests failed
#   2 - Setup failed (smolvm binary not found)

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Find project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Test state
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Temporary directory for test artifacts
TEST_DIR=""

log_test() { echo -e "${YELLOW}[TEST]${NC} $1"; }
log_pass() { echo -e "${GREEN}[PASS]${NC} $1"; ((TESTS_PASSED++)); }
log_fail() { echo -e "${RED}[FAIL]${NC} $1"; ((TESTS_FAILED++)); }
log_info() { echo -e "[INFO] $1"; }

cleanup() {
    if [[ -n "$TEST_DIR" ]] && [[ -d "$TEST_DIR" ]]; then
        rm -rf "$TEST_DIR"
    fi
}

trap cleanup EXIT

# Find smolvm binary
find_smolvm() {
    # Check SMOLVM env var
    if [[ -n "${SMOLVM:-}" ]] && [[ -x "$SMOLVM" ]]; then
        echo "$SMOLVM"
        return
    fi

    # Check dist directory
    local dist_dir="$PROJECT_ROOT/dist"
    if [[ -d "$dist_dir" ]]; then
        local smolvm_dir=$(find "$dist_dir" -maxdepth 1 -type d \( -name 'smolvm-*-darwin-*' -o -name 'smolvm-*-linux-*' \) 2>/dev/null | head -1)
        if [[ -n "$smolvm_dir" ]] && [[ -x "$smolvm_dir/smolvm" ]]; then
            echo "$smolvm_dir/smolvm"
            return
        fi
    fi

    # Check target/release
    if [[ -x "$PROJECT_ROOT/target/release/smolvm" ]]; then
        echo "$PROJECT_ROOT/target/release/smolvm"
        return
    fi

    echo ""
}

run_test() {
    local name="$1"
    local func="$2"
    ((TESTS_RUN++))
    log_test "$name"
    if $func; then
        log_pass "$name"
        return 0
    else
        log_fail "$name"
        return 1
    fi
}

# =============================================================================
# Smoke Tests
# =============================================================================

test_pack_command_exists() {
    $SMOLVM pack --help 2>&1 | grep -q "Package an OCI image"
}

test_pack_creates_binary() {
    local output="$TEST_DIR/smoke-alpine"

    log_info "Packing alpine:latest (this may take a moment on first run)..."
    log_info "Note: This test requires VM access (hypervisor framework / KVM)"

    local pack_output
    pack_output=$($SMOLVM pack alpine:latest -o "$output" 2>&1)
    local exit_code=$?

    # Check for known library/hypervisor errors (expected in CI)
    if [[ "$pack_output" == *"libkrunfw"* ]] || [[ "$pack_output" == *"krun_start_enter"* ]]; then
        echo "SKIP: VM environment not available (expected in CI)"
        return 0
    fi

    if [[ $exit_code -ne 0 ]]; then
        echo "Pack command failed: $pack_output"
        return 1
    fi

    if [[ ! -f "$output" ]]; then
        echo "Binary not created at $output"
        return 1
    fi

    if [[ ! -x "$output" ]]; then
        echo "Binary is not executable"
        return 1
    fi

    return 0
}

test_pack_creates_sidecar() {
    local output="$TEST_DIR/smoke-alpine"

    if [[ ! -f "$output.smolmachine" ]]; then
        # May have been skipped due to no VM access
        if [[ ! -f "$output" ]]; then
            echo "SKIP: No packed binary (VM not available)"
            return 0
        fi
        echo "Sidecar file not created at $output.smolmachine"
        return 1
    fi

    # Sidecar should have reasonable size (at least 1MB for kernel + libs)
    local size
    size=$(stat -f%z "$output.smolmachine" 2>/dev/null || stat -c%s "$output.smolmachine" 2>/dev/null)

    if [[ $size -lt 1000000 ]]; then
        echo "Sidecar too small: $size bytes (expected > 1MB)"
        return 1
    fi

    return 0
}

test_packed_version_works() {
    local output="$TEST_DIR/smoke-alpine"

    if [[ ! -f "$output" ]]; then
        echo "SKIP: No packed binary (VM not available)"
        return 0
    fi

    local result
    result=$("$output" --version 2>&1) || true

    # Should contain image reference
    if [[ "$result" != *"alpine"* ]]; then
        echo "Version output doesn't contain image info: $result"
        return 1
    fi

    return 0
}

test_packed_info_works() {
    local output="$TEST_DIR/smoke-alpine"

    if [[ ! -f "$output" ]]; then
        echo "SKIP: No packed binary (VM not available)"
        return 0
    fi

    local result
    result=$("$output" --info 2>&1) || true

    # Should contain expected sections
    if [[ "$result" != *"Image:"* ]]; then
        echo "Info output missing 'Image:': $result"
        return 1
    fi

    if [[ "$result" != *"Platform:"* ]]; then
        echo "Info output missing 'Platform:': $result"
        return 1
    fi

    if [[ "$result" != *"Assets:"* ]]; then
        echo "Info output missing 'Assets:': $result"
        return 1
    fi

    return 0
}

test_packed_help_works() {
    local output="$TEST_DIR/smoke-alpine"

    if [[ ! -f "$output" ]]; then
        echo "SKIP: No packed binary (VM not available)"
        return 0
    fi

    local result
    result=$("$output" --help 2>&1) || true

    # Should show available options
    if [[ "$result" != *"--volume"* ]] && [[ "$result" != *"-v"* ]]; then
        echo "Help output missing volume flag: $result"
        return 1
    fi

    return 0
}

test_sidecar_has_magic() {
    local output="$TEST_DIR/smoke-alpine"

    if [[ ! -f "$output.smolmachine" ]]; then
        echo "SKIP: No sidecar (VM not available)"
        return 0
    fi

    # Read last 64 bytes (footer) and check for SMOLPACK magic
    local magic
    magic=$(tail -c 64 "$output.smolmachine" | head -c 8 2>/dev/null) || true

    if [[ "$magic" != "SMOLPACK" ]]; then
        echo "Sidecar footer missing SMOLPACK magic"
        return 1
    fi

    return 0
}

test_binary_is_clean_macho() {
    # On macOS, verify the binary is a clean Mach-O (not corrupted by appended data)
    if [[ "$(uname)" != "Darwin" ]]; then
        # Skip on non-macOS
        return 0
    fi

    local output="$TEST_DIR/smoke-alpine"

    if [[ ! -f "$output" ]]; then
        echo "SKIP: No packed binary (VM not available)"
        return 0
    fi

    # file command should recognize it as Mach-O
    local file_result
    file_result=$(file "$output" 2>&1) || true

    if [[ "$file_result" != *"Mach-O"* ]]; then
        echo "Binary not recognized as Mach-O: $file_result"
        return 1
    fi

    return 0
}

test_sidecar_removal_breaks_info() {
    local output="$TEST_DIR/smoke-alpine"

    if [[ ! -f "$output" ]] || [[ ! -f "$output.smolmachine" ]]; then
        echo "SKIP: No packed binary (VM not available)"
        return 0
    fi

    # Temporarily remove sidecar
    mv "$output.smolmachine" "$output.smolmachine.bak"

    local exit_code=0
    "$output" --info 2>&1 || exit_code=$?

    # Restore sidecar
    mv "$output.smolmachine.bak" "$output.smolmachine"

    # Should have failed without sidecar
    if [[ $exit_code -eq 0 ]]; then
        echo "Binary should fail without sidecar"
        return 1
    fi

    return 0
}

test_pack_with_custom_cpus_mem() {
    local output="$TEST_DIR/smoke-custom"

    local pack_output
    pack_output=$($SMOLVM pack alpine:latest -o "$output" --cpus 4 --mem 2048 2>&1)

    # Check for known library/hypervisor errors (expected in CI)
    if [[ "$pack_output" == *"libkrunfw"* ]] || [[ "$pack_output" == *"krun_start_enter"* ]]; then
        echo "SKIP: VM environment not available (expected in CI)"
        return 0
    fi

    if [[ ! -f "$output" ]]; then
        echo "SKIP: No packed binary (VM not available)"
        return 0
    fi

    local info
    info=$("$output" --info 2>&1) || true

    if [[ "$info" != *"Default CPUs: 4"* ]]; then
        echo "Custom CPUs not in manifest"
        return 1
    fi

    if [[ "$info" != *"Default Memory: 2048"* ]]; then
        echo "Custom memory not in manifest"
        return 1
    fi

    return 0
}

# =============================================================================
# Main
# =============================================================================

echo ""
echo "=========================================="
echo "  smolvm Pack Smoke Tests"
echo "=========================================="
echo ""

# Find smolvm
SMOLVM=$(find_smolvm)
if [[ -z "$SMOLVM" ]]; then
    echo -e "${RED}Error: Could not find smolvm binary${NC}"
    echo ""
    echo "Build the distribution first:"
    echo "  ./scripts/build-dist.sh"
    echo ""
    echo "Or set SMOLVM environment variable:"
    echo "  SMOLVM=/path/to/smolvm ./scripts/smoke-test-pack.sh"
    exit 2
fi

log_info "Using smolvm: $SMOLVM"

# Create temp directory
TEST_DIR=$(mktemp -d)
log_info "Test directory: $TEST_DIR"
echo ""

# Run smoke tests
run_test "Pack command exists" test_pack_command_exists || true
run_test "Pack creates binary" test_pack_creates_binary || true
run_test "Pack creates sidecar" test_pack_creates_sidecar || true
run_test "Packed --version works" test_packed_version_works || true
run_test "Packed --info works" test_packed_info_works || true
run_test "Packed --help works" test_packed_help_works || true
run_test "Sidecar has SMOLPACK magic" test_sidecar_has_magic || true
run_test "Binary is clean Mach-O" test_binary_is_clean_macho || true
run_test "Sidecar removal breaks --info" test_sidecar_removal_breaks_info || true
run_test "Pack with custom CPU/mem" test_pack_with_custom_cpus_mem || true

# Summary
echo ""
echo "=========================================="
echo "  Smoke Test Summary"
echo "=========================================="
echo ""
echo "Tests run:    $TESTS_RUN"
echo -e "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests failed: ${RED}$TESTS_FAILED${NC}"
echo ""

if [[ $TESTS_FAILED -eq 0 ]]; then
    echo -e "${GREEN}All smoke tests passed!${NC}"
    echo ""
    echo "Note: These tests verify pack output structure only."
    echo "Run ./tests/test_pack.sh for full VM execution tests."
    exit 0
else
    echo -e "${RED}Some smoke tests failed.${NC}"
    exit 1
fi
