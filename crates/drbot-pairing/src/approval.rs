//! Approval code generation and validation.

use crate::{PairingError, Result};
use chrono::{DateTime, Utc};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An approval code for pairing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalCode {
    /// The code itself.
    pub code: String,
    /// When the code was created.
    pub created_at: DateTime<Utc>,
    /// When the code expires.
    pub expires_at: DateTime<Utc>,
}

impl ApprovalCode {
    /// Create a new approval code.
    pub fn new(code: String, validity_secs: u64) -> Self {
        let now = Utc::now();
        Self {
            code,
            created_at: now,
            expires_at: now + chrono::Duration::seconds(validity_secs as i64),
        }
    }

    /// Check if the code has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Get remaining validity in seconds.
    pub fn remaining_secs(&self) -> i64 {
        (self.expires_at - Utc::now()).num_seconds().max(0)
    }
}

/// A pending approval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    /// Unique ID.
    pub id: Uuid,
    /// Sender ID requesting approval.
    pub sender_id: String,
    /// Channel where approval was requested.
    pub channel: String,
    /// The approval code.
    pub code: ApprovalCode,
    /// Number of verification attempts.
    pub attempts: u32,
    /// Maximum allowed attempts.
    pub max_attempts: u32,
    /// Whether the approval has been consumed.
    pub consumed: bool,
}

impl PendingApproval {
    /// Create a new pending approval.
    pub fn new(sender_id: &str, channel: &str, code: ApprovalCode) -> Self {
        Self {
            id: Uuid::new_v4(),
            sender_id: sender_id.to_string(),
            channel: channel.to_string(),
            code,
            attempts: 0,
            max_attempts: 3,
            consumed: false,
        }
    }

    /// Check if more attempts are allowed.
    pub fn can_attempt(&self) -> bool {
        !self.consumed && self.attempts < self.max_attempts && !self.code.is_expired()
    }

    /// Record an attempt.
    pub fn record_attempt(&mut self) {
        self.attempts += 1;
    }

    /// Mark as consumed (successfully verified).
    pub fn consume(&mut self) {
        self.consumed = true;
    }
}

/// Generator for approval codes.
pub struct ApprovalCodeGenerator {
    code_length: usize,
    validity_secs: u64,
    rng: SystemRandom,
}

impl ApprovalCodeGenerator {
    /// Create a new generator.
    pub fn new(code_length: usize, validity_secs: u64) -> Self {
        Self {
            code_length,
            validity_secs,
            rng: SystemRandom::new(),
        }
    }

    /// Generate a new approval code.
    pub fn generate(&self) -> Result<ApprovalCode> {
        let code = self.generate_code_string()?;
        Ok(ApprovalCode::new(code, self.validity_secs))
    }

    /// Generate a numeric code string.
    fn generate_code_string(&self) -> Result<String> {
        let mut bytes = vec![0u8; self.code_length];
        self.rng
            .fill(&mut bytes)
            .map_err(|_| PairingError::ConfigError("Failed to generate random bytes".into()))?;

        // Convert to digits 0-9
        let code: String = bytes
            .iter()
            .map(|b| char::from_digit((b % 10) as u32, 10).unwrap())
            .collect();

        Ok(code)
    }

    /// Generate an alphanumeric code string.
    pub fn generate_alphanumeric(&self) -> Result<ApprovalCode> {
        const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // Excluding confusing chars

        let mut bytes = vec![0u8; self.code_length];
        self.rng
            .fill(&mut bytes)
            .map_err(|_| PairingError::ConfigError("Failed to generate random bytes".into()))?;

        let code: String = bytes
            .iter()
            .map(|b| CHARSET[(*b as usize) % CHARSET.len()] as char)
            .collect();

        Ok(ApprovalCode::new(code, self.validity_secs))
    }
}

impl Default for ApprovalCodeGenerator {
    fn default() -> Self {
        Self::new(6, 300)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_code() {
        let code = ApprovalCode::new("123456".to_string(), 300);
        assert!(!code.is_expired());
        assert!(code.remaining_secs() > 0);
    }

    #[test]
    fn test_pending_approval() {
        let code = ApprovalCode::new("123456".to_string(), 300);
        let mut pending = PendingApproval::new("user1", "telegram", code);

        assert!(pending.can_attempt());
        pending.record_attempt();
        assert_eq!(pending.attempts, 1);
        assert!(pending.can_attempt());

        pending.consume();
        assert!(!pending.can_attempt());
    }

    #[test]
    fn test_code_generator() {
        let gen = ApprovalCodeGenerator::new(6, 300);

        let code = gen.generate().unwrap();
        assert_eq!(code.code.len(), 6);
        assert!(code.code.chars().all(|c| c.is_ascii_digit()));

        let alpha_code = gen.generate_alphanumeric().unwrap();
        assert_eq!(alpha_code.code.len(), 6);
    }
}
