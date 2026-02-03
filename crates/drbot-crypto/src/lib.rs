//! Cryptographic utilities for drbot.
//!
//! This crate provides:
//! - Symmetric encryption (AES-GCM)
//! - Asymmetric encryption (RSA, Ed25519)
//! - Digital signatures
//! - Key generation and management

use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Crypto error types.
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Invalid key: {0}")]
    InvalidKey(String),

    #[error("Invalid nonce")]
    InvalidNonce,

    #[error("Random generation failed")]
    RandomFailed,

    #[error("Signature verification failed")]
    SignatureVerificationFailed,
}

/// Result type for crypto operations.
pub type Result<T> = std::result::Result<T, CryptoError>;

/// Encrypted data with nonce.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    /// Ciphertext.
    pub ciphertext: Vec<u8>,
    /// Nonce used for encryption.
    pub nonce: [u8; 12],
}

/// AES-256-GCM encryptor.
pub struct AesGcmEncryptor {
    key: LessSafeKey,
    rng: SystemRandom,
}

impl AesGcmEncryptor {
    /// Create from key bytes (must be 32 bytes).
    pub fn new(key_bytes: &[u8]) -> Result<Self> {
        if key_bytes.len() != 32 {
            return Err(CryptoError::InvalidKey("Key must be 32 bytes".to_string()));
        }

        let unbound_key = UnboundKey::new(&AES_256_GCM, key_bytes)
            .map_err(|_| CryptoError::InvalidKey("Invalid key".to_string()))?;

        Ok(Self {
            key: LessSafeKey::new(unbound_key),
            rng: SystemRandom::new(),
        })
    }

    /// Generate random key.
    pub fn generate_key() -> Result<[u8; 32]> {
        let rng = SystemRandom::new();
        let mut key = [0u8; 32];
        rng.fill(&mut key).map_err(|_| CryptoError::RandomFailed)?;
        Ok(key)
    }

    /// Encrypt plaintext.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedData> {
        let mut nonce_bytes = [0u8; 12];
        self.rng
            .fill(&mut nonce_bytes)
            .map_err(|_| CryptoError::RandomFailed)?;

        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut in_out = plaintext.to_vec();

        self.key
            .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| CryptoError::EncryptionFailed("Seal failed".to_string()))?;

        Ok(EncryptedData {
            ciphertext: in_out,
            nonce: nonce_bytes,
        })
    }

    /// Decrypt ciphertext.
    pub fn decrypt(&self, encrypted: &EncryptedData) -> Result<Vec<u8>> {
        let nonce = Nonce::assume_unique_for_key(encrypted.nonce);
        let mut in_out = encrypted.ciphertext.clone();

        let plaintext = self
            .key
            .open_in_place(nonce, Aad::empty(), &mut in_out)
            .map_err(|_| CryptoError::DecryptionFailed("Open failed".to_string()))?;

        Ok(plaintext.to_vec())
    }
}

/// Key derivation function wrapper.
pub struct KeyDerivation;

impl KeyDerivation {
    /// Derive key using HKDF-SHA256.
    pub fn hkdf_sha256(salt: &[u8], ikm: &[u8], info: &[u8], output_len: usize) -> Result<Vec<u8>> {
        use ring::hkdf;

        let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, salt);
        let prk = salt.extract(ikm);

        let mut output = vec![0u8; output_len];
        prk.expand(&[info], HkdfOutputLen(output_len))
            .map_err(|_| CryptoError::InvalidKey("HKDF expand failed".to_string()))?
            .fill(&mut output)
            .map_err(|_| CryptoError::InvalidKey("HKDF fill failed".to_string()))?;

        Ok(output)
    }
}

struct HkdfOutputLen(usize);

impl ring::hkdf::KeyType for HkdfOutputLen {
    fn len(&self) -> usize {
        self.0
    }
}

/// Secure random number generator.
pub struct SecureRng {
    rng: SystemRandom,
}

impl SecureRng {
    /// Create new secure RNG.
    pub fn new() -> Self {
        Self {
            rng: SystemRandom::new(),
        }
    }

    /// Generate random bytes.
    pub fn random_bytes(&self, len: usize) -> Result<Vec<u8>> {
        let mut bytes = vec![0u8; len];
        self.rng
            .fill(&mut bytes)
            .map_err(|_| CryptoError::RandomFailed)?;
        Ok(bytes)
    }

    /// Generate random u64.
    pub fn random_u64(&self) -> Result<u64> {
        let bytes = self.random_bytes(8)?;
        Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
    }

    /// Generate random u32.
    pub fn random_u32(&self) -> Result<u32> {
        let bytes = self.random_bytes(4)?;
        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    /// Generate random UUID v4.
    pub fn random_uuid(&self) -> Result<String> {
        let mut bytes = [0u8; 16];
        self.rng
            .fill(&mut bytes)
            .map_err(|_| CryptoError::RandomFailed)?;

        // Set version (4) and variant (RFC4122)
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;

        Ok(format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5],
            bytes[6], bytes[7],
            bytes[8], bytes[9],
            bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
        ))
    }
}

impl Default for SecureRng {
    fn default() -> Self {
        Self::new()
    }
}

/// Constant-time comparison.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

/// Secure memory zeroing.
pub fn secure_zero(data: &mut [u8]) {
    for byte in data.iter_mut() {
        unsafe {
            std::ptr::write_volatile(byte, 0);
        }
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes_encrypt_decrypt() {
        let key = AesGcmEncryptor::generate_key().unwrap();
        let encryptor = AesGcmEncryptor::new(&key).unwrap();

        let plaintext = b"Hello, World!";
        let encrypted = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_hkdf() {
        let salt = b"salt";
        let ikm = b"input key material";
        let info = b"context";

        let key = KeyDerivation::hkdf_sha256(salt, ikm, info, 32).unwrap();
        assert_eq!(key.len(), 32);
    }

    #[test]
    fn test_secure_rng() {
        let rng = SecureRng::new();

        let bytes = rng.random_bytes(32).unwrap();
        assert_eq!(bytes.len(), 32);

        let uuid = rng.random_uuid().unwrap();
        assert_eq!(uuid.len(), 36);
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hell"));
    }

    #[test]
    fn test_secure_zero() {
        let mut data = vec![1, 2, 3, 4, 5];
        secure_zero(&mut data);
        assert!(data.iter().all(|&b| b == 0));
    }
}
