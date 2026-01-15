//! OCI Runtime Specification generation for crun integration.
//!
//! This module provides types and functions for generating OCI-compliant
//! config.json files used by crun to execute containers.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// OCI Runtime Specification (subset for container execution).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciSpec {
    #[serde(rename = "ociVersion")]
    pub oci_version: String,
    pub root: OciRoot,
    pub process: OciProcess,
    pub linux: OciLinux,
    pub mounts: Vec<OciMount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
}

/// Root filesystem configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciRoot {
    /// Path to the root filesystem (relative to bundle or absolute).
    pub path: String,
    /// Whether the root filesystem should be read-only.
    #[serde(default)]
    pub readonly: bool,
}

/// Process configuration for the container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciProcess {
    /// Whether to allocate a pseudo-terminal.
    #[serde(default)]
    pub terminal: bool,
    /// User and group IDs.
    pub user: OciUser,
    /// Command and arguments to execute.
    pub args: Vec<String>,
    /// Environment variables in KEY=VALUE format.
    #[serde(default)]
    pub env: Vec<String>,
    /// Working directory inside the container.
    pub cwd: String,
    /// Linux capabilities (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<OciCapabilities>,
    /// Resource limits (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rlimits: Option<Vec<OciRlimit>>,
    /// Do not create a new session for the process.
    #[serde(rename = "noNewPrivileges", default)]
    pub no_new_privileges: bool,
}

/// User configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciUser {
    pub uid: u32,
    pub gid: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_gids: Vec<u32>,
}

/// Linux capabilities configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciCapabilities {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bounding: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effective: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inheritable: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permitted: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ambient: Vec<String>,
}

/// Resource limit configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciRlimit {
    #[serde(rename = "type")]
    pub rlimit_type: String,
    pub hard: u64,
    pub soft: u64,
}

/// Linux-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciLinux {
    /// Namespaces to create.
    pub namespaces: Vec<OciNamespace>,
    /// Masked paths (paths that should appear empty).
    #[serde(rename = "maskedPaths", default, skip_serializing_if = "Vec::is_empty")]
    pub masked_paths: Vec<String>,
    /// Read-only paths.
    #[serde(rename = "readonlyPaths", default, skip_serializing_if = "Vec::is_empty")]
    pub readonly_paths: Vec<String>,
}

/// Namespace configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciNamespace {
    /// Type of namespace (pid, network, mount, ipc, uts, user, cgroup).
    #[serde(rename = "type")]
    pub ns_type: String,
    /// Path to an existing namespace to join (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Mount configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OciMount {
    /// Destination path inside the container.
    pub destination: String,
    /// Filesystem type (proc, sysfs, tmpfs, bind, etc.).
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub mount_type: Option<String>,
    /// Source path or device.
    pub source: String,
    /// Mount options.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

impl OciSpec {
    /// Create a new OCI spec with sensible defaults for container execution.
    ///
    /// # Arguments
    /// * `command` - Command and arguments to execute
    /// * `env` - Environment variables as (key, value) pairs
    /// * `workdir` - Working directory inside the container
    /// * `tty` - Whether to allocate a pseudo-terminal
    pub fn new(
        command: &[String],
        env: &[(String, String)],
        workdir: &str,
        tty: bool,
    ) -> Self {
        // Build environment variables
        let env_strings: Vec<String> = [
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
            "HOME=/root".to_string(),
            "TERM=xterm-256color".to_string(),
        ]
        .into_iter()
        .chain(env.iter().map(|(k, v)| format!("{}={}", k, v)))
        .collect();

        // Default capabilities for root containers
        let capabilities = OciCapabilities {
            bounding: default_capabilities(),
            effective: default_capabilities(),
            inheritable: vec![],
            permitted: default_capabilities(),
            ambient: vec![],
        };

        Self {
            oci_version: "1.0.2".to_string(),
            root: OciRoot {
                path: "rootfs".to_string(),
                readonly: false,
            },
            process: OciProcess {
                terminal: tty,
                user: OciUser {
                    uid: 0,
                    gid: 0,
                    additional_gids: vec![],
                },
                args: command.to_vec(),
                env: env_strings,
                cwd: workdir.to_string(),
                capabilities: Some(capabilities),
                rlimits: Some(vec![OciRlimit {
                    rlimit_type: "RLIMIT_NOFILE".to_string(),
                    hard: 1024,
                    soft: 1024,
                }]),
                no_new_privileges: false,
            },
            linux: OciLinux {
                namespaces: vec![
                    OciNamespace {
                        ns_type: "pid".to_string(),
                        path: None,
                    },
                    OciNamespace {
                        ns_type: "mount".to_string(),
                        path: None,
                    },
                    OciNamespace {
                        ns_type: "ipc".to_string(),
                        path: None,
                    },
                    OciNamespace {
                        ns_type: "uts".to_string(),
                        path: None,
                    },
                ],
                masked_paths: vec![
                    "/proc/asound".to_string(),
                    "/proc/acpi".to_string(),
                    "/proc/kcore".to_string(),
                    "/proc/keys".to_string(),
                    "/proc/latency_stats".to_string(),
                    "/proc/timer_list".to_string(),
                    "/proc/timer_stats".to_string(),
                    "/proc/sched_debug".to_string(),
                    "/proc/scsi".to_string(),
                    "/sys/firmware".to_string(),
                ],
                readonly_paths: vec![
                    "/proc/bus".to_string(),
                    "/proc/fs".to_string(),
                    "/proc/irq".to_string(),
                    "/proc/sys".to_string(),
                    "/proc/sysrq-trigger".to_string(),
                ],
            },
            mounts: default_mounts(),
            hostname: Some("container".to_string()),
        }
    }

