use crate::{
    config::Config,
    errors::{Result, StorageError},
    models::{StoredFile, FileInfo},
    performance_optimizations::*,
};
use std::{path::PathBuf, sync::Arc, borrow::Cow};
use sqlx::PgPool;
use uuid::Uuid;
use tracing::{info, error, warn};

/// Optimized storage manager that removes RwLock bottleneck
pub struct OptimizedStorageManager {
    config: Arc<Config>,
    base_path: Arc<PathBuf>,
    pool: PgPool,
    memory_pool: Arc<MemoryPool>,
}

impl OptimizedStorageManager {
    pub async fn new(config: Config, pool: PgPool) -> Result<Self> {
        let base_path = Arc::new(PathBuf::from(&config.storage.path));
        let memory_pool = Arc::new(MemoryPool::new(10, 1024 * 1024)); // 10 buffers of 1MB each
        
        tokio::fs::create_dir_all(&*base_path).await?;
        
        Ok(Self {
            config: Arc::new(config),
            base_path,
            pool,
            memory_pool,
        })
    }

    /// Optimized file storage with parallel operations and no unnecessary locks
    pub async fn store_file_optimized(
        &self,
        bucket: &str,
        key: &str,
        content: &[u8], // Take slice instead of owned Vec to avoid clone
        content_type: Option<String>,
    ) -> Result<StoredFile> {
        let file_id = Uuid::new_v4();
        let upload_time = chrono::Utc::now();
        
        // Ensure bucket exists first
        self.ensure_bucket_exists_optimized(bucket).await?;

        // Compute hashes in parallel while preparing other data
        let hashes_future = compute_hashes_parallel(content);
        let file_path = self.get_file_path(bucket, key);
        
        // Create directory if needed
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Wait for parallel hash computation
        let (hash_blake3, hash_md5) = hashes_future.await;

        // Write file to disk with optimized atomic write
        write_file_atomic_optimized(&file_path, content).await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        info!("💾 File written to disk: {}", file_path.display());

        // Prepare file record
        let file = StoredFile {
            id: file_id,
            bucket: bucket.to_string(),
            key: key.to_string(),
            filename: key.to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            file_size: content.len() as i64,
            original_size: content.len() as i64,
            content_type: content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
            hash_blake3,
            hash_md5,
            metadata: None,
            is_compressed: Some(false),
            is_encrypted: Some(false),
            compression_algorithm: None,
            encryption_algorithm: None,
            compression_ratio: None,
            upload_time: Some(upload_time),
            last_accessed: None,
            access_count: 0,
            encryption_key_id: None,
            compression_enabled: Some(false),
            encryption_enabled: Some(false),
            compression_level: None,
            cache_status: Some("not_cached".to_string()),
            last_cache_update: None,
            cache_hits: Some(0),
            cache_priority: Some(0),
        };

        // Insert into database with error handling and cleanup
        let result = self.insert_file_record(&file).await;
        
        match result {
            Ok(_) => {
                info!("✅ File stored successfully: {}/{} ({} bytes)", bucket, key, content.len());
                Ok(file)
            }
            Err(e) => {
                error!("❌ Database insert failed, cleaning up file: {}", e);
                // Clean up the file if database insert fails
                if let Err(cleanup_err) = tokio::fs::remove_file(&file_path).await {
                    warn!("Failed to cleanup file after DB error: {}", cleanup_err);
                }
                Err(e)
            }
        }
    }

    /// Optimized file retrieval with streaming
    pub async fn get_file_optimized(&self, bucket: &str, key: &str) -> Result<(Vec<u8>, Option<String>)> {
        // Get file metadata and content in parallel
        let metadata_future = self.get_file_metadata(bucket, key);
        let file_path = self.get_file_path(bucket, key);
        let content_future = tokio::fs::read(&file_path);
        
        let (metadata_result, content_result) = tokio::join!(metadata_future, content_future);
        
        let file = metadata_result?;
        let content = content_result.map_err(|e| StorageError::Io(e.to_string()))?;
        
        // Update access count asynchronously (fire and forget)
        let pool = self.pool.clone();
        let file_id = file.id;
        tokio::spawn(async move {
            let _ = sqlx::query!(
                "UPDATE files SET last_accessed = NOW(), access_count = access_count + 1 WHERE id = $1",
                file_id
            )
            .execute(&pool)
            .await;
        });

        Ok((content, Some(file.content_type)))
    }

