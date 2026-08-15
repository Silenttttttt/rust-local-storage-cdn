-- Optional per-file TTL (2026-08-15). NULL means "never expires" - the
-- default and only behavior for every file that ever existed before this
-- migration, and for any future upload that doesn't explicitly opt in.
-- Partial index only covers rows that actually have a TTL set, so it stays
-- tiny regardless of how many permanent files exist.
ALTER TABLE files ADD COLUMN expires_at TIMESTAMP WITH TIME ZONE;
CREATE INDEX idx_files_expires_at ON files (expires_at) WHERE expires_at IS NOT NULL;
