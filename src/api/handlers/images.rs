//! Image management handlers.

use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;

use crate::agent::PullOptions;
use crate::api::error::ApiError;
use crate::api::state::{ensure_sandbox_running, ApiState};
use crate::api::types::{
    ApiErrorResponse, ImageInfo, ListImagesResponse, PullImageRequest, PullImageResponse,
};

/// List images in a sandbox.
#[utoipa::path(
    get,
    path = "/api/v1/sandboxes/{id}/images",
    tag = "Images",
    params(
        ("id" = String, Path, description = "Sandbox name")
    ),
    responses(
        (status = 200, description = "List of images", body = ListImagesResponse),
        (status = 404, description = "Sandbox not found", body = ApiErrorResponse)
    )
)]
pub async fn list_images(
    State(state): State<Arc<ApiState>>,
    Path(sandbox_id): Path<String>,
) -> Result<Json<ListImagesResponse>, ApiError> {
    let entry = state.get_sandbox(&sandbox_id)?;

    // Check if sandbox is running, return empty list if not
    {
        let entry = entry.lock();
        if !entry.manager.is_running() {
            return Ok(Json(ListImagesResponse { images: Vec::new() }));
        }
    }

    // List images in blocking task
    let entry_clone = entry.clone();
    let images = tokio::task::spawn_blocking(move || {
        let entry = entry_clone.lock();
        let mut client = entry.manager.connect()?;
        client.list_images()
    })
    .await?
    .map_err(ApiError::internal)?;

    let images = images
        .into_iter()
        .map(|i| ImageInfo {
            reference: i.reference,
            digest: i.digest,
            size: i.size,
            architecture: i.architecture,
            os: i.os,
            layer_count: i.layer_count,
        })
        .collect();

    Ok(Json(ListImagesResponse { images }))
}

/// Pull an image into a sandbox.
#[utoipa::path(
    post,
    path = "/api/v1/sandboxes/{id}/images/pull",
    tag = "Images",
    params(
        ("id" = String, Path, description = "Sandbox name")
    ),
    request_body = PullImageRequest,
    responses(
        (status = 200, description = "Image pulled", body = PullImageResponse),
        (status = 400, description = "Invalid request", body = ApiErrorResponse),
        (status = 404, description = "Sandbox not found", body = ApiErrorResponse),
        (status = 500, description = "Failed to pull image", body = ApiErrorResponse)
    )
)]
pub async fn pull_image(
    State(state): State<Arc<ApiState>>,
    Path(sandbox_id): Path<String>,
    Json(req): Json<PullImageRequest>,
) -> Result<Json<PullImageResponse>, ApiError> {
    if req.image.is_empty() {
        return Err(ApiError::BadRequest(
            "image reference cannot be empty".into(),
        ));
    }

    let entry = state.get_sandbox(&sandbox_id)?;

    // Ensure sandbox is running
    ensure_sandbox_running(&entry)
        .await
        .map_err(ApiError::internal)?;

    // Pull image in blocking task
    let image = req.image.clone();
    let platform = req.platform.clone();
    let entry_clone = entry.clone();
    let image_info = tokio::task::spawn_blocking(move || {
        let entry = entry_clone.lock();
        let mut client = entry.manager.connect()?;
        let mut opts = PullOptions::new().use_registry_config(true);
        if let Some(p) = platform {
            opts = opts.platform(p);
        }
        client.pull(&image, opts)
    })
    .await?
    .map_err(ApiError::internal)?;

    Ok(Json(PullImageResponse {
        image: ImageInfo {
            reference: image_info.reference,
            digest: image_info.digest,
            size: image_info.size,
            architecture: image_info.architecture,
            os: image_info.os,
            layer_count: image_info.layer_count,
        },
    }))
}
