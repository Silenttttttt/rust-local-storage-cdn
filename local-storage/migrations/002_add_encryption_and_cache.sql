-- Migration: Add encryption key management and caching
-- This migration adds tables for encryption keys and cache configuration

-- Create encryption_keys table
CREATE TABLE encryption_keys (
    id SERIAL PRIMARY KEY,
    key_id VARCHAR(64) UNIQUE NOT NULL,  -- Unique identifier for the key
    key_data BYTEA NOT NULL,             -- The actual encryption key (encrypted)
    algorithm VARCHAR(50) NOT NULL,       -- aes-gcm, chacha20poly1305, etc.
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    is_active BOOLEAN DEFAULT TRUE,
    description TEXT                      -- Optional description of the key
);

-- Add encryption key reference to files table
ALTER TABLE files ADD COLUMN encryption_key_id VARCHAR(64) REFERENCES encryption_keys(key_id);

-- Add indexes for encryption keys
CREATE INDEX idx_encryption_keys_key_id ON encryption_keys(key_id);
CREATE INDEX idx_encryption_keys_active ON encryption_keys(is_active);
CREATE INDEX idx_files_encryption_key_id ON files(encryption_key_id);

-- Add cache-specific fields to the files table
ALTER TABLE files
    ADD COLUMN cache_status VARCHAR(20) DEFAULT 'not_cached',  -- Values: not_cached, cached, pending
    ADD COLUMN last_cache_update TIMESTAMP WITH TIME ZONE,
    ADD COLUMN cache_hits BIGINT DEFAULT 0,
    ADD COLUMN cache_priority INTEGER DEFAULT 0;  -- Higher number = higher priority for caching

-- Create an index for cache management
CREATE INDEX idx_files_cache_management ON files (
    cache_status,
    cache_priority DESC,
    access_count DESC,
    last_accessed DESC
) WHERE cache_status != 'not_cached';

-- Add a check constraint for valid cache status values
ALTER TABLE files
    ADD CONSTRAINT valid_cache_status 
    CHECK (cache_status IN ('not_cached', 'cached', 'pending'));

-- Add a table for Redis cache configuration
CREATE TABLE cache_config (
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