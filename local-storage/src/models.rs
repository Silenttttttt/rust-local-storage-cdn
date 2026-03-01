use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StoredFile {
    pub id: Uuid,
    pub bucket: String,
    pub key: String,
    pub filename: String,
    pub file_path: String,
    pub file_size: i64,
    pub original_size: i64,
    pub content_type: String,
    pub hash_blake3: String,
    pub hash_md5: String,
    pub metadata: Option<serde_json::Value>,
    pub is_compressed: Option<bool>,
    pub is_encrypted: Option<bool>,
    pub compression_algorithm: Option<String>,
    pub encryption_algorithm: Option<String>,
    pub compression_ratio: Option<f32>,
    pub upload_time: Option<DateTime<Utc>>,
    pub last_accessed: Option<DateTime<Utc>>,
    pub access_count: i64,
    pub encryption_key_id: Option<String>,
    pub compression_enabled: Option<bool>,
    pub encryption_enabled: Option<bool>,
    pub compression_level: Option<i32>,
    pub cache_status: Option<String>,
    pub last_cache_update: Option<DateTime<Utc>>,
    pub cache_hits: Option<i64>,
    pub cache_priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EncryptionKey {
    pub id: i32,
    pub key_id: String,
    pub key_data: Vec<u8>,
    pub algorithm: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub is_active: Option<bool>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CacheConfig {
    pub id: i32,
    pub max_cache_size_gb: Option<f64>,
    pub cache_ttl_seconds: Option<i32>,
    pub preload_enabled: Option<bool>,
    pub min_access_count: Option<i32>,
    pub cache_priority_weights: Option<serde_json::Value>,
    pub auto_cache_threshold: Option<i32>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct StorageStats {
    pub total_files: i64,
    pub total_size: i64,
    pub compressed_files: i64,
    pub encrypted_files: i64,
    pub compression_ratio: Option<f32>,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadRequest {
    pub bucket: String,
    pub key: String,
    pub metadata: Option<serde_json::Value>,
    pub compress: Option<bool>,
    pub encrypt: Option<bool>,
    pub compression_algorithm: Option<String>,
    pub compression_level: Option<i32>,
    pub encryption_key_id: Option<String>,
}

impl Default for UploadRequest {
    fn default() -> Self {
        Self {
            bucket: String::new(),
            key: String::new(),
            metadata: None,
            compress: None,
            encrypt: None,
            compression_algorithm: None,
            compression_level: None,
            encryption_key_id: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResponse {
    pub id: Uuid,
    pub bucket: String,
    pub key: String,
    pub file_size: i64,
    pub content_type: String,
    pub hash_blake3: String,
    pub hash_md5: String,
    pub is_compressed: bool,
    pub is_encrypted: bool,
    pub compression_ratio: Option<f32>,
    pub upload_time: DateTime<Utc>,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
    pub encryption_key_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub id: Uuid,
    pub bucket: String,
    pub key: String,
    pub filename: String,
    pub file_size: i64,
    pub original_size: i64,
    pub content_type: String,
    pub hash_blake3: String,
    pub hash_md5: String,
    pub metadata: Option<serde_json::Value>,
    pub is_compressed: bool,
    pub is_encrypted: bool,
    pub compression_algorithm: Option<String>,
    pub encryption_algorithm: Option<String>,
    pub compression_ratio: Option<f32>,
    pub upload_time: DateTime<Utc>,
    pub last_accessed: Option<DateTime<Utc>>,
    pub access_count: i64,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
    pub encryption_key_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileListResponse {
    pub files: Vec<FileInfo>,
    pub total_count: u64,
    pub page: u32,
    pub per_page: u32,
    pub has_more: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BucketInfo {
    pub name: String,
    pub file_count: u64,
    pub total_size: u64,
    pub compressed_files: u64,
    pub encrypted_files: u64,
    pub compression_ratio: f64,
    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageStatsResponse {
    pub total_files: u64,
    pub total_size: u64,
    pub total_buckets: u64,
    pub compressed_files: u64,
    pub encrypted_files: u64,
    pub average_file_size: f64,
    pub compression_ratio: f64,
    pub popular_files: Vec<PopularFile>,
    pub bucket_stats: Vec<BucketInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PopularFile {
    pub bucket: String,
    pub key: String,
    pub filename: String,
    pub access_count: u64,
    pub file_size: u64,
    pub last_accessed: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub database_connected: bool,
    pub redis_connected: bool,
    pub storage_path: String,
    pub available_space: u64,
    pub total_space: u64,
    pub memory_usage: MemoryUsage,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub used_mb: f64,
    pub total_mb: f64,
    pub usage_percent: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub bucket: Option<String>,
    pub content_type: Option<String>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub uploaded_after: Option<DateTime<Utc>>,
    pub uploaded_before: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub status: u16,
    pub timestamp: DateTime<Utc>,
}

impl From<StoredFile> for FileInfo {
    fn from(file: StoredFile) -> Self {
        FileInfo {
            id: file.id,
            bucket: file.bucket,
            key: file.key,
            filename: file.filename,
            file_size: file.file_size,
            original_size: file.original_size,
            content_type: file.content_type,
            hash_blake3: file.hash_blake3,
            hash_md5: file.hash_md5,
            metadata: file.metadata,
            is_compressed: file.is_compressed.unwrap_or(false),
            is_encrypted: file.is_encrypted.unwrap_or(false),
            compression_algorithm: file.compression_algorithm,
            encryption_algorithm: file.encryption_algorithm,
            compression_ratio: file.compression_ratio,
            upload_time: file.upload_time.unwrap_or_default(),
            last_accessed: file.last_accessed,
            access_count: file.access_count,
            compression_enabled: file.compression_enabled.unwrap_or(false),
            encryption_enabled: file.encryption_enabled.unwrap_or(false),
            encryption_key_id: file.encryption_key_id,
        }
    }
}

impl From<StoredFile> for UploadResponse {
    fn from(file: StoredFile) -> Self {
        UploadResponse {
            id: file.id,
            bucket: file.bucket,
            key: file.key,
            file_size: file.file_size,
            content_type: file.content_type,
            hash_blake3: file.hash_blake3,
            hash_md5: file.hash_md5,
            is_compressed: file.is_compressed.unwrap_or(false),
            is_encrypted: file.is_encrypted.unwrap_or(false),
            compression_ratio: file.compression_ratio,
            upload_time: file.upload_time.unwrap_or_default(),
            compression_enabled: file.compression_enabled.unwrap_or(false),
            encryption_enabled: file.encryption_enabled.unwrap_or(false),
            encryption_key_id: file.encryption_key_id,
        }
    }
} 