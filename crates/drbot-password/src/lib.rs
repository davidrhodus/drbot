//! Password hashing and validation for drbot.
//!
//! This crate provides:
//! - Secure password hashing with PBKDF2
//! - Password verification
//! - Password strength validation
//! - Salt generation

use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use std::num::NonZeroU32;
use thiserror::Error;

/// Password error types.
#[derive(Error, Debug)]
pub enum PasswordError {
    #[error("Invalid hash format")]
    InvalidFormat,

    #[error("Hash verification failed")]
    VerificationFailed,

    #[error("Password too weak: {0}")]
    TooWeak(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for password operations.
pub type Result<T> = std::result::Result<T, PasswordError>;

/// Password hasher configuration.
#[derive(Debug, Clone)]
pub struct HashConfig {
    /// Number of iterations.
    pub iterations: u32,
    /// Salt length in bytes.
    pub salt_length: usize,
    /// Output length in bytes.
    pub output_length: usize,
}

impl Default for HashConfig {
    fn default() -> Self {
        Self {
            iterations: 100_000,
            salt_length: 16,
            output_length: 32,
        }
    }
}

/// Password hasher.
pub struct Hasher {
    config: HashConfig,
}

impl Hasher {
    /// Create new hasher with default config.
    pub fn new() -> Self {
        Self::with_config(HashConfig::default())
    }

    /// Create hasher with custom config.
    pub fn with_config(config: HashConfig) -> Self {
        Self { config }
    }

    /// Hash a password.
    pub fn hash(&self, password: &str) -> Result<String> {
        let rng = SystemRandom::new();
        let mut salt = vec![0u8; self.config.salt_length];
        rng.fill(&mut salt)
            .map_err(|_| PasswordError::Internal("Failed to generate salt".into()))?;

        let mut hash = vec![0u8; self.config.output_length];

        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(self.config.iterations).unwrap(),
            &salt,
            password.as_bytes(),
            &mut hash,
        );

        // Format: $pbkdf2-sha256$iterations$salt$hash
        Ok(format!(
            "$pbkdf2-sha256${}${}${}",
            self.config.iterations,
            hex_encode(&salt),
            hex_encode(&hash)
        ))
    }

    /// Verify a password against a hash.
    pub fn verify(&self, password: &str, hash_string: &str) -> Result<bool> {
        let parts: Vec<&str> = hash_string.split('$').collect();
        if parts.len() != 5 || parts[1] != "pbkdf2-sha256" {
            return Err(PasswordError::InvalidFormat);
        }

        let iterations: u32 = parts[2].parse().map_err(|_| PasswordError::InvalidFormat)?;
        let salt = hex_decode(parts[3]).map_err(|_| PasswordError::InvalidFormat)?;
        let expected_hash = hex_decode(parts[4]).map_err(|_| PasswordError::InvalidFormat)?;

        let result = pbkdf2::verify(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(iterations).ok_or(PasswordError::InvalidFormat)?,
            &salt,
            password.as_bytes(),
            &expected_hash,
        );

        Ok(result.is_ok())
    }
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Password strength requirements.
#[derive(Debug, Clone)]
pub struct StrengthRequirements {
    /// Minimum length.
    pub min_length: usize,
    /// Require uppercase letter.
    pub require_uppercase: bool,
    /// Require lowercase letter.
    pub require_lowercase: bool,
    /// Require digit.
    pub require_digit: bool,
    /// Require special character.
    pub require_special: bool,
    /// List of common passwords to reject.
    pub common_passwords: Vec<String>,
}

impl Default for StrengthRequirements {
    fn default() -> Self {
        Self {
            min_length: 8,
            require_uppercase: true,
            require_lowercase: true,
            require_digit: true,
            require_special: false,
            common_passwords: vec![
                "password".into(),
                "123456".into(),
                "qwerty".into(),
                "letmein".into(),
                "admin".into(),
            ],
        }
    }
}

/// Password strength level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    /// Very weak password.
    VeryWeak,
    /// Weak password.
    Weak,
    /// Fair password.
    Fair,
    /// Strong password.
    Strong,
    /// Very strong password.
    VeryStrong,
}

/// Password strength validator.
pub struct Validator {
    requirements: StrengthRequirements,
}

impl Validator {
    /// Create validator with default requirements.
    pub fn new() -> Self {
        Self::with_requirements(StrengthRequirements::default())
    }

