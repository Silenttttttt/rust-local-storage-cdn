use axum::{
    extract::{State, Path, Query},
    http::{StatusCode, HeaderMap, HeaderValue},
    Json,
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::{
    errors::{Result, StorageError},
    models::FileInfo,
    app::{AppState, ListFilesQuery, SearchQuery},
    storage::StoreOptions,
};

use tracing::{info, error};
use chrono;

/// Optional per-upload overrides, e.g. `POST /buckets/x/files?encrypt=true&encryption_key_id=abc`.
/// All fields are additive and default to the service's global config when omitted, so existing
/// callers that don't set them keep behaving exactly as before.
#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    pub compress: Option<bool>,
    pub encrypt: Option<bool>,
    pub compression_algorithm: Option<String>,
    pub compression_level: Option<i32>,
    pub encryption_key_id: Option<String>,
}

fn extract_filename_from_content_disposition(headers: &HeaderMap) -> Option<String> {
    headers
        .get("content-disposition")
        .and_then(|h| {
            // Try to decode as UTF-8 first
            if let Ok(s) = h.to_str() {
                info!("📝 Content-Disposition header (UTF-8): {}", s);
                Some(s.to_string())
            } else {
                // If UTF-8 fails, try to decode as bytes and convert
                let bytes = h.as_bytes();
                let lossy = String::from_utf8_lossy(bytes);
                info!("📝 Content-Disposition header (lossy): {}", lossy);
                lossy.parse().ok()
            }
        })
        .and_then(|s| {
            // Parse the Content-Disposition header
            if s.contains("filename=") {
                // Handle both quoted and unquoted filenames
                // Split by filename= and take the first part after it
                let parts: Vec<&str> = s.split("filename=").collect();
                if parts.len() >= 2 {
                    let filename_part = parts[1].trim();
                    info!("📝 Filename part: {}", filename_part);
                    
                    // Find the end of the filename (either end of string or next semicolon)
                    let filename = if filename_part.starts_with('"') {
                        // Quoted filename - find the closing quote
                        if let Some(end_quote) = filename_part[1..].find('"') {
                            &filename_part[1..end_quote+1]
                        } else {
                            filename_part
                        }
                    } else if filename_part.starts_with('\'') {
                        // Single quoted filename - find the closing quote
                        if let Some(end_quote) = filename_part[1..].find('\'') {
                            &filename_part[1..end_quote+1]
                        } else {
                            filename_part
                        }
                    } else {
                        // Unquoted filename - find the next semicolon or end of string
                        if let Some(semicolon) = filename_part.find(';') {
                            &filename_part[..semicolon]
                        } else {
                            filename_part
                        }
                    };
                    info!("📝 Filename after quote removal: {}", filename);
                    
                    // Handle URL encoding (RFC 5987)
                    if filename.starts_with("UTF-8''") {
                        // Format: UTF-8''encoded-filename
                        let encoded = &filename[7..];
                        info!("📝 UTF-8 encoded filename: {}", encoded);
                        urlencoding::decode(encoded).ok().map(|s| {
                            info!("📝 Decoded UTF-8 filename: {}", s);
                            s.to_string()
                        })
                    } else {
                        // Regular filename - try URL decode first, then use as-is
                        if let Ok(decoded) = urlencoding::decode(filename) {
                            info!("📝 URL decoded filename: {}", decoded);
                            Some(decoded.to_string())
                        } else {
                            // If URL decode fails, use the original filename
                            info!("📝 Using original filename: {}", filename);
                            Some(filename.to_string())
                        }
                    }
                } else {
                    info!("📝 No filename part found in Content-Disposition");
                    None
                }
            } else {
                info!("📝 No filename= in Content-Disposition");
                None
            }
        })
}

