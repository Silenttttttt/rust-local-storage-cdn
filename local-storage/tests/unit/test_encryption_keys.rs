use crate::config::Config;
use crate::encryption_keys::EncryptionKeyManager;
use crate::errors::StorageError;
use crate::models::EncryptionKey;
use crate::tests::helpers::{setup_test_db, TestConfig};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn test_create_encryption_key() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    let key_id = "test-key-123";
    let key_data = vec![1u8; 32]; // 32 bytes for AES-GCM
    let algorithm = "aes-gcm";
    let description = Some("Test encryption key");

    let key = manager
        .create_key(key_id, &key_data, algorithm, description)
        .await
        .unwrap();

    assert_eq!(key.key_id, key_id);
    assert_eq!(key.key_data, key_data);
    assert_eq!(key.algorithm, algorithm);
    assert_eq!(key.description, description);
    assert!(key.is_active);
}

#[tokio::test]
async fn test_get_encryption_key() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    let key_id = "test-key-456";
    let key_data = vec![2u8; 32];
    let algorithm = "chacha20poly1305";

    // Create the key
    manager
        .create_key(key_id, &key_data, algorithm, None)
        .await
        .unwrap();

    // Retrieve the key
    let retrieved_key = manager.get_key(key_id).await.unwrap();

    assert_eq!(retrieved_key.key_id, key_id);
    assert_eq!(retrieved_key.key_data, key_data);
    assert_eq!(retrieved_key.algorithm, algorithm);
    assert!(retrieved_key.is_active);
}

#[tokio::test]
async fn test_get_nonexistent_key() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    let result = manager.get_key("nonexistent-key").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_list_encryption_keys() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    // Create multiple keys
    let key_data = vec![3u8; 32];
    manager
        .create_key("key-1", &key_data, "aes-gcm", None)
        .await
        .unwrap();
    manager
        .create_key("key-2", &key_data, "chacha20poly1305", None)
        .await
        .unwrap();

    let keys = manager.list_keys().await.unwrap();
    assert_eq!(keys.len(), 2);
    assert!(keys.iter().any(|k| k.key_id == "key-1"));
    assert!(keys.iter().any(|k| k.key_id == "key-2"));
}

#[tokio::test]
async fn test_deactivate_encryption_key() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    let key_id = "test-key-deactivate";
    let key_data = vec![4u8; 32];

    // Create the key
    manager
        .create_key(key_id, &key_data, "aes-gcm", None)
        .await
        .unwrap();

    // Deactivate the key
    manager.deactivate_key(key_id).await.unwrap();

    // Try to get the key - should fail because it's inactive
    let result = manager.get_key(key_id).await;
    assert!(result.is_err());

    // List keys should not include deactivated keys
    let keys = manager.list_keys().await.unwrap();
    assert!(!keys.iter().any(|k| k.key_id == key_id));
}

#[tokio::test]
async fn test_validate_key_format() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    // Valid AES-GCM key (32 bytes)
    let valid_aes_key = vec![5u8; 32];
    assert!(manager.validate_key_format(&valid_aes_key, "aes-gcm").is_ok());

    // Valid ChaCha20-Poly1305 key (32 bytes)
    let valid_chacha_key = vec![6u8; 32];
    assert!(manager.validate_key_format(&valid_chacha_key, "chacha20poly1305").is_ok());

    // Invalid AES-GCM key (wrong size)
    let invalid_aes_key = vec![7u8; 16]; // 16 bytes instead of 32
    let result = manager.validate_key_format(&invalid_aes_key, "aes-gcm");
    assert!(result.is_err());
    match result {
        Err(StorageError::Validation(msg)) => {
            assert!(msg.contains("AES-GCM requires 32-byte key"));
        }
        _ => panic!("Expected Validation error"),
    }

    // Invalid ChaCha20-Poly1305 key (wrong size)
    let invalid_chacha_key = vec![8u8; 24]; // 24 bytes instead of 32
    let result = manager.validate_key_format(&invalid_chacha_key, "chacha20poly1305");
    assert!(result.is_err());
    match result {
        Err(StorageError::Validation(msg)) => {
            assert!(msg.contains("ChaCha20-Poly1305 requires 32-byte key"));
        }
        _ => panic!("Expected Validation error"),
    }

    // Unsupported algorithm
    let valid_key = vec![9u8; 32];
    let result = manager.validate_key_format(&valid_key, "unsupported-algo");
    assert!(result.is_err());
    match result {
        Err(StorageError::Validation(msg)) => {
            assert!(msg.contains("Unsupported encryption algorithm"));
        }
        _ => panic!("Expected Validation error"),
    }
}

