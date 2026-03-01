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