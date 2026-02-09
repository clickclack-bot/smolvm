//! `run-packed` subcommand: run a VM from a `.smolmachine` sidecar file.
//!
//! This allows the main `smolvm` binary to act as a sidecar runner, booting
//! a VM from extracted assets using the dlopen-based libkrun launcher and
//! reusing the existing `AgentClient` for all I/O.

use crate::cli::parsers::{parse_env_spec, parse_mounts, parse_port};
use clap::Args;
use smolvm::agent::launcher_dynamic::{
    launch_agent_vm_dynamic, KrunFunctions, PackedLaunchConfig, PackedMount,
};
use smolvm::agent::{mount_tag, AgentClient, PortMapping, RunConfig, VmResources};
use smolvm::Error;
use smolvm_pack::extract;
use smolvm_pack::packer::{
    read_footer_from_sidecar, read_manifest_from_sidecar, verify_sidecar_checksum,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Timeout waiting for the agent to become ready.
const AGENT_READY_TIMEOUT: Duration = Duration::from_secs(30);

/// Run a VM from a packed `.smolmachine` sidecar file.
///
/// Extracts runtime assets (if not already cached), boots a VM using
/// dynamically loaded libkrun, and executes a command using the full
/// smolvm agent infrastructure.
///
/// Examples:
///   smolvm run-packed -- echo hello
///   smolvm run-packed --sidecar my-app.smolmachine -it -- /bin/sh
///   smolvm run-packed -p 8080:80 --net
#[derive(Args, Debug)]
pub struct RunPackedCmd {
    /// Path to the `.smolmachine` sidecar file.
    ///
    /// If not specified, looks for `<exe_name>.smolmachine` next to the
    /// smolvm binary, or any `.smolmachine` file in the current directory.
    #[arg(long, value_name = "PATH")]
    pub sidecar: Option<PathBuf>,

    /// Command and arguments to run (default: image entrypoint)
    #[arg(trailing_var_arg = true, value_name = "COMMAND")]
    pub command: Vec<String>,

    /// Keep stdin open for interactive input
    #[arg(short = 'i', long, help_heading = "Execution")]
    pub interactive: bool,

    /// Allocate a pseudo-TTY (use with -i for interactive shells)
    #[arg(short = 't', long, help_heading = "Execution")]
    pub tty: bool,

    /// Kill command after duration (e.g., "30s", "5m")
    #[arg(
        long,
        value_parser = crate::cli::parsers::parse_duration,
        value_name = "DURATION",
        help_heading = "Execution"
    )]
    pub timeout: Option<Duration>,

    /// Set working directory inside container
    #[arg(short = 'w', long, value_name = "DIR", help_heading = "Container")]
    pub workdir: Option<String>,

    /// Set environment variable (can be used multiple times)
    #[arg(
        short = 'e',
        long = "env",
        value_name = "KEY=VALUE",
        help_heading = "Container"
    )]
    pub env: Vec<String>,

    /// Mount host directory into container (can be used multiple times)
    #[arg(
        short = 'v',
        long = "volume",
        value_name = "HOST:CONTAINER[:ro]",
        help_heading = "Container"
    )]
    pub volume: Vec<String>,

    /// Expose port from container to host (can be used multiple times)
    #[arg(
        short = 'p',
        long = "port",
        value_parser = parse_port,
        value_name = "HOST:GUEST",
        help_heading = "Network"
    )]
    pub port: Vec<PortMapping>,

    /// Enable outbound network access
    #[arg(long, help_heading = "Network")]
    pub net: bool,

    /// Number of virtual CPUs (overrides manifest default)
    #[arg(long, value_name = "N", help_heading = "Resources")]
    pub cpus: Option<u8>,

    /// Memory allocation in MiB (overrides manifest default)
    #[arg(long, value_name = "MiB", help_heading = "Resources")]
    pub mem: Option<u32>,

    /// Re-extract assets even if already cached
    #[arg(long)]
    pub force_extract: bool,

    /// Show manifest info and exit
    #[arg(long)]
    pub info: bool,

    /// Enable debug output
    #[arg(long)]
    pub debug: bool,
}

