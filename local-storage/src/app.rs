use axum::{
    extract::State,
    http::{StatusCode, Method, Request},
    response::IntoResponse,
    middleware::{self, Next},
    body::Body,
    routing::{get, post, delete},
    Router,
};
use tower_http::cors::{CorsLayer, Any};
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;

use crate::{
    handlers::{
        bucket_handlers,
        file_handlers,
        health_handler,
    },
};

use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tracing::{warn, info};
use serde::Deserialize;
use std::time::Duration;

// Concurrency limits to prevent overwhelming the service
const MAX_CONCURRENT_REQUESTS: usize = 100;
const MAX_REQUEST_BODY_SIZE: usize = 1024 * 1024 * 1024; // 1GB (increased from 500MB)
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes (increased from 2 minutes)

#[derive(Debug, Deserialize, Clone)]
pub struct ListFilesQuery {
    pub prefix: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SearchQuery {
    pub bucket: Option<String>,
    pub query: String,
    pub limit: Option<i64>,
}

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<RwLock<crate::storage::StorageManager>>,
    pub request_semaphore: Arc<Semaphore>,
}



// Concurrency limiting middleware
async fn concurrency_limiter(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<impl IntoResponse, StatusCode> {
    let method = request.method().clone();
    let uri = request.uri().clone();
    
    let permit = match state.request_semaphore.try_acquire() {
        Ok(permit) => permit,
        Err(_) => {
            warn!("🚫 Request rejected - too many concurrent requests (limit: {})", MAX_CONCURRENT_REQUESTS);
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    };

    info!("📥 {} {} - Processing request", method, uri);
    
    let response = next.run(request).await;
    
    drop(permit);
    
    Ok(response)
}

pub fn create_router(state: AppState) -> Router {
    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::HEAD, Method::OPTIONS])
        .allow_headers(Any);

    // Create separate router for upload routes with extended timeout
    let upload_router = Router::new()
        .route("/buckets/:bucket/files", post(file_handlers::upload_file))
        .layer(
            ServiceBuilder::new()
                .layer(cors.clone())
        );

    // Main router for other routes
    let main_router = Router::new()
        .route("/health", get(health_handler::health_check))
        .route("/buckets/:bucket/files", get(file_handlers::list_files))
        .route("/buckets/:bucket/stats", get(bucket_handlers::get_bucket_stats))
        .route("/buckets/:bucket", delete(bucket_handlers::delete_bucket))
        .route("/buckets/:bucket", post(bucket_handlers::create_bucket))
        .route("/buckets", get(bucket_handlers::list_buckets))
        .route("/stats", get(bucket_handlers::get_storage_stats))
        .route("/search", get(file_handlers::search_files))
        .route("/buckets/:bucket/files/*path", get(file_handlers::handle_file_request))
        .route("/buckets/:bucket/files/*path", delete(file_handlers::handle_file_delete))
        .layer(
            ServiceBuilder::new()
                .layer(middleware::from_fn_with_state(state.clone(), concurrency_limiter))
                .layer(TimeoutLayer::new(REQUEST_TIMEOUT))
                .layer(cors)
        );

    // Merge routers
    upload_router.merge(main_router).with_state(state)
} 