#[axum::debug_handler]
pub async fn upload_file(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<UploadQuery>,
    headers: HeaderMap,
    body: axum::body::Body,
) -> Result<impl IntoResponse> {
    info!("📤 Starting file upload to bucket: {}", bucket);

    // Get filename from Content-Disposition header or use timestamp
    let filename = extract_filename_from_content_disposition(&headers)
        .unwrap_or_else(|| {
            format!("file_{}.bin", chrono::Utc::now().timestamp())
        });

    let content_type = headers
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let storage = state.storage.read().await;
    let max_size = storage.max_file_size();

    // Reject obviously-oversized uploads before reading any body bytes.
    if let Some(len) = headers.get("content-length").and_then(|h| h.to_str().ok()).and_then(|s| s.parse::<usize>().ok()) {
        if len > max_size {
            return Err(StorageError::PayloadTooLarge(format!(
                "Content-Length {} exceeds max file size {} bytes", len, max_size
            )));
        }
    }

    // Cap the body read at the configured max file size instead of buffering an unbounded
    // amount of memory - covers chunked bodies without an accurate Content-Length too.
    let bytes = axum::body::to_bytes(body, max_size).await.map_err(|e| {
        error!("❌ Failed to read body (over {} bytes or I/O error): {}", max_size, e);
        if e.to_string().contains("length limit exceeded") {
            StorageError::PayloadTooLarge(format!("Body exceeds max file size {} bytes", max_size))
        } else {
            StorageError::BadRequest(format!("Failed to read body: {}", e))
        }
    })?;

    if bytes.is_empty() {
        error!("❌ No file content provided");
        return Err(StorageError::BadRequest("No file content provided".into()));
    }

    let opts = StoreOptions {
        compress: query.compress,
        encrypt: query.encrypt,
        compression_algorithm: query.compression_algorithm,
        compression_level: query.compression_level,
        encryption_key_id: query.encryption_key_id,
        metadata: None,
    };

    info!("💾 Storing file: {}/{} ({} bytes)", bucket, filename, bytes.len());

    match storage.store_file(&bucket, &filename, bytes.to_vec(), Some(content_type), opts).await {
        Ok(file) => {
            info!("✅ Successfully uploaded file: {}/{} ({} bytes)", bucket, filename, file.file_size);
            Ok((StatusCode::CREATED, Json(FileInfo::from(file))))
        }
        Err(e) => {
            error!("❌ Failed to store file {}/{}: {}", bucket, filename, e);
            Err(e)
        }
    }
}

/// Parses a single-range `Range: bytes=start-end` header (RFC 7233). Only the
/// common single-range forms video players/browsers actually send for
/// seeking are handled - `start-end`, the open-ended `start-`, and the
/// suffix form `-N` (last N bytes). A multi-range request (`bytes=0-9,20-29`)
/// or anything malformed returns `None`, which the caller treats as "serve
/// the whole file" rather than erroring - a client that sent something we
/// don't understand still gets a working (if non-partial) response.
fn parse_range_header(value: &str, total_len: u64) -> Option<(u64, u64)> {
    let spec = value.strip_prefix("bytes=")?;
    if spec.contains(',') {
        return None; // multi-range: not supported, fall back to a full response
    }
    let (start_str, end_str) = spec.split_once('-')?;

    if start_str.is_empty() {
        // Suffix range: "-500" means the last 500 bytes.
        let suffix_len: u64 = end_str.parse().ok()?;
        if suffix_len == 0 || total_len == 0 {
            return None;
        }
        let start = total_len.saturating_sub(suffix_len);
        return Some((start, total_len - 1));
    }

    let start: u64 = start_str.parse().ok()?;
    let end: u64 = if end_str.is_empty() {
        total_len.saturating_sub(1) // open-ended: "500-" means from 500 to EOF
    } else {
        end_str.parse().ok()?
    };
    if start > end || start >= total_len {
        return None;
    }
    Some((start, end.min(total_len.saturating_sub(1))))
}

