//! JWT token handling for drbot.
//!
//! This crate provides:
//! - JWT creation and signing
//! - JWT verification and decoding
//! - Claims management
//! - Multiple algorithms support

use chrono::{DateTime, Duration, Utc};
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// JWT error types.
#[derive(Error, Debug)]
pub enum JwtError {
    #[error("Invalid token format")]
    InvalidFormat,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Token expired")]
    Expired,

    #[error("Token not yet valid")]
    NotYetValid,

    #[error("Missing claim: {0}")]
    MissingClaim(String),

    #[error("Encoding error: {0}")]
    EncodingError(String),

    #[error("Decoding error: {0}")]
    DecodingError(String),
}

/// Result type for JWT operations.
pub type Result<T> = std::result::Result<T, JwtError>;

/// JWT algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// HMAC-SHA256.
    HS256,
    /// HMAC-SHA384.
    HS384,
    /// HMAC-SHA512.
    HS512,
}

impl Algorithm {
    /// Get algorithm name.
    pub fn name(&self) -> &'static str {
        match self {
            Algorithm::HS256 => "HS256",
            Algorithm::HS384 => "HS384",
            Algorithm::HS512 => "HS512",
        }
    }
}

/// JWT header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    /// Algorithm.
    pub alg: String,
    /// Type (always "JWT").
    pub typ: String,
}

impl Header {
    /// Create new header.
    pub fn new(algorithm: Algorithm) -> Self {
        Self {
            alg: algorithm.name().to_string(),
            typ: "JWT".to_string(),
        }
    }
}

impl Default for Header {
    fn default() -> Self {
        Self::new(Algorithm::HS256)
    }
}

/// Standard JWT claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Issuer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// Subject.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    /// Audience.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
    /// Expiration time (Unix timestamp).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    /// Not before (Unix timestamp).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbf: Option<i64>,
    /// Issued at (Unix timestamp).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iat: Option<i64>,
    /// JWT ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    /// Custom claims.
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

impl Claims {
    /// Create new empty claims.
    pub fn new() -> Self {
        Self {
            iss: None,
            sub: None,
            aud: None,
            exp: None,
            nbf: None,
            iat: Some(Utc::now().timestamp()),
            jti: None,
            custom: HashMap::new(),
        }
    }

    /// Set issuer.
    pub fn issuer(mut self, issuer: &str) -> Self {
        self.iss = Some(issuer.to_string());
        self
    }

    /// Set subject.
    pub fn subject(mut self, subject: &str) -> Self {
        self.sub = Some(subject.to_string());
        self
    }

    /// Set audience.
    pub fn audience(mut self, audience: &str) -> Self {
        self.aud = Some(audience.to_string());
        self
    }

    /// Set expiration.
    pub fn expires_at(mut self, exp: DateTime<Utc>) -> Self {
        self.exp = Some(exp.timestamp());
        self
    }

    /// Set expiration from duration.
    pub fn expires_in(mut self, duration: Duration) -> Self {
        self.exp = Some((Utc::now() + duration).timestamp());
        self
    }

    /// Set not before.
    pub fn not_before(mut self, nbf: DateTime<Utc>) -> Self {
        self.nbf = Some(nbf.timestamp());
        self
    }

    /// Set JWT ID.
    pub fn jti(mut self, id: &str) -> Self {
        self.jti = Some(id.to_string());
        self
    }

    /// Add custom claim.
    pub fn claim<V: Serialize>(mut self, key: &str, value: V) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.custom.insert(key.to_string(), v);
        }
        self
    }

    /// Get custom claim.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.custom
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Check if token is expired.
    pub fn is_expired(&self) -> bool {
        self.exp
            .map(|exp| Utc::now().timestamp() > exp)
            .unwrap_or(false)
    }

    /// Check if token is valid (not expired and not before valid).
    pub fn is_valid(&self) -> bool {
        let now = Utc::now().timestamp();

        if let Some(exp) = self.exp {
            if now > exp {
                return false;
            }
        }

        if let Some(nbf) = self.nbf {
            if now < nbf {
                return false;
            }
        }

        true
    }
}

