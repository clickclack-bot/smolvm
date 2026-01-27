# 🤏 smolVM
Open source software to enable usage of microVMs for cross platform sandboxing Agentic and/or containerized workloads with minimal setup.

Note: MicroVMs are lightweight virtual machines with security & isolation provided by hardware virtualization with the speed of containers.

> **Alpha** - APIs can change, there may be bugs. Please submit a report or contribute!

## Mission

MicroVMs are used to power much of the internet by hyperscalers for services like AWS Lambda. 

However, it is also inaccessible to the average developer's workflow due to setup and configuration complexity.

smolVM works to make microVM more accessible for the general developer to take advantage of microVM's strong points in fast coldstarts <200ms, security, and isolation with generally good defaults.

## What is smolVM?

smolVM is an microVM manager that orchestrates multiple components:

- **libkrun** - Lightweight VMM using Apple Hypervisor.framework (macOS) and KVM (Linux)
- **libkrunfw** - Minimal Linux kernel for microVMs.
- **crun** - OCI-compliant container runtime to run containers inside the VM
- **crane** - Pulls and extracts OCI images from registries
- **smolvm-agent** - Secret sauce, it's a daemon program inside of the guest that manages communication in/out of the microVM and initiates other workflows inside the guest. 
- **good logo** - 🤏 


```
┌─────────────────────────────────────────────┐
│ Host (macOS/Linux)                          │
│   smolvm CLI ──vsock──► smolVM            │
│                         ├─ smolvm-agent     │
│                         ├─ crun (container)│
│                         └─ /storage (ext4)  │
└─────────────────────────────────────────────┘
```

## You can use this to...

- run coding agents locally and safely
- run microVMs locally on both macOS and Linux with minimal setup
- run containers within microvm for improved isolation
- **distribute self-contained sandboxed applications**



## Install

```bash
# Quick install (macOS/Linux)
curl -sSL https://smolmachines.com/install.sh | sh

# macOS (Homebrew) - WIP
brew install smolvm/tap/smolvm

# From source
./scripts/build-dist.sh && ./scripts/install-local.sh
```

### Prerequisites

**macOS:**
- macOS 11.0 (Big Sur) or later
- For disk formatting: `brew install e2fsprogs`

**Linux:**
- KVM support (`/dev/kvm` must exist)
- e2fsprogs (usually pre-installed)

## Usage

```bash
# Quick sandbox (ephemeral)
smolvm sandbox run alpine:latest echo "Hello"
smolvm sandbox run -v ~/code:/code python:3.12 python /code/script.py

# MicroVM management
smolvm microvm run alpine:latest echo "Hello"      # Run and stop
smolvm microvm exec echo "Fast"                     # Persistent (~50ms warm)
smolvm microvm exec -it /bin/sh                     # Interactive shell
smolvm microvm stop

# Named VMs
smolvm microvm create --name web --cpus 2 --mem 512 node:20
smolvm microvm start web
smolvm microvm stop web
smolvm microvm ls
smolvm microvm delete web

# Containers inside VMs
smolvm container create myvm alpine -- sleep 300
smolvm container start myvm <id>
smolvm container exec myvm <id> -- ps aux
smolvm container stop myvm <id>
smolvm container ls myvm

# Server mode (HTTP API)
smolvm serve                          # localhost:8080
smolvm serve --listen 0.0.0.0:9000    # Custom address

# Pack into distributable binary
smolvm pack alpine:latest -o ./my-app
smolvm pack python:3.12 -o ./py-sandbox --cpus 2 --mem 1024

# Run packed binary (no smolvm needed)
./my-app echo "Hello"
./py-sandbox start                    # Daemon mode
./py-sandbox exec python script.py    # Fast exec (~10-20ms)
./py-sandbox stop
```

## Options

**Runtime flags:**

| Flag | Description |
|------|-------------|
| `-e KEY=VAL` | Environment variable |
| `-v host:guest[:ro]` | Volume mount (directories only) |
| `-w /path` | Working directory |
| `-p HOST:GUEST` | Port forwarding |
| `--cpus N` | vCPU count |
| `--mem N` | Memory (MiB) |
| `--net` | Enable network |
| `--timeout 30s` | Execution timeout |
| `-i` | Interactive (stdin) |
| `-t` | Allocate TTY |


## EXPERIMENTAL: Packed Binaries - Zero-Dependency Distribution

Package your application into a **single executable** with the entire microVM environment embedded:


**Pack command:**

| Flag | Description |
|------|-------------|
| `-o PATH` | Output binary path |
| `--cpus N` | Default vCPU count |
| `--mem N` | Default memory (MiB) |

**Packed binary subcommands:**

| Command | Description |
|---------|-------------|
| `./packed [cmd]` | Ephemeral run (boot, execute, exit) |
| `./packed start` | Start daemon VM |
| `./packed exec [cmd]` | Execute in daemon (~10-20ms) |
| `./packed stop` | Stop daemon VM |
| `./packed status` | Check daemon status |


```bash
# Create a self-contained binary
smolvm pack python:3.12 -o ./my-python-sandbox

# Distribute to users - they just run it (no Docker, no smolvm install needed)
./my-python-sandbox python -c "print('Hello from isolated microVM')"
./my-python-sandbox -v ~/code:/workspace python /workspace/script.py
```

