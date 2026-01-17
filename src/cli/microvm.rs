//! MicroVM management commands.
//!
//! All VM-related commands are under the `microvm` subcommand:
//! - run: Ephemeral execution (stops microvm after)
//! - exec: Persistent execution (microvm keeps running)
//! - create: Create named VM configuration
//! - start: Start a microvm (named or default)
//! - stop: Stop a microvm (named or default)
//! - delete: Delete a named VM configuration
//! - status: Show microvm status
//! - ls: List all named VMs

use clap::{Args, Subcommand};
use smolvm::agent::{AgentClient, AgentManager, HostMount, PortMapping, VmResources};
use smolvm::config::{RecordState, SmolvmConfig, VmRecord};
use std::path::PathBuf;
use std::time::Duration;

/// Parse a duration string (e.g., "30s", "5m", "1h").
fn parse_duration(s: &str) -> Result<Duration, humantime::DurationError> {
    humantime::parse_duration(s)
}

/// Parse a port mapping specification (HOST:GUEST or PORT).
fn parse_port(s: &str) -> Result<PortMapping, String> {
    if let Some((host, guest)) = s.split_once(':') {
        let host: u16 = host
            .parse()
            .map_err(|_| format!("invalid host port: {}", host))?;
        let guest: u16 = guest
            .parse()
            .map_err(|_| format!("invalid guest port: {}", guest))?;
        Ok(PortMapping::new(host, guest))
    } else {
        let port: u16 = s.parse().map_err(|_| format!("invalid port: {}", s))?;
        Ok(PortMapping::same(port))
    }
}

/// Parse an environment variable specification (KEY=VALUE).
fn parse_env_spec(spec: &str) -> Option<(String, String)> {
    let (key, value) = spec.split_once('=')?;
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), value.to_string()))
}

/// Manage microvms
#[derive(Subcommand, Debug)]
pub enum MicrovmCmd {
    /// Run a command in a container (ephemeral - stops microvm after)
    Run(RunCmd),

    /// Execute a command in a microvm (persistent - microvm keeps running)
    Exec(ExecCmd),

    /// Create a named VM configuration without starting it
    Create(CreateCmd),

    /// Start a microvm (named or default)
    Start(StartCmd),

    /// Stop a microvm (named or default)
    Stop(StopCmd),

    /// Delete a named VM configuration
    #[command(alias = "rm")]
    Delete(DeleteCmd),

    /// Show microvm status
    Status(StatusCmd),

    /// List all named VMs
    #[command(alias = "list")]
    Ls(LsCmd),

    /// Test network connectivity (debug TSI)
    NetworkTest(NetworkTestCmd),
}

impl MicrovmCmd {
    pub fn run(self) -> smolvm::Result<()> {
        match self {
            MicrovmCmd::Run(cmd) => cmd.run(),
            MicrovmCmd::Exec(cmd) => cmd.run(),
            MicrovmCmd::Create(cmd) => cmd.run(),
            MicrovmCmd::Start(cmd) => cmd.run(),
            MicrovmCmd::Stop(cmd) => cmd.run(),
            MicrovmCmd::Delete(cmd) => cmd.run(),
            MicrovmCmd::Status(cmd) => cmd.run(),
            MicrovmCmd::Ls(cmd) => cmd.run(),
            MicrovmCmd::NetworkTest(cmd) => cmd.run(),
        }
    }
}

// ============================================================================
// Run Command (Ephemeral)
// ============================================================================

/// Run a command in a container (ephemeral - stops microvm after).
#[derive(Args, Debug)]
pub struct RunCmd {
    /// OCI image reference
    pub image: String,

    /// Command to execute inside the container
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,

    /// Number of vCPUs
    #[arg(long, default_value = "1")]
    pub cpus: u8,

    /// Memory in MiB
    #[arg(long, default_value = "512")]
    pub mem: u32,

    /// Working directory inside the container
    #[arg(short = 'w', long)]
    pub workdir: Option<String>,

    /// Environment variable (KEY=VALUE)
    #[arg(short = 'e', long = "env")]
    pub env: Vec<String>,

    /// Volume mount (host:container[:ro])
    #[arg(short = 'v', long = "volume")]
    pub volume: Vec<String>,

    /// Enable network egress (auto-enabled when -p is used)
    #[arg(long)]
    pub net: bool,

    /// Port mapping from host to guest (HOST:GUEST or PORT)
    #[arg(short = 'p', long = "port", value_parser = parse_port)]
    pub port: Vec<PortMapping>,

