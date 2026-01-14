//! Create command implementation.

use clap::Args;
use smolvm::config::{SmolvmConfig, VmRecord};
use std::path::PathBuf;

/// Parse an environment variable specification (KEY=VALUE).
fn parse_env_spec(spec: &str) -> Option<(String, String)> {
    let (key, value) = spec.split_once('=')?;
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), value.to_string()))
}

/// Create a VM without starting it.
///
/// Saves the VM configuration for later use with `start`.
#[derive(Args, Debug)]
pub struct CreateCmd {
    /// OCI image reference.
    pub image: String,

    /// Command to execute when the VM starts.
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,

    /// VM name (required).
    #[arg(long)]
    pub name: String,

    /// Number of vCPUs.
    #[arg(long, default_value = "1")]
    pub cpus: u8,

    /// Memory in MiB.
    #[arg(long, default_value = "256")]
    pub mem: u32,

    /// Working directory inside the container.
    #[arg(short = 'w', long)]
    pub workdir: Option<String>,

    /// Environment variable (KEY=VALUE).
    #[arg(short = 'e', long = "env")]
    pub env: Vec<String>,

    /// Volume mount (host:guest[:ro]).
    #[arg(short = 'v', long = "volume")]
    pub volume: Vec<String>,
}

impl CreateCmd {
    /// Execute the create command.
    pub fn run(self, config: &mut SmolvmConfig) -> smolvm::Result<()> {
        // Check if VM already exists
        if config.get_vm(&self.name).is_some() {
            return Err(smolvm::Error::Config(format!(
                "VM '{}' already exists",
                self.name
            )));
        }

        // Parse environment variables
        let env: Vec<(String, String)> = self
            .env
            .iter()
            .filter_map(|e| parse_env_spec(e))
            .collect();

        // Parse and validate volume mounts
        let mounts = self.parse_mounts()?;

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
        println!("\nUse 'smolvm start {}' to start the VM", self.name);

        Ok(())
    }

    /// Parse volume mount specifications.
    fn parse_mounts(&self) -> smolvm::Result<Vec<(String, String, bool)>> {
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
                Error::Mount(format!(
                    "failed to resolve host path '{}': {}",
                    parts[0], e
                ))
            })?;

            mounts.push((host_path.to_string_lossy().to_string(), guest_path, read_only));
        }

        Ok(mounts)
    }
}
