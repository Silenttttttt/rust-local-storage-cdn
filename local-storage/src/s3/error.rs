use axum::{
    response::{IntoResponse, Response},
    http::StatusCode,
};
use serde::Serialize;

/// S3-style error: XML body with Code/Message/Resource/RequestId, matching what real S3 clients
/// (boto3, aws-cli, rclone, mc) parse to raise their own typed exceptions.
#[derive(Debug, Clone)]
pub struct S3Error {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub resource: String,
}

#[derive(Serialize)]
#[serde(rename = "Error")]
struct ErrorBody {
    #[serde(rename = "Code")]
    code: String,
    #[serde(rename = "Message")]
    message: String,
    #[serde(rename = "Resource")]
    resource: String,
    #[serde(rename = "RequestId")]
    request_id: String,
}

impl S3Error {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>, resource: impl Into<String>) -> Self {
        Self { status, code, message: message.into(), resource: resource.into() }
    }

    pub fn no_such_bucket(bucket: &str) -> Self {
        Self::new(StatusCode::NOT_FOUND, "NoSuchBucket", "The specified bucket does not exist", format!("/{}", bucket))
    }

    pub fn no_such_key(bucket: &str, key: &str) -> Self {
        Self::new(StatusCode::NOT_FOUND, "NoSuchKey", "The specified key does not exist", format!("/{}/{}", bucket, key))
    }

    pub fn access_denied(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "AccessDenied", message, "")
    }

    pub fn signature_does_not_match() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "SignatureDoesNotMatch",
            "The request signature we calculated does not match the signature you provided",
            "",
        )
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "InvalidRequest", message, "")
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "InternalError", message, "")
    }
}

impl IntoResponse for S3Error {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            code: self.code.to_string(),
            message: self.message,
            resource: self.resource,
            request_id: uuid::Uuid::new_v4().to_string(),
        };
        let xml = crate::s3::xml::to_xml(&body);
        (self.status, [(axum::http::header::CONTENT_TYPE, "application/xml")], xml).into_response()
    }
}

impl From<crate::errors::StorageError> for S3Error {
    fn from(e: crate::errors::StorageError) -> Self {
        use crate::errors::StorageError::*;
        match e {
            NotFound { bucket, key } => S3Error::no_such_key(&bucket, &key),
            InvalidBucket(msg) => S3Error::new(StatusCode::NOT_FOUND, "NoSuchBucket", msg.clone(), msg),
            AlreadyExists { bucket, key } => Self::new(
                StatusCode::CONFLICT,
                "KeyAlreadyExists",
                format!("Object already exists: {}/{}", bucket, key),
                format!("/{}/{}", bucket, key),
            ),
            InvalidKey(msg) | Validation(msg) | BadRequest(msg) => S3Error::invalid_request(msg),
            PayloadTooLarge(msg) => Self::new(StatusCode::PAYLOAD_TOO_LARGE, "EntityTooLarge", msg, ""),
            other => S3Error::internal(other.to_string()),
        }
    }
}
