//! Privacy firewall for drbot.
//!
//! AI-aware privacy protection.
//!
//! # Features
//!
//! - PII detection and redaction
//! - Content filtering rules
//! - Data flow monitoring
//! - Privacy policy enforcement
//! - Sensitive data alerts

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Firewall result type.
pub type Result<T> = std::result::Result<T, FirewallError>;

/// Firewall errors.
#[derive(Debug, thiserror::Error)]
pub enum FirewallError {
    #[error("Content blocked: {0}")]
    Blocked(String),
    #[error("Policy violation: {0}")]
    PolicyViolation(String),
    #[error("Invalid rule: {0}")]
    InvalidRule(String),
}

/// Sensitive data types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitiveDataType {
    Email,
    Phone,
    CreditCard,
    Ssn,
    Address,
    Name,
    DateOfBirth,
    Password,
    ApiKey,
    IpAddress,
    Custom,
}

/// Detection result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    /// Detection ID.
    pub id: Uuid,
    /// Data type detected.
    pub data_type: SensitiveDataType,
    /// Start position.
    pub start: usize,
    /// End position.
    pub end: usize,
    /// Original value.
    pub original: String,
    /// Confidence.
    pub confidence: f32,
}

/// Redaction style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionStyle {
    /// Replace with [REDACTED]
    Full,
    /// Replace with type marker [EMAIL]
    TypeMarker,
    /// Partial mask (e.g., j***@example.com)
    Partial,
    /// Hash the value
    Hash,
}

/// Firewall rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallRule {
    /// Rule ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Data types to match.
    pub data_types: Vec<SensitiveDataType>,
    /// Action.
    pub action: RuleAction,
    /// Enabled.
    pub enabled: bool,
    /// Contexts where rule applies.
    pub contexts: Vec<String>,
    /// Priority (lower = higher priority).
    pub priority: i32,
}

impl FirewallRule {
    /// Create a new rule.
    pub fn new(name: &str, data_types: Vec<SensitiveDataType>, action: RuleAction) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            data_types,
            action,
            enabled: true,
            contexts: Vec::new(),
            priority: 100,
        }
    }
}

/// Rule action.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    /// Allow the content.
    Allow,
    /// Block entirely.
    Block,
    /// Redact sensitive data.
    Redact(RedactionStyle),
    /// Log but allow.
    Audit,
    /// Alert and allow.
    Alert,
}

/// Data flow direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowDirection {
    Inbound,
    Outbound,
    Internal,
}

/// Flow event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowEvent {
    /// Event ID.
    pub id: Uuid,
    /// Direction.
    pub direction: FlowDirection,
    /// Source.
    pub source: String,
    /// Destination.
    pub destination: String,
    /// Content type.
    pub content_type: String,
    /// Size (bytes).
    pub size: usize,
    /// Detections.
    pub detections: Vec<Detection>,
    /// Action taken.
    pub action_taken: RuleAction,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Privacy policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyPolicy {
    /// Policy ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Rules.
    pub rules: Vec<FirewallRule>,
    /// Default action.
    pub default_action: RuleAction,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl Default for PrivacyPolicy {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "Default Policy".to_string(),
            description: "Standard privacy protection".to_string(),
            rules: vec![
                FirewallRule::new("Block SSN", vec![SensitiveDataType::Ssn], RuleAction::Block),
                FirewallRule::new(
                    "Redact Credit Cards",
                    vec![SensitiveDataType::CreditCard],
                    RuleAction::Redact(RedactionStyle::Partial),
                ),
                FirewallRule::new(
                    "Redact Passwords",
                    vec![SensitiveDataType::Password],
                    RuleAction::Redact(RedactionStyle::Full),
                ),
            ],
            default_action: RuleAction::Allow,
            created_at: Utc::now(),
        }
    }
}

/// Firewall configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallConfig {
    /// Enable firewall.
    pub enabled: bool,
    /// Log all events.
    pub log_all: bool,
    /// Alert on detections.
    pub alert_on_detection: bool,
    /// Maximum content size to scan.
    pub max_scan_size: usize,
}

impl Default for FirewallConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_all: true,
            alert_on_detection: true,
            max_scan_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Processing result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingResult {
    /// Original content.
    pub original: String,
    /// Processed content.
    pub processed: String,
    /// Detections.
    pub detections: Vec<Detection>,
    /// Was modified.
    pub modified: bool,
    /// Was blocked.
    pub blocked: bool,
    /// Applied rules.
    pub applied_rules: Vec<String>,
}

/// Trait for PII detectors.
#[async_trait]
pub trait PiiDetector: Send + Sync {
    /// Detect sensitive data in content.
    async fn detect(&self, content: &str) -> Vec<Detection>;
}

/// Trait for content redactors.
pub trait Redactor: Send + Sync {
    /// Redact detected content.
    fn redact(&self, content: &str, detections: &[Detection], style: RedactionStyle) -> String;
}

