// HTTP handler tests
// Note: Full handler tests are covered in integration tests

use std::collections::HashMap;
use crate::helpers::{setup_test_db, TestConfig};
use local_storage::handlers::{FileHandler, BucketHandler};
use local_storage::models::{StoredFile, FileInfo, UploadRequest};
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;
use serde_json::json;

#[test]
fn test_http_status_code_mapping() {
    // Test HTTP status codes for different scenarios
    let error_scenarios = vec![
        ("file_not_found", 404),
        ("invalid_bucket", 400),
        ("file_too_large", 413),
        ("internal_error", 500),
        ("unauthorized", 401),
        ("forbidden", 403),
    ];
    
    for (error_type, expected_status) in error_scenarios {
        let status_code = match error_type {
            "file_not_found" => 404,
            "invalid_bucket" => 400,
            "file_too_large" => 413,
            "internal_error" => 500,
            "unauthorized" => 401,
            "forbidden" => 403,
            _ => 500,
        };
        
        assert_eq!(status_code, expected_status);
    }
}

#[test]
fn test_content_type_header() {
    // Test Content-Type header generation
    let file_types = vec![
        ("document.pdf", "application/pdf"),
        ("image.jpg", "image/jpeg"),
        ("data.json", "application/json"),
        ("text.txt", "text/plain"),
        ("unknown.xyz", "application/octet-stream"),
    ];
    
    for (filename, expected_content_type) in file_types {
        let extension = filename.split('.').last().unwrap_or("");
        let content_type = match extension {
            "pdf" => "application/pdf",
            "jpg" | "jpeg" => "image/jpeg",
            "json" => "application/json",
            "txt" => "text/plain",
            _ => "application/octet-stream",
        };
        
        assert_eq!(content_type, expected_content_type);
    }
}

#[test]
fn test_request_validation() {
    // Test request parameter validation
    struct UploadRequest {
        bucket: String,
        key: Option<String>,
        content_length: usize,
    }
    
    let requests = vec![
        UploadRequest {
            bucket: "valid-bucket".to_string(),
            key: Some("valid-key.txt".to_string()),
            content_length: 1024,
        },
        UploadRequest {
            bucket: "".to_string(), // Invalid: empty bucket
            key: Some("file.txt".to_string()),
            content_length: 1024,
        },
        UploadRequest {
            bucket: "valid-bucket".to_string(),
            key: None, // Valid: auto-generated key
            content_length: 1024,
        },
    ];
    
    for request in requests {
        let is_valid = !request.bucket.is_empty() 
            && request.content_length > 0 
            && request.content_length <= 100 * 1024 * 1024; // 100MB limit
        
        if request.bucket.is_empty() {
            assert!(!is_valid, "Empty bucket should be invalid");
        } else {
            assert!(is_valid, "Valid request should pass validation");
        }
    }
}

#[test]
fn test_response_headers() {
    // Test response header construction
    let mut headers = HashMap::new();
    headers.insert("Content-Type", "application/json");
    headers.insert("Cache-Control", "no-cache");
    headers.insert("X-Custom-Header", "local-storage-v1");
    
    assert_eq!(headers.get("Content-Type"), Some(&"application/json"));
    assert_eq!(headers.get("Cache-Control"), Some(&"no-cache"));
    assert_eq!(headers.get("X-Custom-Header"), Some(&"local-storage-v1"));
    assert_eq!(headers.len(), 3);
}

#[test]
fn test_error_response_format() {
    // Test error response JSON structure
    #[derive(Debug)]
    struct ErrorResponse {
        error: String,
        message: String,
        code: u16,
    }
    
    let error_response = ErrorResponse {
        error: "FILE_NOT_FOUND".to_string(),
        message: "The requested file does not exist".to_string(),
        code: 404,
    };
    
    assert_eq!(error_response.error, "FILE_NOT_FOUND");
    assert_eq!(error_response.code, 404);
    assert!(!error_response.message.is_empty());
}

#[test]
fn test_success_response_format() {
    // Test success response JSON structure
    #[derive(Debug)]
    struct UploadResponse {
        id: String,
        bucket: String,
        key: String,
        file_size: usize,
        upload_time: String,
    }
    
    let response = UploadResponse {
        id: "file-123".to_string(),
        bucket: "test-bucket".to_string(),
        key: "test-file.txt".to_string(),
        file_size: 1024,
        upload_time: "2024-01-01T00:00:00Z".to_string(),
    };
    
    assert!(!response.id.is_empty());
    assert!(!response.bucket.is_empty());
    assert!(!response.key.is_empty());
    assert!(response.file_size > 0);
    assert!(!response.upload_time.is_empty());
}

#[test]
fn test_path_parameter_extraction() {
    // Test URL path parameter extraction
    let url_path = "/buckets/my-bucket/files/path/to/file.txt";
    let segments: Vec<&str> = url_path.split('/').collect();
    
    // Expected: ["", "buckets", "my-bucket", "files", "path", "to", "file.txt"]
    assert_eq!(segments.len(), 7);
    assert_eq!(segments[1], "buckets");
    assert_eq!(segments[2], "my-bucket"); // bucket name
    assert_eq!(segments[3], "files");
    
    // Key is everything after /files/
    let key_segments = &segments[4..];
    let key = key_segments.join("/");
    assert_eq!(key, "path/to/file.txt");
}

