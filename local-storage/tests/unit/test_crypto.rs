use std::sync::Arc;
use local_storage::crypto::CryptoManager;
use local_storage::config::CryptoConfig;
use crate::helpers::TestData;
use crate::helpers::{setup_test_db, TestConfig};
use local_storage::models::StoredFile;
use uuid::Uuid;
use chrono::Utc;
use serde_json::json;

fn create_aes_manager() -> CryptoManager {
    let config = Arc::new(CryptoConfig {
        enabled: true,
        algorithm: "aes-gcm".to_string(),
        key: Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()),
    });
    CryptoManager::new(config).expect("Failed to create AES manager")
}

fn create_chacha_manager() -> CryptoManager {
    let config = Arc::new(CryptoConfig {
        enabled: true,
        algorithm: "chacha20poly1305".to_string(),
        key: Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()),
    });
    CryptoManager::new(config).expect("Failed to create ChaCha manager")
}

fn create_disabled_manager() -> CryptoManager {
    let config = Arc::new(CryptoConfig {
        enabled: false,
        algorithm: "aes-gcm".to_string(),
        key: None,
    });
    CryptoManager::new(config).expect("Failed to create disabled manager")
}

#[test]
fn test_aes_encryption_basic() {
    let manager = create_aes_manager();
    let data = TestData::sample_text();
    
    let encrypted = manager.encrypt(&data).expect("Encryption should succeed");
    assert_ne!(data, encrypted, "Encrypted data should be different from original");
    assert!(encrypted.len() > data.len(), "Encrypted data should be larger (includes nonce and tag)");
    
    let decrypted = manager.decrypt(&encrypted).expect("Decryption should succeed");
    assert_eq!(data, decrypted, "Decrypted data should match original");
}

#[test]
fn test_chacha_encryption_basic() {
    let manager = create_chacha_manager();
    let data = TestData::sample_text();
    
    let encrypted = manager.encrypt(&data).expect("Encryption should succeed");
    assert_ne!(data, encrypted, "Encrypted data should be different from original");
    assert!(encrypted.len() > data.len(), "Encrypted data should be larger (includes nonce and tag)");
    
    let decrypted = manager.decrypt(&encrypted).expect("Decryption should succeed");
    assert_eq!(data, decrypted, "Decrypted data should match original");
}

#[test]
fn test_encryption_disabled() {
    let manager = create_disabled_manager();
    let data = TestData::sample_text();
    
    // When encryption is disabled, data should pass through unchanged
    let result = manager.encrypt(&data).expect("Should succeed even when disabled");
    assert_eq!(data, result, "Data should be unchanged when encryption is disabled");
    
    let decrypted = manager.decrypt(&result).expect("Decryption should succeed");
    assert_eq!(data, decrypted, "Data should remain unchanged");
}

#[test]
fn test_encryption_various_data_types() {
    let manager = create_aes_manager();
    
    let test_cases = vec![
        TestData::sample_text(),
        TestData::json_data(),
        TestData::large_text(5000),
        TestData::random_bytes(2048),
        Vec::new(), // Empty data
    ];
    
    for data in test_cases {
        let encrypted = manager.encrypt(&data).expect("Encryption should succeed");
        let decrypted = manager.decrypt(&encrypted).expect("Decryption should succeed");
        assert_eq!(data, decrypted, "Data integrity should be maintained");
    }
}

#[test]
fn test_encryption_randomness() {
    let manager = create_aes_manager();
    let data = TestData::sample_text();
    
    // Encrypt the same data multiple times - results should be different due to random nonces
    let encrypted1 = manager.encrypt(&data).expect("First encryption should succeed");
    let encrypted2 = manager.encrypt(&data).expect("Second encryption should succeed");
    
    assert_ne!(encrypted1, encrypted2, "Multiple encryptions should produce different ciphertext");
    
    // Both should decrypt to the same original data
    let decrypted1 = manager.decrypt(&encrypted1).expect("First decryption should succeed");
    let decrypted2 = manager.decrypt(&encrypted2).expect("Second decryption should succeed");
    
    assert_eq!(data, decrypted1);
    assert_eq!(data, decrypted2);
}

#[test]
fn test_manager_properties() {
    let aes_manager = create_aes_manager();
    assert!(aes_manager.is_enabled());
    assert_eq!(aes_manager.algorithm(), "aes-gcm");
    
    let chacha_manager = create_chacha_manager();
    assert!(chacha_manager.is_enabled());
    assert_eq!(chacha_manager.algorithm(), "chacha20poly1305");
    
    let disabled_manager = create_disabled_manager();
    assert!(!disabled_manager.is_enabled());
}

