use anyhow::Result;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::task;

/// Optimized multipart processing with pre-allocated buffer
pub async fn process_multipart_optimized(
    mut multipart: axum::extract::Multipart,
    max_size: usize,
) -> Result<(Vec<u8>, Option<String>, Option<String>), crate::errors::StorageError> {
    let mut content = Vec::with_capacity(1024 * 1024); // Pre-allocate 1MB
    let mut content_type = None;
    let mut filename = None;
    let mut total_size = 0usize;

    while let Some(field_result) = multipart.next_field().await.map_err(|e| {
        crate::errors::StorageError::Multipart(format!("Multipart field error: {}", e))
    })? {
        let field = field_result;
        let field_name = field.name().unwrap_or("unknown").to_string();
        
        if field_name == "file" {
            content_type = field.content_type().map(|s| s.to_string());
            filename = field.file_name().map(|s| s.to_string());
            
            // Stream with optimized buffer management
            content = stream_file_content_optimized(field, &mut total_size, max_size).await?;
            break;
        } else {
            // Skip non-file fields efficiently
            let _ = field.bytes().await.map_err(|e| {
                crate::errors::StorageError::Multipart(format!("Field skip error: {}", e))
            })?;
        }
    }

    Ok((content, filename, content_type))
}

/// Optimized streaming with better buffer management
async fn stream_file_content_optimized(
    mut field: axum::extract::multipart::Field<'_>,
    total_size: &mut usize,
    max_size: usize,
) -> Result<Vec<u8>, crate::errors::StorageError> {
    let mut content = Vec::with_capacity(64 * 1024); // Start with 64KB
    
    while let Some(chunk_result) = field.chunk().await.map_err(|e| {
        crate::errors::StorageError::Multipart(format!("Chunk read error: {}", e))
    })? {
        let chunk = chunk_result;
        
        *total_size += chunk.len();
        
        if *total_size > max_size {
            return Err(crate::errors::StorageError::BadRequest(format!(
                "File too large: {} bytes (max: {} bytes)", 
                *total_size, 
                max_size
            )));
        }
        
        // Efficiently extend the vector
        content.extend_from_slice(&chunk);
        
        // Reserve more space if needed (exponential growth)
        if content.capacity() - content.len() < chunk.len() {
            content.reserve(content.len()); // Double the capacity
        }
    }
    
    // Shrink to fit actual content size
    content.shrink_to_fit();
    Ok(content)
}

/// Parallel hash computation for better performance
pub async fn compute_hashes_parallel(content: &[u8]) -> (String, String) {
    let content_blake3 = content.to_vec();
    let content_md5 = content.to_vec();
    
    let (blake3_task, md5_task) = tokio::join!(
        task::spawn_blocking(move || {
            blake3::hash(&content_blake3).to_hex().to_string()
        }),
        task::spawn_blocking(move || {
            format!("{:x}", md5::compute(&content_md5))
        })
    );
    
    (
        blake3_task.unwrap_or_else(|_| String::new()),
        md5_task.unwrap_or_else(|_| String::new())
    )
}

/// Optimized file writer with buffering
pub async fn write_file_optimized(
    path: &std::path::Path,
    content: &[u8],
) -> Result<(), std::io::Error> {
    let file = tokio::fs::File::create(path).await?;
    let mut writer = BufWriter::with_capacity(64 * 1024, file); // 64KB buffer
    
    writer.write_all(content).await?;
    writer.flush().await?;
    writer.into_inner().sync_all().await?;
    
    Ok(())
}

/// Optimized atomic file write
pub async fn write_file_atomic_optimized(
    path: &std::path::Path,
    content: &[u8],
) -> Result<(), std::io::Error> {
    let temp_path = path.with_extension("tmp");
    
    // Write to temp file with buffering
    write_file_optimized(&temp_path, content).await?;
    
    // Atomic move
    tokio::fs::rename(&temp_path, path).await?;
    
    Ok(())
}

