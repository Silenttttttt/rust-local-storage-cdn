use crate::{
    config::{CacheBackendKind, Config},
    errors::{Result, StorageError},
};
use crate::models::{CacheConfig, StoredFile};
use redis::AsyncCommands;
use serde_json;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use serde::{Serialize, Deserialize, de::DeserializeOwned};
use uuid::Uuid;

/// Either backend stores raw bytes keyed by string - metadata/JSON values are serialized before
/// going in, regardless of which backend is active, so the rest of the cache API is
/// backend-agnostic.
enum Backend {
    Memory(moka::future::Cache<String, Vec<u8>>),
    Redis(redis::Client),
}

/// Handles both the simple read-through cache (metadata + small file content, in-process by
/// default or Redis if CACHE_BACKEND=redis) and the DB-driven popularity-based pre-caching
/// service described by the `cache_config` table. Previously these lived in two separate, never-
/// fully-wired structs (`cache::CacheManager` and `cache_manager::CacheManager`) - consolidated
/// here since they cache the same underlying data.
#[derive(Clone)]
pub struct CacheManager {
    config: Arc<Config>,
    pool: PgPool,
    backend: Arc<Backend>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_keys: u64,
    pub max_size_gb: f64,
    pub ttl_seconds: u64,
    pub preload_enabled: bool,
    pub backend: String,
}

impl CacheManager {
    pub async fn new(config: Arc<Config>, pool: PgPool) -> Result<Self> {
        let backend = match config.cache.backend {
            CacheBackendKind::Redis => match redis::Client::open(config.redis_url()) {
                Ok(client) => {
                    info!("✅ Cache backend: Redis");
                    Backend::Redis(client)
                }
                Err(e) => {
                    warn!("⚠️ Redis URL invalid ({}), falling back to in-process cache", e);
                    Backend::Memory(Self::build_memory_cache(&config))
                }
            },
            CacheBackendKind::Memory => {
                info!("✅ Cache backend: in-process ({}MB max)", config.cache.max_size_mb);
                Backend::Memory(Self::build_memory_cache(&config))
            }
        };

        Ok(Self { config, pool, backend: Arc::new(backend) })
    }

    fn build_memory_cache(config: &Config) -> moka::future::Cache<String, Vec<u8>> {
        moka::future::Cache::builder()
            .max_capacity(config.cache.max_size_mb * 1024 * 1024)
            .weigher(|_key: &String, value: &Vec<u8>| -> u32 {
                value.len().try_into().unwrap_or(u32::MAX)
            })
            .time_to_live(Duration::from_secs(config.cache.ttl_seconds))
            .build()
    }

    pub fn is_enabled(&self) -> bool {
        true // both backends are always "available" - Memory can't be down, Redis degrades gracefully per-call
    }

    // ---- Low-level backend primitives ----

