//! Agent VM management commands.

use clap::{Args, Subcommand};
use smolvm::agent::AgentManager;

/// Manage the agent VM
#[derive(Subcommand, Debug)]
pub enum AgentCmd {
    /// Start the agent VM
    Start(AgentStartCmd),
    /// Stop the agent VM
    Stop(AgentStopCmd),
    /// Show agent status
    Status(AgentStatusCmd),
}

impl AgentCmd {
    pub fn run(self) -> smolvm::Result<()> {
        match self {
            AgentCmd::Start(cmd) => cmd.run(),
            AgentCmd::Stop(cmd) => cmd.run(),
            AgentCmd::Status(cmd) => cmd.run(),
        }
    }
}

/// Start the agent VM
#[derive(Args, Debug)]
pub struct AgentStartCmd {}

impl AgentStartCmd {
    pub fn run(self) -> smolvm::Result<()> {
        let manager = AgentManager::default()?;

        // Check if already running
        if manager.try_connect_existing().is_some() {
            println!("Agent VM already running");
            // Don't stop - agent stays running
            std::mem::forget(manager);
            return Ok(());
        }

        println!("Starting agent VM...");
        manager.ensure_running()?;

        let pid = manager.child_pid().unwrap_or(0);
        println!("Agent VM running (PID: {})", pid);

        // Don't stop - agent stays running
        std::mem::forget(manager);

        Ok(())
    }
}

/// Stop the agent VM
#[derive(Args, Debug)]
pub struct AgentStopCmd {}

impl AgentStopCmd {
    pub fn run(self) -> smolvm::Result<()> {
        let manager = AgentManager::default()?;

        if manager.try_connect_existing().is_some() {
            println!("Stopping agent VM...");
            manager.stop()?;
            println!("Agent VM stopped");
        } else {
            println!("Agent VM not running");
        }

        Ok(())
    }
}

/// Show agent status
#[derive(Args, Debug)]
pub struct AgentStatusCmd {}

impl AgentStatusCmd {
    pub fn run(self) -> smolvm::Result<()> {
        let manager = AgentManager::default()?;

        if manager.try_connect_existing().is_some() {
            let pid = manager.child_pid().map(|p| format!(" (PID: {})", p)).unwrap_or_default();
            println!("Agent VM: running{}", pid);
            // Don't stop - just checking status
            std::mem::forget(manager);
        } else {
            println!("Agent VM: stopped");
        }

        Ok(())
    }
}
