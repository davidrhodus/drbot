//! Hashing utilities for drbot.
//!
//! This crate provides:
//! - SHA-256, SHA-384, SHA-512 hashing
//! - HMAC
//! - Content-based hashing
//! - Hash verification

use ring::digest::{self, Context};
use ring::hmac;
use thiserror::Error;

/// Hashing error types.
#[derive(Error, Debug)]
pub enum HashError {
    #[error("Invalid key: {0}")]
    InvalidKey(String),

    #[error("Hash verification failed")]
    VerificationFailed,
}

/// Result type for hashing operations.
pub type Result<T> = std::result::Result<T, HashError>;

/// Hash algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Sha256,
    Sha384,
    Sha512,
}

impl Algorithm {
    fn ring_algorithm(&self) -> &'static digest::Algorithm {
        match self {
            Algorithm::Sha256 => &digest::SHA256,
            Algorithm::Sha384 => &digest::SHA384,
            Algorithm::Sha512 => &digest::SHA512,
        }
    }

    fn hmac_algorithm(&self) -> hmac::Algorithm {
        match self {
            Algorithm::Sha256 => hmac::HMAC_SHA256,
            Algorithm::Sha384 => hmac::HMAC_SHA384,
            Algorithm::Sha512 => hmac::HMAC_SHA512,
        }
    }

    /// Get output length in bytes.
    pub fn output_len(&self) -> usize {
        match self {
            Algorithm::Sha256 => 32,
            Algorithm::Sha384 => 48,
            Algorithm::Sha512 => 64,
        }
    }
}

/// Hash result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hash {
    bytes: Vec<u8>,
    algorithm: Algorithm,
}

impl Hash {
    /// Get hash bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Get hash as hex string.
    pub fn to_hex(&self) -> String {
        self.bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Get algorithm.
    pub fn algorithm(&self) -> Algorithm {
        self.algorithm
    }

    /// Verify hash matches data.
    pub fn verify(&self, data: &[u8]) -> bool {
        let computed = Hasher::hash(self.algorithm, data);
        computed.bytes == self.bytes
    }
}

/// Hasher for computing hashes.
pub struct Hasher;

impl Hasher {
    /// Hash data with algorithm.
    pub fn hash(algorithm: Algorithm, data: &[u8]) -> Hash {
        let digest = digest::digest(algorithm.ring_algorithm(), data);
        Hash {
            bytes: digest.as_ref().to_vec(),
            algorithm,
        }
    }

    /// SHA-256 hash.
    pub fn sha256(data: &[u8]) -> Hash {
        Self::hash(Algorithm::Sha256, data)
    }

    /// SHA-384 hash.
    pub fn sha384(data: &[u8]) -> Hash {
        Self::hash(Algorithm::Sha384, data)
    }

    /// SHA-512 hash.
    pub fn sha512(data: &[u8]) -> Hash {
        Self::hash(Algorithm::Sha512, data)
    }

    /// Hash string.
    pub fn hash_str(algorithm: Algorithm, data: &str) -> Hash {
        Self::hash(algorithm, data.as_bytes())
    }
}

/// Streaming hasher for large data.
pub struct StreamingHasher {
    context: Context,
    algorithm: Algorithm,
}

impl StreamingHasher {
    /// Create new streaming hasher.
    pub fn new(algorithm: Algorithm) -> Self {
        Self {
            context: Context::new(algorithm.ring_algorithm()),
            algorithm,
        }
    }

    /// Update with data.
    pub fn update(&mut self, data: &[u8]) {
        self.context.update(data);
    }

    /// Finalize and get hash.
    pub fn finalize(self) -> Hash {
        let digest = self.context.finish();
        Hash {
            bytes: digest.as_ref().to_vec(),
            algorithm: self.algorithm,
        }
    }
}

/// HMAC generator.
pub struct Hmac {
    key: hmac::Key,
    algorithm: Algorithm,
}

impl Hmac {
    /// Create new HMAC with key.
    pub fn new(algorithm: Algorithm, key: &[u8]) -> Self {
        Self {
            key: hmac::Key::new(algorithm.hmac_algorithm(), key),
            algorithm,
        }
    }

    /// Sign data.
    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        let tag = hmac::sign(&self.key, data);
        tag.as_ref().to_vec()
    }

