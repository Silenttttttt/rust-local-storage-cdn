import axios from 'axios';
import { BucketStats, FileListParams, FileSearchParams, FileUploadResponse, StorageStats, StoredFile, HealthResponse } from '../types/api';

// Get API URL from runtime configuration or fallback to environment variable
const getApiUrl = (): string => {
  // Try runtime config first (for production)
  if (typeof window !== 'undefined' && window.APP_CONFIG?.API_URL) {
    console.log('📡 Using runtime API URL:', window.APP_CONFIG.API_URL);
    return window.APP_CONFIG.API_URL;
  }
  // Fallback to environment variable (for development)
  const envUrl = process.env.REACT_APP_API_URL || 'http://localhost:8080';
  console.log('📡 Using environment API URL:', envUrl);
  return envUrl;
};

const API_URL = getApiUrl();

// Debug logging
console.log('🔧 API Client Configuration:');
console.log('  API_URL:', API_URL);
console.log('  Environment:', process.env.NODE_ENV);
console.log('  Runtime Config:', typeof window !== 'undefined' ? window.APP_CONFIG : 'Not available');

const api = axios.create({
  baseURL: API_URL,
  headers: {
    'Content-Type': 'application/json',
  },
});

// Add request interceptor for debugging
api.interceptors.request.use(
  (config) => {
    console.log('🚀 API Request:', {
      method: config.method?.toUpperCase(),
      url: config.url,
      baseURL: config.baseURL,
      fullURL: `${config.baseURL}${config.url}`,
    });
    return config;
  },
  (error) => {
    console.error('❌ API Request Error:', error);
    return Promise.reject(error);
  }
);

// Add response interceptor for debugging
api.interceptors.response.use(
  (response) => {
    console.log('✅ API Response:', {
      status: response.status,
      url: response.config.url,
      data: response.data,
    });
    return response;
  },
  (error) => {
    console.error('❌ API Response Error:', {
      status: error.response?.status,
      statusText: error.response?.statusText,
      url: error.config?.url,
      message: error.message,
      response: error.response?.data,
    });
    return Promise.reject(error);
  }
);

export const uploadFile = async (bucket: string, file: File, path?: string): Promise<FileUploadResponse> => {
  // Use the new raw file data format instead of multipart form data
  const headers = {
    'Content-Type': 'application/octet-stream',
    'Content-Disposition': `attachment; filename="${encodeURIComponent(file.name)}"`,
  };
  
  // If path is specified, add it to the URL
  const url = path 
    ? `/buckets/${bucket}/files?path=${encodeURIComponent(path)}`
    : `/buckets/${bucket}/files`;
    
  const response = await api.post<FileUploadResponse>(url, file, {
    headers: headers,
    timeout: 300000, // 5 minutes timeout for large files
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
  const response = await api.get<Blob>(`/buckets/${bucket}/files/${key}`, {
    responseType: 'blob',
  });
  return response.data;
};

export const deleteFile = async (bucket: string, key: string): Promise<void> => {
  await api.delete(`/buckets/${bucket}/files/${key}`);
};

export const getFileInfo = async (bucket: string, key: string): Promise<StoredFile> => {
  const response = await api.get<StoredFile>(`/buckets/${bucket}/files/${key}/info`);
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

export const deleteBucket = async (bucket: string): Promise<void> => {
  await api.delete(`/buckets/${bucket}`);
};

export const getHealth = async (): Promise<HealthResponse> => {
  const response = await api.get<HealthResponse>('/health');
  return response.data;
}; 