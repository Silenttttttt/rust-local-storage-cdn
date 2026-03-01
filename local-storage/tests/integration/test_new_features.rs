use crate::app::create_app;
use crate::config::Config;
use crate::encryption_keys::EncryptionKeyManager;
use crate::models::{UploadRequest, UploadResponse};
use crate::tests::helpers::{setup_test_app, setup_test_db, TestConfig, TestServer};
use axum::http::{Method, StatusCode};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn test_per_file_compression_flags() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool, config).await;
    let server = TestServer::new(app).unwrap();

    // Test upload with compression enabled
    let upload_data = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "compressed-file.txt".to_string(),
        metadata: None,
        compress: Some(true),
        encrypt: Some(false),
        compression_algorithm: Some("gzip".to_string()),
        compression_level: Some(6),
        encryption_key_id: None,
    };

    let response = server
        .post("/upload")
        .json(&upload_data)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let upload_response: UploadResponse = response.json();
    assert!(upload_response.compression_enabled);
    assert!(!upload_response.encryption_enabled);
    assert!(upload_response.is_compressed);

    // Test upload with compression disabled
    let upload_data = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "uncompressed-file.txt".to_string(),
        metadata: None,
        compress: Some(false),
        encrypt: Some(false),
        compression_algorithm: None,
        compression_level: None,
        encryption_key_id: None,
    };

    let response = server
        .post("/upload")
        .json(&upload_data)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let upload_response: UploadResponse = response.json();
    assert!(!upload_response.compression_enabled);
    assert!(!upload_response.encryption_enabled);
    assert!(!upload_response.is_compressed);
}

#[tokio::test]
async fn test_per_file_encryption_flags() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Create an encryption key first
    let key_manager = EncryptionKeyManager::new(pool.clone(), config);
    let key_id = "test-encryption-key";
    let key_data = vec![1u8; 32];
    key_manager
        .create_key(key_id, &key_data, "aes-gcm", Some("Test key"))
        .await
        .unwrap();

    // Test upload with encryption enabled
    let upload_data = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "encrypted-file.txt".to_string(),
        metadata: None,
        compress: Some(false),
        encrypt: Some(true),
        compression_algorithm: None,
        compression_level: None,
        encryption_key_id: Some(key_id.to_string()),
    };

    let response = server
        .post("/upload")
        .json(&upload_data)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let upload_response: UploadResponse = response.json();
    assert!(!upload_response.compression_enabled);
    assert!(upload_response.encryption_enabled);
    assert!(upload_response.is_encrypted);
    assert_eq!(upload_response.encryption_key_id, Some(key_id.to_string()));

    // Test upload with encryption disabled
    let upload_data = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "unencrypted-file.txt".to_string(),
        metadata: None,
        compress: Some(false),
        encrypt: Some(false),
        compression_algorithm: None,
        compression_level: None,
        encryption_key_id: None,
    };

    let response = server
        .post("/upload")
        .json(&upload_data)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let upload_response: UploadResponse = response.json();
    assert!(!upload_response.compression_enabled);
    assert!(!upload_response.encryption_enabled);
    assert!(!upload_response.is_encrypted);
    assert_eq!(upload_response.encryption_key_id, None);
}

#[tokio::test]
async fn test_compression_and_encryption_combined() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Create an encryption key
    let key_manager = EncryptionKeyManager::new(pool.clone(), config);
    let key_id = "test-combined-key";
    let key_data = vec![2u8; 32];
    key_manager
        .create_key(key_id, &key_data, "aes-gcm", Some("Test combined key"))
        .await
        .unwrap();

    // Test upload with both compression and encryption enabled
    let upload_data = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "compressed-encrypted-file.txt".to_string(),
        metadata: None,
        compress: Some(true),
        encrypt: Some(true),
        compression_algorithm: Some("gzip".to_string()),
        compression_level: Some(9),
        encryption_key_id: Some(key_id.to_string()),
    };

    let response = server
        .post("/upload")
        .json(&upload_data)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let upload_response: UploadResponse = response.json();
    assert!(upload_response.compression_enabled);
    assert!(upload_response.encryption_enabled);
    assert!(upload_response.is_compressed);
    assert!(upload_response.is_encrypted);
    assert_eq!(upload_response.encryption_key_id, Some(key_id.to_string()));
}

