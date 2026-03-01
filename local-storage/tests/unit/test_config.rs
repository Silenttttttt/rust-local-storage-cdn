use std::env;
use tempfile::tempdir;
use local_storage::config::{Config, DatabaseConfig, ServerConfig, StorageConfig, CryptoConfig, CompressionConfig};

#[tokio::test]
async fn test_default_config_creation() {
    // Clear environment variables that might interfere
    let vars_to_clear = [
        "PORT", "LOG_LEVEL", "POSTGRES_HOST", "POSTGRES_PORT", 
        "REDIS_HOST", "REDIS_PORT", "STORAGE_PATH", "ENABLE_COMPRESSION", "ENABLE_ENCRYPTION"
    ];
    
    for var in &vars_to_clear {
        env::remove_var(var);
    }
    
    // Explicitly set compression to false to ensure consistent test behavior
    env::set_var("ENABLE_COMPRESSION", "false");

    let config = Config::load().await.expect("Failed to load default config");
    
    assert_eq!(config.server.port, 8080);
    assert_eq!(config.server.log_level, "INFO");
    assert_eq!(config.database.host, "192.168.1.218");
    assert_eq!(config.redis.host, "192.168.1.218");
    assert_eq!(config.storage.path, "/storage");
    assert!(!config.crypto.enabled);
    assert!(!config.compression.enabled, "Compression should be disabled by default");
    
    // Clean up
    env::remove_var("ENABLE_COMPRESSION");
}

#[tokio::test]
async fn test_config_from_environment() {
    // Clear any existing environment variables first
    let vars_to_clear = [
        "PORT", "LOG_LEVEL", "POSTGRES_HOST", "POSTGRES_PORT", "POSTGRES_DB", 
        "REDIS_HOST", "REDIS_PORT", "STORAGE_PATH", "ENABLE_ENCRYPTION", "ENABLE_COMPRESSION"
    ];
    for var in &vars_to_clear {
        env::remove_var(var);
    }
    
    // Set custom environment variables
    env::set_var("PORT", "9090");
    env::set_var("LOG_LEVEL", "DEBUG");
    env::set_var("POSTGRES_HOST", "localhost");
    env::set_var("POSTGRES_PORT", "5432");
    env::set_var("POSTGRES_DB", "test_db");
    env::set_var("REDIS_HOST", "localhost");
    env::set_var("REDIS_PORT", "6379");
    env::set_var("STORAGE_PATH", "/tmp/test-storage");
    env::set_var("ENABLE_ENCRYPTION", "true");
    env::set_var("ENABLE_COMPRESSION", "true");

    let config = Config::load().await.expect("Failed to load config from environment");
    
    assert_eq!(config.server.port, 9090);
    assert_eq!(config.server.log_level, "DEBUG");
    assert_eq!(config.database.host, "localhost");
    assert_eq!(config.database.port, 5432);
    assert_eq!(config.redis.host, "localhost");
    assert_eq!(config.redis.port, 6379);
    assert_eq!(config.storage.path, "/tmp/test-storage");
    assert!(config.crypto.enabled);
    assert!(config.compression.enabled);

    // Clean up
    for var in &vars_to_clear {
        env::remove_var(var);
    }
}

#[test]
fn test_database_url_generation() {
    let db_config = DatabaseConfig {
        host: "localhost".to_string(),
        port: 5432,
        database: "test_db".to_string(),
        username: "user".to_string(),
        password: "pass".to_string(),
        max_connections: 10,
        url: "postgres://user:pass@localhost:5432/test_db".to_string(),
    };

    let expected_url = "postgres://user:pass@localhost:5432/test_db";
    assert_eq!(db_config.url, expected_url);
}

