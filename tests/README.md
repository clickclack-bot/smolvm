# smolvm Integration Tests

This directory contains integration tests for smolvm, organized by functionality.

## Test Files

| File | Description | Requires VM |
|------|-------------|-------------|
| `test_cli.sh` | Basic CLI tests (--version, --help) | No |
| `test_sandbox.sh` | Sandbox run tests | Yes |
| `test_microvm.sh` | MicroVM lifecycle tests | Yes |
| `test_container.sh` | Container lifecycle tests | Yes |
| `integration_test.sh` | Legacy combined test script | Yes |

## Running Tests

### Run All Tests

```bash
./tests/run_all.sh
```

### Run Specific Test Suite

```bash
./tests/run_all.sh cli        # CLI tests only
./tests/run_all.sh sandbox    # Sandbox tests only
./tests/run_all.sh microvm    # MicroVM tests only
./tests/run_all.sh container  # Container tests only
```

### Run Individual Test Files

```bash
./tests/test_cli.sh
./tests/test_sandbox.sh
./tests/test_microvm.sh
./tests/test_container.sh
```

### Use Specific Binary

```bash
SMOLVM=/path/to/smolvm ./tests/run_all.sh
```

## Unit Tests

Unit tests are run via cargo:

```bash
# Protocol tests (no VM required)
cargo test -p smolvm-protocol

# Agent tests (no VM required)
cargo test -p smolvm-agent

# All unit tests
cargo test -p smolvm-protocol -p smolvm-agent
```

## Test Requirements

- **CLI tests**: Only require the smolvm binary
- **Sandbox/MicroVM/Container tests**: Require VM environment (macOS Hypervisor.framework or Linux KVM)

## Binary Discovery

Tests automatically look for the smolvm binary in:

1. `$SMOLVM` environment variable
2. `dist/smolvm-*-darwin-*/smolvm` or `dist/smolvm-*-linux-*/smolvm`
3. `target/release/smolvm`

## Common Utilities

The `common.sh` file provides shared test utilities:

- `find_smolvm` - Locate the smolvm binary
- `init_smolvm` - Initialize and validate the binary
- `run_test` - Run a test function with pass/fail tracking
- `ensure_microvm_running` - Start the default microvm
- `cleanup_microvm` - Stop the default microvm
- `extract_container_id` - Parse container ID from command output
- `cleanup_container` - Force remove a container
