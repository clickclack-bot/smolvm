//! HTTP API server command.

use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;

use smolvm::api::state::ApiState;
use smolvm::Result;

/// Start the HTTP API server.
#[derive(Parser, Debug)]
pub struct ServeCmd {
    /// Listen address.
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    listen: String,

    /// Enable verbose logging.
    #[arg(short, long)]
    verbose: bool,
}

impl ServeCmd {
    /// Run the serve command.
    pub fn run(self) -> Result<()> {
        // Parse listen address
        let addr: SocketAddr = self.listen.parse().map_err(|e| {
            smolvm::error::Error::Config(format!("invalid listen address '{}': {}", self.listen, e))
        })?;

        // Set up verbose logging if requested
        if self.verbose {
            // Re-initialize logging at debug level
            // Note: This won't work if logging is already initialized,
            // but the RUST_LOG env var can be used instead
            tracing::info!("verbose logging enabled");
        }

        // Create the runtime and run the server
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| smolvm::error::Error::Io(e))?;

        runtime.block_on(async move {
            self.run_server(addr).await
        })
    }

    async fn run_server(self, addr: SocketAddr) -> Result<()> {
        // Create shared state
        let state = Arc::new(ApiState::new());

        // Create router
        let app = smolvm::api::create_router(state);

        // Create listener
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| smolvm::error::Error::Io(e))?;

        tracing::info!(address = %addr, "starting HTTP API server");
        println!("smolvm API server listening on http://{}", addr);

        // Run the server
        axum::serve(listener, app)
            .await
            .map_err(|e| smolvm::error::Error::Io(e))?;

        Ok(())
    }
}
