use axum::{
    body::Bytes,
    extract::{OriginalUri, Path, Query, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
};
use std::collections::HashMap;

use crate::{
    app::AppState,
    storage::StoreOptions,
    s3::{
        auth::verify_sigv4,
        error::S3Error,
        xml::{self, to_xml},
    },
};

/// Verifies SigV4 for a /v2 request. Every handler calls this first, using the untouched
/// original URI (routes are nested under /v2, and axum's plain `Uri` extractor would otherwise
/// see the path with that prefix stripped - but the client signed the full path it actually
/// sent, so `OriginalUri` is what has to be used here).
fn require_s3_config(state: &AppState) -> std::result::Result<std::sync::Arc<crate::config::S3Config>, S3Error> {
    state
        .s3_config
        .clone()
        .ok_or_else(|| S3Error::new(StatusCode::NOT_IMPLEMENTED, "NotImplemented", "S3 v2 API is not configured", ""))
}

fn verify(
    state: &AppState,
    headers: &HeaderMap,
    method: &Method,
    uri: &axum::http::Uri,
    body: &[u8],
) -> std::result::Result<(), S3Error> {
    let s3_config = require_s3_config(state)?;
    verify_sigv4(headers, method, uri, body, &s3_config)
}

fn etag(hash_md5: &str) -> String {
    format!("\"{}\"", hash_md5)
}

// ---- Buckets ----

pub async fn list_buckets(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, S3Error> {
    verify(&state, &headers, &method, &uri, &body)?;

    let storage = state.storage.read().await;
    let buckets = storage.list_buckets_with_created_at().await.map_err(S3Error::from)?;

    let result = xml::ListAllMyBucketsResult {
        xmlns: xml::XMLNS.to_string(),
        owner: xml::Owner { id: "local".to_string(), display_name: "local".to_string() },
        buckets: xml::Buckets {
            bucket: buckets
                .into_iter()
                .map(|(name, created_at)| xml::BucketEntry { name, creation_date: created_at.to_rfc3339() })
                .collect(),
        },
    };

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml")],
        to_xml(&result),
    )
        .into_response())
}