impl Default for Claims {
    fn default() -> Self {
        Self::new()
    }
}

/// JWT encoder/decoder.
pub struct Jwt {
    secret: Vec<u8>,
    algorithm: Algorithm,
}

impl Jwt {
    /// Create new JWT with secret.
    pub fn new(secret: &[u8], algorithm: Algorithm) -> Self {
        Self {
            secret: secret.to_vec(),
            algorithm,
        }
    }

    /// Create with HS256 algorithm.
    pub fn hs256(secret: &[u8]) -> Self {
        Self::new(secret, Algorithm::HS256)
    }

    /// Encode claims to JWT token.
    pub fn encode(&self, claims: &Claims) -> Result<String> {
        let header = Header::new(self.algorithm);
        let header_json =
            serde_json::to_string(&header).map_err(|e| JwtError::EncodingError(e.to_string()))?;
        let claims_json =
            serde_json::to_string(&claims).map_err(|e| JwtError::EncodingError(e.to_string()))?;

        let header_b64 = base64_url_encode(header_json.as_bytes());
        let claims_b64 = base64_url_encode(claims_json.as_bytes());

        let message = format!("{}.{}", header_b64, claims_b64);
        let signature = self.sign(message.as_bytes());
        let signature_b64 = base64_url_encode(&signature);

        Ok(format!("{}.{}", message, signature_b64))
    }

    /// Decode and verify JWT token.
    pub fn decode(&self, token: &str) -> Result<Claims> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(JwtError::InvalidFormat);
        }

        let header_b64 = parts[0];
        let claims_b64 = parts[1];
        let signature_b64 = parts[2];

        // Verify signature
        let message = format!("{}.{}", header_b64, claims_b64);
        let signature = base64_url_decode(signature_b64).map_err(|_| JwtError::InvalidSignature)?;

        if !self.verify(message.as_bytes(), &signature) {
            return Err(JwtError::InvalidSignature);
        }

        // Decode claims
        let claims_json =
            base64_url_decode(claims_b64).map_err(|e| JwtError::DecodingError(e.to_string()))?;
        let claims: Claims = serde_json::from_slice(&claims_json)
            .map_err(|e| JwtError::DecodingError(e.to_string()))?;

        // Validate claims
        if let Some(exp) = claims.exp {
            if Utc::now().timestamp() > exp {
                return Err(JwtError::Expired);
            }
        }

        if let Some(nbf) = claims.nbf {
            if Utc::now().timestamp() < nbf {
                return Err(JwtError::NotYetValid);
            }
        }

        Ok(claims)
    }

    /// Decode without verifying signature.
    pub fn decode_unsafe(&self, token: &str) -> Result<Claims> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(JwtError::InvalidFormat);
        }

        let claims_json =
            base64_url_decode(parts[1]).map_err(|e| JwtError::DecodingError(e.to_string()))?;
        let claims: Claims = serde_json::from_slice(&claims_json)
            .map_err(|e| JwtError::DecodingError(e.to_string()))?;

        Ok(claims)
    }

    fn sign(&self, message: &[u8]) -> Vec<u8> {
        let algorithm = match self.algorithm {
            Algorithm::HS256 => hmac::HMAC_SHA256,
            Algorithm::HS384 => hmac::HMAC_SHA384,
            Algorithm::HS512 => hmac::HMAC_SHA512,
        };

        let key = hmac::Key::new(algorithm, &self.secret);
        let tag = hmac::sign(&key, message);
        tag.as_ref().to_vec()
    }

    fn verify(&self, message: &[u8], signature: &[u8]) -> bool {
        let algorithm = match self.algorithm {
            Algorithm::HS256 => hmac::HMAC_SHA256,
            Algorithm::HS384 => hmac::HMAC_SHA384,
            Algorithm::HS512 => hmac::HMAC_SHA512,
        };

        let key = hmac::Key::new(algorithm, &self.secret);
        hmac::verify(&key, message, signature).is_ok()
    }
}