    /// Add a bind mount to the spec.
    ///
    /// # Arguments
    /// * `source` - Source path on the host
    /// * `destination` - Destination path inside the container
    /// * `read_only` - Whether the mount should be read-only
    pub fn add_bind_mount(&mut self, source: &str, destination: &str, read_only: bool) {
        let mut options = vec!["bind".to_string(), "rprivate".to_string()];
        if read_only {
            options.push("ro".to_string());
        }
        self.mounts.push(OciMount {
            destination: destination.to_string(),
            mount_type: Some("bind".to_string()),
            source: source.to_string(),
            options,
        });
    }

    /// Write the OCI spec to a config.json file in the bundle directory.
    pub fn write_to(&self, bundle_dir: &Path) -> std::io::Result<()> {
        let config_path = bundle_dir.join("config.json");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(config_path, json)
    }
}

/// Generate a unique container ID.
pub fn generate_container_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // Use lower 48 bits of timestamp + some randomness from the upper bits
    format!("smolvm-{:012x}", timestamp & 0xFFFF_FFFF_FFFF)
}

/// Default Linux capabilities for root containers.
fn default_capabilities() -> Vec<String> {
    vec![
        "CAP_CHOWN".to_string(),
        "CAP_DAC_OVERRIDE".to_string(),
        "CAP_FSETID".to_string(),
        "CAP_FOWNER".to_string(),
        "CAP_MKNOD".to_string(),
        "CAP_NET_RAW".to_string(),
        "CAP_SETGID".to_string(),
        "CAP_SETUID".to_string(),
        "CAP_SETFCAP".to_string(),
        "CAP_SETPCAP".to_string(),
        "CAP_NET_BIND_SERVICE".to_string(),
        "CAP_SYS_CHROOT".to_string(),
        "CAP_KILL".to_string(),
        "CAP_AUDIT_WRITE".to_string(),
    ]
}

/// Default mounts for container execution.
fn default_mounts() -> Vec<OciMount> {
    vec![
        // /proc - process information
        OciMount {
            destination: "/proc".to_string(),
            mount_type: Some("proc".to_string()),
            source: "proc".to_string(),
            options: vec!["nosuid".to_string(), "noexec".to_string(), "nodev".to_string()],
        },
        // /dev - device nodes
        OciMount {
            destination: "/dev".to_string(),
            mount_type: Some("tmpfs".to_string()),
            source: "tmpfs".to_string(),
            options: vec![
                "nosuid".to_string(),
                "strictatime".to_string(),
                "mode=755".to_string(),
                "size=65536k".to_string(),
            ],
        },
        // /dev/pts - pseudo-terminal devices
        OciMount {
            destination: "/dev/pts".to_string(),
            mount_type: Some("devpts".to_string()),
            source: "devpts".to_string(),
            options: vec![
                "nosuid".to_string(),
                "noexec".to_string(),
                "newinstance".to_string(),
                "ptmxmode=0666".to_string(),
                "mode=0620".to_string(),
            ],
        },
        // /dev/shm - shared memory
        OciMount {
            destination: "/dev/shm".to_string(),
            mount_type: Some("tmpfs".to_string()),
            source: "shm".to_string(),
            options: vec![
                "nosuid".to_string(),
                "noexec".to_string(),
                "nodev".to_string(),
                "mode=1777".to_string(),
                "size=65536k".to_string(),
            ],
        },
        // /dev/mqueue - POSIX message queues
        OciMount {
            destination: "/dev/mqueue".to_string(),
            mount_type: Some("mqueue".to_string()),
            source: "mqueue".to_string(),
            options: vec![
                "nosuid".to_string(),
                "noexec".to_string(),
                "nodev".to_string(),
            ],
        },
        // /sys - sysfs (read-only for security)
        OciMount {
            destination: "/sys".to_string(),
            mount_type: Some("sysfs".to_string()),
            source: "sysfs".to_string(),
            options: vec![
                "nosuid".to_string(),
                "noexec".to_string(),
                "nodev".to_string(),
                "ro".to_string(),
            ],
        },
        // /sys/fs/cgroup - cgroup filesystem (read-only)
        OciMount {
            destination: "/sys/fs/cgroup".to_string(),
            mount_type: Some("cgroup2".to_string()),
            source: "cgroup".to_string(),
            options: vec![
                "nosuid".to_string(),
                "noexec".to_string(),
                "nodev".to_string(),
                "ro".to_string(),
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_container_id() {
        let id1 = generate_container_id();
        let id2 = generate_container_id();

        assert!(id1.starts_with("smolvm-"));
        assert!(id2.starts_with("smolvm-"));
        // IDs should be unique (different timestamps)
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_oci_spec_creation() {
        let spec = OciSpec::new(
            &["echo".to_string(), "hello".to_string()],
            &[("FOO".to_string(), "bar".to_string())],
            "/",
            false,
        );

        assert_eq!(spec.oci_version, "1.0.2");
        assert_eq!(spec.process.args, vec!["echo", "hello"]);
        assert!(spec.process.env.contains(&"FOO=bar".to_string()));
        assert!(!spec.process.terminal);
    }

    #[test]
    fn test_add_bind_mount() {
        let mut spec = OciSpec::new(&["sh".to_string()], &[], "/", false);
        spec.add_bind_mount("/host/path", "/container/path", true);

        let mount = spec.mounts.last().unwrap();
        assert_eq!(mount.destination, "/container/path");
        assert_eq!(mount.source, "/host/path");
        assert!(mount.options.contains(&"ro".to_string()));
    }
}