    /// Timeout for command execution (e.g., "30s", "5m")
    #[arg(long, value_parser = parse_duration)]
    pub timeout: Option<Duration>,

    /// Keep stdin open (interactive mode)
    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Allocate a pseudo-TTY
    #[arg(short = 't', long)]
    pub tty: bool,
}

impl RunCmd {
    pub fn run(self) -> smolvm::Result<()> {
        use smolvm::Error;
        use std::io::Write;

        // Parse volume mounts
        let mounts = parse_mounts(&self.volume)?;
        let ports = self.port.clone();

        let resources = VmResources {
            cpus: self.cpus,
            mem: self.mem,
        };

        // Start agent VM
        let manager = AgentManager::default()
            .map_err(|e| Error::AgentError(format!("failed to create agent manager: {}", e)))?;

        // Show startup message
        let mount_info = if !mounts.is_empty() {
            format!(" with {} mount(s)", mounts.len())
        } else {
            String::new()
        };
        let port_info = if !ports.is_empty() {
            format!(" and {} port mapping(s)", ports.len())
        } else {
            String::new()
        };
        println!("Starting microvm{}{}...", mount_info, port_info);

        manager
            .ensure_running_with_full_config(mounts.clone(), ports, resources)
            .map_err(|e| Error::AgentError(format!("failed to start microvm: {}", e)))?;

        // Connect to agent
        let mut client = AgentClient::connect(manager.vsock_socket())?;

        // Pull image
        println!("Pulling image {}...", self.image);
        client.pull(&self.image, None)?;

        // Build command
        let command = if self.command.is_empty() {
            vec!["/bin/sh".to_string()]
        } else {
            self.command.clone()
        };

        // Parse environment variables
        let env: Vec<(String, String)> =
            self.env.iter().filter_map(|e| parse_env_spec(e)).collect();

        // Convert mounts to agent format
        let mount_bindings: Vec<(String, String, bool)> = mounts
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    format!("smolvm{}", i),
                    m.target.to_string_lossy().to_string(),
                    m.read_only,
                )
            })
            .collect();

        // Run command
        let exit_code = if self.interactive || self.tty {
            client.run_interactive(
                &self.image,
                command,
                env,
                self.workdir.clone(),
                mount_bindings,
                self.timeout,
                self.tty,
            )?
        } else {
            let (exit_code, stdout, stderr) = client.run_with_mounts_and_timeout(
                &self.image,
                command,
                env,
                self.workdir.clone(),
                mount_bindings,
                self.timeout,
            )?;

            if !stdout.is_empty() {
                print!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().flush();
            exit_code
        };

        // Stop the microvm (ephemeral mode)
        if let Err(e) = manager.stop() {
            tracing::warn!(error = %e, "failed to stop microvm");
        }

        std::process::exit(exit_code);
    }
}

// ============================================================================
// Exec Command (Persistent) - Direct VM Execution
// ============================================================================

/// Execute a command directly in the VM (persistent - microvm keeps running).
///
/// Unlike `run`, this executes commands directly in the VM's Alpine rootfs,
/// not inside a container. This is useful for VM-level operations and debugging.
#[derive(Args, Debug)]
pub struct ExecCmd {
    /// Command to execute in the VM
    #[arg(trailing_var_arg = true, required = true)]
    pub command: Vec<String>,

    /// Named microvm to exec into
    #[arg(long)]
    pub name: Option<String>,

    /// Working directory in the VM
    #[arg(short = 'w', long)]
    pub workdir: Option<String>,

    /// Environment variable (KEY=VALUE)
    #[arg(short = 'e', long = "env")]
    pub env: Vec<String>,

    /// Timeout for command execution (e.g., "30s", "5m")
    #[arg(long, value_parser = parse_duration)]
    pub timeout: Option<Duration>,

    /// Keep stdin open (interactive mode)
    #[arg(short = 'i', long)]
    pub interactive: bool,

    /// Allocate a pseudo-TTY
    #[arg(short = 't', long)]
    pub tty: bool,
}

