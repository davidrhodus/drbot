//! Compression utilities for drbot.
//!
//! This crate provides:
//! - Compression trait abstraction
//! - Simple RLE compression
//! - Compression statistics

use thiserror::Error;

/// Compression error types.
#[derive(Error, Debug, Clone)]
pub enum CompressionError {
    #[error("Compression failed: {0}")]
    CompressionFailed(String),

    #[error("Decompression failed: {0}")]
    DecompressionFailed(String),

    #[error("Invalid data")]
    InvalidData,

    #[error("Buffer too small")]
    BufferTooSmall,
}

/// Result type for compression operations.
pub type Result<T> = std::result::Result<T, CompressionError>;

/// Compressor trait.
pub trait Compressor {
    /// Compress data.
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// Decompress data.
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// Get compression name.
    fn name(&self) -> &str;
}

/// No compression (identity).
pub struct NoCompression;

impl Compressor for NoCompression {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }

    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }

    fn name(&self) -> &str {
        "none"
    }
}

/// Simple Run-Length Encoding compression.
pub struct RleCompressor;

impl RleCompressor {
    /// Create new RLE compressor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for RleCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl Compressor for RleCompressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        let mut result = Vec::new();
        let mut i = 0;

        while i < data.len() {
            let byte = data[i];
            let mut count = 1u8;

            while i + (count as usize) < data.len()
                && data[i + (count as usize)] == byte
                && count < 255
            {
                count += 1;
            }

            // Only use RLE for runs of 4+ (saves space)
            if count >= 4 {
                result.push(0x00); // Escape byte
                result.push(count);
                result.push(byte);
            } else {
                for _ in 0..count {
                    if byte == 0x00 {
                        result.push(0x00);
                        result.push(1);
                        result.push(0x00);
                    } else {
                        result.push(byte);
                    }
                }
            }

            i += count as usize;
        }

        Ok(result)
    }

    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut result = Vec::new();
        let mut i = 0;

        while i < data.len() {
            if data[i] == 0x00 {
                if i + 2 >= data.len() {
                    return Err(CompressionError::InvalidData);
                }
                let count = data[i + 1] as usize;
                let byte = data[i + 2];
                for _ in 0..count {
                    result.push(byte);
                }
                i += 3;
            } else {
                result.push(data[i]);
                i += 1;
            }
        }

        Ok(result)
    }

    fn name(&self) -> &str {
        "rle"
    }
}

/// Compression statistics.
#[derive(Debug, Clone)]
pub struct CompressionStats {
    pub original_size: usize,
    pub compressed_size: usize,
    pub ratio: f64,
}

impl CompressionStats {
    /// Calculate stats.
    pub fn new(original_size: usize, compressed_size: usize) -> Self {
        let ratio = if original_size == 0 {
            1.0
        } else {
            compressed_size as f64 / original_size as f64
        };
        Self {
            original_size,
            compressed_size,
            ratio,
        }
    }

    /// Get space savings as percentage.
    pub fn savings_percent(&self) -> f64 {
        (1.0 - self.ratio) * 100.0
    }
}

/// Compress and return stats.
pub fn compress_with_stats<C: Compressor>(
    compressor: &C,
    data: &[u8],
) -> Result<(Vec<u8>, CompressionStats)> {
    let compressed = compressor.compress(data)?;
    let stats = CompressionStats::new(data.len(), compressed.len());
    Ok((compressed, stats))
}

/// Compression level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionLevel {
    None,
    Fast,
    Default,
    Best,
}

impl Default for CompressionLevel {
    fn default() -> Self {
        Self::Default
    }
}

/// Dictionary-based compression placeholder.
pub struct DictionaryCompressor {
    dictionary: Vec<Vec<u8>>,
}

impl DictionaryCompressor {
    /// Create with dictionary.
    pub fn new(dictionary: Vec<Vec<u8>>) -> Self {
        Self { dictionary }
    }

    /// Create empty.
    pub fn empty() -> Self {
        Self {
            dictionary: Vec::new(),
        }
    }

    /// Add pattern to dictionary.
    pub fn add_pattern(&mut self, pattern: Vec<u8>) {
        if !self.dictionary.contains(&pattern) {
            self.dictionary.push(pattern);
        }
    }
}

impl Compressor for DictionaryCompressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Simple placeholder - just return original data
        // Real implementation would replace patterns with references
        Ok(data.to_vec())
    }

    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }

    fn name(&self) -> &str {
        "dictionary"
    }
}

/// Estimate compression ratio without actually compressing.
pub fn estimate_compression_ratio(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 1.0;
    }

    // Count unique bytes
    let mut seen = [false; 256];
    for &byte in data {
        seen[byte as usize] = true;
    }
    let unique_count = seen.iter().filter(|&&b| b).count();

    // Estimate based on entropy
    let entropy = unique_count as f64 / 256.0;
    0.3 + (entropy * 0.7) // Between 30% and 100%
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_compression() {
        let comp = NoCompression;
        let data = b"Hello, World!";
        let compressed = comp.compress(data).unwrap();
        let decompressed = comp.decompress(&compressed).unwrap();
        assert_eq!(data.to_vec(), decompressed);
    }

    #[test]
    fn test_rle_no_runs() {
        let comp = RleCompressor::new();
        let data = b"abcd";
        let compressed = comp.compress(data).unwrap();
        let decompressed = comp.decompress(&compressed).unwrap();
        assert_eq!(data.to_vec(), decompressed);
    }

    #[test]
    fn test_rle_with_runs() {
        let comp = RleCompressor::new();
        let data = b"aaaaabbbbbccccc";
        let compressed = comp.compress(data).unwrap();
        let decompressed = comp.decompress(&compressed).unwrap();
        assert_eq!(data.to_vec(), decompressed);
        // Should be smaller due to runs
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_compression_stats() {
        let stats = CompressionStats::new(100, 50);
        assert_eq!(stats.ratio, 0.5);
        assert_eq!(stats.savings_percent(), 50.0);
    }

    #[test]
    fn test_compress_with_stats() {
        let comp = RleCompressor::new();
        let data = vec![0xAA; 100]; // Highly compressible
        let (compressed, stats) = compress_with_stats(&comp, &data).unwrap();

        assert!(compressed.len() < data.len());
        assert!(stats.savings_percent() > 0.0);
    }
}
