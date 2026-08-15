use crate::{
    cache::CacheManager,
    config::Config,
    crypto::{CryptoManager, EncryptionAlgorithm},
    compression::CompressionManager,
    database::DatabaseManager,
    encryption_keys::EncryptionKeyManager,
    errors::{Result, StorageError},
    models::{StoredFile, FileInfo, StorageStats},
    performance_optimizations::{compute_hashes_parallel, write_file_atomic_optimized},
};

use tracing::{info, error, warn};
use sqlx::PgPool;
use std::sync::Arc;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use uuid::Uuid;
use chrono;


/// Per-upload overrides. Any field left `None` falls back to the service's global config
/// default, so existing callers that don't set these keep behaving exactly as before.
#[derive(Debug, Clone, Default)]
pub struct StoreOptions {
    pub compress: Option<bool>,
    pub encrypt: Option<bool>,
    /// Optional TTL, as an already-resolved absolute timestamp (not a
    /// duration) - callers compute `Utc::now() + requested_ttl` themselves,
    /// so this struct never needs its own notion of "now". `None` means
    /// "never expires" - the default, and the only behavior any upload had
    /// before this field existed.
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub compression_algorithm: Option<String>,
    pub compression_level: Option<i32>,
    pub encryption_key_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Clone)]
pub struct StorageManager {
    config: Arc<Config>,
    base_path: Arc<PathBuf>,
    cache: Arc<CacheManager>,
    crypto: Arc<CryptoManager>,
    compression: Arc<CompressionManager>,
    db: Arc<DatabaseManager>,
    encryption_keys: Arc<EncryptionKeyManager>,
    pool: PgPool,
}

impl StorageManager {
    pub async fn new(config: Config, pool: PgPool) -> Result<Self> {
        let config = Arc::new(config);
        let base_path = Arc::new(PathBuf::from(&config.storage.path));
        fs::create_dir_all(&*base_path).await?;

        let cache = Arc::new(CacheManager::new(Arc::clone(&config), pool.clone()).await?);
        let crypto = Arc::new(CryptoManager::new(Arc::new(config.crypto.clone()))?);
        let compression = Arc::new(CompressionManager::new(Arc::new(config.compression.clone())));
        let db = Arc::new(DatabaseManager::new(pool.clone(), Arc::new(config.database.clone())).await?);
        let encryption_keys = Arc::new(EncryptionKeyManager::new(pool.clone(), Arc::clone(&config)));

        // Gated internally on cache_config.preload_enabled - benefits both cache backends.
        Arc::clone(&cache).start_preload_service();

        Ok(Self {
            config,
            base_path,
            cache,
            crypto,
            compression,
            db,
            encryption_keys,
            pool,
        })
    }

