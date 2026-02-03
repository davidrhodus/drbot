//! LZ4 compression for drbot.
//!
//! This crate provides:
//! - LZ4 compression and decompression
//! - High-speed compression
//! - Block and frame formats
//!
//! Note: This is a stub implementation. Add `lz4` dependency for actual compression.

use std::io::{Read, Write};
use thiserror::Error;

/// LZ4 error types.
#[derive(Error, Debug)]
pub enum Lz4Error {
    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("Decompression error: {0}")]
    DecompressionError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid data")]
    InvalidData,
}

/// Result type for LZ4 operations.
pub type Result<T> = std::result::Result<T, Lz4Error>;

/// LZ4 frame magic number.
const MAGIC: [u8; 4] = [0x04, 0x22, 0x4d, 0x18];

/// Compression mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Fast compression (default).
    Fast,
    /// High compression ratio.
    HighCompression,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Fast
    }
}

/// LZ4 compressor.
pub struct Compressor {
    mode: Mode,
}

impl Compressor {
    /// Create new compressor.
    pub fn new() -> Self {
        Self {
            mode: Mode::default(),
        }
    }

    /// Create with specific mode.
    pub fn with_mode(mode: Mode) -> Self {
        Self { mode }
    }

    /// Compress data.
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Stub: Add lz4 dependency for real implementation
        let mut result = Vec::with_capacity(data.len() + 8);
        result.extend_from_slice(&MAGIC);
        result.extend_from_slice(data);
        Ok(result)
    }

    /// Compress to writer.
    pub fn compress_to<W: Write>(&self, data: &[u8], mut writer: W) -> Result<usize> {
        let compressed = self.compress(data)?;
        writer.write_all(&compressed)?;
        Ok(compressed.len())
    }

    /// Get mode.
    pub fn mode(&self) -> Mode {
        self.mode
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new()
    }
}

/// LZ4 decompressor.
pub struct Decompressor;

impl Decompressor {
    /// Create new decompressor.
    pub fn new() -> Self {
        Self
    }

    /// Decompress data.
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 4 || data[..4] != MAGIC {
            return Err(Lz4Error::InvalidData);
        }
        Ok(data[4..].to_vec())
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

/// Compress with fast mode.
pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
    Compressor::new().compress(data)
}

/// Decompress data.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    Decompressor::new().decompress(data)
}

/// Compress with high compression mode.
pub fn compress_hc(data: &[u8]) -> Result<Vec<u8>> {
    Compressor::with_mode(Mode::HighCompression).compress(data)
}

/// Get max compressed size for given input size.
pub fn max_compressed_size(input_size: usize) -> usize {
    // LZ4 worst case: input + (input/255) + 16
    input_size + (input_size / 255) + 16
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
    fn test_modes() {
        let data = b"test data";
        let fast = compress(data).unwrap();
        let hc = compress_hc(data).unwrap();
        assert!(!fast.is_empty());
        assert!(!hc.is_empty());
    }
}