#[tokio::test]
async fn test_encryption_key_management_endpoints() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Test creating an encryption key
    let key_data = vec![3u8; 32];
    let create_key_data = json!({
        "key_id": "test-api-key",
        "key_data": base64::encode(&key_data),
        "algorithm": "aes-gcm",
        "description": "Test API key"
    });

    let response = server
        .post("/encryption-keys")
        .json(&create_key_data)
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let key_response = response.json::<serde_json::Value>();
    assert_eq!(key_response["key_id"], "test-api-key");
    assert_eq!(key_response["algorithm"], "aes-gcm");
    assert_eq!(key_response["description"], "Test API key");
    assert_eq!(key_response["is_active"], true);

    // Test listing encryption keys
    let response = server
        .get("/encryption-keys")
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let keys_response = response.json::<serde_json::Value>();
    assert!(keys_response["keys"].as_array().unwrap().len() > 0);

    // Test getting a specific encryption key
    let response = server
        .get(&format!("/encryption-keys/{}", "test-api-key"))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let key_response = response.json::<serde_json::Value>();
    assert_eq!(key_response["key_id"], "test-api-key");

    // Test deactivating an encryption key
    let response = server
        .delete(&format!("/encryption-keys/{}", "test-api-key"))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    // Verify the key is deactivated
    let response = server
        .get(&format!("/encryption-keys/{}", "test-api-key"))
        .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_file_encryption_key_association() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Create an encryption key
    let key_manager = EncryptionKeyManager::new(pool.clone(), config);
    let key_id = "test-file-association-key";
    let key_data = vec![4u8; 32];
    key_manager
        .create_key(key_id, &key_data, "aes-gcm", Some("Test file association key"))
        .await
        .unwrap();

    // Upload a file with the encryption key
    let upload_data = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "associated-file.txt".to_string(),
        metadata: None,
        compress: Some(false),
        encrypt: Some(true),
        compression_algorithm: None,
        compression_level: None,
        encryption_key_id: Some(key_id.to_string()),
    };

    let response = server
        .post("/upload")
        .json(&upload_data)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let upload_response: UploadResponse = response.json();
    let file_id = upload_response.id;

    // Test getting files associated with the encryption key
    let response = server
        .get(&format!("/encryption-keys/{}/files", key_id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let files_response = response.json::<serde_json::Value>();
    let files = files_response["files"].as_array().unwrap();
    let files = files.iter().map(|f| f["id"].as_str().unwrap().to_string()).collect::<Vec<String>>();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0], file_id.to_string());

    // Test updating file's encryption key
    let new_key_id = "new-file-key";
    let new_key_data = vec![5u8; 32];
    key_manager
        .create_key(new_key_id, &new_key_data, "aes-gcm", Some("New file key"))
        .await
        .unwrap();

    let response = server
        .put(&format!("/files/{}/encryption-key", file_id))
        .json(&serde_json::json!({
            "encryption_key_id": new_key_id
        }))
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    // Verify file uses new key
    let file = sqlx::query!(
        r#"
        SELECT encryption_key_id, is_encrypted, encryption_enabled
        FROM files
        WHERE id = $1
        "#,
        file_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(file.encryption_key_id, Some(new_key_id.to_string()));
    assert!(file.is_encrypted);
    assert!(file.encryption_enabled);
}

#[tokio::test]
async fn test_cache_management_endpoints() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Test getting cache configuration
    let response = server
        .get("/cache/config")
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let config_response = response.json::<serde_json::Value>();
    assert!(config_response["max_cache_size_gb"].as_f64().unwrap() > 0.0);
    assert!(config_response["cache_ttl_seconds"].as_i64().unwrap() > 0);

    // Test updating cache configuration
    let update_data = json!({
        "max_cache_size_gb": 2.5,
        "cache_ttl_seconds": 1800,
        "preload_enabled": true
    });

    let response = server
        .put("/cache/config")
        .json(&update_data)
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    // Verify the update
    let response = server
        .get("/cache/config")
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let config_response = response.json::<serde_json::Value>();
    assert_eq!(config_response["max_cache_size_gb"], 2.5);
    assert_eq!(config_response["cache_ttl_seconds"], 1800);
    assert_eq!(config_response["preload_enabled"], true);

    // Test getting cache statistics
    let response = server
        .get("/cache/stats")
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let stats_response = response.json::<serde_json::Value>();
    assert!(stats_response["max_size_gb"].as_f64().unwrap() > 0.0);
    assert!(stats_response["ttl_seconds"].as_u64().unwrap() > 0);

    // Test clearing cache
    let response = server
        .delete("/cache")
        .await;

    // This might fail if Redis is not running, but that's expected
    if response.status() != StatusCode::OK {
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}