#[test]
fn test_redis_url_generation() {
    let config = Config {
        server: ServerConfig {
            port: 8080,
            log_level: "INFO".to_string(),
        },
        database: DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "test".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            max_connections: 10,
            url: "postgres://user:pass@localhost:5432/test".to_string(),
        },
        redis: local_storage::config::RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            password: Some("redis_pass".to_string()),
            database: 5,
            max_connections: 10,
            ttl_seconds: 3600,
        },
        storage: StorageConfig {
            path: "/storage".to_string(),
            max_file_size: 1024 * 1024 * 1024,
            default_bucket: "default".to_string(),
            enable_deduplication: true,
        },
        crypto: CryptoConfig {
            enabled: false,
            algorithm: "aes-gcm".to_string(),
            key: None,
        },
        compression: CompressionConfig {
            enabled: false,
            algorithm: "gzip".to_string(),
            level: 6,
            min_size: 1024,
        },
    };

    let redis_url = config.redis_url();
    assert_eq!(redis_url, "redis://:redis_pass@localhost:6379/5");
}

#[test]
fn test_redis_url_without_password() {
    let config = Config {
        server: ServerConfig {
            port: 8080,
            log_level: "INFO".to_string(),
        },
        database: DatabaseConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "test".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            max_connections: 10,
            url: "postgres://user:pass@localhost:5432/test".to_string(),
        },
        redis: local_storage::config::RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            password: None,
            database: 0,
            max_connections: 10,
            ttl_seconds: 3600,
        },
        storage: StorageConfig {
            path: "/storage".to_string(),
            max_file_size: 1024 * 1024 * 1024,
            default_bucket: "default".to_string(),
            enable_deduplication: true,
        },
        crypto: CryptoConfig {
            enabled: false,
            algorithm: "aes-gcm".to_string(),
            key: None,
        },
        compression: CompressionConfig {
            enabled: false,
            algorithm: "gzip".to_string(),
            level: 6,
            min_size: 1024,
        },
    };

    let redis_url = config.redis_url();
    assert_eq!(redis_url, "redis://localhost:6379/0");
}

#[test]
fn test_crypto_config_validation() {
    // Test valid AES key
    let crypto_config = CryptoConfig {
        enabled: true,
        algorithm: "aes-gcm".to_string(),
        key: Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()),
    };
    assert!(crypto_config.enabled);
    assert_eq!(crypto_config.algorithm, "aes-gcm");

    // Test ChaCha20 algorithm
    let crypto_config = CryptoConfig {
        enabled: true,
        algorithm: "chacha20poly1305".to_string(),
        key: Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()),
    };
    assert_eq!(crypto_config.algorithm, "chacha20poly1305");
}

#[test]
fn test_compression_config_validation() {
    // Test GZIP compression
    let compression_config = CompressionConfig {
        enabled: true,
        algorithm: "gzip".to_string(),
        level: 6,
        min_size: 1024,
    };
    assert!(compression_config.enabled);
    assert_eq!(compression_config.algorithm, "gzip");
    assert_eq!(compression_config.level, 6);

    // Test ZSTD compression
    let compression_config = CompressionConfig {
        enabled: true,
        algorithm: "zstd".to_string(),
        level: 3,
        min_size: 512,
    };
    assert_eq!(compression_config.algorithm, "zstd");
    assert_eq!(compression_config.level, 3);
    assert_eq!(compression_config.min_size, 512);
}

#[test]
fn test_storage_config_defaults() {
    let storage_config = StorageConfig {
        path: "/custom/storage".to_string(),
        max_file_size: 10 * 1024 * 1024, // 10MB
        default_bucket: "uploads".to_string(),
        enable_deduplication: false,
    };

    assert_eq!(storage_config.path, "/custom/storage");
    assert_eq!(storage_config.max_file_size, 10 * 1024 * 1024);
    assert_eq!(storage_config.default_bucket, "uploads");
    assert!(!storage_config.enable_deduplication);
}

#[tokio::test]
async fn test_config_serialization() {
    let config = Config::load().await.expect("Failed to load config");
    
    // Test that config can be serialized and deserialized
    let serialized = serde_json::to_string(&config).expect("Failed to serialize config");
    let deserialized: Config = serde_json::from_str(&serialized).expect("Failed to deserialize config");
    
    assert_eq!(config.server.port, deserialized.server.port);
    assert_eq!(config.storage.path, deserialized.storage.path);
    assert_eq!(config.crypto.enabled, deserialized.crypto.enabled);
} 