impl ExecCmd {
    pub fn run(self) -> smolvm::Result<()> {
        use std::io::Write;

        let manager = get_manager(&self.name)?;
        let label = microvm_label(&self.name);

        // Check if microvm is running, start if needed
        let was_running = manager.try_connect_existing().is_some();

        if !was_running {
            println!("Starting microvm '{}'...", label);
            manager.ensure_running()?;
        }

        // Connect to agent
        let mut client = AgentClient::connect(manager.vsock_socket())?;

        // Parse environment variables
        let env: Vec<(String, String)> =
            self.env.iter().filter_map(|e| parse_env_spec(e)).collect();

        // Run command directly in VM
        let exit_code = if self.interactive || self.tty {
            client.vm_exec_interactive(
                self.command.clone(),
                env,
                self.workdir.clone(),
                self.timeout,
                self.tty,
            )?
        } else {
            let (exit_code, stdout, stderr) = client.vm_exec(
                self.command.clone(),
                env,
                self.workdir.clone(),
                self.timeout,
            )?;

            if !stdout.is_empty() {
                print!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().flush();
            exit_code
        };

        // Keep microvm running (persistent)
        std::mem::forget(manager);
        std::process::exit(exit_code);
    }
}

// ============================================================================
// Create Command
// ============================================================================

/// Create a named VM configuration without starting it.
#[derive(Args, Debug)]
pub struct CreateCmd {
    /// OCI image reference
    pub image: String,

    /// Command to execute when the VM starts
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,

    /// VM name (required)
    #[arg(long)]
    pub name: String,

    /// Number of vCPUs
    #[arg(long, default_value = "1")]
    pub cpus: u8,

    /// Memory in MiB
    #[arg(long, default_value = "256")]
    pub mem: u32,

    /// Working directory inside the container
    #[arg(short = 'w', long)]
    pub workdir: Option<String>,

    /// Environment variable (KEY=VALUE)
    #[arg(short = 'e', long = "env")]
    pub env: Vec<String>,

    /// Volume mount (host:guest[:ro])
    #[arg(short = 'v', long = "volume")]
    pub volume: Vec<String>,
}

impl CreateCmd {
    pub fn run(self) -> smolvm::Result<()> {
        let mut config = SmolvmConfig::load().unwrap_or_default();

        // Check if VM already exists
        if config.get_vm(&self.name).is_some() {
            return Err(smolvm::Error::Config(format!(
                "VM '{}' already exists",
                self.name
            )));
        }

        // Parse environment variables
        let env: Vec<(String, String)> =
            self.env.iter().filter_map(|e| parse_env_spec(e)).collect();

        // Parse and validate volume mounts
        let mounts = parse_mounts_as_tuples(&self.volume)?;

        // Build command
        let command = if self.command.is_empty() {
            None
        } else {
            Some(self.command.clone())
        };

        // Create record
        let record = VmRecord::new(
            self.name.clone(),
            self.image.clone(),
            self.cpus,
            self.mem,
            command,
            self.workdir.clone(),
            env,
            mounts,
        );

        // Store in config
        config.vms.insert(self.name.clone(), record);
        config.save()?;

        println!("Created VM: {}", self.name);
        println!("  Image: {}", self.image);
        println!("  CPUs: {}, Memory: {} MiB", self.cpus, self.mem);
        if !self.volume.is_empty() {
            println!("  Mounts: {}", self.volume.len());
        }
        println!("\nUse 'smolvm microvm start {}' to start the VM", self.name);

        Ok(())
    }
}

// ============================================================================
// Start Command
// ============================================================================

/// Start a microvm (named or default anonymous).
#[derive(Args, Debug)]
pub struct StartCmd {
    /// Named VM to start (omit for default anonymous microvm)
    pub name: Option<String>,
}

impl StartCmd {
    pub fn run(self) -> smolvm::Result<()> {
        use smolvm::Error;
        use std::io::Write;

        // If no name provided, start default anonymous microvm
        if self.name.is_none() {
            return self.start_anonymous();
        }

        let name = self.name.as_ref().unwrap();
        let mut config = SmolvmConfig::load().unwrap_or_default();

        // Get VM record
        let record = config
            .get_vm(name)
            .ok_or_else(|| Error::VmNotFound(name.clone()))?
            .clone();

        // Check state
        let actual_state = record.actual_state();
        if actual_state == RecordState::Running {
            return Err(Error::InvalidState {
                expected: "created or stopped".to_string(),
                actual: "running".to_string(),
            });
        }

        // Convert stored mounts to HostMount
        let mounts: Vec<HostMount> = record
            .mounts
            .iter()
            .map(|(host, guest, ro)| HostMount {
                source: PathBuf::from(host),
                target: PathBuf::from(guest),
                read_only: *ro,
            })
            .collect();

        let resources = VmResources {
            cpus: record.cpus,
            mem: record.mem,
        };

        // Start agent VM for this named VM
        let manager = AgentManager::for_vm(name)
            .map_err(|e| Error::AgentError(format!("failed to create agent manager: {}", e)))?;

        if !mounts.is_empty() {
            println!("Starting VM '{}' with {} mount(s)...", name, mounts.len());
        } else {
            println!("Starting VM '{}'...", name);
        }

        manager
            .ensure_running_with_config(mounts.clone(), resources)
            .map_err(|e| Error::AgentError(format!("failed to start microvm: {}", e)))?;

        // Update state
        let pid = manager.child_pid();
        config.update_vm(name, |r| {
            r.state = RecordState::Running;
            r.pid = pid;
        });
        config.save()?;

        // Connect to agent
        let mut client = AgentClient::connect(manager.vsock_socket())?;

        // Pull image
        println!("Pulling image {}...", record.image);
        client.pull(&record.image, None)?;

        // Build command
        let command = record
            .command
            .clone()
            .unwrap_or_else(|| vec!["/bin/sh".to_string()]);

        // Convert mounts to agent format
        let mount_bindings: Vec<(String, String, bool)> = mounts
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    format!("smolvm{}", i),
                    m.target.to_string_lossy().to_string(),
                    m.read_only,
                )
            })
            .collect();

        // Run command
        let (exit_code, stdout, stderr) = client.run_with_mounts(
            &record.image,
            command,
            record.env.clone(),
            record.workdir.clone(),
            mount_bindings,
        )?;

        if !stdout.is_empty() {
            print!("{}", stdout);
        }
        if !stderr.is_empty() {
            eprint!("{}", stderr);
        }
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();

        // Update state to stopped
        config.update_vm(name, |r| {
            r.state = RecordState::Stopped;
            r.pid = None;
        });
        config.save()?;

        // Stop the agent
        if let Err(e) = manager.stop() {
            tracing::warn!(error = %e, "failed to stop microvm");
        }

        std::process::exit(exit_code);
    }

    fn start_anonymous(&self) -> smolvm::Result<()> {
        let manager = AgentManager::default()?;

        // Check if already running
        if manager.try_connect_existing().is_some() {
            let pid = manager
                .child_pid()
                .map(|p| format!(" (PID: {})", p))
                .unwrap_or_default();
            println!("MicroVM 'default' already running{}", pid);
            std::mem::forget(manager);
            return Ok(());
        }

        println!("Starting microvm 'default'...");
        manager.ensure_running()?;

        let pid = manager.child_pid().unwrap_or(0);
        println!("MicroVM 'default' running (PID: {})", pid);

        std::mem::forget(manager);
        Ok(())
    }
}