**What's inside the packed binary:**
- Linux microkernel (libkrunfw)
- Hypervisor interface (libkrun)
- Container runtime (crun)
- Your OCI image layers
- smolvm agent

**For coding agents - daemon mode with ~10-20ms exec:**

```bash
# Start the VM daemon (boots once, stays running)
./my-agent start

# Fast repeated execution (~10-20ms each, not ~500ms)
./my-agent exec python -c "x = 1"
./my-agent exec python -c "print(x)"  # Fresh process, but VM is warm
./my-agent exec ls /workspace

# Check status
./my-agent status
# → Daemon running (pid: 12345, uptime: 60s)

# Stop when done
./my-agent stop
```

This is ideal for AI coding agents that need to execute many commands in isolated sandboxes with low latency.

## Compared to existing tools

| Tool | Local Dev | Single Binary | No Daemon | Hardware Isolation |
|------|-----------|---------------|-----------|-------------------|
| smolvm | ✅ | ✅ (pack) | ✅ | ✅ (microVM) |
| Docker | ✅ | ❌ | ❌ | ❌ (namespaces) |
| Firecracker | ❌ (server) | ❌ | ❌ | ✅ |
| gVisor | ✅ | ❌ | ❌ | ⚠️ (syscall filter) |

smolvm is designed for dev machines - easy setup, single binary distribution, hardware-level isolation.

## Platform Support

| Host | Guest | Status |
|------|-------|--------|
| macOS Apple Silicon | arm64 Linux | ✅ |
| macOS Apple Silicon | x86_64 Linux | WIP (Rosetta 2, [experimental]) |
| macOS Intel | x86_64 Linux | ? | No machine to test this.
| Linux x86_64 | x86_64 Linux | WIP | My machine needs repairs.

## Known Limitations

- **Container rootfs writes**: Writes to container filesystem (`/tmp`, `/home`, etc.) fail with "Connection reset by network" due to a libkrun TSI bug with overlayfs. **Writes to mounted volumes work** - see below.
- **Volume mounts**: Directories only (no single files)
- **Rosetta 2**: Required for x86_64 images on Apple Silicon (`softwareupdate --install-rosetta`)
- **macOS**: Binary must be signed with Hypervisor.framework entitlements

### Coding Agent File Writes

```bash
# Works: use top-level mount path like /workspace
smolvm sandbox run -v ~/code:/workspace python:3.12 -- python -c "open('/workspace/out.py', 'w').write('hello')"

# Fails: nested mount paths like /mnt/code trigger the bug
smolvm sandbox run -v ~/code:/mnt/code python:3.12 -- python -c "open('/mnt/code/out.py', 'w').write('hello')"

# Fails: write to container rootfs
smolvm sandbox run python:3.12 -- python -c "open('/tmp/out.py', 'w').write('hello')"
```

Use top-level mount paths (`/workspace`, `/code` ,`/documents/projectname`) - paths like `/mnt/host` which conflict with mounted paths alpine/your container will fail.

## Storage

OCI images and container overlays are stored in a sparse ext4 disk image:

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/smolvm/storage.raw` |
| Linux | `~/.local/share/smolvm/storage.raw` |

Default size is 20 GB (sparse - only uses actual written space). The ext4 filesystem inside the VM handles Linux case-sensitivity correctly, avoiding issues with macOS's case-insensitive filesystem.

## Development

### Running Tests Locally

```bash
# Format check
cargo fmt --all -- --check

# Clippy (warnings as errors)
cargo clippy --all-targets -- -D warnings

# Unit tests (requires libkrun in lib/)
DYLD_LIBRARY_PATH=$PWD/lib cargo test --lib   # macOS
LD_LIBRARY_PATH=$PWD/lib cargo test --lib     # Linux

# All checks in one command (macOS)
cargo fmt --all -- --check && \
  cargo clippy --all-targets -- -D warnings && \
  DYLD_LIBRARY_PATH=$PWD/lib cargo test --lib
```

### Integration Tests

Integration tests require actual VM execution, which needs:
- **macOS**: Hypervisor.framework access (binary must be signed with entitlements)
- **Linux**: KVM access (`/dev/kvm`)

GitHub CI cannot run these tests. Run them locally before releases:

```bash
# Build and sign (macOS)
cargo build --release
codesign --sign - --entitlements entitlements.plist --force target/release/smolvm

# Basic integration tests
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm microvm run alpine:latest echo "Hello from VM"
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm sandbox run alpine:latest -- ls /

# Test with mounts
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm sandbox run -v /tmp:/workspace alpine:latest -- ls /workspace

# Test container operations
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm microvm create --name test-vm
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm microvm start test-vm
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm microvm exec test-vm -- echo "Hello"
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm microvm stop test-vm
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm microvm delete test-vm
```

## AI Usage disclosure

AI was used to write code in this project.

I write code until the first working version. AI is then used to extend on my prototypes and refactor.

## Contributions

Please ensure to have human oversight before opening a PR, hence no totally AI generated PRs. Please run tests as well.

## License

Apache-2.0
