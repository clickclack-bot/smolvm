//! List command implementation.

use clap::Args;
use smolvm::config::SmolvmConfig;

/// List all VMs.
#[derive(Args, Debug)]
pub struct ListCmd {
    /// Show detailed output.
    #[arg(short, long)]
    pub verbose: bool,

    /// Output as JSON.
    #[arg(long)]
    pub json: bool,
}

impl ListCmd {
    /// Execute the list command.
    pub fn run(&self, config: &SmolvmConfig) -> smolvm::Result<()> {
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
            // Table output
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

/// Truncate a string to max length, adding "..." if needed.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
