use axum::http::{HeaderMap, Method, StatusCode, Uri};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::{config::S3Config, s3::error::S3Error};

type HmacSha256 = Hmac<Sha256>;

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts a key of any length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

struct ParsedAuth {
    access_key: String,
    date: String,
    region: String,
    signed_headers: Vec<String>,
    signature: String,
}

fn parse_authorization(header: &str) -> Option<ParsedAuth> {
    let header = header.strip_prefix("AWS4-HMAC-SHA256 ")?;

    let mut credential = None;
    let mut signed_headers = None;
    let mut signature = None;
    for part in header.split(',') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("Credential=") {
            credential = Some(v);
        } else if let Some(v) = part.strip_prefix("SignedHeaders=") {
            signed_headers = Some(v);
        } else if let Some(v) = part.strip_prefix("Signature=") {
            signature = Some(v);
        }
    }

    let mut cred_parts = credential?.splitn(5, '/');
    let access_key = cred_parts.next()?.to_string();
    let date = cred_parts.next()?.to_string();
    let region = cred_parts.next()?.to_string();
    cred_parts.next()?; // service, always "s3" for our purposes
    cred_parts.next()?; // "aws4_request"

    let mut signed_headers: Vec<String> = signed_headers?.split(';').map(|s| s.to_lowercase()).collect();
    signed_headers.sort();

    Some(ParsedAuth {
        access_key,
        date,
        region,
        signed_headers,
        signature: signature?.to_string(),
    })
}

/// AWS SigV4 URI encoding: unreserved characters pass through unencoded, everything else is
/// percent-encoded with uppercase hex. `preserve_slash` keeps '/' literal, used for path
/// segments but not query-string components.
fn uri_encode(input: &str, preserve_slash: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        let c = byte as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else if preserve_slash && c == '/' {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

fn decode_or_original(s: &str) -> String {
    urlencoding::decode(s).map(|c| c.into_owned()).unwrap_or_else(|_| s.to_string())
}

fn canonical_uri(path: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    path.split('/')
        .map(|segment| uri_encode(&decode_or_original(segment), false))
        .collect::<Vec<_>>()
        .join("/")
}

fn canonical_query_string(query: &str) -> String {
    if query.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<(String, String)> = query
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = decode_or_original(it.next().unwrap_or(""));
            let v = decode_or_original(it.next().unwrap_or(""));
            (k, v)
        })
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(k, v)| format!("{}={}", uri_encode(&k, false), uri_encode(&v, false)))
        .collect::<Vec<_>>()
        .join("&")
}

fn canonical_headers(headers: &HeaderMap, sorted_signed_header_names: &[String]) -> String {
    let mut lines = Vec::with_capacity(sorted_signed_header_names.len());
    for name in sorted_signed_header_names {
        let value = headers
            .get(name.as_str())
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let collapsed = value.trim().split_whitespace().collect::<Vec<_>>().join(" ");
        lines.push(format!("{}:{}", name, collapsed));
    }
    let mut result = lines.join("\n");
    result.push('\n');
    result
}

fn canonical_request(
    method: &Method,
    uri: &Uri,
    headers: &HeaderMap,
    sorted_signed_headers: &[String],
    hashed_payload: &str,
) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method.as_str(),
        canonical_uri(uri.path()),
        canonical_query_string(uri.query().unwrap_or("")),
        canonical_headers(headers, sorted_signed_headers),
        sorted_signed_headers.join(";"),
        hashed_payload,
    )
}

fn signing_key(secret_key: &str, date: &str, region: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{}", secret_key).as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    hmac_sha256(&k_service, b"aws4_request")
}

