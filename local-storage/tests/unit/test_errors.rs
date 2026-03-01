use axum::{http::StatusCode, response::IntoResponse};
use local_storage::errors::StorageError;

#[test]
fn test_io_error_creation() {
    let error = StorageError::Io("File not found".to_string());
    assert_eq!(error.to_string(), "IO error: File not found");
}

#[test]
fn test_database_error_creation() {
    let error = StorageError::Database("Connection failed".to_string());
    assert_eq!(error.to_string(), "Database error: Connection failed");
}

#[test]
fn test_redis_error_creation() {
    let error = StorageError::Redis("Connection timeout".to_string());
    assert_eq!(error.to_string(), "Redis error: Connection timeout");
}

#[test]
fn test_encryption_error_creation() {
    let error = StorageError::Encryption("Invalid key".to_string());
    assert_eq!(error.to_string(), "Encryption error: Invalid key");
}

#[test]
fn test_compression_error_creation() {
    let error = StorageError::Compression("Compression failed".to_string());
    assert_eq!(error.to_string(), "Compression error: Compression failed");
}

#[test]
fn test_not_found_error_creation() {
    let error = StorageError::NotFound {
        bucket: "test-bucket".to_string(),
        key: "test-key".to_string(),
    };
    assert_eq!(error.to_string(), "File not found: test-bucket/test-key");
}

#[test]
fn test_bucket_not_found_error() {
    let error = StorageError::BucketNotFound("missing-bucket".to_string());
    assert_eq!(error.to_string(), "Bucket not found: missing-bucket");
}

#[test]
fn test_bad_request_error() {
    let error = StorageError::BadRequest("Invalid parameters".to_string());
    assert_eq!(error.to_string(), "Invalid request: Invalid parameters");
}

#[test]
fn test_internal_error() {
    let error = StorageError::Internal("Something went wrong".to_string());
    assert_eq!(error.to_string(), "Internal error: Something went wrong");
}

#[test]
fn test_json_error() {
    let error = StorageError::Json("Invalid JSON format".to_string());
    assert_eq!(error.to_string(), "JSON error: Invalid JSON format");
}

#[test]
fn test_multipart_error() {
    let error = StorageError::Multipart("Invalid multipart data".to_string());
    assert_eq!(error.to_string(), "Multipart error: Invalid multipart data");
}

#[test]
fn test_migration_error() {
    let error = StorageError::Migration("Migration failed".to_string());
    assert_eq!(error.to_string(), "Migration error: Migration failed");
}

#[tokio::test]
async fn test_error_response_conversion() {
    // Test NotFound error
    let error = StorageError::NotFound {
        bucket: "test".to_string(),
        key: "file.txt".to_string(),
    };
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Test BadRequest error
    let error = StorageError::BadRequest("Bad data".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test Internal errors
    let error = StorageError::Database("DB error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let error = StorageError::Encryption("Crypto error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let error = StorageError::Compression("Compression error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let error = StorageError::Io("IO error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let error = StorageError::Redis("Redis error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let error = StorageError::Json("JSON error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let error = StorageError::Internal("Internal error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let error = StorageError::Migration("Migration error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Test BucketNotFound error
    let error = StorageError::BucketNotFound("bucket".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Test Multipart error (should be BAD_REQUEST)
    let error = StorageError::Multipart("Multipart error".to_string());
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_error_from_std_io_error() {
    let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
    let storage_error: StorageError = io_error.into();
    
    match storage_error {
        StorageError::Io(msg) => assert!(msg.contains("File not found")),
        _ => panic!("Expected IO error"),
    }
}

#[test]
fn test_error_from_serde_json_error() {
    let json_error = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
    let storage_error: StorageError = json_error.into();
    
    match storage_error {
        StorageError::Json(_) => {},
        _ => panic!("Expected JSON error"),
    }
}

#[test]
fn test_error_chain_display() {
    let error = StorageError::Database("Primary error".to_string());
    let error_string = format!("{}", error);
    assert!(error_string.contains("Database error"));
    assert!(error_string.contains("Primary error"));
}

#[test]
fn test_error_debug_format() {
    let error = StorageError::NotFound {
        bucket: "test".to_string(),
        key: "file.txt".to_string(),
    };
    
    let debug_string = format!("{:?}", error);
    assert!(debug_string.contains("NotFound"));
    assert!(debug_string.contains("test"));
    assert!(debug_string.contains("file.txt"));
}

#[test]
fn test_error_clone() {
    let original = StorageError::Encryption("Key error".to_string());
    let cloned = original.clone();
    
    assert_eq!(original.to_string(), cloned.to_string());
}

#[test]
fn test_result_type_alias() {
    fn returns_result() -> local_storage::errors::Result<i32> {
        Ok(42)
    }
    
    fn returns_error() -> local_storage::errors::Result<i32> {
        Err(StorageError::BadRequest("Test error".to_string()))
    }
    
    assert_eq!(returns_result().unwrap(), 42);
    assert!(returns_error().is_err());
}

#[test]
fn test_error_categorization() {
    // Client errors (4xx)
    let client_errors = vec![
        StorageError::NotFound { bucket: "b".to_string(), key: "k".to_string() },
        StorageError::BucketNotFound("bucket".to_string()),
        StorageError::BadRequest("bad".to_string()),
        StorageError::Multipart("multipart".to_string()),
    ];
    
    for error in client_errors {
        let response = error.into_response();
        let status = response.status();
        assert!(status.is_client_error(), "Should be client error: {}", status);
    }
    
    // Server errors (5xx)
    let server_errors = vec![
        StorageError::Io("io".to_string()),
        StorageError::Database("db".to_string()),
        StorageError::Redis("redis".to_string()),
        StorageError::Encryption("crypto".to_string()),
        StorageError::Compression("compression".to_string()),
        StorageError::Json("json".to_string()),
        StorageError::Migration("migration".to_string()),
        StorageError::Internal("internal".to_string()),
    ];
    
    for error in server_errors {
        let response = error.into_response();
        let status = response.status();
        assert!(status.is_server_error(), "Should be server error: {}", status);
    }
} 