use std::path::PathBuf;
use serde_json::{json, Value};
use chrono::Utc;
use uuid::Uuid;
use crate::models::StoredFile;

/// Sample files for testing
pub mod sample_files {
    use super::*;

    pub fn text_file() -> (&'static str, Vec<u8>, &'static str) {
        ("sample.txt", b"Hello, World!".to_vec(), "text/plain")
    }

    pub fn json_file() -> (&'static str, Vec<u8>, &'static str) {
        let content = br#"{"message": "Hello, JSON!"}"#;
        ("sample.json", content.to_vec(), "application/json")
    }

    pub fn binary_file() -> (&'static str, Vec<u8>, &'static str) {
        let content = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG header
        ("sample.png", content, "image/png")
    }

    pub fn large_file(size: usize) -> (&'static str, Vec<u8>, &'static str) {
        let content = "A".repeat(size).into_bytes();
        ("large.txt", content, "text/plain")
    }
}

/// Sample storage statistics for testing
pub mod sample_stats {
    use super::*;

    pub fn basic_stats() -> Value {
        json!({
            "total_files": 10,
            "total_size": 1024 * 1024,
            "compressed_files": 5,
            "encrypted_files": 3,
            "compression_ratio": 0.7
        })
    }

    pub fn empty_stats() -> Value {
        json!({
            "total_files": 0,
            "total_size": 0,
            "compressed_files": 0,
            "encrypted_files": 0,
            "compression_ratio": null
        })
    }

    pub fn large_stats() -> Value {
        json!({
            "total_files": 1000,
            "total_size": 100 * 1024 * 1024,
            "compressed_files": 600,
            "encrypted_files": 400,
            "compression_ratio": 0.65
        })
    }
}

/// Sample content for different file types
pub struct SampleContent;

impl SampleContent {
    pub fn text() -> &'static str {
        "Hello, World! This is a test file content that can be used for testing purposes."
    }

    pub fn json() -> serde_json::Value {
        json!({
            "id": 1,
            "name": "Test Document",
            "tags": ["test", "sample", "document"],
            "metadata": {
                "created_at": "2024-01-01T00:00:00Z",
                "author": "Test User"
            },
            "content": {
                "title": "Sample Title",
                "body": "This is the body content of the test document."
            }
        })
    }

    pub fn csv() -> &'static str {
        "name,age,city\nJohn,30,New York\nJane,25,Los Angeles\nBob,35,Chicago"
    }

    pub fn html() -> &'static str {
        r#"<!DOCTYPE html>
<html>
<head>
    <title>Test Page</title>
</head>
<body>
    <h1>Hello, World!</h1>
    <p>This is a test HTML file.</p>
</body>
</html>"#
    }

    pub fn xml() -> &'static str {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<root>
    <item id="1">
        <name>Test Item</name>
        <value>42</value>
    </item>
    <item id="2">
        <name>Another Item</name>
        <value>84</value>
    </item>
</root>"#
    }

    pub fn binary() -> Vec<u8> {
        // Simple binary data pattern
        (0..256).cycle().take(1024).map(|x| x as u8).collect()
    }

    pub fn compressible() -> String {
        // Highly repetitive content that compresses well
        "ABCDEFGHIJ".repeat(1000)
    }

    pub fn random(size: usize) -> Vec<u8> {
        // Simple pseudo-random data for testing
        let mut data = Vec::with_capacity(size);
        let mut seed = 12345u64;
        
        for _ in 0..size {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            data.push((seed >> 8) as u8);
        }
        
        data
    }
}

pub struct TestConfig {
    pub storage_path: PathBuf,
    pub cache_ttl: u64,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
}

impl TestConfig {
    pub async fn new() -> Self {
        let temp_dir = std::env::temp_dir().join(format!("local_storage_test_{}", std::process::id()));
        
        Self {
            storage_path: temp_dir,
            cache_ttl: 3600,
            compression_enabled: true,
            encryption_enabled: false,
        }
    }
    
    pub fn storage_path(&self) -> String {
        self.storage_path.to_string_lossy().to_string()
    }
    
    pub fn with_compression(mut self, enabled: bool) -> Self {
        self.compression_enabled = enabled;
        self
    }
    
    pub fn with_encryption(mut self, enabled: bool) -> Self {
        self.encryption_enabled = enabled;
        self
    }
}

pub struct TestData;

impl TestData {
    pub fn sample_text() -> String {
        "This is sample text for testing purposes. It contains various characters and should be suitable for compression and encryption tests.".to_string()
    }
    