impl RunPackedCmd {
    /// Execute the run-packed command.
    pub fn run(self) -> smolvm::Result<()> {
        // 1. Resolve sidecar path
        let sidecar_path = resolve_sidecar_path(self.sidecar.as_deref())?;

        if self.debug {
            eprintln!("debug: using sidecar: {}", sidecar_path.display());
        }

        // 2. Read footer and verify checksum before trusting any content
        let footer = read_footer_from_sidecar(&sidecar_path)
            .map_err(|e| Error::agent("read footer", e.to_string()))?;

        match verify_sidecar_checksum(&sidecar_path, &footer) {
            Ok(true) => {
                if self.debug {
                    eprintln!("debug: sidecar checksum verified ({:08x})", footer.checksum);
                }
            }
            Ok(false) => {
                return Err(Error::agent(
                    "verify sidecar",
                    format!(
                        "checksum mismatch for {}: sidecar may be corrupt or tampered with.\n\
                         Try re-packing the image with `smolvm pack`.",
                        sidecar_path.display()
                    ),
                ));
            }
            Err(e) => {
                return Err(Error::agent(
                    "verify sidecar",
                    format!("failed to verify checksum: {}", e),
                ));
            }
        }

        // 3. Read manifest (safe now that checksum is verified)
        let manifest = read_manifest_from_sidecar(&sidecar_path)
            .map_err(|e| Error::agent("read manifest", e.to_string()))?;

        // 4. Handle --info: show manifest and exit
        if self.info {
            println!("Image:      {}", manifest.image);
            println!("Digest:     {}", manifest.digest);
            println!("Platform:   {}", manifest.platform);
            println!("CPUs:       {}", manifest.cpus);
            println!("Memory:     {} MiB", manifest.mem);
            if !manifest.entrypoint.is_empty() {
                println!("Entrypoint: {}", manifest.entrypoint.join(" "));
            }
            if !manifest.cmd.is_empty() {
                println!("Cmd:        {}", manifest.cmd.join(" "));
            }
            if let Some(ref wd) = manifest.workdir {
                println!("Workdir:    {}", wd);
            }
            if !manifest.env.is_empty() {
                println!("Env:");
                for e in &manifest.env {
                    println!("  {}", e);
                }
            }
            println!("Checksum:   {:08x}", footer.checksum);
            return Ok(());
        }

        // 5. Extract assets to cache (locked to prevent concurrent extraction races)
        let cache_dir = extract::get_cache_dir(footer.checksum)
            .map_err(|e| Error::agent("get cache dir", e.to_string()))?;

        extract::extract_sidecar(
            &sidecar_path,
            &cache_dir,
            &footer,
            self.force_extract,
            self.debug,
        )
        .map_err(|e| Error::agent("extract assets", e.to_string()))?;

        // 6. Set up paths — use a unique runtime directory per invocation so
        //    concurrent runs of the same checksum don't conflict on
        //    storage.ext4 / agent.sock.  tempdir_in gives us a truly unique
        //    directory that survives PID reuse and abrupt termination.
        let rootfs_path = cache_dir.join("agent-rootfs");
        let lib_dir = cache_dir.join("lib");
        let layers_dir = cache_dir.join("layers");
        let runtime_parent = cache_dir.join("runtime");
        std::fs::create_dir_all(&runtime_parent)
            .map_err(|e| Error::agent("create runtime parent", e.to_string()))?;
        let runtime_dir = tempfile::tempdir_in(&runtime_parent)
            .map_err(|e| Error::agent("create runtime dir", e.to_string()))?;

        let storage_path = runtime_dir.path().join("storage.ext4");
        let vsock_path = runtime_dir.path().join("agent.sock");

        // Create storage disk (each invocation gets its own copy)
        let template = manifest
            .assets
            .storage_template
            .as_ref()
            .map(|t| t.path.as_str());
        extract::create_or_copy_storage_disk(&cache_dir, template, &storage_path)
            .map_err(|e| Error::agent("create storage disk", e.to_string()))?;

        // 7. Parse CLI args
        let mounts = parse_mounts(&self.volume)?;
        let port_mappings: Vec<(u16, u16)> = self.port.iter().map(|p| (p.host, p.guest)).collect();

        let resources = VmResources {
            cpus: self.cpus.unwrap_or(manifest.cpus),
            mem: self.mem.unwrap_or(manifest.mem),
            network: self.net || !self.port.is_empty(),
        };

        // Build packed mounts for the launcher
        let packed_mounts: Vec<PackedMount> = mounts
            .iter()
            .enumerate()
            .map(|(i, m)| PackedMount {
                tag: mount_tag(i),
                host_path: m.source.to_string_lossy().to_string(),
                guest_path: m.target.to_string_lossy().to_string(),
                read_only: m.read_only,
            })
            .collect();

        if self.debug {
            eprintln!("debug: rootfs={}", rootfs_path.display());
            eprintln!("debug: lib_dir={}", lib_dir.display());
            eprintln!("debug: storage={}", storage_path.display());
            eprintln!("debug: vsock={}", vsock_path.display());
            eprintln!(
                "debug: resources cpus={} mem={} net={}",
                resources.cpus, resources.mem, resources.network
            );
        }

        // 8. Fork child → launch VM with dynamically loaded libkrun
        smolvm::process::install_sigchld_handler();

        let vsock_path_clone = vsock_path.clone();
        let child_pid = smolvm::process::fork_session_leader(move || {
            // Child process: load libkrun via dlopen and launch VM
            let krun = match unsafe { KrunFunctions::load(&lib_dir) } {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("failed to load libkrun: {}", e);
                    smolvm::process::exit_child(1);
                }
            };

            let config = PackedLaunchConfig {
                rootfs_path: &rootfs_path,
                storage_path: &storage_path,
                vsock_socket: &vsock_path_clone,
                layers_dir: &layers_dir,
                mounts: &packed_mounts,
                port_mappings: &port_mappings,
                resources,
                debug: self.debug,
            };

            if let Err(e) = launch_agent_vm_dynamic(&krun, &config) {
                eprintln!("VM launch failed: {}", e);
            }

            smolvm::process::exit_child(1);
        })
        .map_err(|e| Error::agent("fork VM process", e.to_string()))?;

