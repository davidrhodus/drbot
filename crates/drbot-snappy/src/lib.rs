//! Snappy compression for drbot.
//!
//! This crate provides:
//! - Snappy compression and decompression
//! - Very fast compression
//! - Framing format support
//!
//! Note: This is a stub implementation. Add `snap` dependency for actual compression.

use std::io::{Read, Write};
use thiserror::Error;

/// Snappy error types.
#[derive(Error, Debug)]
pub enum SnappyError {
    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("Decompression error: {0}")]
    DecompressionError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid data")]
    InvalidData,
}

/// Result type for snappy operations.
pub type Result<T> = std::result::Result<T, SnappyError>;

/// Snappy stream identifier.
const STREAM_IDENTIFIER: [u8; 10] = [0xff, 0x06, 0x00, 0x00, 0x73, 0x4e, 0x61, 0x50, 0x70, 0x59];

/// Snappy compressor.
pub struct Compressor;

impl Compressor {
    /// Create new compressor.
    pub fn new() -> Self {
        Self
    }

    /// Compress data (raw format).
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Stub: Add snap dependency for real implementation
        let mut result = Vec::with_capacity(data.len() + 5);
        // Simple length prefix
        let len = data.len() as u32;
        result.extend_from_slice(&len.to_le_bytes());
        result.extend_from_slice(data);
        Ok(result)
    }

    /// Compress data (framing format).
    pub fn compress_framed(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(data.len() + 20);
        result.extend_from_slice(&STREAM_IDENTIFIER);
        result.extend_from_slice(data);
        Ok(result)
    }

    /// Compress to writer.
    pub fn compress_to<W: Write>(&self, data: &[u8], mut writer: W) -> Result<usize> {
        let compressed = self.compress(data)?;
        writer.write_all(&compressed)?;
        Ok(compressed.len())
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new()
    }
}

/// Snappy decompressor.
pub struct Decompressor;

impl Decompressor {
    /// Create new decompressor.
    pub fn new() -> Self {
        Self
    }

    /// Decompress data (raw format).
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 4 {
            return Err(SnappyError::InvalidData);
        }
        Ok(data[4..].to_vec())
    }

    /// Decompress framed data.
    pub fn decompress_framed(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 10 || data[..10] != STREAM_IDENTIFIER {
            return Err(SnappyError::InvalidData);
        }
        Ok(data[10..].to_vec())
    }

    /// Decompress from reader.
    pub fn decompress_from<R: Read>(&self, mut reader: R) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        self.decompress(&data)
    }
}

impl Default for Decompressor {
    fn default() -> Self {
        Self::new()
    }
}

/// Compress data.
pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
    Compressor::new().compress(data)
}

/// Decompress data.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    Decompressor::new().decompress(data)
}

/// Compress with framing.
pub fn compress_framed(data: &[u8]) -> Result<Vec<u8>> {
    Compressor::new().compress_framed(data)
}

/// Decompress framed data.
pub fn decompress_framed(data: &[u8]) -> Result<Vec<u8>> {
    Decompressor::new().decompress_framed(data)
}

/// Get max compressed size.
pub fn max_compressed_size(input_size: usize) -> usize {
    // Snappy worst case formula
    32 + input_size + input_size / 6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress() {
        let data = b"Hello, World!";
        let compressed = compress(data).unwrap();
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn test_framed() {
        let data = b"Hello, World!";
        let compressed = compress_framed(data).unwrap();
        let decompressed = decompress_framed(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }
}
