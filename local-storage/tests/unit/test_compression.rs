use std::sync::Arc;
use local_storage::compression::CompressionManager;
use local_storage::config::CompressionConfig;
use crate::helpers::{TestData, assertions::assert_compression_effective};
use crate::helpers::{setup_test_db, TestConfig};
use local_storage::models::StoredFile;
use uuid::Uuid;
use chrono::Utc;
use serde_json::json;
use crate::compression::CompressionManager;
use crate::config::Config;
use crate::helpers::fixtures::create_test_file;

fn create_gzip_manager() -> CompressionManager {
    let config = Arc::new(CompressionConfig {
        enabled: true,
        algorithm: "gzip".to_string(),
        level: 6,
        min_size: 100,
    });
    CompressionManager::new(config)
}

fn create_zstd_manager() -> CompressionManager {
    let config = Arc::new(CompressionConfig {
        enabled: true,
        algorithm: "zstd".to_string(),
        level: 3,
        min_size: 100,
    });
    CompressionManager::new(config)
}

fn create_disabled_manager() -> CompressionManager {
    let config = Arc::new(CompressionConfig {
        enabled: false,
        algorithm: "gzip".to_string(),
        level: 6,
        min_size: 100,
    });
    CompressionManager::new(config)
}

#[test]
fn test_gzip_compression_basic() {
    let manager = create_gzip_manager();
    let data = TestData::compressible_data();
    
    let compressed = manager.compress(&data).expect("Compression should succeed");
    assert!(compressed.len() < data.len(), "Compressed data should be smaller");
    
    let decompressed = manager.decompress(&compressed).expect("Decompression should succeed");
    assert_eq!(data, decompressed, "Decompressed data should match original");
}

#[test]
fn test_zstd_compression_basic() {
    let manager = create_zstd_manager();
    let data = TestData::compressible_data();
    
    let compressed = manager.compress(&data).expect("Compression should succeed");
    assert!(compressed.len() < data.len(), "Compressed data should be smaller");
    
    let decompressed = manager.decompress(&compressed).expect("Decompression should succeed");
    assert_eq!(data, decompressed, "Decompressed data should match original");
}

#[test]
fn test_compression_disabled() {
    let manager = create_disabled_manager();
    let data = TestData::sample_text();
    
    // When compression is disabled, data should pass through unchanged
    let result = manager.compress(&data).expect("Should succeed even when disabled");
    assert_eq!(data, result, "Data should be unchanged when compression is disabled");
    
    let decompressed = manager.decompress(&result).expect("Decompression should succeed");
    assert_eq!(data, decompressed, "Data should remain unchanged");
}

#[test]
fn test_compression_min_size_threshold() {
    let config = Arc::new(CompressionConfig {
        enabled: true,
        algorithm: "gzip".to_string(),
        level: 6,
        min_size: 1000, // Set high threshold
    });
    let manager = CompressionManager::new(config);
    
    let small_data = TestData::sample_text(); // Should be under 1000 bytes
    
    // Small data should not be compressed
    let result = manager.compress(&small_data).expect("Should succeed");
    assert_eq!(small_data, result, "Small data should not be compressed");
}

#[test]
fn test_compression_various_data_types() {
    let manager = create_gzip_manager();
    
    // Test with different types of data
    let test_cases = vec![
        TestData::sample_text(),
        TestData::json_data(),
        TestData::large_text(5000),
        TestData::random_bytes(2048),
    ];
    
    for data in test_cases {
        let compressed = manager.compress(&data).expect("Compression should succeed");
        let decompressed = manager.decompress(&compressed).expect("Decompression should succeed");
        assert_eq!(data, decompressed, "Data integrity should be maintained");
    }
}