/// Privacy firewall engine.
pub struct PrivacyFirewall<D: PiiDetector, R: Redactor> {
    config: FirewallConfig,
    policy: Arc<RwLock<PrivacyPolicy>>,
    detector: D,
    redactor: R,
    events: Arc<RwLock<Vec<FlowEvent>>>,
}

impl<D: PiiDetector, R: Redactor> PrivacyFirewall<D, R> {
    /// Create a new firewall.
    pub fn new(config: FirewallConfig, policy: PrivacyPolicy, detector: D, redactor: R) -> Self {
        Self {
            config,
            policy: Arc::new(RwLock::new(policy)),
            detector,
            redactor,
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Process content through firewall.
    pub async fn process(
        &self,
        content: &str,
        direction: FlowDirection,
        source: &str,
        destination: &str,
    ) -> Result<ProcessingResult> {
        if !self.config.enabled {
            return Ok(ProcessingResult {
                original: content.to_string(),
                processed: content.to_string(),
                detections: Vec::new(),
                modified: false,
                blocked: false,
                applied_rules: Vec::new(),
            });
        }

        if content.len() > self.config.max_scan_size {
            return Ok(ProcessingResult {
                original: content.to_string(),
                processed: content.to_string(),
                detections: Vec::new(),
                modified: false,
                blocked: false,
                applied_rules: vec!["Size limit exceeded, skipped".to_string()],
            });
        }

        // Detect sensitive data
        let detections = self.detector.detect(content).await;

        // Get policy
        let policy = self.policy.read().await;

        // Apply rules
        let mut processed = content.to_string();
        let mut blocked = false;
        let mut modified = false;
        let mut applied_rules = Vec::new();
        let mut action_taken = policy.default_action.clone();

        for detection in &detections {
            // Find matching rule
            let mut rules: Vec<_> = policy
                .rules
                .iter()
                .filter(|r| r.enabled && r.data_types.contains(&detection.data_type))
                .collect();
            rules.sort_by_key(|r| r.priority);

            if let Some(rule) = rules.first() {
                applied_rules.push(rule.name.clone());
                action_taken = rule.action.clone();

                match &rule.action {
                    RuleAction::Block => {
                        blocked = true;
                    }
                    RuleAction::Redact(style) => {
                        processed = self
                            .redactor
                            .redact(&processed, &[detection.clone()], *style);
                        modified = true;
                    }
                    RuleAction::Allow | RuleAction::Audit | RuleAction::Alert => {}
                }
            }
        }

        // Log event
        if self.config.log_all || !detections.is_empty() {
            let event = FlowEvent {
                id: Uuid::new_v4(),
                direction,
                source: source.to_string(),
                destination: destination.to_string(),
                content_type: "text".to_string(),
                size: content.len(),
                detections: detections.clone(),
                action_taken,
                timestamp: Utc::now(),
            };
            self.events.write().await.push(event);
        }

        if blocked {
            return Err(FirewallError::Blocked(
                "Content contains blocked data types".to_string(),
            ));
        }

        Ok(ProcessingResult {
            original: content.to_string(),
            processed,
            detections,
            modified,
            blocked,
            applied_rules,
        })
    }

    /// Update policy.
    pub async fn set_policy(&self, policy: PrivacyPolicy) {
        *self.policy.write().await = policy;
    }

    /// Add rule to policy.
    pub async fn add_rule(&self, rule: FirewallRule) {
        self.policy.write().await.rules.push(rule);
    }

    /// Get recent events.
    pub async fn get_events(&self, limit: usize) -> Vec<FlowEvent> {
        self.events
            .read()
            .await
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get events by type.
    pub async fn get_events_by_type(&self, data_type: SensitiveDataType) -> Vec<FlowEvent> {
        self.events
            .read()
            .await
            .iter()
            .filter(|e| e.detections.iter().any(|d| d.data_type == data_type))
            .cloned()
            .collect()
    }

    /// Get statistics.
    pub async fn stats(&self) -> FirewallStats {
        let events = self.events.read().await;

        let mut by_type: HashMap<SensitiveDataType, usize> = HashMap::new();
        let mut blocked_count = 0;
        let mut redacted_count = 0;

        for event in events.iter() {
            for detection in &event.detections {
                *by_type.entry(detection.data_type).or_insert(0) += 1;
            }
            match event.action_taken {
                RuleAction::Block => blocked_count += 1,
                RuleAction::Redact(_) => redacted_count += 1,
                _ => {}
            }
        }

        FirewallStats {
            total_events: events.len(),
            total_detections: events.iter().map(|e| e.detections.len()).sum(),
            blocked_count,
            redacted_count,
            by_type,
        }
    }
}

/// Firewall statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallStats {
    pub total_events: usize,
    pub total_detections: usize,
    pub blocked_count: usize,
    pub redacted_count: usize,
    pub by_type: HashMap<SensitiveDataType, usize>,
}

/// Simple regex-based PII detector.
pub struct SimpleDetector;

#[async_trait]
impl PiiDetector for SimpleDetector {
    async fn detect(&self, content: &str) -> Vec<Detection> {
        let mut detections = Vec::new();

        // Email pattern
        for (i, part) in content.split_whitespace().enumerate() {
            if part.contains('@') && part.contains('.') {
                detections.push(Detection {
                    id: Uuid::new_v4(),
                    data_type: SensitiveDataType::Email,
                    start: i,
                    end: i + part.len(),
                    original: part.to_string(),
                    confidence: 0.9,
                });
            }
        }

        // Credit card pattern (simplified: 16 digits)
        let digits: String = content.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 16 {
            detections.push(Detection {
                id: Uuid::new_v4(),
                data_type: SensitiveDataType::CreditCard,
                start: 0,
                end: 16,
                original: digits[..16].to_string(),
                confidence: 0.8,
            });
        }

        // Phone pattern (simplified)
        if content.chars().filter(|c| c.is_ascii_digit()).count() >= 10 {
            let potential_phone: String = content
                .chars()
                .filter(|c| c.is_ascii_digit() || *c == '-' || *c == ' ')
                .take(14)
                .collect();
            if potential_phone
                .chars()
                .filter(|c| c.is_ascii_digit())
                .count()
                >= 10
            {
                detections.push(Detection {
                    id: Uuid::new_v4(),
                    data_type: SensitiveDataType::Phone,
                    start: 0,
                    end: potential_phone.len(),
                    original: potential_phone,
                    confidence: 0.7,
                });
            }
        }

        detections
    }
}

/// Simple redactor.
pub struct SimpleRedactor;

impl Redactor for SimpleRedactor {
    fn redact(&self, content: &str, detections: &[Detection], style: RedactionStyle) -> String {
        let mut result = content.to_string();

        for detection in detections {
            let replacement = match style {
                RedactionStyle::Full => "[REDACTED]".to_string(),
                RedactionStyle::TypeMarker => format!("[{:?}]", detection.data_type).to_uppercase(),
                RedactionStyle::Partial => {
                    let original = &detection.original;
                    if original.len() <= 4 {
                        "****".to_string()
                    } else {
                        let visible = original.len() / 4;
                        format!(
                            "{}{}{}",
                            &original[..visible],
                            "*".repeat(original.len() - visible * 2),
                            &original[original.len() - visible..]
                        )
                    }
                }
                RedactionStyle::Hash => {
                    use std::collections::hash_map::DefaultHasher;
                    use std::hash::{Hash, Hasher};
                    let mut hasher = DefaultHasher::new();
                    detection.original.hash(&mut hasher);
                    format!("[HASH:{:x}]", hasher.finish())
                }
            };

            result = result.replace(&detection.original, &replacement);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_detect_email() {
        let detector = SimpleDetector;
        let detections = detector
            .detect("Contact me at test@example.com please")
            .await;

        assert!(!detections.is_empty());
        assert!(detections
            .iter()
            .any(|d| d.data_type == SensitiveDataType::Email));
    }

    #[tokio::test]
    async fn test_redact_full() {
        let redactor = SimpleRedactor;
        let detections = vec![Detection {
            id: Uuid::new_v4(),
            data_type: SensitiveDataType::Email,
            start: 0,
            end: 16,
            original: "test@example.com".to_string(),
            confidence: 0.9,
        }];

        let result = redactor.redact(
            "Contact test@example.com",
            &detections,
            RedactionStyle::Full,
        );
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("test@example.com"));
    }

    #[tokio::test]
    async fn test_firewall_process() {
        let firewall = PrivacyFirewall::new(
            FirewallConfig::default(),
            PrivacyPolicy::default(),
            SimpleDetector,
            SimpleRedactor,
        );

        let result = firewall
            .process(
                "My email is test@example.com",
                FlowDirection::Outbound,
                "user",
                "ai",
            )
            .await
            .unwrap();

        assert!(!result.detections.is_empty());
    }

    #[tokio::test]
    async fn test_block_rule() {
        let mut policy = PrivacyPolicy::default();
        policy.rules.push(FirewallRule::new(
            "Block Email",
            vec![SensitiveDataType::Email],
            RuleAction::Block,
        ));

        let firewall = PrivacyFirewall::new(
            FirewallConfig::default(),
            policy,
            SimpleDetector,
            SimpleRedactor,
        );

        let result = firewall
            .process(
                "My email is test@example.com",
                FlowDirection::Outbound,
                "user",
                "ai",
            )
            .await;

        assert!(matches!(result, Err(FirewallError::Blocked(_))));
    }

    #[tokio::test]
    async fn test_stats() {
        let firewall = PrivacyFirewall::new(
            FirewallConfig::default(),
            PrivacyPolicy::default(),
            SimpleDetector,
            SimpleRedactor,
        );

        firewall
            .process("test@example.com", FlowDirection::Outbound, "a", "b")
            .await
            .ok();
        firewall
            .process("another@test.com", FlowDirection::Inbound, "c", "d")
            .await
            .ok();

        let stats = firewall.stats().await;
        assert_eq!(stats.total_events, 2);
    }
}
