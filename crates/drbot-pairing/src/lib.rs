//! DM pairing and security system for drbot.
//!
//! This crate provides sender verification and pairing mechanisms to control
//! who can interact with the bot through direct messages.
//!
//! # Features
//!
//! - Multiple pairing modes (Open, ApprovalCode, Allowlist, Hybrid)
//! - Approval code generation and validation
//! - Sender allowlist management
//! - Per-channel pairing configuration
//! - SQLite persistence for pairing data
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_pairing::{PairingConfig, PairingManager, PairingMode, SqlitePairingStore};
//!
//! async fn example() {
//!     let store = SqlitePairingStore::new("pairing.db").unwrap();
//!     let config = PairingConfig {
//!         default_mode: PairingMode::ApprovalCode,
//!         ..Default::default()
//!     };
//!     let manager = PairingManager::new(store, config).await.unwrap();
//!
//!     // Generate an approval code
//!     let code = manager.generate_approval_code("user123", "dm").await.unwrap();
//!     println!("Approval code: {}", code.code);
//!
//!     // Verify the code
//!     let valid = manager.verify_approval_code("user123", "dm", &code.code).await.unwrap();
//!     assert!(valid);
//! }
//! ```

mod allowlist;
mod approval;
mod channel;
mod manager;
mod mode;
mod store;

pub use allowlist::{Allowlist, AllowlistEntry, AllowlistManager};
pub use approval::{ApprovalCode, ApprovalCodeGenerator, PendingApproval};
pub use channel::{ChannelPairingConfig, ChannelPairingState};
pub use manager::{PairingDecision, PairingManager, PairingResult};
pub use mode::PairingMode;
pub use store::{PairingStore, SqlitePairingStore};

use serde::{Deserialize, Serialize};

/// Result type for pairing operations.
pub type Result<T> = std::result::Result<T, PairingError>;

/// Pairing system errors.
#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    #[error("Invalid approval code")]
    InvalidCode,
    #[error("Approval code expired")]
    CodeExpired,
    #[error("Sender not allowed: {0}")]
    SenderNotAllowed(String),
    #[error("Pairing already exists for sender: {0}")]
    AlreadyPaired(String),
    #[error("Channel not configured: {0}")]
    ChannelNotConfigured(String),
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

/// Global pairing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingConfig {
    /// Default pairing mode.
    #[serde(default)]
    pub default_mode: PairingMode,
    /// Approval code validity duration (seconds).
    #[serde(default = "default_code_validity")]
    pub code_validity_secs: u64,
    /// Approval code length.
    #[serde(default = "default_code_length")]
    pub code_length: usize,
    /// Maximum pending approvals per sender.
    #[serde(default = "default_max_pending")]
    pub max_pending_per_sender: usize,
    /// Enable rate limiting.
    #[serde(default = "default_rate_limiting")]
    pub rate_limiting_enabled: bool,
    /// Rate limit window (seconds).
    #[serde(default = "default_rate_limit_window")]
    pub rate_limit_window_secs: u64,
    /// Maximum requests per window.
    #[serde(default = "default_rate_limit_max")]
    pub rate_limit_max_requests: usize,
    /// Audit logging enabled.
    #[serde(default = "default_audit_logging")]
    pub audit_logging: bool,
}

fn default_code_validity() -> u64 {
    300 // 5 minutes
}

fn default_code_length() -> usize {
    6
}

fn default_max_pending() -> usize {
    3
}

fn default_rate_limiting() -> bool {
    true
}

fn default_rate_limit_window() -> u64 {
    60 // 1 minute
}

fn default_rate_limit_max() -> usize {
    10
}

fn default_audit_logging() -> bool {
    true
}

impl Default for PairingConfig {
    fn default() -> Self {
        Self {
            default_mode: PairingMode::Open,
            code_validity_secs: default_code_validity(),
            code_length: default_code_length(),
            max_pending_per_sender: default_max_pending(),
            rate_limiting_enabled: default_rate_limiting(),
            rate_limit_window_secs: default_rate_limit_window(),
            rate_limit_max_requests: default_rate_limit_max(),
            audit_logging: default_audit_logging(),
        }
    }
}

/// Paired sender information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedSender {
    /// Sender identifier.
    pub sender_id: String,
    /// Channel the sender is paired on.
    pub channel: String,
    /// When the pairing was created.
    pub paired_at: chrono::DateTime<chrono::Utc>,
    /// Who approved the pairing (if applicable).
    pub approved_by: Option<String>,
    /// Additional metadata.
    pub metadata: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pairing_config_default() {
        let config = PairingConfig::default();
        assert_eq!(config.default_mode, PairingMode::Open);
        assert_eq!(config.code_validity_secs, 300);
        assert_eq!(config.code_length, 6);
    }
}