/// Generate a random JWT secret.
pub fn generate_secret(length: usize) -> Vec<u8> {
    let rng = SystemRandom::new();
    let mut secret = vec![0u8; length];
    rng.fill(&mut secret).unwrap();
    secret
}

fn base64_url_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let b0 = data[i] as usize;
        result.push(ALPHABET[b0 >> 2] as char);

        if i + 1 < data.len() {
            let b1 = data[i + 1] as usize;
            result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

            if i + 2 < data.len() {
                let b2 = data[i + 2] as usize;
                result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
                result.push(ALPHABET[b2 & 0x3f] as char);
            } else {
                result.push(ALPHABET[(b1 & 0x0f) << 2] as char);
            }
        } else {
            result.push(ALPHABET[(b0 & 0x03) << 4] as char);
        }

        i += 3;
    }

    result
}

fn base64_url_decode(data: &str) -> std::result::Result<Vec<u8>, String> {
    const DECODE: [i8; 128] = [
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 62,
        -1, -1, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, -1, -1, -1, -1, -1, -1, -1, 0, 1, 2, 3, 4,
        5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, -1, -1, -1,
        -1, 63, -1, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
        46, 47, 48, 49, 50, 51, -1, -1, -1, -1, -1,
    ];

    let bytes: Vec<u8> = data.bytes().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        let b0 = DECODE.get(bytes[i] as usize).copied().unwrap_or(-1);
        if b0 < 0 {
            return Err("invalid character".to_string());
        }

        if i + 1 >= bytes.len() {
            break;
        }
        let b1 = DECODE.get(bytes[i + 1] as usize).copied().unwrap_or(-1);
        if b1 < 0 {
            return Err("invalid character".to_string());
        }

        result.push(((b0 as u8) << 2) | ((b1 as u8) >> 4));

        if i + 2 >= bytes.len() {
            break;
        }
        let b2 = DECODE.get(bytes[i + 2] as usize).copied().unwrap_or(-1);
        if b2 < 0 {
            return Err("invalid character".to_string());
        }

        result.push(((b1 as u8) << 4) | ((b2 as u8) >> 2));

        if i + 3 >= bytes.len() {
            break;
        }
        let b3 = DECODE.get(bytes[i + 3] as usize).copied().unwrap_or(-1);
        if b3 < 0 {
            return Err("invalid character".to_string());
        }

        result.push(((b2 as u8) << 6) | (b3 as u8));

        i += 4;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode() {
        let jwt = Jwt::hs256(b"secret");

        let claims = Claims::new()
            .subject("user123")
            .issuer("drbot")
            .expires_in(Duration::hours(1));

        let token = jwt.encode(&claims).unwrap();
        let decoded = jwt.decode(&token).unwrap();

        assert_eq!(decoded.sub, Some("user123".to_string()));
        assert_eq!(decoded.iss, Some("drbot".to_string()));
    }

    #[test]
    fn test_invalid_signature() {
        let jwt1 = Jwt::hs256(b"secret1");
        let jwt2 = Jwt::hs256(b"secret2");

        let claims = Claims::new().subject("user");
        let token = jwt1.encode(&claims).unwrap();

        assert!(jwt2.decode(&token).is_err());
    }

    #[test]
    fn test_expired_token() {
        let jwt = Jwt::hs256(b"secret");

        let claims = Claims::new().expires_in(Duration::seconds(-10)); // Already expired

        let token = jwt.encode(&claims).unwrap();
        let result = jwt.decode(&token);

        assert!(matches!(result, Err(JwtError::Expired)));
    }

    #[test]
    fn test_custom_claims() {
        let jwt = Jwt::hs256(b"secret");

        let claims = Claims::new()
            .claim("role", "admin")
            .claim("permissions", vec!["read", "write"]);

        let token = jwt.encode(&claims).unwrap();
        let decoded = jwt.decode(&token).unwrap();

        assert_eq!(decoded.get::<String>("role"), Some("admin".to_string()));
    }

    #[test]
    fn test_generate_secret() {
        let secret = generate_secret(32);
        assert_eq!(secret.len(), 32);
    }
}