    async fn backend_get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        match &*self.backend {
            Backend::Memory(cache) => Ok(cache.get(key).await),
            Backend::Redis(client) => {
                let mut conn = client.get_async_connection().await?;
                let value: Option<Vec<u8>> = conn.get(key).await?;
                Ok(value)
            }
        }
    }

    async fn backend_set(&self, key: &str, value: Vec<u8>) -> Result<()> {
        match &*self.backend {
            Backend::Memory(cache) => {
                cache.insert(key.to_string(), value).await;
                Ok(())
            }
            Backend::Redis(client) => {
                let mut conn = client.get_async_connection().await?;
                let _: () = conn.set_ex(key, value, self.config.cache.ttl_seconds).await?;
                Ok(())
            }
        }
    }

    async fn backend_del(&self, keys: &[String]) -> Result<()> {
        match &*self.backend {
            Backend::Memory(cache) => {
                for key in keys {
                    cache.invalidate(key).await;
                }
                Ok(())
            }
            Backend::Redis(client) => {
                let mut conn = client.get_async_connection().await?;
                let _: () = conn.del(keys).await?;
                Ok(())
            }
        }
    }

    // ---- Read-through cache used directly by StorageManager on the hot path ----

    pub async fn get_file_metadata(&self, bucket: &str, key: &str) -> Result<Option<StoredFile>> {
        let cache_key = format!("file:{}:{}", bucket, key);
        match self.backend_get(&cache_key).await {
            Ok(Some(bytes)) => match serde_json::from_slice::<StoredFile>(&bytes) {
                Ok(file) => {
                    debug!("📋 Cache hit for file metadata: {}", cache_key);
                    Ok(Some(file))
                }
                Err(e) => {
                    warn!("❌ Failed to deserialize cached file metadata: {}", e);
                    let _ = self.backend_del(&[cache_key]).await;
                    Ok(None)
                }
            },
            Ok(None) => Ok(None),
            Err(e) => {
                error!("❌ Cache get error: {}", e);
                Err(e)
            }
        }
    }

    pub async fn set_file_metadata(&self, bucket: &str, key: &str, file: &StoredFile) -> Result<()> {
        let cache_key = format!("file:{}:{}", bucket, key);
        let serialized = serde_json::to_vec(file)?;
        self.backend_set(&cache_key, serialized).await?;
        debug!("💾 Cached file metadata: {}", cache_key);
        Ok(())
    }

    pub async fn get_file_content(&self, bucket: &str, key: &str) -> Result<Option<Vec<u8>>> {
        let cache_key = format!("content:{}:{}", bucket, key);
        let result = self.backend_get(&cache_key).await?;
        if result.is_some() {
            debug!("🗂️ Cache hit for file content: {}", cache_key);
        }
        Ok(result)
    }

    /// Cache small file content. Files above 1MB are skipped to avoid blowing up cache memory
    /// (Redis or in-process) with content that's cheap enough to re-read from disk anyway.
    pub async fn set_file_content(&self, bucket: &str, key: &str, content: &[u8]) -> Result<()> {
        const MAX_CACHE_SIZE: usize = 1024 * 1024; // 1MB
        if content.len() > MAX_CACHE_SIZE {
            debug!("📁 File too large to cache: {} bytes", content.len());
            return Ok(());
        }

        let cache_key = format!("content:{}:{}", bucket, key);
        self.backend_set(&cache_key, content.to_vec()).await?;
        debug!("💾 Cached file content: {} ({} bytes)", cache_key, content.len());
        Ok(())
    }

    pub async fn invalidate_file(&self, bucket: &str, key: &str) -> Result<()> {
        let metadata_key = format!("file:{}:{}", bucket, key);
        let content_key = format!("content:{}:{}", bucket, key);
        self.backend_del(&[metadata_key, content_key]).await?;
        debug!("🗑️ Invalidated cache for: {}/{}", bucket, key);
        Ok(())
    }

    pub async fn health_check(&self) -> bool {
        match &*self.backend {
            Backend::Memory(_) => true,
            Backend::Redis(client) => match client.get_async_connection().await {
                Ok(mut conn) => {
                    let pong: std::result::Result<String, redis::RedisError> =
                        redis::cmd("PING").query_async(&mut conn).await;
                    pong.is_ok()
                }
                Err(_) => false,
            },
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.backend_get(key).await? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.backend_set(key, serde_json::to_vec(value)?).await
    }

    // ---- DB-backed popularity pre-caching service (cache_config table) ----

    pub async fn get_cache_config(&self) -> Result<CacheConfig> {
        let config = sqlx::query_as!(
            CacheConfig,
            "SELECT id, max_cache_size_gb, cache_ttl_seconds, preload_enabled, min_access_count, cache_priority_weights, auto_cache_threshold, updated_at FROM cache_config WHERE id = 1"
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(config)
    }

    pub async fn update_cache_status(&self, file_id: Uuid, status: &str, cache_hits: Option<i64>) -> Result<()> {
        sqlx::query!(
            "UPDATE files SET cache_status = $1, last_cache_update = NOW(), cache_hits = COALESCE($2, cache_hits) WHERE id = $3",
            status,
            cache_hits,
            file_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Most accessed files that aren't cached yet, ordered by cache priority/access.
    /// Uses the runtime-checked query form (not `query_as!`) since this is a relocated query
    /// with no matching entry in the offline sqlx cache, and there's no live DB here to
    /// regenerate it against.
    async fn get_popular_uncached_files(&self, min_access_count: i64, limit: i64) -> Result<Vec<StoredFile>> {
        let files = sqlx::query_as::<_, StoredFile>(
            r#"
            SELECT *
            FROM files
            WHERE access_count >= $1
              AND cache_status != 'cached'
              AND file_size <= 10485760  -- 10MB max
            ORDER BY
                cache_priority DESC,
                access_count DESC,
                last_accessed DESC NULLS LAST
            LIMIT $2
            "#
        )
        .bind(min_access_count)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(files)
    }

    /// Pre-cache popular files, respecting the configured max cache size. Useful for both
    /// backends - it warms the in-process cache just as well as Redis.
    pub async fn preload_popular_files(&self) -> Result<()> {
        let cache_config = self.get_cache_config().await?;
        if !cache_config.preload_enabled.unwrap_or(false) {
            return Ok(());
        }

        let max_cache_size_bytes = (cache_config.max_cache_size_gb.unwrap_or(1.0) * 1024.0 * 1024.0 * 1024.0) as u64;
        let min_access_count = cache_config.min_access_count.unwrap_or(5) as i64;
        let mut current_cache_size = 0u64;
        let mut cached_files = 0u32;

        let popular_files = self.get_popular_uncached_files(min_access_count, 1000).await?;

        for file in popular_files {
            if current_cache_size >= max_cache_size_bytes {
                break;
            }

            match self.cache_file_from_disk(&file).await {
                Ok(cached_size) => {
                    self.update_cache_status(file.id, "cached", None).await?;
                    current_cache_size += cached_size;
                    cached_files += 1;
                }
                Err(e) => {
                    warn!("Failed to cache file {}: {}", file.id, e);
                    self.update_cache_status(file.id, "not_cached", None).await?;
                }
            }
        }

        info!(
            "Pre-cached {} files, total size: {:.2} MB",
            cached_files,
            current_cache_size as f64 / (1024.0 * 1024.0)
        );

        Ok(())
    }

    async fn cache_file_from_disk(&self, file: &StoredFile) -> Result<u64> {
        let file_path = std::path::Path::new(&file.file_path);
        if !file_path.exists() {
            return Err(StorageError::NotFound { bucket: file.bucket.clone(), key: file.key.clone() });
        }

        let content = tokio::fs::read(file_path).await?;
        self.set_file_content(&file.bucket, &file.key, &content).await?;

        Ok(content.len() as u64)
    }

    pub async fn clear_cache(&self) -> Result<()> {
        match &*self.backend {
            Backend::Memory(cache) => cache.invalidate_all(),
            Backend::Redis(client) => {
                let mut conn = client.get_async_connection().await?;
                let _: () = redis::cmd("FLUSHDB").query_async(&mut conn).await
                    .map_err(|e| StorageError::Cache(format!("Failed to clear cache: {}", e)))?;
            }
        }

        sqlx::query!(
            "UPDATE files SET cache_status = 'not_cached', last_cache_update = NOW() WHERE cache_status = 'cached'"
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_cache_stats(&self) -> Result<CacheStats> {
        let cache_config = self.get_cache_config().await?;

        let (total_keys, backend_name) = match &*self.backend {
            Backend::Memory(cache) => (cache.entry_count(), "memory".to_string()),
            Backend::Redis(client) => {
                let count = match client.get_async_connection().await {
                    Ok(mut conn) => redis::cmd("DBSIZE").query_async(&mut conn).await.unwrap_or(0u64),
                    Err(_) => 0,
                };
                (count, "redis".to_string())
            }
        };

        Ok(CacheStats {
            total_keys,
            max_size_gb: cache_config.max_cache_size_gb.unwrap_or(1.0),
            ttl_seconds: cache_config.cache_ttl_seconds.unwrap_or(3600) as u64,
            preload_enabled: cache_config.preload_enabled.unwrap_or(false),
            backend: backend_name,
        })
    }

    /// Runs `preload_popular_files` on a fixed interval for as long as the process is alive.
    pub fn start_preload_service(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                if let Err(e) = self.preload_popular_files().await {
                    error!("Preload service error: {}", e);
                }
                sleep(Duration::from_secs(1800)).await; // every 30 minutes
            }
        });
    }
}
