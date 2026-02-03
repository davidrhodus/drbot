//! Content fingerprinting for drbot.
//!
//! This crate provides:
//! - Content fingerprinting
//! - Similarity detection
//! - Change detection

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use thiserror::Error;

/// Fingerprint error types.
#[derive(Error, Debug, Clone)]
pub enum FingerprintError {
    #[error("Empty content")]
    EmptyContent,

    #[error("Invalid fingerprint")]
    Invalid,
}

/// Result type for fingerprint operations.
pub type Result<T> = std::result::Result<T, FingerprintError>;

/// A content fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint {
    hash: u64,
    size: usize,
}

impl Fingerprint {
    /// Create from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        Self {
            hash: hasher.finish(),
            size: bytes.len(),
        }
    }

    /// Create from string.
    pub fn from_str(s: &str) -> Self {
        Self::from_bytes(s.as_bytes())
    }

    /// Create from hashable value.
    pub fn from_value<T: Hash>(value: &T) -> Self {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        Self {
            hash: hasher.finish(),
            size: 0,
        }
    }

    /// Get hash value.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Get content size.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Check if matches.
    pub fn matches(&self, other: &Fingerprint) -> bool {
        self.hash == other.hash
    }
}

/// Fingerprintable trait.
pub trait Fingerprintable {
    /// Generate fingerprint.
    fn fingerprint(&self) -> Fingerprint;
}

impl Fingerprintable for [u8] {
    fn fingerprint(&self) -> Fingerprint {
        Fingerprint::from_bytes(self)
    }
}

impl Fingerprintable for str {
    fn fingerprint(&self) -> Fingerprint {
        Fingerprint::from_str(self)
    }
}

impl Fingerprintable for String {
    fn fingerprint(&self) -> Fingerprint {
        Fingerprint::from_str(self)
    }
}

impl Fingerprintable for Vec<u8> {
    fn fingerprint(&self) -> Fingerprint {
        Fingerprint::from_bytes(self)
    }
}

/// Rolling fingerprint for streaming content.
#[derive(Debug, Clone)]
pub struct RollingFingerprint {
    window_size: usize,
    buffer: Vec<u8>,
    hash: u64,
}

impl RollingFingerprint {
    /// Create with window size.
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            buffer: Vec::with_capacity(window_size),
            hash: 0,
        }
    }

    /// Add byte.
    pub fn push(&mut self, byte: u8) {
        if self.buffer.len() >= self.window_size {
            let removed = self.buffer.remove(0);
            self.hash = self.hash.wrapping_sub(
                (removed as u64).wrapping_mul(31u64.pow(self.window_size as u32 - 1)),
            );
        }
        self.buffer.push(byte);
        self.hash = self.hash.wrapping_mul(31).wrapping_add(byte as u64);
    }

    /// Get current hash.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Get current fingerprint.
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint {
            hash: self.hash,
            size: self.buffer.len(),
        }
    }

    /// Reset.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.hash = 0;
    }
}

/// Content diff detector.
#[derive(Debug, Clone)]
pub struct DiffDetector {
    fingerprint: Option<Fingerprint>,
}

impl DiffDetector {
    /// Create new detector.
    pub fn new() -> Self {
        Self { fingerprint: None }
    }

    /// Check if content changed.
    pub fn changed(&mut self, content: &[u8]) -> bool {
        let new_fp = Fingerprint::from_bytes(content);
        let changed = match &self.fingerprint {
            Some(old) => !old.matches(&new_fp),
            None => true,
        };
        self.fingerprint = Some(new_fp);
        changed
    }

    /// Get current fingerprint.
    pub fn current(&self) -> Option<&Fingerprint> {
        self.fingerprint.as_ref()
    }

    /// Reset.
    pub fn reset(&mut self) {
        self.fingerprint = None;
    }
}

impl Default for DiffDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Chunked fingerprint for large content.
#[derive(Debug, Clone)]
pub struct ChunkedFingerprint {
    chunks: Vec<Fingerprint>,
    chunk_size: usize,
}

impl ChunkedFingerprint {
    /// Create from bytes with chunk size.
    pub fn new(content: &[u8], chunk_size: usize) -> Self {
        let chunks = content
            .chunks(chunk_size)
            .map(Fingerprint::from_bytes)
            .collect();
        Self { chunks, chunk_size }
    }

    /// Get chunk count.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Get chunk fingerprints.
    pub fn chunks(&self) -> &[Fingerprint] {
        &self.chunks
    }

    /// Find different chunks.
    pub fn diff(&self, other: &ChunkedFingerprint) -> Vec<usize> {
        let mut diffs = Vec::new();
        let max_len = self.chunks.len().max(other.chunks.len());

        for i in 0..max_len {
            let same = match (self.chunks.get(i), other.chunks.get(i)) {
                (Some(a), Some(b)) => a.matches(b),
                _ => false,
            };
            if !same {
                diffs.push(i);
            }
        }

        diffs
    }

    /// Compute overall fingerprint.
    pub fn overall(&self) -> Fingerprint {
        let mut hasher = DefaultHasher::new();
        for chunk in &self.chunks {
            chunk.hash.hash(&mut hasher);
        }
        Fingerprint {
            hash: hasher.finish(),
            size: self.chunks.len() * self.chunk_size,
        }
    }
}

/// Simhash for similarity detection.
pub fn simhash(features: &[u64]) -> u64 {
    let mut v = [0i32; 64];

    for &feature in features {
        for i in 0..64 {
            if (feature >> i) & 1 == 1 {
                v[i] += 1;
            } else {
                v[i] -= 1;
            }
        }
    }

    let mut result = 0u64;
    for i in 0..64 {
        if v[i] > 0 {
            result |= 1 << i;
        }
    }
    result
}

/// Hamming distance between hashes.
pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Similarity score (0.0 to 1.0).
pub fn similarity(a: u64, b: u64) -> f64 {
    1.0 - (hamming_distance(a, b) as f64 / 64.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint() {
        let fp1 = Fingerprint::from_str("hello");
        let fp2 = Fingerprint::from_str("hello");
        let fp3 = Fingerprint::from_str("world");

        assert!(fp1.matches(&fp2));
        assert!(!fp1.matches(&fp3));
    }

    #[test]
    fn test_diff_detector() {
        let mut detector = DiffDetector::new();
        assert!(detector.changed(b"hello"));
        assert!(!detector.changed(b"hello"));
        assert!(detector.changed(b"world"));
    }

    #[test]
    fn test_chunked() {
        let content = b"hello world this is test";
        let chunked = ChunkedFingerprint::new(content, 5);
        assert!(chunked.chunk_count() > 0);
    }

    #[test]
    fn test_hamming() {
        assert_eq!(hamming_distance(0, 0), 0);
        assert_eq!(hamming_distance(0, 1), 1);
        assert_eq!(hamming_distance(0b1111, 0b0000), 4);
    }

    #[test]
    fn test_similarity() {
        assert_eq!(similarity(0, 0), 1.0);
        assert!(similarity(0, 1) > 0.9);
    }
}