    /// Store a file, applying compression/encryption/deduplication per `opts` (falling back to
    /// global config defaults for anything unset). This is the one real upload path - it used to
    /// be split across `store_file` (no compression/encryption/dedup, always used) and
    /// `store_file_with_config` (the version that actually respected config, never called by any
    /// handler), which meant the advertised compression/encryption/dedup features silently never
    /// ran on a real upload.
    pub async fn store_file(
        &self,
        bucket: &str,
        key: &str,
        content: Vec<u8>,
        content_type: Option<String>,
        opts: StoreOptions,
    ) -> Result<StoredFile> {
        self.ensure_bucket_exists(bucket).await?;

        let should_compress = opts.compress.unwrap_or(self.config.compression.enabled);
        let should_encrypt = opts.encrypt.unwrap_or(self.config.crypto.enabled);
        let compression_algo = opts.compression_algorithm.unwrap_or_else(|| self.config.compression.algorithm.clone());
        let compression_lvl = opts.compression_level.unwrap_or(self.config.compression.level);
        let encryption_key_id = opts.encryption_key_id;

        // Hash the original (pre-compression/encryption) content - this is what deduplication
        // and integrity checks key off, in parallel since both hashes are independently CPU-bound.
        let (hash_blake3, hash_md5) = compute_hashes_parallel(&content).await;

        if self.config.storage.enable_deduplication {
            if let Some(existing_file) = self.db.get_file_by_hash(&hash_blake3).await? {
                // A true re-upload of the exact same (bucket, key) is a no-op - hand back the
                // existing record as-is.
                if existing_file.bucket == bucket && existing_file.key == key {
                    info!("♻️ Identical re-upload, reusing: {}/{}", existing_file.bucket, existing_file.key);
                    return Ok(existing_file);
                }

                // Otherwise this is a *new* (bucket, key) whose content happens to already be on
                // disk (this is exactly the path CopyObject and "upload the same file under a
                // different name" both take). It must still become its own independently
                // listable/gettable/deletable record - returning the *existing* file's own
                // bucket/key here (as this used to) means the caller's requested key is silently
                // never created: the response reports success but a subsequent GET on the
                // requested key 404s. Create a new row that points at the same on-disk bytes
                // instead of writing a second physical copy.
                if existing_file.is_compressed.unwrap_or(false) != should_compress
                    || existing_file.is_encrypted.unwrap_or(false) != should_encrypt
                {
                    // The physically-stored bytes were processed with whatever settings applied
                    // at first upload; a dedup hit always reuses those bytes as-is rather than
                    // re-processing, so make that explicit when the caller asked for something
                    // different this time.
                    warn!(
                        "♻️ Dedup reusing existing bytes for {}/{} whose stored compression/encryption \
                        settings differ from what this upload requested; the requested settings are ignored",
                        bucket, key
                    );
                }

                info!("♻️ Found duplicate content, aliasing {}/{} -> existing bytes at {}", bucket, key, existing_file.file_path);
                let aliased = StoredFile {
                    id: Uuid::new_v4(),
                    bucket: bucket.to_string(),
                    key: key.to_string(),
                    filename: key.split('/').last().unwrap_or(key).to_string(),
                    file_path: existing_file.file_path.clone(),
                    file_size: existing_file.file_size,
                    original_size: existing_file.original_size,
                    content_type: content_type.unwrap_or(existing_file.content_type),
                    hash_blake3: hash_blake3.clone(),
                    hash_md5: hash_md5.clone(),
                    metadata: opts.metadata,
                    is_compressed: existing_file.is_compressed,
                    is_encrypted: existing_file.is_encrypted,
                    compression_algorithm: existing_file.compression_algorithm.clone(),
                    encryption_algorithm: existing_file.encryption_algorithm.clone(),
                    compression_ratio: existing_file.compression_ratio,
                    upload_time: Some(chrono::Utc::now()),
                    last_accessed: None,
                    access_count: 0,
                    encryption_key_id: existing_file.encryption_key_id.clone(),
                    compression_enabled: existing_file.compression_enabled,
                    encryption_enabled: existing_file.encryption_enabled,
                    compression_level: existing_file.compression_level,
                    cache_status: Some("not_cached".to_string()),
                    last_cache_update: None,
                    cache_hits: Some(0),
                    cache_priority: Some(0),
                    expires_at: opts.expires_at,
                };
                self.db.save_file(&aliased).await?;
                return Ok(aliased);
            }
        }

        let original_size = content.len();
        let mut processed_content = content;

        let is_compressed = if should_compress && original_size >= self.config.compression.min_size as usize {
            let compression_config = crate::config::CompressionConfig {
                enabled: true,
                algorithm: compression_algo.clone(),
                level: compression_lvl,
                min_size: self.config.compression.min_size,
            };
            let compressor = CompressionManager::new(Arc::new(compression_config));
            match compressor.compress(&processed_content) {
                // Small/incompressible input can come out *larger* than the original once
                // format-header and frame overhead are added (zstd/gzip both have a fixed
                // minimum overhead per frame). Only keep the compressed form - and only claim
                // is_compressed - when it's actually smaller; otherwise the DB's
                // valid_compression_ratio CHECK (<=1.0) would reject the insert outright.
                Ok(compressed) if compressed.len() < processed_content.len() => {
                    processed_content = compressed;
                    true
                }
                Ok(_) => false,
                Err(e) => {
                    warn!("⚠️ Compression failed, storing uncompressed: {}", e);
                    false
                }
            }
        } else {
            false
        };
        let compression_ratio = if is_compressed {
            Some(processed_content.len() as f32 / original_size.max(1) as f32)
        } else {
            None
        };

        let is_encrypted = if should_encrypt {
            let algorithm = EncryptionAlgorithm::from_str(&self.config.crypto.algorithm)?;
            processed_content = match &encryption_key_id {
                Some(key_id) => {
                    let key = self.encryption_keys.get_key(key_id).await?;
                    self.crypto.encrypt_with_key(&processed_content, algorithm, &key.key_data)?
                }
                None => self.crypto.encrypt(&processed_content, algorithm).await?,
            };
            true
        } else {
            false
        };

        let file_path = self.get_file_path(bucket, key);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        write_file_atomic_optimized(&file_path, &processed_content).await
            .map_err(|e| StorageError::Io(e.to_string()))?;
        info!("💾 File written to disk: {}", file_path.display());

        let file = StoredFile {
            id: Uuid::new_v4(),
            bucket: bucket.to_string(),
            key: key.to_string(),
            filename: key.split('/').last().unwrap_or(key).to_string(),
            file_path: file_path.to_string_lossy().to_string(),
            file_size: processed_content.len() as i64,
            original_size: original_size as i64,
            content_type: content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
            hash_blake3,
            hash_md5,
            metadata: opts.metadata,
            is_compressed: Some(is_compressed),
            is_encrypted: Some(is_encrypted),
            compression_algorithm: if is_compressed { Some(compression_algo) } else { None },
            encryption_algorithm: if is_encrypted { Some(self.config.crypto.algorithm.clone()) } else { None },
            compression_ratio,
            upload_time: Some(chrono::Utc::now()),
            last_accessed: None,
            access_count: 0,
            encryption_key_id,
            compression_enabled: Some(should_compress),
            encryption_enabled: Some(should_encrypt),
            compression_level: Some(compression_lvl),
            cache_status: Some("not_cached".to_string()),
            last_cache_update: None,
            cache_hits: Some(0),
            cache_priority: Some(0),
            expires_at: opts.expires_at,
        };

        self.db.save_file(&file).await.map_err(|e| {
            error!("❌ Database insert failed, cleaning up file: {}", e);
            if let Err(cleanup_err) = std::fs::remove_file(&file_path) {
                warn!("Failed to cleanup file after DB error: {}", cleanup_err);
            }
            e
        })?;

        // Warm the cache so the first read after upload doesn't have to hit disk.
        if let Err(e) = self.cache.set_file_metadata(bucket, key, &file).await {
            warn!("⚠️ Failed to cache file metadata after upload: {}", e);
        }
        if let Err(e) = self.cache.set_file_content(bucket, key, &processed_content).await {
            warn!("⚠️ Failed to cache file content after upload: {}", e);
        }

        info!(
            "✅ File stored successfully: {}/{} ({} bytes, compressed={}, encrypted={})",
            bucket, key, file.file_size, is_compressed, is_encrypted
        );
        Ok(file)
    }

