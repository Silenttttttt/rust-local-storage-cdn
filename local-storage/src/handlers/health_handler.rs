use axum::{
    extract::State,
    response::IntoResponse,
    http::StatusCode,
};

use crate::app::AppState;

/// Lightweight liveness/readiness check - just confirms the HTTP server itself is up and
/// responding, with no downstream dependency check at all. This is what Kubernetes' own
/// liveness/readiness probes poll every 10-30s, so it must not touch the database: on a
/// scale-to-zero-managed backend, a DB ping here would count as real traffic on every probe
/// tick and keep the database from ever idling out as long as this app itself stays warm.
/// Also matches the standard liveness-probe pattern of "is the process alive", not "are all
/// its dependencies healthy" - restarting this process fixes neither a real DB outage nor
/// (in this single-replica homelab setup) gains anything a restart of the DB itself wouldn't.
/// For an actual DB/cache connectivity check, use /health/deep.
pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Pings the database (and Redis, if that's the configured cache backend) for a real
/// connectivity check - intended for manual/monitoring use, not automatic k8s polling.
#[axum::debug_handler]
pub async fn health_check_deep(State(state): State<AppState>) -> impl IntoResponse {
    let storage = state.storage.read().await;
    let (db_ok, redis_ok) = storage.health_check().await;

    if db_ok && redis_ok {
        (StatusCode::OK, "OK")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "UNHEALTHY")
    }
} 