#[tokio::test]
async fn test_generate_key_id() {
    let key_id1 = EncryptionKeyManager::generate_key_id();
    let key_id2 = EncryptionKeyManager::generate_key_id();

    // Key IDs should be 32 characters long
    assert_eq!(key_id1.len(), 32);
    assert_eq!(key_id2.len(), 32);

    // Key IDs should be different (very unlikely to be the same)
    assert_ne!(key_id1, key_id2);

    // Key IDs should be alphanumeric
    assert!(key_id1.chars().all(|c| c.is_alphanumeric()));
    assert!(key_id2.chars().all(|c| c.is_alphanumeric()));
}

#[tokio::test]
async fn test_update_file_key() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    // First, we need to create a file in the database
    let file_id = Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO files (id, bucket, key, filename, file_path, file_size, original_size, content_type, hash_blake3, hash_md5, is_compressed, is_encrypted, compression_enabled, encryption_enabled) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        file_id,
        "test-bucket",
        "test-key",
        "test.txt",
        "/tmp/test.txt",
        100i64,
        100i64,
        "text/plain",
        "hash123",
        "md5hash123",
        false,
        false,
        false,
        false
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // Create an encryption key
    let key_id = "test-file-key";
    let key_data = vec![10u8; 32];
    manager
        .create_key(key_id, &key_data, "aes-gcm", None)
        .await
        .unwrap();

    // Update file's encryption key
    manager.update_file_key(file_id, Some(key_id)).await.unwrap();

    // Verify the file now has the encryption key
    let file_key = manager.get_file_key(file_id).await.unwrap();
    assert!(file_key.is_some());
    assert_eq!(file_key.unwrap().key_id, key_id);

    // Remove the encryption key
    manager.update_file_key(file_id, None).await.unwrap();

    // Verify the file no longer has an encryption key
    let file_key = manager.get_file_key(file_id).await.unwrap();
    assert!(file_key.is_none());
}

#[tokio::test]
async fn test_get_file_key_nonexistent_file() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    let nonexistent_file_id = Uuid::new_v4();
    let result = manager.get_file_key(nonexistent_file_id).await;
    assert!(result.is_err());
    match result {
        Err(StorageError::NotFound(msg)) => {
            assert!(msg.contains("File not found"));
        }
        _ => panic!("Expected NotFound error"),
    }
}

#[tokio::test]
async fn test_get_files_with_key() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    // Create an encryption key
    let key_id = "test-files-key";
    let key_data = vec![11u8; 32];
    manager
        .create_key(key_id, &key_data, "aes-gcm", Some("Test key"))
        .await
        .unwrap();

    // Create files with the encryption key
    let file_id1 = Uuid::new_v4();
    let file_id2 = Uuid::new_v4();
    let file_id3 = Uuid::new_v4();

    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled, encryption_key_id,
            compression_algorithm, encryption_algorithm, compression_ratio,
            compression_level
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19
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
        true,
        false,
        true,
        key_id,
        None::<String>,
        Some("aes-gcm"),
        None::<f32>,
        None::<i32>
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled, encryption_key_id,
            compression_algorithm, encryption_algorithm, compression_ratio,
            compression_level
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19
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
        true,
        false,
        true,
        key_id,
        None::<String>,
        Some("aes-gcm"),
        None::<f32>,
        None::<i32>
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // Create a file without encryption key
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
        None::<String>,
        None::<String>,
        None::<String>,
        None::<f32>,
        None::<i32>
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // Get files with the encryption key
    let files = manager.get_files_with_key(key_id).await.unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|f| f.id == file_id1));
    assert!(files.iter().any(|f| f.id == file_id2));
    assert!(!files.iter().any(|f| f.id == file_id3));
}