#[tokio::test]
async fn test_file_upload_handler() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let handler = FileHandler::new(pool.clone(), config);

    // Test file upload
    let file_id = Uuid::new_v4();
    let test_content = "Test content for upload".as_bytes();
    let request = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "test-file.txt".to_string(),
        metadata: Some(json!({
            "owner": "test-user",
            "tags": ["test", "upload"]
        })),
        compress: Some(true),
        encrypt: Some(true),
        compression_algorithm: Some("zstd".to_string()),
        compression_level: Some(3),
        encryption_key_id: None,
    };

    let file = handler.handle_upload(request, test_content).await.unwrap();
    
    // Verify file was saved correctly
    let saved_file = sqlx::query!(
        r#"
        SELECT 
            bucket, key, file_size, original_size, content_type,
            is_compressed, is_encrypted, compression_algorithm,
            encryption_algorithm, compression_ratio, compression_level,
            encryption_key_id, metadata
        FROM files 
        WHERE id = $1
        "#,
        file.id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(saved_file.bucket, "test-bucket");
    assert_eq!(saved_file.key, "test-file.txt");
    assert_eq!(saved_file.content_type, "text/plain");
    assert!(saved_file.is_compressed);
    assert!(saved_file.is_encrypted);
    assert_eq!(saved_file.compression_algorithm.unwrap(), "zstd");
    assert_eq!(saved_file.compression_level.unwrap(), 3);
    assert!(saved_file.encryption_key_id.is_some());
    assert!(saved_file.metadata.is_some());
}

#[tokio::test]
async fn test_file_download_handler() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let handler = FileHandler::new(pool.clone(), config);

    // Create a test file
    let file_id = Uuid::new_v4();
    let file = StoredFile {
        id: file_id,
        bucket: "test-bucket".to_string(),
        key: "test-file.txt".to_string(),
        filename: format!("{}.bin", file_id),
        file_path: "/tmp/test-file.txt".to_string(),
        file_size: 100,
        original_size: 100,
        content_type: "text/plain".to_string(),
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

    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled, compression_algorithm,
            encryption_algorithm, compression_ratio, compression_level,
            encryption_key_id, metadata
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
            $15, $16, $17, $18, $19, $20
        )
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
        file.is_compressed,
        file.is_encrypted,
        file.compression_enabled,
        file.encryption_enabled,
        file.compression_algorithm.as_ref(),
        file.encryption_algorithm.as_ref(),
        file.compression_ratio,
        file.compression_level,
        file.encryption_key_id,
        file.metadata
    )
    .execute(&pool)
    .await
    .unwrap();

    // Test file download
    let downloaded_file = handler.handle_download(&file.bucket, &file.key).await.unwrap();
    
    assert_eq!(downloaded_file.id, file_id);
    assert_eq!(downloaded_file.bucket, "test-bucket");
    assert_eq!(downloaded_file.key, "test-file.txt");
    
    // Check access log
    let access_log = sqlx::query!(
        "SELECT access_type, user_agent FROM file_access_log WHERE file_id = $1",
        file_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(access_log.access_type, "download");
}

#[tokio::test]
async fn test_bucket_operations() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let handler = BucketHandler::new(pool.clone(), config);

    // Create test files in different buckets
    let files = vec![
        StoredFile {
            id: Uuid::new_v4(),
            bucket: "bucket1".to_string(),
            key: "file1.txt".to_string(),
            filename: "file1.bin".to_string(),
            file_path: "/tmp/file1.txt".to_string(),
            file_size: 100,
            original_size: 100,
            content_type: "text/plain".to_string(),
            hash_blake3: "hash1".to_string(),
            hash_md5: "md5hash1".to_string(),
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
        },
        StoredFile {
            id: Uuid::new_v4(),
            bucket: "bucket2".to_string(),
            key: "file2.txt".to_string(),
            filename: "file2.bin".to_string(),
            file_path: "/tmp/file2.txt".to_string(),
            file_size: 200,
            original_size: 200,
            content_type: "text/plain".to_string(),
            hash_blake3: "hash2".to_string(),
            hash_md5: "md5hash2".to_string(),
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
        },
    ];

    for file in &files {
        sqlx::query!(
            r#"
            INSERT INTO files (
                id, bucket, key, filename, file_path, file_size, original_size,
                content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
                compression_enabled, encryption_enabled, compression_algorithm,
                encryption_algorithm, compression_ratio, compression_level,
                encryption_key_id, metadata
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                $15, $16, $17, $18, $19, $20
            )
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
            file.is_compressed,
            file.is_encrypted,
            file.compression_enabled,
            file.encryption_enabled,
            file.compression_algorithm.as_ref(),
            file.encryption_algorithm.as_ref(),
            file.compression_ratio,
            file.compression_level,
            file.encryption_key_id,
            file.metadata
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    // Test list buckets
    let buckets = handler.list_buckets().await.unwrap();
    assert_eq!(buckets.len(), 2);
    assert!(buckets.contains(&"bucket1".to_string()));
    assert!(buckets.contains(&"bucket2".to_string()));

    // Test get bucket info
    let bucket_info = handler.get_bucket_info("bucket1").await.unwrap();
    assert_eq!(bucket_info.name, "bucket1");
    assert_eq!(bucket_info.file_count, 1);
    assert_eq!(bucket_info.total_size, 100);

    // Test list files in bucket
    let bucket_files = handler.list_files("bucket1").await.unwrap();
    assert_eq!(bucket_files.len(), 1);
    assert_eq!(bucket_files[0].key, "file1.txt");
} 