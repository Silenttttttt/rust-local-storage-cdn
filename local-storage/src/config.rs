use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub cache: CacheSettings,
    pub storage: StorageConfig,
    pub crypto: CryptoConfig,
    pub compression: CompressionConfig,
    pub s3: Option<S3Config>,
    /// Shared secret required to read file content/listings, mirroring the SAME
    /// `WRITE_PROTECTION_TOKEN` the cube-activator's HTTP front door already
    /// requires (as `X-Activator-Write-Token`) for mutating methods on any
    /// `write_protected` app - see cube-activator/src/cube_activator/
    /// http_proxy.py. Reused here, on the read side, rather than a second
    /// secret: chat-server already holds this exact value server-side
    /// (`LOCAL_STORAGE_WRITE_TOKEN` in its own env, see apps/chat-server/
    /// backend/src/local-storage/client.ts) and never sends it to a browser,
    /// so it's already the right shape of secret for gating reads too.
    ///
    /// Optional/presence-gated, same convention as `s3` above: unset (the
    /// default) leaves every GET exactly as unauthenticated as before this
    /// field existed - this app is also run standalone (docker-compose,
    /// local dev) with no activator or chat-server anywhere nearby, and must
    /// keep working with zero config there.
    pub download_protection_token: Option<String>,
    /// Which buckets `download_protection_token` actually gates, when set.
    ///
    /// Added after `download_protection_token` itself: the very first
    /// deployment of that field gated EVERY bucket unconditionally the
    /// instant the token was configured, on a shared multi-tenant instance
    /// of this service (many unrelated apps' buckets, not just the one that
    /// needed protecting) - confirmed live as a real regression the moment
    /// it shipped: buckets with nothing to do with the app that requested
    /// this feature started 403ing for their own normal, previously-public
    /// reads. This field narrows the gate to only the bucket(s) that
    /// actually need it.
    ///
    /// `None` (unset) with a token configured falls back to the original
    /// protect-everything behavior - a deliberate fail-CLOSED default for
    /// any OTHER deployment of this exact version that already relies on
    /// blanket protection and hasn't set this new var; every bucket this
    /// instance actually serves must be enumerated explicitly to get
    /// selective protection instead.
    pub protected_read_buckets: Option<Vec<String>>,
}

/// The v2 S3-compatible API is opt-in: it's only mounted at /v2 if both S3_ACCESS_KEY and
/// S3_SECRET_KEY are set. No auto-generated credentials on startup - that would silently change
/// on every restart and break any client already configured against it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CacheBackendKind {
    Memory,
    Redis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSettings {
    pub backend: CacheBackendKind,
    pub max_size_mb: u64,
    pub ttl_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub max_connections: u32,
    pub url: String,
}

/// Connection details used only when `CacheSettings.backend == Redis`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
    pub database: u8,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub path: String,
    pub max_file_size: u64,
    pub default_bucket: String,
    pub enable_deduplication: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoConfig {
    pub enabled: bool,
    pub algorithm: String, // "aes-gcm" or "chacha20poly1305"
    pub key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    pub enabled: bool,
    pub algorithm: String, // "gzip", "zstd"
    pub level: i32,
    pub min_size: u64, // Minimum file size to compress
}

