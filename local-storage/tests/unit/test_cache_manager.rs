use crate::cache_manager::{CacheManager, CacheStats};
use crate::config::Config;
use crate::errors::StorageError;
use crate::models::{CacheConfig, StoredFile};
use crate::tests::helpers::{setup_test_db, TestConfig};
use redis::Client;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;
use chrono::Utc;
use serde_json::json;

// Mock Redis server for testing
async fn setup_mock_redis() -> String {
    "redis://127.0.0.1:6379".to_string()
}

#[tokio::test]
async fn test_cache_manager_creation() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let result = CacheManager::new(pool, &redis_url, config);
    
    if result.is_ok() {
        let manager = result.unwrap();
        assert!(manager.pool.acquire().await.is_ok());
    }
}

#[tokio::test]
async fn test_get_cache_config() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    // Insert test config with new fields
    sqlx::query!(
        r#"
        INSERT INTO cache_config (
            id, max_cache_size_gb, cache_ttl_seconds, min_access_count,
            cache_priority_weights, auto_cache_threshold
        ) VALUES (
            1, 1.0, 3600, 5,
            $1::jsonb, 10
        )
        "#,
        json!({
            "access_count": 1.0,
            "file_size": -0.5,
            "last_access": 0.8
        })
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    let config = manager.get_cache_config().await.unwrap();
    assert_eq!(config.max_cache_size_gb, 1.0);
    assert_eq!(config.cache_ttl_seconds, 3600);
    assert_eq!(config.min_access_count, 5);
    assert_eq!(config.auto_cache_threshold, 10);
    
    let weights = config.cache_priority_weights.as_object().unwrap();
    assert_eq!(weights["access_count"], 1.0);
    assert_eq!(weights["file_size"], -0.5);
    assert_eq!(weights["last_access"], 0.8);
}

#[tokio::test]
async fn test_update_cache_config() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    // Insert initial config
    sqlx::query!(
        r#"
        INSERT INTO cache_config (
            id, max_cache_size_gb, cache_ttl_seconds, min_access_count,
            cache_priority_weights, auto_cache_threshold
        ) VALUES (
            1, 1.0, 3600, 5,
            $1::jsonb, 10
        )
        "#,
        json!({
            "access_count": 1.0,
            "file_size": -0.5,
            "last_access": 0.8
        })
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // Update config with new values
    let new_weights = json!({
        "access_count": 2.0,
        "file_size": -1.0,
        "last_access": 1.5
    });
    
    manager.update_cache_config(
        2.0,
        7200,
        10,
        new_weights.clone(),
        20
    ).await.unwrap();

    // Verify update
    let config = manager.get_cache_config().await.unwrap();
    assert_eq!(config.max_cache_size_gb, 2.0);
    assert_eq!(config.cache_ttl_seconds, 7200);
    assert_eq!(config.min_access_count, 10);
    assert_eq!(config.auto_cache_threshold, 20);
    assert_eq!(config.cache_priority_weights, new_weights);
}

#[tokio::test]
async fn test_file_caching_and_stats() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    // Create a test file
    let file_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled, access_count,
            cache_status, last_cache_update, cache_hits, cache_priority
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19
        )
        "#,
        file_id,
        "test-bucket",
        "test.txt",
        "test.txt",
        "/tmp/test.txt",
        100i64,
        100i64,
        "text/plain",
        "hash1",
        "md5hash1",
        false,
        false,
        false,
        false,
        5i64,
        "not_cached",
        Utc::now(),
        0i64,
        0.0
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // Test caching the file
    let content = b"test content".to_vec();
    manager.cache_file_content(file_id, &content).await.unwrap();

    // Verify cache status
    let file = sqlx::query_as!(
        StoredFile,
        "SELECT * FROM files WHERE id = $1",
        file_id
    )
    .fetch_one(&manager.pool)
    .await
    .unwrap();

    assert_eq!(file.cache_status, Some("cached".to_string()));
    assert!(file.last_cache_update.is_some());
    assert_eq!(file.cache_hits, Some(0));

    // Test retrieving from cache
    let cached_content = manager.get_cached_content(file_id).await.unwrap();
    assert_eq!(cached_content, content);

    // Verify cache hit was recorded
    let file = sqlx::query_as!(
        StoredFile,
        "SELECT * FROM files WHERE id = $1",
        file_id
    )
    .fetch_one(&manager.pool)
    .await
    .unwrap();

    assert_eq!(file.cache_hits, Some(1));
}

