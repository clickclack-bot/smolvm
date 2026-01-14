//! vsock client for communicating with the smolvm-agent.
//!
//! This module provides a client for sending requests to the agent
//! and receiving responses.

use crate::error::{Error, Result};
use crate::protocol::{
    encode_message, AgentRequest, AgentResponse, ImageInfo, OverlayInfo, StorageStatus,
};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

/// Client for communicating with the smolvm-agent.
pub struct AgentClient {
    stream: UnixStream,
}

impl AgentClient {
    /// Connect to the agent via Unix socket.
    ///
    /// # Arguments
    ///
    /// * `socket_path` - Path to the vsock Unix socket
    pub fn connect(socket_path: impl AsRef<Path>) -> Result<Self> {
        let stream = UnixStream::connect(socket_path.as_ref()).map_err(|e| {
            Error::AgentError(format!("failed to connect to agent: {}", e))
        })?;

        // Set timeouts
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .ok();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .ok();

        Ok(Self { stream })
    }

    /// Send a request and receive a response.
    fn request(&mut self, req: &AgentRequest) -> Result<AgentResponse> {
        // Encode and send request
        let data = encode_message(req).map_err(|e| Error::AgentError(e.to_string()))?;
        self.stream
            .write_all(&data)
            .map_err(|e| Error::AgentError(format!("write failed: {}", e)))?;

        // Read response
        self.read_response()
    }

    /// Read a response from the stream.
    fn read_response(&mut self) -> Result<AgentResponse> {
        // Read length header
        let mut header = [0u8; 4];
        self.stream
            .read_exact(&mut header)
            .map_err(|e| Error::AgentError(format!("read header failed: {}", e)))?;

        let len = u32::from_be_bytes(header) as usize;

        // Read payload
        let mut buf = vec![0u8; len];
        self.stream
            .read_exact(&mut buf)
            .map_err(|e| Error::AgentError(format!("read payload failed: {}", e)))?;

        // Parse response
        serde_json::from_slice(&buf).map_err(|e| Error::AgentError(format!("parse failed: {}", e)))
    }

    /// Ping the helper daemon.
    pub fn ping(&mut self) -> Result<u32> {
        let resp = self.request(&AgentRequest::Ping)?;

        match resp {
            AgentResponse::Pong { version } => Ok(version),
            AgentResponse::Error { message, .. } => Err(Error::AgentError(message)),
            _ => Err(Error::AgentError("unexpected response".into())),
        }
    }

    /// Pull an OCI image.
    ///
    /// # Arguments
    ///
    /// * `image` - Image reference (e.g., "alpine:latest")
    /// * `platform` - Optional platform (e.g., "linux/arm64")
    pub fn pull(&mut self, image: &str, platform: Option<&str>) -> Result<ImageInfo> {
        let resp = self.request(&AgentRequest::Pull {
            image: image.to_string(),
            platform: platform.map(String::from),
        })?;

        match resp {
            AgentResponse::Ok { data: Some(data) } => {
                serde_json::from_value(data).map_err(|e| Error::AgentError(e.to_string()))
            }
            AgentResponse::Error { message, .. } => Err(Error::AgentError(message)),
            _ => Err(Error::AgentError("unexpected response".into())),
        }
    }

    /// Query if an image exists locally.
    pub fn query(&mut self, image: &str) -> Result<Option<ImageInfo>> {
        let resp = self.request(&AgentRequest::Query {
            image: image.to_string(),
        })?;

        match resp {
            AgentResponse::Ok { data: Some(data) } => {
                let info: ImageInfo =
                    serde_json::from_value(data).map_err(|e| Error::AgentError(e.to_string()))?;
                Ok(Some(info))
            }
            AgentResponse::Error { code, .. } if code.as_deref() == Some("NOT_FOUND") => Ok(None),
            AgentResponse::Error { message, .. } => Err(Error::AgentError(message)),
            _ => Err(Error::AgentError("unexpected response".into())),
        }
    }

    /// List all cached images.
    pub fn list_images(&mut self) -> Result<Vec<ImageInfo>> {
        let resp = self.request(&AgentRequest::ListImages)?;

        match resp {
            AgentResponse::Ok { data: Some(data) } => {
                serde_json::from_value(data).map_err(|e| Error::AgentError(e.to_string()))
            }
            AgentResponse::Error { message, .. } => Err(Error::AgentError(message)),
            _ => Err(Error::AgentError("unexpected response".into())),
        }
    }

    /// Run garbage collection.
    ///
    /// # Arguments
    ///
    /// * `dry_run` - If true, only report what would be deleted
    pub fn garbage_collect(&mut self, dry_run: bool) -> Result<u64> {
        let resp = self.request(&AgentRequest::GarbageCollect { dry_run })?;

        match resp {
            AgentResponse::Ok { data: Some(data) } => {
                let freed = data["freed_bytes"].as_u64().unwrap_or(0);
                Ok(freed)
            }
            AgentResponse::Error { message, .. } => Err(Error::AgentError(message)),
            _ => Err(Error::AgentError("unexpected response".into())),
        }
    }

