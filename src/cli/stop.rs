//! Stop command implementation.

use clap::Args;
use smolvm::config::{RecordState, SmolvmConfig};
use smolvm::process;
use std::time::Duration;

/// Stop a running VM.
#[derive(Args, Debug)]
pub struct StopCmd {
    /// VM name to stop.
    pub name: String,

    /// Force stop (SIGKILL after timeout).
    #[arg(short, long)]
    pub force: bool,

    /// Timeout in seconds before force kill.
    #[arg(long, default_value = "10")]
    pub timeout: u64,
}

impl StopCmd {
    /// Execute the stop command.
    pub fn run(self, config: &mut SmolvmConfig) -> smolvm::Result<()> {
        // Get VM record
        let record = config
            .get_vm(&self.name)
            .ok_or_else(|| smolvm::Error::VmNotFound(self.name.clone()))?
            .clone();

        // Check state
        let actual_state = record.actual_state();
        if actual_state != RecordState::Running {
            return Err(smolvm::Error::InvalidState {
                expected: "running".to_string(),
                actual: actual_state.to_string(),
            });
        }

        let pid = record
            .pid
            .ok_or_else(|| smolvm::Error::vm_creation("no PID recorded for running VM"))?;

        println!("Stopping VM {} (PID: {})...", self.name, pid);

        // Check if already dead
        if !process::is_alive(pid) {
            self.cleanup_state(config)?;
            println!("VM {} already stopped", self.name);
            return Ok(());
        }

        // Use shared process utilities to stop with timeout
        let timeout = Duration::from_secs(if self.force { 1 } else { self.timeout });
        match process::stop_process(pid, timeout, self.force) {
            Ok(_) => {
                self.cleanup_state(config)?;
                println!("Stopped VM: {}", self.name);
                Ok(())
            }
            Err(e) => {
                // If force wasn't requested, provide helpful error
                if !self.force {
                    Err(smolvm::Error::vm_creation(
                        "timeout waiting for VM to stop (use --force to kill)",
                    ))
                } else {
                    Err(e)
                }
            }
        }
    }

    fn cleanup_state(&self, config: &mut SmolvmConfig) -> smolvm::Result<()> {
        // Get PID file path before updating
        let pid_file = config
            .get_vm(&self.name)
            .and_then(|r| r.pid_file.clone());

        // Update config
        config.update_vm(&self.name, |r| {
            r.state = RecordState::Stopped;
            r.pid = None;
            r.pid_file = None;
        });
        config.save()?;

        // Clean up PID file
        if let Some(pf) = pid_file {
            let _ = std::fs::remove_file(pf);
        }

        Ok(())
    }
}
