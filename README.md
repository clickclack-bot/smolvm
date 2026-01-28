# 🤏 smolVM
Open source software to enable usage of microVMs for cross platform sandboxing Agentic and/or containerized workloads with minimal setup.

Note: MicroVMs are lightweight virtual machines with security & isolation provided by hardware virtualization with the speed of containers.

> **Alpha** - APIs can change, there may be bugs. Please submit a report or contribute!

## Mission

MicroVMs are used to power much of the internet by hyperscalers for services like AWS Lambda. 

However, it is also inaccessible to the average developer's workflow due to setup and configuration complexity.

smolVM works to make microVM more accessible for the general developer to take advantage of microVM's strong points in fast coldstarts <250ms, security, and isolation with generally good defaults.

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
# Quick install (macOS Apple Silicon only for now)
curl -sSL https://smolmachines.com/install.sh | bash

# Uninstall
curl -sSL https://smolmachines.com/install.sh | bash -s -- --uninstall

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
- Linux kernel 5.4+ with KVM support
- User must have access to `/dev/kvm` (typically via `kvm` group)
- e2fsprogs (usually pre-installed)

**Linux KVM Setup:**
```bash
# Check if KVM is available
ls -la /dev/kvm

# If /dev/kvm doesn't exist, load the KVM modules:
sudo modprobe kvm
sudo modprobe kvm_intel  # For Intel CPUs
# OR
sudo modprobe kvm_amd    # For AMD CPUs

# Grant your user access to KVM (re-login required):
sudo usermod -aG kvm $USER

# Verify after re-login:
groups | grep kvm
```

## Usage

```bash
# Quick sandbox (ephemeral)
smolvm sandbox run alpine:latest -- echo "Hello"
smolvm sandbox run -v /tmp:/workspace alpine:latest -- ls /workspace

# MicroVM management
smolvm microvm start                              # Start default VM
smolvm microvm exec -- echo "Hello"               # Execute command
smolvm microvm exec -- cat /etc/os-release        # Check OS
smolvm microvm stop

# Named VMs
smolvm microvm create myvm
smolvm microvm start myvm
smolvm microvm exec myvm -- echo "Hello"
smolvm microvm stop myvm
smolvm microvm delete myvm

# Server mode (HTTP API)
smolvm serve                          # localhost:8080
smolvm serve --listen 0.0.0.0:9000    # Custom address

# Pack into distributable binary
smolvm pack alpine:latest -o ./my-sandbox
./my-sandbox echo "Hello"             # Run command
./my-sandbox start                    # Start daemon
./my-sandbox exec echo "Fast"         # Fast exec (~10-20ms)
./my-sandbox stop                     # Stop daemon
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

Package your application into a **distributable binary** with the entire microVM environment embedded.
No dependencies required on the target machine - no Docker, no smolvm, no e2fsprogs.

**Output files:**
```
./my-app              # Executable stub (~1.7MB)
./my-app.smolmachine  # Assets: libs, rootfs, layers (~15-75MB)
```
Keep both files together when distributing.


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
smolvm pack alpine:latest -o ./my-sandbox

# Distribute to users - they just run it (no Docker, no smolvm install needed)
./my-sandbox echo "Hello from isolated microVM"
./my-sandbox -v /tmp:/workspace ls /workspace
```

**What's inside the .smolmachine sidecar:**
- Linux microkernel (libkrunfw)
- Hypervisor interface (libkrun)
- Container runtime (crun)
- Your OCI image layers
- smolvm agent rootfs
- Pre-formatted storage disk (no mkfs.ext4 needed)

**For coding agents - daemon mode with ~10-20ms exec:**

```bash
# Start the VM daemon (boots once, stays running)
./my-sandbox start

# Fast repeated execution (~10-20ms each, not ~500ms)
./my-sandbox exec echo "command 1"
./my-sandbox exec echo "command 2"
./my-sandbox exec ls /

# Check status
./my-sandbox status
# → Daemon running (pid: 12345, uptime: 60s)

# Stop when done
./my-sandbox stop
```

This is ideal for AI coding agents that need to execute many commands in isolated sandboxes with low latency.

## Comparison

