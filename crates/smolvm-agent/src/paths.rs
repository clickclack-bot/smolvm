//! Centralized path constants and helpers for the smolvm agent.
//!
//! All filesystem paths used by the agent are defined here for consistency
//! and easy modification.

use std::path::PathBuf;

// =============================================================================
// Binary Paths
// =============================================================================

/// Path to crun OCI runtime binary.
pub const CRUN_PATH: &str = "/usr/bin/crun";

/// Path to conmon (container monitor) binary.
pub const CONMON_PATH: &str = "/usr/bin/conmon";

/// Path to crane (OCI image tool) binary.
pub const CRANE_PATH: &str = "/usr/local/bin/crane";

/// crun cgroup manager setting.
/// Set to "disabled" because libkrun mounts cgroup2 as read-only.
/// Without this, crun create/start hang trying to create container cgroups.
pub const CRUN_CGROUP_MANAGER: &str = "disabled";

// =============================================================================
// Storage Paths
// =============================================================================

/// Root directory for all persistent storage.
pub const STORAGE_ROOT: &str = "/storage";

/// Directory for extracted OCI layers.
pub const LAYERS_DIR: &str = "/storage/layers";

/// Directory for image manifest cache.
pub const MANIFESTS_DIR: &str = "/storage/manifests";

/// Directory for image config cache.
pub const CONFIGS_DIR: &str = "/storage/configs";

/// Directory for overlay filesystems.
pub const OVERLAYS_DIR: &str = "/storage/overlays";

// =============================================================================
// Container Runtime Paths
// =============================================================================

/// Root directory for container runtime state.
pub const CONTAINERS_ROOT: &str = "/storage/containers";

/// Directory for per-container runtime state (pidfile, conmon.pid, etc).
pub const CONTAINERS_RUN_DIR: &str = "/storage/containers/run";

/// Directory for container logs.
pub const CONTAINERS_LOGS_DIR: &str = "/storage/containers/logs";

/// Directory for container exit code files.
pub const CONTAINERS_EXIT_DIR: &str = "/storage/containers/exit";

/// Path to the persistent container registry file.
pub const REGISTRY_PATH: &str = "/storage/containers/registry.json";

/// Path to the registry lock file.
pub const REGISTRY_LOCK_PATH: &str = "/storage/containers/registry.lock";

/// crun runtime root directory.
pub const CRUN_ROOT: &str = "/run/crun";

// =============================================================================
// Mount Paths
// =============================================================================

/// Root directory where virtiofs mounts are staged.
pub const VIRTIOFS_MOUNT_ROOT: &str = "/mnt/virtiofs";

// =============================================================================
// Timeouts (milliseconds)
// =============================================================================

/// Timeout for waiting on PID files to appear.
pub const PID_FILE_TIMEOUT_MS: u64 = 5000;

/// Timeout for acquiring registry lock.
pub const REGISTRY_LOCK_TIMEOUT_MS: u64 = 5000;

// =============================================================================
// Path Helper Functions
// =============================================================================

/// Get the runtime directory for a specific container.
pub fn container_run_dir(container_id: &str) -> PathBuf {
    PathBuf::from(CONTAINERS_RUN_DIR).join(container_id)
}

/// Get the log file path for a container.
pub fn container_log_path(container_id: &str) -> PathBuf {
    PathBuf::from(CONTAINERS_LOGS_DIR).join(format!("{}.log", container_id))
}

/// Get the exit code file path for a container.
pub fn container_exit_path(container_id: &str) -> PathBuf {
    PathBuf::from(CONTAINERS_EXIT_DIR).join(container_id)
}

/// Get the pidfile path for a container (written by crun).
pub fn container_pidfile_path(container_id: &str) -> PathBuf {
    container_run_dir(container_id).join("pidfile")
}

/// Get the conmon pidfile path for a container.
pub fn conmon_pidfile_path(container_id: &str) -> PathBuf {
    container_run_dir(container_id).join("conmon.pid")
}

/// Get the attach socket path for a container.
pub fn attach_socket_path(container_id: &str) -> PathBuf {
    container_run_dir(container_id).join("attach")
}

/// Get the overlay directory for a workload.
pub fn overlay_dir(workload_id: &str) -> PathBuf {
    PathBuf::from(OVERLAYS_DIR).join(workload_id)
}

/// Get the bundle directory for a workload.
pub fn bundle_dir(workload_id: &str) -> PathBuf {
    overlay_dir(workload_id).join("bundle")
}

/// Get the merged rootfs path for a workload.
pub fn merged_rootfs_path(workload_id: &str) -> PathBuf {
    overlay_dir(workload_id).join("merged")
}

/// Get the upper (writable) directory path for a workload.
pub fn upper_dir(workload_id: &str) -> PathBuf {
    overlay_dir(workload_id).join("upper")
}

/// Get the work directory path for a workload (used by overlayfs).
pub fn work_dir(workload_id: &str) -> PathBuf {
    overlay_dir(workload_id).join("work")
}

/// Get the layer directory for a specific digest.
pub fn layer_dir(digest: &str) -> PathBuf {
    PathBuf::from(LAYERS_DIR).join(digest)
}

/// Get the virtiofs mount path for a tag.
pub fn virtiofs_mount_path(tag: &str) -> PathBuf {
    PathBuf::from(VIRTIOFS_MOUNT_ROOT).join(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_paths() {
        let id = "abc123";
        assert_eq!(
            container_run_dir(id),
            PathBuf::from("/storage/containers/run/abc123")
        );
        assert_eq!(
            container_log_path(id),
            PathBuf::from("/storage/containers/logs/abc123.log")
        );
        assert_eq!(
            container_exit_path(id),
            PathBuf::from("/storage/containers/exit/abc123")
        );
        assert_eq!(
            container_pidfile_path(id),
            PathBuf::from("/storage/containers/run/abc123/pidfile")
        );
        assert_eq!(
            conmon_pidfile_path(id),
            PathBuf::from("/storage/containers/run/abc123/conmon.pid")
        );
    }

    #[test]
    fn test_overlay_paths() {
        let wl = "workload-123";
        assert_eq!(
            overlay_dir(wl),
            PathBuf::from("/storage/overlays/workload-123")
        );
        assert_eq!(
            bundle_dir(wl),
            PathBuf::from("/storage/overlays/workload-123/bundle")
        );
        assert_eq!(
            merged_rootfs_path(wl),
            PathBuf::from("/storage/overlays/workload-123/merged")
        );
    }
}
