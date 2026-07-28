use serde::{Serialize, Deserialize};

pub const XMLNS: &str = "http://s3.amazonaws.com/doc/2006-03-01/";

pub fn to_xml<T: Serialize>(value: &T) -> String {
    let body = quick_xml::se::to_string(value).unwrap_or_default();
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}", body)
}

#[derive(Serialize)]
#[serde(rename = "ListAllMyBucketsResult")]
pub struct ListAllMyBucketsResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Owner")]
    pub owner: Owner,
    #[serde(rename = "Buckets")]
    pub buckets: Buckets,
}

#[derive(Serialize)]
pub struct Owner {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "DisplayName")]
    pub display_name: String,
}

#[derive(Serialize)]
pub struct Buckets {
    #[serde(rename = "Bucket", default)]
    pub bucket: Vec<BucketEntry>,
}

#[derive(Serialize)]
pub struct BucketEntry {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "CreationDate")]
    pub creation_date: String,
}

#[derive(Serialize)]
#[serde(rename = "ListBucketResult")]
pub struct ListBucketResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Prefix")]
    pub prefix: String,
    #[serde(rename = "KeyCount")]
    pub key_count: usize,
    #[serde(rename = "MaxKeys")]
    pub max_keys: i64,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "Contents", default)]
    pub contents: Vec<Content>,
}

#[derive(Serialize)]
pub struct Content {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "Size")]
    pub size: i64,
    #[serde(rename = "StorageClass")]
    pub storage_class: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename = "Delete")]
pub struct DeleteRequest {
    #[serde(rename = "Object", default)]
    pub object: Vec<ObjectIdentifier>,
}

#[derive(Deserialize, Debug)]
pub struct ObjectIdentifier {
    #[serde(rename = "Key")]
    pub key: String,
}

#[derive(Serialize)]
#[serde(rename = "DeleteResult")]
pub struct DeleteResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "Deleted", default)]
    pub deleted: Vec<DeletedEntry>,
    #[serde(rename = "Error", default)]
    pub error: Vec<DeleteErrorEntry>,
}

#[derive(Serialize)]
pub struct DeletedEntry {
    #[serde(rename = "Key")]
    pub key: String,
}

#[derive(Serialize)]
pub struct DeleteErrorEntry {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "Message")]
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename = "CopyObjectResult")]
pub struct CopyObjectResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: String,
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_all_my_buckets_result_has_expected_shape() {
        let result = ListAllMyBucketsResult {
            xmlns: XMLNS.to_string(),
            owner: Owner { id: "local".to_string(), display_name: "local".to_string() },
            buckets: Buckets {
                bucket: vec![BucketEntry { name: "mybucket".to_string(), creation_date: "2024-01-01T00:00:00+00:00".to_string() }],
            },
        };
        let xml = to_xml(&result);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
        assert!(xml.contains("<ListAllMyBucketsResult"));
        assert!(xml.contains(&format!("xmlns=\"{}\"", XMLNS)));
        assert!(xml.contains("<Name>mybucket</Name>"));
        assert!(xml.contains("<Owner>"));
        assert!(xml.contains("<Buckets>"));
    }

    #[test]
    fn delete_request_parses_from_xml() {
        let xml = r#"<Delete><Object><Key>a.txt</Key></Object><Object><Key>b/c.txt</Key></Object></Delete>"#;
        let parsed: DeleteRequest = quick_xml::de::from_str(xml).expect("should parse");
        assert_eq!(parsed.object.len(), 2);
        assert_eq!(parsed.object[0].key, "a.txt");
        assert_eq!(parsed.object[1].key, "b/c.txt");
    }

    #[test]
    fn list_bucket_result_serializes_contents() {
        let result = ListBucketResult {
            xmlns: XMLNS.to_string(),
            name: "mybucket".to_string(),
            prefix: "".to_string(),
            key_count: 1,
            max_keys: 1000,
            is_truncated: false,
            contents: vec![Content {
                key: "file.txt".to_string(),
                last_modified: "2024-01-01T00:00:00+00:00".to_string(),
                etag: "\"abc123\"".to_string(),
                size: 42,
                storage_class: "STANDARD".to_string(),
            }],
        };
        let xml = to_xml(&result);
        assert!(xml.contains("<Key>file.txt</Key>"));
        assert!(xml.contains("<Size>42</Size>"));
        assert!(xml.contains("<IsTruncated>false</IsTruncated>"));
    }
}