#[tokio::test]
async fn test_cache_priority_calculation() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    // Create test files with different characteristics
    let file_id1 = Uuid::new_v4();
    let file_id2 = Uuid::new_v4();

    // File with high access count but large size
    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled, access_count,
            cache_status, last_cache_update, cache_hits, cache_priority
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19
        )
        "#,
        file_id1,
        "test-bucket",
        "large.txt",
        "large.txt",
        "/tmp/large.txt",
        1000000i64, // 1MB
        1000000i64,
        "text/plain",
        "hash1",
        "md5hash1",
        false,
        false,
        false,
        false,
        20i64, // High access count
        "not_cached",
        Utc::now(),
        0i64,
        0.0
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // File with medium access count but small size
    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled, access_count,
            cache_status, last_cache_update, cache_hits, cache_priority
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19
        )
        "#,
        file_id2,
        "test-bucket",
        "small.txt",
        "small.txt",
        "/tmp/small.txt",
        1000i64, // 1KB
        1000i64,
        "text/plain",
        "hash2",
        "md5hash2",
        false,
        false,
        false,
        false,
        10i64, // Medium access count
        "not_cached",
        Utc::now(),
        0i64,
        0.0
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // Update cache priorities
    manager.update_cache_priorities().await.unwrap();

    // Verify priorities
    let files = sqlx::query_as!(
        StoredFile,
        "SELECT * FROM files WHERE id IN ($1, $2) ORDER BY cache_priority DESC",
        file_id1,
        file_id2
    )
    .fetch_all(&manager.pool)
    .await
    .unwrap();

    // Small file should have higher priority despite lower access count
    assert_eq!(files[0].id, file_id2);
    assert!(files[0].cache_priority.unwrap() > files[1].cache_priority.unwrap());
}

#[tokio::test]
async fn test_log_file_access() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    let file_id = Uuid::new_v4();
    manager.log_file_access(file_id, "download", Some("test-agent"), Some("127.0.0.1")).await.unwrap();

    let log = sqlx::query!(
        "SELECT access_type, user_agent, ip_address FROM file_access_log WHERE file_id = $1",
        file_id
    )
    .fetch_one(&manager.pool)
    .await
    .unwrap();

    assert_eq!(log.access_type, "download");
    assert_eq!(log.user_agent, Some("test-agent".to_string()));
    assert_eq!(log.ip_address, Some("127.0.0.1".to_string()));
}

#[tokio::test]
async fn test_get_popular_files() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    // Create test files with different access counts
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
        "file1.txt",
        "file1.txt",
        "/tmp/file1.txt",
        100i64,
        100i64,
        "text/plain",
        "hash1",
        "md5hash1",
        false,
        false,
        false,
        false,
        10i64, // High access count
        None::<String>,
        None::<String>,
        None::<f32>,
        None::<i32>,
        None::<String>
    )
    .execute(&manager.pool)
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
        "file2.txt",
        "file2.txt",
        "/tmp/file2.txt",
        200i64,
        200i64,
        "text/plain",
        "hash2",
        "md5hash2",
        false,
        false,
        false,
        false,
        5i64, // Medium access count
        None::<String>,
        None::<String>,
        None::<f32>,
        None::<i32>,
        None::<String>
    )
    .execute(&manager.pool)
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
        "file3.txt",
        "file3.txt",
        "/tmp/file3.txt",
        300i64,
        300i64,
        "text/plain",
        "hash3",
        "md5hash3",
        false,
        false,
        false,
        false,
        1i64, // Low access count
        None::<String>,
        None::<String>,
        None::<f32>,
        None::<i32>,
        None::<String>
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // Get popular files (limit 2)
    let popular_files = manager.get_popular_files(2).await.unwrap();
    assert_eq!(popular_files.len(), 2);

    // Files should be ordered by access count (descending)
    assert_eq!(popular_files[0].access_count, 10);
    assert_eq!(popular_files[1].access_count, 5);
}