#[test]
fn test_compression_levels() {
    let data = TestData::compressible_data();
    
    // Test different compression levels
    let levels = vec![1, 6, 9];
    let mut compressed_sizes = Vec::new();
    
    for level in levels {
        let config = Arc::new(CompressionConfig {
            enabled: true,
            algorithm: "gzip".to_string(),
            level,
            min_size: 100,
        });
        let manager = CompressionManager::new(config);
        
        let compressed = manager.compress(&data).expect("Compression should succeed");
        compressed_sizes.push((level, compressed.len()));
        
        // Verify decompression works
        let decompressed = manager.decompress(&compressed).expect("Decompression should succeed");
        assert_eq!(data, decompressed, "Data integrity should be maintained");
    }
    
    // Generally, higher compression levels should produce smaller output
    // (though this isn't guaranteed for all data types)
    println!("Compression sizes by level: {:?}", compressed_sizes);
}

#[test]
fn test_compression_ratio_calculation() {
    let manager = create_gzip_manager();
    let original_size = 1000;
    let compressed_size = 600;
    
    let ratio = manager.compression_ratio(original_size, compressed_size);
    assert_eq!(ratio, 0.4); // (1000 - 600) / 1000 = 0.4
    
    // Test edge case with zero original size
    let ratio_zero = manager.compression_ratio(0, 100);
    assert_eq!(ratio_zero, 0.0);
}

#[test]
fn test_should_compress_logic() {
    let manager = create_gzip_manager();
    
    assert!(manager.should_compress(1000)); // Above threshold
    assert!(!manager.should_compress(50));  // Below threshold
    
    let disabled_manager = create_disabled_manager();
    assert!(!disabled_manager.should_compress(1000)); // Disabled
}

#[test]
fn test_manager_properties() {
    let gzip_manager = create_gzip_manager();
    assert!(gzip_manager.is_enabled());
    assert_eq!(gzip_manager.algorithm(), "gzip");
    
    let zstd_manager = create_zstd_manager();
    assert!(zstd_manager.is_enabled());
    assert_eq!(zstd_manager.algorithm(), "zstd");
    
    let disabled_manager = create_disabled_manager();
    assert!(!disabled_manager.is_enabled());
}

#[test]
fn test_unsupported_algorithm() {
    let config = Arc::new(CompressionConfig {
        enabled: true,
        algorithm: "unsupported".to_string(),
        level: 6,
        min_size: 100,
    });
    let manager = CompressionManager::new(config);
    let data = TestData::large_text(2000); // Large enough to trigger compression
    
    let result = manager.compress(&data);
    assert!(result.is_err(), "Unsupported algorithm should return error");
    
    if let Err(error) = result {
        assert!(error.to_string().contains("Unsupported compression algorithm"), 
                "Error message should mention unsupported algorithm: {}", error);
    }
}

#[test]
fn test_empty_data_compression() {
    let manager = create_gzip_manager();
    let empty_data = Vec::new();
    
    // Empty data should be passed through unchanged (below min_size threshold)
    let compressed = manager.compress(&empty_data).expect("Should handle empty data");
    assert_eq!(empty_data, compressed, "Empty data should be passed through unchanged");
    
    // Decompression should also pass through unchanged data
    let decompressed = manager.decompress(&compressed).expect("Should handle empty data");
    assert_eq!(empty_data, decompressed);
}

#[test]
fn test_large_data_compression() {
    let manager = create_gzip_manager();
    let large_data = TestData::large_text(100_000); // 100KB of repeated text
    
    let compressed = manager.compress(&large_data).expect("Should compress large data");
    assert_compression_effective(large_data.len(), compressed.len(), 0.1); // Should compress to <10%
    
    let decompressed = manager.decompress(&compressed).expect("Should decompress large data");
    assert_eq!(large_data, decompressed);
}

#[test]
fn test_binary_data_compression() {
    let manager = create_gzip_manager();
    let binary_data = TestData::random_bytes(10000);
    
    // Random binary data typically doesn't compress well, but should still work
    let compressed = manager.compress(&binary_data).expect("Should compress binary data");
    let decompressed = manager.decompress(&compressed).expect("Should decompress binary data");
    assert_eq!(binary_data, decompressed);
}

