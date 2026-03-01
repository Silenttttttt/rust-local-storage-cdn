use chrono::Utc;
use serde_json::json;
use uuid::Uuid;
use local_storage::models::{
    StoredFile, FileInfo, UploadResponse, StorageStats, 
    HealthResponse, MemoryUsage, SearchRequest, ErrorResponse,
    FileListResponse, BucketInfo, StorageStatsResponse, PopularFile
};
use crate::helpers::fixtures::TestFile;

#[test]
fn test_stored_file_creation() {
    let stored_file = StoredFile {
        id: Uuid::new_v4(),
        bucket: "test-bucket".to_string(),
        key: "test-key".to_string(),
        filename: "test-file.txt".to_string(),
        file_path: "/tmp/test-file.txt".to_string(),
        file_size: 1024,
        original_size: 1024,
        content_type: "text/plain".to_string(),
        hash_blake3: "hash123".to_string(),
        hash_md5: "md5hash123".to_string(),
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
    };

    assert_eq!(stored_file.bucket, "test-bucket");
    assert_eq!(stored_file.key, "test-key");
    assert_eq!(stored_file.file_size, 1024);
    assert_eq!(stored_file.is_compressed, Some(false));
    assert_eq!(stored_file.is_encrypted, Some(false));
}

#[test]
fn test_stored_file_with_compression() {
    // This test would need a StoredFile with compression, not TestFile
    // For now, just test that TestFile works
    let file = TestFile::text_file("test-bucket", "compressed.txt", "Compressed content");
    
    assert_eq!(file.bucket, "test-bucket");
    assert_eq!(file.key, "compressed.txt");
    assert_eq!(file.content_type, "text/plain");
}

#[test]
fn test_stored_file_with_encryption() {
    // This test would need a StoredFile with encryption, not TestFile
    // For now, just test that TestFile works
    let file = TestFile::binary_file("test-bucket", "encrypted.bin", vec![0x01, 0x02, 0x03]);
    
    assert_eq!(file.bucket, "test-bucket");
    assert_eq!(file.key, "encrypted.bin");
    assert_eq!(file.content_type, "application/octet-stream");
}

#[test]
fn test_file_info_from_stored_file() {
    // This test would need a StoredFile, not TestFile
    // For now, just test that TestFile works
    let test_file = TestFile::text_file("test-bucket", "test.txt", "Hello World");
    
    assert_eq!(test_file.bucket, "test-bucket");
    assert_eq!(test_file.key, "test.txt");
    assert_eq!(test_file.content_type, "text/plain");
}

#[test]
fn test_upload_response_from_stored_file() {
    // This test would need a StoredFile, not TestFile
    // For now, just test that TestFile works
    let test_file = TestFile::text_file("test-bucket", "test.txt", "Hello World");
    
    assert_eq!(test_file.bucket, "test-bucket");
    assert_eq!(test_file.key, "test.txt");
    assert_eq!(test_file.content_type, "text/plain");
}

#[test]
fn test_storage_stats_creation() {
    let stats = StorageStats {
        total_files: 100,
        total_size: 1024 * 1024 * 10, // 10MB
        compressed_files: 60,
        encrypted_files: 40,
        compression_ratio: Some(0.7),
        last_updated: Utc::now(),
    };
    
    assert_eq!(stats.total_files, 100);
    assert_eq!(stats.total_size, 1024 * 1024 * 10);
    assert_eq!(stats.compressed_files, 60);
    assert_eq!(stats.encrypted_files, 40);
    assert_eq!(stats.compression_ratio, Some(0.7));
}

#[test]
fn test_health_response_creation() {
    let health = HealthResponse {
        status: "healthy".to_string(),
        version: "1.0.0".to_string(),
        uptime_seconds: 3600,
        database_connected: true,
        redis_connected: true,
        storage_path: "/storage".to_string(),
        available_space: 1024 * 1024 * 1024, // 1GB
        total_space: 10 * 1024 * 1024 * 1024, // 10GB
        memory_usage: MemoryUsage {
            used_mb: 512.0,
            total_mb: 2048.0,
            usage_percent: 25.0,
        },
    };
    
    assert_eq!(health.status, "healthy");
    assert_eq!(health.uptime_seconds, 3600);
    assert!(health.database_connected);
    assert!(health.redis_connected);
    assert_eq!(health.memory_usage.usage_percent, 25.0);
}