        // Capture the child's start time so we can verify PID identity
        // later (guards against PID reuse).  The proc info may not be
        // available on the very first try if the kernel hasn't finished
        // setting up the child, so retry briefly.
        let child_start_time = {
            let mut st = smolvm::process::process_start_time(child_pid);
            if st.is_none() && smolvm::process::is_alive(child_pid) {
                for _ in 0..5 {
                    std::thread::sleep(Duration::from_millis(1));
                    st = smolvm::process::process_start_time(child_pid);
                    if st.is_some() {
                        break;
                    }
                }
            }
            // If the child is alive but we still can't get its start
            // time, we have no way to safely verify PID identity later.
            // Kill it now (we KNOW it's our child — we just forked it)
            // rather than risk either an orphan or a misidentified kill.
            if st.is_none() && smolvm::process::is_alive(child_pid) {
                let _ = smolvm::process::stop_process_fast(child_pid, Duration::from_secs(5), true);
                // Clean up runtime dir ourselves since the guard won't
                // be created.
                let _ = std::fs::remove_dir_all(runtime_dir.path());
                return Err(Error::agent(
                    "verify child process",
                    "unable to capture child start time for safe lifecycle management",
                ));
            }
            st
        };

        if self.debug {
            eprintln!("debug: forked VM process with PID {}", child_pid);
        }

        // Guard ensures the VM child is terminated and runtime dir is
        // cleaned up on every exit path — including ? propagation,
        // panics, AND the explicit process::exit() on the success path
        // (which skips Rust destructors, so we must drop manually).
        // start_time is guaranteed Some when the child is alive (enforced
        // above), so is_our_process_strict always has data to verify.
        let child_guard = ChildGuard {
            pid: child_pid,
            start_time: child_start_time,
            runtime_dir,
        };

        // 9. Parent: wait for agent, connect, execute command
        let mut client = wait_for_agent(&vsock_path, self.debug)?;

        let exit_code = execute_command(&mut client, &manifest, &self, &mounts)?;

        // std::process::exit skips destructors, so drop explicitly first.
        drop(child_guard);
        std::process::exit(exit_code);
    }
}

/// RAII guard that terminates the VM child process and cleans up the
/// per-invocation runtime directory on drop.
struct ChildGuard {
    pid: libc::pid_t,
    start_time: Option<u64>,
    runtime_dir: tempfile::TempDir,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        // Only signal the child if we can verify it's still ours via
        // start_time.  When start_time is None (child exited before we
        // could query it), we skip signaling entirely — the PID may have
        // been recycled and we must not target an unrelated process.
        if smolvm::process::is_our_process_strict(self.pid, self.start_time) {
            let _ = smolvm::process::stop_process_fast(self.pid, Duration::from_secs(5), true);
        }
        // TempDir removes itself on drop, but we also do an explicit
        // remove to handle partially-cleaned states.
        let _ = std::fs::remove_dir_all(self.runtime_dir.path());
    }
}

