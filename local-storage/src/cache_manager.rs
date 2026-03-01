use crate::config::Config;
use crate::errors::{Result, StorageError};
use crate::models::{CacheConfig, StoredFile};
use redis::{Client, Commands};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

pub struct CacheManager {
    pool: PgPool,
    redis_client: Client,
    config: Arc<Config>,
}

impl CacheManager {
    pub fn new(pool: PgPool, redis_url: &str, config: Arc<Config>) -> Result<Self> {
        let redis_client = Client::open(redis_url)
            .map_err(|e| StorageError::Cache(format!("Failed to create Redis client: {}", e)))?;

        Ok(Self {
            pool,
            redis_client,
            config,
        })
    }

    /// Get cache configuration from database
    pub async fn get_cache_config(&self) -> Result<CacheConfig> {
        let config = sqlx::query_as!(
            CacheConfig,
            "SELECT id, max_cache_size_gb, cache_ttl_seconds, preload_enabled, min_access_count, cache_priority_weights, auto_cache_threshold, updated_at FROM cache_config WHERE id = 1"
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(config)
    }

    /// Update cache configuration
    pub async fn update_cache_config(
        &self,
        max_cache_size_gb: f64,
        cache_ttl_seconds: i32,
        preload_enabled: bool,
        min_access_count: i32,
        auto_cache_threshold: i32,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE cache_config SET max_cache_size_gb = $1, cache_ttl_seconds = $2, preload_enabled = $3, min_access_count = $4, auto_cache_threshold = $5, updated_at = NOW() WHERE id = 1",
            max_cache_size_gb,
            cache_ttl_seconds,
            preload_enabled,
            min_access_count,
            auto_cache_threshold
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update file's cache status
    pub async fn update_cache_status(
        &self,
        file_id: Uuid,
        status: &str,
        cache_hits: Option<i64>,
    ) -> Result<()> {
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

    /// Get most accessed files for pre-caching
    pub async fn get_popular_files(&self, limit: i64) -> Result<Vec<StoredFile>> {
        let cache_config = self.get_cache_config().await?;
        let min_access_count = cache_config.min_access_count.unwrap_or(5) as i64;

        let files = sqlx::query_as!(
            StoredFile,
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
            "#,
            min_access_count,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(files)
    }

    /// Pre-cache popular files in Redis
    pub async fn preload_popular_files(&self) -> Result<()> {
        let cache_config = self.get_cache_config().await?;
        
        if !cache_config.preload_enabled.unwrap_or(false) {
            return Ok(());
        }

        let max_cache_size_bytes = (cache_config.max_cache_size_gb.unwrap_or(1.0) * 1024.0 * 1024.0 * 1024.0) as u64;
        let mut current_cache_size = 0u64;
        let mut cached_files = 0u32;

        // Get popular files
        let popular_files = self.get_popular_files(1000).await?;

        for file in popular_files {
            // Check if we've reached the cache size limit
            if current_cache_size >= max_cache_size_bytes {
                break;
            }

            // Try to read file content and cache it
            match self.cache_file_content(&file).await {
                Ok(cached_size) => {
                    self.update_cache_status(file.id, "cached", None).await?;
                    current_cache_size += cached_size;
                    cached_files += 1;
                }
                Err(e) => {
                    log::warn!("Failed to cache file {}: {}", file.id, e);
                    self.update_cache_status(file.id, "not_cached", None).await?;
                }
            }
        }

        log::info!(
            "Pre-cached {} files, total size: {:.2} MB",
            cached_files,
            current_cache_size as f64 / (1024.0 * 1024.0)
        );

        Ok(())
    }

    /// Cache a specific file's content in Redis
    async fn cache_file_content(&self, file: &StoredFile) -> Result<u64> {
        use std::fs;
        use std::path::Path;

        let file_path = Path::new(&file.file_path);
        if !file_path.exists() {
            return Err(StorageError::NotFound { bucket: file.bucket.clone(), key: file.key.clone() });
        }

        // Read file content
        let content = fs::read(file_path)?;
        
        // Get Redis connection
        let mut conn = self.redis_client.get_connection()
            .map_err(|e| StorageError::Cache(format!("Failed to get Redis connection: {}", e)))?;

        // Create cache key
        let cache_key = format!("file:{}:{}:{}", file.bucket, file.key, file.id);
        
        // Set in Redis with TTL
        let cache_config = self.get_cache_config().await?;
        let ttl = cache_config.cache_ttl_seconds.unwrap_or(3600) as u64;
        
        let _: () = conn.set_ex(&cache_key, content, ttl)
            .map_err(|e| StorageError::Cache(format!("Failed to cache file: {}", e)))?;

        Ok(file.file_size as u64)
    }

    /// Get cached file content from Redis
    pub async fn get_cached_content(&self, file_id: Uuid, bucket: &str, key: &str) -> Result<Option<Vec<u8>>> {
        let mut conn = self.redis_client.get_connection()
            .map_err(|e| StorageError::Cache(format!("Failed to get Redis connection: {}", e)))?;

        let cache_key = format!("file:{}:{}:{}", bucket, key, file_id);
        
        let content: Option<Vec<u8>> = conn.get(&cache_key)
            .map_err(|e| StorageError::Cache(format!("Failed to get cached content: {}", e)))?;

        if content.is_some() {
            // Update cache hits
            self.update_cache_status(file_id, "cached", Some(1)).await?;
        }

        Ok(content)
    }

    /// Remove file from cache
    pub async fn remove_from_cache(&self, file_id: Uuid, bucket: &str, key: &str) -> Result<()> {
        let mut conn = self.redis_client.get_connection()
            .map_err(|e| StorageError::Cache(format!("Failed to get Redis connection: {}", e)))?;

        let cache_key = format!("file:{}:{}:{}", bucket, key, file_id);
        
        let _: () = conn.del(&cache_key)
            .map_err(|e| StorageError::Cache(format!("Failed to remove from cache: {}", e)))?;

        self.update_cache_status(file_id, "not_cached", None).await?;

        Ok(())
    }

    /// Clear entire cache
    pub async fn clear_cache(&self) -> Result<()> {
        let mut conn = self.redis_client.get_connection()
            .map_err(|e| StorageError::Cache(format!("Failed to get Redis connection: {}", e)))?;

        redis::cmd("FLUSHDB")
            .query::<()>(&mut conn)
            .map_err(|e| StorageError::Cache(format!("Failed to clear cache: {}", e)))?;

        // Reset all files' cache status
        sqlx::query!(
            "UPDATE files SET cache_status = 'not_cached', last_cache_update = NOW() WHERE cache_status = 'cached'"
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> Result<CacheStats> {
        let mut conn = self.redis_client.get_connection()
            .map_err(|e| StorageError::Cache(format!("Failed to get Redis connection: {}", e)))?;

        let db_size: i64 = redis::cmd("DBSIZE")
            .query(&mut conn)
            .map_err(|e| StorageError::Cache(format!("Failed to get cache size: {}", e)))?;

        let cache_config = self.get_cache_config().await?;
        
        Ok(CacheStats {
            total_keys: db_size as u64,
            max_size_gb: cache_config.max_cache_size_gb.unwrap_or(1.0),
            ttl_seconds: cache_config.cache_ttl_seconds.unwrap_or(3600) as u64,
            preload_enabled: cache_config.preload_enabled.unwrap_or(false),
        })
    }

    /// Start background pre-caching service
    pub async fn start_preload_service(&self) {
        let cache_manager = self.clone();
        
        tokio::spawn(async move {
            loop {
                if let Err(e) = cache_manager.preload_popular_files().await {
                    log::error!("Preload service error: {}", e);
                }
                
                // Run every 30 minutes
                sleep(Duration::from_secs(1800)).await;
            }
        });
    }
}

impl Clone for CacheManager {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            redis_client: self.redis_client.clone(),
            config: self.config.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_keys: u64,
    pub max_size_gb: f64,
    pub ttl_seconds: u64,
    pub preload_enabled: bool,
} 