#[tokio::test]
async fn test_file_access_logging() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Create a test file
    let file_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled,
            compression_algorithm, encryption_algorithm, compression_ratio,
            compression_level, encryption_key_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
            $15, $16, $17, $18, $19
        )"#,
        file_id,
        "test-bucket",
        "test-file.txt",
        "test-file.txt",
        "/tmp/test-file.txt",
        100i64,
        100i64,
        "text/plain",
        "hash1",
        "md5hash1",
        false,
        false,
        false,
        false,
        None::<String>,
        None::<String>,
        None::<f32>,
        None::<i32>,
        None::<String>
    )
    .execute(&pool)
    .await
    .unwrap();

    // Download the file to trigger access logging
    let response = server
        .get(&format!("/files/{}/download", file_id))
        .header("User-Agent", "test-agent")
        .header("X-Forwarded-For", "127.0.0.1")
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    // Verify access log entry
    let log_entry = sqlx::query!(
        "SELECT file_id, access_type, user_agent, ip_address FROM file_access_log WHERE file_id = $1",
        file_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(log_entry.access_type, "download");
    assert_eq!(log_entry.user_agent, Some("test-agent".to_string()));
    assert_eq!(log_entry.ip_address, Some("127.0.0.1".to_string()));
}

#[tokio::test]
async fn test_popular_files_endpoint() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Create files with different access counts
    let file_id1 = Uuid::new_v4();
    let file_id2 = Uuid::new_v4();
    let file_id3 = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled, access_count,
            compression_algorithm, encryption_algorithm, compression_ratio,
            compression_level, encryption_key_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19, $20
        )"#,
        file_id1,
        "test-bucket",
        "popular-file1.txt",
        "popular-file1.txt",
        "/tmp/popular-file1.txt",
        100i64,
        100i64,
        "text/plain",
        "hash1",
        "md5hash1",
        false,
        false,
        false,
        false,
        15i64, // High access count
        None::<String>,
        None::<String>,
        None::<f32>,
        None::<i32>,
        None::<String>
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled, access_count,
            compression_algorithm, encryption_algorithm, compression_ratio,
            compression_level, encryption_key_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19, $20
        )"#,
        file_id2,
        "test-bucket",
        "popular-file2.txt",
        "popular-file2.txt",
        "/tmp/popular-file2.txt",
        200i64,
        200i64,
        "text/plain",
        "hash2",
        "md5hash2",
        false,
        false,
        false,
        false,
        8i64, // Medium access count
        None::<String>,
        None::<String>,
        None::<f32>,
        None::<i32>,
        None::<String>
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled, access_count,
            compression_algorithm, encryption_algorithm, compression_ratio,
            compression_level, encryption_key_id
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19, $20
        )"#,
        file_id3,
        "test-bucket",
        "popular-file3.txt",
        "popular-file3.txt",
        "/tmp/popular-file3.txt",
        300i64,
        300i64,
        "text/plain",
        "hash3",
        "md5hash3",
        false,
        false,
        false,
        false,
        3i64, // Low access count
        None::<String>,
        None::<String>,
        None::<f32>,
        None::<i32>,
        None::<String>
    )
    .execute(&pool)
    .await
    .unwrap();

    // Test getting popular files
    let response = server
        .get("/cache/popular-files?limit=2")
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let popular_response = response.json::<serde_json::Value>();
    let files = popular_response["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);

    // Files should be ordered by access count (descending)
    assert_eq!(files[0]["access_count"], 15);
    assert_eq!(files[1]["access_count"], 8);
}

#[tokio::test]
async fn test_preload_cache_endpoint() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Test preloading cache
    let response = server
        .post("/cache/preload")
        .await;

    // This might fail if Redis is not running, but that's expected
    if response.status() != StatusCode::OK {
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}

#[tokio::test]
async fn test_invalid_encryption_key_upload() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Test upload with non-existent encryption key
    let upload_data = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "invalid-key-file.txt".to_string(),
        metadata: None,
        compress: Some(false),
        encrypt: Some(true),
        compression_algorithm: None,
        compression_level: None,
        encryption_key_id: Some("non-existent-key".to_string()),
    };

    let response = server
        .post("/upload")
        .json(&upload_data)
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error_response = response.json::<serde_json::Value>();
    assert!(error_response["error"].as_str().unwrap().contains("encryption key"));
}

#[tokio::test]
async fn test_invalid_compression_algorithm() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Test upload with invalid compression algorithm
    let upload_data = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "invalid-compression-file.txt".to_string(),
        metadata: None,
        compress: Some(true),
        encrypt: Some(false),
        compression_algorithm: Some("invalid-algorithm".to_string()),
        compression_level: Some(6),
        encryption_key_id: None,
    };

    let response = server
        .post("/upload")
        .json(&upload_data)
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error_response = response.json::<serde_json::Value>();
    assert!(error_response["error"].as_str().unwrap().contains("compression algorithm"));
}

#[tokio::test]
async fn test_compression_level_validation() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Test upload with invalid compression level (too high)
    let upload_data = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "invalid-level-file.txt".to_string(),
        metadata: None,
        compress: Some(true),
        encrypt: Some(false),
        compression_algorithm: Some("gzip".to_string()),
        compression_level: Some(15), // Invalid level
        encryption_key_id: None,
    };

    let response = server
        .post("/upload")
        .json(&upload_data)
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error_response = response.json::<serde_json::Value>();
    assert!(error_response["error"].as_str().unwrap().contains("compression level"));
}

