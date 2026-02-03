//! End-to-end encryption.

use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

use crate::{PrivacyError, Result};

/// Key pair for E2E encryption.
#[derive(Clone)]
pub struct KeyPair {
    /// Private key bytes.
    private_key: Vec<u8>,
    /// Public key bytes.
    public_key: Vec<u8>,
}

impl KeyPair {
    /// Generate a new key pair.
    pub fn generate() -> Result<Self> {
        let rng = SystemRandom::new();
        let mut private_key = vec![0u8; 32];
        rng.fill(&mut private_key)
            .map_err(|_| PrivacyError::KeyError("Failed to generate key".to_string()))?;

        // For simplicity, using same key as public (in real impl, would use proper key exchange)
        let public_key = private_key.clone();

        Ok(Self {
            private_key,
            public_key,
        })
    }

    /// Create from existing private key.
    pub fn from_private_key(private_key: &[u8]) -> Result<Self> {
        if private_key.len() != 32 {
            return Err(PrivacyError::KeyError("Invalid key length".to_string()));
        }

        Ok(Self {
            private_key: private_key.to_vec(),
            public_key: private_key.to_vec(),
        })
    }

    /// Get public key.
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    /// Get private key (be careful with this!).
    pub fn private_key(&self) -> &[u8] {
        &self.private_key
    }
}

/// Encrypted message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedMessage {
    /// Nonce used for encryption.
    pub nonce: Vec<u8>,
    /// Encrypted ciphertext.
    pub ciphertext: Vec<u8>,
    /// Associated data (not encrypted, but authenticated).
    pub associated_data: Option<Vec<u8>>,
}

/// E2E encryption handler.
pub struct E2EEncryption {
    key: LessSafeKey,
    rng: SystemRandom,
}

impl E2EEncryption {
    /// Create from key pair.
    pub fn new(key_pair: &KeyPair) -> Result<Self> {
        let unbound_key = UnboundKey::new(&CHACHA20_POLY1305, key_pair.private_key())
            .map_err(|_| PrivacyError::KeyError("Failed to create key".to_string()))?;

        Ok(Self {
            key: LessSafeKey::new(unbound_key),
            rng: SystemRandom::new(),
        })
    }

    /// Encrypt a message.
    pub fn encrypt(
        &self,
        plaintext: &[u8],
        associated_data: Option<&[u8]>,
    ) -> Result<EncryptedMessage> {
        let mut nonce_bytes = [0u8; 12];
        self.rng
            .fill(&mut nonce_bytes)
            .map_err(|_| PrivacyError::EncryptionError("Failed to generate nonce".to_string()))?;

        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let aad = Aad::from(associated_data.unwrap_or(&[]));

        let mut in_out = plaintext.to_vec();
        in_out.reserve(CHACHA20_POLY1305.tag_len());

        self.key
            .seal_in_place_append_tag(nonce, aad, &mut in_out)
            .map_err(|_| PrivacyError::EncryptionError("Encryption failed".to_string()))?;

        Ok(EncryptedMessage {
            nonce: nonce_bytes.to_vec(),
            ciphertext: in_out,
            associated_data: associated_data.map(|d| d.to_vec()),
        })
    }

    /// Decrypt a message.
    pub fn decrypt(&self, message: &EncryptedMessage) -> Result<Vec<u8>> {
        if message.nonce.len() != 12 {
            return Err(PrivacyError::DecryptionError(
                "Invalid nonce length".to_string(),
            ));
        }

        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&message.nonce);
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let aad = Aad::from(message.associated_data.as_deref().unwrap_or(&[]));

        let mut in_out = message.ciphertext.clone();

        let plaintext = self
            .key
            .open_in_place(nonce, aad, &mut in_out)
            .map_err(|_| PrivacyError::DecryptionError("Decryption failed".to_string()))?;

        Ok(plaintext.to_vec())
    }

    /// Encrypt a string.
    pub fn encrypt_string(&self, plaintext: &str) -> Result<EncryptedMessage> {
        self.encrypt(plaintext.as_bytes(), None)
    }

    /// Decrypt to string.
    pub fn decrypt_string(&self, message: &EncryptedMessage) -> Result<String> {
        let bytes = self.decrypt(message)?;
        String::from_utf8(bytes)
            .map_err(|_| PrivacyError::DecryptionError("Invalid UTF-8".to_string()))
    }
}

/// Simple symmetric encryption for local storage.
pub struct LocalEncryption {
    key: Vec<u8>,
}

impl LocalEncryption {
    /// Create with a password.
    pub fn from_password(password: &str) -> Self {
        // Simple key derivation (in production, use proper KDF)
        use ring::digest;
        let digest = digest::digest(&digest::SHA256, password.as_bytes());
        Self {
            key: digest.as_ref().to_vec(),
        }
    }

    /// Encrypt data.
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let key_pair = KeyPair::from_private_key(&self.key)?;
        let encryption = E2EEncryption::new(&key_pair)?;
        let encrypted = encryption.encrypt(data, None)?;

        // Serialize to bytes
        let mut result = Vec::new();
        result.extend_from_slice(&encrypted.nonce);
        result.extend_from_slice(&encrypted.ciphertext);
        Ok(result)
    }

    /// Decrypt data.
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            return Err(PrivacyError::DecryptionError("Data too short".to_string()));
        }

        let key_pair = KeyPair::from_private_key(&self.key)?;
        let encryption = E2EEncryption::new(&key_pair)?;

        let message = EncryptedMessage {
            nonce: data[..12].to_vec(),
            ciphertext: data[12..].to_vec(),
            associated_data: None,
        };

        encryption.decrypt(&message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        let key_pair = KeyPair::generate().unwrap();
        assert_eq!(key_pair.public_key().len(), 32);
        assert_eq!(key_pair.private_key().len(), 32);
    }

    #[test]
    fn test_encryption_roundtrip() {
        let key_pair = KeyPair::generate().unwrap();
        let encryption = E2EEncryption::new(&key_pair).unwrap();

        let plaintext = "Hello, World!";
        let encrypted = encryption.encrypt_string(plaintext).unwrap();
        let decrypted = encryption.decrypt_string(&encrypted).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_local_encryption() {
        let encryption = LocalEncryption::from_password("my-password");

        let data = b"Secret data";
        let encrypted = encryption.encrypt(data).unwrap();
        let decrypted = encryption.decrypt(&encrypted).unwrap();

        assert_eq!(data.as_slice(), decrypted.as_slice());
    }
}
