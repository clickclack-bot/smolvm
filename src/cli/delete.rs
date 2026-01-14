//! Delete command implementation.

use clap::Args;
use smolvm::config::SmolvmConfig;
use smolvm::error::Error;

/// Delete a VM.
#[derive(Args, Debug)]
pub struct DeleteCmd {
    /// VM name to delete.
    pub name: String,

    /// Force deletion without confirmation.
    #[arg(short, long)]
    pub force: bool,
}

impl DeleteCmd {
    /// Execute the delete command.
    pub fn run(&self, config: &mut SmolvmConfig) -> smolvm::Result<()> {
        // Check if VM exists
        if config.get_vm(&self.name).is_none() {
            return Err(Error::VmNotFound(self.name.clone()));
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