// ============================================================================
// Stop Command
// ============================================================================

/// Stop a microvm (named or default anonymous).
#[derive(Args, Debug)]
pub struct StopCmd {
    /// Named VM to stop (omit for default anonymous microvm)
    pub name: Option<String>,
}

impl StopCmd {
    pub fn run(self) -> smolvm::Result<()> {
        // If no name provided, stop default anonymous microvm
        if self.name.is_none() {
            return self.stop_anonymous();
        }

        let name = self.name.as_ref().unwrap();
        let mut config = SmolvmConfig::load().unwrap_or_default();

        // Get VM record
        let record = match config.get_vm(name) {
            Some(r) => r.clone(),
            None => {
                // Maybe it's a running anonymous microvm with this name?
                return self.stop_named_microvm(name);
            }
        };

        // Check state
        let actual_state = record.actual_state();
        if actual_state != RecordState::Running {
            println!("VM '{}' is not running (state: {})", name, actual_state);
            return Ok(());
        }

        println!("Stopping VM '{}'...", name);

        // Stop this VM's agent
        if let Ok(manager) = AgentManager::for_vm(name) {
            if let Err(e) = manager.stop() {
                tracing::warn!(error = %e, "failed to stop microvm");
            }
        }

        // Update state
        config.update_vm(name, |r| {
            r.state = RecordState::Stopped;
            r.pid = None;
        });
        config.save()?;

        println!("Stopped VM: {}", name);
        Ok(())
    }

    fn stop_anonymous(&self) -> smolvm::Result<()> {
        let manager = AgentManager::default()?;

        if manager.try_connect_existing().is_some() {
            println!("Stopping microvm 'default'...");
            manager.stop()?;
            println!("MicroVM 'default' stopped");
        } else {
            println!("MicroVM 'default' not running");
        }

        Ok(())
    }

