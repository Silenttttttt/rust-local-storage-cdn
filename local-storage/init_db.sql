-- Local Storage Service Database Initialization
-- Combined migrations for development

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Create buckets table
CREATE TABLE IF NOT EXISTS buckets (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    description TEXT,
    is_active BOOLEAN DEFAULT TRUE,
    max_file_size BIGINT,
    allowed_content_types TEXT[],
    encryption_required BOOLEAN DEFAULT FALSE,
    compression_required BOOLEAN DEFAULT FALSE
);

-- Add indexes for buckets
CREATE INDEX IF NOT EXISTS idx_buckets_name ON buckets(name);
CREATE INDEX IF NOT EXISTS idx_buckets_active ON buckets(is_active);

-- Create encryption_keys table
CREATE TABLE IF NOT EXISTS encryption_keys (
    id SERIAL PRIMARY KEY,
    key_id VARCHAR(64) UNIQUE NOT NULL,
    key_data BYTEA NOT NULL,
    algorithm VARCHAR(50) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    is_active BOOLEAN DEFAULT TRUE,
    description TEXT
);

-- Files table
CREATE TABLE IF NOT EXISTS files (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    bucket VARCHAR(255) NOT NULL REFERENCES buckets(name) ON DELETE CASCADE ON UPDATE CASCADE,
    key VARCHAR(1024) NOT NULL,
    filename VARCHAR(1024) NOT NULL,
    file_path TEXT NOT NULL,
    file_size BIGINT NOT NULL,
    original_size BIGINT NOT NULL,
    content_type VARCHAR(255) NOT NULL,
    hash_blake3 VARCHAR(64) NOT NULL,
    hash_md5 VARCHAR(32) NOT NULL,
    metadata JSONB,
    is_compressed BOOLEAN NOT NULL DEFAULT FALSE,
    is_encrypted BOOLEAN NOT NULL DEFAULT FALSE,
    compression_algorithm VARCHAR(50),
    encryption_algorithm VARCHAR(50),
    compression_ratio REAL,
    compression_enabled BOOLEAN DEFAULT FALSE,
    encryption_enabled BOOLEAN DEFAULT FALSE,
    compression_level INTEGER,
    upload_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_accessed TIMESTAMP WITH TIME ZONE,
    access_count BIGINT NOT NULL DEFAULT 0,
    encryption_key_id VARCHAR(64) REFERENCES encryption_keys(key_id),
    cache_status VARCHAR(20) DEFAULT 'not_cached',
    last_cache_update TIMESTAMP WITH TIME ZONE,
    cache_hits BIGINT DEFAULT 0,
    cache_priority INTEGER DEFAULT 0,
    
    CONSTRAINT unique_bucket_key UNIQUE (bucket, key),
    CONSTRAINT valid_file_size CHECK (file_size >= 0),
    CONSTRAINT valid_original_size CHECK (original_size >= 0),
    CONSTRAINT valid_access_count CHECK (access_count >= 0),
    CONSTRAINT valid_compression_ratio CHECK (compression_ratio IS NULL OR (compression_ratio >= 0.0 AND compression_ratio <= 1.0)),
    CONSTRAINT valid_cache_status CHECK (cache_status IN ('not_cached', 'cached', 'pending'))
);

-- Storage statistics table
CREATE TABLE IF NOT EXISTS storage_stats (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    bucket VARCHAR(255) NOT NULL,
    total_files BIGINT NOT NULL DEFAULT 0,
    total_size BIGINT NOT NULL DEFAULT 0,
    compressed_files BIGINT NOT NULL DEFAULT 0,
    encrypted_files BIGINT NOT NULL DEFAULT 0,
    compression_ratio DECIMAL(5,4) NOT NULL DEFAULT 0.0,
    last_updated TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    CONSTRAINT unique_bucket_stats UNIQUE (bucket),
    CONSTRAINT valid_stats CHECK (
        total_files >= 0 AND 
        total_size >= 0 AND 
        compressed_files >= 0 AND 
        encrypted_files >= 0 AND
        compression_ratio >= 0.0 AND 
        compression_ratio <= 1.0
    )
);

