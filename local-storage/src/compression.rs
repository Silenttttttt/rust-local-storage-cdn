use crate::config::CompressionConfig;
use crate::errors::{Result, StorageError};
use flate2::{write::GzEncoder, read::GzDecoder, Compression as GzCompression};
use std::io::{Read, Write};
use std::sync::Arc;

pub struct CompressionManager {
    config: Arc<CompressionConfig>,
}

impl CompressionManager {
    pub fn new(config: Arc<CompressionConfig>) -> Self {
        CompressionManager { config }
    }

    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if !self.config.enabled || data.len() < self.config.min_size as usize || data.is_empty() {
            // Return data unchanged when compression is disabled or data is too small
            return Ok(data.to_vec());
        }

        match self.config.algorithm.as_str() {
            "gzip" => {
                let mut result = Vec::new();
                result.push(1); // 1 = gzip compressed
                let compressed = self.compress_gzip(data)?;
                result.extend_from_slice(&compressed);
                Ok(result)
            },
            "zstd" => {
                let mut result = Vec::new();
                result.push(2); // 2 = zstd compressed
                let compressed = self.compress_zstd(data)?;
                result.extend_from_slice(&compressed);
                Ok(result)
            },
            _ => Err(StorageError::Compression(format!(
                "Unsupported compression algorithm: {}",
                self.config.algorithm
            ))),
        }
    }

    pub fn decompress(&self, compressed_data: &[u8]) -> Result<Vec<u8>> {
        if !self.config.enabled || compressed_data.is_empty() {
            return Ok(compressed_data.to_vec());
        }

        // Check if data has our header format (compressed data)
        if compressed_data.len() >= 1 {
            let format = compressed_data[0];
            let data = &compressed_data[1..];
            
            match format {
                1 => return self.decompress_gzip(data), // Gzip compressed
                2 => return self.decompress_zstd(data), // Zstd compressed
                _ => {} // Not our format, continue to legacy handling
            }
        }

        // If no header format detected, check if data looks like it might be compressed
        // For gzip, check for magic bytes; for zstd, try decompression
        match self.config.algorithm.as_str() {
            "gzip" => {
                // Check for gzip magic bytes (0x1f, 0x8b)
                if compressed_data.len() >= 2 && compressed_data[0] == 0x1f && compressed_data[1] == 0x8b {
                    self.decompress_gzip(compressed_data)
                } else {
                    // Assume uncompressed data
                    Ok(compressed_data.to_vec())
                }
            },
            "zstd" => {
                // For zstd, we can't easily detect magic bytes, so try decompression
                // If it fails, assume it's uncompressed data
                match self.decompress_zstd(compressed_data) {
                    Ok(data) => Ok(data),
                    Err(_) => Ok(compressed_data.to_vec()),
                }
            },
            _ => Err(StorageError::Compression(format!(
                "Unsupported compression algorithm: {}",
                self.config.algorithm
            ))),
        }
    }

    /// Decompresses only the requested byte range of the DECOMPRESSED output -
    /// discards decoded bytes before `start` as they're produced (never held in
    /// memory) and stops reading as soon as `len` bytes have been collected
    /// (never decodes the remainder of the file past the requested range
    /// either). Can't skip decoding the portion BEFORE `start` - these formats
    /// aren't seekable, decompression is inherently sequential from the
    /// beginning - but this avoids ever materializing the FULL decompressed
    /// output in memory, which is the real win for a large file where only a
    /// small slice is actually being served (e.g. a video player seeking to
    /// one point). Returns `Ok(None)` for anything other than the two
    /// recognized header-byte formats (1=gzip, 2=zstd) - the caller falls back
    /// to the existing full `decompress()`, which already handles the rarer
    /// legacy/headerless cases safely.
    pub fn decompress_range(&self, compressed_data: &[u8], start: usize, len: usize) -> Result<Option<Vec<u8>>> {
        if compressed_data.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let format = compressed_data[0];
        let data = &compressed_data[1..];
        let mut reader: Box<dyn Read> = match format {
            1 => Box::new(GzDecoder::new(data)),
            2 => match zstd::Decoder::new(data) {
                Ok(d) => Box::new(d),
                Err(_) => return Ok(None),
            },
            _ => return Ok(None),
        };

        let mut discard_buf = [0u8; 65536];
        let mut remaining_to_skip = start;
        while remaining_to_skip > 0 {
            let chunk = remaining_to_skip.min(discard_buf.len());
            let read = reader.read(&mut discard_buf[..chunk])
                .map_err(|e| StorageError::Compression(format!("Decompression failed while skipping to range start: {}", e)))?;
            if read == 0 {
                break; // EOF before reaching `start` - caller's own range validation should prevent this
            }
            remaining_to_skip -= read;
        }

        let mut result = vec![0u8; len];
        let mut filled = 0;
        while filled < len {
            let read = reader.read(&mut result[filled..])
                .map_err(|e| StorageError::Compression(format!("Decompression failed while reading range: {}", e)))?;
            if read == 0 {
                break; // EOF before filling the full requested length
            }
            filled += read;
        }
        result.truncate(filled);
        Ok(Some(result))
    }

    fn compress_gzip(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = GzEncoder::new(Vec::new(), GzCompression::new(self.config.level as u32));
        encoder.write_all(data).map_err(|e| {
            StorageError::Compression(format!("Gzip compression failed: {}", e))
        })?;
        
        encoder.finish().map_err(|e| {
            StorageError::Compression(format!("Gzip compression failed: {}", e))
        })
    }

    fn decompress_gzip(&self, compressed_data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = GzDecoder::new(compressed_data);
        let mut decompressed = Vec::new();
        
        decoder.read_to_end(&mut decompressed).map_err(|e| {
            StorageError::Compression(format!("Gzip decompression failed: {}", e))
        })?;
        
        Ok(decompressed)
    }

    fn compress_zstd(&self, data: &[u8]) -> Result<Vec<u8>> {
        zstd::bulk::compress(data, self.config.level).map_err(|e| {
            StorageError::Compression(format!("Zstd compression failed: {}", e))
        })
    }

    fn decompress_zstd(&self, compressed_data: &[u8]) -> Result<Vec<u8>> {
        // Use streaming decompression to avoid buffer size guessing
        let mut decoder = zstd::Decoder::new(compressed_data)
            .map_err(|e| StorageError::Compression(format!("Failed to create zstd decoder: {}", e)))?;
        
        let mut decompressed = Vec::new();
        let mut buffer = vec![0; 4096]; // 4KB chunks
        
        loop {
            match decoder.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => decompressed.extend_from_slice(&buffer[..n]),
                Err(e) => return Err(StorageError::Compression(format!("Zstd decompression failed: {}", e))),
            }
        }
        
        Ok(decompressed)
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn algorithm(&self) -> &str {
        &self.config.algorithm
    }

    pub fn should_compress(&self, data_size: u64) -> bool {
        self.config.enabled && data_size >= self.config.min_size
    }

    pub fn compression_ratio(&self, original_size: usize, compressed_size: usize) -> f64 {
        if original_size == 0 {
            return 0.0;
        }
        (original_size as f64 - compressed_size as f64) / original_size as f64
    }
}