    pub fn json_data() -> String {
        serde_json::json!({
            "test": "data",
            "number": 42,
            "array": [1, 2, 3, 4, 5],
            "nested": {
                "key": "value",
                "another_key": "another_value"
            },
            "boolean": true,
            "null_value": null
        }).to_string()
    }
    
    pub fn binary_data() -> Vec<u8> {
        vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD, 0xFC, 0x80, 0x7F]
    }
    
    pub fn compressible_data() -> String {
        // Highly repetitive data that should compress well
        "AAAA".repeat(1000) + &"BBBB".repeat(500) + &"CCCC".repeat(250) + &"This is some text that repeats. ".repeat(100)
    }
    
    pub fn large_text(size: usize) -> String {
        let base_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. ";
        let repeat_count = (size / base_text.len()) + 1;
        let full_text = base_text.repeat(repeat_count);
        full_text.chars().take(size).collect()
    }
    
    pub fn random_bytes(size: usize) -> Vec<u8> {
        // Generate pseudo-random bytes using a simple algorithm
        let mut bytes = Vec::with_capacity(size);
        let mut seed = 12345u64;
        
        for _ in 0..size {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            bytes.push((seed >> 8) as u8);
        }
        
        bytes
    }
    
    pub fn xml_data() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<root>
    <item id="1">
        <name>Test Item 1</name>
        <value>100</value>
    </item>
    <item id="2">
        <name>Test Item 2</name>
        <value>200</value>
    </item>
</root>"#.to_string()
    }
}

pub struct TestFile {
    pub bucket: String,
    pub key: String,
    pub content: Vec<u8>,
    pub content_type: String,
}

impl TestFile {
    pub fn text_file(bucket: &str, key: &str, content: &str) -> Self {
        Self {
            bucket: bucket.to_string(),
            key: key.to_string(),
            content: content.as_bytes().to_vec(),
            content_type: "text/plain".to_string(),
        }
    }
    
    pub fn json_file(bucket: &str, key: &str, json: &Value) -> Self {
        Self {
            bucket: bucket.to_string(),
            key: key.to_string(),
            content: json.to_string().as_bytes().to_vec(),
            content_type: "application/json".to_string(),
        }
    }
    
    pub fn binary_file(bucket: &str, key: &str, data: Vec<u8>) -> Self {
        Self {
            bucket: bucket.to_string(),
            key: key.to_string(),
            content: data,
            content_type: "application/octet-stream".to_string(),
        }
    }
}

pub mod assertions {
    use serde_json::Value;
    
    pub fn assert_valid_file_response(response: &Value) {
        assert!(response["id"].is_string(), "Response should have string id");
        assert!(response["bucket"].is_string(), "Response should have string bucket");
        assert!(response["key"].is_string(), "Response should have string key");
        assert!(response["file_size"].is_number(), "Response should have numeric file_size");
        assert!(response["upload_time"].is_string(), "Response should have string upload_time");
    }
    
    pub fn assert_valid_hash(hash: &str) {
        assert!(!hash.is_empty(), "Hash should not be empty");
        assert!(hash.len() >= 32, "Hash should be at least 32 characters");
    }
    
    pub fn assert_compression_effective(original_size: usize, compressed_size: usize) {
        assert!(compressed_size < original_size, "Compressed size should be smaller than original");
        let ratio = compressed_size as f64 / original_size as f64;
        assert!(ratio < 1.0, "Compression ratio should be less than 1.0");
    }
}

pub fn create_test_file(bucket: &str, key: &str, size: i64) -> StoredFile {
    StoredFile {
        id: Uuid::new_v4(),
        bucket: bucket.to_string(),
        key: key.to_string(),
        filename: key.to_string(),
        file_path: format!("/tmp/{}/{}", bucket, key),
        file_size: size,
        original_size: size,
        content_type: "text/plain".to_string(),
        hash_blake3: "test_hash_blake3".to_string(),
        hash_md5: "test_hash_md5".to_string(),
        metadata: None,
        is_compressed: Some(false),
        is_encrypted: Some(false),
        compression_algorithm: None,
        encryption_algorithm: None,
        compression_ratio: None,
        upload_time: Some(Utc::now()),
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
    }
}

pub fn create_test_file_with_access_count(bucket: &str, key: &str, size: i64, access_count: i64) -> StoredFile {
    let mut file = create_test_file(bucket, key, size);
    file.access_count = access_count;
    file.cache_priority = Some((access_count / 10) as i32);
    file
} 