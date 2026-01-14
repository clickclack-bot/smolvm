# smolvm

OCI-native microVM runtime. Run containers in lightweight VMs using [libkrun](https://github.com/containers/libkrun).

> **Alpha** - APIs may change. Not for production.

## Quick Start

### Prerequisites (macOS)

```bash
brew install libkrun@1.15.1 libkrunfw buildah
```

### Build

```bash
# Clone and build
git clone https://github.com/smolvm/smolvm.git
cd smolvm

# Build agent (cross-compile for Linux guest)
./scripts/build-agent-rootfs.sh

# Build smolvm
cargo build --release

# Sign binary (required for Hypervisor.framework)
codesign --entitlements smolvm.entitlements --force -s - ./target/release/smolvm
```

### Test

```bash
# Set library path
export DYLD_LIBRARY_PATH=$PWD/lib

# Basic test
./target/release/smolvm run alpine:latest echo "Hello World"

# With network
./target/release/smolvm run --net alpine:latest wget -qO- ifconfig.me

# With volume mount
mkdir -p /tmp/test && echo "hello" > /tmp/test/file.txt
./target/release/smolvm run -v /tmp/test:/data alpine:latest cat /data/file.txt

# Agent mode (faster for repeated commands)
./target/release/smolvm exec alpine:latest echo "Fast"
./target/release/smolvm exec alpine:latest ls /
./target/release/smolvm agent stop
```

## Usage

### Ephemeral Run

```bash
smolvm run [OPTIONS] <IMAGE> [COMMAND]

# Examples
smolvm run alpine:latest                           # Interactive shell
smolvm run --memory 1024 --cpus 2 ubuntu:22.04     # Custom resources
smolvm run --net alpine:latest ping -c1 google.com # With network
smolvm run -e FOO=bar alpine:latest env            # Environment vars
smolvm run -v /host/path:/guest/path alpine:latest # Volume mount
```

### Agent Mode (Persistent VM)

```bash
smolvm exec [OPTIONS] <IMAGE> [COMMAND]

# First call starts agent VM (~2s), subsequent calls are fast (~50ms)
smolvm exec alpine:latest echo "First"   # Starts agent
smolvm exec alpine:latest echo "Second"  # Reuses agent
smolvm exec -v ~/project:/workspace node:latest npm test

# Manage agent
smolvm agent status
smolvm agent stop
```

### Persistent VMs

```bash
smolvm create --name myvm alpine:latest /bin/sh
smolvm start myvm
smolvm list
smolvm stop myvm
smolvm delete myvm
```

## Options

| Flag | Description |
|------|-------------|
| `--memory <MiB>` | Memory (default: 512) |
| `--cpus <N>` | vCPUs (default: 1) |
| `--net` | Enable network egress |
| `--dns <IP>` | Custom DNS server |
| `-e KEY=VAL` | Environment variable |
| `-v host:guest[:ro]` | Volume mount (directories only) |
| `-w /path` | Working directory |

## Troubleshooting

```bash
# Enable debug logging
RUST_LOG=debug ./target/release/smolvm run alpine:latest

# Check agent logs
cat ~/Library/Caches/smolvm/agent-console.log

# Kill stuck agent
smolvm agent stop
pkill -9 -f krun
```

## Limitations

- Volume mounts must be directories (virtiofs limitation)
- No port forwarding yet
- No x86 emulation on ARM Macs

## License

MIT
