//! Zstandard compression for drbot.
//!
//! This crate provides:
//! - Zstd compression and decompression
//! - Dictionary support
//! - Streaming compression
//!
//! Note: This is a stub implementation. Add `zstd` dependency for actual compression.

use std::io::{Read, Write};
use thiserror::Error;

/// Zstd error types.
#[derive(Error, Debug)]
pub enum ZstdError {
    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("Decompression error: {0}")]
    DecompressionError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid data")]
    InvalidData,
}

/// Result type for zstd operations.
pub type Result<T> = std::result::Result<T, ZstdError>;

/// Zstd magic number.
const MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// Compression level (-7 to 22, default 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level(pub i32);

impl Level {
    /// Fastest compression.
    pub const FAST: Level = Level(-7);
    /// Default compression.
    pub const DEFAULT: Level = Level(3);
    /// Better compression.
    pub const BETTER: Level = Level(9);
    /// Best compression.
    pub const BEST: Level = Level(22);
}

impl Default for Level {
    fn default() -> Self {
        Level::DEFAULT
    }
}

/// Zstd compressor.
pub struct Compressor {
    level: Level,
}

impl Compressor {
    /// Create new compressor.
    pub fn new() -> Self {
        Self {
            level: Level::default(),
        }
    }

    /// Create with specific level.
    pub fn with_level(level: Level) -> Self {
        Self { level }
    }

    /// Compress data.
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Stub: Add zstd dependency for real implementation
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

    /// Get level.
    pub fn level(&self) -> Level {
        self.level
    }
}

impl Default for Compressor {
    fn default() -> Self {
        Self::new()
    }
}

/// Zstd decompressor.
pub struct Decompressor;

impl Decompressor {
    /// Create new decompressor.
    pub fn new() -> Self {
        Self
    }

    /// Decompress data.
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Check magic number
        if data.len() < 4 || data[..4] != MAGIC {
            return Err(ZstdError::InvalidData);
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

/// Compress with default settings.
pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
    Compressor::new().compress(data)
}

/// Decompress data.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    Decompressor::new().decompress(data)
}

/// Compress with level.
pub fn compress_level(data: &[u8], level: Level) -> Result<Vec<u8>> {
    Compressor::with_level(level).compress(data)
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
    fn test_levels() {
        assert_eq!(Level::FAST.0, -7);
        assert_eq!(Level::DEFAULT.0, 3);
        assert_eq!(Level::BEST.0, 22);
    }
}