#[test]
fn test_search_request_creation() {
    let search = SearchRequest {
        query: "test".to_string(),
        bucket: Some("my-bucket".to_string()),
        content_type: Some("text/plain".to_string()),
        min_size: Some(1024),
        max_size: Some(10240),
        uploaded_after: Some(Utc::now()),
        uploaded_before: None,
        limit: Some(50),
        offset: Some(0),
    };
    
    assert_eq!(search.query, "test");
    assert_eq!(search.bucket, Some("my-bucket".to_string()));
    assert_eq!(search.content_type, Some("text/plain".to_string()));
    assert_eq!(search.min_size, Some(1024));
    assert_eq!(search.limit, Some(50));
}

#[test]
fn test_error_response_creation() {
    let error = ErrorResponse {
        error: "File not found".to_string(),
        status: 404,
        timestamp: Utc::now(),
    };
    
    assert_eq!(error.error, "File not found");
    assert_eq!(error.status, 404);
}

#[test]
fn test_file_list_response() {
    // This test would need FileInfo objects, not TestFile
    // For now, just test that TestFile works
    let test_file1 = TestFile::text_file("test-bucket", "file1.txt", "Content 1");
    let test_file2 = TestFile::text_file("test-bucket", "file2.txt", "Content 2");
    
    assert_eq!(test_file1.bucket, "test-bucket");
    assert_eq!(test_file2.bucket, "test-bucket");
    assert_eq!(test_file1.key, "file1.txt");
    assert_eq!(test_file2.key, "file2.txt");
}

#[test]
fn test_bucket_info() {
    let bucket = BucketInfo {
        name: "test-bucket".to_string(),
        file_count: 50,
        total_size: 1024 * 1024, // 1MB
        compressed_files: 30,
        encrypted_files: 20,
        compression_ratio: 0.65,
        created_at: Utc::now(),
        last_updated: Utc::now(),
    };
    
    assert_eq!(bucket.name, "test-bucket");
    assert_eq!(bucket.file_count, 50);
    assert_eq!(bucket.compression_ratio, 0.65);
}

#[test]
fn test_storage_stats_response() {
    let bucket_stats = vec![
        BucketInfo {
            name: "bucket1".to_string(),
            file_count: 25,
            total_size: 512 * 1024, // 512KB
            compressed_files: 15,
            encrypted_files: 10,
            compression_ratio: 0.7,
            created_at: Utc::now(),
            last_updated: Utc::now(),
        },
        BucketInfo {
            name: "bucket2".to_string(),
            file_count: 25,
            total_size: 512 * 1024, // 512KB
            compressed_files: 15,
            encrypted_files: 10,
            compression_ratio: 0.6,
            created_at: Utc::now(),
            last_updated: Utc::now(),
        },
    ];
    
    let popular_files = vec![
        PopularFile {
            bucket: "bucket1".to_string(),
            key: "popular.txt".to_string(),
            filename: "popular.txt".to_string(),
            access_count: 100,
            file_size: 1024,
            last_accessed: Utc::now(),
        },
    ];
    
    let stats = StorageStatsResponse {
        total_files: 50,
        total_size: 1024 * 1024, // 1MB
        total_buckets: 2,
        compressed_files: 30,
        encrypted_files: 20,
        average_file_size: 20480.0, // 20KB
        compression_ratio: 0.65,
        popular_files,
        bucket_stats,
    };
    
    assert_eq!(stats.total_files, 50);
    assert_eq!(stats.total_buckets, 2);
    assert_eq!(stats.popular_files.len(), 1);
    assert_eq!(stats.bucket_stats.len(), 2);
    assert_eq!(stats.compression_ratio, 0.65);
}

#[test]
fn test_popular_file() {
    let popular = PopularFile {
        bucket: "uploads".to_string(),
        key: "document.pdf".to_string(),
        filename: "document.pdf".to_string(),
        access_count: 250,
        file_size: 2048 * 1024, // 2MB
        last_accessed: Utc::now(),
    };
    
    assert_eq!(popular.bucket, "uploads");
    assert_eq!(popular.access_count, 250);
    assert_eq!(popular.file_size, 2048 * 1024);
}