#[tokio::test]
async fn test_cache_configuration_endpoints() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Test getting cache configuration
    let response = server
        .get("/cache/config")
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let config_response = response.json::<serde_json::Value>();
    assert_eq!(config_response["max_cache_size_gb"], 1.0);
    assert_eq!(config_response["cache_ttl_seconds"], 3600);
    assert_eq!(config_response["min_access_count"], 5);
    assert_eq!(config_response["auto_cache_threshold"], 10);

    // Test updating cache configuration
    let update_data = json!({
        "max_cache_size_gb": 2.0,
        "cache_ttl_seconds": 7200,
        "min_access_count": 10,
        "cache_priority_weights": {
            "access_count": 2.0,
            "file_size": -1.0,
            "last_access": 1.5
        },
        "auto_cache_threshold": 20
    });

    let response = server
        .put("/cache/config")
        .json(&update_data)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    
    // Verify the update
    let response = server
        .get("/cache/config")
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let config_response = response.json::<serde_json::Value>();
    assert_eq!(config_response["max_cache_size_gb"], 2.0);
    assert_eq!(config_response["cache_ttl_seconds"], 7200);
    assert_eq!(config_response["min_access_count"], 10);
    assert_eq!(config_response["auto_cache_threshold"], 20);
    assert_eq!(config_response["cache_priority_weights"]["access_count"], 2.0);
}

#[tokio::test]
async fn test_file_cache_status() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Upload a file
    let content = b"Test content for caching".to_vec();
    let upload_data = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "cache-test.txt".to_string(),
        metadata: None,
        compress: Some(false),
        encrypt: Some(false),
        compression_algorithm: None,
        compression_level: None,
        encryption_key_id: None,
    };

    let response = server
        .post("/upload")
        .json(&upload_data)
        .with_body(content.clone())
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let upload_response: UploadResponse = response.json();

    // Check initial cache status
    let response = server
        .get(&format!("/files/{}/cache-status", upload_response.id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let status = response.json::<serde_json::Value>();
    assert_eq!(status["cache_status"], "not_cached");
    assert_eq!(status["cache_hits"], 0);

    // Access the file multiple times to trigger caching
    for _ in 0..6 {
        let response = server
            .get(&format!("/files/{}/download", upload_response.id))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    // Check updated cache status
    let response = server
        .get(&format!("/files/{}/cache-status", upload_response.id))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let status = response.json::<serde_json::Value>();
    assert_eq!(status["cache_status"], "cached");
    assert!(status["cache_hits"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn test_cache_priority_update() {
    let (pool, temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let app = create_app(pool.clone(), config.clone()).await;
    let server = TestServer::new(app).unwrap();

    // Upload two files with different sizes
    let small_content = b"Small file".to_vec();
    let large_content = b"Large file content that is much bigger than the small file".repeat(100).as_bytes().to_vec();

    // Upload small file
    let small_file = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "small.txt".to_string(),
        metadata: None,
        compress: Some(false),
        encrypt: Some(false),
        compression_algorithm: None,
        compression_level: None,
        encryption_key_id: None,
    };

    let response = server
        .post("/upload")
        .json(&small_file)
        .with_body(small_content)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let small_file_id = response.json::<UploadResponse>().id;

    // Upload large file
    let large_file = UploadRequest {
        bucket: "test-bucket".to_string(),
        key: "large.txt".to_string(),
        metadata: None,
        compress: Some(false),
        encrypt: Some(false),
        compression_algorithm: None,
        compression_level: None,
        encryption_key_id: None,
    };

    let response = server
        .post("/upload")
        .json(&large_file)
        .with_body(large_content)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let large_file_id = response.json::<UploadResponse>().id;

    // Access both files multiple times
    for _ in 0..10 {
        let response = server
            .get(&format!("/files/{}/download", small_file_id))
            .await;
        assert_eq!(response.status(), StatusCode::OK);

        let response = server
            .get(&format!("/files/{}/download", large_file_id))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    // Trigger cache priority update
    let response = server
        .post("/cache/update-priorities")
        .await;

    assert_eq!(response.status(), StatusCode::OK);

    // Check cache priorities
    let response = server
        .get(&format!("/files/{}/cache-status", small_file_id))
        .await;
    let small_status = response.json::<serde_json::Value>();

    let response = server
        .get(&format!("/files/{}/cache-status", large_file_id))
        .await;
    let large_status = response.json::<serde_json::Value>();

    // Small file should have higher priority due to size despite same access count
    assert!(
        small_status["cache_priority"].as_f64().unwrap() >
        large_status["cache_priority"].as_f64().unwrap()
    );
} 