#[test]
fn test_zstd_vs_gzip_comparison() {
    let compressible_data = TestData::compressible_data();
    
    let gzip_manager = create_gzip_manager();
    let zstd_manager = create_zstd_manager();
    
    let gzip_compressed = gzip_manager.compress(&compressible_data).expect("GZIP compression should succeed");
    let zstd_compressed = zstd_manager.compress(&compressible_data).expect("ZSTD compression should succeed");
    
    // Both should compress the data
    assert!(gzip_compressed.len() < compressible_data.len());
    assert!(zstd_compressed.len() < compressible_data.len());
    
    // Both should decompress correctly
    let gzip_decompressed = gzip_manager.decompress(&gzip_compressed).expect("GZIP decompression should succeed");
    let zstd_decompressed = zstd_manager.decompress(&zstd_compressed).expect("ZSTD decompression should succeed");
    
    assert_eq!(compressible_data, gzip_decompressed);
    assert_eq!(compressible_data, zstd_decompressed);
    
    println!("Original size: {}", compressible_data.len());
    println!("GZIP compressed: {}", gzip_compressed.len());
    println!("ZSTD compressed: {}", zstd_compressed.len());
}

#[test]
fn test_invalid_compressed_data() {
    let manager = create_gzip_manager();
    
    // Test with data that starts with gzip magic bytes but is corrupted
    let mut invalid_data = vec![0x1f, 0x8b]; // gzip magic bytes
    invalid_data.extend_from_slice(b"This is corrupted gzip data");
    
    let result = manager.decompress(&invalid_data);
    assert!(result.is_err(), "Invalid compressed data should return error");
}

#[tokio::test]
async fn test_compression_configuration() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = CompressionManager::new(pool.clone(), config);

    // Test default compression settings
    let settings = manager.get_compression_settings().await.unwrap();
    assert!(settings.enabled);
    assert_eq!(settings.default_algorithm, CompressionAlgorithm::Zstd);
    assert_eq!(settings.default_level, 3);
}

