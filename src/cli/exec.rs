//! Exec command implementation.

use clap::Args;
use smolvm::agent::{AgentClient, AgentManager, HostMount, VmResources};
use std::path::PathBuf;

/// Execute a command in the agent VM
#[derive(Args, Debug)]
pub struct ExecCmd {
    /// OCI image reference
    pub image: String,

    /// Command to execute
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,

    /// Named VM to exec into (uses isolated agent)
    #[arg(long)]
    pub name: Option<String>,

    /// Number of vCPUs
    #[arg(long, default_value = "1")]
    pub cpus: u8,

    /// Memory in MiB
    #[arg(long, default_value = "256")]
    pub mem: u32,

    /// Working directory inside container
    #[arg(short = 'w', long)]
    pub workdir: Option<String>,

    /// Environment variable (KEY=VALUE)
    #[arg(short = 'e', long = "env")]
    pub env: Vec<String>,

    /// Volume mount (host:container[:ro])
    #[arg(short = 'v', long = "volume")]
    pub volume: Vec<String>,
}

impl ExecCmd {
    pub fn run(self) -> smolvm::Result<()> {
        use std::io::Write;

        // Parse volume mounts
        let mounts = self.parse_mounts()?;

        let resources = VmResources {
            cpus: self.cpus,
            mem: self.mem,
        };

        // Get the appropriate agent manager (named or default)
        let manager = if let Some(ref name) = self.name {
            AgentManager::for_vm(name)?
        } else {
            AgentManager::default()?
        };

        let was_running = manager.try_connect_existing().is_some()
            && manager.mounts_match(&mounts)
            && manager.resources_match(resources);
        if !was_running {
            let vm_label = self.name.as_deref().unwrap_or("default");
            if !mounts.is_empty() {
                println!("Starting agent VM '{}' with {} mount(s)...", vm_label, mounts.len());
            } else {
                println!("Starting agent VM '{}'...", vm_label);
            }
            manager.ensure_running_with_config(mounts.clone(), resources)?;
        }

        // Connect to agent
        let mut client = AgentClient::connect(manager.vsock_socket())?;

        // Pull image if needed (not a local path)
        if !std::path::Path::new(&self.image).exists() {
            // Only show pulling message on first startup
            if !was_running {
                println!("Pulling image {}...", self.image);
            }
            client.pull(&self.image, None)?;
        }

        // Build command
        let command = if self.command.is_empty() {
            vec!["/bin/sh".to_string()]
        } else {
            self.command.clone()
        };

        // Parse environment variables
        let env: Vec<(String, String)> = self
            .env
            .iter()
            .filter_map(|e| {
                let (k, v) = e.split_once('=')?;
                if k.is_empty() {
                    None
                } else {
                    Some((k.to_string(), v.to_string()))
                }
            })
            .collect();

        // Convert mounts to the format expected by the agent
        let mount_bindings: Vec<(String, String, bool)> = mounts
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    format!("smolvm{}", i), // virtiofs tag
                    m.guest_path.to_string_lossy().to_string(),
                    m.read_only,
                )
            })
            .collect();

        // Run command
        let (exit_code, stdout, stderr) = client.run_with_mounts(
            &self.image,
            command,
            env,
            self.workdir.clone(),
            mount_bindings,
        )?;

        // Print output
        if !stdout.is_empty() {
            print!("{}", stdout);
        }
        if !stderr.is_empty() {
            eprint!("{}", stderr);
        }

        // Flush output
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();

        // DON'T stop the agent - leave it running for next exec
        std::mem::forget(manager);

        std::process::exit(exit_code);
    }

    /// Parse volume mount specifications.
    fn parse_mounts(&self) -> smolvm::Result<Vec<HostMount>> {
        use smolvm::Error;

        let mut mounts = Vec::new();

        for spec in &self.volume {
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

            // Canonicalize host path
            let host_path = host_path.canonicalize().map_err(|e| {
                Error::Mount(format!(
                    "failed to resolve host path '{}': {}",
                    parts[0], e
                ))
            })?;

            mounts.push(HostMount {
                host_path,
                guest_path,
                read_only,
            });
        }

        Ok(mounts)
    }
}
