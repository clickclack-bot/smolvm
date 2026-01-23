//! Global smolvm configuration.
//!
//! This module handles persistent configuration storage for smolvm,
//! including default settings and VM registry.
//!
//! State is persisted to a redb database at `~/.local/share/smolvm/server/smolvm.redb`.
//! For backward compatibility, `SmolvmConfig` maintains an in-memory cache of VMs
//! and provides the same API as the old confy-based implementation.

use crate::db::SmolvmDb;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// VM lifecycle state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RecordState {
    /// Container exists, VM not started.
    #[default]
    Created,
    /// VM process is running.
    Running,
    /// VM exited cleanly.
    Stopped,
    /// VM crashed or error.
    Failed,
}

impl std::fmt::Display for RecordState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordState::Created => write!(f, "created"),
            RecordState::Running => write!(f, "running"),
            RecordState::Stopped => write!(f, "stopped"),
            RecordState::Failed => write!(f, "failed"),
        }
    }
}

/// Global smolvm configuration with database-backed persistence.
///
/// This struct provides backward-compatible access to VM records while
/// using redb for ACID-compliant storage. The `vms` field is an in-memory
/// cache that is kept in sync with the database.
#[derive(Debug, Clone)]
pub struct SmolvmConfig {
    /// Database handle for persistence.
    db: SmolvmDb,
    /// Configuration format version.
    pub version: u8,
    /// Default number of vCPUs for new VMs.
    pub default_cpus: u8,
    /// Default memory in MiB for new VMs.
    pub default_mem: u32,
    /// Default DNS server for VMs with network egress.
    pub default_dns: String,
    /// Storage volume path (macOS only, for case-sensitive filesystem).
    #[cfg(target_os = "macos")]
    pub storage_volume: String,
    /// Registry of known VMs (by name) - in-memory cache.
    pub vms: HashMap<String, VmRecord>,
}

impl Default for SmolvmConfig {
    fn default() -> Self {
        Self {
            db: SmolvmDb::open().expect("failed to open database"),
            version: 1,
            default_cpus: 1,
            default_mem: 512,
            default_dns: "1.1.1.1".to_string(),
            #[cfg(target_os = "macos")]
            storage_volume: String::new(),
            vms: HashMap::new(),
        }
    }
}

impl SmolvmConfig {
    /// Load configuration from the database.
    ///
    /// Opens the database and loads all VM records into the in-memory cache.
    /// If this is the first run and an old confy config exists, it will be
    /// migrated automatically.
    pub fn load() -> Result<Self> {
        let db = SmolvmDb::open()?;

        // Load global config settings with defaults
        let version = db
            .get_config("version")?
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let default_cpus = db
            .get_config("default_cpus")?
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let default_mem = db
            .get_config("default_mem")?
            .and_then(|s| s.parse().ok())
            .unwrap_or(512);
        let default_dns = db
            .get_config("default_dns")?
            .unwrap_or_else(|| "1.1.1.1".to_string());

        #[cfg(target_os = "macos")]
        let storage_volume = db.get_config("storage_volume")?.unwrap_or_default();

        // Load all VMs into cache
        let vms = db.load_all_vms()?;

        Ok(Self {
            db,
            version,
            default_cpus,
            default_mem,
            default_dns,
            #[cfg(target_os = "macos")]
            storage_volume,
            vms,
        })
    }

    /// Save configuration to the database.
    ///
    /// This is now a no-op for VM records since writes are immediate.
    /// Global config changes are persisted here.
    pub fn save(&self) -> Result<()> {
        // Persist global config settings
        self.db.set_config("version", &self.version.to_string())?;
        self.db
            .set_config("default_cpus", &self.default_cpus.to_string())?;
        self.db
            .set_config("default_mem", &self.default_mem.to_string())?;
        self.db.set_config("default_dns", &self.default_dns)?;

        #[cfg(target_os = "macos")]
        if !self.storage_volume.is_empty() {
            self.db.set_config("storage_volume", &self.storage_volume)?;
        }

        Ok(())
    }

