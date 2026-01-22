//! API server state management.

use crate::agent::{AgentManager, HostMount, PortMapping, VmResources};
use crate::api::error::ApiError;
use crate::api::types::{MountSpec, PortSpec, ResourceSpec, SandboxInfo};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Shared API server state.
pub struct ApiState {
    /// Registry of sandbox managers by name.
    sandboxes: RwLock<HashMap<String, Arc<parking_lot::Mutex<SandboxEntry>>>>,
}

/// Internal sandbox entry with manager and configuration.
pub struct SandboxEntry {
    /// The agent manager for this sandbox.
    pub manager: AgentManager,
    /// Host mounts configured for this sandbox.
    pub mounts: Vec<MountSpec>,
    /// Port mappings configured for this sandbox.
    pub ports: Vec<PortSpec>,
    /// VM resources configured for this sandbox.
    pub resources: ResourceSpec,
}

impl ApiState {
    /// Create a new API state.
    pub fn new() -> Self {
        Self {
            sandboxes: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new sandbox.
    pub fn register_sandbox(
        &self,
        name: String,
        manager: AgentManager,
        mounts: Vec<MountSpec>,
        ports: Vec<PortSpec>,
        resources: ResourceSpec,
    ) -> Result<(), ApiError> {
        let mut sandboxes = self.sandboxes.write();
        if sandboxes.contains_key(&name) {
            return Err(ApiError::Conflict(format!(
                "sandbox '{}' already exists",
                name
            )));
        }
        sandboxes.insert(
            name,
            Arc::new(parking_lot::Mutex::new(SandboxEntry {
                manager,
                mounts,
                ports,
                resources,
            })),
        );
        Ok(())
    }

    /// Get a sandbox entry by name.
    pub fn get_sandbox(&self, name: &str) -> Result<Arc<parking_lot::Mutex<SandboxEntry>>, ApiError> {
        let sandboxes = self.sandboxes.read();
        sandboxes
            .get(name)
            .cloned()
            .ok_or_else(|| ApiError::NotFound(format!("sandbox '{}' not found", name)))
    }

    /// Remove a sandbox from the registry.
    pub fn remove_sandbox(&self, name: &str) -> Result<Arc<parking_lot::Mutex<SandboxEntry>>, ApiError> {
        let mut sandboxes = self.sandboxes.write();
        sandboxes
            .remove(name)
            .ok_or_else(|| ApiError::NotFound(format!("sandbox '{}' not found", name)))
    }

    /// List all sandboxes.
    pub fn list_sandboxes(&self) -> Vec<SandboxInfo> {
        let sandboxes = self.sandboxes.read();
        sandboxes
            .iter()
            .map(|(name, entry)| {
                let entry = entry.lock();
                let state = format!("{:?}", entry.manager.state());
                let pid = entry.manager.child_pid();
                SandboxInfo {
                    name: name.clone(),
                    state: state.to_lowercase(),
                    pid,
                    mounts: entry.mounts.clone(),
                    ports: entry.ports.clone(),
                    resources: entry.resources.clone(),
                }
            })
            .collect()
    }

    /// Check if a sandbox exists.
    pub fn sandbox_exists(&self, name: &str) -> bool {
        self.sandboxes.read().contains_key(name)
    }
}

impl Default for ApiState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Type Conversions
// ============================================================================

/// Convert MountSpec to HostMount.
pub fn mount_spec_to_host_mount(spec: &MountSpec) -> crate::Result<HostMount> {
    let mount = if spec.readonly {
        HostMount::new(&spec.source, &spec.target)
    } else {
        HostMount::new_writable(&spec.source, &spec.target)
    };
    Ok(mount)
}

/// Convert PortSpec to PortMapping.
pub fn port_spec_to_mapping(spec: &PortSpec) -> PortMapping {
    PortMapping::new(spec.host, spec.guest)
}

/// Convert ResourceSpec to VmResources.
pub fn resource_spec_to_vm_resources(spec: &ResourceSpec) -> VmResources {
    VmResources {
        cpus: spec.cpus.unwrap_or(crate::agent::DEFAULT_CPUS),
        mem: spec.memory_mb.unwrap_or(crate::agent::DEFAULT_MEMORY_MIB),
    }
}

/// Convert VmResources to ResourceSpec.
pub fn vm_resources_to_spec(res: VmResources) -> ResourceSpec {
    ResourceSpec {
        cpus: Some(res.cpus),
        memory_mb: Some(res.mem),
    }
}

/// Convert HostMount to MountSpec.
pub fn host_mount_to_spec(mount: &HostMount) -> MountSpec {
    MountSpec {
        source: mount.source.to_string_lossy().to_string(),
        target: mount.target.to_string_lossy().to_string(),
        readonly: mount.read_only,
    }
}

/// Convert PortMapping to PortSpec.
pub fn port_mapping_to_spec(mapping: &PortMapping) -> PortSpec {
    PortSpec {
        host: mapping.host,
        guest: mapping.guest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_conversions() {
        // MountSpec -> HostMount preserves readonly flag
        let spec = MountSpec { source: "/host".into(), target: "/guest".into(), readonly: true };
        assert!(mount_spec_to_host_mount(&spec).unwrap().read_only);

        let spec = MountSpec { source: "/host".into(), target: "/guest".into(), readonly: false };
        assert!(!mount_spec_to_host_mount(&spec).unwrap().read_only);

        // ResourceSpec with None uses defaults
        let spec = ResourceSpec { cpus: None, memory_mb: None };
        let res = resource_spec_to_vm_resources(&spec);
        assert_eq!(res.cpus, crate::agent::DEFAULT_CPUS);
        assert_eq!(res.mem, crate::agent::DEFAULT_MEMORY_MIB);
    }

    #[test]
    fn test_sandbox_not_found() {
        let state = ApiState::new();
        assert!(matches!(state.get_sandbox("nope"), Err(ApiError::NotFound(_))));
        assert!(matches!(state.remove_sandbox("nope"), Err(ApiError::NotFound(_))));
    }
}
