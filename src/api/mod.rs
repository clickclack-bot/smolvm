//! HTTP API server for smolvm.
//!
//! This module provides an HTTP API for managing sandboxes, containers, and images
//! without CLI overhead.
//!
//! # Example
//!
//! ```bash
//! # Start the server
//! smolvm serve --listen 127.0.0.1:8080
//!
//! # Create a sandbox
//! curl -X POST http://localhost:8080/api/v1/sandboxes \
//!   -H "Content-Type: application/json" \
//!   -d '{"name": "test"}'
//! ```

pub mod error;
pub mod handlers;
pub mod state;
pub mod types;

use axum::{
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use state::ApiState;

/// Create the API router with all endpoints.
pub fn create_router(state: Arc<ApiState>) -> Router {
    // Health check route
    let health_route = Router::new().route("/health", get(handlers::health::health));

    // Sandbox routes
    let sandbox_routes = Router::new()
        .route("/", post(handlers::sandboxes::create_sandbox))
        .route("/", get(handlers::sandboxes::list_sandboxes))
        .route("/:id", get(handlers::sandboxes::get_sandbox))
        .route("/:id/start", post(handlers::sandboxes::start_sandbox))
        .route("/:id/stop", post(handlers::sandboxes::stop_sandbox))
        .route("/:id", delete(handlers::sandboxes::delete_sandbox))
        // Exec routes
        .route("/:id/exec", post(handlers::exec::exec_command))
        .route("/:id/run", post(handlers::exec::run_command))
        .route("/:id/logs", get(handlers::exec::stream_logs))
        // Container routes
        .route("/:id/containers", post(handlers::containers::create_container))
        .route("/:id/containers", get(handlers::containers::list_containers))
        .route(
            "/:id/containers/:cid/start",
            post(handlers::containers::start_container),
        )
        .route(
            "/:id/containers/:cid/stop",
            post(handlers::containers::stop_container),
        )
        .route(
            "/:id/containers/:cid",
            delete(handlers::containers::delete_container),
        )
        .route(
            "/:id/containers/:cid/exec",
            post(handlers::containers::exec_in_container),
        )
        // Image routes
        .route("/:id/images", get(handlers::images::list_images))
        .route("/:id/images/pull", post(handlers::images::pull_image));

    // API v1 routes
    let api_v1 = Router::new().nest("/sandboxes", sandbox_routes);

    // Combine all routes
    Router::new()
        .merge(health_route)
        .nest("/api/v1", api_v1)
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(300)))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state)
}
