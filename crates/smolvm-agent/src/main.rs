//! smolvm guest agent.
//!
//! This agent runs inside smolvm VMs and handles:
//! - OCI image pulling via crane
//! - Layer extraction and storage management
//! - Overlay filesystem preparation for workloads
//! - Command execution and output streaming (TODO)
//!
//! Communication is via vsock on port 6000.

use smolvm_protocol::{
    ports, DecodeError, AgentRequest, AgentResponse, ImageInfo, OverlayInfo, StorageStatus,
    PROTOCOL_VERSION,
};
use std::io::{Read, Write};
use tracing::{debug, error, info, warn};

mod storage;
mod vsock;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("smolvm_agent=debug".parse().unwrap()),
        )
        .init();

    info!(version = env!("CARGO_PKG_VERSION"), "starting smolvm-agent");

    // Initialize storage
    if let Err(e) = storage::init() {
        error!(error = %e, "failed to initialize storage");
        std::process::exit(1);
    }

    // Start vsock server
    if let Err(e) = run_server() {
        error!(error = %e, "server error");
        std::process::exit(1);
    }
}

/// Run the vsock server.
fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    let listener = vsock::listen(ports::AGENT_CONTROL)?;
    info!(port = ports::AGENT_CONTROL, "listening on vsock");

    loop {
        match listener.accept() {
            Ok(mut stream) => {
                info!("accepted connection");

                if let Err(e) = handle_connection(&mut stream) {
                    warn!(error = %e, "connection error");
                }
            }
            Err(e) => {
                warn!(error = %e, "accept error");
            }
        }
    }
}

/// Handle a single connection.
fn handle_connection(stream: &mut impl ReadWrite) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf = vec![0u8; 64 * 1024]; // 64KB buffer

    loop {
        // Read length header
        let mut header = [0u8; 4];
        match stream.read_exact(&mut header) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                debug!("connection closed");
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }

        let len = u32::from_be_bytes(header) as usize;
        if len > buf.len() {
            buf.resize(len, 0);
        }

        // Read payload
        stream.read_exact(&mut buf[..len])?;

        // Parse request
        let request: AgentRequest = match serde_json::from_slice(&buf[..len]) {
            Ok(req) => req,
            Err(e) => {
                warn!(error = %e, "invalid request");
                send_response(stream, &AgentResponse::Error {
                    message: format!("invalid request: {}", e),
                    code: Some("INVALID_REQUEST".to_string()),
                })?;
                continue;
            }
        };

        debug!(?request, "received request");

        // Handle request
        let response = handle_request(request);
        send_response(stream, &response)?;

        // Check for shutdown
        if matches!(response, AgentResponse::Ok { .. }) {
            // If this was a shutdown request, exit
            if let AgentResponse::Ok { data: Some(ref d) } = response {
                if d.get("shutdown").and_then(|v| v.as_bool()) == Some(true) {
                    info!("shutdown requested");
                    return Ok(());
                }
            }
        }
    }
}

/// Handle a single request.
fn handle_request(request: AgentRequest) -> AgentResponse {
    match request {
        AgentRequest::Ping => AgentResponse::Pong {
            version: PROTOCOL_VERSION,
        },

        AgentRequest::Pull { image, platform } => handle_pull(&image, platform.as_deref()),

        AgentRequest::Query { image } => handle_query(&image),

        AgentRequest::ListImages => handle_list_images(),

        AgentRequest::GarbageCollect { dry_run } => handle_gc(dry_run),

        AgentRequest::PrepareOverlay { image, workload_id } => {
            handle_prepare_overlay(&image, &workload_id)
        }

        AgentRequest::CleanupOverlay { workload_id } => handle_cleanup_overlay(&workload_id),

        AgentRequest::FormatStorage => handle_format_storage(),

        AgentRequest::StorageStatus => handle_storage_status(),

        AgentRequest::Shutdown => {
            info!("shutdown requested");
            AgentResponse::Ok {
                data: Some(serde_json::json!({"shutdown": true})),
            }
        }

        AgentRequest::Run {
            image,
            command,
            env,
            workdir,
            mounts,
        } => handle_run(&image, &command, &env, workdir.as_deref(), &mounts),
    }
}

