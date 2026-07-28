# 🦀 Local Storage CDN

**A high-performance S3-like local storage service** built in Rust with Redis caching for maximum throughput and minimal latency. Store files in buckets, with optional deduplication, compression, encryption, and Redis caching. Use it as a local CDN, backup target, or file API for your applications.

The backend is the core—a lean HTTP API that runs anywhere. An optional React UI and Redis cache can be added when needed.

## 🚀 Performance Features

- **🦀 Rust Performance**: 5-10x faster than Python equivalent
- **⚡ Redis Caching**: Sub-millisecond file metadata access
- **🗜️ Smart Compression**: ZSTD/GZIP with configurable thresholds  
- **🔒 Encryption**: AES-GCM & ChaCha20-Poly1305 support
- **📊 Deduplication**: BLAKE3 hashing prevents duplicate storage
- **💾 PostgreSQL**: Robust metadata storage with advanced indexing
- **🔄 Async I/O**: Tokio-based async operations throughout

---

## Backend Overview

The backend is a Rust (Axum) service that exposes an S3-style HTTP API. Files are stored on disk with metadata in PostgreSQL. The design prioritizes:

- **Performance** — Async I/O, mimalloc, concurrency limits, streaming
- **Reliability** — Atomic writes, transactions, migrations
- **Flexibility** — Optional Redis, compression, encryption, deduplication

### Core Features

| Feature | Description |
|--------|-------------|
| **Buckets & Objects** | Create buckets, upload/download files with paths (e.g. `images/photo.jpg`) |
| **Content Addressing** | BLAKE3 and MD5 hashes for integrity and deduplication |
| **Deduplication** | Identical content stored once; duplicate uploads reuse existing files |
| **Atomic Writes** | Temp file + rename for crash-safe uploads |
| **Full-Text Search** | PostgreSQL trigram search over filenames and keys |
| **Concurrency Control** | 100 concurrent requests max, 5-minute timeout for large uploads |
| **CORS** | Configured for cross-origin access |

### Optional Features

| Feature | Description |
|--------|-------------|
| **Redis Cache** | Metadata and small file content (≤1MB) cached with TTL; toggle on/off |
| **Compression** | zstd or gzip; configurable level and min size |
| **Encryption** | AES-256-GCM or ChaCha20-Poly1305; per-file or global |
| **Web UI** | React dashboard for buckets and files (optional) |

### Tech Stack

- **Rust** — Axum, Tokio, SQLx
- **PostgreSQL** — Metadata, migrations, indexes, full-text search
- **Redis** (optional) — Caching
- **Storage** — Local filesystem with configurable path

---

## Quick Start

### Backend only with existing PostgreSQL

```bash
export POSTGRES_HOST=your-db-host
export POSTGRES_PASSWORD=your-password

docker compose -f docker-compose.standalone.yml up -d

curl http://localhost:8080/health
```

### Local dev with Docker PostgreSQL

```bash
docker compose --profile db up -d
```

### With Redis and Web UI

```bash
docker compose -f docker-compose.yml -f docker-compose.redis.yml \
  --profile db --profile redis --profile frontend up -d
```

---

## API Reference

### Health

```
GET /health  →  200 "OK"          (database and cache both reachable)
              →  503 "UNHEALTHY"  (otherwise)
```

### Buckets

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/buckets` | List all buckets |
| POST | `/buckets/:bucket` | Create bucket |
| DELETE | `/buckets/:bucket` | Delete bucket (and its files) |
| GET | `/buckets/:bucket/stats` | Bucket stats (file count, size, compression) |

### Files

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/buckets/:bucket/files` | List files (`?prefix=`, `?limit=`, `?offset=`) |
| POST | `/buckets/:bucket/files` | Upload file (raw body + `Content-Disposition: filename="..."`). Optional `?compress=`, `?encrypt=`, `?compression_algorithm=`, `?compression_level=`, `?encryption_key_id=` override the global config per upload. |
| GET | `/buckets/:bucket/files/*path` | Download file |
| GET | `/buckets/:bucket/files/*path/info` | File metadata (JSON) |
| DELETE | `/buckets/:bucket/files/*path` | Delete file |