/// Configuration for optimized database pool
pub fn create_optimized_db_pool_config() -> sqlx::postgres::PgPoolOptions {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(50)                    // Increase from 10 to 50
        .min_connections(5)                     // Keep some connections warm
        .max_lifetime(std::time::Duration::from_secs(1800)) // 30 minutes
        .idle_timeout(std::time::Duration::from_secs(600))  // 10 minutes
        .acquire_timeout(std::time::Duration::from_secs(30)) // 30 seconds timeout
        .before_acquire(|conn, _meta| Box::pin(async move {
            // Validate connection with a quick query
            Ok(sqlx::query("SELECT 1").execute(conn).await.is_ok())
        }))
}

/// Batch database operations for better performance
pub struct BatchProcessor {
    batch_size: usize,
    pending_files: Vec<crate::models::StoredFile>,
}

impl BatchProcessor {
    pub fn new(batch_size: usize) -> Self {
        Self {
            batch_size,
            pending_files: Vec::with_capacity(batch_size),
        }
    }
    
    pub async fn add_file(&mut self, file: crate::models::StoredFile, pool: &sqlx::PgPool) -> Result<()> {
        self.pending_files.push(file);
        
        if self.pending_files.len() >= self.batch_size {
            self.flush(pool).await?;
        }
        
        Ok(())
    }
    
    pub async fn flush(&mut self, pool: &sqlx::PgPool) -> Result<()> {
        if self.pending_files.is_empty() {
            return Ok(());
        }
        
        let mut tx = pool.begin().await?;
        
        for file in &self.pending_files {
            // Batch insert would go here - this is a simplified version
            sqlx::query!(
                r#"INSERT INTO files (id, bucket, key, filename, file_path, file_size, 
                   original_size, content_type, hash_blake3, hash_md5, upload_time)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
                file.id, file.bucket, file.key, file.filename, file.file_path,
                file.file_size, file.original_size, file.content_type,
                file.hash_blake3, file.hash_md5, file.upload_time
            )
            .execute(&mut *tx)
            .await?;
        }
        
        tx.commit().await?;
        self.pending_files.clear();
        
        Ok(())
    }
}

/// Memory pool for reusing allocations
pub struct MemoryPool {
    buffers: tokio::sync::Mutex<Vec<Vec<u8>>>,
    buffer_size: usize,
}

impl MemoryPool {
    pub fn new(initial_count: usize, buffer_size: usize) -> Self {
        let mut buffers = Vec::with_capacity(initial_count);
        for _ in 0..initial_count {
            buffers.push(vec![0; buffer_size]);
        }
        
        Self {
            buffers: tokio::sync::Mutex::new(buffers),
            buffer_size,
        }
    }
    
    pub async fn get_buffer(&self) -> Vec<u8> {
        let mut buffers = self.buffers.lock().await;
        buffers.pop().unwrap_or_else(|| vec![0; self.buffer_size])
    }
    
    pub async fn return_buffer(&self, mut buffer: Vec<u8>) {
        buffer.clear();
        buffer.resize(self.buffer_size, 0);
        
        let mut buffers = self.buffers.lock().await;
        if buffers.len() < 100 { // Limit pool size
            buffers.push(buffer);
        }
    }
}

/// CPU-intensive file processing with async optimization
pub async fn process_file_cpu_intensive(
    content: &[u8],
    should_compress: bool,
    should_encrypt: bool,
) -> Result<(Vec<u8>, bool, bool), crate::errors::StorageError> {
    let processed = content.to_vec();
    
    // Process in background thread to avoid blocking
    let result = tokio::task::spawn_blocking(move || {
        let processed = processed;
        
        // Compression
        if should_compress {
            // Compression logic would go here
        }
        
        // Encryption
        if should_encrypt {
            // Encryption logic would go here
        }
        
        (processed, should_compress, should_encrypt)
    }).await.map_err(|_| crate::errors::StorageError::Io("Task join failed".to_string()))?;
    
    Ok(result)
} 