    /// Ensure bucket exists, creating it if necessary
    async fn ensure_bucket_exists(&self, bucket: &str) -> Result<()> {
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
        let file = match self.cache.get_file_metadata(bucket, key).await {
            Ok(Some(cached)) => cached,
            _ => {
                let file = self.db.get_file(bucket, key).await?;
                if let Err(e) = self.cache.set_file_metadata(bucket, key, &file).await {
                    warn!("⚠️ Failed to cache file metadata: {}", e);
                }
                file
            }
        };

        // Lazy TTL expiry: a file past its expires_at is treated as already
        // gone, even if the periodic sweep hasn't reaped it yet - matches
        // real S3 lifecycle-expiry semantics (the object stops being
        // servable the moment it expires, not whenever some background job
        // happens to run next). Self-heals by actually deleting it here,
        // via the same reference-counted delete_file used everywhere else.
        if let Some(expires_at) = file.expires_at {
            if expires_at <= chrono::Utc::now() {
                let _ = self.delete_file(bucket, key).await;
                return Err(StorageError::NotFound {
                    bucket: bucket.to_string(),
                    key: key.to_string(),
                });
            }
        }

        let raw_content = match self.cache.get_file_content(bucket, key).await {
            Ok(Some(cached)) => cached,
            _ => {
                // Use the DB-recorded path, not one recomputed from (bucket, key): a
                // deduplication-aliased row's bytes physically live at a *different* key's path.
                let file_path = PathBuf::from(&file.file_path);
                let content = match tokio::fs::read(&file_path).await {
                    Ok(c) => c,
                    Err(e) => {
                        // Safety net: if DB has a row but disk file is missing, delete the broken
                        // row and return NotFound so callers can self-heal.
                        let msg = e.to_string();
                        if msg.contains("No such file or directory") || msg.contains("os error 2") {
                            let _ = self.delete_file(bucket, key).await;
                            return Err(StorageError::NotFound {
                                bucket: bucket.to_string(),
                                key: key.to_string(),
                            });
                        }
                        return Err(StorageError::Io(msg));
                    }
                };
                if let Err(e) = self.cache.set_file_content(bucket, key, &content).await {
                    warn!("⚠️ Failed to cache file content: {}", e);
                }
                content
            }
        };

        let content = self.decode_content(&file, raw_content).await?;

        if let Err(e) = self.db.update_access(&file.id).await {
            warn!("⚠️ Failed to update access stats for {}/{}: {}", bucket, key, e);
        }

        Ok((content, Some(file.content_type)))
    }

