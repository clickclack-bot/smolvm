//! Process management utilities.
//!
//! This module provides utilities for managing child processes,
//! including signal handling and graceful shutdown.

use std::time::{Duration, Instant};

use crate::error::{Error, Result};

/// Default timeout for graceful shutdown before SIGKILL.
pub const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// Default timeout for SIGKILL to take effect.
pub const SIGKILL_WAIT: Duration = Duration::from_millis(500);

/// Check if a process is alive.
///
/// Returns true if the process exists and is running.
pub fn is_alive(pid: libc::pid_t) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Wait for a process to exit (non-blocking check).
///
/// Returns `Some(exit_code)` if the process has exited, `None` if still running.
pub fn try_wait(pid: libc::pid_t) -> Option<i32> {
    let mut status: libc::c_int = 0;
    let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };

    if result == pid {
        // Process exited
        let exit_code = if libc::WIFEXITED(status) {
            libc::WEXITSTATUS(status)
        } else if libc::WIFSIGNALED(status) {
            128 + libc::WTERMSIG(status)
        } else {
            -1
        };
        Some(exit_code)
    } else if result < 0 {
        // Error (process doesn't exist or not our child)
        Some(-1)
    } else {
        // Still running
        None
    }
}

/// Wait for a process to exit (blocking).
///
/// Returns the exit code.
pub fn wait(pid: libc::pid_t) -> i32 {
    let mut status: libc::c_int = 0;
    let result = unsafe { libc::waitpid(pid, &mut status, 0) };

    if result < 0 {
        return -1;
    }

    if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else if libc::WIFSIGNALED(status) {
        128 + libc::WTERMSIG(status)
    } else {
        -1
    }
}

/// Send SIGTERM to a process.
///
/// Returns true if the signal was sent successfully.
pub fn terminate(pid: libc::pid_t) -> bool {
    unsafe { libc::kill(pid, libc::SIGTERM) == 0 }
}

/// Send SIGKILL to a process.
///
/// Returns true if the signal was sent successfully.
pub fn kill(pid: libc::pid_t) -> bool {
    unsafe { libc::kill(pid, libc::SIGKILL) == 0 }
}

/// Gracefully stop a process.
///
/// 1. Sends SIGTERM
/// 2. Waits up to `timeout` for graceful exit
/// 3. If still running and `force` is true, sends SIGKILL
///
/// Returns `Ok(exit_code)` on success, `Err` if timeout without force.
pub fn stop_process(pid: libc::pid_t, timeout: Duration, force: bool) -> Result<i32> {
    // Check if already dead
    if !is_alive(pid) {
        // Try to reap zombie
        if let Some(code) = try_wait(pid) {
            return Ok(code);
        }
        return Ok(0);
    }

    // Send SIGTERM
    if !terminate(pid) {
        // Process already dead
        return Ok(try_wait(pid).unwrap_or(0));
    }

    // Wait for graceful exit
    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    while start.elapsed() < timeout {
        if let Some(code) = try_wait(pid) {
            return Ok(code);
        }

        if !is_alive(pid) {
            return Ok(try_wait(pid).unwrap_or(0));
        }

        std::thread::sleep(poll_interval);
    }

    // Timeout reached
    if force {
        tracing::debug!(pid = pid, "SIGTERM timeout, sending SIGKILL");
        kill(pid);

        // Wait for SIGKILL to take effect
        std::thread::sleep(SIGKILL_WAIT);

        // Reap the process
        Ok(wait(pid))
    } else {
        Err(Error::vm_creation(format!(
            "timeout waiting for process {} to stop",
            pid
        )))
    }
}

/// A handle to a running child process.
///
/// Provides methods to check status, stop, and kill the process.
#[derive(Debug)]
pub struct ChildProcess {
    pid: libc::pid_t,
    exit_code: Option<i32>,
}

impl ChildProcess {
    /// Create a new child process handle.
    pub fn new(pid: libc::pid_t) -> Self {
        Self {
            pid,
            exit_code: None,
        }
    }

    /// Get the process ID.
    pub fn pid(&self) -> libc::pid_t {
        self.pid
    }

    /// Check if the process is still running.
    pub fn is_running(&mut self) -> bool {
        if self.exit_code.is_some() {
            return false;
        }

        if let Some(code) = try_wait(self.pid) {
            self.exit_code = Some(code);
            false
        } else {
            is_alive(self.pid)
        }
    }

    /// Get the exit code if the process has exited.
    pub fn exit_code(&mut self) -> Option<i32> {
        if self.exit_code.is_none() {
            self.exit_code = try_wait(self.pid);
        }
        self.exit_code
    }

    /// Wait for the process to exit (blocking).
    pub fn wait(&mut self) -> i32 {
        if let Some(code) = self.exit_code {
            return code;
        }

        let code = wait(self.pid);
        self.exit_code = Some(code);
        code
    }

    /// Send SIGTERM to the process.
    pub fn terminate(&self) -> bool {
        terminate(self.pid)
    }

    /// Send SIGKILL to the process.
    pub fn kill(&self) -> bool {
        kill(self.pid)
    }

    /// Gracefully stop the process.
    ///
    /// Sends SIGTERM, waits for `timeout`, then SIGKILL if `force` is true.
    pub fn stop(&mut self, timeout: Duration, force: bool) -> Result<i32> {
        if let Some(code) = self.exit_code {
            return Ok(code);
        }

        let code = stop_process(self.pid, timeout, force)?;
        self.exit_code = Some(code);
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_alive_self() {
        // Current process should be alive
        let pid = unsafe { libc::getpid() };
        assert!(is_alive(pid));
    }

    #[test]
    fn test_is_alive_nonexistent() {
        // PID 99999999 is unlikely to exist
        assert!(!is_alive(99999999));
    }
}
