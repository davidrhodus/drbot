//! Data signature utilities for drbot.
//!
//! This crate provides:
//! - Data signing
//! - Signature verification
//! - HMAC-like signatures

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use thiserror::Error;

/// Signature error types.
#[derive(Error, Debug, Clone)]
pub enum SignatureError {
    #[error("Invalid signature")]
    Invalid,

    #[error("Signature mismatch")]
    Mismatch,

    #[error("Missing key")]
    MissingKey,

    #[error("Expired signature")]
    Expired,
}

/// Result type for signature operations.
pub type Result<T> = std::result::Result<T, SignatureError>;

/// A data signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Signature {
    value: u64,
}

impl Signature {
    /// Create from value.
    pub fn new(value: u64) -> Self {
        Self { value }
    }

    /// Get value.
    pub fn value(&self) -> u64 {
        self.value
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        format!("{:016x}", self.value)
    }

    /// Parse from hex string.
    pub fn from_hex(s: &str) -> Result<Self> {
        u64::from_str_radix(s, 16)
            .map(Self::new)
            .map_err(|_| SignatureError::Invalid)
    }

    /// Verify against expected.
    pub fn verify(&self, expected: &Signature) -> Result<()> {
        if self.value == expected.value {
            Ok(())
        } else {
            Err(SignatureError::Mismatch)
        }
    }
}

/// Sign data with key.
pub fn sign(data: &[u8], key: &[u8]) -> Signature {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    data.hash(&mut hasher);
    key.hash(&mut hasher); // Key again for added security
    Signature::new(hasher.finish())
}

/// Verify signature.
pub fn verify(data: &[u8], key: &[u8], signature: &Signature) -> Result<()> {
    let expected = sign(data, key);
    signature.verify(&expected)
}

/// Sign string.
pub fn sign_str(data: &str, key: &str) -> Signature {
    sign(data.as_bytes(), key.as_bytes())
}

/// Signer with key.
#[derive(Clone)]
pub struct Signer {
    key: Vec<u8>,
}

impl Signer {
    /// Create with key.
    pub fn new(key: &[u8]) -> Self {
        Self { key: key.to_vec() }
    }

    /// Create from string key.
    pub fn from_str(key: &str) -> Self {
        Self::new(key.as_bytes())
    }

    /// Sign data.
    pub fn sign(&self, data: &[u8]) -> Signature {
        sign(data, &self.key)
    }

    /// Verify signature.
    pub fn verify(&self, data: &[u8], signature: &Signature) -> Result<()> {
        verify(data, &self.key, signature)
    }

    /// Sign string.
    pub fn sign_str(&self, data: &str) -> Signature {
        self.sign(data.as_bytes())
    }
}

/// Timestamped signature.
#[derive(Debug, Clone)]
pub struct TimestampedSignature {
    signature: Signature,
    timestamp: u64,
}

impl TimestampedSignature {
    /// Create new.
    pub fn new(signature: Signature, timestamp: u64) -> Self {
        Self {
            signature,
            timestamp,
        }
    }

    /// Get signature.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Get timestamp.
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Check if expired.
    pub fn is_expired(&self, current_time: u64, max_age: u64) -> bool {
        current_time.saturating_sub(self.timestamp) > max_age
    }
}

/// Timestamped signer.
pub struct TimestampedSigner {
    signer: Signer,
}

impl TimestampedSigner {
    /// Create with key.
    pub fn new(key: &[u8]) -> Self {
        Self {
            signer: Signer::new(key),
        }
    }

    /// Sign with timestamp.
    pub fn sign(&self, data: &[u8], timestamp: u64) -> TimestampedSignature {
        let mut combined = data.to_vec();
        combined.extend_from_slice(&timestamp.to_le_bytes());
        let signature = self.signer.sign(&combined);
        TimestampedSignature::new(signature, timestamp)
    }

    /// Verify with timestamp check.
    pub fn verify(
        &self,
        data: &[u8],
        sig: &TimestampedSignature,
        current_time: u64,
        max_age: u64,
    ) -> Result<()> {
        if sig.is_expired(current_time, max_age) {
            return Err(SignatureError::Expired);
        }

        let mut combined = data.to_vec();
        combined.extend_from_slice(&sig.timestamp.to_le_bytes());
        self.signer.verify(&combined, sig.signature())
    }
}

/// Chained signature.
#[derive(Debug, Clone)]
pub struct SignatureChain {
    signatures: Vec<Signature>,
}

impl SignatureChain {
    /// Create empty chain.
    pub fn new() -> Self {
        Self {
            signatures: Vec::new(),
        }
    }

    /// Add signature.
    pub fn add(&mut self, sig: Signature) {
        self.signatures.push(sig);
    }

    /// Get chain hash.
    pub fn chain_hash(&self) -> Signature {
        let mut hasher = DefaultHasher::new();
        for sig in &self.signatures {
            sig.value.hash(&mut hasher);
        }
        Signature::new(hasher.finish())
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.signatures.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.signatures.is_empty()
    }

    /// Get signatures.
    pub fn signatures(&self) -> &[Signature] {
        &self.signatures
    }
}

impl Default for SignatureChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_verify() {
        let sig = sign(b"hello", b"secret");
        assert!(verify(b"hello", b"secret", &sig).is_ok());
        assert!(verify(b"world", b"secret", &sig).is_err());
    }

    #[test]
    fn test_signer() {
        let signer = Signer::from_str("secret");
        let sig = signer.sign(b"hello");
        assert!(signer.verify(b"hello", &sig).is_ok());
    }

    #[test]
    fn test_hex() {
        let sig = Signature::new(12345);
        let hex = sig.to_hex();
        let parsed = Signature::from_hex(&hex).unwrap();
        assert_eq!(sig, parsed);
    }

    #[test]
    fn test_timestamped() {
        let signer = TimestampedSigner::new(b"secret");
        let sig = signer.sign(b"hello", 1000);

        assert!(signer.verify(b"hello", &sig, 1050, 100).is_ok());
        assert!(signer.verify(b"hello", &sig, 1200, 100).is_err()); // Expired
    }

    #[test]
    fn test_chain() {
        let mut chain = SignatureChain::new();
        chain.add(Signature::new(1));
        chain.add(Signature::new(2));
        assert_eq!(chain.len(), 2);
    }
}
