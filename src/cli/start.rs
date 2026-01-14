//! Start command implementation.

use clap::Args;
use smolvm::agent::{AgentClient, AgentManager, HostMount, VmResources};
use smolvm::config::{RecordState, SmolvmConfig};
use std::path::PathBuf;

/// Start a created VM.
#[derive(Args, Debug)]
pub struct StartCmd {
    /// VM name to start.
    pub name: String,
}

impl StartCmd {
    /// Execute the start command.
    pub fn run(self, config: &mut SmolvmConfig) -> smolvm::Result<()> {
        use smolvm::Error;
        use std::io::Write;

        // Get VM record
        let record = config
            .get_vm(&self.name)
            .ok_or_else(|| Error::VmNotFound(self.name.clone()))?
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
                host_path: PathBuf::from(host),
                guest_path: PathBuf::from(guest),
                read_only: *ro,
            })
            .collect();

        // Get VM resources from record
        let resources = VmResources {
            cpus: record.cpus,
            mem: record.mem,
        };

        // Start agent VM for this named VM
        let manager = AgentManager::for_vm(&self.name).map_err(|e| {
            Error::AgentError(format!("failed to create agent manager: {}", e))
        })?;

        if !mounts.is_empty() {
            println!("Starting VM {} with {} mount(s)...", self.name, mounts.len());
        } else {
            println!("Starting VM {}...", self.name);
        }

        manager.ensure_running_with_config(mounts.clone(), resources).map_err(|e| {
            Error::AgentError(format!("failed to start agent: {}", e))
        })?;

        // Update state
        let pid = manager.child_pid();
        config.update_vm(&self.name, |r| {
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
        let command = record.command.clone().unwrap_or_else(|| vec!["/bin/sh".to_string()]);

        // Convert mounts to agent format
        let mount_bindings: Vec<(String, String, bool)> = mounts
            .iter()
            .enumerate()
            .map(|(i, m)| {
                (
                    format!("smolvm{}", i),
                    m.guest_path.to_string_lossy().to_string(),
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

        // Update state to stopped
        config.update_vm(&self.name, |r| {
            r.state = RecordState::Stopped;
            r.pid = None;
        });
        config.save()?;

        // Stop the agent
        if let Err(e) = manager.stop() {
            tracing::warn!(error = %e, "failed to stop agent");
        }

        std::process::exit(exit_code);
    }
}
