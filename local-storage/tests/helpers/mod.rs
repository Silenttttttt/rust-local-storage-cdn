use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;
use uuid::Uuid;
use serde_json;
use std::path::PathBuf;
use crate::config::{Config, ServerConfig, DatabaseConfig, RedisConfig, StorageConfig, CryptoConfig, CompressionConfig};

pub mod fixtures;

/// Test configuration helper
pub struct TestConfig {
    pub temp_dir: TempDir,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            temp_dir: tempfile::tempdir().expect("Failed to create temp dir"),
        }
    }
}

impl TestConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(self) -> Arc<Config> {
        Arc::new(Config {
            server: ServerConfig {
                port: 8080,
                log_level: "debug".to_string(),
            },
            database: DatabaseConfig {
                host: "192.168.1.218".to_string(),
                port: 30432,
                database: "local_storage_db".to_string(),
                username: "postgres".to_string(),
                password: "postgres123".to_string(),
                max_connections: 10,
                url: "postgresql://postgres:postgres123@192.168.1.218:30432/local_storage_db".to_string(),
            },
            redis: RedisConfig {
                host: "localhost".to_string(),
                port: 6379,
                password: None,
                database: 0,
                max_connections: 10,
                ttl_seconds: 3600,
            },
            storage: StorageConfig {
                path: self.temp_dir.path().to_string_lossy().to_string(),
                max_file_size: 1024 * 1024 * 10, // 10MB
                default_bucket: "test".to_string(),
                enable_deduplication: true,
            },
            crypto: CryptoConfig {
                enabled: false,
                algorithm: "aes-gcm".to_string(),
                key: None,
            },
            compression: CompressionConfig {
                enabled: false,
                algorithm: "zstd".to_string(),
                level: 3,
                min_size: 1024,
            },
        })
    }
}

/// Test data generators
pub struct TestData;

impl TestData {
    pub fn sample_text() -> Vec<u8> {
        b"Hello, World! This is a test file.".to_vec()
    }

    pub fn large_text(size: usize) -> Vec<u8> {
        "A".repeat(size).into_bytes()
    }

    pub fn random_bytes(size: usize) -> Vec<u8> {
        // Simple pseudo-random bytes for testing
        let mut data = Vec::with_capacity(size);
        let mut seed = 12345u64;
        
        for _ in 0..size {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            data.push((seed >> 8) as u8);
        }
        
        data
    }

    pub fn json_data() -> Vec<u8> {
        br#"{"name":"test","value":42,"array":[1,2,3],"nested":{"key":"value"}}"#.to_vec()
    }

    pub fn compressible_data() -> Vec<u8> {
        // Highly repetitive data that compresses well
        "This is a test string that repeats. ".repeat(100).into_bytes()
    }
}

/// Test file helpers
pub struct TestFile {
    pub bucket: String,
    pub key: String,
    pub content: Vec<u8>,
    pub content_type: String,
}

impl TestFile {
    pub fn new(bucket: &str, key: &str, content: Vec<u8>) -> Self {
        Self {
            bucket: bucket.to_string(),
            key: key.to_string(),
            content,
            content_type: "application/octet-stream".to_string(),
        }
    }

    pub fn with_content_type(mut self, content_type: &str) -> Self {
        self.content_type = content_type.to_string();
        self
    }

    pub fn text_file(bucket: &str, key: &str, content: &str) -> Self {
        Self::new(bucket, key, content.as_bytes().to_vec())
            .with_content_type("text/plain")
    }

    pub fn json_file(bucket: &str, key: &str, content: &str) -> Self {
        Self::new(bucket, key, content.as_bytes().to_vec())
            .with_content_type("application/json")
    }
}

/// Assertion helpers
pub mod assertions {
    pub fn assert_file_content_equal(actual: &[u8], expected: &[u8]) {
        assert_eq!(actual, expected, "File content does not match");
    }

    pub fn assert_hash_valid(hash: &str, expected_length: usize) {
        assert!(!hash.is_empty(), "Hash should not be empty");
        assert_eq!(hash.len(), expected_length, "Hash length is incorrect");
    }

    pub fn assert_compression_effective(original_size: usize, compressed_size: usize, min_ratio: f64) {
        let ratio = compressed_size as f64 / original_size as f64;
        assert!(ratio < min_ratio, "Compression ratio {} is not effective enough (should be < {})", ratio, min_ratio);
    }

    pub fn assert_file_exists(path: &std::path::Path) {
        assert!(path.exists(), "File should exist at path: {:?}", path);
    }

    pub fn assert_file_not_exists(path: &std::path::Path) {
        assert!(!path.exists(), "File should not exist at path: {:?}", path);
    }
} 