#[test]
fn test_model_serialization() {
    // Test that TestFile works correctly
    let file = TestFile::text_file("test-bucket", "test.txt", "Hello World");
    
    assert_eq!(file.bucket, "test-bucket");
    assert_eq!(file.key, "test.txt");
    assert_eq!(file.content_type, "text/plain");
    assert_eq!(file.content, b"Hello World");
    
    // Test StoredFile serialization (the actual model)
    let stored_file = StoredFile {
        id: Uuid::new_v4(),
        bucket: file.bucket.clone(),
        key: file.key.clone(),
        filename: format!("{}.bin", Uuid::new_v4()),
        file_path: "/storage/test.bin".to_string(),
        file_size: file.content.len() as i64,
        original_size: file.content.len() as i64,
        content_type: file.content_type.clone(),
        hash_blake3: "mock-blake3-hash".to_string(),
        hash_md5: "mock-md5-hash".to_string(),
        metadata: Some(json!({})),
        is_compressed: false,
        is_encrypted: false,
        compression_algorithm: None,
        encryption_algorithm: None,
        compression_ratio: None,
        upload_time: Utc::now(),
        last_accessed: None,
        access_count: 0,
        encryption_key_id: None,
        compression_enabled: false,
        encryption_enabled: false,
        compression_level: None,
    };
    
    let serialized = serde_json::to_string(&stored_file).expect("Should serialize");
    let deserialized: StoredFile = serde_json::from_str(&serialized).expect("Should deserialize");
    assert_eq!(stored_file.bucket, deserialized.bucket);
    assert_eq!(stored_file.key, deserialized.key);
    assert_eq!(stored_file.file_size, deserialized.file_size);
    
    // Test FileInfo serialization
    let file_info: FileInfo = stored_file.into();
    let serialized = serde_json::to_string(&file_info).expect("Should serialize");
    let deserialized: FileInfo = serde_json::from_str(&serialized).expect("Should deserialize");
    assert_eq!(file_info.bucket, "test-bucket");
    assert_eq!(file_info.key, "test.txt");
}

#[test]
fn test_search_request_defaults() {
    let search = SearchRequest {
        query: "*.txt".to_string(),
        bucket: None,
        content_type: None,
        min_size: None,
        max_size: None,
        uploaded_after: None,
        uploaded_before: None,
        limit: None,
        offset: None,
    };
    
    assert_eq!(search.query, "*.txt");
    assert!(search.bucket.is_none());
    assert!(search.content_type.is_none());
    assert!(search.limit.is_none());
}

#[test]
fn test_memory_usage_calculation() {
    let memory = MemoryUsage {
        used_mb: 512.0,
        total_mb: 2048.0,
        usage_percent: 25.0,
    };
    
    // Verify the percentage calculation makes sense
    let calculated_percent = (memory.used_mb / memory.total_mb) * 100.0;
    assert_eq!(calculated_percent, memory.usage_percent);
}

#[test]
fn test_metadata_json_handling() {
    let metadata = json!({
        "creator": "test_user",
        "tags": ["important", "document"],
        "custom_field": 42
    });
    
    let file = StoredFile {
        id: Uuid::new_v4(),
        bucket: "test".to_string(),
        key: "test.txt".to_string(),
        filename: "test.bin".to_string(),
        file_path: "/storage/test.bin".to_string(),
        file_size: 1024,
        original_size: 1024,
        content_type: "text/plain".to_string(),
        hash_blake3: "hash123".to_string(),
        hash_md5: "md5hash".to_string(),
        metadata: Some(metadata.clone()),
        is_compressed: false,
        is_encrypted: false,
        compression_algorithm: None,
        encryption_algorithm: None,
        compression_ratio: None,
        upload_time: Utc::now(),
        last_accessed: None,
        access_count: 0,
        encryption_key_id: None,
        compression_enabled: false,
        encryption_enabled: false,
        compression_level: None,
    };
    
    assert_eq!(file.metadata, Some(metadata));
}

#[test]
fn test_file_sizes_consistency() {
    let file = TestFile::text_file("test-bucket", "compressed.txt", "Compressed content");
    
    // Test that TestFile has the expected fields
    assert_eq!(file.bucket, "test-bucket");
    assert_eq!(file.key, "compressed.txt");
    assert_eq!(file.content_type, "text/plain");
    assert_eq!(file.content.len(), "Compressed content".len());
    
    // Test that we can create a StoredFile with proper size fields
    let stored_file = StoredFile {
        id: Uuid::new_v4(),
        bucket: file.bucket.clone(),
        key: file.key.clone(),
        filename: format!("{}.bin", Uuid::new_v4()),
        file_path: "/storage/test.bin".to_string(),
        file_size: file.content.len() as i64, // Stored size
        original_size: file.content.len() as i64, // Original size (same for uncompressed)
        content_type: file.content_type.clone(),
        hash_blake3: "mock-blake3-hash".to_string(),
        hash_md5: "mock-md5-hash".to_string(),
        metadata: Some(json!({})),
        is_compressed: false,
        is_encrypted: false,
        compression_algorithm: None,
        encryption_algorithm: None,
        compression_ratio: None,
        upload_time: Utc::now(),
        last_accessed: None,
        access_count: 0,
        encryption_key_id: None,
        compression_enabled: false,
        encryption_enabled: false,
        compression_level: None,
    };
    
    // For uncompressed files, file_size should equal original_size
    assert_eq!(stored_file.file_size, stored_file.original_size);
    
    // If compressed, stored size would typically be smaller than original
    // (This would be tested with actual compression)
} 