#[cfg(test)]
mod decompress_range_tests {
    use super::*;
    use crate::config::CompressionConfig;

    fn manager(algorithm: &str) -> CompressionManager {
        CompressionManager::new(Arc::new(CompressionConfig {
            enabled: true,
            algorithm: algorithm.to_string(),
            level: 3,
            min_size: 0,
        }))
    }

    fn sample_data(len: usize) -> Vec<u8> {
        // Deterministic, non-trivial content (not all-zeros) so a real
        // encode/decode round trip is actually being exercised.
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    fn assert_range_matches(algorithm: &str) {
        let mgr = manager(algorithm);
        let original = sample_data(500_000);
        let compressed = mgr.compress(&original).unwrap();

        // A middle slice - exercises both "discard prefix" and "stop before EOF".
        let (start, len) = (123_456, 10_000);
        let range = mgr.decompress_range(&compressed, start, len).unwrap().unwrap();
        assert_eq!(range, original[start..start + len]);

        // From the very start.
        let range = mgr.decompress_range(&compressed, 0, 1_000).unwrap().unwrap();
        assert_eq!(range, original[0..1_000]);

        // Right up to (and including) the last byte.
        let tail_start = original.len() - 1_000;
        let range = mgr.decompress_range(&compressed, tail_start, 1_000).unwrap().unwrap();
        assert_eq!(range, original[tail_start..]);
    }

    #[test]
    fn zstd_range_matches_full_decompress() {
        assert_range_matches("zstd");
    }

    #[test]
    fn gzip_range_matches_full_decompress() {
        assert_range_matches("gzip");
    }

    #[test]
    fn unrecognized_format_returns_none() {
        let mgr = manager("zstd");
        assert_eq!(mgr.decompress_range(&[0xFF, 1, 2, 3], 0, 2).unwrap(), None);
    }

    #[test]
    fn empty_input_returns_empty() {
        let mgr = manager("zstd");
        assert_eq!(mgr.decompress_range(&[], 0, 10).unwrap(), Some(Vec::new()));
    }
} 