#[tokio::test]
async fn test_file_compression() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = CompressionManager::new(pool.clone(), config);

    // Create a test file
    let file_id = Uuid::new_v4();
    let test_content = "Test content for compression".repeat(100); // Make it large enough to compress
    
    let file = StoredFile {
        id: file_id,
        bucket: "test-bucket".to_string(),
        key: "test-file.txt".to_string(),
        filename: format!("{}.bin", file_id),
        file_path: "/tmp/test-file.txt".to_string(),
        file_size: test_content.len() as i64,
        original_size: test_content.len() as i64,
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
        compression_enabled: true,
        encryption_enabled: false,
        compression_level: Some(3),
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

    // Compress the file
    let compressed_data = manager.compress_file(&file, test_content.as_bytes()).await.unwrap();
    
    // Verify compression results
    assert!(compressed_data.len() < test_content.len());
    
    // Check database update
    let updated_file = sqlx::query!(
        r#"
        SELECT 
            is_compressed, compression_algorithm, compression_ratio,
            compression_level, file_size, original_size
        FROM files 
        WHERE id = $1
        "#,
        file_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(updated_file.is_compressed);
    assert_eq!(updated_file.compression_algorithm.unwrap(), "zstd");
    assert!(updated_file.compression_ratio.unwrap() < 1.0);
    assert_eq!(updated_file.compression_level.unwrap(), 3);
    assert!(updated_file.file_size < updated_file.original_size);
}

#[tokio::test]
async fn test_compression_algorithms() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = CompressionManager::new(pool.clone(), config);

    let test_data = "Test content for compression".repeat(100);
    
    // Test different algorithms
    let algorithms = vec![
        CompressionAlgorithm::Zstd,
        CompressionAlgorithm::Lz4,
        CompressionAlgorithm::Snappy,
    ];

    for algorithm in algorithms {
        let file_id = Uuid::new_v4();
        let file = StoredFile {
            id: file_id,
            bucket: "test-bucket".to_string(),
            key: format!("test-file-{}.txt", algorithm),
            filename: format!("{}.bin", file_id),
            file_path: format!("/tmp/test-file-{}.txt", algorithm),
            file_size: test_data.len() as i64,
            original_size: test_data.len() as i64,
            content_type: "text/plain".to_string(),
            hash_blake3: "mock-blake3-hash".to_string(),
            hash_md5: "mock-md5-hash".to_string(),
            metadata: Some(json!({})),
            is_compressed: false,
            is_encrypted: false,
            compression_algorithm: Some(algorithm.to_string()),
            encryption_algorithm: None,
            compression_ratio: None,
            upload_time: Utc::now(),
            last_accessed: None,
            access_count: 0,
            encryption_key_id: None,
            compression_enabled: true,
            encryption_enabled: false,
            compression_level: Some(3),
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

        let compressed_data = manager.compress_file(&file, test_data.as_bytes()).await.unwrap();
        
        // Verify compression
        assert!(compressed_data.len() < test_data.len());
        
        // Check database update
        let updated_file = sqlx::query!(
            r#"
            SELECT 
                is_compressed, compression_algorithm, compression_ratio,
                compression_level, file_size, original_size
            FROM files 
            WHERE id = $1
            "#,
            file_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(updated_file.is_compressed);
        assert_eq!(updated_file.compression_algorithm.unwrap(), algorithm.to_string());
        assert!(updated_file.compression_ratio.unwrap() < 1.0);
        assert!(updated_file.file_size < updated_file.original_size);
    }
}

#[tokio::test]
async fn test_compression_levels() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = CompressionManager::new(pool.clone(), config);

    let test_data = "Test content for compression".repeat(100);
    let levels = vec![1, 3, 6, 9]; // Test different compression levels

    let mut previous_size = test_data.len();
    for level in levels {
        let file_id = Uuid::new_v4();
        let file = StoredFile {
            id: file_id,
            bucket: "test-bucket".to_string(),
            key: format!("test-file-level-{}.txt", level),
            filename: format!("{}.bin", file_id),
            file_path: format!("/tmp/test-file-level-{}.txt", level),
            file_size: test_data.len() as i64,
            original_size: test_data.len() as i64,
            content_type: "text/plain".to_string(),
            hash_blake3: "mock-blake3-hash".to_string(),
            hash_md5: "mock-md5-hash".to_string(),
            metadata: Some(json!({})),
            is_compressed: false,
            is_encrypted: false,
            compression_algorithm: Some("zstd".to_string()),
            encryption_algorithm: None,
            compression_ratio: None,
            upload_time: Utc::now(),
            last_accessed: None,
            access_count: 0,
            encryption_key_id: None,
            compression_enabled: true,
            encryption_enabled: false,
            compression_level: Some(level),
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

        let compressed_data = manager.compress_file(&file, test_data.as_bytes()).await.unwrap();
        
        // Higher levels should generally achieve better compression
        assert!(compressed_data.len() <= previous_size);
        previous_size = compressed_data.len();
        
        // Check database update
        let updated_file = sqlx::query!(
            r#"
            SELECT 
                is_compressed, compression_algorithm, compression_ratio,
                compression_level, file_size, original_size
            FROM files 
            WHERE id = $1
            "#,
            file_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(updated_file.is_compressed);
        assert_eq!(updated_file.compression_level.unwrap(), level);
        assert!(updated_file.compression_ratio.unwrap() < 1.0);
        assert!(updated_file.file_size < updated_file.original_size);
    }
}

#[tokio::test]
async fn test_compression() {
    let config = Arc::new(Config::default());
    let manager = CompressionManager::new(config);
    let mut file = create_test_file("test-bucket", "test.txt", 1024);
    file.compression_enabled = Some(true);

    let test_data = b"Test data for compression";
    let compressed_data = manager.compress(test_data).unwrap();
    assert!(compressed_data.len() < test_data.len());

    let decompressed_data = manager.decompress(&compressed_data).unwrap();
    assert_eq!(decompressed_data, test_data);
} 