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

    // Lazy pool: doesn't attempt a real connection until first actual use, instead of
    // blocking here until Postgres accepts one. This app used to have its own readiness
    // (and thus the activator's cold-start cascade, when both wake in parallel) bounded
    // below by however long POSTGRES itself takes to boot, even though nothing about
    // starting the HTTP listener needs a live DB connection at all.
    let opts: PgConnectOptions = config.database.url.parse()?;
    let opts = opts.log_slow_statements(log::LevelFilter::Warn, Duration::from_secs(5));
    let pool = sqlx::postgres::PgPoolOptions::new().connect_lazy_with(opts);
    info!("📊 Database pool created (lazy - connects on first real use)");

    // Migrations and the orphaned-files sanity check both need a real connection, but
    // neither has to gate request-serving: migrations rarely change (this schema is
    // mature), and the sanity check is advisory logging only. Run them in the background
    // with their own bounded retry, since Postgres may still be cold-starting at the same
    // moment as this app - a request landing before either finishes just hits the DB's
    // normal transient-error handling, which the app already has to tolerate at runtime
    // regardless (a Postgres blip doesn't crash a long-running server).
    tokio::spawn(run_background_db_init(pool.clone(), config.clone()));

    // Create storage manager - doesn't itself run any query at construction (each of its
    // sub-managers just stores the pool handle), so it's safe to build immediately against
    // the still-lazy pool rather than waiting on the background init above.
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
    let download_protection_token = config.download_protection_token.clone().map(Arc::new);
    if download_protection_token.is_none() {
        info!("ℹ️ Read/download protection disabled - set WRITE_PROTECTION_TOKEN to require X-Activator-Write-Token on GET routes too");
    } else {
        info!("🔒 Read/download protection enabled - GET file/list/search routes now require X-Activator-Write-Token");
    }
    let state = AppState {
        storage,
        request_semaphore,
        s3_config,
        download_protection_token,
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

/// Runs migrations, then the orphaned-files sanity check, entirely off the server-startup
/// path (see the comment above where this is spawned in `main`). Retries the migration step
/// on its own for a while rather than giving up on the first attempt, since Postgres may
/// still be cold-starting at the same moment as this app.
async fn run_background_db_init(pool: sqlx::PgPool, config: Config) {
    let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let migrator = match sqlx::migrate::Migrator::new(migrations).await {
        Ok(m) => m,
        Err(e) => {
            error!("❌ Failed to load migrations: {}", e);
            return;
        }
    };

    const MAX_ATTEMPTS: u32 = 30; // 30 * 500ms = 15s of retry budget
    let mut attempts = 0;
    loop {
        match migrator.run(&pool).await {
            Ok(()) => {
                info!("📦 Database migrations applied");
                break;
            }
            Err(e) if attempts < MAX_ATTEMPTS => {
                attempts += 1;
                warn!("⏳ Migrations not ready yet (attempt {attempts}/{MAX_ATTEMPTS}): {e}");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(e) => {
                error!("❌ Database migrations failed after {attempts} attempts: {}", e);
                return;
            }
        }
    }

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
}
