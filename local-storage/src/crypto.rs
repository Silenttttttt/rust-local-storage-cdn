use crate::config::CryptoConfig;
use crate::errors::{Result, StorageError};
use crate::models::StoredFile;
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use chacha20poly1305::{ChaCha20Poly1305, Key as ChaChaKey, Nonce as ChaChaNonce};
use rand::RngCore;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EncryptionAlgorithm {
    AesGcm,
    ChaCha20Poly1305,
}

impl EncryptionAlgorithm {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AesGcm => "aes-gcm",
            Self::ChaCha20Poly1305 => "chacha20poly1305",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "aes-gcm" => Ok(EncryptionAlgorithm::AesGcm),
            "chacha20poly1305" => Ok(EncryptionAlgorithm::ChaCha20Poly1305),
            _ => Err(StorageError::InvalidEncryptionAlgorithm(format!("Unsupported algorithm: {}", s))),
        }
    }
}

pub struct CryptoManager {
    config: Arc<CryptoConfig>,
    aes_key: Option<Aes256Gcm>,
    chacha_key: Option<ChaCha20Poly1305>,
}

impl CryptoManager {
    pub fn new(config: Arc<CryptoConfig>) -> Result<Self> {
        let mut manager = CryptoManager {
            config: config.clone(),
            aes_key: None,
            chacha_key: None,
        };

        if config.enabled {
            manager.init_cipher()?;
        }

        Ok(manager)
    }

    fn init_cipher(&mut self) -> Result<()> {
        let key_bytes = match &self.config.key {
            Some(key_str) => {
                // Decode hex key or use as-is if 32 bytes
                if key_str.len() == 64 {
                    hex::decode(key_str).map_err(|e| {
                        StorageError::Encryption(format!("Invalid hex key: {}", e))
                    })?
                } else if key_str.len() == 32 {
                    key_str.as_bytes().to_vec()
                } else {
                    return Err(StorageError::Encryption(
                        "Key must be 32 bytes or 64 hex characters".to_string(),
                    ));
                }
            }
            None => {
                // Generate random key
                let mut key = [0u8; 32];
                OsRng.fill_bytes(&mut key);
                tracing::warn!("🔑 Generated random encryption key - files encrypted with this key will be unrecoverable after restart!");
                key.to_vec()
            }
        };

        if key_bytes.len() != 32 {
            return Err(StorageError::Encryption(
                "Key must be exactly 32 bytes".to_string(),
            ));
        }

        match self.config.algorithm.as_str() {
            "aes-gcm" => {
                let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
                self.aes_key = Some(Aes256Gcm::new(key));
            }
            "chacha20poly1305" => {
                let key = ChaChaKey::from_slice(&key_bytes);
                self.chacha_key = Some(ChaCha20Poly1305::new(key));
            }
            _ => {
                return Err(StorageError::Encryption(format!(
                    "Unsupported encryption algorithm: {}",
                    self.config.algorithm
                )));
            }
        }

        Ok(())
    }

    pub async fn encrypt(&self, data: &[u8], algorithm: EncryptionAlgorithm) -> Result<Vec<u8>> {
        if !self.config.enabled {
            return Ok(data.to_vec());
        }

        match algorithm {
            EncryptionAlgorithm::AesGcm => self.encrypt_aes(data),
            EncryptionAlgorithm::ChaCha20Poly1305 => self.encrypt_chacha(data),
        }
    }

    pub async fn decrypt(&self, data: &[u8], algorithm: EncryptionAlgorithm) -> Result<Vec<u8>> {
        if !self.config.enabled {
            return Ok(data.to_vec());
        }

        match algorithm {
            EncryptionAlgorithm::AesGcm => self.decrypt_aes(data),
            EncryptionAlgorithm::ChaCha20Poly1305 => self.decrypt_chacha(data),
        }
    }

    pub async fn encrypt_file(&self, file: &StoredFile, data: &[u8]) -> Result<Vec<u8>> {
        let algorithm = EncryptionAlgorithm::from_str(
            file.encryption_algorithm.as_deref().unwrap_or("aes-gcm")
        )?;
        self.encrypt(data, algorithm).await
    }

    pub async fn decrypt_file(&self, file: &StoredFile, data: &[u8]) -> Result<Vec<u8>> {
        let algorithm = EncryptionAlgorithm::from_str(
            file.encryption_algorithm.as_deref().unwrap_or("aes-gcm")
        )?;
        self.decrypt(data, algorithm).await
    }

    fn encrypt_aes(&self, data: &[u8]) -> Result<Vec<u8>> {
        let cipher = self.aes_key.as_ref().ok_or_else(|| {
            StorageError::Encryption("AES cipher not initialized".to_string())
        })?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt data
        let ciphertext = cipher.encrypt(nonce, data).map_err(|e| {
            StorageError::Encryption(format!("AES encryption failed: {}", e))
        })?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    fn decrypt_aes(&self, encrypted_data: &[u8]) -> Result<Vec<u8>> {
        if encrypted_data.len() < 12 {
            return Err(StorageError::Encryption(
                "Invalid encrypted data: too short".to_string(),
            ));
        }

        let cipher = self.aes_key.as_ref().ok_or_else(|| {
            StorageError::Encryption("AES cipher not initialized".to_string())
        })?;

        // Extract nonce and ciphertext
        let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        // Decrypt data
        cipher.decrypt(nonce, ciphertext).map_err(|e| {
            StorageError::Encryption(format!("AES decryption failed: {}", e))
        })
    }

    fn encrypt_chacha(&self, data: &[u8]) -> Result<Vec<u8>> {
        let cipher = self.chacha_key.as_ref().ok_or_else(|| {
            StorageError::Encryption("ChaCha20 cipher not initialized".to_string())
        })?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = ChaChaNonce::from_slice(&nonce_bytes);

        // Encrypt data
        let ciphertext = cipher.encrypt(nonce, data).map_err(|e| {
            StorageError::Encryption(format!("ChaCha20 encryption failed: {}", e))
        })?;

        // Prepend nonce to ciphertext
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    fn decrypt_chacha(&self, encrypted_data: &[u8]) -> Result<Vec<u8>> {
        if encrypted_data.len() < 12 {
            return Err(StorageError::Encryption(
                "Invalid encrypted data: too short".to_string(),
            ));
        }

        let cipher = self.chacha_key.as_ref().ok_or_else(|| {
            StorageError::Encryption("ChaCha20 cipher not initialized".to_string())
        })?;

        // Extract nonce and ciphertext
        let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
        let nonce = ChaChaNonce::from_slice(nonce_bytes);

        // Decrypt data
        cipher.decrypt(nonce, ciphertext).map_err(|e| {
            StorageError::Encryption(format!("ChaCha20 decryption failed: {}", e))
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn algorithm(&self) -> &str {
        &self.config.algorithm
    }
} 