|                     | Containers | QEMU (VM) | Firecracker | Kata | smolvm |
|---------------------|------------|-----------|-------------|------|--------|
| Kernel isolation    | Shared with host ¹ | Separate | Separate | Separate | Separate |
| Boot time           | ~100ms ² | ~15-30s ³ | <125ms ⁴ | ~500ms ⁵ | <250ms |
| Setup               | Easy | Complex | Complex | Complex | Easy |
| Linux               | Yes | Yes | Yes | Yes | Yes |
| macOS               | Via Docker VM | Yes | No ⁶ | No ⁷ | Yes |
| Guest rootfs        | Layered images | Disk image | DIY ⁸ | Bundled + DIY | Bundled |
| Embeddable          | No | No | No | No | Yes |
| Distribution        | Daemon + CLI ⁹ | Multiple binaries | Binary + rootfs | Runtime stack ¹⁰ | Single binary |

<details>
<summary>References</summary>

1. [Container isolation fundamentals](https://www.docker.com/blog/understanding-docker-container-escapes/)
2. [containerd vs dockerd benchmark](https://github.com/containerd/containerd/issues/4482)
3. [QEMU boot time](https://wiki.qemu.org/Features/TCG)
4. [Firecracker website](https://firecracker-microvm.github.io/)
5. [Kata boot time](https://github.com/kata-containers/kata-containers/issues/4292)
6. [Firecracker requires KVM](https://github.com/firecracker-microvm/firecracker/blob/main/docs/getting-started.md)
7. [Kata macOS support](https://github.com/kata-containers/kata-containers/issues/243)
8. [Firecracker rootfs setup](https://github.com/firecracker-microvm/firecracker/blob/main/docs/rootfs-and-kernel-setup.md)
9. [Docker daemon docs](https://docs.docker.com/config/daemon/)
10. [Kata installation guide](https://github.com/kata-containers/kata-containers/blob/main/docs/install/README.md)

</details>

smolvm is designed for dev machines - easy setup, single binary distribution, hardware-level isolation.

## Platform Support

| Host | Guest | Status |
|------|-------|--------|
| macOS Apple Silicon | arm64 Linux | ✅ |
| macOS Apple Silicon | x86_64 Linux | WIP (Rosetta 2, experimental) |
| macOS Intel | x86_64 Linux | Untested |
| Linux x86_64 | x86_64 Linux | ✅ |
| Linux aarch64 | aarch64 Linux | ✅ |

## Known Limitations

- **Container rootfs writes**: Writes to container filesystem (`/tmp`, `/home`, etc.) fail with "Connection reset by network" due to a libkrun TSI bug with overlayfs. **Writes to mounted volumes work** - see below.
- **Network: TCP/UDP only**: TSI (Transparent Socket Impersonation) only supports TCP and UDP sockets. ICMP (`ping`) and raw sockets do not work. Use `wget`, `curl`, or other TCP-based tools to test connectivity.
- **Volume mounts**: Directories only (no single files)
- **Rosetta 2**: Required for x86_64 images on Apple Silicon (`softwareupdate --install-rosetta`)
- **macOS**: Binary must be signed with Hypervisor.framework entitlements

### Coding Agent File Writes

```bash
# Works: use top-level mount path like /workspace
smolvm sandbox run -v /tmp:/workspace alpine:latest -- sh -c "echo 'hello' > /workspace/out.txt"

# Fails: nested mount paths like /mnt/data trigger the bug
smolvm sandbox run -v /tmp:/mnt/data alpine:latest -- sh -c "echo 'hello' > /mnt/data/out.txt"

# Fails: write to container rootfs
smolvm sandbox run alpine:latest -- sh -c "echo 'hello' > /tmp/out.txt"
```

Use top-level mount paths (`/workspace`, `/code`, `/data`) - nested paths like `/mnt/data` trigger a libkrun bug.

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
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm sandbox run alpine:latest -- echo "Hello"
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm sandbox run alpine:latest -- cat /etc/os-release

# Test with mounts
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm sandbox run -v /tmp:/workspace alpine:latest -- ls /workspace

# Test microvm lifecycle
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm microvm start
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm microvm exec -- echo "Hello"
DYLD_LIBRARY_PATH=$PWD/lib ./target/release/smolvm microvm stop
```

## AI Usage disclosure

AI was used to write code in this project.

I write code until the first working version. AI is then used to extend on my prototypes and refactor.

## Contributions

Please ensure to have human oversight before opening a PR, hence no totally AI generated PRs. Please run tests as well.

## License

Apache-2.0
