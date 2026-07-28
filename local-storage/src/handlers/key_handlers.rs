use axum::{
    extract::{State, Path},
    http::StatusCode,
    Json,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::{errors::Result, app::AppState};

#[derive(Debug, Deserialize)]
pub struct CreateKeyRequest {
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
    pub description: Option<String>,
}

fn default_algorithm() -> String {
    "aes-gcm".to_string()
}

/// Deliberately omits `key_data` - encryption keys are write-only over the API once created.
#[derive(Debug, Serialize)]
pub struct EncryptionKeyInfo {
    pub key_id: String,
    pub algorithm: String,
    pub description: Option<String>,
    pub is_active: Option<bool>,
    pub created_at: Option<DateTime<Utc>>,
}

impl From<crate::models::EncryptionKey> for EncryptionKeyInfo {
    fn from(k: crate::models::EncryptionKey) -> Self {
        Self {
            key_id: k.key_id,
            algorithm: k.algorithm,
            description: k.description,
            is_active: k.is_active,
            created_at: k.created_at,
        }
    }
}

#[axum::debug_handler]
pub async fn create_key(
    State(state): State<AppState>,
    Json(req): Json<CreateKeyRequest>,
) -> Result<impl IntoResponse> {
    let storage = state.storage.read().await;
    let key = storage.create_encryption_key(&req.algorithm, req.description.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(EncryptionKeyInfo::from(key))))
}

#[axum::debug_handler]
pub async fn list_keys(State(state): State<AppState>) -> Result<impl IntoResponse> {
    let storage = state.storage.read().await;
    let keys = storage.list_encryption_keys().await?;
    let keys: Vec<EncryptionKeyInfo> = keys.into_iter().map(EncryptionKeyInfo::from).collect();
    Ok(Json(keys))
}

#[axum::debug_handler]
pub async fn deactivate_key(
    State(state): State<AppState>,
    Path(key_id): Path<String>,
) -> Result<impl IntoResponse> {
    let storage = state.storage.read().await;
    storage.deactivate_encryption_key(&key_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