    /// For a Range request on a PLAIN (uncompressed, unencrypted) file, reads only the
    /// requested byte span directly from disk via seek - skips the full-file read, the
    /// content cache, and decode entirely, unlike `get_file()`. Returns `Ok(None)` when the
    /// file is compressed or encrypted (decode needs the whole buffer to decrypt/validate)
    /// so the caller falls back to `get_file()` + slicing the result, same as before this
    /// existed. A real memory/latency win for the common case this targets - a large plain
    /// video file, where a viewer seeking to one point shouldn't cost a full-file read.
    pub async fn get_file_range_if_plain(
        &self,
        bucket: &str,
        key: &str,
        start: u64,
        end: u64,
    ) -> Result<Option<(Vec<u8>, String, u64)>> {
        let file = match self.cache.get_file_metadata(bucket, key).await {
            Ok(Some(cached)) => cached,
            _ => {
                let file = self.db.get_file(bucket, key).await?;
                if let Err(e) = self.cache.set_file_metadata(bucket, key, &file).await {
                    warn!("⚠️ Failed to cache file metadata: {}", e);
                }
                file
            }
        };

        if file.is_compressed.unwrap_or(false) || file.is_encrypted.unwrap_or(false) {
            return Ok(None);
        }

        let total = file.file_size as u64;
        let clamped_end = end.min(total.saturating_sub(1));
        if total == 0 || start > clamped_end {
            return Ok(None); // let the caller's own range validation produce the right error
        }
        let len = (clamped_end - start + 1) as usize;

        let mut f = fs::File::open(&file.file_path)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;
        f.seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;
        let mut buf = vec![0u8; len];
        f.read_exact(&mut buf)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;

        if let Err(e) = self.db.update_access(&file.id).await {
            warn!("⚠️ Failed to update access stats for {}/{}: {}", bucket, key, e);
        }

        Ok(Some((buf, file.content_type, total)))
    }

    /// For a Range request on a file that's compressed but NOT encrypted (the actual state
    /// of most of this instance's real video clips - compression is applied automatically
    /// above a size threshold), streams the decompression instead of reading the whole file
    /// AND materializing the whole decompressed output in memory the way `get_file()` does.
    /// Can't skip decoding the portion before `start` (gzip/zstd aren't seekable formats,
    /// decompression is inherently sequential from the beginning), but never holds more than
    /// the compressed input plus the requested output slice at once - the real memory win for
    /// a large file where only a small span is actually being served.
    ///
    /// Returns `Ok(None)` - falling back to the existing `get_file()` + slice path, unchanged
    /// behavior - when the file is encrypted (AEAD's auth tag covers the whole ciphertext, so
    /// partial decryption isn't safe without changing the storage format entirely, out of
    /// scope here), not compressed (the plain fast path already covers that), or the
    /// decompressor doesn't recognize the format (a rarer legacy case the full path already
    /// handles correctly).
    pub async fn get_compressed_file_range(
        &self,
        bucket: &str,
        key: &str,
        start: u64,
        end: u64,
    ) -> Result<Option<(Vec<u8>, String, u64)>> {
        let file = match self.cache.get_file_metadata(bucket, key).await {
            Ok(Some(cached)) => cached,
            _ => {
                let file = self.db.get_file(bucket, key).await?;
                if let Err(e) = self.cache.set_file_metadata(bucket, key, &file).await {
                    warn!("⚠️ Failed to cache file metadata: {}", e);
                }
                file
            }
        };

        if file.is_encrypted.unwrap_or(false) || !file.is_compressed.unwrap_or(false) {
            return Ok(None);
        }

        let total = file.original_size as u64; // Range refers to the DECOMPRESSED length
        let clamped_end = end.min(total.saturating_sub(1));
        if total == 0 || start > clamped_end {
            return Ok(None);
        }
        let want_len = (clamped_end - start + 1) as usize;
        let content_type = file.content_type.clone();
        let file_path = file.file_path.clone();
        let compression = Arc::clone(&self.compression);
        let start_usize = start as usize;

        let decoded = tokio::task::spawn_blocking(move || -> Result<Option<Vec<u8>>> {
            let compressed = std::fs::read(&file_path)?;
            compression.decompress_range(&compressed, start_usize, want_len)
        })
        .await
        .map_err(|e| StorageError::Io(format!("decompress task panicked: {}", e)))??;

        let buf = match decoded {
            Some(buf) => buf,
            None => return Ok(None),
        };

        if let Err(e) = self.db.update_access(&file.id).await {
            warn!("⚠️ Failed to update access stats for {}/{}: {}", bucket, key, e);
        }

        Ok(Some((buf, content_type, total)))
    }