/// Handle command execution request.
fn handle_run(
    image: &str,
    command: &[String],
    env: &[(String, String)],
    workdir: Option<&str>,
    mounts: &[(String, String, bool)],
) -> AgentResponse {
    info!(image = %image, command = ?command, mounts = ?mounts, "running command");

    match storage::run_command(image, command, env, workdir, mounts) {
        Ok(result) => AgentResponse::Completed {
            exit_code: result.exit_code,
            stdout: result.stdout,
            stderr: result.stderr,
        },
        Err(e) => AgentResponse::Error {
            message: e.to_string(),
            code: Some("RUN_FAILED".to_string()),
        },
    }
}

/// Handle image pull request.
fn handle_pull(image: &str, platform: Option<&str>) -> AgentResponse {
    info!(image = %image, ?platform, "pulling image");

    match storage::pull_image(image, platform) {
        Ok(info) => AgentResponse::Ok {
            data: Some(serde_json::to_value(info).unwrap()),
        },
        Err(e) => AgentResponse::Error {
            message: e.to_string(),
            code: Some("PULL_FAILED".to_string()),
        },
    }
}

/// Handle image query request.
fn handle_query(image: &str) -> AgentResponse {
    match storage::query_image(image) {
        Ok(Some(info)) => AgentResponse::Ok {
            data: Some(serde_json::to_value(info).unwrap()),
        },
        Ok(None) => AgentResponse::Error {
            message: format!("image not found: {}", image),
            code: Some("NOT_FOUND".to_string()),
        },
        Err(e) => AgentResponse::Error {
            message: e.to_string(),
            code: Some("QUERY_FAILED".to_string()),
        },
    }
}

/// Handle list images request.
fn handle_list_images() -> AgentResponse {
    match storage::list_images() {
        Ok(images) => AgentResponse::Ok {
            data: Some(serde_json::to_value(images).unwrap()),
        },
        Err(e) => AgentResponse::Error {
            message: e.to_string(),
            code: Some("LIST_FAILED".to_string()),
        },
    }
}

/// Handle garbage collection request.
fn handle_gc(dry_run: bool) -> AgentResponse {
    match storage::garbage_collect(dry_run) {
        Ok(freed) => AgentResponse::Ok {
            data: Some(serde_json::json!({
                "freed_bytes": freed,
                "dry_run": dry_run,
            })),
        },
        Err(e) => AgentResponse::Error {
            message: e.to_string(),
            code: Some("GC_FAILED".to_string()),
        },
    }
}

/// Handle overlay preparation request.
fn handle_prepare_overlay(image: &str, workload_id: &str) -> AgentResponse {
    info!(image = %image, workload_id = %workload_id, "preparing overlay");

    match storage::prepare_overlay(image, workload_id) {
        Ok(info) => AgentResponse::Ok {
            data: Some(serde_json::to_value(info).unwrap()),
        },
        Err(e) => AgentResponse::Error {
            message: e.to_string(),
            code: Some("OVERLAY_FAILED".to_string()),
        },
    }
}

/// Handle overlay cleanup request.
fn handle_cleanup_overlay(workload_id: &str) -> AgentResponse {
    info!(workload_id = %workload_id, "cleaning up overlay");

    match storage::cleanup_overlay(workload_id) {
        Ok(_) => AgentResponse::Ok { data: None },
        Err(e) => AgentResponse::Error {
            message: e.to_string(),
            code: Some("CLEANUP_FAILED".to_string()),
        },
    }
}

/// Handle storage format request.
fn handle_format_storage() -> AgentResponse {
    info!("formatting storage");

    match storage::format() {
        Ok(_) => AgentResponse::Ok { data: None },
        Err(e) => AgentResponse::Error {
            message: e.to_string(),
            code: Some("FORMAT_FAILED".to_string()),
        },
    }
}

/// Handle storage status request.
fn handle_storage_status() -> AgentResponse {
    match storage::status() {
        Ok(status) => AgentResponse::Ok {
            data: Some(serde_json::to_value(status).unwrap()),
        },
        Err(e) => AgentResponse::Error {
            message: e.to_string(),
            code: Some("STATUS_FAILED".to_string()),
        },
    }
}

/// Send a response to the client.
fn send_response(
    stream: &mut impl Write,
    response: &AgentResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_vec(response)?;
    let len = json.len() as u32;

    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(&json)?;
    stream.flush()?;

    debug!(?response, "sent response");
    Ok(())
}

/// Trait for read+write streams.
trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}
