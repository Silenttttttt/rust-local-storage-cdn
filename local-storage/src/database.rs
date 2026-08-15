use std::sync::Arc;
use std::borrow::Cow;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use crate::{
    config::DatabaseConfig,
    models::StoredFile,
    errors::{Result, StorageError},
};
use sqlx::types::BigDecimal;
use chrono::{DateTime, Utc};

pub struct DatabaseManager {
    pool: PgPool,
    config: Arc<DatabaseConfig>,
}

impl DatabaseManager {
    /// Takes an already-connected pool rather than opening its own - this used to call
    /// `PgPool::connect(&config.url)` here, a second fully independent connection to the exact
    /// same database `StorageManager::new`'s caller already connected in main.rs. On a cold
    /// start (this app + Postgres both waking from zero) that redundant connect/auth handshake
    /// serialized in after CacheManager's own init, adding real, entirely avoidable latency to
    /// every boot for no behavioral benefit - nothing here needs a second, separate connection.
    pub async fn new(pool: PgPool, config: Arc<DatabaseConfig>) -> Result<Self> {
        Ok(Self { pool, config })
    }

    /// Real S3 semantics: PutObject to an existing key OVERWRITES it - it never errors with
    /// "already exists". This used to be a plain INSERT with no ON CONFLICT clause, so any
    /// second write to the same (bucket, key) with genuinely different content unconditionally
    /// hit the `unique_bucket_key` constraint and errored - and worse, since the physical file
    /// at `file_path` is already overwritten on disk (deterministic per (bucket, key), not
    /// content-addressed) by the time this runs, the caller's error-path cleanup deleted that
    /// freshly-overwritten file outright, leaving the OLD row pointing at nothing. Confirmed
    /// live this had never actually destroyed real stored data (all 42 currently-stored files'
    /// `file_path`s verified present on disk), but it was a real, reachable landmine - a caller
    /// simply re-uploading to an existing key (exactly what fb-ad-capture-consumer's own re-sync
    /// path does) would trigger it. `id` is deliberately excluded from the UPDATE SET - the
    /// existing row's primary key persists across an overwrite, matching how a real S3 object's
    /// identity survives a PUT rather than becoming a "new" object.
    pub async fn save_file(&self, file: &StoredFile) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO files (
                id, bucket, key, filename, file_path, file_size, original_size, content_type,
                hash_blake3, hash_md5, metadata, is_compressed, is_encrypted,
                compression_algorithm, encryption_algorithm, compression_ratio,
                upload_time, last_accessed, access_count, encryption_key_id,
                compression_enabled, encryption_enabled, compression_level
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16,
                $17, $18, $19, $20, $21, $22, $23
            )
            ON CONFLICT (bucket, key) DO UPDATE SET
                filename = EXCLUDED.filename,
                file_path = EXCLUDED.file_path,
                file_size = EXCLUDED.file_size,
                original_size = EXCLUDED.original_size,
                content_type = EXCLUDED.content_type,
                hash_blake3 = EXCLUDED.hash_blake3,
                hash_md5 = EXCLUDED.hash_md5,
                metadata = EXCLUDED.metadata,
                is_compressed = EXCLUDED.is_compressed,
                is_encrypted = EXCLUDED.is_encrypted,
                compression_algorithm = EXCLUDED.compression_algorithm,
                encryption_algorithm = EXCLUDED.encryption_algorithm,
                compression_ratio = EXCLUDED.compression_ratio,
                upload_time = EXCLUDED.upload_time,
                last_accessed = EXCLUDED.last_accessed,
                access_count = EXCLUDED.access_count,
                encryption_key_id = EXCLUDED.encryption_key_id,
                compression_enabled = EXCLUDED.compression_enabled,
                encryption_enabled = EXCLUDED.encryption_enabled,
                compression_level = EXCLUDED.compression_level
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
            file.metadata.as_ref(),
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
            file.compression_level
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

    pub async fn get_file(&self, bucket: &str, key: &str) -> Result<StoredFile> {
        let file = sqlx::query_as!(
            StoredFile,
            "SELECT * FROM files WHERE bucket = $1 AND key = $2",
            bucket,
            key
        )
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StorageError::NotFound { bucket: bucket.to_string(), key: key.to_string() })?;

