# Multi-stage Rust build for maximum performance
FROM rustlang/rust:nightly-slim AS builder

# Install system dependencies for building
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy Cargo files first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Set SQLx to offline mode to avoid database connection during build
ENV SQLX_OFFLINE=true

# Copy SQLx offline data files (essential for offline builds)
COPY .sqlx ./.sqlx

# Create dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies (this layer will be cached)
RUN cargo build --release && rm -rf src

# Copy source code and migrations
COPY src ./src
COPY migrations ./migrations

# Build the actual application
RUN cargo build --release

# Runtime stage with Debian slim (compatible with glibc)
FROM debian:bookworm-slim

# Install runtime dependencies and ca-certificates
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -g 1001 appgroup && \
    useradd -u 1001 -g appgroup -m appuser

# Create storage directory
RUN mkdir -p /storage && \
    chown appuser:appgroup /storage

# Copy binary and migrations from builder stage
COPY --from=builder /app/target/release/local-storage /usr/local/bin/local-storage
COPY --from=builder /app/migrations /app/migrations
RUN chmod +x /usr/local/bin/local-storage && chown -R appuser:appgroup /app

# Switch to non-root user
USER appuser

# Expose port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8080/health || exit 1

# Run the application
CMD ["/usr/local/bin/local-storage"] 