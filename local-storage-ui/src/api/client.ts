import axios from 'axios';
import {
  BucketStats, FileListParams, FileSearchParams, FileUploadResponse, StorageStats, StoredFile,
  UploadOptions, EncryptionKeyInfo, CreateKeyRequest,
} from '../types/api';

const getApiUrl = (): string => {
  if (typeof window !== 'undefined' && window.APP_CONFIG?.API_URL) {
    return window.APP_CONFIG.API_URL;
  }
  return process.env.REACT_APP_API_URL || 'http://localhost:8080';
};

const API_URL = getApiUrl();

const api = axios.create({
  baseURL: API_URL,
  headers: {
    'Content-Type': 'application/json',
  },
});

/**
 * Uploads a file. `key` is the full object key including any folder prefix
 * (e.g. "photos/2024/img.jpg") - it's carried entirely in the Content-Disposition filename,
 * since that's the only place the backend reads it from. A previous version of this client also
 * sent a `?path=` query parameter with the same value, which the backend has never read; files
 * uploaded while "inside" a folder silently landed at the bucket root instead.
 */
export const uploadFile = async (
  bucket: string,
  file: File,
  key: string,
  options?: UploadOptions,
): Promise<FileUploadResponse> => {
  const headers = {
    'Content-Type': file.type || 'application/octet-stream',
    'Content-Disposition': `attachment; filename="${encodeURIComponent(key)}"`,
  };

  const params: Record<string, string> = {};
  if (options?.compress !== undefined) params.compress = String(options.compress);
  if (options?.encrypt !== undefined) params.encrypt = String(options.encrypt);
  if (options?.compression_algorithm) params.compression_algorithm = options.compression_algorithm;
  if (options?.compression_level !== undefined) params.compression_level = String(options.compression_level);
  if (options?.encryption_key_id) params.encryption_key_id = options.encryption_key_id;

  const response = await api.post<FileUploadResponse>(`/buckets/${bucket}/files`, file, {
    headers,
    params,
    timeout: 300000, // 5 minutes for large files
  });
  return response.data;
};

export const listFiles = async (params: FileListParams): Promise<StoredFile[]> => {
  const { bucket, prefix, limit = 100, offset = 0 } = params;
  const response = await api.get<StoredFile[]>(`/buckets/${bucket}/files`, {
    params: { prefix, limit, offset },
  });
  return response.data;
};

export const searchFiles = async (params: FileSearchParams): Promise<StoredFile[]> => {
  const { bucket, query, limit = 100 } = params;
  const response = await api.get<StoredFile[]>('/search', {
    params: { bucket, query, limit },
  });
  return response.data;
};

export const downloadFile = async (bucket: string, key: string): Promise<Blob> => {
  const response = await api.get<Blob>(`/buckets/${bucket}/files/${encodeURIComponent(key)}`, {
    responseType: 'blob',
  });
  return response.data;
};

/**
 * Direct URL to a file's raw bytes - for opening in a new tab (video/image/
 * PDF/text render inline; anything else falls back to the browser's own
 * download behavior). Unlike `downloadFile`, this doesn't fetch anything
 * itself - the browser makes the request when the tab opens, so the
 * server's real Content-Type (not a blob: URL, which loses it) decides
 * whether it renders or downloads.
 */
export const getFileUrl = (bucket: string, key: string): string =>
  `${API_URL}/buckets/${bucket}/files/${encodeURIComponent(key)}`;

export const deleteFile = async (bucket: string, key: string): Promise<void> => {
  await api.delete(`/buckets/${bucket}/files/${encodeURIComponent(key)}`);
};

export const getFileInfo = async (bucket: string, key: string): Promise<StoredFile> => {
  const response = await api.get<StoredFile>(`/buckets/${bucket}/files/${encodeURIComponent(key)}/info`);
  return response.data;
};

export const getStorageStats = async (): Promise<StorageStats> => {
  const response = await api.get<StorageStats>('/stats');
  return response.data;
};

export const getBucketStats = async (bucket: string): Promise<BucketStats> => {
  const response = await api.get<BucketStats>(`/buckets/${bucket}/stats`);
  return response.data;
};

export const listBuckets = async (): Promise<string[]> => {
  const response = await api.get<string[]>('/buckets');
  return response.data;
};

export const createBucket = async (bucket: string): Promise<void> => {
  await api.post(`/buckets/${bucket}`);
};

export const deleteBucket = async (bucket: string): Promise<void> => {
  await api.delete(`/buckets/${bucket}`);
};

/**
 * The backend returns plain text ("OK"/"UNHEALTHY"), not JSON - the real signal is the HTTP
 * status code (200 vs 503). A previous version of this client typed the response as JSON
 * ({status: "healthy"}) and checked `.status === 'healthy'`, which is always undefined against a
 * plain-text body - the health indicator showed "Offline" unconditionally regardless of actual
 * backend health.
 */
export const getHealth = async (): Promise<boolean> => {
  try {
    const response = await api.get('/health', { validateStatus: () => true });
    return response.status === 200;
  } catch {
    return false;
  }
};

export const listEncryptionKeys = async (): Promise<EncryptionKeyInfo[]> => {
  const response = await api.get<EncryptionKeyInfo[]>('/keys');
  return response.data;
};

export const createEncryptionKey = async (req: CreateKeyRequest): Promise<EncryptionKeyInfo> => {
  const response = await api.post<EncryptionKeyInfo>('/keys', req);
  return response.data;
};

export const deactivateEncryptionKey = async (keyId: string): Promise<void> => {
  await api.delete(`/keys/${encodeURIComponent(keyId)}`);
};
