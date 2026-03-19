use crate::{
    cache::CacheManager,
    config::Config,
    crypto::{CryptoManager, EncryptionAlgorithm},
    compression::CompressionManager,
    database::DatabaseManager,
    errors::{Result, StorageError},
    models::{StoredFile, FileInfo, StorageStats},
};

use tracing::{info, error, warn};
use sqlx::PgPool;
use std::sync::Arc;
use std::path::PathBuf;
use std::borrow::Cow;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use chrono;
use blake3;
use md5;


#[derive(Clone)]
pub struct StorageManager {
    config: Arc<Config>,
    base_path: Arc<PathBuf>,
    cache: Arc<CacheManager>,
    crypto: Arc<CryptoManager>,
    compression: Arc<CompressionManager>,
    db: Arc<DatabaseManager>,
    pool: PgPool,
}

impl StorageManager {
    pub async fn new(config: Config, pool: PgPool) -> Result<Self> {
        let config = Arc::new(config);
        let base_path = Arc::new(PathBuf::from(&config.storage.path));
        fs::create_dir_all(&*base_path).await?;

        let cache = Arc::new(CacheManager::new(Arc::clone(&config)).await?);
        let crypto = Arc::new(CryptoManager::new(Arc::new(config.crypto.clone()))?);
        let compression = Arc::new(CompressionManager::new(Arc::new(config.compression.clone())));
        let db = Arc::new(DatabaseManager::new(Arc::new(config.database.clone())).await?);

        Ok(Self {
            config,
            base_path,
            cache,
            crypto,
            compression,
            db,
            pool,
        })
    }