#[test]
fn test_invalid_key_length() {
    // Test with invalid key length (too short)
    let config = Arc::new(CryptoConfig {
        enabled: true,
        algorithm: "aes-gcm".to_string(),
        key: Some("short_key".to_string()),
    });
    
    let result = CryptoManager::new(config);
    assert!(result.is_err(), "Invalid key length should return error");
}

#[test]
fn test_invalid_hex_key() {
    // Test with invalid hex characters
    let config = Arc::new(CryptoConfig {
        enabled: true,
        algorithm: "aes-gcm".to_string(),
        key: Some("gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg".to_string()),
    });
    
    let result = CryptoManager::new(config);
    assert!(result.is_err(), "Invalid hex key should return error");
}

#[test]
fn test_unsupported_algorithm() {
    let config = Arc::new(CryptoConfig {
        enabled: true,
        algorithm: "unsupported".to_string(),
        key: Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()),
    });
    
    let result = CryptoManager::new(config);
    assert!(result.is_err(), "Unsupported algorithm should return error");
}

#[test]
fn test_32_byte_string_key() {
    // Test with 32-byte string key (not hex) - using only ASCII characters
    let config = Arc::new(CryptoConfig {
        enabled: true,
        algorithm: "aes-gcm".to_string(),
        key: Some("abcdefghijklmnopqrstuvwxyz123456".to_string()), // Exactly 32 ASCII bytes
    });
    
    let manager = CryptoManager::new(config).expect("32-byte string key should work");
    let data = TestData::sample_text();
    
    let encrypted = manager.encrypt(&data).expect("Encryption should succeed");
    let decrypted = manager.decrypt(&encrypted).expect("Decryption should succeed");
    assert_eq!(data, decrypted);
}

#[test]
fn test_random_key_generation() {
    // Test with no key provided (should generate random key)
    let config = Arc::new(CryptoConfig {
        enabled: true,
        algorithm: "aes-gcm".to_string(),
        key: None,
    });
    
    let manager = CryptoManager::new(config).expect("Random key generation should work");
    let data = TestData::sample_text();
    
    let encrypted = manager.encrypt(&data).expect("Encryption should succeed");
    let decrypted = manager.decrypt(&encrypted).expect("Decryption should succeed");
    assert_eq!(data, decrypted);
}

#[test]
fn test_invalid_encrypted_data() {
    let manager = create_aes_manager();
    
    // Test with data that's too short (less than nonce size)
    let short_data = b"short".to_vec();
    let result = manager.decrypt(&short_data);
    assert!(result.is_err(), "Too short encrypted data should return error");
    
    // Test with invalid encrypted data
    let invalid_data = b"This is not encrypted data but long enough".to_vec();
    let result = manager.decrypt(&invalid_data);
    assert!(result.is_err(), "Invalid encrypted data should return error");
}

#[test]
fn test_large_data_encryption() {
    let manager = create_aes_manager();
    let large_data = TestData::large_text(100_000); // 100KB
    
    let encrypted = manager.encrypt(&large_data).expect("Should encrypt large data");
    let decrypted = manager.decrypt(&encrypted).expect("Should decrypt large data");
    assert_eq!(large_data, decrypted);
}

#[test]
fn test_binary_data_encryption() {
    let manager = create_aes_manager();
    let binary_data = TestData::random_bytes(10000);
    
    let encrypted = manager.encrypt(&binary_data).expect("Should encrypt binary data");
    let decrypted = manager.decrypt(&encrypted).expect("Should decrypt binary data");
    assert_eq!(binary_data, decrypted);
}

#[test]
fn test_aes_vs_chacha_comparison() {
    let data = TestData::sample_text();
    
    let aes_manager = create_aes_manager();
    let chacha_manager = create_chacha_manager();
    
    let aes_encrypted = aes_manager.encrypt(&data).expect("AES encryption should succeed");
    let chacha_encrypted = chacha_manager.encrypt(&data).expect("ChaCha encryption should succeed");
    
    // Both should encrypt the data (make it different from original)
    assert_ne!(data, aes_encrypted);
    assert_ne!(data, chacha_encrypted);
    
    // Both should decrypt correctly
    let aes_decrypted = aes_manager.decrypt(&aes_encrypted).expect("AES decryption should succeed");
    let chacha_decrypted = chacha_manager.decrypt(&chacha_encrypted).expect("ChaCha decryption should succeed");
    
    assert_eq!(data, aes_decrypted);
    assert_eq!(data, chacha_decrypted);
    
    // Cross-decryption should fail (AES encrypted data can't be decrypted with ChaCha)
    let cross_result = chacha_manager.decrypt(&aes_encrypted);
    assert!(cross_result.is_err(), "Cross-algorithm decryption should fail");
}

