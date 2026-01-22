//! Command execution handlers.

use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use std::convert::Infallible;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::api::error::ApiError;
use crate::api::state::{mount_spec_to_host_mount, port_spec_to_mapping, resource_spec_to_vm_resources, ApiState};
use crate::api::types::{ExecRequest, ExecResponse, LogsQuery, RunRequest};

/// POST /api/v1/sandboxes/:id/exec - Execute a command in a sandbox.
///
/// This executes directly in the VM (not in a container).
pub async fn exec_command(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(req): Json<ExecRequest>,
) -> Result<Json<ExecResponse>, ApiError> {
    if req.command.is_empty() {
        return Err(ApiError::BadRequest("command cannot be empty".into()));
    }

    let entry = state.get_sandbox(&id)?;

    // Ensure sandbox is running
    {
        let entry = entry.lock();
        let mounts_result: Result<Vec<_>, _> = entry
            .mounts
            .iter()
            .map(mount_spec_to_host_mount)
            .collect();
        let mounts = mounts_result.map_err(|e| ApiError::Internal(e.to_string()))?;
        let ports: Vec<_> = entry.ports.iter().map(port_spec_to_mapping).collect();
        let resources = resource_spec_to_vm_resources(&entry.resources);

        entry
            .manager
            .ensure_running_with_full_config(mounts, ports, resources)
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    // Prepare execution parameters
    let command = req.command.clone();
    let env: Vec<(String, String)> = req
        .env
        .iter()
        .map(|e| (e.name.clone(), e.value.clone()))
        .collect();
    let workdir = req.workdir.clone();
    let timeout = req.timeout_secs.map(Duration::from_secs);

    // Execute in blocking task
    let entry_clone = entry.clone();
    let (exit_code, stdout, stderr) = tokio::task::spawn_blocking(move || {
        let entry = entry_clone.lock();
        let mut client = entry.manager.connect()?;
        client.vm_exec(command, env, workdir, timeout)
    })
    .await?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(ExecResponse {
        exit_code,
        stdout,
        stderr,
    }))
}

/// POST /api/v1/sandboxes/:id/run - Run a command in an image.
///
/// This creates a temporary overlay from the image and runs the command.
pub async fn run_command(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Json(req): Json<RunRequest>,
) -> Result<Json<ExecResponse>, ApiError> {
    if req.command.is_empty() {
        return Err(ApiError::BadRequest("command cannot be empty".into()));
    }

    let entry = state.get_sandbox(&id)?;

    // Ensure sandbox is running
    {
        let entry = entry.lock();
        let mounts_result: Result<Vec<_>, _> = entry
            .mounts
            .iter()
            .map(mount_spec_to_host_mount)
            .collect();
        let mounts = mounts_result.map_err(|e| ApiError::Internal(e.to_string()))?;
        let ports: Vec<_> = entry.ports.iter().map(port_spec_to_mapping).collect();
        let resources = resource_spec_to_vm_resources(&entry.resources);

        entry
            .manager
            .ensure_running_with_full_config(mounts, ports, resources)
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }

    // Prepare execution parameters
    let image = req.image.clone();
    let command = req.command.clone();
    let env: Vec<(String, String)> = req
        .env
        .iter()
        .map(|e| (e.name.clone(), e.value.clone()))
        .collect();
    let workdir = req.workdir.clone();
    let timeout = req.timeout_secs.map(Duration::from_secs);

    // Get mounts from sandbox config (converted to protocol format)
    let mounts_config = {
        let entry = entry.lock();
        entry
            .mounts
            .iter()
            .map(|m| {
                // Create virtiofs tag from source path
                let tag = format!("mount{}", m.source.replace('/', "_"));
                (tag, m.target.clone(), m.readonly)
            })
            .collect::<Vec<_>>()
    };

    // Execute in blocking task
    let entry_clone = entry.clone();
    let (exit_code, stdout, stderr) = tokio::task::spawn_blocking(move || {
        let entry = entry_clone.lock();
        let mut client = entry.manager.connect()?;
        client.run_with_mounts_and_timeout(&image, command, env, workdir, mounts_config, timeout)
    })
    .await?
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok(Json(ExecResponse {
        exit_code,
        stdout,
        stderr,
    }))
}

/// GET /api/v1/sandboxes/:id/logs - Stream sandbox console logs via SSE.
///
/// Query parameters:
/// - `follow`: If true, keep streaming new logs (like tail -f)
/// - `tail`: Number of lines to show from the end
pub async fn stream_logs(
    State(state): State<Arc<ApiState>>,
    Path(id): Path<String>,
    Query(query): Query<LogsQuery>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let entry = state.get_sandbox(&id)?;

    // Get console log path
    let log_path: PathBuf = {
        let entry = entry.lock();
        entry
            .manager
            .console_log()
            .ok_or_else(|| ApiError::NotFound("console log not configured".into()))?
            .to_path_buf()
    };

    // Check if file exists
    if !log_path.exists() {
        return Err(ApiError::NotFound(format!(
            "log file not found: {}",
            log_path.display()
        )));
    }

    let follow = query.follow;
    let tail = query.tail;

    // Create the SSE stream
    let stream = async_stream::stream! {
        // Open the log file
        let file = match std::fs::File::open(&log_path) {
            Ok(f) => f,
            Err(e) => {
                yield Ok(Event::default().data(format!("error: failed to open log file: {}", e)));
                return;
            }
        };

        let mut reader = BufReader::new(file);

        // If tail is specified, seek to show only last N lines
        if let Some(n) = tail {
            let lines: Vec<String> = reader.by_ref().lines().filter_map(|l: Result<String, _>| l.ok()).collect();
            let start = lines.len().saturating_sub(n);
            for line in lines.into_iter().skip(start) {
                yield Ok(Event::default().data(line));
            }

            if !follow {
                return;
            }

            // Re-open file for following (seek to end)
            let file = match std::fs::File::open(&log_path) {
                Ok(f) => f,
                Err(e) => {
                    yield Ok(Event::default().data(format!("error: failed to reopen log file: {}", e)));
                    return;
                }
            };
            reader = BufReader::new(file);
            if let Err(e) = reader.seek(SeekFrom::End(0)) {
                yield Ok(Event::default().data(format!("error: failed to seek: {}", e)));
                return;
            }
        }

        // Read existing content (or follow new content)
        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            match reader.read_line(&mut line_buf) {
                Ok(0) => {
                    // EOF reached
                    if follow {
                        // Wait a bit and try again
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    } else {
                        break;
                    }
                }
                Ok(_) => {
                    // Remove trailing newline
                    let line = line_buf.trim_end_matches('\n').trim_end_matches('\r');
                    yield Ok(Event::default().data(line.to_string()));
                }
                Err(e) => {
                    yield Ok(Event::default().data(format!("error: read failed: {}", e)));
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