    fn stop_named_microvm(&self, name: &str) -> smolvm::Result<()> {
        if let Ok(manager) = AgentManager::for_vm(name) {
            if manager.try_connect_existing().is_some() {
                println!("Stopping microvm '{}'...", name);
                manager.stop()?;
                println!("MicroVM '{}' stopped", name);
            } else {
                println!("MicroVM '{}' not running", name);
            }
        } else {
            println!("MicroVM '{}' not found", name);
        }
        Ok(())
    }
}

// ============================================================================
// Delete Command
// ============================================================================

/// Delete a named VM configuration.
#[derive(Args, Debug)]
pub struct DeleteCmd {
    /// VM name to delete
    pub name: String,

    /// Force deletion without confirmation
    #[arg(short, long)]
    pub force: bool,
}

impl DeleteCmd {
    pub fn run(&self) -> smolvm::Result<()> {
        let mut config = SmolvmConfig::load().unwrap_or_default();

        // Check if VM exists
        if config.get_vm(&self.name).is_none() {
            return Err(smolvm::Error::VmNotFound(self.name.clone()));
        }

        // Confirm deletion unless --force
        if !self.force {
            eprint!("Delete VM '{}'? [y/N] ", self.name);
            let mut input = String::new();
            if std::io::stdin().read_line(&mut input).is_ok() {
                let input = input.trim().to_lowercase();
                if input != "y" && input != "yes" {
                    println!("Cancelled");
                    return Ok(());
                }
            } else {
                println!("Cancelled");
                return Ok(());
            }
        }

        // Remove from config
        config.remove_vm(&self.name);
        config.save()?;

        println!("Deleted VM: {}", self.name);
        Ok(())
    }
}

// ============================================================================
// Status Command
// ============================================================================

/// Show microvm status.
#[derive(Args, Debug)]
pub struct StatusCmd {
    /// Named microvm to check (omit for default anonymous)
    pub name: Option<String>,
}

impl StatusCmd {
    pub fn run(self) -> smolvm::Result<()> {
        let manager = get_manager(&self.name)?;
        let label = microvm_label(&self.name);

        if manager.try_connect_existing().is_some() {
            let pid = manager
                .child_pid()
                .map(|p| format!(" (PID: {})", p))
                .unwrap_or_default();
            println!("MicroVM '{}': running{}", label, pid);
            std::mem::forget(manager);
        } else {
            println!("MicroVM '{}': stopped", label);
        }

        Ok(())
    }
}

// ============================================================================
// Ls Command
// ============================================================================