/// Resolve the path to the `.smolmachine` sidecar file.
fn resolve_sidecar_path(explicit: Option<&Path>) -> smolvm::Result<PathBuf> {
    // Explicit path
    if let Some(path) = explicit {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        return Err(Error::agent(
            "find sidecar",
            format!(
                "sidecar file not found: {}\nSpecify with --sidecar PATH",
                path.display()
            ),
        ));
    }

    // Try next to the executable: <exe>.smolmachine
    if let Ok(exe) = std::env::current_exe() {
        let sidecar = smolvm_pack::sidecar_path_for(&exe);
        if sidecar.exists() {
            return Ok(sidecar);
        }
    }

    // Try any .smolmachine file in the current directory
    if let Ok(cwd) = std::env::current_dir() {
        let entries: Vec<_> = std::fs::read_dir(&cwd)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "smolmachine"))
            .collect();

        if entries.len() == 1 {
            return Ok(entries[0].path());
        }
        if entries.len() > 1 {
            return Err(Error::agent(
                "find sidecar",
                "multiple .smolmachine files in current directory. Specify with --sidecar PATH"
                    .to_string(),
            ));
        }
    }

    Err(Error::agent(
        "find sidecar",
        "no .smolmachine sidecar file found.\n\
         Specify with: smolvm run-packed --sidecar PATH",
    ))
}

/// Wait for the agent to become ready on the vsock socket.
fn wait_for_agent(vsock_path: &Path, debug: bool) -> smolvm::Result<AgentClient> {
    use std::thread;
    use std::time::Instant;

    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        if start.elapsed() > AGENT_READY_TIMEOUT {
            return Err(Error::agent(
                "wait for agent",
                format!(
                    "agent did not become ready within {} seconds",
                    AGENT_READY_TIMEOUT.as_secs()
                ),
            ));
        }

        if vsock_path.exists() {
            match AgentClient::connect(vsock_path) {
                Ok(client) => {
                    if debug {
                        eprintln!(
                            "debug: agent ready after {:.1}s",
                            start.elapsed().as_secs_f64()
                        );
                    }
                    return Ok(client);
                }
                Err(_) => {
                    // Socket exists but agent not ready yet
                }
            }
        }

        thread::sleep(poll_interval);
    }
}

/// Build the command to execute from manifest defaults and CLI overrides.
fn build_command(manifest: &smolvm_pack::PackManifest, cli_command: &[String]) -> Vec<String> {
    if !cli_command.is_empty() {
        return cli_command.to_vec();
    }

    // Use manifest entrypoint + cmd
    let mut cmd = manifest.entrypoint.clone();
    cmd.extend(manifest.cmd.clone());

    if cmd.is_empty() {
        vec!["/bin/sh".to_string()]
    } else {
        cmd
    }
}

/// Build environment variables from manifest defaults and CLI overrides.
fn build_env(manifest: &smolvm_pack::PackManifest, cli_env: &[String]) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = manifest
        .env
        .iter()
        .filter_map(|e| parse_env_spec(e))
        .collect();

    // CLI env overrides manifest env
    for spec in cli_env {
        if let Some((key, value)) = parse_env_spec(spec) {
            // Remove existing key if present
            env.retain(|(k, _)| k != &key);
            env.push((key, value));
        }
    }

    env
}

/// Execute the command in the VM using the existing AgentClient.
fn execute_command(
    client: &mut AgentClient,
    manifest: &smolvm_pack::PackManifest,
    args: &RunPackedCmd,
    mounts: &[smolvm::vm::config::HostMount],
) -> smolvm::Result<i32> {
    let command = build_command(manifest, &args.command);
    let env = build_env(manifest, &args.env);

    // Convert mounts to virtiofs binding format
    let mount_bindings: Vec<(String, String, bool)> = mounts
        .iter()
        .enumerate()
        .map(|(i, m)| {
            (
                mount_tag(i),
                m.target.to_string_lossy().to_string(),
                m.read_only,
            )
        })
        .collect();

    let workdir = args.workdir.clone().or_else(|| manifest.workdir.clone());

    if args.interactive || args.tty {
        let config = RunConfig::new(&manifest.image, command)
            .with_env(env)
            .with_workdir(workdir)
            .with_mounts(mount_bindings)
            .with_timeout(args.timeout)
            .with_tty(args.tty);
        client.run_interactive(config)
    } else {
        let (exit_code, stdout, stderr) = client.run_with_mounts_and_timeout(
            &manifest.image,
            command,
            env,
            workdir,
            mount_bindings,
            args.timeout,
        )?;

        if !stdout.is_empty() {
            print!("{}", stdout);
        }
        if !stderr.is_empty() {
            eprint!("{}", stderr);
        }
        crate::cli::flush_output();
        Ok(exit_code)
    }
}