    /// Sign and return hex.
    pub fn sign_hex(&self, data: &[u8]) -> String {
        self.sign(data)
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }

    /// Verify signature.
    pub fn verify(&self, data: &[u8], signature: &[u8]) -> Result<()> {
        hmac::verify(&self.key, data, signature).map_err(|_| HashError::VerificationFailed)
    }
}

/// Content hasher for deduplication.
pub struct ContentHasher {
    algorithm: Algorithm,
}

impl ContentHasher {
    /// Create new content hasher.
    pub fn new(algorithm: Algorithm) -> Self {
        Self { algorithm }
    }

    /// Create with SHA-256.
    pub fn sha256() -> Self {
        Self::new(Algorithm::Sha256)
    }

    /// Hash content and return hex.
    pub fn hash(&self, content: &[u8]) -> String {
        Hasher::hash(self.algorithm, content).to_hex()
    }

    /// Hash string content.
    pub fn hash_str(&self, content: &str) -> String {
        self.hash(content.as_bytes())
    }

    /// Hash multiple chunks.
    pub fn hash_chunks<'a>(&self, chunks: impl Iterator<Item = &'a [u8]>) -> String {
        let mut hasher = StreamingHasher::new(self.algorithm);
        for chunk in chunks {
            hasher.update(chunk);
        }
        hasher.finalize().to_hex()
    }
}

impl Default for ContentHasher {
    fn default() -> Self {
        Self::sha256()
    }
}

/// Checksum utilities.
pub struct Checksum;

impl Checksum {
    /// CRC32 checksum.
    pub fn crc32(data: &[u8]) -> u32 {
        const CRC32_TABLE: [u32; 256] = {
            let mut table = [0u32; 256];
            let mut i = 0;
            while i < 256 {
                let mut crc = i as u32;
                let mut j = 0;
                while j < 8 {
                    if crc & 1 == 1 {
                        crc = (crc >> 1) ^ 0xEDB88320;
                    } else {
                        crc >>= 1;
                    }
                    j += 1;
                }
                table[i] = crc;
                i += 1;
            }
            table
        };

        let mut crc = 0xFFFFFFFF;
        for &byte in data {
            let index = ((crc ^ byte as u32) & 0xFF) as usize;
            crc = (crc >> 8) ^ CRC32_TABLE[index];
        }
        !crc
    }

    /// Adler32 checksum.
    pub fn adler32(data: &[u8]) -> u32 {
        const MOD_ADLER: u32 = 65521;

        let mut a: u32 = 1;
        let mut b: u32 = 0;

        for &byte in data {
            a = (a + byte as u32) % MOD_ADLER;
            b = (b + a) % MOD_ADLER;
        }

        (b << 16) | a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256() {
        let hash = Hasher::sha256(b"hello");
        assert_eq!(
            hash.to_hex(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn test_sha512() {
        let hash = Hasher::sha512(b"hello");
        assert_eq!(hash.as_bytes().len(), 64);
    }

    #[test]
    fn test_streaming_hasher() {
        let mut hasher = StreamingHasher::new(Algorithm::Sha256);
        hasher.update(b"hel");
        hasher.update(b"lo");
        let hash = hasher.finalize();

        assert_eq!(hash, Hasher::sha256(b"hello"));
    }

    #[test]
    fn test_hmac() {
        let hmac = Hmac::new(Algorithm::Sha256, b"secret");
        let signature = hmac.sign(b"message");

        assert!(hmac.verify(b"message", &signature).is_ok());
        assert!(hmac.verify(b"other", &signature).is_err());
    }

    #[test]
    fn test_content_hasher() {
        let hasher = ContentHasher::sha256();
        let hash1 = hasher.hash(b"content");
        let hash2 = hasher.hash(b"content");
        let hash3 = hasher.hash(b"different");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_hash_verify() {
        let hash = Hasher::sha256(b"hello");
        assert!(hash.verify(b"hello"));
        assert!(!hash.verify(b"world"));
    }

    #[test]
    fn test_crc32() {
        let crc = Checksum::crc32(b"hello");
        assert_eq!(crc, 0x3610A686);
    }

    #[test]
    fn test_adler32() {
        let adler = Checksum::adler32(b"hello");
        assert_eq!(adler, 0x062C0215);
    }
}
