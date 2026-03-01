use anyhow::Result;
use local_storage::{
    app::{create_router, AppState},
    config::Config,
    storage::StorageManager,
};
use std::{net::SocketAddr, path::Path, sync::Arc, time::Duration};
use sqlx::{postgres::PgConnectOptions, ConnectOptions, Row};
use tokio::fs;
use tokio::sync::{RwLock, Semaphore};
use tracing::{info, error, warn};
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

    // Create database pool (slow query threshold 5s to avoid noisy warnings on cold start / shared Postgres)
    let opts: PgConnectOptions = config.database.url.parse()?;
    let opts = opts.log_slow_statements(log::LevelFilter::Warn, Duration::from_secs(5));
    let pool = sqlx::PgPool::connect_with(opts).await?;
    info!("📊 Database connection established");

    // Run migrations (path relative to CARGO_MANIFEST_DIR)
    let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    sqlx::migrate::Migrator::new(migrations).await?.run(&pool).await?;
    info!("📦 Database migrations applied");

    // If DB has no files (e.g. after reset), clear storage to avoid orphan files on disk
    let should_clear_storage = match sqlx::query("SELECT COUNT(*) FROM files").fetch_one(&pool).await {
        Ok(row) => row.get::<i64, _>(0) == 0,
        Err(_) => false,
    };
    if should_clear_storage {
        let storage_path = Path::new(&config.storage.path);
        info!("🗑️ DB empty, clearing orphan files from {}", storage_path.display());
        if storage_path.exists() {
            // Clear contents only (storage_path may be a mount point - can't remove_dir_all on it)
            match fs::read_dir(storage_path).await {
                Ok(mut entries) => {
                    let mut cleared = 0u32;
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let path = entry.path();
                        if let Ok(ft) = entry.file_type().await {
                            let result = if ft.is_dir() {
                                fs::remove_dir_all(&path).await
                            } else {
                                fs::remove_file(&path).await
                            };
                            match result {
                                Ok(()) => cleared += 1,
                                Err(e) => warn!("⚠️ Failed to remove {}: {}", path.display(), e),
                            }
                        }
                    }
                    info!("🗑️ Cleared storage: removed {} orphan entries", cleared);
                }
                Err(e) => warn!("⚠️ Failed to read storage dir (orphan cleanup): {}", e),
            }
        } else {
            info!("🗑️ Storage path does not exist yet, nothing to clear");
        }
    }

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