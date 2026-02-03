//! Hash trait extensions for drbot.
//!
//! This crate provides:
//! - Hash extensions
//! - Quick hash functions
//! - Combining hashes

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use thiserror::Error;

/// Hash extension error types.
#[derive(Error, Debug, Clone)]
pub enum HashExtError {
    #[error("Hash collision detected")]
    Collision,

    #[error("Invalid hash: {0}")]
    Invalid(String),
}

/// Result type for hash operations.
pub type Result<T> = std::result::Result<T, HashExtError>;

/// Compute hash of a value.
pub fn hash<T: Hash + ?Sized>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Compute hash with seed.
pub fn hash_with_seed<T: Hash + ?Sized>(value: &T, seed: u64) -> u64 {
    let mut hasher = SeededHasher::new(seed);
    value.hash(&mut hasher);
    hasher.finish()
}

/// Seeded hasher.
pub struct SeededHasher {
    state: u64,
}

impl SeededHasher {
    /// Create with seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}

impl Hasher for SeededHasher {
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state = self.state.wrapping_mul(31).wrapping_add(*byte as u64);
        }
    }
}

/// Hash extension trait.
pub trait HashExt: Hash {
    /// Compute hash.
    fn compute_hash(&self) -> u64 {
        hash(self)
    }

    /// Compute hash with seed.
    fn compute_hash_seeded(&self, seed: u64) -> u64 {
        hash_with_seed(self, seed)
    }

    /// Check if hashes are equal.
    fn hash_eq<T: Hash>(&self, other: &T) -> bool {
        hash(self) == hash(other)
    }
}

impl<T: Hash> HashExt for T {}

/// Combine two hashes.
pub fn combine_hashes(h1: u64, h2: u64) -> u64 {
    h1.wrapping_mul(31).wrapping_add(h2)
}

/// Combine multiple hashes.
pub fn combine_many(hashes: &[u64]) -> u64 {
    hashes.iter().fold(0u64, |acc, &h| combine_hashes(acc, h))
}

/// Hash builder for combining multiple values.
#[derive(Debug, Clone, Default)]
pub struct HashBuilder {
    hasher: DefaultHasher,
}

impl HashBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self {
            hasher: DefaultHasher::new(),
        }
    }

    /// Add value to hash.
    pub fn add<T: Hash>(mut self, value: &T) -> Self {
        value.hash(&mut self.hasher);
        self
    }

    /// Add bytes to hash.
    pub fn add_bytes(mut self, bytes: &[u8]) -> Self {
        self.hasher.write(bytes);
        self
    }

    /// Add u64 to hash.
    pub fn add_u64(mut self, value: u64) -> Self {
        self.hasher.write_u64(value);
        self
    }

    /// Finish and get hash.
    pub fn finish(self) -> u64 {
        self.hasher.finish()
    }
}

/// FNV-1a hash.
pub fn fnv1a(bytes: &[u8]) -> u64 {
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
pub fn djb2(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 5381;
    for byte in bytes {
        hash = hash.wrapping_mul(33).wrapping_add(*byte as u64);
    }
    hash
}

/// SDBM hash.
pub fn sdbm(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0;
    for byte in bytes {
        hash = (*byte as u64)
            .wrapping_add(hash.wrapping_shl(6))
            .wrapping_add(hash.wrapping_shl(16))
            .wrapping_sub(hash);
    }
    hash
}

/// Hash a string.
pub fn hash_str(s: &str) -> u64 {
    hash(&s)
}

/// Hash bytes.
pub fn hash_bytes(bytes: &[u8]) -> u64 {
    fnv1a(bytes)
}

/// Consistent hash for distribution.
pub fn consistent_hash(key: u64, buckets: u32) -> u32 {
    if buckets == 0 {
        return 0;
    }
    (key % buckets as u64) as u32
}

/// Jump consistent hash.
pub fn jump_consistent_hash(key: u64, buckets: i32) -> i32 {
    if buckets <= 0 {
        return 0;
    }

    let mut b: i64 = -1;
    let mut j: i64 = 0;
    let mut key = key;

    while j < buckets as i64 {
        b = j;
        key = key.wrapping_mul(2862933555777941757).wrapping_add(1);
        j = ((b.wrapping_add(1) as f64)
            * (((1u64 << 31) as f64) / ((key >> 33).wrapping_add(1) as f64))) as i64;
    }

    b as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash() {
        let h1 = hash(&42);
        let h2 = hash(&42);
        assert_eq!(h1, h2);

        let h3 = hash(&43);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_hash_builder() {
        let h1 = HashBuilder::new().add(&"hello").add(&42).finish();

        let h2 = HashBuilder::new().add(&"hello").add(&42).finish();

        assert_eq!(h1, h2);
    }

    #[test]
    fn test_fnv1a() {
        let h1 = fnv1a(b"hello");
        let h2 = fnv1a(b"hello");
        assert_eq!(h1, h2);

        let h3 = fnv1a(b"world");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_hash_ext() {
        assert!(42.hash_eq(&42));
        assert!(!42.hash_eq(&43));
    }

    #[test]
    fn test_consistent_hash() {
        let bucket = consistent_hash(12345, 10);
        assert!(bucket < 10);
    }
}
