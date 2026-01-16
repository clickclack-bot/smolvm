//! Shared utility functions.

use std::time::{SystemTime, UNIX_EPOCH};

/// Get current timestamp as seconds since Unix epoch.
///
/// Returns the timestamp as a simple string (e.g., "1705312345").
pub fn current_timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    format!("{}", duration.as_secs())
}
