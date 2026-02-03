//! Cryptographic operations for sync.

use crate::{Result, SyncError};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

/// Encryption key for sync.
#[derive(Clone)]
pub struct EncryptionKey {
    key: LessSafeKey,
    raw: Vec<u8>,
}

impl EncryptionKey {
    /// Create from raw bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 32 {
            return Err(SyncError::InvalidKey);
        }

        let unbound = UnboundKey::new(&AES_256_GCM, bytes).map_err(|_| SyncError::InvalidKey)?;

        Ok(Self {
            key: LessSafeKey::new(unbound),
            raw: bytes.to_vec(),
        })
    }

    /// Generate a new random key.
    pub fn generate() -> Result<Self> {
        let rng = SystemRandom::new();
        let mut key_bytes = vec![0u8; 32];
        rng.fill(&mut key_bytes)
            .map_err(|_| SyncError::EncryptionFailed("RNG failed".to_string()))?;

        Self::from_bytes(&key_bytes)
    }

    /// Export as bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.raw
    }

    /// Export as hex string.
    pub fn to_hex(&self) -> String {
        hex_encode(&self.raw)
    }

    /// Import from hex string.
    pub fn from_hex(hex: &str) -> Result<Self> {
        let bytes = hex_decode(hex).map_err(|_| SyncError::InvalidKey)?;
        Self::from_bytes(&bytes)
    }

    /// Encrypt data.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Encrypted> {
        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rng.fill(&mut nonce_bytes)
            .map_err(|_| SyncError::EncryptionFailed("RNG failed".to_string()))?;

        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut ciphertext = plaintext.to_vec();
        self.key
            .seal_in_place_append_tag(nonce, Aad::empty(), &mut ciphertext)
            .map_err(|_| SyncError::EncryptionFailed("Seal failed".to_string()))?;

        Ok(Encrypted {
            nonce: nonce_bytes.to_vec(),
            ciphertext,
        })
    }

    /// Decrypt data.
    pub fn decrypt(&self, encrypted: &Encrypted) -> Result<Vec<u8>> {
        if encrypted.nonce.len() != NONCE_LEN {
            return Err(SyncError::DecryptionFailed("Invalid nonce".to_string()));
        }

        let nonce_bytes: [u8; NONCE_LEN] = encrypted.nonce[..]
            .try_into()
            .map_err(|_| SyncError::DecryptionFailed("Invalid nonce".to_string()))?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut ciphertext = encrypted.ciphertext.clone();
        let plaintext = self
            .key
            .open_in_place(nonce, Aad::empty(), &mut ciphertext)
            .map_err(|_| SyncError::DecryptionFailed("Open failed".to_string()))?;

        Ok(plaintext.to_vec())
    }
}

/// Encrypted data container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Encrypted {
    /// Nonce used for encryption.
    pub nonce: Vec<u8>,
    /// Ciphertext with auth tag.
    pub ciphertext: Vec<u8>,
}

impl Encrypted {
    /// Serialize to bytes for transmission.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(self.nonce.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    /// Deserialize from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 4 {
            return Err(SyncError::DecryptionFailed("Too short".to_string()));
        }

        let nonce_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if bytes.len() < 4 + nonce_len {
            return Err(SyncError::DecryptionFailed("Invalid format".to_string()));
        }

        let nonce = bytes[4..4 + nonce_len].to_vec();
        let ciphertext = bytes[4 + nonce_len..].to_vec();

        Ok(Self { nonce, ciphertext })
    }
}

/// Derive a key from a password.
pub fn derive_key(password: &str, salt: &[u8]) -> Result<EncryptionKey> {
    let mut key_bytes = [0u8; 32];

    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(100_000).unwrap(),
        salt,
        password.as_bytes(),
        &mut key_bytes,
    );

    EncryptionKey::from_bytes(&key_bytes)
}

/// Generate a random salt.
pub fn generate_salt() -> Vec<u8> {
    let rng = SystemRandom::new();
    let mut salt = vec![0u8; 16];
    rng.fill(&mut salt).expect("RNG failed");
    salt
}

// Helper functions for hex encoding
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(hex: &str) -> std::result::Result<Vec<u8>, ()> {
    if hex.len() % 2 != 0 {
        return Err(());
    }

    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        let key = EncryptionKey::generate().unwrap();
        assert_eq!(key.as_bytes().len(), 32);
    }

    #[test]
    fn test_encrypt_decrypt() {
        let key = EncryptionKey::generate().unwrap();
        let plaintext = b"Hello, World!";

        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_key_hex_roundtrip() {
        let key = EncryptionKey::generate().unwrap();
        let hex = key.to_hex();
        let restored = EncryptionKey::from_hex(&hex).unwrap();

        assert_eq!(key.as_bytes(), restored.as_bytes());
    }

    #[test]
    fn test_derive_key() {
        let salt = generate_salt();
        let key1 = derive_key("password123", &salt).unwrap();
        let key2 = derive_key("password123", &salt).unwrap();

        assert_eq!(key1.as_bytes(), key2.as_bytes());
    }

    #[test]
    fn test_encrypted_serialization() {
        let key = EncryptionKey::generate().unwrap();
        let encrypted = key.encrypt(b"test data").unwrap();

        let bytes = encrypted.to_bytes();
        let restored = Encrypted::from_bytes(&bytes).unwrap();

        assert_eq!(encrypted.nonce, restored.nonce);
        assert_eq!(encrypted.ciphertext, restored.ciphertext);
    }
}
