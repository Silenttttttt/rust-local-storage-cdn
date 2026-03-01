use axum::{
    extract::{State, Path},
    http::StatusCode,
    Json,
    response::IntoResponse,
};

use crate::{
    errors::Result,
    app::AppState,
};

#[axum::debug_handler]
pub async fn create_bucket(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Result<impl IntoResponse> {
    let storage = state.storage.read().await;
    storage.create_bucket(&bucket).await?;
    Ok(StatusCode::CREATED)
}

#[axum::debug_handler]
pub async fn delete_bucket(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Result<impl IntoResponse> {
    let storage = state.storage.read().await;
    storage.delete_bucket(&bucket).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
pub async fn list_buckets(
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let storage = state.storage.read().await;
    let buckets = storage.list_buckets().await?;
    Ok(Json(buckets))
}

#[axum::debug_handler]
pub async fn get_bucket_stats(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Result<impl IntoResponse> {
    let storage = state.storage.read().await;
    let stats = storage.get_bucket_stats(&bucket).await?;
    Ok(Json(stats))
}

#[axum::debug_handler]
pub async fn get_storage_stats(
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let storage = state.storage.read().await;
    let stats = storage.get_storage_stats().await?;
    Ok(Json(stats))
} 