-- Cache configuration table
CREATE TABLE IF NOT EXISTS cache_config (
    id INTEGER PRIMARY KEY DEFAULT 1,
    max_cache_size_gb DOUBLE PRECISION DEFAULT 1.0,
    cache_ttl_seconds INTEGER DEFAULT 3600,
    preload_enabled BOOLEAN DEFAULT TRUE,
    min_access_count INTEGER DEFAULT 5,
    cache_priority_weights JSONB DEFAULT '{"access_count": 1.0, "file_size": -0.5, "last_accessed": 1.0}'::jsonb,
    auto_cache_threshold INTEGER DEFAULT 10,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT single_config CHECK (id = 1)
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_files_bucket ON files (bucket);
CREATE INDEX IF NOT EXISTS idx_files_key ON files (key);
CREATE INDEX IF NOT EXISTS idx_files_hash_blake3 ON files (hash_blake3);
CREATE INDEX IF NOT EXISTS idx_files_hash_md5 ON files (hash_md5);
CREATE INDEX IF NOT EXISTS idx_files_upload_time ON files (upload_time DESC);
CREATE INDEX IF NOT EXISTS idx_files_last_accessed ON files (last_accessed DESC);
CREATE INDEX IF NOT EXISTS idx_files_access_count ON files (access_count DESC);
CREATE INDEX IF NOT EXISTS idx_files_file_size ON files (file_size);
CREATE INDEX IF NOT EXISTS idx_files_content_type ON files (content_type);
CREATE INDEX IF NOT EXISTS idx_files_compression_enabled ON files (compression_enabled);
CREATE INDEX IF NOT EXISTS idx_files_encryption_enabled ON files (encryption_enabled);
CREATE INDEX IF NOT EXISTS idx_files_bucket_upload_time ON files (bucket, upload_time DESC);
CREATE INDEX IF NOT EXISTS idx_files_bucket_key_lookup ON files (bucket, key);
CREATE INDEX IF NOT EXISTS idx_encryption_keys_key_id ON encryption_keys(key_id);
CREATE INDEX IF NOT EXISTS idx_encryption_keys_active ON encryption_keys(is_active);
CREATE INDEX IF NOT EXISTS idx_files_encryption_key_id ON files(encryption_key_id);
CREATE INDEX IF NOT EXISTS idx_files_cache_management ON files (
    cache_status,
    cache_priority DESC,
    access_count DESC,
    last_accessed DESC
) WHERE cache_status != 'not_cached';

-- JSONB metadata search index
CREATE INDEX IF NOT EXISTS idx_files_metadata_gin ON files USING GIN (metadata);

-- Enable trigram extension for fuzzy text search
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Full-text search indexes
CREATE INDEX IF NOT EXISTS idx_files_filename_trgm ON files USING GIN (filename gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_files_key_trgm ON files USING GIN (key gin_trgm_ops);

-- Function to update storage stats
CREATE OR REPLACE FUNCTION update_storage_stats()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO storage_stats (bucket, total_files, total_size, compressed_files, encrypted_files, compression_ratio)
    SELECT 
        bucket,
        COUNT(*) as total_files,
        SUM(file_size) as total_size,
        SUM(CASE WHEN is_compressed THEN 1 ELSE 0 END) as compressed_files,
        SUM(CASE WHEN is_encrypted THEN 1 ELSE 0 END) as encrypted_files,
        CASE 
            WHEN SUM(CASE WHEN is_compressed THEN original_size ELSE file_size END) > 0 
            THEN 1.0 - (SUM(file_size)::DECIMAL / SUM(CASE WHEN is_compressed THEN original_size ELSE file_size END))
            ELSE 0.0 
        END as compression_ratio
    FROM files 
    WHERE bucket = COALESCE(NEW.bucket, OLD.bucket)
    GROUP BY bucket
    ON CONFLICT (bucket) 
    DO UPDATE SET
        total_files = EXCLUDED.total_files,
        total_size = EXCLUDED.total_size,
        compressed_files = EXCLUDED.compressed_files,
        encrypted_files = EXCLUDED.encrypted_files,
        compression_ratio = EXCLUDED.compression_ratio,
        last_updated = NOW();
    
    RETURN COALESCE(NEW, OLD);
END;
$$ LANGUAGE plpgsql;

-- Drop existing triggers if they exist
DROP TRIGGER IF EXISTS trigger_update_stats_insert ON files;
DROP TRIGGER IF EXISTS trigger_update_stats_update ON files;
DROP TRIGGER IF EXISTS trigger_update_stats_delete ON files;

-- Create triggers
CREATE TRIGGER trigger_update_stats_insert
    AFTER INSERT ON files
    FOR EACH ROW
    EXECUTE FUNCTION update_storage_stats();

CREATE TRIGGER trigger_update_stats_update
    AFTER UPDATE ON files
    FOR EACH ROW
    EXECUTE FUNCTION update_storage_stats();

CREATE TRIGGER trigger_update_stats_delete
    AFTER DELETE ON files
    FOR EACH ROW
    EXECUTE FUNCTION update_storage_stats();

-- Insert default cache configuration
INSERT INTO cache_config (
    id, 
    max_cache_size_gb, 
    cache_ttl_seconds, 
    preload_enabled, 
    min_access_count,
    cache_priority_weights,
    auto_cache_threshold,
    updated_at
)
VALUES (
    1, 
    1.0, 
    3600, 
    TRUE, 
    5,
    '{"access_count": 1.0, "file_size": -0.5, "last_accessed": 1.0}'::jsonb,
    10,
    NOW()
)
ON CONFLICT (id) DO NOTHING;

-- Insert default dev bucket
INSERT INTO buckets (name, description, is_active)
VALUES ('session-grabs-dev', 'Development bucket for session grabs', true)
ON CONFLICT (name) DO NOTHING;