#[axum::debug_handler]
pub async fn handle_file_request(
    State(state): State<AppState>,
    Path((bucket, path)): Path<(String, String)>,
    request_headers: HeaderMap,
) -> Result<Response> {
    let storage = state.storage.read().await;

    // Check if this is an info request
    let is_info = path.ends_with("/info");
    let key = if is_info {
        path.trim_end_matches("/info").to_string()
    } else {
        path
    };

    info!("📄 File request - bucket: {}, key: {}, is_info: {}", bucket, key, is_info);

    if is_info {
        // Handle file info request
        let info = storage.get_file_info(&bucket, &key).await?;
        info!("✅ File info retrieved - bucket: {}, key: {}, size: {} bytes", bucket, key, info.file_size);
        Ok(Json(info).into_response())
    } else {
        let range_header = request_headers.get("range").and_then(|v| v.to_str().ok());

        // A Range request tries the direct-seek fast path first (get_file_range_if_plain) -
        // reads only the requested bytes straight from disk, skipping the full-file read
        // entirely for the common case (a large plain video file being scrubbed/seeked).
        // Needs the file's total size upfront to validate the range, which get_file_info
        // provides without touching file content at all. Falls through to the existing
        // full-decode path below whenever this doesn't apply: no Range header, the file is
        // compressed/encrypted (get_file_range_if_plain returns None), or the info lookup
        // itself fails for any reason.
        if let Some(range_value) = range_header {
            if let Ok(info) = storage.get_file_info(&bucket, &key).await {
                // original_size, not file_size: Range always refers to the DECODED
                // representation the client receives, not the on-disk stored size -
                // for a compressed file those two differ (file_size is the smaller,
                // compressed size), so validating against file_size here would wrongly
                // reject perfectly valid ranges (or accept invalid ones) whenever
                // compression is in play.
                let total_len = info.original_size as u64;
                match parse_range_header(range_value, total_len) {
                    Some((start, end)) => {
                        if let Ok(Some((slice, content_type, total))) =
                            storage.get_file_range_if_plain(&bucket, &key, start, end).await
                        {
                            let mut headers = HeaderMap::new();
                            headers.insert(
                                "Content-Type",
                                HeaderValue::from_str(&content_type)
                                    .map_err(|_| StorageError::BadRequest("Invalid content type".to_string()))?,
                            );
                            headers.insert("Accept-Ranges", HeaderValue::from_static("bytes"));
                            headers.insert(
                                "Content-Range",
                                HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, total))
                                    .map_err(|_| StorageError::BadRequest("Invalid range".to_string()))?,
                            );
                            info!(
                                "✅ Partial file served (direct seek) - bucket: {}, key: {}, range: {}-{}/{}",
                                bucket, key, start, end, total
                            );
                            return Ok((StatusCode::PARTIAL_CONTENT, headers, slice).into_response());
                        }
                        if let Ok(Some((slice, content_type, total))) =
                            storage.get_compressed_file_range(&bucket, &key, start, end).await
                        {
                            let mut headers = HeaderMap::new();
                            headers.insert(
                                "Content-Type",
                                HeaderValue::from_str(&content_type)
                                    .map_err(|_| StorageError::BadRequest("Invalid content type".to_string()))?,
                            );
                            headers.insert("Accept-Ranges", HeaderValue::from_static("bytes"));
                            headers.insert(
                                "Content-Range",
                                HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, total))
                                    .map_err(|_| StorageError::BadRequest("Invalid range".to_string()))?,
                            );
                            info!(
                                "✅ Partial file served (streamed decompress) - bucket: {}, key: {}, range: {}-{}/{}",
                                bucket, key, start, end, total
                            );
                            return Ok((StatusCode::PARTIAL_CONTENT, headers, slice).into_response());
                        }
                        // Encrypted (or either fast path otherwise declined) - fall
                        // through to the full-decode path below, which re-validates
                        // and slices the range itself using the real decoded length.
                    }
                    None => {
                        // A Range header that doesn't parse into a satisfiable single
                        // range (unsatisfiable bounds, or a multi-range request) gets a
                        // proper 416 only when it at least LOOKED like a bytes= range but
                        // was out of bounds - anything else (unsupported syntax) falls
                        // through to a full 200 response below.
                        if let Some(spec) = range_value.strip_prefix("bytes=") {
                            if !spec.contains(',') {
                                let mut headers = HeaderMap::new();
                                headers.insert("Accept-Ranges", HeaderValue::from_static("bytes"));
                                headers.insert(
                                    "Content-Range",
                                    HeaderValue::from_str(&format!("bytes */{}", total_len))
                                        .map_err(|_| StorageError::BadRequest("Invalid range".to_string()))?,
                                );
                                return Ok((StatusCode::RANGE_NOT_SATISFIABLE, headers).into_response());
                            }
                        }
                    }
                }
            }
        }

        // Full file download (no Range header), or the fallback path for a range request
        // the fast path above didn't handle (compressed/encrypted file, info-lookup
        // failure, or an unsupported Range syntax that still needs a full 200 response).
        let (content, content_type) = storage.get_file(&bucket, &key).await?;
        let total_len = content.len() as u64;

        let mut headers = HeaderMap::new();
        if let Some(ct) = content_type {
            headers.insert("Content-Type", HeaderValue::from_str(&ct).map_err(|_| StorageError::BadRequest("Invalid content type".to_string()))?);
        }
        headers.insert("Accept-Ranges", HeaderValue::from_static("bytes"));

        if let Some(range_value) = range_header {
            if let Some((start, end)) = parse_range_header(range_value, total_len) {
                let slice = content[start as usize..=end as usize].to_vec();
                headers.insert(
                    "Content-Range",
                    HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, total_len))
                        .map_err(|_| StorageError::BadRequest("Invalid range".to_string()))?,
                );
                info!(
                    "✅ Partial file served (full-decode fallback) - bucket: {}, key: {}, range: {}-{}/{}",
                    bucket, key, start, end, total_len
                );
                return Ok((StatusCode::PARTIAL_CONTENT, headers, slice).into_response());
            }
        }

        info!("✅ File downloaded - bucket: {}, key: {}, size: {} bytes", bucket, key, content.len());
        Ok((headers, content).into_response())
    }
}

