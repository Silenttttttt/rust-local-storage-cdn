use crate::{
    config::Config,
    errors::{Result, StorageError},
};
use crate::models::StoredFile;
use redis::AsyncCommands;
use serde_json;
use std::sync::Arc;
use tracing::{debug, error, warn};
use serde::{Serialize, de::DeserializeOwned};

pub struct CacheManager {
    config: Arc<Config>,
    redis: Option<redis::Client>,
}

impl CacheManager {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        let redis = if config.redis.enabled {
            match redis::Client::open(config.redis_url()) {
                Ok(client) => {
                    tracing::info!("✅ Redis cache enabled");
                    Some(client)
                }
                Err(e) => {
                    tracing::warn!("⚠️ Redis connection failed, running without cache: {}", e);
                    None
                }
            }
        } else {
            tracing::info!("📋 Redis cache disabled (ENABLE_REDIS=false)");
            None
        };
        Ok(Self { config, redis })
    }

    pub async fn cache_file(&self, file: &StoredFile, content: &[u8]) -> Result<()> {
        let Some(ref redis) = self.redis else { return Ok(()); };
        let mut conn = redis.get_async_connection().await?;
        let key = format!("file:{}:{}", file.bucket, file.key);
        let _: () = conn.set_ex(key, content.to_vec(), 3600).await?;
        Ok(())
    }

    pub async fn get_cached_file(&self, file: &StoredFile) -> Result<Vec<u8>> {
        let Some(ref redis) = self.redis else {
            return Err(StorageError::NotFound { bucket: file.bucket.clone(), key: file.key.clone() });
        };
        let mut conn = redis.get_async_connection().await?;
        let key = format!("file:{}:{}", file.bucket, file.key);
        let content: Option<Vec<u8>> = conn.get(&key).await?;
        content.ok_or_else(|| StorageError::NotFound {
            bucket: file.bucket.clone(),
            key: file.key.clone(),
        })
    }

    pub async fn preload_popular_files(&self) -> Result<()> {
        if self.redis.is_none() { return Ok(()); }
        // In a real implementation, this would load files from storage into cache
        Ok(())
    }

    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let Some(ref redis) = self.redis else { return Ok(None); };
        let mut conn = redis.get_async_connection().await?;
        
        let data: Option<String> = conn.get(key).await?;
        
        match data {
            Some(json_str) => {
                let value = serde_json::from_str(&json_str)?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let Some(ref redis) = self.redis else { return Ok(()); };
        let mut conn = redis.get_async_connection().await?;
        
        let serialized = serde_json::to_string(value)?;
        let _: () = conn.set_ex(key, serialized, 3600).await?;
        
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let Some(ref redis) = self.redis else { return Ok(()); };
        let mut conn = redis.get_async_connection().await?;
        let _: () = conn.del(key).await?;
        Ok(())
    }

    pub async fn health_check(&self) -> Result<bool> {
        let Some(ref redis) = self.redis else { return Ok(false); };
        let mut conn = redis.get_async_connection().await?;
        let _: String = conn.get("test").await.unwrap_or_default();
        Ok(true)
    }

    pub async fn get_file_metadata(&self, key: &str, bucket: &str) -> Result<Option<StoredFile>> {
        let Some(ref redis) = self.redis else { return Ok(None); };
        let cache_key = format!("file:{}:{}", bucket, key);
        
        let mut conn = redis.get_async_connection().await?;
        match conn.get::<_, Option<String>>(&cache_key).await {
            Ok(Some(cached_data)) => {
                debug!("📋 Cache hit for file metadata: {}", cache_key);
                match serde_json::from_str::<StoredFile>(&cached_data) {
                    Ok(file) => Ok(Some(file)),
                    Err(e) => {
                        warn!("❌ Failed to deserialize cached file metadata: {}", e);
                        // Remove corrupted cache entry
                        let _: () = conn.del(&cache_key).await.unwrap_or_default();
                        Ok(None)
                    }
                }
            }
            Ok(None) => {
                debug!("📋 Cache miss for file metadata: {}", cache_key);
                Ok(None)
            }
            Err(e) => {
                error!("❌ Redis get error: {}", e);
                Err(StorageError::Redis(e.to_string()))
            }
        }
    }

    pub async fn set_file_metadata(&self, key: &str, bucket: &str, file: &StoredFile) -> Result<()> {
        let Some(ref redis) = self.redis else { return Ok(()); };
        let cache_key = format!("file:{}:{}", bucket, key);
        
        let mut conn = redis.get_async_connection().await?;
        let serialized = serde_json::to_string(file)?;
        let _: () = conn.set_ex(&cache_key, serialized, 3600).await?;
        debug!("💾 Cached file metadata: {}", cache_key);
        Ok(())
    }

    pub async fn get_file_content(&self, key: &str, bucket: &str) -> Result<Option<Vec<u8>>> {
        let Some(ref redis) = self.redis else { return Ok(None); };
        let cache_key = format!("content:{}:{}", bucket, key);
        
        let mut conn = redis.get_async_connection().await?;
        match conn.get::<_, Option<Vec<u8>>>(&cache_key).await {
            Ok(Some(cached_data)) => {
                debug!("🗂️ Cache hit for file content: {}", cache_key);
                Ok(Some(cached_data))
            }
            Ok(None) => {
                debug!("🗂️ Cache miss for file content: {}", cache_key);
                Ok(None)
            }
            Err(e) => {
                error!("❌ Redis get error: {}", e);
                Err(StorageError::Redis(e.to_string()))
            }
        }
    }

    pub async fn set_file_content(&self, key: &str, bucket: &str, content: &[u8]) -> Result<()> {
        // Only cache small files to avoid memory issues
        const MAX_CACHE_SIZE: usize = 1024 * 1024; // 1MB
        
        if content.len() > MAX_CACHE_SIZE {
            debug!("📁 File too large to cache: {} bytes", content.len());
            return Ok(());
        }
        
        let Some(ref redis) = self.redis else { return Ok(()); };
        let cache_key = format!("content:{}:{}", bucket, key);
        let mut conn = redis.get_async_connection().await?;
        let _: () = conn.set_ex(&cache_key, content, 3600).await?;
        debug!("💾 Cached file content: {} ({} bytes)", cache_key, content.len());
        Ok(())
    }

    pub async fn invalidate_file(&self, key: &str, bucket: &str) -> Result<()> {
        let Some(ref redis) = self.redis else { return Ok(()); };
        let metadata_key = format!("file:{}:{}", bucket, key);
        let content_key = format!("content:{}:{}", bucket, key);
        
        let mut conn = redis.get_async_connection().await?;
        let _: () = conn.del(&[&metadata_key, &content_key]).await?;
        debug!("🗑️ Invalidated cache for: {}", key);
        Ok(())
    }

    pub async fn get_bucket_stats(&self, bucket: &str) -> Result<Option<(u64, u64)>> {
        let Some(ref redis) = self.redis else { return Ok(None); };
        let cache_key = format!("bucket_stats:{}", bucket);
        
        let mut conn = redis.get_async_connection().await?;
        match conn.get::<_, Option<String>>(&cache_key).await {
            Ok(Some(cached_data)) => {
                match serde_json::from_str::<(u64, u64)>(&cached_data) {
                    Ok(stats) => {
                        debug!("📊 Cache hit for bucket stats: {}", bucket);
                        Ok(Some(stats))
                    }
                    Err(_) => Ok(None)
                }
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::Redis(e.to_string())),
        }
    }

    pub async fn set_bucket_stats(&self, bucket: &str, file_count: u64, total_size: u64) -> Result<()> {
        let Some(ref redis) = self.redis else { return Ok(()); };
        let cache_key = format!("bucket_stats:{}", bucket);
        let mut conn = redis.get_async_connection().await?;
        
        let serialized = serde_json::to_string(&(file_count, total_size))?;
        let _: () = conn.set_ex(&cache_key, serialized, 3600).await?; // 1 hour TTL for stats
        Ok(())
    }

    pub async fn increment_download_count(&self, key: &str, bucket: &str) -> Result<u64> {
        let Some(ref redis) = self.redis else { return Ok(1); };
        let cache_key = format!("downloads:{}:{}", bucket, key);
        let mut conn = redis.get_async_connection().await?;
        let count: u64 = conn.incr(&cache_key, 1).await?;
        Ok(count)
    }

    pub async fn get_popular_files(&self, bucket: &str, limit: usize) -> Result<Vec<String>> {
        let Some(ref redis) = self.redis else { return Ok(vec![]); };
        let pattern = format!("downloads:{}:*", bucket);
        let mut conn = redis.get_async_connection().await?;
        
        let mut keys: Vec<String> = conn.keys(&pattern).await?;
        let mut files = Vec::with_capacity(keys.len());
        
        for key in keys.drain(..) {
            let count: u64 = conn.get(&key).await?;
            let file_key = key.strip_prefix(&format!("downloads:{}:", bucket))
                .unwrap_or_default()
                .to_string();
            files.push((file_key, count));
        }
        
        files.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(files.into_iter().take(limit).map(|(k, _)| k).collect())
    }

    pub async fn invalidate_cache(&self, file: &StoredFile) -> Result<()> {
        let Some(ref redis) = self.redis else { return Ok(()); };
        let mut conn = redis.get_async_connection().await?;
        let key = format!("file:{}:{}", file.bucket, file.key);
        let _: () = conn.del(&key).await?;
        Ok(())
    }
} 