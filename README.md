<p align="center">
  <img src="assets/logo.png" alt="smol machines" width="80">
</p>

<p align="center">
  <strong>smolvm</strong><br>
  Build and run portable, lightweight, self-contained virtual machines.<br>
  Local. Hardware-isolated. Single binary install. No daemon.

</p>

<p align="center">
  <a href="https://smolmachines.com">Website</a> &middot;
  <a href="https://smolmachines.com/sdk/">Docs</a> &middot;
  <a href="https://discord.gg/qhQ7FHZ2zd">Discord</a> &middot;
  <a href="https://github.com/smol-machines/smolvm/releases">Releases</a>
</p>

<p align="center">
  <a href="https://discord.gg/qhQ7FHZ2zd"><img src="https://img.shields.io/discord/1476071043574665346?label=Discord&logo=discord&logoColor=white&color=5865F2" alt="Discord"></a>
  <a href="https://github.com/smol-machines/smolvm/releases"><img src="https://img.shields.io/github/v/release/smol-machines/smolvm?label=Release" alt="Release"></a>
  <a href="https://github.com/smol-machines/smolvm/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="License"></a>
</p>

---

> **Alpha** — APIs may change. [Report issues](https://github.com/smol-machines/smolvm/issues) &middot; [Join Discord](https://discord.gg/qhQ7FHZ2zd)

## What is smolvm?

smolvm runs Linux microVMs on your machine. No daemon, no Docker, no cloud account.

Each workload gets its own virtual machine with a separate kernel. Your host filesystem, network, and credentials are isolated from the workload unless you explicitly share them.

- **<200ms boot** — hardware-accelerated via Hypervisor.framework (macOS) and KVM (Linux)
- **Single binary** — no daemon process, no container runtime to install
- **OCI compatible** — run standard container images inside microVMs
- **Portable artifacts** — pack workloads into self-contained `.smolmachine` executables
- **Embeddable** — Node.js and Python SDKs for programmatic VM management

## Install

Download from [GitHub Releases](https://github.com/smol-machines/smolvm/releases), or:

```bash
curl -sSL https://smolmachines.com/install.sh | bash
```

## Quick Start

```bash
# Run a container image in an isolated microVM
smolvm sandbox run --net alpine -- echo "hello from a microVM"

# Mount a host directory (explicit — host is protected by default)
smolvm sandbox run --net -v ./src:/workspace alpine -- ls /workspace

# Persistent microVM
smolvm microvm create --net myvm
smolvm microvm start myvm
smolvm microvm exec --name myvm -- apk add git
smolvm microvm exec --name myvm -it -- /bin/sh
smolvm microvm stop myvm

# Declarative configuration via Smolfile
smolvm microvm create myvm -s my-app.smolfile
smolvm microvm start myvm

# Pack into a portable executable
smolvm pack create alpine -o ./my-sandbox
./my-sandbox echo "hello"
```

## Comparison

|                     | Containers | Colima + krunkit | QEMU | Firecracker | Kata | smolvm |
|---------------------|------------|-----------------|------|-------------|------|--------|
| Isolation           | Namespace (shared kernel) | Namespace (inside 1 VM) | Separate VM | Separate VM | VM per container | **VM per workload** |
| Boot time           | ~100ms | ~seconds | ~15-30s | <125ms | ~500ms | **<200ms** |
| Architecture        | Daemon | Daemon (containerd in VM) | Process | Process | Runtime stack | **Library (libkrun)** |
| Per-workload VMs    | No | No (shared VM) | Yes | Yes | Yes | **Yes** |
| macOS native        | Via Docker VM | Yes (krunkit) | Yes | No | No | **Yes** |
| Embeddable SDK      | No | No | No | No | No | **Yes** |
| Portable artifacts  | Images (need daemon) | No | No | No | No | **`.smolmachine`** |

<details>
<summary>References</summary>

1. [Container isolation](https://www.docker.com/blog/understanding-docker-container-escapes/)
2. [Kata Containers](https://katacontainers.io/)
3. [containerd benchmark](https://github.com/containerd/containerd/issues/4482)
4. [QEMU boot time](https://wiki.qemu.org/Features/TCG)
5. [Firecracker](https://firecracker-microvm.github.io/)
6. [Kata boot time](https://github.com/kata-containers/kata-containers/issues/4292)
7. [Kata installation](https://github.com/kata-containers/kata-containers/blob/main/docs/install/README.md)
8. [Firecracker requires KVM](https://github.com/firecracker-microvm/firecracker/blob/main/docs/getting-started.md)
9. [Kata macOS support](https://github.com/kata-containers/kata-containers/issues/243)

</details>

## How It Works

[libkrun](https://github.com/containers/libkrun) VMM with [Hypervisor.framework](https://developer.apple.com/documentation/hypervisor) (macOS) or KVM (Linux). Container execution via [crun](https://github.com/containers/crun). No daemon process — the VMM is a library linked into the binary.

Custom kernel optimized for fast boot: [libkrunfw](https://github.com/smol-machines/libkrunfw)

## Platform Support

| Host | Guest | Requirements |
|------|-------|-------------|
| macOS Apple Silicon | arm64 Linux | macOS 11+ |
| macOS Intel | x86_64 Linux | macOS 11+ (untested) |
| Linux x86_64 | x86_64 Linux | KVM (`/dev/kvm`) |
| Linux aarch64 | aarch64 Linux | KVM (`/dev/kvm`) |

## Known Limitations

- **Network is opt-in for sandboxes** — `--net` enables outbound networking for `sandbox run` and `sandbox create`. The default microVM (`smolvm microvm start`) has networking enabled. TCP/UDP only, no ICMP.
- **Volume mounts** — directories only (no single files)
- **macOS** — binary must be signed with Hypervisor.framework entitlements

## Development

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for build instructions, test suites, and contribution guidelines.

## Community

- [Discord](https://discord.gg/qhQ7FHZ2zd) — questions, feedback, collaboration
- [GitHub Issues](https://github.com/smol-machines/smolvm/issues) — bug reports and feature requests
- [Twitter](https://x.com/binsquares) — updates

## License

[Apache-2.0](LICENSE)
