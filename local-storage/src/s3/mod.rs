pub mod auth;
pub mod error;
pub mod handlers;
pub mod xml;

use axum::{
    routing::{get, put, delete, post},
    Router,
};

use crate::app::AppState;

/// S3-compatible v2 API, path-style routing (bucket as first path segment) matching what
/// aws-cli/boto3/rclone/mc expect when pointed at this service with `--endpoint-url
/// http://host:port/v2`. Only mounted when S3_ACCESS_KEY/S3_SECRET_KEY are configured (see
/// app::create_router).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v2/", get(handlers::list_buckets))
        .route("/v2/:bucket", put(handlers::create_bucket))
        .route("/v2/:bucket", delete(handlers::delete_bucket))
        .route("/v2/:bucket", get(handlers::list_objects_v2))
        .route("/v2/:bucket", post(handlers::bucket_post))
        .route("/v2/:bucket/*key", put(handlers::put_object))
        .route("/v2/:bucket/*key", get(handlers::get_object))
        .route("/v2/:bucket/*key", axum::routing::head(handlers::head_object))
        .route("/v2/:bucket/*key", delete(handlers::delete_object))
}