    /// Insert a VM record (persists immediately to database).
    pub fn insert_vm(&mut self, name: String, record: VmRecord) -> Result<()> {
        self.db.insert_vm(&name, &record)?;
        self.vms.insert(name, record);
        Ok(())
    }

    /// Remove a VM from the registry.
    pub fn remove_vm(&mut self, id: &str) -> Option<VmRecord> {
        // Remove from database (ignore errors, just log)
        if let Err(e) = self.db.remove_vm(id) {
            tracing::warn!(error = %e, vm = %id, "failed to remove VM from database");
        }
        self.vms.remove(id)
    }

    /// Get a VM record by ID.
    pub fn get_vm(&self, id: &str) -> Option<&VmRecord> {
        self.vms.get(id)
    }

    /// List all VM records.
    pub fn list_vms(&self) -> impl Iterator<Item = (&String, &VmRecord)> {
        self.vms.iter()
    }

    /// Update a VM record in place (persists immediately to database).
    pub fn update_vm<F>(&mut self, id: &str, f: F) -> Option<()>
    where
        F: FnOnce(&mut VmRecord),
    {
        if let Some(record) = self.vms.get_mut(id) {
            f(record);
            // Persist to database
            if let Err(e) = self.db.insert_vm(id, record) {
                tracing::warn!(error = %e, vm = %id, "failed to persist VM update");
            }
            Some(())
        } else {
            None
        }
    }

    /// Get the underlying database handle.
    pub fn db(&self) -> &SmolvmDb {
        &self.db
    }
}

/// Record of a VM in the registry.
///
/// This stores microvm configuration only. Container configuration
/// is managed separately via the container commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmRecord {
    /// VM name/ID.
    pub name: String,

    /// Creation timestamp.
    pub created_at: String,

    /// VM lifecycle state.
    #[serde(default)]
    pub state: RecordState,

    /// Process ID when running.
    #[serde(default)]
    pub pid: Option<i32>,

    /// Number of vCPUs.
    #[serde(default = "default_cpus")]
    pub cpus: u8,

    /// Memory in MiB.
    #[serde(default = "default_mem")]
    pub mem: u32,

    /// Volume mounts (host_path, guest_path, read_only).
    #[serde(default)]
    pub mounts: Vec<(String, String, bool)>,

    /// Port mappings (host_port, guest_port).
    #[serde(default)]
    pub ports: Vec<(u16, u16)>,
}

fn default_cpus() -> u8 {
    1
}

fn default_mem() -> u32 {
    512
}

impl VmRecord {
    /// Create a new VM record.
    pub fn new(
        name: String,
        cpus: u8,
        mem: u32,
        mounts: Vec<(String, String, bool)>,
        ports: Vec<(u16, u16)>,
    ) -> Self {
        Self {
            name,
            created_at: crate::util::current_timestamp(),
            state: RecordState::Created,
            pid: None,
            cpus,
            mem,
            mounts,
            ports,
        }
    }

    /// Check if the VM process is still alive.
    pub fn is_process_alive(&self) -> bool {
        if let Some(pid) = self.pid {
            crate::process::is_alive(pid)
        } else {
            false
        }
    }

    /// Get the actual state, checking if running process is still alive.
    pub fn actual_state(&self) -> RecordState {
        if self.state == RecordState::Running {
            if self.is_process_alive() {
                RecordState::Running
            } else {
                RecordState::Stopped // Process died
            }
        } else {
            self.state.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_record_serialization() {
        let record = VmRecord::new(
            "test".to_string(),
            2,
            512,
            vec![("/host".to_string(), "/guest".to_string(), false)],
            vec![(8080, 80)],
        );

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: VmRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, record.name);
        assert_eq!(deserialized.mounts, record.mounts);
    }

    #[test]
    fn test_record_state_display() {
        assert_eq!(RecordState::Created.to_string(), "created");
        assert_eq!(RecordState::Running.to_string(), "running");
        assert_eq!(RecordState::Stopped.to_string(), "stopped");
        assert_eq!(RecordState::Failed.to_string(), "failed");
    }
}
