//! Root filesystem management.
//!
//! This module provides abstractions for preparing and managing guest root filesystems.
//! Currently supports direct path to a rootfs directory.

use crate::error::Result;
use std::path::PathBuf;

/// A prepared root filesystem.
///
/// This trait abstracts over different rootfs sources, providing a uniform
/// interface for accessing the mounted filesystem path.
pub trait Rootfs: Send {
    /// Get the path to the mounted rootfs.
    fn path(&self) -> &PathBuf;

    /// Cleanup the rootfs (unmount, etc.).
    fn cleanup(&mut self) -> Result<()>;
}

/// A simple path-based rootfs (no cleanup needed).
pub struct PathRootfs {
    path: PathBuf,
}

impl PathRootfs {
    /// Create a new path-based rootfs.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl Rootfs for PathRootfs {
    fn path(&self) -> &PathBuf {
        &self.path
    }

    fn cleanup(&mut self) -> Result<()> {
        // Nothing to cleanup for a simple path
        Ok(())
    }
}
