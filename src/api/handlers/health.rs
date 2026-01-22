//! Health check endpoint.

use axum::Json;

use crate::api::types::HealthResponse;

/// GET /health - Health check endpoint.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: crate::VERSION,
    })
}
