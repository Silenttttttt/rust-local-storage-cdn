export interface StoredFile {
  id: string;
  bucket: string;
  key: string;
  filename: string;
  file_path: string;
  file_size: number;
  content_type: string;
  hash_blake3: string;
  hash_md5: string;
  metadata: any;
  is_compressed: boolean;
  is_encrypted: boolean;
  compression_algorithm: string | null;
  encryption_algorithm: string | null;
  compression_ratio: number | null;
  upload_time: string;
  last_accessed: string | null;
  access_count: number;
}

export interface StorageStats {
  total_files: number;
  total_size: number;
  compressed_files: number;
  encrypted_files: number;
  compression_ratio: number | null;
  last_updated: string;
}

export interface BucketStats {
  total_files: number;
  total_size: number;
  compressed_files: number;
  encrypted_files: number;
  compression_ratio: number | null;
  last_updated: string;
}

export interface FileUploadResponse {
  id: string;
  bucket: string;
  key: string;
  file_size: number;
  content_type: string;
  hash_blake3: string;
  hash_md5: string;
  is_compressed: boolean;
  is_encrypted: boolean;
  compression_ratio: number | null;
  upload_time: string;
}

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

export interface HealthResponse {
  status: string;
  timestamp: string;
} 