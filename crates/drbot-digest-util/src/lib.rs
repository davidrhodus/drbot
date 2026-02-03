//! Digest utilities for drbot.
//!
//! This crate provides:
//! - Digest computation
//! - Incremental digests
//! - Digest comparison

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use thiserror::Error;

/// Digest error types.
#[derive(Error, Debug, Clone)]
pub enum DigestError {
    #[error("Invalid digest")]
    Invalid,

    #[error("Digest mismatch")]
    Mismatch,

    #[error("Empty input")]
    Empty,
}

/// Result type for digest operations.
pub type Result<T> = std::result::Result<T, DigestError>;

/// A digest value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Digest {
    value: u64,
}

impl Digest {
    /// Create from value.
    pub fn new(value: u64) -> Self {
        Self { value }
    }

    /// Get value.
    pub fn value(&self) -> u64 {
        self.value
    }

    /// Convert to hex.
    pub fn to_hex(&self) -> String {
        format!("{:016x}", self.value)
    }

    /// Parse from hex.
    pub fn from_hex(s: &str) -> Result<Self> {
        u64::from_str_radix(s, 16)
            .map(Self::new)
            .map_err(|_| DigestError::Invalid)
    }

    /// Verify against expected.
    pub fn verify(&self, expected: &Digest) -> Result<()> {
        if self.value == expected.value {
            Ok(())
        } else {
            Err(DigestError::Mismatch)
        }
    }
}

/// Compute digest of bytes.
pub fn digest(data: &[u8]) -> Digest {
    let mut hasher = DefaultHasher::new();
    data.hash(&mut hasher);
    Digest::new(hasher.finish())
}

/// Compute digest of string.
pub fn digest_str(s: &str) -> Digest {
    digest(s.as_bytes())
}

/// Compute digest of hashable value.
pub fn digest_value<T: Hash>(value: &T) -> Digest {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    Digest::new(hasher.finish())
}

/// Incremental digest builder.
#[derive(Debug, Clone, Default)]
pub struct DigestBuilder {
    hasher: DefaultHasher,
}

impl DigestBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            hasher: DefaultHasher::new(),
        }
    }

    /// Add bytes.
    pub fn update(mut self, data: &[u8]) -> Self {
        self.hasher.write(data);
        self
    }

    /// Add string.
    pub fn update_str(self, s: &str) -> Self {
        self.update(s.as_bytes())
    }

    /// Add hashable value.
    pub fn update_value<T: Hash>(mut self, value: &T) -> Self {
        value.hash(&mut self.hasher);
        self
    }

    /// Finalize.
    pub fn finish(self) -> Digest {
        Digest::new(self.hasher.finish())
    }
}

/// Mutable digest builder.
#[derive(Debug, Default)]
pub struct DigestBuilderMut {
    hasher: DefaultHasher,
}

impl DigestBuilderMut {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            hasher: DefaultHasher::new(),
        }
    }

    /// Add bytes.
    pub fn update(&mut self, data: &[u8]) {
        self.hasher.write(data);
    }

    /// Add string.
    pub fn update_str(&mut self, s: &str) {
        self.update(s.as_bytes());
    }

    /// Add hashable value.
    pub fn update_value<T: Hash>(&mut self, value: &T) {
        value.hash(&mut self.hasher);
    }

    /// Finalize and reset.
    pub fn finish(&mut self) -> Digest {
        let result = Digest::new(self.hasher.finish());
        self.hasher = DefaultHasher::new();
        result
    }

    /// Reset without finalize.
    pub fn reset(&mut self) {
        self.hasher = DefaultHasher::new();
    }
}

/// Digest with size info.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SizedDigest {
    digest: Digest,
    size: u64,
}

impl SizedDigest {
    /// Create new.
    pub fn new(digest: Digest, size: u64) -> Self {
        Self { digest, size }
    }

    /// Compute from bytes.
    pub fn from_bytes(data: &[u8]) -> Self {
        Self {
            digest: digest(data),
            size: data.len() as u64,
        }
    }

    /// Get digest.
    pub fn digest(&self) -> &Digest {
        &self.digest
    }

    /// Get size.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Verify.
    pub fn verify(&self, expected: &SizedDigest) -> Result<()> {
        if self.digest == expected.digest && self.size == expected.size {
            Ok(())
        } else {
            Err(DigestError::Mismatch)
        }
    }
}

/// Multi-digest combining multiple algorithms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiDigest {
    digests: Vec<Digest>,
}

impl MultiDigest {
    /// Create new.
    pub fn new(digests: Vec<Digest>) -> Self {
        Self { digests }
    }

    /// Create from bytes using multiple hash functions.
    pub fn from_bytes(data: &[u8]) -> Self {
        let d1 = digest(data);
        let d2 = Digest::new(fnv1a(data));
        let d3 = Digest::new(djb2(data));
        Self {
            digests: vec![d1, d2, d3],
        }
    }

    /// Get digests.
    pub fn digests(&self) -> &[Digest] {
        &self.digests
    }

    /// Verify all digests match.
    pub fn verify(&self, expected: &MultiDigest) -> Result<()> {
        if self.digests == expected.digests {
            Ok(())
        } else {
            Err(DigestError::Mismatch)
        }
    }
}

/// FNV-1a hash.
fn fnv1a(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;

    let mut hash = FNV_OFFSET;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// DJB2 hash.
fn djb2(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 5381;
    for byte in bytes {
        hash = hash.wrapping_mul(33).wrapping_add(*byte as u64);
    }
    hash
}

/// Digestable trait.
pub trait Digestable {
    /// Compute digest.
    fn digest(&self) -> Digest;
}

impl Digestable for [u8] {
    fn digest(&self) -> Digest {
        digest(self)
    }
}

impl Digestable for str {
    fn digest(&self) -> Digest {
        digest_str(self)
    }
}

impl Digestable for String {
    fn digest(&self) -> Digest {
        digest_str(self)
    }
}

impl Digestable for Vec<u8> {
    fn digest(&self) -> Digest {
        digest(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digest() {
        let d1 = digest(b"hello");
        let d2 = digest(b"hello");
        let d3 = digest(b"world");

        assert_eq!(d1, d2);
        assert_ne!(d1, d3);
    }

    #[test]
    fn test_builder() {
        let d1 = DigestBuilder::new()
            .update(b"hello")
            .update(b"world")
            .finish();

        let d2 = DigestBuilder::new()
            .update(b"hello")
            .update(b"world")
            .finish();

        assert_eq!(d1, d2);
    }

    #[test]
    fn test_hex() {
        let d = digest(b"test");
        let hex = d.to_hex();
        let parsed = Digest::from_hex(&hex).unwrap();
        assert_eq!(d, parsed);
    }

    #[test]
    fn test_sized() {
        let sd = SizedDigest::from_bytes(b"hello");
        assert_eq!(sd.size(), 5);
    }

    #[test]
    fn test_multi() {
        let md1 = MultiDigest::from_bytes(b"hello");
        let md2 = MultiDigest::from_bytes(b"hello");
        assert!(md1.verify(&md2).is_ok());
    }
}
