-- Local Storage Service Database Schema
-- High-performance file storage with metadata tracking

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Create buckets table
CREATE TABLE buckets (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    description TEXT,
    is_active BOOLEAN DEFAULT TRUE,
    max_file_size BIGINT,  -- Maximum allowed file size in bytes, NULL means no limit
    allowed_content_types TEXT[],  -- Array of allowed MIME types, NULL means all types allowed
    encryption_required BOOLEAN DEFAULT FALSE,
    compression_required BOOLEAN DEFAULT FALSE
);

-- Add indexes for buckets
CREATE INDEX idx_buckets_name ON buckets(name);
CREATE INDEX idx_buckets_active ON buckets(is_active);

-- Files table
CREATE TABLE files (
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
    
    -- Constraints
    CONSTRAINT unique_bucket_key UNIQUE (bucket, key),
    CONSTRAINT valid_file_size CHECK (file_size >= 0),
    CONSTRAINT valid_original_size CHECK (original_size >= 0),
    CONSTRAINT valid_access_count CHECK (access_count >= 0),
    CONSTRAINT valid_compression_ratio CHECK (compression_ratio IS NULL OR (compression_ratio >= 0.0 AND compression_ratio <= 1.0))
);

-- Storage statistics table
CREATE TABLE storage_stats (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    bucket VARCHAR(255) NOT NULL,
    total_files BIGINT NOT NULL DEFAULT 0,
    total_size BIGINT NOT NULL DEFAULT 0,
    compressed_files BIGINT NOT NULL DEFAULT 0,
    encrypted_files BIGINT NOT NULL DEFAULT 0,
    compression_ratio DECIMAL(5,4) NOT NULL DEFAULT 0.0,
    last_updated TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    -- Constraints
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

-- Indexes for performance
CREATE INDEX idx_files_bucket ON files (bucket);
CREATE INDEX idx_files_key ON files (key);
CREATE INDEX idx_files_hash_blake3 ON files (hash_blake3);
CREATE INDEX idx_files_hash_md5 ON files (hash_md5);
CREATE INDEX idx_files_upload_time ON files (upload_time DESC);
CREATE INDEX idx_files_last_accessed ON files (last_accessed DESC);
CREATE INDEX idx_files_access_count ON files (access_count DESC);
CREATE INDEX idx_files_file_size ON files (file_size);
CREATE INDEX idx_files_content_type ON files (content_type);
CREATE INDEX idx_files_compression_enabled ON files (compression_enabled);
CREATE INDEX idx_files_encryption_enabled ON files (encryption_enabled);

-- Composite indexes for common queries
CREATE INDEX idx_files_bucket_upload_time ON files (bucket, upload_time DESC);
CREATE INDEX idx_files_bucket_key_lookup ON files (bucket, key);

-- JSONB metadata search index
CREATE INDEX idx_files_metadata_gin ON files USING GIN (metadata);

-- Enable trigram extension for fuzzy text search
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Full-text search indexes
CREATE INDEX idx_files_filename_trgm ON files USING GIN (filename gin_trgm_ops);
CREATE INDEX idx_files_key_trgm ON files USING GIN (key gin_trgm_ops);

-- Function to update storage stats
CREATE OR REPLACE FUNCTION update_storage_stats()
RETURNS TRIGGER AS $$
BEGIN
    -- Insert or update stats for the bucket
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

-- Triggers to automatically update stats
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

-- Function to clean up old access logs
CREATE OR REPLACE FUNCTION cleanup_old_access_logs()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    -- Reset access counts for files not accessed in 30 days
    UPDATE files 
    SET access_count = 0 
    WHERE last_accessed < NOW() - INTERVAL '30 days'
    AND access_count > 0;
    
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Create a view for file statistics
CREATE VIEW file_stats AS
SELECT 
    bucket,
    COUNT(*) as total_files,
    SUM(file_size) as total_size,
    COUNT(CASE WHEN is_compressed THEN 1 END) as compressed_files,
    COUNT(CASE WHEN is_encrypted THEN 1 END) as encrypted_files,
    AVG(compression_ratio) FILTER (WHERE is_compressed) as avg_compression_ratio,
    MAX(upload_time) as last_updated
FROM files
GROUP BY bucket;

-- Create a view for popular files
CREATE VIEW popular_files AS
SELECT 
    bucket,
    key,
    filename,
    file_size,
    content_type,
    access_count,
    last_accessed,
    upload_time
FROM files
WHERE access_count > 0
ORDER BY access_count DESC, last_accessed DESC;

-- Comments for documentation
COMMENT ON TABLE files IS 'Main table storing file metadata and references';
COMMENT ON TABLE storage_stats IS 'Aggregated statistics per bucket';
COMMENT ON COLUMN files.hash_blake3 IS 'BLAKE3 hash for deduplication and integrity';
COMMENT ON COLUMN files.hash_md5 IS 'MD5 hash for compatibility';
COMMENT ON COLUMN files.metadata IS 'User-defined metadata stored as JSONB';
COMMENT ON COLUMN files.compression_ratio IS 'Compression ratio (compressed_size / original_size)';
COMMENT ON FUNCTION update_storage_stats() IS 'Automatically maintains storage statistics';
COMMENT ON FUNCTION cleanup_old_access_logs() IS 'Resets access counts for files not accessed in 30 days';
COMMENT ON VIEW file_stats IS 'Aggregated file statistics by bucket';
COMMENT ON VIEW popular_files IS 'Most accessed files across all buckets'; 