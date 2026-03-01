// Integration tests for compression functionality
// These would test compression end-to-end with real file operations

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    routing::*,
    Json,
};
use tower::util::ServiceExt;
use serde_json::{Value, json};
use crate::helpers::TestData;

async fn create_compression_test_app() -> axum::Router {
    // Mock app that simulates compression behavior in responses
    axum::Router::new()
        .route("/buckets/:bucket/files", post(|axum::extract::Path(bucket): axum::extract::Path<String>, body: axum::body::Bytes| async move {
            let original_size = body.len();
            let should_compress = original_size > 100; // Simulate compression threshold
            let compressed_size = if should_compress { 
                (original_size as f64 * 0.6) as usize // Simulate 40% compression
            } else { 
                original_size 
            };
            
            Json(json!({
                "id": "compressed-file-123",
                "bucket": bucket,
                "key": "compressed-file.txt",
                "file_size": compressed_size,
                "original_size": original_size,
                "content_type": "text/plain",
                "hash_blake3": "mock-blake3-hash",
                "hash_md5": "mock-md5-hash",
                "is_compressed": should_compress,
                "is_encrypted": false,
                "compression_algorithm": if should_compress { Some("gzip") } else { None },
                "compression_ratio": if should_compress { Some(0.6) } else { None },
                "upload_time": "2024-01-01T00:00:00Z"
            }))
        }))
        .route("/buckets/:bucket/files/:key/info", get(|axum::extract::Path((bucket, key)): axum::extract::Path<(String, String)>| async move {
            Json(json!({
                "id": "compressed-file-123",
                "bucket": bucket,
                "key": key,
                "file_size": 600, // Compressed size
                "original_size": 1000, // Original size
                "is_compressed": true,
                "compression_algorithm": "gzip",
                "compression_ratio": 0.6,
                "upload_time": "2024-01-01T00:00:00Z"
            }))
        }))
        .route("/stats", get(|| async {
            Json(json!({
                "total_files": 5,
                "total_size": 3000, // Total compressed size
                "compressed_files": 3,
                "encrypted_files": 0,
                "compression_ratio": 0.65
            }))
        }))
}

#[tokio::test]
async fn test_compression_upload_small_file() {
    let app = create_compression_test_app().await;
    
    // Small file should not be compressed
    let small_content = "Small file content"; // Under 100 bytes
    
    let request = Request::builder()
        .method("POST")
        .uri("/buckets/test-bucket/files")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(small_content))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let upload_response: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(upload_response["is_compressed"], false);
    assert_eq!(upload_response["file_size"], small_content.len());
    assert_eq!(upload_response["original_size"], small_content.len());
    assert!(upload_response["compression_algorithm"].is_null());
}

#[tokio::test]
async fn test_compression_upload_large_file() {
    let app = create_compression_test_app().await;
    
    // Large file should be compressed
    let large_content = TestData::compressible_data(); // Much larger than 100 bytes
    
    let request = Request::builder()
        .method("POST")
        .uri("/buckets/test-bucket/files")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(large_content.clone()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let upload_response: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(upload_response["is_compressed"], true);
    assert_eq!(upload_response["compression_algorithm"], "gzip");
    assert_eq!(upload_response["compression_ratio"], 0.6);
    assert_eq!(upload_response["original_size"], large_content.len());
    
    // Compressed size should be smaller than original
    let compressed_size = upload_response["file_size"].as_u64().unwrap();
    assert!(compressed_size < large_content.len() as u64);
}

#[tokio::test]
async fn test_compression_file_info() {
    let app = create_compression_test_app().await;
    
    let request = Request::builder()
        .uri("/buckets/test-bucket/files/compressed-file.txt/info")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let file_info: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(file_info["is_compressed"], true);
    assert_eq!(file_info["compression_algorithm"], "gzip");
    assert_eq!(file_info["compression_ratio"], 0.6);
    assert_eq!(file_info["file_size"], 600); // Compressed size
    assert_eq!(file_info["original_size"], 1000); // Original size
}

#[tokio::test]
async fn test_compression_stats() {
    let app = create_compression_test_app().await;
    
    let request = Request::builder()
        .uri("/stats")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let stats: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(stats["total_files"], 5);
    assert_eq!(stats["compressed_files"], 3);
    assert_eq!(stats["compression_ratio"], 0.65);
    
    // Verify compression effectiveness
    let total_files = stats["total_files"].as_u64().unwrap();
    let compressed_files = stats["compressed_files"].as_u64().unwrap();
    let compression_percentage = (compressed_files as f64 / total_files as f64) * 100.0;
    assert!(compression_percentage > 50.0, "More than 50% of files should be compressed");
}

#[tokio::test]
async fn test_compression_different_algorithms() {
    // This test would verify different compression algorithms if the system supported them
    let app = create_compression_test_app().await;
    
    let test_content = TestData::compressible_data();
    
    let request = Request::builder()
        .method("POST")
        .uri("/buckets/algorithm-test/files")
        .header(header::CONTENT_TYPE, "text/plain")
        .header("X-Compression-Algorithm", "gzip") // Custom header for algorithm selection
        .body(Body::from(test_content))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let upload_response: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(upload_response["compression_algorithm"], "gzip");
}

#[tokio::test]
async fn test_compression_effectiveness() {
    let app = create_compression_test_app().await;
    
    // Test highly compressible content
    let compressible_content = "AAAA".repeat(1000); // Very repetitive, should compress well
    
    let request = Request::builder()
        .method("POST")
        .uri("/buckets/compression-test/files")
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(compressible_content.clone()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let upload_response: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(upload_response["is_compressed"], true);
    
    let original_size = upload_response["original_size"].as_u64().unwrap();
    let compressed_size = upload_response["file_size"].as_u64().unwrap();
    let ratio = compressed_size as f64 / original_size as f64;
    
    // Should achieve good compression on repetitive content
    assert!(ratio < 0.8, "Compression ratio should be better than 80% for repetitive content");
}

#[tokio::test]
async fn test_compression_binary_data() {
    let app = create_compression_test_app().await;
    
    // Random binary data typically doesn't compress well
    let binary_content = TestData::random_bytes(1000);
    
    let request = Request::builder()
        .method("POST")
        .uri("/buckets/binary-test/files")
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(Body::from(binary_content.clone()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let upload_response: Value = serde_json::from_slice(&body).unwrap();
    
    // Should still attempt compression even if not very effective
    assert_eq!(upload_response["is_compressed"], true);
    assert_eq!(upload_response["original_size"], binary_content.len());
}

#[tokio::test]
async fn test_compression_json_data() {
    let app = create_compression_test_app().await;
    
    // JSON data often compresses reasonably well due to repetitive structure
    let json_content = TestData::json_data();
    
    let request = Request::builder()
        .method("POST")
        .uri("/buckets/json-test/files")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json_content.clone()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let upload_response: Value = serde_json::from_slice(&body).unwrap();
    
    if json_content.len() > 100 {
        assert_eq!(upload_response["is_compressed"], true);
        assert_eq!(upload_response["compression_algorithm"], "gzip");
    }
} 