    /// Create validator with custom requirements.
    pub fn with_requirements(requirements: StrengthRequirements) -> Self {
        Self { requirements }
    }

    /// Validate password against requirements.
    pub fn validate(&self, password: &str) -> Result<()> {
        if password.len() < self.requirements.min_length {
            return Err(PasswordError::TooWeak(format!(
                "minimum length is {}",
                self.requirements.min_length
            )));
        }

        if self.requirements.require_uppercase && !password.chars().any(|c| c.is_uppercase()) {
            return Err(PasswordError::TooWeak(
                "must contain an uppercase letter".into(),
            ));
        }

        if self.requirements.require_lowercase && !password.chars().any(|c| c.is_lowercase()) {
            return Err(PasswordError::TooWeak(
                "must contain a lowercase letter".into(),
            ));
        }

        if self.requirements.require_digit && !password.chars().any(|c| c.is_ascii_digit()) {
            return Err(PasswordError::TooWeak("must contain a digit".into()));
        }

        if self.requirements.require_special && !password.chars().any(|c| !c.is_alphanumeric()) {
            return Err(PasswordError::TooWeak(
                "must contain a special character".into(),
            ));
        }

        let lower = password.to_lowercase();
        if self
            .requirements
            .common_passwords
            .iter()
            .any(|p| lower.contains(p))
        {
            return Err(PasswordError::TooWeak(
                "password contains a common word".into(),
            ));
        }

        Ok(())
    }

    /// Calculate password strength.
    pub fn strength(&self, password: &str) -> Strength {
        let mut score: i32 = 0;

        // Length score
        score += match password.len() {
            0..=4 => 0,
            5..=7 => 1,
            8..=11 => 2,
            12..=15 => 3,
            _ => 4,
        };

        // Character variety
        if password.chars().any(|c| c.is_lowercase()) {
            score += 1;
        }
        if password.chars().any(|c| c.is_uppercase()) {
            score += 1;
        }
        if password.chars().any(|c| c.is_ascii_digit()) {
            score += 1;
        }
        if password.chars().any(|c| !c.is_alphanumeric()) {
            score += 2;
        }

        // Penalty for common patterns
        let lower = password.to_lowercase();
        if ["password", "123456", "qwerty"]
            .iter()
            .any(|p| lower.contains(p))
        {
            score = score.saturating_sub(3);
        }

        match score {
            0..=2 => Strength::VeryWeak,
            3..=4 => Strength::Weak,
            5..=6 => Strength::Fair,
            7..=8 => Strength::Strong,
            _ => Strength::VeryStrong,
        }
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

/// Hash a password with default settings.
pub fn hash(password: &str) -> Result<String> {
    Hasher::new().hash(password)
}

/// Verify a password against a hash.
pub fn verify(password: &str, hash: &str) -> Result<bool> {
    Hasher::new().verify(password, hash)
}

/// Generate a random password.
pub fn generate(length: usize) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*";

    let rng = SystemRandom::new();
    let mut password = vec![0u8; length];
    rng.fill(&mut password).unwrap();

    password
        .iter()
        .map(|&b| CHARS[(b as usize) % CHARS.len()] as char)
        .collect()
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> std::result::Result<Vec<u8>, String> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_verify() {
        let password = "MySecurePassword123!";
        let hash = hash(password).unwrap();

        assert!(verify(password, &hash).unwrap());
        assert!(!verify("wrong", &hash).unwrap());
    }

    #[test]
    fn test_different_passwords_different_hashes() {
        let hash1 = hash("password1").unwrap();
        let hash2 = hash("password1").unwrap();

        // Same password should produce different hashes (different salts)
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_validate_password() {
        let validator = Validator::new();

        assert!(validator.validate("StrongPass1").is_ok());
        assert!(validator.validate("weak").is_err());
        assert!(validator.validate("nouppercase1").is_err());
    }

    #[test]
    fn test_password_strength() {
        let validator = Validator::new();

        assert_eq!(validator.strength("abc"), Strength::VeryWeak);
        assert_eq!(validator.strength("password123"), Strength::VeryWeak); // Contains "password"
        assert!(validator.strength("MyStr0ngP@ss!") >= Strength::Strong);
    }

    #[test]
    fn test_generate_password() {
        let password = generate(16);
        assert_eq!(password.len(), 16);
    }
}
