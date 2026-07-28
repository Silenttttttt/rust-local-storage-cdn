// Mirrors local-storage/src/models.rs::FileInfo - the shape actually returned by
// upload/list/search/get_file_info. Previously this type had a `file_path` field that the API
// never returns (that's an internal-only field on the DB's StoredFile) and was missing
// original_size/compression_enabled/encryption_enabled/encryption_key_id, which it does return.
export interface StoredFile {
  id: string;
  bucket: string;
  key: string;
  filename: string;
  file_size: number;
  original_size: number;
  content_type: string;
  hash_blake3: string;
  hash_md5: string;
  metadata: Record<string, unknown> | null;
  is_compressed: boolean;
  is_encrypted: boolean;
  compression_algorithm: string | null;
  encryption_algorithm: string | null;
  compression_ratio: number | null;
  upload_time: string;
  last_accessed: string | null;
  access_count: number;
  compression_enabled: boolean;
  encryption_enabled: boolean;
  encryption_key_id: string | null;
}

// Upload responses use the exact same shape as StoredFile (FileInfo::from(file) server-side).
export type FileUploadResponse = StoredFile;

export interface StorageStats {
  total_files: number;
  total_size: number;
  compressed_files: number;
  encrypted_files: number;
  compression_ratio: number | null;
  last_updated: string;
}

export type BucketStats = StorageStats;

export interface FileListParams {
  bucket: string;
  prefix?: string;
  limit?: number;
  offset?: number;
}

export interface FileSearchParams {
  bucket?: string;
  query: string;
  limit?: number;
}

// Optional per-upload overrides (POST /buckets/:bucket/files?compress=...). Any field left
// undefined falls back to the server's global config default.
export interface UploadOptions {
  compress?: boolean;
  encrypt?: boolean;
  compression_algorithm?: 'zstd' | 'gzip';
  compression_level?: number;
  encryption_key_id?: string;
}

// GET /health returns plain text ("OK"/"UNHEALTHY") with the real signal in the HTTP status
// code (200/503), not a JSON body - there is no HealthResponse JSON shape to parse.
export interface HealthStatus {
  healthy: boolean;
}

// Mirrors key_handlers.rs::EncryptionKeyInfo - deliberately never includes raw key material.
export interface EncryptionKeyInfo {
  key_id: string;
  algorithm: string;
  description: string | null;
  is_active: boolean | null;
  created_at: string | null;
}

export interface CreateKeyRequest {
  algorithm: 'aes-gcm' | 'chacha20poly1305';
  description?: string;
}
