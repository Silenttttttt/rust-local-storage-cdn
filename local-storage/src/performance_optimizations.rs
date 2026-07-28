use anyhow::Result;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::task;

/// Parallel hash computation - runs BLAKE3 and MD5 on separate blocking threads concurrently
/// instead of sequentially, since both are CPU-bound and otherwise block the async runtime.
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

/// Buffered file writer.
async fn write_file_optimized(
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

/// Atomic file write: write to a temp file, then rename into place, so readers never see a
/// partially-written file.
pub async fn write_file_atomic_optimized(
    path: &std::path::Path,
    content: &[u8],
) -> Result<(), std::io::Error> {
    let temp_path = path.with_extension("tmp");

    write_file_optimized(&temp_path, content).await?;
    tokio::fs::rename(&temp_path, path).await?;

    Ok(())
}