### Global

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/stats` | Global storage statistics |
| GET | `/search?query=...` | Search files (`?bucket=` to scope) |

### Encryption Keys

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/keys` | Create a new encryption key (`{"algorithm": "aes-gcm", "description": "..."}`) |
| GET | `/keys` | List active keys (never returns raw key material) |
| DELETE | `/keys/:key_id` | Deactivate a key |

Pass a key's `key_id` as `encryption_key_id` on upload to encrypt that file with it instead of the global `CRYPTO_KEY`.

### Upload Format

Upload with raw body and headers:

```
POST /buckets/my-bucket/files
Content-Type: application/octet-stream
Content-Disposition: attachment; filename="document.pdf"

<raw file bytes>
```

For nested paths, use `Content-Disposition: attachment; filename="folder/sub/file.txt"`.

---

## S3-Compatible API (v2)

A second, S3-compatible API is available alongside the native API above, for anything that
already speaks S3 (aws-cli, boto3, rclone, mc, backup tools, etc). It's opt-in - only mounted
when `S3_ACCESS_KEY` and `S3_SECRET_KEY` are both set - and lives entirely at `/v2`, so it never
collides with the native `/buckets/...` routes.

Point any S3 client at `http://host:port/v2` as the endpoint (path-style addressing):

```bash
aws --endpoint-url http://host:port/v2 s3 ls
aws --endpoint-url http://host:port/v2 s3 cp file.txt s3://mybucket/
aws --endpoint-url http://host:port/v2 s3 cp s3://mybucket/file.txt ./file.txt

rclone config create myremote s3 provider=Other endpoint=http://host:port/v2 \
  access_key_id=$S3_ACCESS_KEY secret_access_key=$S3_SECRET_KEY
rclone copy ./dir myremote:mybucket
```

Supported operations: `ListBuckets`, `CreateBucket`, `DeleteBucket`, `ListObjectsV2`,
`PutObject`, `GetObject`, `HeadObject`, `DeleteObject`, `DeleteObjects` (batch), `CopyObject`
(via `PUT` with an `x-amz-copy-source` header). Auth is real AWS SigV4 request signing against
the one configured key pair - no request goes through without a valid signature.

**Not supported yet**: the Multipart Upload API (`CreateMultipartUpload`/`UploadPart`/...) and
chunked/streaming signed payloads (`STREAMING-AWS4-HMAC-SHA256-PAYLOAD`) - both return a clear
`NotImplemented` error rather than silently misbehaving. Regular (non-chunked) `PutObject` has no
special size handling beyond the usual `MAX_FILE_SIZE` limit.

