//! One-time password generation for drbot.
//!
//! This crate provides:
//! - TOTP (Time-based OTP)
//! - HOTP (HMAC-based OTP)
//! - QR code generation for authenticator apps
//! - Verification with time drift tolerance

use chrono::Utc;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use thiserror::Error;

/// OTP error types.
#[derive(Error, Debug)]
pub enum OtpError {
    #[error("Invalid secret")]
    InvalidSecret,

    #[error("Invalid code")]
    InvalidCode,

    #[error("Code expired")]
    Expired,

    #[error("Generation error: {0}")]
    GenerationError(String),
}

/// Result type for OTP operations.
pub type Result<T> = std::result::Result<T, OtpError>;

/// OTP algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// SHA1 (most compatible).
    SHA1,
    /// SHA256.
    SHA256,
    /// SHA512.
    SHA512,
}

/// TOTP configuration.
#[derive(Debug, Clone)]
pub struct TotpConfig {
    /// Secret key.
    pub secret: Vec<u8>,
    /// Number of digits.
    pub digits: u32,
    /// Time step in seconds.
    pub step: u64,
    /// Algorithm.
    pub algorithm: Algorithm,
    /// Issuer name.
    pub issuer: Option<String>,
    /// Account name.
    pub account: Option<String>,
}

impl TotpConfig {
    /// Create new TOTP config with secret.
    pub fn new(secret: Vec<u8>) -> Self {
        Self {
            secret,
            digits: 6,
            step: 30,
            algorithm: Algorithm::SHA1,
            issuer: None,
            account: None,
        }
    }

    /// Set number of digits.
    pub fn digits(mut self, digits: u32) -> Self {
        self.digits = digits;
        self
    }

    /// Set time step.
    pub fn step(mut self, step: u64) -> Self {
        self.step = step;
        self
    }

    /// Set algorithm.
    pub fn algorithm(mut self, algorithm: Algorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Set issuer.
    pub fn issuer(mut self, issuer: &str) -> Self {
        self.issuer = Some(issuer.to_string());
        self
    }

    /// Set account name.
    pub fn account(mut self, account: &str) -> Self {
        self.account = Some(account.to_string());
        self
    }
}

/// TOTP generator.
pub struct Totp {
    config: TotpConfig,
}

impl Totp {
    /// Create new TOTP generator.
    pub fn new(config: TotpConfig) -> Self {
        Self { config }
    }

    /// Create with secret.
    pub fn with_secret(secret: &[u8]) -> Self {
        Self::new(TotpConfig::new(secret.to_vec()))
    }

    /// Generate current TOTP code.
    pub fn generate(&self) -> String {
        let time = Utc::now().timestamp() as u64;
        self.generate_at(time)
    }

    /// Generate TOTP code at specific time.
    pub fn generate_at(&self, time: u64) -> String {
        let counter = time / self.config.step;
        self.generate_hotp(counter)
    }

    /// Verify TOTP code with tolerance.
    pub fn verify(&self, code: &str, tolerance: u64) -> bool {
        let time = Utc::now().timestamp() as u64;
        self.verify_at(code, time, tolerance)
    }

    /// Verify TOTP code at specific time.
    pub fn verify_at(&self, code: &str, time: u64, tolerance: u64) -> bool {
        let counter = time / self.config.step;

        for i in 0..=tolerance {
            if counter >= i {
                let expected = self.generate_hotp(counter - i);
                if constant_time_eq(code.as_bytes(), expected.as_bytes()) {
                    return true;
                }
            }

            if i > 0 {
                let expected = self.generate_hotp(counter + i);
                if constant_time_eq(code.as_bytes(), expected.as_bytes()) {
                    return true;
                }
            }
        }

        false
    }

    /// Generate HOTP code.
    fn generate_hotp(&self, counter: u64) -> String {
        let counter_bytes = counter.to_be_bytes();

        let algorithm = match self.config.algorithm {
            Algorithm::SHA1 => hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
            Algorithm::SHA256 => hmac::HMAC_SHA256,
            Algorithm::SHA512 => hmac::HMAC_SHA512,
        };

        let key = hmac::Key::new(algorithm, &self.config.secret);
        let tag = hmac::sign(&key, &counter_bytes);
        let hash = tag.as_ref();

        // Dynamic truncation
        let offset = (hash[hash.len() - 1] & 0x0f) as usize;
        let binary = ((hash[offset] & 0x7f) as u32) << 24
            | (hash[offset + 1] as u32) << 16
            | (hash[offset + 2] as u32) << 8
            | (hash[offset + 3] as u32);

        let otp = binary % 10u32.pow(self.config.digits);
        format!("{:0width$}", otp, width = self.config.digits as usize)
    }

    /// Get provisioning URI for authenticator apps.
    pub fn provisioning_uri(&self) -> String {
        let secret_b32 = base32_encode(&self.config.secret);

        let account = self.config.account.as_deref().unwrap_or("user");
        let issuer = self.config.issuer.as_deref().unwrap_or("drbot");

        let label = format!("{}:{}", issuer, account);
        let encoded_label = url_encode(&label);

        format!(
            "otpauth://totp/{}?secret={}&issuer={}&algorithm={}&digits={}&period={}",
            encoded_label,
            secret_b32,
            url_encode(issuer),
            match self.config.algorithm {
                Algorithm::SHA1 => "SHA1",
                Algorithm::SHA256 => "SHA256",
                Algorithm::SHA512 => "SHA512",
            },
            self.config.digits,
            self.config.step
        )
    }