    /// Prepare an overlay filesystem for a workload.
    ///
    /// # Arguments
    ///
    /// * `image` - Image reference
    /// * `workload_id` - Unique workload identifier
    pub fn prepare_overlay(&mut self, image: &str, workload_id: &str) -> Result<OverlayInfo> {
        let resp = self.request(&AgentRequest::PrepareOverlay {
            image: image.to_string(),
            workload_id: workload_id.to_string(),
        })?;

        match resp {
            AgentResponse::Ok { data: Some(data) } => {
                serde_json::from_value(data).map_err(|e| Error::AgentError(e.to_string()))
            }
            AgentResponse::Error { message, .. } => Err(Error::AgentError(message)),
            _ => Err(Error::AgentError("unexpected response".into())),
        }
    }

    /// Clean up an overlay filesystem.
    pub fn cleanup_overlay(&mut self, workload_id: &str) -> Result<()> {
        let resp = self.request(&AgentRequest::CleanupOverlay {
            workload_id: workload_id.to_string(),
        })?;

        match resp {
            AgentResponse::Ok { .. } => Ok(()),
            AgentResponse::Error { message, .. } => Err(Error::AgentError(message)),
            _ => Err(Error::AgentError("unexpected response".into())),
        }
    }

    /// Format the storage disk.
    pub fn format_storage(&mut self) -> Result<()> {
        let resp = self.request(&AgentRequest::FormatStorage)?;

        match resp {
            AgentResponse::Ok { .. } => Ok(()),
            AgentResponse::Error { message, .. } => Err(Error::AgentError(message)),
            _ => Err(Error::AgentError("unexpected response".into())),
        }
    }

    /// Get storage status.
    pub fn storage_status(&mut self) -> Result<StorageStatus> {
        let resp = self.request(&AgentRequest::StorageStatus)?;

        match resp {
            AgentResponse::Ok { data: Some(data) } => {
                serde_json::from_value(data).map_err(|e| Error::AgentError(e.to_string()))
            }
            AgentResponse::Error { message, .. } => Err(Error::AgentError(message)),
            _ => Err(Error::AgentError("unexpected response".into())),
        }
    }

    /// Request agent shutdown.
    pub fn shutdown(&mut self) -> Result<()> {
        let resp = self.request(&AgentRequest::Shutdown)?;

        match resp {
            AgentResponse::Ok { .. } => Ok(()),
            AgentResponse::Error { message, .. } => Err(Error::AgentError(message)),
            _ => Err(Error::AgentError("unexpected response".into())),
        }
    }

    /// Run a command in an image's rootfs.
    ///
    /// # Arguments
    ///
    /// * `image` - Image reference (must be pulled first)
    /// * `command` - Command and arguments
    /// * `env` - Environment variables
    /// * `workdir` - Working directory inside the rootfs
    ///
    /// # Returns
    ///
    /// A tuple of (exit_code, stdout, stderr)
    pub fn run(
        &mut self,
        image: &str,
        command: Vec<String>,
        env: Vec<(String, String)>,
        workdir: Option<String>,
    ) -> Result<(i32, String, String)> {
        self.run_with_mounts(image, command, env, workdir, Vec::new())
    }

    /// Run a command in an image's rootfs with volume mounts.
    ///
    /// # Arguments
    ///
    /// * `image` - Image reference (must be pulled first)
    /// * `command` - Command and arguments
    /// * `env` - Environment variables
    /// * `workdir` - Working directory inside the rootfs
    /// * `mounts` - Volume mounts as (virtiofs_tag, container_path, read_only)
    ///
    /// # Returns
    ///
    /// A tuple of (exit_code, stdout, stderr)
    pub fn run_with_mounts(
        &mut self,
        image: &str,
        command: Vec<String>,
        env: Vec<(String, String)>,
        workdir: Option<String>,
        mounts: Vec<(String, String, bool)>,
    ) -> Result<(i32, String, String)> {
        // Set longer timeout for command execution
        self.stream
            .set_read_timeout(Some(Duration::from_secs(3600)))
            .ok();

        let resp = self.request(&AgentRequest::Run {
            image: image.to_string(),
            command,
            env,
            workdir,
            mounts,
        })?;

        // Reset timeout
        self.stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .ok();

        match resp {
            AgentResponse::Completed {
                exit_code,
                stdout,
                stderr,
            } => Ok((exit_code, stdout, stderr)),
            AgentResponse::Error { message, .. } => Err(Error::AgentError(message)),
            _ => Err(Error::AgentError("unexpected response".into())),
        }
    }
}
