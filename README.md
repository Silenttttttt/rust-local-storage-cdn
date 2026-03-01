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
GET /health  →  "OK"
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
| POST | `/buckets/:bucket/files` | Upload file (raw body + `Content-Disposition: filename="..."`) |
| GET | `/buckets/:bucket/files/*path` | Download file |
| GET | `/buckets/:bucket/files/*path/info` | File metadata (JSON) |
| DELETE | `/buckets/:bucket/files/*path` | Delete file |

### Global

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/stats` | Global storage statistics |
| GET | `/search?query=...` | Search files (`?bucket=` to scope) |

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

### Redis (optional)

| Variable | Default | Description |
|----------|---------|-------------|
| `ENABLE_REDIS` | `false` | Enable Redis cache |
| `REDIS_HOST` | `redis` | Redis host |
| `REDIS_PORT` | `6379` | Redis port |
| `REDIS_PASSWORD` | — | Redis password |
| `REDIS_DB` | `0` | Redis database number |
| `REDIS_TTL_SECONDS` | `3600` | Cache TTL |

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
