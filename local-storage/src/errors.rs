use axum::{
    response::{IntoResponse, Response},
    http::StatusCode,
    Json,
};
use serde_json::json;
use std::fmt;
use std::string::FromUtf8Error;
use axum::extract::multipart::MultipartError;
use tracing::{error, warn};
use chrono;

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Clone)]
pub enum StorageError {
    NotFound { bucket: String, key: String },
    AlreadyExists { bucket: String, key: String },
    InvalidBucket(String),
    InvalidKey(String),
    InvalidFile(String),
    Validation(String),
    Configuration(String),
    Migration(String),
    Redis(String),
    Json(String),
    Multipart(String),
    BadRequest(String),
    Database(String),
    Io(String),
    Compression(String),
    Encryption(String),
    InvalidEncryptionAlgorithm(String),
    Cache(String),
    MissingEncryptionKey,
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl From<sqlx::Error> for StorageError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err.to_string())
    }
}

impl From<sqlx::migrate::MigrateError> for StorageError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        StorageError::Migration(err.to_string())
    }
}

impl From<redis::RedisError> for StorageError {
    fn from(err: redis::RedisError) -> Self {
        StorageError::Redis(err.to_string())
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(err: serde_json::Error) -> Self {
        StorageError::Json(err.to_string())
    }
}

impl From<MultipartError> for StorageError {
    fn from(err: MultipartError) -> Self {
        StorageError::Multipart(err.to_string())
    }
}

impl From<anyhow::Error> for StorageError {
    fn from(err: anyhow::Error) -> Self {
        StorageError::Io(err.to_string())
    }
}

impl From<FromUtf8Error> for StorageError {
    fn from(_: FromUtf8Error) -> Self {
        StorageError::Validation("Invalid UTF-8 in request".to_string())
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { bucket, key } => write!(f, "File not found: {}/{}", bucket, key),
            Self::AlreadyExists { bucket, key } => write!(f, "File already exists: {}/{}", bucket, key),
            Self::InvalidBucket(msg) => write!(f, "Invalid bucket: {}", msg),
            Self::InvalidKey(msg) => write!(f, "Invalid key: {}", msg),
            Self::InvalidFile(msg) => write!(f, "Invalid file: {}", msg),
            Self::Validation(msg) => write!(f, "Validation error: {}", msg),
            Self::Configuration(msg) => write!(f, "Configuration error: {}", msg),
            Self::Migration(msg) => write!(f, "Migration error: {}", msg),
            Self::Redis(msg) => write!(f, "Redis error: {}", msg),
            Self::Json(msg) => write!(f, "JSON error: {}", msg),
            Self::Multipart(msg) => write!(f, "Multipart error: {}", msg),
            Self::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            Self::Database(msg) => write!(f, "Database error: {}", msg),
            Self::Io(msg) => write!(f, "IO error: {}", msg),
            Self::Compression(msg) => write!(f, "Compression error: {}", msg),
            Self::Encryption(msg) => write!(f, "Encryption error: {}", msg),
            Self::InvalidEncryptionAlgorithm(msg) => write!(f, "Invalid encryption algorithm: {}", msg),
            Self::Cache(msg) => write!(f, "Cache error: {}", msg),
            Self::MissingEncryptionKey => write!(f, "Missing encryption key"),
        }
    }
}

impl std::error::Error for StorageError {}

impl IntoResponse for StorageError {
    fn into_response(self) -> Response {
        let (status, error_message) = match &self {
            Self::NotFound { bucket, key } => {
                warn!("🔍 File not found: {}/{}", bucket, key);
                (StatusCode::NOT_FOUND, format!("File not found: {}/{}", bucket, key))
            },
            Self::AlreadyExists { bucket, key } => {
                warn!("⚠️ File already exists: {}/{}", bucket, key);
                (StatusCode::CONFLICT, format!("File already exists: {}/{}", bucket, key))
            },
            Self::InvalidBucket(msg) => {
                warn!("🗑️ Invalid bucket: {}", msg);
                (StatusCode::BAD_REQUEST, format!("Invalid bucket: {}", msg))
            },
            Self::InvalidKey(msg) => {
                warn!("🔑 Invalid key: {}", msg);
                (StatusCode::BAD_REQUEST, format!("Invalid key: {}", msg))
            },
            Self::InvalidFile(msg) => {
                warn!("📄 Invalid file: {}", msg);
                (StatusCode::BAD_REQUEST, format!("Invalid file: {}", msg))
            },
            Self::Validation(msg) => {
                warn!("✅ Validation error: {}", msg);
                (StatusCode::BAD_REQUEST, format!("Validation error: {}", msg))
            },
            Self::Configuration(msg) => {
                error!("⚙️ Configuration error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Configuration error: {}", msg))
            },
            Self::Migration(msg) => {
                error!("🔄 Migration error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Migration error: {}", msg))
            },
            Self::Redis(msg) => {
                error!("🔴 Redis error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Redis error: {}", msg))
            },
            Self::Json(msg) => {
                warn!("📋 JSON error: {}", msg);
                (StatusCode::BAD_REQUEST, format!("JSON error: {}", msg))
            },
            Self::Multipart(msg) => {
                error!("📦 Multipart error: {}", msg);
                (StatusCode::BAD_REQUEST, format!("Multipart error: {}", msg))
            },
            Self::BadRequest(msg) => {
                warn!("❌ Bad request: {}", msg);
                (StatusCode::BAD_REQUEST, format!("Bad request: {}", msg))
            },
            Self::Database(msg) => {
                error!("🗄️ Database error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Database error: {}", msg))
            },
            Self::Io(msg) => {
                error!("💾 IO error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("IO error: {}", msg))
            },
            Self::Compression(msg) => {
                error!("🗜️ Compression error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Compression error: {}", msg))
            },
            Self::Encryption(msg) => {
                error!("🔐 Encryption error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Encryption error: {}", msg))
            },
            Self::InvalidEncryptionAlgorithm(msg) => {
                error!("🔒 Invalid encryption algorithm: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Invalid encryption algorithm: {}", msg))
            },
            Self::Cache(msg) => {
                error!("💨 Cache error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Cache error: {}", msg))
            },
            Self::MissingEncryptionKey => {
                error!("🔑 Missing encryption key");
                (StatusCode::INTERNAL_SERVER_ERROR, "Missing encryption key".to_string())
            },
        };

        let body = Json(json!({
            "error": error_message,
            "status": status.as_u16(),
            "timestamp": chrono::Utc::now().to_rfc3339()
        }));

        (status, body).into_response()
    }
} 