    pub async fn store_file(
        &self,
        bucket: &str,
        key: &str,
        content: Vec<u8>,
        content_type: Option<String>,
    ) -> Result<StoredFile> {
        let file_hash = blake3::hash(&content);
        let file_id = Uuid::new_v4();
        let upload_time = chrono::Utc::now();
        let hash_hex = file_hash.to_hex().to_string();
        let md5_hex = format!("{:x}", md5::compute(&content));

        // Ensure bucket exists first
        self.ensure_bucket_exists(bucket).await?;

        // Write file to disk FIRST to ensure it exists before database record
        let file_path = self.get_file_path(bucket, key);
        tokio::fs::create_dir_all(file_path.parent().unwrap()).await?;
        
        // Use a temporary file to ensure atomic write
        let temp_path = file_path.with_extension("tmp");
        let mut file_handle = tokio::fs::File::create(&temp_path).await?;
        file_handle.write_all(&content).await?;
        file_handle.sync_all().await?;
        drop(file_handle);

        // Atomically move temp file to final location
        tokio::fs::rename(&temp_path, &file_path).await?;
        info!("💾 File written to disk: {}", file_path.display());

        // Start database transaction
        let mut tx = self.pool.begin().await?;

        // Store the file in the database (within transaction)
        let file = sqlx::query_as!(
            StoredFile,
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
            RETURNING *
            "#,
            file_id,
            bucket,
            key,
            key.split('/').last().unwrap_or(&key).to_string(),  // filename - extract just the filename from the path
            file_path.to_string_lossy().to_string(),  // file_path
            content.len() as i64,  // file_size
            content.len() as i64,  // original_size
            content_type.unwrap_or_else(|| "application/octet-stream".to_string()),  // content_type
            &hash_hex,  // hash_blake3
            &md5_hex,  // hash_md5
            None::<serde_json::Value>,  // metadata
            false,  // is_compressed
            false,  // is_encrypted
            None::<String>,  // compression_algorithm
            None::<String>,  // encryption_algorithm
            None::<f32>,  // compression_ratio
            upload_time,  // upload_time
            None::<chrono::DateTime<chrono::Utc>>,  // last_accessed
            0i64,  // access_count
            None::<String>,  // encryption_key_id
            false,  // compression_enabled
            false,  // encryption_enabled
            None::<i32>,  // compression_level
            "not_cached",  // cache_status
            None::<chrono::DateTime<chrono::Utc>>,  // last_cache_update
            0i64,  // cache_hits
            0i32   // cache_priority
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            // If this is a duplicate key, another upload already won the DB insert.
            // In that case, deleting the on-disk file here can create DB/disk mismatch
            // (the existing DB row still points to `file_path`, but we removed it).
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.code() == Some(Cow::Borrowed("23505")) {
                    return StorageError::AlreadyExists {
                        bucket: bucket.to_string(),
                        key: key.to_string(),
                    };
                }
            }

            error!("❌ Database insert failed, cleaning up file: {}", e);
            // Clean up the file if database insert fails (only for non-duplicate errors)
            if let Err(cleanup_err) = std::fs::remove_file(&file_path) {
                warn!("Failed to cleanup file after DB error: {}", cleanup_err);
            }

            StorageError::Database(e.to_string())
        })?;

        // Commit transaction
        tx.commit().await.map_err(|e| {
            error!("❌ Transaction commit failed, cleaning up file: {}", e);
            // Clean up the file if transaction commit fails
            if let Err(cleanup_err) = std::fs::remove_file(&file_path) {
                warn!("Failed to cleanup file after transaction error: {}", cleanup_err);
            }
            StorageError::Database(e.to_string())
        })?;

        info!("✅ File stored successfully: {}/{} ({} bytes)", bucket, key, content.len());
        Ok(file)
    }

    /// Ensure bucket exists, creating it if necessary
    async fn ensure_bucket_exists(&self, bucket: &str) -> Result<()> {
        // Check if bucket exists
        let exists = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM buckets WHERE name = $1)",
            bucket
        )
        .fetch_one(&self.pool)
        .await?
        .unwrap_or(false);

        if !exists {
            info!("📁 Creating bucket: {}", bucket);
            self.create_bucket(bucket).await?;
        }

        Ok(())
    }

    pub async fn get_file(&self, bucket: &str, key: &str) -> Result<(Vec<u8>, Option<String>)> {
        // Get file metadata from database
        let file = sqlx::query_as!(
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
        })?;

        // Read file content
        let file_path = self.get_file_path(bucket, key);
        let content = match tokio::fs::read(&file_path).await {
            Ok(c) => c,
            Err(e) => {
                // Safety net: if DB has a row but disk file is missing, delete the broken row
                // and return NotFound so callers can self-heal.
                let msg = e.to_string();
                if msg.contains("No such file or directory") || msg.contains("os error 2") {
                    // Best-effort delete; if it fails we'll still return NotFound.
                    let _ = self.delete_file(bucket, key).await;
                    return Err(StorageError::NotFound {
                        bucket: bucket.to_string(),
                        key: key.to_string(),
                    });
                }
                return Err(StorageError::Io(msg));
            }
        };

        Ok((content, Some(file.content_type)))
    }

    /// Store file with per-file configuration for compression and encryption
    pub async fn store_file_with_config(
        &self,
        bucket: &str,
        key: &str,
        content: Vec<u8>,
        content_type: Option<String>,
        compress: Option<bool>,
        encrypt: Option<bool>,
        compression_algorithm: Option<&str>,
        compression_level: Option<i32>,
        encryption_key_id: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<StoredFile> {
        info!("📝 Starting file storage process with config for {}/{}", bucket, key);
        
        // Create bucket directory if it doesn't exist
        let bucket_path = (*self.base_path).join(bucket);
        fs::create_dir_all(&bucket_path).await.map_err(|e| {
            error!("❌ Failed to create bucket directory {}: {}", bucket_path.display(), e);
            StorageError::Io(e.to_string())
        })?;
        info!("📁 Ensured bucket directory exists: {}", bucket_path.display());

        // Use the key as the filename
        let id = Uuid::new_v4();
        let filename = key.split('/').last().unwrap_or(&key).to_string();
        let file_path = bucket_path.join(&key);
        info!("🔑 Generated file ID: {} -> {}", id, filename);

        // Process file content
        let mut processed_content = content.clone();
        let original_size = processed_content.len();
        info!("📊 Original content size: {} bytes", original_size);

        // Determine compression settings
        let should_compress = compress.unwrap_or(false);
        let compression_algo = compression_algorithm.unwrap_or("gzip");
        let compression_lvl = compression_level.unwrap_or(6);

        // Compress if requested and meets minimum size
        let is_compressed = if should_compress && original_size >= 100 { // 100 bytes minimum
            info!("🗜️ Attempting compression with {} level {}", compression_algo, compression_lvl);
            let compression_config = crate::config::CompressionConfig {
                enabled: true,
                algorithm: compression_algo.to_string(),
                level: compression_lvl,
                min_size: 100,
            };
            let compression_manager = CompressionManager::new(Arc::new(compression_config));
            
            match compression_manager.compress(&processed_content) {
                Ok(compressed) => {
                    processed_content = compressed;
                    info!("✅ Compressed size: {} bytes (ratio: {:.2})", processed_content.len(), processed_content.len() as f64 / original_size as f64);
                    true
                }
                Err(e) => {
                    warn!("⚠️ Compression failed, storing uncompressed: {}", e);
                    false
                }
            }
        } else {
            false
        };

        // Determine encryption settings
        let should_encrypt = encrypt.unwrap_or(false);
        let is_encrypted = if should_encrypt {
            info!("🔒 Encrypting content...");
            processed_content = self.crypto.encrypt(&processed_content, EncryptionAlgorithm::from_str(&self.config.crypto.algorithm)?).await.map_err(|e| {
                error!("❌ Encryption failed: {}", e);
                e
            })?;
            info!("✅ Content encrypted");
            true
        } else {
            false
        };

        // Calculate hashes
        let hash_blake3 = blake3::hash(&content).to_hex().to_string();
        let hash_md5 = format!("{:x}", md5::compute(&content));
        info!("🔍 Content hashes - BLAKE3: {}... MD5: {}...", &hash_blake3[..8], &hash_md5[..8]);

        // Check for duplicates if deduplication is enabled
        if self.config.storage.enable_deduplication {
            info!("🔍 Checking for duplicates...");
            if let Some(existing_file) = self.db.get_file_by_hash(&hash_blake3).await? {
                info!("♻️ Found duplicate file, reusing: {}/{}", existing_file.bucket, existing_file.key);
                return Ok(existing_file);
            }
        }

        // Write file to disk
        fs::write(&file_path, &processed_content).await.map_err(|e| {
            error!("❌ Failed to write file to disk at {}: {}", file_path.display(), e);
            StorageError::Io(e.to_string())
        })?;
        info!("💾 File written to disk: {}", file_path.display());

        // Create database record
        let file = StoredFile {
            id,
            bucket: bucket.to_string(),
            key: key.to_string(),
            filename,
            file_path: file_path.to_string_lossy().to_string(),
            file_size: processed_content.len() as i64,
            original_size: original_size as i64,
            content_type: content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
            hash_blake3,
            hash_md5,
            metadata,
            is_compressed: Some(is_compressed),
            is_encrypted: Some(is_encrypted),
            compression_algorithm: if is_compressed { Some(compression_algo.to_string()) } else { None },
            encryption_algorithm: if is_encrypted { Some(self.config.crypto.algorithm.clone()) } else { None },
            compression_ratio: Some(processed_content.len() as f32 / content.len() as f32),
            upload_time: Some(chrono::Utc::now()),
            last_accessed: None,
            access_count: 0,
            encryption_key_id: encryption_key_id.map(|s| s.to_string()),
            compression_enabled: Some(self.config.compression.enabled),
            encryption_enabled: Some(self.config.crypto.enabled),
            compression_level: Some(compression_lvl),
            cache_status: Some("not_cached".to_string()),
            last_cache_update: None,
            cache_hits: Some(0),
            cache_priority: Some(0),
        };

        // Save to database
        self.db.save_file(&file).await?;
        info!("✅ File metadata saved to database");

        Ok(file)
    }

    pub async fn delete_file(&self, bucket: &str, key: &str) -> Result<()> {
        // Start transaction
        let mut tx = self.pool.begin().await?;

        // Get file info before deletion
        let _file = sqlx::query!(
            "SELECT file_path FROM files WHERE bucket = $1 AND key = $2",
            bucket,
            key
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| StorageError::NotFound {
            bucket: bucket.to_string(),
            key: key.to_string(),
        })?;

        // Delete from database first (within transaction)
        sqlx::query!(
            "DELETE FROM files WHERE bucket = $1 AND key = $2",
            bucket,
            key
        )
        .execute(&mut *tx)
        .await?;

        // Commit database transaction
        tx.commit().await?;

        // Delete file from disk after successful database deletion
        let file_path = self.get_file_path(bucket, key);
        if let Err(e) = tokio::fs::remove_file(&file_path).await {
            warn!("⚠️ Failed to delete file from disk (file may not exist): {}", e);
            // Don't fail the operation if file doesn't exist on disk
        }

        info!("🗑️ File deleted: {}/{}", bucket, key);
        Ok(())
    }

    pub async fn list_files(&self, bucket: &str, prefix: Option<&str>, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<FileInfo>> {
        let files = self.db.list_files(bucket, prefix, limit.unwrap_or(100), offset.unwrap_or(0)).await?;
        Ok(files.into_iter().map(FileInfo::from).collect())
    }

    pub async fn get_file_info(&self, bucket: &str, key: &str) -> Result<FileInfo> {
        let file = self.db.get_file(bucket, key).await?;
        Ok(FileInfo::from(file))
    }

    pub async fn get_storage_stats(&self) -> Result<StorageStats> {
        self.db.get_total_stats().await
    }

    pub async fn list_buckets(&self) -> Result<Vec<String>> {
        self.db.list_buckets().await
    }

    pub async fn delete_bucket(&self, bucket: &str) -> Result<()> {
        info!("🗑️ Starting bucket deletion: {}", bucket);
        
        // Get all files in bucket before deletion (for disk cleanup)
        let files = sqlx::query!(
            "SELECT key, file_path FROM files WHERE bucket = $1",
            bucket
        )
        .fetch_all(&self.pool)
        .await?;

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

        // Commit transaction (files are deleted automatically via CASCADE)
        tx.commit().await?;
        info!("✅ Database cleanup completed for bucket {} (CASCADE deleted {} file records)", bucket, files.len());

        // Clean up files from disk (after successful database deletion)
        let mut cleaned_files = 0;
        for file in files {
            let file_path = self.get_file_path(bucket, &file.key);
            if let Err(e) = tokio::fs::remove_file(&file_path).await {
                warn!("⚠️ Failed to delete file from disk: {} - {}", file_path.display(), e);
            } else {
                cleaned_files += 1;
            }
        }

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

    pub async fn get_bucket_stats(&self, bucket: &str) -> Result<StorageStats> {
        let (file_count, total_size) = self.db.get_bucket_stats(bucket).await?;
        
        // Get compression and encryption stats for this bucket
        let files = self.db.list_files(bucket, None, 1000, 0).await?;
        let compressed_files = files.iter().filter(|f| f.is_compressed.unwrap_or(false)).count() as i64;
        let encrypted_files = files.iter().filter(|f| f.is_encrypted.unwrap_or(false)).count() as i64;
        
        let avg_compression = if compressed_files > 0 {
            let total_ratio: f32 = files.iter()
                .filter_map(|f| f.compression_ratio)
                .sum();
            Some(total_ratio / compressed_files as f32)
        } else {
            None
        };

        Ok(StorageStats {
            total_files: file_count,
            total_size,
            compressed_files,
            encrypted_files,
            compression_ratio: avg_compression,
            last_updated: chrono::Utc::now(),
        })
    }

    pub async fn search_files(&self, bucket: Option<&str>, query: &str, limit: Option<i64>) -> Result<Vec<FileInfo>> {
        let files = if let Some(bucket) = bucket {
            self.db.search_files(bucket, query, limit.unwrap_or(100)).await?
        } else {
            // Search across all buckets by using empty string bucket
            self.db.search_files("", query, limit.unwrap_or(100)).await?
        };
        Ok(files.into_iter().map(FileInfo::from).collect())
    }

    pub async fn store_file_with_crypto(&self, file: &StoredFile, content: &[u8]) -> Result<()> {
        let mut processed_content = content.to_vec();

        // Apply compression if enabled
        if file.compression_enabled.unwrap_or(false) {
            processed_content = self.compression.compress(&processed_content)?;
        }

        // Apply encryption if enabled
        if file.is_encrypted.unwrap_or(false) {
            let algorithm = EncryptionAlgorithm::from_str(
                file.encryption_algorithm.as_deref().unwrap_or("aes-gcm")
            )?;
            
            processed_content = self.crypto.encrypt(&processed_content, algorithm).await?;
        }

        // Create directory if it doesn't exist
        let file_path = self.get_file_path(&file.bucket, &file.key);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write file
        let mut file_handle = fs::File::create(&file_path).await?;
        file_handle.write_all(&processed_content).await?;
        file_handle.sync_all().await?;

        Ok(())
    }

    pub async fn read_file(&self, file: &StoredFile) -> Result<Vec<u8>> {
        let file_path = self.get_file_path(&file.bucket, &file.key);
        let mut processed_content = fs::read(&file_path).await?;

        // Decrypt if encrypted
        if file.is_encrypted.unwrap_or(false) {
            let algorithm = EncryptionAlgorithm::from_str(
                file.encryption_algorithm.as_deref().unwrap_or("aes-gcm")
            )?;
            
            processed_content = self.crypto.decrypt(&processed_content, algorithm).await?;
        }

        // Decompress if compressed
        if file.is_compressed.unwrap_or(false) {
            processed_content = self.compression.decompress(&processed_content)?;
        }

        Ok(processed_content)
    }

    fn get_file_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.base_path.join(bucket).join(key)
    }

    pub async fn upload_file(&self, file: &mut StoredFile, content: &[u8]) -> Result<()> {
        let mut processed_content = content.to_vec();

        // Apply compression if enabled
        if file.compression_enabled.unwrap_or(false) {
            processed_content = self.compression.compress(&processed_content)?;
            file.is_compressed = Some(true);
            file.compression_ratio = Some(processed_content.len() as f32 / content.len() as f32);
        }

        // Apply encryption if enabled
        if file.encryption_enabled.unwrap_or(false) {
            let algorithm = EncryptionAlgorithm::from_str(
                file.encryption_algorithm.as_deref().unwrap_or("aes-gcm")
            )?;
            
            processed_content = self.crypto.encrypt(&processed_content, algorithm).await?;
            file.is_encrypted = Some(true);
        }

        // Store file
        self.store_file(&file.bucket, &file.key, processed_content.clone(), Some(file.content_type.clone())).await?;

        // Update database
        sqlx::query!(
            r#"
            UPDATE files
            SET 
                file_size = $1,
                original_size = $2,
                hash_blake3 = $3,
                is_compressed = $4,
                is_encrypted = $5,
                compression_algorithm = $6,
                encryption_algorithm = $7,
                compression_ratio = $8,
                compression_level = $9,
                cache_status = $10,
                cache_hits = $11,
                cache_priority = $12,
                last_cache_update = $13,
                last_accessed = $14,
                access_count = $15
            WHERE id = $16
            "#,
            processed_content.len() as i64,
            file.original_size,
            file.hash_blake3,
            file.is_compressed,
            file.is_encrypted,
            file.compression_algorithm,
            file.encryption_algorithm,
            file.compression_ratio,
            file.compression_level,
            file.cache_status,
            file.cache_hits,
            file.cache_priority,
            file.last_cache_update,
            file.last_accessed,
            file.access_count,
            file.id,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn create_bucket(&self, bucket: &str) -> Result<()> {
        // Create bucket directory
        let bucket_path = self.base_path.join(bucket);
        tokio::fs::create_dir_all(&bucket_path).await?;
        
        // Create bucket record in database
        let _ = sqlx::query!(
            "INSERT INTO buckets (name, is_active) VALUES ($1, true) ON CONFLICT (name) DO NOTHING",
            bucket
        )
        .execute(&self.pool)
        .await?;
        
        info!("✅ Created bucket: {}", bucket);
        Ok(())
    }

    /// Get the maximum file size from configuration
    pub fn max_file_size(&self) -> usize {
        self.config.storage.max_file_size as usize
    }
} 