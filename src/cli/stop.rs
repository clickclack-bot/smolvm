//! Stop command implementation.

use clap::Args;
use smolvm::agent::AgentManager;
use smolvm::config::{RecordState, SmolvmConfig};

/// Stop a running VM.
#[derive(Args, Debug)]
pub struct StopCmd {
    /// VM name to stop.
    pub name: String,
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
            println!("VM {} is not running (state: {})", self.name, actual_state);
            return Ok(());
        }

        println!("Stopping VM {}...", self.name);

        // Stop this VM's agent
        if let Ok(manager) = AgentManager::for_vm(&self.name) {
            if let Err(e) = manager.stop() {
                tracing::warn!(error = %e, "failed to stop agent");
            }
        }

        // Update state
        config.update_vm(&self.name, |r| {
            r.state = RecordState::Stopped;
            r.pid = None;
        });
        config.save()?;

        println!("Stopped VM: {}", self.name);
        Ok(())
    }
}