    /// Get remaining seconds until code changes.
    pub fn remaining_seconds(&self) -> u64 {
        let time = Utc::now().timestamp() as u64;
        self.config.step - (time % self.config.step)
    }
}

/// HOTP generator.
pub struct Hotp {
    secret: Vec<u8>,
    digits: u32,
    algorithm: Algorithm,
}

impl Hotp {
    /// Create new HOTP generator.
    pub fn new(secret: &[u8]) -> Self {
        Self {
            secret: secret.to_vec(),
            digits: 6,
            algorithm: Algorithm::SHA1,
        }
    }

    /// Set number of digits.
    pub fn digits(mut self, digits: u32) -> Self {
        self.digits = digits;
        self
    }

    /// Set algorithm.
    pub fn algorithm(mut self, algorithm: Algorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Generate HOTP code for counter.
    pub fn generate(&self, counter: u64) -> String {
        let counter_bytes = counter.to_be_bytes();

        let algorithm = match self.algorithm {
            Algorithm::SHA1 => hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
            Algorithm::SHA256 => hmac::HMAC_SHA256,
            Algorithm::SHA512 => hmac::HMAC_SHA512,
        };

        let key = hmac::Key::new(algorithm, &self.secret);
        let tag = hmac::sign(&key, &counter_bytes);
        let hash = tag.as_ref();

        let offset = (hash[hash.len() - 1] & 0x0f) as usize;
        let binary = ((hash[offset] & 0x7f) as u32) << 24
            | (hash[offset + 1] as u32) << 16
            | (hash[offset + 2] as u32) << 8
            | (hash[offset + 3] as u32);

        let otp = binary % 10u32.pow(self.digits);
        format!("{:0width$}", otp, width = self.digits as usize)
    }

    /// Verify HOTP code.
    pub fn verify(&self, code: &str, counter: u64, look_ahead: u64) -> Option<u64> {
        for i in 0..=look_ahead {
            let expected = self.generate(counter + i);
            if constant_time_eq(code.as_bytes(), expected.as_bytes()) {
                return Some(counter + i + 1); // Return next counter
            }
        }
        None
    }
}

/// Generate a random secret.
pub fn generate_secret(length: usize) -> Vec<u8> {
    let rng = SystemRandom::new();
    let mut secret = vec![0u8; length];
    rng.fill(&mut secret).unwrap();
    secret
}

/// Generate a random secret as base32.
pub fn generate_secret_base32(length: usize) -> String {
    base32_encode(&generate_secret(length))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

fn base32_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    let mut result = String::new();
    let mut buffer = 0u64;
    let mut bits = 0;

    for &byte in data {
        buffer = (buffer << 8) | byte as u64;
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            result.push(ALPHABET[((buffer >> bits) & 0x1f) as usize] as char);
        }
    }

    if bits > 0 {
        buffer <<= 5 - bits;
        result.push(ALPHABET[(buffer & 0x1f) as usize] as char);
    }

    result
}

fn url_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_totp_generate() {
        let secret = b"12345678901234567890";
        let totp = Totp::with_secret(secret);

        let code = totp.generate();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_totp_verify() {
        let secret = b"12345678901234567890";
        let totp = Totp::with_secret(secret);

        let code = totp.generate();
        assert!(totp.verify(&code, 0));
    }

    #[test]
    fn test_totp_verify_with_tolerance() {
        let secret = b"12345678901234567890";
        let totp = Totp::with_secret(secret);

        let time = Utc::now().timestamp() as u64;
        let code = totp.generate_at(time - 30); // Previous period

        assert!(!totp.verify_at(&code, time, 0));
        assert!(totp.verify_at(&code, time, 1));
    }

    #[test]
    fn test_hotp_generate() {
        let secret = b"12345678901234567890";
        let hotp = Hotp::new(secret);

        // RFC 4226 test vectors
        let expected = ["755224", "287082", "359152", "969429", "338314"];

        for (counter, expected_code) in expected.iter().enumerate() {
            let code = hotp.generate(counter as u64);
            assert_eq!(&code, expected_code);
        }
    }

    #[test]
    fn test_hotp_verify() {
        let secret = b"12345678901234567890";
        let hotp = Hotp::new(secret);

        assert!(hotp.verify("755224", 0, 0).is_some());
        assert!(hotp.verify("287082", 0, 5).is_some());
        assert!(hotp.verify("invalid", 0, 5).is_none());
    }

    #[test]
    fn test_provisioning_uri() {
        let config = TotpConfig::new(b"secret".to_vec())
            .issuer("TestApp")
            .account("user@example.com");
        let totp = Totp::new(config);

        let uri = totp.provisioning_uri();
        assert!(uri.starts_with("otpauth://totp/"));
        assert!(uri.contains("secret="));
        assert!(uri.contains("issuer=TestApp"));
    }

    #[test]
    fn test_generate_secret() {
        let secret = generate_secret(20);
        assert_eq!(secret.len(), 20);

        let secret_b32 = generate_secret_base32(20);
        assert!(secret_b32
            .chars()
            .all(|c| "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".contains(c)));
    }
}