    /// Optimized bucket existence check with prepared statement
    async fn ensure_bucket_exists_optimized(&self, bucket: &str) -> Result<()> {
        // Use a more efficient query with UPSERT
        sqlx::query!(
            r#"
            INSERT INTO buckets (name, created_at) 
            VALUES ($1, NOW())
            ON CONFLICT (name) DO NOTHING
            "#,
            bucket
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get file metadata without reading file content
    async fn get_file_metadata(&self, bucket: &str, key: &str) -> Result<StoredFile> {
        sqlx::query_as!(
            StoredFile,
            "SELECT * FROM files WHERE bucket = $1 AND key = $2",
            bucket,
            key
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound {
            bucket: bucket.to_string(),
            key: key.to_string(),
        })
    }

    /// Optimized database insert with better error handling
    async fn insert_file_record(&self, file: &StoredFile) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO files (
                id, bucket, key, filename, file_path, file_size, original_size, content_type,
                hash_blake3, hash_md5, metadata, is_compressed, is_encrypted,
                compression_algorithm, encryption_algorithm, compression_ratio,
                upload_time, last_accessed, access_count, encryption_key_id,
                compression_enabled, encryption_enabled, compression_level,
                cache_status, last_cache_update, cache_hits, cache_priority
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27
            )
            "#,
            file.id,
            file.bucket,
            file.key,
            file.filename,
            file.file_path,
            file.file_size,
            file.original_size,
            file.content_type,
            file.hash_blake3,
            file.hash_md5,
            file.metadata,
            file.is_compressed,
            file.is_encrypted,
            file.compression_algorithm,
            file.encryption_algorithm,
            file.compression_ratio,
            file.upload_time,
            file.last_accessed,
            file.access_count,
            file.encryption_key_id,
            file.compression_enabled,
            file.encryption_enabled,
            file.compression_level,
            file.cache_status,
            file.last_cache_update,
            file.cache_hits,
            file.cache_priority
        )
        .execute(&self.pool)
        .await.map_err(|e| {
            // Check if this is a duplicate key constraint violation
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.code() == Some(Cow::Borrowed("23505")) {
                    // PostgreSQL unique constraint violation
                    return StorageError::AlreadyExists {
                        bucket: file.bucket.clone(),
                        key: file.key.clone(),
                    };
                }
            }
            
            StorageError::Database(e.to_string())
        })?;

        Ok(())
    }

    /// Optimized file listing with pagination
    pub async fn list_files_optimized(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<FileInfo>> {
        let limit = limit.unwrap_or(100).min(1000); // Cap at 1000
        let offset = offset.unwrap_or(0);

        let files = if let Some(prefix) = prefix {
            sqlx::query_as!(
                StoredFile,
                "SELECT * FROM files WHERE bucket = $1 AND key LIKE $2 ORDER BY upload_time DESC LIMIT $3 OFFSET $4",
                bucket,
                format!("{}%", prefix),
                limit,
                offset
            )
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as!(
                StoredFile,
                "SELECT * FROM files WHERE bucket = $1 ORDER BY upload_time DESC LIMIT $2 OFFSET $3",
                bucket,
                limit,
                offset
            )
            .fetch_all(&self.pool)
            .await?
        };

        Ok(files.into_iter().map(FileInfo::from).collect())
    }

    /// Optimized bucket deletion with batch operations
    pub async fn delete_bucket_optimized(&self, bucket: &str) -> Result<()> {
        info!("🗑️ Starting optimized bucket deletion: {}", bucket);

        // Get file paths for cleanup in parallel with database deletion
        let files_future = sqlx::query!(
            "SELECT key, file_path FROM files WHERE bucket = $1",
            bucket
        )
        .fetch_all(&self.pool);

        let files = files_future.await?;
        info!("📊 Found {} files to delete in bucket {}", files.len(), bucket);

        // Start transaction for bucket deletion
        let mut tx = self.pool.begin().await?;

        // Delete bucket (CASCADE will automatically delete all files)
        let deleted_count = sqlx::query!(
            "DELETE FROM buckets WHERE name = $1",
            bucket
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if deleted_count == 0 {
            warn!("⚠️ No bucket was deleted - bucket '{}' may not exist", bucket);
            tx.rollback().await?;
            return Err(StorageError::InvalidBucket(bucket.to_string()));
        }

        // Commit transaction
        tx.commit().await?;
        info!("✅ Database cleanup completed for bucket {} (CASCADE deleted {} file records)", bucket, files.len());

        // Clean up files from disk in parallel
        let cleanup_tasks: Vec<_> = files
            .into_iter()
            .map(|file| {
                let file_path = self.get_file_path(bucket, &file.key);
                tokio::spawn(async move {
                    if let Err(e) = tokio::fs::remove_file(&file_path).await {
                        warn!("⚠️ Failed to delete file from disk: {} - {}", file_path.display(), e);
                        false
                    } else {
                        true
                    }
                })
            })
            .collect();

        // Wait for all cleanup tasks
        let results = futures::future::join_all(cleanup_tasks).await;
        let cleaned_files = results.into_iter().filter_map(|r| r.ok()).filter(|&success| success).count();

        // Delete bucket directory
        let bucket_path = self.base_path.join(bucket);
        if let Err(e) = tokio::fs::remove_dir_all(&bucket_path).await {
            warn!("⚠️ Failed to delete bucket directory: {} - {}", bucket_path.display(), e);
        } else {
            info!("📁 Bucket directory deleted: {}", bucket_path.display());
        }

        info!("✅ Bucket deletion completed: {} ({} files cleaned from disk)", bucket, cleaned_files);
        Ok(())
    }

    fn get_file_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.base_path.join(bucket).join(key)
    }
} 