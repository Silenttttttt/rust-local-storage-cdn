use axum::{
    response::IntoResponse,
    http::StatusCode,
};

#[axum::debug_handler]
pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
} 