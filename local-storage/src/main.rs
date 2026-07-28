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

    // If DB has no files but disk has content, warn loudly instead of deleting anything.
    // A previous version of this service auto-deleted everything under STORAGE_PATH here
    // whenever the `files` table was empty (e.g. after pointing at a fresh/wrong DB) - that's
    // a data-loss trap, especially when migrating to a new host where the DB is freshly
    // provisioned but the storage volume still holds real files. Never delete automatically.
    let db_is_empty = match sqlx::query("SELECT COUNT(*) FROM files").fetch_one(&pool).await {
        Ok(row) => row.get::<i64, _>(0) == 0,
        Err(_) => false,
    };
    if db_is_empty {
        let storage_path = Path::new(&config.storage.path);
        if let Ok(mut entries) = fs::read_dir(storage_path).await {
            if entries.next_entry().await.ok().flatten().is_some() {
                warn!(
                    "⚠️ Database has no file records, but {} is not empty. \
                    This usually means the database was reset/repointed while old files remain on disk. \
                    Files on disk are orphaned from the DB's perspective and will not be served. \
                    Nothing was deleted - investigate before doing anything destructive.",
                    storage_path.display()
                );
            }
        }
    }

    // Create storage manager
    let storage = Arc::new(RwLock::new(StorageManager::new(config.clone(), pool).await?));
    info!("💾 Storage manager initialized");

    // Create concurrency semaphore to prevent overwhelming
    let request_semaphore = Arc::new(Semaphore::new(100)); // Max 100 concurrent requests
    info!("🚦 Concurrency limiter initialized (max: 100 concurrent requests)");

    // Create app state
    let s3_config = config.s3.clone().map(Arc::new);
    if s3_config.is_none() {
        info!("ℹ️ S3 v2 API disabled - set S3_ACCESS_KEY and S3_SECRET_KEY to enable it");
    }
    let state = AppState {
        storage,
        request_semaphore,
        s3_config,
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