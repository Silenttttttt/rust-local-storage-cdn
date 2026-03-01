use anyhow::Result;
use local_storage::{
    app::{create_router, AppState},
    config::Config,
    storage::StorageManager,
};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::{RwLock, Semaphore};
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing with more verbose logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "local_storage=debug,tower_http=debug,sqlx=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_target(true).with_line_number(true))
        .init();

    info!("🚀 Starting Local Storage Service (Rust)");

    // Load configuration
    let config = Config::load().await?;
    info!("📋 Configuration loaded successfully");

    // Create database pool
    let pool = sqlx::PgPool::connect(&config.database.url).await?;
    info!("📊 Database connection established");

    // Run migrations (path relative to CARGO_MANIFEST_DIR)
    let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations).await?.run(&pool).await?;
    info!("📦 Database migrations applied");

    // Create storage manager
    let storage = Arc::new(RwLock::new(StorageManager::new(config.clone(), pool).await?));
    info!("💾 Storage manager initialized");

    // Create concurrency semaphore to prevent overwhelming
    let request_semaphore = Arc::new(Semaphore::new(100)); // Max 100 concurrent requests
    info!("🚦 Concurrency limiter initialized (max: 100 concurrent requests)");

    // Create app state
    let state = AppState { 
        storage,
        request_semaphore,
    };
    info!("🔧 Application state created");

    // Create the router
    let app = create_router(state);
    info!("✅ Router initialized with concurrency limiting");

    // Start the server
    let addr = SocketAddr::from(([0, 0, 0, 0], config.server.port));
    info!("🌐 Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    match axum::serve(listener, app).await {
        Ok(_) => info!("✅ Server shutdown gracefully"),
        Err(e) => {
            error!("❌ Server error: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
} 