#[axum::debug_handler]
pub async fn handle_file_delete(
    State(state): State<AppState>,
    Path((bucket, path)): Path<(String, String)>,
) -> Result<impl IntoResponse> {
    let storage = state.storage.read().await;
    
    // Remove /info suffix if present for delete operations
    let key = path.trim_end_matches("/info").to_string();
    
    info!("🗑️ File delete request - bucket: {}, key: {}", bucket, key);
    
    storage.delete_file(&bucket, &key).await?;
    info!("✅ File deleted - bucket: {}, key: {}", bucket, key);
    Ok(StatusCode::NO_CONTENT)
}

#[axum::debug_handler]
pub async fn list_files(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<ListFilesQuery>,
) -> Result<impl IntoResponse> {
    let storage = state.storage.read().await;
    let files = storage.list_files(
        &bucket,
        query.prefix.as_deref(),
        query.limit,
        query.offset,
    ).await?;
    Ok(Json(files))
}

#[axum::debug_handler]
pub async fn search_files(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<impl IntoResponse> {
    let storage = state.storage.read().await;
    let files = storage.search_files(
        query.bucket.as_deref(),
        &query.query,
        query.limit,
    ).await?;
    Ok(Json(files))
}

#[cfg(test)]
mod range_header_tests {
    use super::parse_range_header;

    #[test]
    fn plain_start_end() {
        assert_eq!(parse_range_header("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range_header("bytes=500-999", 1000), Some((500, 999)));
    }

    #[test]
    fn open_ended() {
        assert_eq!(parse_range_header("bytes=900-", 1000), Some((900, 999)));
    }

    #[test]
    fn suffix_range() {
        assert_eq!(parse_range_header("bytes=-500", 1000), Some((500, 999)));
        assert_eq!(parse_range_header("bytes=-2000", 1000), Some((0, 999))); // longer than the file: clamp to the whole thing
    }

    #[test]
    fn end_beyond_total_is_clamped() {
        assert_eq!(parse_range_header("bytes=0-9999", 1000), Some((0, 999)));
    }

    #[test]
    fn unsatisfiable_start_returns_none() {
        assert_eq!(parse_range_header("bytes=1000-1999", 1000), None);
        assert_eq!(parse_range_header("bytes=2000-3000", 1000), None);
    }

    #[test]
    fn inverted_range_returns_none() {
        assert_eq!(parse_range_header("bytes=500-100", 1000), None);
    }

    #[test]
    fn multi_range_unsupported() {
        assert_eq!(parse_range_header("bytes=0-99,200-299", 1000), None);
    }

    #[test]
    fn malformed_returns_none() {
        assert_eq!(parse_range_header("not-a-range", 1000), None);
        assert_eq!(parse_range_header("bytes=abc-def", 1000), None);
        assert_eq!(parse_range_header("bytes=", 1000), None);
    }

    #[test]
    fn zero_length_file() {
        assert_eq!(parse_range_header("bytes=0-99", 0), None);
        assert_eq!(parse_range_header("bytes=-10", 0), None);
    }
} 