    /// Reverse whatever `store_file` did: decrypt (if encrypted), then decompress (if
    /// compressed) - the exact inverse order of compress-then-encrypt on write.
    async fn decode_content(&self, file: &StoredFile, mut content: Vec<u8>) -> Result<Vec<u8>> {
        if file.is_encrypted.unwrap_or(false) {
            let algorithm = EncryptionAlgorithm::from_str(
                file.encryption_algorithm.as_deref().unwrap_or("aes-gcm")
            )?;
            content = match &file.encryption_key_id {
                Some(key_id) => {
                    let key = self.encryption_keys.get_key(key_id).await?;
                    self.crypto.decrypt_with_key(&content, algorithm, &key.key_data)?
                }
                None => self.crypto.decrypt(&content, algorithm).await?,
            };
        }

        if file.is_compressed.unwrap_or(false) {
            content = self.compression.decompress(&content)?;
        }

        Ok(content)
    }

    pub async fn delete_file(&self, bucket: &str, key: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        let file = sqlx::query!(
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

        // Deduplication (see store_file) can alias multiple (bucket, key) rows onto the same
        // on-disk file_path. Only unlink the physical file once nothing else still points at it,
        // otherwise deleting one aliased key would corrupt every other key sharing those bytes.
        let other_references: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM files WHERE file_path = $1 AND NOT (bucket = $2 AND key = $3)"
        )
        .bind(&file.file_path)
        .bind(bucket)
        .bind(key)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
            "DELETE FROM files WHERE bucket = $1 AND key = $2",
            bucket,
            key
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        if other_references == 0 {
            if let Err(e) = tokio::fs::remove_file(&file.file_path).await {
                warn!("⚠️ Failed to delete file from disk (file may not exist): {}", e);
            }
        } else {
            info!("🔗 Skipping physical delete for {}/{} - {} other key(s) still reference these bytes", bucket, key, other_references);
        }

        if let Err(e) = self.cache.invalidate_file(bucket, key).await {
            warn!("⚠️ Failed to invalidate cache for {}/{}: {}", bucket, key, e);
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
        // Same lazy-expiry self-heal as get_file - see that method's comment.
        if let Some(expires_at) = file.expires_at {
            if expires_at <= chrono::Utc::now() {
                let _ = self.delete_file(bucket, key).await;
                return Err(StorageError::NotFound {
                    bucket: bucket.to_string(),
                    key: key.to_string(),
                });
            }
        }
        Ok(FileInfo::from(file))
    }

    /// Permanently deletes every currently-expired file (expires_at set AND
    /// in the past). Called on a periodic interval from main.rs. Reuses
    /// delete_file's existing reference-counted physical-delete logic per
    /// key - never touches a file still referenced by another (bucket, key)
    /// row (deduplication can alias multiple keys onto the same bytes).
    /// Returns the number of rows actually removed. The WHERE clause this
    /// relies on (in database.rs's `list_expired_files`) is deliberately
    /// narrow: `expires_at IS NOT NULL AND expires_at < NOW()` - a file that
    /// never opted into a TTL can never match it, regardless of anything
    /// else about that file.
    pub async fn sweep_expired_files(&self) -> Result<usize> {
        let expired = self.db.list_expired_files().await?;
        let mut removed = 0usize;
        for (bucket, key) in expired {
            match self.delete_file(&bucket, &key).await {
                Ok(()) => {
                    removed += 1;
                    info!("⏰ TTL sweep removed expired file: {}/{}", bucket, key);
                }
                Err(e) => {
                    warn!("⚠️ TTL sweep failed to remove {}/{}: {}", bucket, key, e);
                }
            }
        }
        Ok(removed)
    }

    pub async fn get_storage_stats(&self) -> Result<StorageStats> {
        self.db.get_total_stats().await
    }

    pub async fn list_buckets(&self) -> Result<Vec<String>> {
        self.db.list_buckets().await
    }

    pub async fn list_buckets_with_created_at(&self) -> Result<Vec<(String, chrono::DateTime<chrono::Utc>)>> {
        self.db.list_buckets_with_created_at().await
    }

    pub async fn delete_bucket(&self, bucket: &str) -> Result<()> {
        info!("🗑️ Starting bucket deletion: {}", bucket);

        let files = sqlx::query!(
            "SELECT key, file_path FROM files WHERE bucket = $1",
            bucket
        )
        .fetch_all(&self.pool)
        .await?;

        info!("📊 Found {} files to delete in bucket {}", files.len(), bucket);

        let mut tx = self.pool.begin().await?;

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

        tx.commit().await?;
        info!("✅ Database cleanup completed for bucket {} (CASCADE deleted {} file records)", bucket, files.len());

        let mut cleaned_files = 0;
        for file in files {
            // Same aliasing concern as delete_file(): the bucket's rows are already gone (CASCADE
            // above), so any remaining row with this file_path is a dedup-aliased key elsewhere
            // that's still using these bytes.
            let other_references: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM files WHERE file_path = $1")
                .bind(&file.file_path)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);

            if other_references == 0 {
                if let Err(e) = tokio::fs::remove_file(&file.file_path).await {
                    warn!("⚠️ Failed to delete file from disk: {} - {}", file.file_path, e);
                } else {
                    cleaned_files += 1;
                }
            } else {
                info!("🔗 Skipping physical delete for {}/{} - {} other key(s) still reference these bytes", bucket, file.key, other_references);
            }
            if let Err(e) = self.cache.invalidate_file(bucket, &file.key).await {
                warn!("⚠️ Failed to invalidate cache for {}/{}: {}", bucket, file.key, e);
            }
        }

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
            self.db.search_files("", query, limit.unwrap_or(100)).await?
        };
        Ok(files.into_iter().map(FileInfo::from).collect())
    }

    pub async fn create_bucket(&self, bucket: &str) -> Result<()> {
        let bucket_path = self.base_path.join(bucket);
        tokio::fs::create_dir_all(&bucket_path).await?;

        let _ = sqlx::query!(
            "INSERT INTO buckets (name, is_active) VALUES ($1, true) ON CONFLICT (name) DO NOTHING",
            bucket
        )
        .execute(&self.pool)
        .await?;

        info!("✅ Created bucket: {}", bucket);
        Ok(())
    }

    /// Ping the database and (if enabled) Redis, for /health.
    pub async fn health_check(&self) -> (bool, bool) {
        let db_ok = sqlx::query("SELECT 1").execute(&self.pool).await.is_ok();
        let redis_ok = if self.cache.is_enabled() {
            self.cache.health_check().await
        } else {
            true // Redis is optional - not configured is not "unhealthy"
        };
        (db_ok, redis_ok)
    }

    /// Get the maximum file size from configuration
    pub fn max_file_size(&self) -> usize {
        self.config.storage.max_file_size as usize
    }

    // ---- Encryption key management (used by per-file encryption_key_id) ----

    pub async fn create_encryption_key(&self, algorithm: &str, description: Option<&str>) -> Result<crate::models::EncryptionKey> {
        let algorithm = EncryptionAlgorithm::from_str(algorithm)?;
        let key_id = EncryptionKeyManager::generate_key_id();
        let mut key_data = vec![0u8; 32];
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut key_data);
        self.encryption_keys.create_key(&key_id, &key_data, algorithm.as_str(), description).await
    }

    pub async fn list_encryption_keys(&self) -> Result<Vec<crate::models::EncryptionKey>> {
        self.encryption_keys.list_keys().await
    }

    pub async fn deactivate_encryption_key(&self, key_id: &str) -> Result<()> {
        self.encryption_keys.deactivate_key(key_id).await
    }

    fn get_file_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.base_path.join(bucket).join(key)
    }
}