pub async fn create_bucket(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    Path(bucket): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, S3Error> {
    verify(&state, &headers, &method, &uri, &body)?;

    let storage = state.storage.read().await;
    storage.create_bucket(&bucket).await.map_err(S3Error::from)?;

    Ok((StatusCode::OK, [(header::LOCATION, format!("/{}", bucket))]).into_response())
}

pub async fn delete_bucket(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    Path(bucket): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, S3Error> {
    verify(&state, &headers, &method, &uri, &body)?;

    let storage = state.storage.read().await;
    storage.delete_bucket(&bucket).await.map_err(S3Error::from)?;

    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Debug, serde::Deserialize)]
pub struct ListObjectsQuery {
    prefix: Option<String>,
    #[serde(rename = "max-keys")]
    max_keys: Option<i64>,
}

pub async fn list_objects_v2(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    Path(bucket): Path<String>,
    Query(query): Query<ListObjectsQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, S3Error> {
    verify(&state, &headers, &method, &uri, &body)?;

    let max_keys = query.max_keys.unwrap_or(1000).clamp(1, 1000);
    let storage = state.storage.read().await;
    // Fetch one extra to know whether the result was truncated.
    let mut files = storage
        .list_files(&bucket, query.prefix.as_deref(), Some(max_keys + 1), Some(0))
        .await
        .map_err(S3Error::from)?;

    let is_truncated = files.len() as i64 > max_keys;
    files.truncate(max_keys as usize);

    let result = xml::ListBucketResult {
        xmlns: xml::XMLNS.to_string(),
        name: bucket,
        prefix: query.prefix.unwrap_or_default(),
        key_count: files.len(),
        max_keys,
        is_truncated,
        contents: files
            .into_iter()
            .map(|f| xml::Content {
                key: f.key,
                last_modified: f.upload_time.to_rfc3339(),
                etag: etag(&f.hash_md5),
                size: f.file_size,
                storage_class: "STANDARD".to_string(),
            })
            .collect(),
    };

    Ok((StatusCode::OK, [(header::CONTENT_TYPE, "application/xml")], to_xml(&result)).into_response())
}

// ---- Objects ----

pub async fn put_object(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, S3Error> {
    verify(&state, &headers, &method, &uri, &body)?;

    let storage = state.storage.read().await;

    // CopyObject shares the PutObject route/method in the real S3 API, disambiguated by this header.
    if let Some(source) = headers.get("x-amz-copy-source").and_then(|v| v.to_str().ok()) {
        let source = source.trim_start_matches('/');
        let (src_bucket, src_key) = source
            .split_once('/')
            .ok_or_else(|| S3Error::invalid_request("Invalid x-amz-copy-source"))?;

        let (content, content_type) = storage.get_file(src_bucket, src_key).await.map_err(S3Error::from)?;
        let file = storage
            .store_file(&bucket, &key, content, content_type, StoreOptions::default())
            .await
            .map_err(S3Error::from)?;

        let result = xml::CopyObjectResult {
            xmlns: xml::XMLNS.to_string(),
            etag: etag(&file.hash_md5),
            last_modified: file.upload_time.map(|t| t.to_rfc3339()).unwrap_or_default(),
        };
        return Ok((StatusCode::OK, [(header::CONTENT_TYPE, "application/xml")], to_xml(&result)).into_response());
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let file = storage
        .store_file(&bucket, &key, body.to_vec(), content_type, StoreOptions::default())
        .await
        .map_err(S3Error::from)?;

    Ok((StatusCode::OK, [(header::ETAG, etag(&file.hash_md5))]).into_response())
}

pub async fn get_object(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, S3Error> {
    verify(&state, &headers, &method, &uri, &body)?;

    let storage = state.storage.read().await;
    let (content, content_type) = storage.get_file(&bucket, &key).await.map_err(S3Error::from)?;
    let info = storage.get_file_info(&bucket, &key).await.map_err(S3Error::from)?;

    let mut response_headers = HeaderMap::new();
    if let Some(ct) = content_type {
        if let Ok(v) = ct.parse() {
            response_headers.insert(header::CONTENT_TYPE, v);
        }
    }
    if let Ok(v) = etag(&info.hash_md5).parse() {
        response_headers.insert(header::ETAG, v);
    }

    Ok((StatusCode::OK, response_headers, content).into_response())
}

pub async fn head_object(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, S3Error> {
    verify(&state, &headers, &method, &uri, &body)?;

    let storage = state.storage.read().await;
    let info = storage.get_file_info(&bucket, &key).await.map_err(S3Error::from)?;

    let mut response_headers = HeaderMap::new();
    if let Ok(v) = info.content_type.parse() {
        response_headers.insert(header::CONTENT_TYPE, v);
    }
    if let Ok(v) = etag(&info.hash_md5).parse() {
        response_headers.insert(header::ETAG, v);
    }
    if let Ok(v) = header::HeaderValue::from_str(&info.file_size.to_string()) {
        response_headers.insert(header::CONTENT_LENGTH, v);
    }

    Ok((StatusCode::OK, response_headers).into_response())
}

pub async fn delete_object(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, S3Error> {
    verify(&state, &headers, &method, &uri, &body)?;

    let storage = state.storage.read().await;
    // S3's DeleteObject is idempotent: deleting an already-absent key is still a success.
    match storage.delete_file(&bucket, &key).await {
        Ok(()) | Err(crate::errors::StorageError::NotFound { .. }) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(e) => Err(S3Error::from(e)),
    }
}

/// Handles both `DELETE /v2/{bucket}?delete` (batch delete) and rejects any other POST.
pub async fn bucket_post(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    Path(bucket): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> std::result::Result<Response, S3Error> {
    verify(&state, &headers, &method, &uri, &body)?;

    if !query.contains_key("delete") {
        return Err(S3Error::invalid_request("Unsupported POST operation on bucket"));
    }

    let request: xml::DeleteRequest = quick_xml::de::from_str(
        std::str::from_utf8(&body).map_err(|_| S3Error::invalid_request("Delete request body is not valid UTF-8"))?,
    )
    .map_err(|e| S3Error::invalid_request(format!("Invalid Delete XML body: {}", e)))?;

    let storage = state.storage.read().await;
    let mut deleted = Vec::new();
    let mut errors = Vec::new();

    for object in request.object {
        match storage.delete_file(&bucket, &object.key).await {
            Ok(()) | Err(crate::errors::StorageError::NotFound { .. }) => {
                deleted.push(xml::DeletedEntry { key: object.key });
            }
            Err(e) => {
                errors.push(xml::DeleteErrorEntry {
                    key: object.key,
                    code: "InternalError".to_string(),
                    message: e.to_string(),
                });
            }
        }
    }

    let result = xml::DeleteResult { xmlns: xml::XMLNS.to_string(), deleted, error: errors };
    Ok((StatusCode::OK, [(header::CONTENT_TYPE, "application/xml")], to_xml(&result)).into_response())
}
