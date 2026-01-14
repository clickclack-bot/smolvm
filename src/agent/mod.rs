//! Agent VM management.
//!
//! This module manages the agent VM lifecycle and provides a client
//! for communicating with the smolvm-agent via vsock.

mod client;
mod launcher;
mod manager;

pub use client::AgentClient;
pub use launcher::HostMount;
pub use manager::{AgentManager, AgentState};

/// Default agent VM memory in MiB.
pub const AGENT_MEMORY_MIB: u32 = 256;

/// Default agent VM CPU count.
pub const AGENT_CPUS: u8 = 1;

/// Agent VM name.
pub const AGENT_VM_NAME: &str = "smolvm-agent";
