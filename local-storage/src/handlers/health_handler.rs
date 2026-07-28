use axum::{
    extract::State,
    response::IntoResponse,
    http::StatusCode,
};

use crate::app::AppState;

/// Pings the database (and Redis, if that's the configured cache backend) so a broken DB
/// connection actually fails liveness/readiness instead of always reporting healthy. Response
/// shape is unchanged on success (200/"OK") so existing `wget --spider` style checks keep
/// working; only the failure case is new (503).
#[axum::debug_handler]
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let storage = state.storage.read().await;
    let (db_ok, redis_ok) = storage.health_check().await;

    if db_ok && redis_ok {
        (StatusCode::OK, "OK")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "UNHEALTHY")
    }
} 