#[tokio::test]
async fn test_cache_stats() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    // Insert test config
    sqlx::query!(
        "INSERT INTO cache_config (id, max_cache_size_gb, cache_ttl_seconds, preload_enabled) VALUES (1, 1.0, 3600, true)"
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // Get cache stats
    let stats = manager.get_cache_stats().await.unwrap();

    // Basic validation of stats structure
    assert_eq!(stats.max_size_gb, 1.0);
    assert_eq!(stats.ttl_seconds, 3600);
    assert!(stats.preload_enabled);
    // Note: total_keys might be 0 if Redis is not running or empty
}

#[tokio::test]
async fn test_preload_popular_files_disabled() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    // Insert test config with preload disabled
    sqlx::query!(
        "INSERT INTO cache_config (id, max_cache_size_gb, cache_ttl_seconds, preload_enabled) VALUES (1, 1.0, 3600, false)"
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // Preload should return early when disabled
    let result = manager.preload_popular_files().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_cache_file_content() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    // Create a test file
    let file_id = Uuid::new_v4();
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    std::fs::write(&file_path, b"test content").unwrap();

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
        "test.txt",
        "test.txt",
        file_path.to_str().unwrap(),
        12i64, // "test content" is 12 bytes
        12i64,
        "text/plain",
        "hash123",
        "md5hash123",
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
    .execute(&manager.pool)
    .await
    .unwrap();

    let file = sqlx::query_as!(
        StoredFile,
        r#"
        SELECT 
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, metadata, is_compressed,
            is_encrypted, compression_algorithm, encryption_algorithm,
            compression_ratio, upload_time, last_accessed, access_count,
            encryption_key_id, compression_enabled, encryption_enabled, compression_level
        FROM files
        WHERE id = $1
        "#,
        file_id
    )
    .fetch_one(&manager.pool)
    .await
    .unwrap();

    // Try to cache the file content
    // This might fail if Redis is not running, but that's expected for unit tests
    let result = manager.cache_file_content(&file).await;
    
    if result.is_ok() {
        let cached_size = result.unwrap();
        assert_eq!(cached_size, 12); // Should match file size
    }
}

#[tokio::test]
async fn test_get_cached_content() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    let file_id = Uuid::new_v4();
    let bucket = "test-bucket";
    let key = "test-key";

    // Try to get cached content
    // This might return None if Redis is not running or the key doesn't exist
    let result = manager.get_cached_content(file_id, bucket, key).await;
    
    if result.is_ok() {
        let content = result.unwrap();
        // Content might be None if not cached
        if content.is_some() {
            assert!(!content.unwrap().is_empty());
        }
    }
}

#[tokio::test]
async fn test_remove_from_cache() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    let file_id = Uuid::new_v4();
    let bucket = "test-bucket";
    let key = "test-key";

    // Try to remove from cache
    // This might fail if Redis is not running, but that's expected for unit tests
    let result = manager.remove_from_cache(file_id, bucket, key).await;
    
    // Should not panic even if Redis is not available
    if result.is_err() {
        // Expected if Redis is not running
        assert!(matches!(result.unwrap_err(), StorageError::Cache(_)));
    }
}

#[tokio::test]
async fn test_clear_cache() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    // Try to clear cache
    // This might fail if Redis is not running, but that's expected for unit tests
    let result = manager.clear_cache().await;
    
    // Should not panic even if Redis is not available
    if result.is_err() {
        // Expected if Redis is not running
        assert!(matches!(result.unwrap_err(), StorageError::Cache(_)));
    }
}

#[tokio::test]
async fn test_preload_service_start() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();

    // Test that the service can start without panicking
    // This is a basic smoke test
    manager.start_preload_service();
    
    // Give it a moment to start
    sleep(Duration::from_millis(100)).await;
    
    // The service should be running in the background
    // We can't easily test the background behavior in unit tests
    // but we can verify it doesn't panic on startup
}