impl Config {
    pub async fn load() -> Result<Self> {
        dotenvy::dotenv().ok(); // Load .env file if present

        let config = Config {
            server: ServerConfig {
                port: env::var("PORT")
                    .unwrap_or_else(|_| "8080".to_string())
                    .parse()?,
                log_level: env::var("LOG_LEVEL")
                    .unwrap_or_else(|_| "INFO".to_string()),
            },
            database: DatabaseConfig::new()?,
            redis: RedisConfig {
                host: env::var("REDIS_HOST")
                    .unwrap_or_else(|_| "redis".to_string()),
                port: env::var("REDIS_PORT")
                    .unwrap_or_else(|_| "6379".to_string())
                    .parse()?,
                password: env::var("REDIS_PASSWORD").ok(),
                database: env::var("REDIS_DB")
                    .unwrap_or_else(|_| "0".to_string())
                    .parse()?,
                max_connections: env::var("REDIS_MAX_CONNECTIONS")
                    .unwrap_or_else(|_| "10".to_string())
                    .parse()?,
            },
            cache: CacheSettings {
                // CACHE_BACKEND=memory|redis, default memory. Falls back to the legacy
                // ENABLE_REDIS=true toggle for anyone relying on it, so existing deployments
                // that already set it don't silently change behavior.
                backend: match env::var("CACHE_BACKEND").ok().as_deref() {
                    Some("redis") => CacheBackendKind::Redis,
                    Some("memory") => CacheBackendKind::Memory,
                    _ if env::var("ENABLE_REDIS").map(|v| v == "true").unwrap_or(false) => CacheBackendKind::Redis,
                    _ => CacheBackendKind::Memory,
                },
                max_size_mb: env::var("CACHE_MAX_SIZE_MB")
                    .unwrap_or_else(|_| "256".to_string())
                    .parse()?,
                ttl_seconds: env::var("CACHE_TTL_SECONDS")
                    .or_else(|_| env::var("REDIS_TTL_SECONDS"))
                    .unwrap_or_else(|_| "3600".to_string())
                    .parse()?,
            },
            storage: StorageConfig {
                path: env::var("STORAGE_PATH")
                    .unwrap_or_else(|_| "/storage".to_string()),
                max_file_size: env::var("MAX_FILE_SIZE")
                    .unwrap_or_else(|_| "1073741824".to_string()) // 1GB
                    .parse()?,
                default_bucket: env::var("DEFAULT_BUCKET")
                    .unwrap_or_else(|_| "default".to_string()),
                enable_deduplication: env::var("ENABLE_DEDUPLICATION")
                    .unwrap_or_else(|_| "true".to_string())
                    .parse()?,
            },
            crypto: CryptoConfig {
                enabled: env::var("ENABLE_ENCRYPTION")
                    .unwrap_or_else(|_| "false".to_string())
                    .parse()?,
                algorithm: env::var("CRYPTO_ALGORITHM")
                    .unwrap_or_else(|_| "aes-gcm".to_string()),
                key: env::var("CRYPTO_KEY").ok(),
            },
            compression: CompressionConfig {
                enabled: env::var("ENABLE_COMPRESSION")
                    .unwrap_or_else(|_| "false".to_string())
                    .parse()?,
                algorithm: env::var("COMPRESSION_ALGORITHM")
                    .unwrap_or_else(|_| "zstd".to_string()),
                level: env::var("COMPRESSION_LEVEL")
                    .unwrap_or_else(|_| "3".to_string())
                    .parse()?,
                min_size: env::var("COMPRESSION_MIN_SIZE")
                    .unwrap_or_else(|_| "1024".to_string()) // 1KB
                    .parse()?,
            },
            s3: match (env::var("S3_ACCESS_KEY").ok(), env::var("S3_SECRET_KEY").ok()) {
                (Some(access_key), Some(secret_key)) if !access_key.is_empty() && !secret_key.is_empty() => {
                    Some(S3Config {
                        access_key,
                        secret_key,
                        region: env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
                    })
                }
                _ => None,
            },
            // Deliberately the SAME env var name the activator's own front door
            // reads (WRITE_PROTECTION_TOKEN) - not a new, second secret. Empty
            // string is treated the same as unset (matches how the activator's
            // own `secrets_as_env_vars` wiring can hand an app an empty-string
            // env var rather than omitting it outright when a k8s Secret key is
            // present but blank).
            download_protection_token: env::var("WRITE_PROTECTION_TOKEN")
                .ok()
                .filter(|v| !v.is_empty()),
            // Comma-separated bucket names, e.g. "chat-attachments,chat-attachments-live-verify".
            // Whitespace around each name is trimmed (operator-friendly for a
            // hand-edited env value); empty entries (from a trailing comma or
            // an all-whitespace value) are dropped. Unset or empty overall
            // parses to `None` - see this field's own doc comment in
            // `Config` for exactly what that falls back to.
            protected_read_buckets: env::var("PROTECTED_READ_BUCKETS")
                .ok()
                .map(|raw| {
                    raw.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<String>>()
                })
                .filter(|list| !list.is_empty()),
        };

        Ok(config)
    }

    pub fn database_url(&self) -> String {
        self.database.url.clone()
    }

    pub fn redis_url(&self) -> String {
        match &self.redis.password {
            Some(password) => format!(
                "redis://:{}@{}:{}/{}",
                password, self.redis.host, self.redis.port, self.redis.database
            ),
            None => format!(
                "redis://{}:{}/{}",
                self.redis.host, self.redis.port, self.redis.database
            ),
        }
    }
}

impl DatabaseConfig {
    pub fn new() -> Result<Self, anyhow::Error> {
        let host = env::var("POSTGRES_HOST")
            .unwrap_or_else(|_| "localhost".to_string());
        let port = env::var("POSTGRES_PORT")
            .unwrap_or_else(|_| "5432".to_string())
            .parse()?;
        let database = env::var("POSTGRES_DB")
            .unwrap_or_else(|_| "local_storage".to_string());
        let username = env::var("POSTGRES_USER")
            .unwrap_or_else(|_| "postgres".to_string());
        let password = env::var("POSTGRES_PASSWORD")
            .unwrap_or_else(|_| "postgres123".to_string());
        let max_connections = env::var("DB_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "10".to_string())
            .parse()?;
        
        let url = format!(
            "postgres://{}:{}@{}:{}/{}",
            username, password, host, port, database
        );

        Ok(Self {
            host,
            port,
            database,
            username,
            password,
            max_connections,
            url,
        })
    }

    pub async fn pool(&self) -> Result<sqlx::PgPool> {
        let pool = sqlx::PgPool::connect(&self.url).await?;
        Ok(pool)
    }
} 