| Variable | Default | Description |
|----------|---------|--------------|
| `S3_ACCESS_KEY` | — | Enables /v2 when set together with `S3_SECRET_KEY` |
| `S3_SECRET_KEY` | — | Secret half of the key pair used for SigV4 verification |
| `S3_REGION` | `us-east-1` | Region string used in the credential scope (arbitrary - there's no real AWS region here) |

## Backend Configuration

All configuration is via environment variables. Create a `.env` file or pass them to Docker.

### Database (required)

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_HOST` | `localhost` | PostgreSQL host |
| `POSTGRES_PORT` | `5432` | PostgreSQL port |
| `POSTGRES_DB` | `local_storage` | Database name |
| `POSTGRES_USER` | `postgres` | Username |
| `POSTGRES_PASSWORD` | — | Password |
| `DB_MAX_CONNECTIONS` | `10` | Connection pool size |

### Storage

| Variable | Default | Description |
|----------|---------|-------------|
| `STORAGE_PATH` | `/storage` | Base path for file storage |
| `MAX_FILE_SIZE` | `1073741824` | Max file size in bytes (1GB) |
| `DEFAULT_BUCKET` | `default` | Default bucket name |
| `ENABLE_DEDUPLICATION` | `true` | Deduplicate by BLAKE3 hash |

### Cache

Metadata and small file content (≤1MB) are cached read-through, backed by either an in-process
cache (default - no extra service to run, no network hop) or Redis (opt-in, useful mainly if you
run multiple replicas and want them sharing a cache).

| Variable | Default | Description |
|----------|---------|-------------|
| `CACHE_BACKEND` | `memory` | `memory` or `redis` |
| `CACHE_MAX_SIZE_MB` | `256` | Max size of the in-process cache (only used when `CACHE_BACKEND=memory`) |
| `CACHE_TTL_SECONDS` | `3600` | Cache entry TTL, either backend (falls back to `REDIS_TTL_SECONDS` if set, for backward compatibility) |
| `ENABLE_REDIS` | `false` | Legacy alias for `CACHE_BACKEND=redis` - still honored if set, but `CACHE_BACKEND` takes precedence |
| `REDIS_HOST` | `redis` | Redis host (only used when the Redis backend is active) |
| `REDIS_PORT` | `6379` | Redis port |
| `REDIS_PASSWORD` | — | Redis password |
| `REDIS_DB` | `0` | Redis database number |

### Compression

| Variable | Default | Description |
|----------|---------|-------------|
| `ENABLE_COMPRESSION` | `false` | Global compression |
| `COMPRESSION_ALGORITHM` | `zstd` | `zstd` or `gzip` |
| `COMPRESSION_LEVEL` | `3` | Level (zstd 1–22, gzip 1–9) |
| `COMPRESSION_MIN_SIZE` | `1024` | Min size in bytes to compress |

### Encryption

| Variable | Default | Description |
|----------|---------|-------------|
| `ENABLE_ENCRYPTION` | `false` | Global encryption |
| `CRYPTO_ALGORITHM` | `aes-gcm` | `aes-gcm` or `chacha20poly1305` |
| `CRYPTO_KEY` | — | 32 bytes or 64 hex chars; omit to auto-generate (not persistent) |

### Server

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8080` | Listen port |
| `LOG_LEVEL` | `INFO` | Log level |

---

## Docker

### Compose Files

| File | Use case |
|------|----------|
| `docker-compose.standalone.yml` | Backend only, external PostgreSQL |
| `docker-compose.yml` | Full stack with Docker Postgres |

### Profiles (docker-compose.yml)

| Profile | Services |
|---------|----------|
| `db` | PostgreSQL + backend |
| `redis` | + Redis (use with `docker-compose.redis.yml`) |
| `frontend` | + Web UI |

### Examples

```bash
# Backend + external DB (typical)
docker compose -f docker-compose.standalone.yml up -d

# Local dev with Docker DB
docker compose --profile db up -d

# Full stack
docker compose -f docker-compose.yml -f docker-compose.redis.yml \
  --profile db --profile redis --profile frontend up -d
```

---

## Project Structure

```
local-storage-cdn/
├── local-storage/           # Rust backend
│   ├── src/
│   │   ├── app.rs           # Router, CORS, concurrency
│   │   ├── config.rs        # Env config
│   │   ├── storage.rs       # Core storage logic
│   │   ├── cache.rs         # Redis cache (optional)
│   │   ├── crypto.rs        # AES-GCM, ChaCha20
│   │   ├── compression.rs   # zstd, gzip
│   │   └── handlers/
│   ├── migrations/
│   └── Cargo.toml
├── local-storage-ui/        # Optional React UI
├── docker-compose.yml
├── docker-compose.standalone.yml
└── docker-compose.redis.yml
```

---

## Development

### Backend (Rust)

```bash
cd local-storage

# Migrations (requires Postgres)
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/local_storage
sqlx migrate run

# Run
cargo run
```

### Frontend (React)

```bash
cd local-storage-ui
npm install
REACT_APP_API_URL=http://localhost:8080 npm start
```

---

## License

MIT
