//! Privacy and security layer for drbot.
//!
//! Provides privacy features including PII detection, E2E encryption, and compliance.
//!
//! # Features
//!
//! - PII detection and redaction
//! - End-to-end encryption
//! - Local-only mode support
//! - Audit logging for compliance

mod audit;
mod encryption;
mod local_mode;
mod pii;

pub use audit::{AuditConfig, AuditEvent, AuditLevel, AuditLogger};
pub use encryption::{E2EEncryption, EncryptedMessage, KeyPair};
pub use local_mode::{LocalMode, LocalModeConfig};
pub use pii::{PiiDetector, PiiType, RedactedText, RedactionConfig};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Privacy result type.
pub type Result<T> = std::result::Result<T, PrivacyError>;

/// Privacy errors.
#[derive(Debug, thiserror::Error)]
pub enum PrivacyError {
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    #[error("Decryption error: {0}")]
    DecryptionError(String),
    #[error("Key error: {0}")]
    KeyError(String),
    #[error("Audit error: {0}")]
    AuditError(String),
    #[error("Policy violation: {0}")]
    PolicyViolation(String),
}

/// Privacy policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyPolicy {
    /// Policy ID.
    pub id: Uuid,
    /// Policy name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Rules.
    pub rules: Vec<PrivacyRule>,
    /// Is active.
    pub active: bool,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl PrivacyPolicy {
    /// Create a new privacy policy.
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            rules: Vec::new(),
            active: true,
            created_at: Utc::now(),
        }
    }

    /// Add a rule.
    pub fn with_rule(mut self, rule: PrivacyRule) -> Self {
        self.rules.push(rule);
        self
    }
}

/// Privacy rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyRule {
    /// Rule ID.
    pub id: Uuid,
    /// Rule name.
    pub name: String,
    /// Rule type.
    pub rule_type: RuleType,
    /// Action to take.
    pub action: RuleAction,
    /// Priority.
    pub priority: i32,
}

/// Rule types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleType {
    /// Block PII types.
    BlockPii { pii_types: Vec<PiiType> },
    /// Block keywords.
    BlockKeywords { keywords: Vec<String> },
    /// Require encryption.
    RequireEncryption,
    /// Block external sending.
    LocalOnly,
    /// Block providers.
    BlockProviders { providers: Vec<String> },
}

/// Actions for rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    /// Block the action.
    Block,
    /// Redact and allow.
    Redact,
    /// Warn but allow.
    Warn,
    /// Log only.
    Log,
}

/// Privacy settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacySettings {
    /// Enable PII detection.
    pub pii_detection: bool,
    /// Auto-redact PII.
    pub auto_redact: bool,
    /// Enable E2E encryption.
    pub e2e_encryption: bool,
    /// Local-only mode.
    pub local_only: bool,
    /// Audit logging.
    pub audit_enabled: bool,
    /// Retention days.
    pub retention_days: u32,
    /// Blocked providers.
    pub blocked_providers: Vec<String>,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            pii_detection: true,
            auto_redact: false,
            e2e_encryption: false,
            local_only: false,
            audit_enabled: true,
            retention_days: 90,
            blocked_providers: Vec::new(),
        }
    }
}

/// Privacy manager.
pub struct PrivacyManager {
    settings: PrivacySettings,
    policies: Vec<PrivacyPolicy>,
    pii_detector: pii::PiiDetector,
    audit_logger: audit::AuditLogger,
}

impl PrivacyManager {
    /// Create a new privacy manager.
    pub fn new(settings: PrivacySettings) -> Self {
        Self {
            settings,
            policies: Vec::new(),
            pii_detector: pii::PiiDetector::new(pii::RedactionConfig::default()),
            audit_logger: audit::AuditLogger::new(audit::AuditConfig::default()),
        }
    }

    /// Check message against privacy policies.
    pub async fn check_message(&self, message: &str) -> Result<PrivacyCheckResult> {
        let mut result = PrivacyCheckResult {
            allowed: true,
            action: None,
            pii_detected: Vec::new(),
            redacted_message: None,
            violations: Vec::new(),
        };

        // Check PII
        if self.settings.pii_detection {
            let pii_result = self.pii_detector.detect(message);
            result.pii_detected = pii_result.detected_types.clone();

            if self.settings.auto_redact && !pii_result.detected_types.is_empty() {
                let redacted = self.pii_detector.redact(message);
                result.redacted_message = Some(redacted.redacted);
            }
        }

        // Check policies
        for policy in &self.policies {
            if !policy.active {
                continue;
            }

            for rule in &policy.rules {
                let violated = self.check_rule(&rule.rule_type, message, &result.pii_detected);

                if violated {
                    result.violations.push(rule.name.clone());

                    match rule.action {
                        RuleAction::Block => {
                            result.allowed = false;
                            result.action = Some(rule.action);
                        }
                        RuleAction::Redact => {
                            result.action = Some(rule.action);
                        }
                        RuleAction::Warn => {
                            result.action = Some(rule.action);
                        }
                        RuleAction::Log => {
                            // Just log
                        }
                    }
                }
            }
        }

        // Log the check
        if self.settings.audit_enabled {
            self.audit_logger
                .log(audit::AuditEvent::new(
                    audit::AuditLevel::Info,
                    "privacy_check",
                    format!(
                        "Checked message: allowed={}, pii={}",
                        result.allowed,
                        result.pii_detected.len()
                    ),
                ))
                .await;
        }

        Ok(result)
    }

    fn check_rule(&self, rule_type: &RuleType, message: &str, pii_types: &[PiiType]) -> bool {
        match rule_type {
            RuleType::BlockPii { pii_types: blocked } => {
                pii_types.iter().any(|t| blocked.contains(t))
            }
            RuleType::BlockKeywords { keywords } => {
                let lower = message.to_lowercase();
                keywords.iter().any(|k| lower.contains(&k.to_lowercase()))
            }
            RuleType::RequireEncryption => !self.settings.e2e_encryption,
            RuleType::LocalOnly => !self.settings.local_only,
            RuleType::BlockProviders { .. } => false,
        }
    }

    /// Add a policy.
    pub fn add_policy(&mut self, policy: PrivacyPolicy) {
        self.policies.push(policy);
    }
}

/// Privacy check result.
#[derive(Debug, Clone)]
pub struct PrivacyCheckResult {
    /// Is the action allowed.
    pub allowed: bool,
    /// Action to take.
    pub action: Option<RuleAction>,
    /// PII types detected.
    pub pii_detected: Vec<PiiType>,
    /// Redacted message.
    pub redacted_message: Option<String>,
    /// Violated rules.
    pub violations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_privacy_manager() {
        let settings = PrivacySettings::default();
        let manager = PrivacyManager::new(settings);

        let result = manager.check_message("Hello world").await.unwrap();
        assert!(result.allowed);
    }
}
