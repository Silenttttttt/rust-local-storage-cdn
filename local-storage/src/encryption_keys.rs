use crate::config::Config;
use crate::errors::{Result, StorageError};
use crate::models::{EncryptionKey, StoredFile};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

pub struct EncryptionKeyManager {
    pool: PgPool,
    config: Arc<Config>,
}

impl EncryptionKeyManager {
    pub fn new(pool: PgPool, config: Arc<Config>) -> Self {
        Self { pool, config }
    }

    /// Create a new encryption key
    pub async fn create_key(
        &self,
        key_id: &str,
        key_data: &[u8],
        algorithm: &str,
        description: Option<&str>,
    ) -> Result<EncryptionKey> {
        let key = sqlx::query_as!(
            EncryptionKey,
            r#"
            INSERT INTO encryption_keys (key_id, key_data, algorithm, description)
            VALUES ($1, $2, $3, $4)
            RETURNING id, key_id, key_data, algorithm, created_at, updated_at, is_active, description
            "#,
            key_id,
            key_data,
            algorithm,
            description
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(key)
    }

    /// Get an encryption key by ID
    pub async fn get_key(&self, key_id: &str) -> Result<EncryptionKey> {
        let key = sqlx::query_as!(
            EncryptionKey,
            r#"
            SELECT id, key_id, key_data, algorithm, created_at, updated_at, is_active, description
            FROM encryption_keys
            WHERE key_id = $1 AND is_active = true
            "#,
            key_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(key)
    }

    /// List all active encryption keys
    pub async fn list_keys(&self) -> Result<Vec<EncryptionKey>> {
        let keys = sqlx::query_as!(
            EncryptionKey,
            r#"
            SELECT id, key_id, key_data, algorithm, created_at, updated_at, is_active, description
            FROM encryption_keys
            WHERE is_active = true
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(keys)
    }

    /// Deactivate an encryption key
    pub async fn deactivate_key(&self, key_id: &str) -> Result<()> {
        sqlx::query!(
            "UPDATE encryption_keys SET is_active = false, updated_at = NOW() WHERE key_id = $1",
            key_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get encryption key for a specific file
    pub async fn get_file_key(&self, file_id: Uuid) -> Result<Option<EncryptionKey>> {
        let file = sqlx::query!(
            "SELECT encryption_key_id FROM files WHERE id = $1",
            file_id
        )
        .fetch_optional(&self.pool)
        .await?;

        match file {
            Some(file) => {
                if let Some(key_id) = &file.encryption_key_id {
                    let key = self.get_key(key_id).await?;
                    Ok(Some(key))
                } else {
                    Ok(None)
                }
            }
            None => Err(StorageError::NotFound { bucket: "unknown".to_string(), key: "unknown".to_string() }),
        }
    }

    /// Update file's encryption key
    pub async fn update_file_key(&self, file_id: Uuid, key_id: Option<&str>) -> Result<()> {
        sqlx::query!(
            "UPDATE files SET encryption_key_id = $1 WHERE id = $2",
            key_id,
            file_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get files using a specific encryption key
    pub async fn get_files_with_key(&self, key_id: &str) -> Result<Vec<StoredFile>> {
        let files = sqlx::query_as!(
            StoredFile,
            r#"
            SELECT *
            FROM files
            WHERE encryption_key_id = $1
            ORDER BY upload_time DESC
            "#,
            key_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(files)
    }

    /// Generate a new key ID
    pub fn generate_key_id() -> String {
        use rand::{thread_rng, Rng};
        use rand::distributions::Alphanumeric;
        
        let mut rng = thread_rng();
        let key_id: String = (0..32)
            .map(|_| rng.sample(Alphanumeric) as char)
            .collect();
        
        key_id
    }

    /// Validate encryption key format
    pub fn validate_key_format(&self, key_data: &[u8], algorithm: &str) -> Result<()> {
        match algorithm {
            "aes-gcm" => {
                if key_data.len() != 32 {
                    return Err(StorageError::Validation(
                        "AES-GCM requires 32-byte key".to_string(),
                    ));
                }
            }
            "chacha20poly1305" => {
                if key_data.len() != 32 {
                    return Err(StorageError::Validation(
                        "ChaCha20-Poly1305 requires 32-byte key".to_string(),
                    ));
                }
            }
            _ => {
                return Err(StorageError::Validation(
                    format!("Unsupported encryption algorithm: {}", algorithm),
                ));
            }
        }
        Ok(())
    }
} 