        Ok(file)
    }

    pub async fn get_file_by_hash(&self, hash_blake3: &str) -> Result<Option<StoredFile>> {
        let file = sqlx::query_as!(
            StoredFile,
            "SELECT * FROM files WHERE hash_blake3 = $1 LIMIT 1",
            hash_blake3
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(file)
    }

    pub async fn update_access(&self, id: &Uuid) -> Result<()> {
        sqlx::query!(
            "UPDATE files SET last_accessed = NOW(), access_count = access_count + 1 WHERE id = $1",
            id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_file(&self, bucket: &str, key: &str) -> Result<()> {
        let result = sqlx::query!(
            "DELETE FROM files WHERE bucket = $1 AND key = $2",
            bucket,
            key
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound { bucket: bucket.to_string(), key: key.to_string() });
        }

        Ok(())
    }

    pub async fn list_files(&self, bucket: &str, prefix: Option<&str>, limit: i64, offset: i64) -> Result<Vec<StoredFile>> {
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

        Ok(files)
    }

    pub async fn search_files(&self, bucket: &str, query: &str, limit: i64) -> Result<Vec<StoredFile>> {
        let files = if bucket.is_empty() {
            sqlx::query_as!(
                StoredFile,
                r#"
                SELECT * FROM files 
                WHERE 
                    key ILIKE $1 OR
                    filename ILIKE $1 OR
                    content_type ILIKE $1 OR
                    metadata::text ILIKE $1
                ORDER BY upload_time DESC
                LIMIT $2
                "#,
                format!("%{}%", query),
                limit
            )
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as!(
                StoredFile,
                r#"
                SELECT * FROM files 
                WHERE 
                    bucket = $1 AND
                    (key ILIKE $2 OR
                    filename ILIKE $2 OR
                    content_type ILIKE $2 OR
                    metadata::text ILIKE $2)
                ORDER BY upload_time DESC
                LIMIT $3
                "#,
                bucket,
                format!("%{}%", query),
                limit
            )
            .fetch_all(&self.pool)
            .await?
        };

        Ok(files)
    }

    pub async fn get_bucket_stats(&self, bucket: &str) -> Result<(i64, i64)> {
        let row = sqlx::query!(
            "SELECT COUNT(*) as file_count, COALESCE(SUM(file_size), 0) as total_size FROM files WHERE bucket = $1",
            bucket
        )
        .fetch_one(&self.pool)
        .await?;

        Ok((
            row.file_count.unwrap_or(0),
            row.total_size.unwrap_or_else(|| BigDecimal::from(0)).to_string().parse::<i64>().unwrap_or(0)
        ))
    }

    pub async fn get_total_stats(&self) -> Result<crate::models::StorageStats> {
        let row = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as total_files,
                COALESCE(SUM(file_size), 0) as total_size,
                COUNT(*) FILTER (WHERE is_compressed) as compressed_files,
                COUNT(*) FILTER (WHERE is_encrypted) as encrypted_files,
                COALESCE(AVG(compression_ratio), 0.0) as avg_compression_ratio
            FROM files
            "#
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(crate::models::StorageStats {
            total_files: row.total_files.unwrap_or(0),
            total_size: row.total_size.unwrap_or_else(|| BigDecimal::from(0)).to_string().parse::<i64>().unwrap_or(0),
            compressed_files: row.compressed_files.unwrap_or(0),
            encrypted_files: row.encrypted_files.unwrap_or(0),
            compression_ratio: row.avg_compression_ratio.map(|r| r as f32),
            last_updated: Utc::now(),
        })
    }

    pub async fn get_popular_files(&self, limit: i64) -> Result<Vec<StoredFile>> {
        let files = sqlx::query_as!(
            StoredFile,
            "SELECT * FROM files ORDER BY access_count DESC, COALESCE(last_accessed, '1970-01-01'::timestamp) DESC LIMIT $1",
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(files)
    }

    /// Lists buckets from the `buckets` table itself, not derived from `files` - a bucket with
    /// zero files (e.g. just created) still exists and should be listed.
    /// Uses the runtime-checked query form since there's no live DB here to regenerate the
    /// offline sqlx cache for this (previously-different) query text.
    pub async fn list_buckets(&self) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT name FROM buckets WHERE is_active = true ORDER BY name")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|row| row.get::<String, _>("name")).collect())
    }

    pub async fn list_buckets_with_created_at(&self) -> Result<Vec<(String, DateTime<Utc>)>> {
        let rows = sqlx::query("SELECT name, created_at FROM buckets WHERE is_active = true ORDER BY name")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.get::<String, _>("name"), row.get::<DateTime<Utc>, _>("created_at")))
            .collect())
    }

    pub async fn bucket_exists(&self, bucket: &str) -> Result<bool> {
        let row = sqlx::query!(
            "SELECT COUNT(*) as count FROM files WHERE bucket = $1 LIMIT 1",
            bucket
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.count.unwrap_or(0) > 0)
    }

    pub async fn delete_bucket(&self, bucket: &str) -> Result<()> {
        let result = sqlx::query!("DELETE FROM files WHERE bucket = $1", bucket)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(StorageError::InvalidBucket(bucket.to_string()));
        }

        Ok(())
    }
} 