/// Verifies an incoming request's AWS SigV4 `Authorization` header against the single
/// configured access/secret key pair. `uri` must be the *original*, pre-route-stripping URI
/// (e.g. axum's `OriginalUri`) since that's what the client actually signed.
pub fn verify_sigv4(
    headers: &HeaderMap,
    method: &Method,
    uri: &Uri,
    body: &[u8],
    s3_config: &S3Config,
) -> std::result::Result<(), S3Error> {
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| S3Error::access_denied("Missing Authorization header"))?;

    let parsed = parse_authorization(auth_header)
        .ok_or_else(|| S3Error::invalid_request("Malformed Authorization header"))?;

    if parsed.access_key != s3_config.access_key {
        return Err(S3Error::signature_does_not_match());
    }

    let amz_date = headers
        .get("x-amz-date")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| S3Error::invalid_request("Missing x-amz-date header"))?;

    let request_time = chrono::NaiveDateTime::parse_from_str(amz_date, "%Y%m%dT%H%M%SZ")
        .map_err(|_| S3Error::invalid_request("Invalid x-amz-date"))?;
    let request_time = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(request_time, chrono::Utc);
    let skew_seconds = (chrono::Utc::now() - request_time).num_seconds().abs();
    if skew_seconds > 900 {
        return Err(S3Error::new(
            StatusCode::FORBIDDEN,
            "RequestTimeTooSkewed",
            "The difference between the request time and the current time is too large",
            uri.path().to_string(),
        ));
    }

    let hashed_payload = match headers.get("x-amz-content-sha256").and_then(|v| v.to_str().ok()) {
        Some("UNSIGNED-PAYLOAD") => "UNSIGNED-PAYLOAD".to_string(),
        Some(v) if v.starts_with("STREAMING-") => {
            return Err(S3Error::new(
                StatusCode::NOT_IMPLEMENTED,
                "NotImplemented",
                "Chunked/streaming signed payloads (STREAMING-AWS4-HMAC-SHA256-PAYLOAD) are not supported yet",
                uri.path().to_string(),
            ));
        }
        Some(declared) => {
            let actual = sha256_hex(body);
            if declared != actual {
                return Err(S3Error::signature_does_not_match());
            }
            actual
        }
        None => sha256_hex(body),
    };

    let canonical = canonical_request(method, uri, headers, &parsed.signed_headers, &hashed_payload);
    let credential_scope = format!("{}/{}/s3/aws4_request", parsed.date, parsed.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        credential_scope,
        sha256_hex(canonical.as_bytes()),
    );

    let signing_key = signing_key(&s3_config.secret_key, &parsed.date, &parsed.region);
    let computed_signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    if !constant_time_eq(computed_signature.as_bytes(), parsed.signature.as_bytes()) {
        return Err(S3Error::signature_does_not_match());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    /// Mirrors what a real SigV4 client does, to build a correctly-signed synthetic request for
    /// testing `verify_sigv4` end-to-end without depending on a hand-copied external test vector.
    fn client_sign(
        method: &Method,
        uri: &Uri,
        headers: &mut HeaderMap,
        body: &[u8],
        access_key: &str,
        secret_key: &str,
        region: &str,
        date: &str,      // YYYYMMDD
        amz_date: &str,  // YYYYMMDDTHHMMSSZ
    ) {
        let hashed_payload = sha256_hex(body);
        headers.insert("x-amz-date", HeaderValue::from_str(amz_date).unwrap());
        headers.insert("x-amz-content-sha256", HeaderValue::from_str(&hashed_payload).unwrap());

        let mut signed_header_names: Vec<String> = headers.keys().map(|k| k.as_str().to_lowercase()).collect();
        signed_header_names.sort();

        let canonical = canonical_request(method, uri, headers, &signed_header_names, &hashed_payload);
        let credential_scope = format!("{}/{}/s3/aws4_request", date, region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date,
            credential_scope,
            sha256_hex(canonical.as_bytes()),
        );
        let key = signing_key(secret_key, date, region);
        let signature = hex::encode(hmac_sha256(&key, string_to_sign.as_bytes()));

        let auth = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            access_key, credential_scope, signed_header_names.join(";"), signature
        );
        headers.insert(axum::http::header::AUTHORIZATION, HeaderValue::from_str(&auth).unwrap());
    }

    fn test_config() -> S3Config {
        S3Config {
            access_key: "AKIATESTACCESSKEY".to_string(),
            secret_key: "test-secret-key-value".to_string(),
            region: "us-east-1".to_string(),
        }
    }

    #[test]
    fn accepts_correctly_signed_request() {
        let cfg = test_config();
        let uri: Uri = "/v2/mybucket/path/to/object.txt".parse().unwrap();
        let method = Method::PUT;
        let body = b"hello world".to_vec();

        // The skew check compares x-amz-date against real "now", so sign with the current time
        // to keep the test stable regardless of when it runs.
        let now = chrono::Utc::now();
        let date = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("localhost:9000"));
        client_sign(&method, &uri, &mut headers, &body, &cfg.access_key, &cfg.secret_key, &cfg.region, &date, &amz_date);

        let result = verify_sigv4(&headers, &method, &uri, &body, &cfg);
        assert!(result.is_ok(), "expected valid signature to verify, got {:?}", result.err());
    }

    #[test]
    fn rejects_tampered_body() {
        let cfg = test_config();
        let uri: Uri = "/v2/mybucket/object.txt".parse().unwrap();
        let method = Method::PUT;
        let body = b"original body".to_vec();

        let now = chrono::Utc::now();
        let date = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("localhost:9000"));
        client_sign(&method, &uri, &mut headers, &body, &cfg.access_key, &cfg.secret_key, &cfg.region, &date, &amz_date);

        let tampered_body = b"tampered body!!".to_vec();
        let result = verify_sigv4(&headers, &method, &uri, &tampered_body, &cfg);
        assert!(result.is_err(), "tampered body must not verify");
    }

    #[test]
    fn rejects_wrong_secret_key() {
        let cfg = test_config();
        let uri: Uri = "/v2/mybucket/object.txt".parse().unwrap();
        let method = Method::GET;
        let body: Vec<u8> = vec![];

        let now = chrono::Utc::now();
        let date = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("localhost:9000"));
        client_sign(&method, &uri, &mut headers, &body, &cfg.access_key, "wrong-secret", &cfg.region, &date, &amz_date);

        let result = verify_sigv4(&headers, &method, &uri, &body, &cfg);
        assert!(result.is_err(), "signature made with wrong secret key must not verify");
    }

    #[test]
    fn rejects_stale_timestamp() {
        let cfg = test_config();
        let uri: Uri = "/v2/mybucket/object.txt".parse().unwrap();
        let method = Method::GET;
        let body: Vec<u8> = vec![];

        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("localhost:9000"));
        // Clearly outside the 15 minute skew window.
        client_sign(&method, &uri, &mut headers, &body, &cfg.access_key, &cfg.secret_key, &cfg.region, "20200101", "20200101T000000Z");

        let result = verify_sigv4(&headers, &method, &uri, &body, &cfg);
        assert!(result.is_err(), "stale timestamp must be rejected");
    }
}
