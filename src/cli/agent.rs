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

/// Get the agent manager for a name (or default if None)
fn get_manager(name: &Option<String>) -> smolvm::Result<AgentManager> {
    if let Some(name) = name {
        AgentManager::for_vm(name)
    } else {
        AgentManager::default()
    }
}

/// Format the agent label for display
fn agent_label(name: &Option<String>) -> String {
    name.as_deref().unwrap_or("default").to_string()
}

/// Start the agent VM
#[derive(Args, Debug)]
pub struct AgentStartCmd {
    /// Named VM's agent to start (default: anonymous agent)
    #[arg(long)]
    pub name: Option<String>,
}

impl AgentStartCmd {
    pub fn run(self) -> smolvm::Result<()> {
        let manager = get_manager(&self.name)?;
        let label = agent_label(&self.name);

        // Check if already running
        if manager.try_connect_existing().is_some() {
            println!("Agent VM '{}' already running", label);
            // Don't stop - agent stays running
            std::mem::forget(manager);
            return Ok(());
        }

        println!("Starting agent VM '{}'...", label);
        manager.ensure_running()?;

        let pid = manager.child_pid().unwrap_or(0);
        println!("Agent VM '{}' running (PID: {})", label, pid);

        // Don't stop - agent stays running
        std::mem::forget(manager);

        Ok(())
    }
}

/// Stop the agent VM
#[derive(Args, Debug)]
pub struct AgentStopCmd {
    /// Named VM's agent to stop (default: anonymous agent)
    #[arg(long)]
    pub name: Option<String>,
}

impl AgentStopCmd {
    pub fn run(self) -> smolvm::Result<()> {
        let manager = get_manager(&self.name)?;
        let label = agent_label(&self.name);

        if manager.try_connect_existing().is_some() {
            println!("Stopping agent VM '{}'...", label);
            manager.stop()?;
            println!("Agent VM '{}' stopped", label);
        } else {
            println!("Agent VM '{}' not running", label);
        }

        Ok(())
    }
}

/// Show agent status
#[derive(Args, Debug)]
pub struct AgentStatusCmd {
    /// Named VM's agent to check (default: anonymous agent)
    #[arg(long)]
    pub name: Option<String>,
}

impl AgentStatusCmd {
    pub fn run(self) -> smolvm::Result<()> {
        let manager = get_manager(&self.name)?;
        let label = agent_label(&self.name);

        if manager.try_connect_existing().is_some() {
            let pid = manager.child_pid().map(|p| format!(" (PID: {})", p)).unwrap_or_default();
            println!("Agent VM '{}': running{}", label, pid);
            // Don't stop - just checking status
            std::mem::forget(manager);
        } else {
            println!("Agent VM '{}': stopped", label);
        }

        Ok(())
    }
}