#[test]
fn test_encryption_overhead() {
    let manager = create_aes_manager();
    let data = TestData::sample_text();
    
    let encrypted = manager.encrypt(&data).expect("Encryption should succeed");
    let overhead = encrypted.len() - data.len();
    
    // AES-GCM has 12 bytes nonce + 16 bytes authentication tag = 28 bytes overhead
    assert_eq!(overhead, 28, "AES-GCM should have exactly 28 bytes overhead");
}

#[test]
fn test_empty_data_encryption() {
    let manager = create_aes_manager();
    let empty_data = Vec::new();
    
    let encrypted = manager.encrypt(&empty_data).expect("Should handle empty data");
    let decrypted = manager.decrypt(&encrypted).expect("Should decrypt empty data");
    assert_eq!(empty_data, decrypted);
    
    // Even empty data should have overhead when encrypted
    assert!(encrypted.len() > 0, "Encrypted empty data should still have overhead");
}

#[tokio::test]
async fn test_encryption_configuration() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = CryptoManager::new(pool.clone(), config);

    // Test default encryption settings
    let settings = manager.get_encryption_settings().await.unwrap();
    assert!(settings.enabled);
    assert_eq!(settings.default_algorithm, EncryptionAlgorithm::AesGcm);
}

#[tokio::test]
async fn test_file_encryption() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = CryptoManager::new(pool.clone(), config);

    // Create a test file
    let file_id = Uuid::new_v4();
    let test_content = "Test content for encryption".repeat(10);
    
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
        compression_enabled: false,
        encryption_enabled: true,
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

    // Encrypt the file
    let encrypted_data = manager.encrypt_file(&file, test_content.as_bytes()).await.unwrap();
    
    // Verify encryption results
    assert_ne!(encrypted_data, test_content.as_bytes());
    assert!(encrypted_data.len() > test_content.len()); // Due to IV and auth tag
    
    // Check database update
    let updated_file = sqlx::query!(
        r#"
        SELECT 
            is_encrypted, encryption_algorithm, encryption_key_id
        FROM files 
        WHERE id = $1
        "#,
        file_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(updated_file.is_encrypted);
    assert_eq!(updated_file.encryption_algorithm.unwrap(), "aes-gcm");
    assert!(updated_file.encryption_key_id.is_some());
}

#[tokio::test]
async fn test_encryption_algorithms() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = CryptoManager::new(pool.clone(), config);

    let test_data = "Test content for encryption".repeat(10);
    
    // Test different algorithms
    let algorithms = vec![
        EncryptionAlgorithm::AesGcm,
        EncryptionAlgorithm::ChaCha20Poly1305,
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
            compression_algorithm: None,
            encryption_algorithm: Some(algorithm.to_string()),
            compression_ratio: None,
            upload_time: Utc::now(),
            last_accessed: None,
            access_count: 0,
            encryption_key_id: None,
            compression_enabled: false,
            encryption_enabled: true,
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

        let encrypted_data = manager.encrypt_file(&file, test_data.as_bytes()).await.unwrap();
        
        // Verify encryption
        assert_ne!(encrypted_data, test_data.as_bytes());
        assert!(encrypted_data.len() > test_data.len()); // Due to IV and auth tag
        
        // Check database update
        let updated_file = sqlx::query!(
            r#"
            SELECT 
                is_encrypted, encryption_algorithm, encryption_key_id
            FROM files 
            WHERE id = $1
            "#,
            file_id
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(updated_file.is_encrypted);
        assert_eq!(updated_file.encryption_algorithm.unwrap(), algorithm.to_string());
        assert!(updated_file.encryption_key_id.is_some());
    }
}

#[tokio::test]
async fn test_key_rotation() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = CryptoManager::new(pool.clone(), config);

    // Create a test file with encryption
    let file_id = Uuid::new_v4();
    let test_content = "Test content for key rotation".repeat(10);
    
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
        compression_enabled: false,
        encryption_enabled: true,
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

    // First encryption
    let encrypted_data = manager.encrypt_file(&file, test_content.as_bytes()).await.unwrap();
    let old_key_id = sqlx::query!(
        "SELECT encryption_key_id FROM files WHERE id = $1",
        file_id
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .encryption_key_id
    .unwrap();

    // Rotate key
    manager.rotate_encryption_key(&file).await.unwrap();

    // Check that key was rotated
    let updated_file = sqlx::query!(
        r#"
        SELECT encryption_key_id, is_encrypted, encryption_algorithm
        FROM files 
        WHERE id = $1
        "#,
        file_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_ne!(updated_file.encryption_key_id.unwrap(), old_key_id);
    assert!(updated_file.is_encrypted);
    assert!(updated_file.encryption_algorithm.is_some());
} 