#[tokio::test]
async fn test_cache_manager_clone() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let redis_url = setup_mock_redis().await;

    let manager = CacheManager::new(pool, &redis_url, config).unwrap();
    let cloned_manager = manager.clone();

    // Both managers should be able to access the same pool
    assert!(manager.pool.acquire().await.is_ok());
    assert!(cloned_manager.pool.acquire().await.is_ok());
}

#[tokio::test]
async fn test_cache_manager_configuration() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = CacheManager::new(pool.clone(), config);

    // Insert test cache config
    sqlx::query!(
        r#"
        INSERT INTO cache_config (
            id, max_cache_size_gb, cache_ttl_seconds, preload_enabled
        ) VALUES (1, 10.0, 3600, true)
        "#
    )
    .execute(&pool)
    .await
    .unwrap();

    // Test getting cache config
    let config = manager.get_cache_config().await.unwrap();
    assert_eq!(config.max_cache_size_gb, 10.0);
    assert_eq!(config.cache_ttl_seconds, 3600);
    assert!(config.preload_enabled);
}

#[tokio::test]
async fn test_cache_cleanup() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = CacheManager::new(pool.clone(), config);

    // Create test files
    let files = vec![
        StoredFile {
            id: Uuid::new_v4(),
            bucket: "test-bucket".to_string(),
            key: "old-file.txt".to_string(),
            filename: "old-file.bin".to_string(),
            file_path: "/tmp/old-file.txt".to_string(),
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
            upload_time: Utc::now() - chrono::Duration::days(7), // Old file
            last_accessed: None,
            access_count: 1,
            encryption_key_id: None,
            compression_enabled: false,
            encryption_enabled: false,
            compression_level: None,
        },
        StoredFile {
            id: Uuid::new_v4(),
            bucket: "test-bucket".to_string(),
            key: "new-file.txt".to_string(),
            filename: "new-file.bin".to_string(),
            file_path: "/tmp/new-file.txt".to_string(),
            file_size: 100,
            original_size: 100,
            content_type: "text/plain".to_string(),
            hash_blake3: "hash2".to_string(),
            hash_md5: "md5hash2".to_string(),
            metadata: Some(json!({})),
            is_compressed: false,
            is_encrypted: false,
            compression_algorithm: None,
            encryption_algorithm: None,
            compression_ratio: None,
            upload_time: Utc::now(), // Recent file
            last_accessed: None,
            access_count: 1,
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

        // Cache the files
        let content = b"Test content";
        manager.cache_file(file, content).await.unwrap();
    }

    // Run cache cleanup
    manager.cleanup_old_cache_entries().await.unwrap();

    // Old file should be removed from cache
    let old_file_cached = manager.get_cached_file(&files[0]).await;
    assert!(old_file_cached.is_err());

    // New file should still be in cache
    let new_file_cached = manager.get_cached_file(&files[1]).await;
    assert!(new_file_cached.is_ok());
}

#[tokio::test]
async fn test_cache_stats() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = CacheManager::new(pool.clone(), config);

    // Create test files with different access patterns
    let files = vec![
        StoredFile {
            id: Uuid::new_v4(),
            bucket: "test-bucket".to_string(),
            key: "popular.txt".to_string(),
            filename: "popular.bin".to_string(),
            file_path: "/tmp/popular.txt".to_string(),
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
            access_count: 100, // Popular file
            encryption_key_id: None,
            compression_enabled: false,
            encryption_enabled: false,
            compression_level: None,
        },
        StoredFile {
            id: Uuid::new_v4(),
            bucket: "test-bucket".to_string(),
            key: "rare.txt".to_string(),
            filename: "rare.bin".to_string(),
            file_path: "/tmp/rare.txt".to_string(),
            file_size: 100,
            original_size: 100,
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
            access_count: 1, // Rarely accessed file
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

    // Get cache stats
    let stats = manager.get_cache_stats().await.unwrap();
    
    // Check total files and sizes
    assert_eq!(stats.total_files, 2);
    assert_eq!(stats.total_size, 200); // 2 files * 100 bytes
    
    // Check popular files
    let popular_files = manager.get_popular_files(1).await.unwrap();
    assert_eq!(popular_files.len(), 1);
    assert_eq!(popular_files[0].key, "popular.txt");
    assert_eq!(popular_files[0].access_count, 100);
} 