#[tokio::test]
async fn test_key_rotation() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    // Create initial key
    let old_key_id = "old-key";
    let old_key_data = vec![1u8; 32];
    manager
        .create_key(old_key_id, &old_key_data, "aes-gcm", Some("Old key"))
        .await
        .unwrap();

    // Create a file using the old key
    let file_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled, encryption_key_id,
            compression_algorithm, encryption_algorithm, compression_ratio,
            compression_level
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19
        )"#,
        file_id,
        "test-bucket",
        "encrypted-file.txt",
        "encrypted-file.txt",
        "/tmp/encrypted-file.txt",
        100i64,
        100i64,
        "text/plain",
        "hash1",
        "md5hash1",
        false,
        true,
        false,
        true,
        old_key_id,
        None::<String>,
        Some("aes-gcm"),
        None::<f32>,
        None::<i32>
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // Rotate to new key
    let new_key_data = vec![2u8; 32];
    let new_key = manager.rotate_key(old_key_id, new_key_data).await.unwrap();

    // Verify file now uses new key
    let file = sqlx::query!(
        r#"
        SELECT encryption_key_id, is_encrypted, encryption_enabled
        FROM files
        WHERE id = $1
        "#,
        file_id
    )
    .fetch_one(&manager.pool)
    .await
    .unwrap();

    assert_eq!(file.encryption_key_id, Some(new_key.key_id));
    assert!(file.is_encrypted);
    assert!(file.encryption_enabled);

    // Verify old key is deactivated
    let old_key = sqlx::query!(
        "SELECT is_active FROM encryption_keys WHERE key_id = $1",
        old_key_id
    )
    .fetch_one(&manager.pool)
    .await
    .unwrap();

    assert!(!old_key.is_active);
}

#[tokio::test]
async fn test_delete_unused_key() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    // Create a key
    let key_id = "unused-key";
    let key_data = vec![3u8; 32];
    manager
        .create_key(key_id, &key_data, "aes-gcm", Some("Unused key"))
        .await
        .unwrap();

    // Delete the unused key
    manager.delete_key(key_id).await.unwrap();

    // Verify key is deleted
    let result = sqlx::query!(
        "SELECT COUNT(*) as count FROM encryption_keys WHERE key_id = $1",
        key_id
    )
    .fetch_one(&manager.pool)
    .await
    .unwrap();

    assert_eq!(result.count.unwrap_or(0), 0);
}

#[tokio::test]
async fn test_delete_key_in_use() {
    let (pool, _temp_dir) = setup_test_db().await;
    let config = Arc::new(TestConfig::default().build());
    let manager = EncryptionKeyManager::new(pool, config);

    // Create a key
    let key_id = "used-key";
    let key_data = vec![4u8; 32];
    manager
        .create_key(key_id, &key_data, "aes-gcm", Some("Used key"))
        .await
        .unwrap();

    // Create a file using the key
    let file_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO files (
            id, bucket, key, filename, file_path, file_size, original_size,
            content_type, hash_blake3, hash_md5, is_compressed, is_encrypted,
            compression_enabled, encryption_enabled, encryption_key_id,
            compression_algorithm, encryption_algorithm, compression_ratio,
            compression_level
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
            $16, $17, $18, $19
        )"#,
        file_id,
        "test-bucket",
        "encrypted-file.txt",
        "encrypted-file.txt",
        "/tmp/encrypted-file.txt",
        100i64,
        100i64,
        "text/plain",
        "hash1",
        "md5hash1",
        false,
        true,
        false,
        true,
        key_id,
        None::<String>,
        Some("aes-gcm"),
        None::<f32>,
        None::<i32>
    )
    .execute(&manager.pool)
    .await
    .unwrap();

    // Try to delete the key (should fail)
    let result = manager.delete_key(key_id).await;
    assert!(matches!(result, Err(StorageError::KeyInUse(_))));

    // Verify key still exists
    let key = sqlx::query!(
        "SELECT COUNT(*) as count FROM encryption_keys WHERE key_id = $1",
        key_id
    )
    .fetch_one(&manager.pool)
    .await
    .unwrap();

    assert_eq!(key.count.unwrap_or(0), 1);
} 