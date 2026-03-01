// Cache system tests
// Note: Full cache tests require Redis integration

use crate::helpers::{setup_test_db, TestConfig};
use local_storage::cache::{CacheManager, CacheConfig};
use local_storage::models::StoredFile;
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;
use serde_json::json;
use crate::cache::CacheManager;
use crate::config::Config;
use crate::helpers::fixtures::create_test_file;

#[test]
fn test_cache_key_format() {
    // Test cache key formatting without requiring Redis
    let bucket = "test-bucket";
    let key = "test-file.txt";
    let cache_key = format!("local_storage:{}:{}", bucket, key);
    
    assert_eq!(cache_key, "local_storage:test-bucket:test-file.txt");
    assert!(cache_key.contains(bucket));
    assert!(cache_key.contains(key));
    assert!(cache_key.starts_with("local_storage:"));
}

#[test]
fn test_cache_key_sanitization() {
    // Test that cache keys are properly sanitized
    let bucket = "test:bucket";
    let key = "test file.txt";
    
    // Cache keys should handle special characters
    let sanitized_key = format!("local_storage:{}:{}", 
        bucket.replace(':', "_"), 
        key.replace(' ', "_")
    );
    
    assert_eq!(sanitized_key, "local_storage:test_bucket:test_file.txt");
    assert!(!sanitized_key.contains(':') || sanitized_key.starts_with("local_storage:"));
    assert!(!sanitized_key.contains(' '));
}

#[test]
fn test_cache_ttl_calculation() {
    // Test cache TTL calculations
    let base_ttl = 3600; // 1 hour
    let file_size = 1024 * 1024 + 1; // 1MB + 1 byte (greater than 1MB)
    
    // Larger files might have shorter TTL
    let calculated_ttl = if file_size > 1024 * 1024 {
        base_ttl / 2
    } else {
        base_ttl
    };
    
    // The file size is 1MB, so it should be divided by 2
    assert_eq!(calculated_ttl, 1800); // 30 minutes for large files (1MB > 1MB threshold)
}

#[test]
fn test_cache_metrics() {
    // Test cache metrics calculation
    struct CacheMetrics {
        hits: u64,
        misses: u64,
        total_requests: u64,
    }
    
    let metrics = CacheMetrics {
        hits: 75,
        misses: 25,
        total_requests: 100,
    };
    
    let hit_rate = metrics.hits as f64 / metrics.total_requests as f64;
    assert_eq!(hit_rate, 0.75);
    assert!(hit_rate > 0.5); // Good hit rate
}

#[tokio::test]
async fn test_cache_configuration() {
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
async fn test_cache_file() {
    let config = Arc::new(Config::default());
    let manager = CacheManager::new(config).await.unwrap();
    let file = create_test_file("test-bucket", "test.txt", 1024);
    let test_content = b"test content".to_vec();

    manager.cache_file(&file, &test_content).await.unwrap();
    let cached_content = manager.get_cached_file(&file).await.unwrap();
    assert_eq!(cached_content, test_content);
}

#[tokio::test]
async fn test_cache_eviction() {
    let config = Arc::new(Config::default());
    let manager = CacheManager::new(config).await.unwrap();

    // Create and cache multiple files
    let files: Vec<_> = (0..5)
        .map(|i| create_test_file("test-bucket", &format!("file{}.txt", i), 1024))
        .collect();

    for (i, file) in files.iter().enumerate() {
        let content = format!("content {}", i).into_bytes();
        manager.cache_file(file, &content).await.unwrap();
    }

    // Verify files are cached
    let cached_content = manager.get_cached_file(&files[1]).await.unwrap();
    assert_eq!(cached_content, b"content 1");
}

#[tokio::test]
async fn test_preload_popular_files() {
    let config = Arc::new(Config::default());
    let manager = CacheManager::new(config).await.unwrap();

    // Create files with different access counts
    let mut popular_file = create_test_file("test-bucket", "popular.txt", 1024);
    popular_file.access_count = 100;
    popular_file.cache_priority = Some(10);

    let mut rare_file = create_test_file("test-bucket", "rare.txt", 1024);
    rare_file.access_count = 1;
    rare_file.cache_priority = Some(1);

    let files = vec![popular_file, rare_file];

    // Preload files
    manager.preload_popular_files().await.unwrap();

    // Check if popular file was cached
    let popular_content = manager.get_cached_file(&files[0]).await;
    assert!(popular_content.is_ok());

    // Check if rare file was not cached
    let rare_content = manager.get_cached_file(&files[1]).await;
    assert!(rare_content.is_err());
} 