/// List all named VMs.
#[derive(Args, Debug)]
pub struct LsCmd {
    /// Show detailed output
    #[arg(short, long)]
    pub verbose: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

impl LsCmd {
    pub fn run(&self) -> smolvm::Result<()> {
        let config = SmolvmConfig::load().unwrap_or_default();
        let vms: Vec<_> = config.list_vms().collect();

        if vms.is_empty() {
            if !self.json {
                println!("No VMs found");
            } else {
                println!("[]");
            }
            return Ok(());
        }

        if self.json {
            let json_vms: Vec<_> = vms
                .iter()
                .map(|(name, record)| {
                    let actual_state = record.actual_state();
                    serde_json::json!({
                        "name": name,
                        "state": actual_state.to_string(),
                        "image": record.image,
                        "cpus": record.cpus,
                        "memory_mib": record.mem,
                        "pid": record.pid,
                        "command": record.command,
                        "workdir": record.workdir,
                        "env": record.env,
                        "mounts": record.mounts.len(),
                        "created_at": record.created_at,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&json_vms).unwrap());
        } else {
            println!(
                "{:<20} {:<10} {:<5} {:<8} {:<25} {:<6}",
                "NAME", "STATE", "CPUS", "MEMORY", "IMAGE", "MOUNTS"
            );
            println!("{}", "-".repeat(78));

            for (name, record) in vms {
                let actual_state = record.actual_state();
                println!(
                    "{:<20} {:<10} {:<5} {:<8} {:<25} {:<6}",
                    truncate(name, 18),
                    actual_state,
                    record.cpus,
                    format!("{} MiB", record.mem),
                    truncate(&record.image, 23),
                    record.mounts.len(),
                );

                if self.verbose {
                    if let Some(cmd) = &record.command {
                        println!("  Command: {:?}", cmd);
                    }
                    if let Some(wd) = &record.workdir {
                        println!("  Workdir: {}", wd);
                    }
                    if !record.env.is_empty() {
                        println!("  Env: {} variable(s)", record.env.len());
                    }
                    for (host, guest, ro) in &record.mounts {
                        let ro_str = if *ro { " (ro)" } else { "" };
                        println!("  Mount: {} -> {}{}", host, guest, ro_str);
                    }
                    println!("  Created: {}", record.created_at);
                    println!();
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// Network Test Command
// ============================================================================

/// Test network connectivity directly from microvm (debug TSI).
#[derive(Args, Debug)]
pub struct NetworkTestCmd {
    /// Named microvm to test (omit for default anonymous)
    #[arg(long)]
    pub name: Option<String>,

    /// URL to test
    #[arg(default_value = "http://1.1.1.1")]
    pub url: String,
}

impl NetworkTestCmd {
    pub fn run(self) -> smolvm::Result<()> {
        let manager = get_manager(&self.name)?;
        let label = microvm_label(&self.name);

        // Ensure microvm is running
        if manager.try_connect_existing().is_none() {
            println!("Starting microvm '{}'...", label);
            manager.ensure_running()?;
        }

        // Connect and test
        println!("Testing network from microvm: {}", self.url);
        let mut client = manager.connect()?;
        let result = client.network_test(&self.url)?;

        println!(
            "Result: {}",
            serde_json::to_string_pretty(&result).unwrap_or_default()
        );

        std::mem::forget(manager);
        Ok(())
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get the agent manager for a name (or default if None).
fn get_manager(name: &Option<String>) -> smolvm::Result<AgentManager> {
    if let Some(name) = name {
        AgentManager::for_vm(name)
    } else {
        AgentManager::default()
    }
}

/// Format the microvm label for display.
fn microvm_label(name: &Option<String>) -> String {
    name.as_deref().unwrap_or("default").to_string()
}

/// Truncate a string to max length, adding "..." if needed.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

/// Parse volume mount specifications into HostMount structs.
fn parse_mounts(specs: &[String]) -> smolvm::Result<Vec<HostMount>> {
    use smolvm::Error;

    let mut mounts = Vec::new();

    for spec in specs {
        let parts: Vec<&str> = spec.split(':').collect();
        if parts.len() < 2 {
            return Err(Error::Mount(format!(
                "invalid volume specification '{}': expected host:container[:ro]",
                spec
            )));
        }

        let host_path = PathBuf::from(parts[0]);
        let guest_path = PathBuf::from(parts[1]);
        let read_only = parts.get(2).map(|&s| s == "ro").unwrap_or(false);

        // Validate host path exists
        if !host_path.exists() {
            return Err(Error::Mount(format!(
                "host path does not exist: {}",
                host_path.display()
            )));
        }

        // Must be a directory (virtiofs limitation)
        if !host_path.is_dir() {
            return Err(Error::Mount(format!(
                "host path must be a directory (virtiofs limitation): {}",
                host_path.display()
            )));
        }

        // Canonicalize host path
        let host_path = host_path.canonicalize().map_err(|e| {
            Error::Mount(format!("failed to resolve host path '{}': {}", parts[0], e))
        })?;

        mounts.push(HostMount {
            source: host_path,
            target: guest_path,
            read_only,
        });
    }

    Ok(mounts)
}

/// Parse volume mount specifications into tuple format for VmRecord storage.
fn parse_mounts_as_tuples(specs: &[String]) -> smolvm::Result<Vec<(String, String, bool)>> {
    use smolvm::Error;

    let mut mounts = Vec::new();

    for spec in specs {
        let parts: Vec<&str> = spec.split(':').collect();
        if parts.len() < 2 {
            return Err(Error::Mount(format!(
                "invalid volume specification '{}': expected host:container[:ro]",
                spec
            )));
        }

        let host_path = PathBuf::from(parts[0]);
        let guest_path = parts[1].to_string();
        let read_only = parts.get(2).map(|&s| s == "ro").unwrap_or(false);

        // Validate host path exists
        if !host_path.exists() {
            return Err(Error::Mount(format!(
                "host path does not exist: {}",
                host_path.display()
            )));
        }

        // Must be a directory
        if !host_path.is_dir() {
            return Err(Error::Mount(format!(
                "host path must be a directory: {}",
                host_path.display()
            )));
        }

        // Canonicalize host path
        let host_path = host_path.canonicalize().map_err(|e| {
            Error::Mount(format!("failed to resolve host path '{}': {}", parts[0], e))
        })?;

        mounts.push((
            host_path.to_string_lossy().to_string(),
            guest_path,
            read_only,
        ));
    }

    Ok(mounts)
}
