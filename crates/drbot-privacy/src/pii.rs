//! PII detection and redaction.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// PII types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiType {
    Email,
    Phone,
    Ssn,
    CreditCard,
    IpAddress,
    Address,
    Name,
    DateOfBirth,
    Password,
    ApiKey,
    Custom,
}

impl PiiType {
    /// Get default redaction placeholder.
    pub fn redaction_placeholder(&self) -> &str {
        match self {
            PiiType::Email => "[EMAIL]",
            PiiType::Phone => "[PHONE]",
            PiiType::Ssn => "[SSN]",
            PiiType::CreditCard => "[CREDIT_CARD]",
            PiiType::IpAddress => "[IP_ADDRESS]",
            PiiType::Address => "[ADDRESS]",
            PiiType::Name => "[NAME]",
            PiiType::DateOfBirth => "[DOB]",
            PiiType::Password => "[PASSWORD]",
            PiiType::ApiKey => "[API_KEY]",
            PiiType::Custom => "[REDACTED]",
        }
    }
}

/// Redaction configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionConfig {
    /// PII types to detect.
    pub detect_types: Vec<PiiType>,
    /// Custom patterns.
    pub custom_patterns: HashMap<String, String>,
    /// Redaction style.
    pub style: RedactionStyle,
    /// Preserve format (e.g., keep @ in emails).
    pub preserve_format: bool,
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            detect_types: vec![
                PiiType::Email,
                PiiType::Phone,
                PiiType::Ssn,
                PiiType::CreditCard,
                PiiType::IpAddress,
                PiiType::ApiKey,
            ],
            custom_patterns: HashMap::new(),
            style: RedactionStyle::Placeholder,
            preserve_format: false,
        }
    }
}

/// Redaction style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionStyle {
    /// Replace with placeholder.
    Placeholder,
    /// Replace with asterisks.
    Asterisks,
    /// Replace with hash.
    Hash,
    /// Remove completely.
    Remove,
}

/// Detection result.
#[derive(Debug, Clone)]
pub struct DetectionResult {
    /// Detected PII types.
    pub detected_types: Vec<PiiType>,
    /// Matches with positions.
    pub matches: Vec<PiiMatch>,
}

/// A PII match.
#[derive(Debug, Clone)]
pub struct PiiMatch {
    /// PII type.
    pub pii_type: PiiType,
    /// Start position.
    pub start: usize,
    /// End position.
    pub end: usize,
    /// Matched text.
    pub text: String,
}

/// Redacted text result.
#[derive(Debug, Clone)]
pub struct RedactedText {
    /// Original text.
    pub original: String,
    /// Redacted text.
    pub redacted: String,
    /// Number of redactions.
    pub redaction_count: usize,
    /// Redacted items.
    pub redactions: Vec<Redaction>,
}

/// A redaction.
#[derive(Debug, Clone)]
pub struct Redaction {
    /// PII type.
    pub pii_type: PiiType,
    /// Original text.
    pub original: String,
    /// Replacement text.
    pub replacement: String,
}

/// PII detector.
pub struct PiiDetector {
    config: RedactionConfig,
    patterns: HashMap<PiiType, Regex>,
}

impl PiiDetector {
    /// Create a new PII detector.
    pub fn new(config: RedactionConfig) -> Self {
        let mut patterns = HashMap::new();

        // Email pattern
        if config.detect_types.contains(&PiiType::Email) {
            patterns.insert(
                PiiType::Email,
                Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap(),
            );
        }

        // Phone pattern (various formats)
        if config.detect_types.contains(&PiiType::Phone) {
            patterns.insert(
                PiiType::Phone,
                Regex::new(r"(\+?1?[-.\s]?)?\(?[0-9]{3}\)?[-.\s]?[0-9]{3}[-.\s]?[0-9]{4}").unwrap(),
            );
        }

        // SSN pattern
        if config.detect_types.contains(&PiiType::Ssn) {
            patterns.insert(
                PiiType::Ssn,
                Regex::new(r"\b\d{3}[-\s]?\d{2}[-\s]?\d{4}\b").unwrap(),
            );
        }

        // Credit card pattern
        if config.detect_types.contains(&PiiType::CreditCard) {
            patterns.insert(
                PiiType::CreditCard,
                Regex::new(r"\b(?:\d{4}[-\s]?){3}\d{4}\b").unwrap(),
            );
        }

        // IP address pattern
        if config.detect_types.contains(&PiiType::IpAddress) {
            patterns.insert(
                PiiType::IpAddress,
                Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap(),
            );
        }

        // API key patterns (various formats)
        if config.detect_types.contains(&PiiType::ApiKey) {
            patterns.insert(
                PiiType::ApiKey,
                Regex::new(
                    r#"(?i)(api[_-]?key|token|secret|password)\s*[:=]\s*['"]?[\w-]{20,}['"]?"#,
                )
                .unwrap(),
            );
        }

        Self { config, patterns }
    }

    /// Detect PII in text.
    pub fn detect(&self, text: &str) -> DetectionResult {
        let mut detected_types = Vec::new();
        let mut matches = Vec::new();

        for (pii_type, pattern) in &self.patterns {
            for m in pattern.find_iter(text) {
                if !detected_types.contains(pii_type) {
                    detected_types.push(*pii_type);
                }
                matches.push(PiiMatch {
                    pii_type: *pii_type,
                    start: m.start(),
                    end: m.end(),
                    text: m.as_str().to_string(),
                });
            }
        }

        DetectionResult {
            detected_types,
            matches,
        }
    }

    /// Redact PII from text.
    pub fn redact(&self, text: &str) -> RedactedText {
        let mut redacted = text.to_string();
        let mut redactions = Vec::new();
        let detection = self.detect(text);

        // Sort matches by position (descending) to replace from end
        let mut matches = detection.matches;
        matches.sort_by(|a, b| b.start.cmp(&a.start));

        for m in matches {
            let replacement = match self.config.style {
                RedactionStyle::Placeholder => m.pii_type.redaction_placeholder().to_string(),
                RedactionStyle::Asterisks => "*".repeat(m.text.len()),
                RedactionStyle::Hash => format!("#{:08x}", hash_text(&m.text)),
                RedactionStyle::Remove => String::new(),
            };

            redacted.replace_range(m.start..m.end, &replacement);

            redactions.push(Redaction {
                pii_type: m.pii_type,
                original: m.text,
                replacement: replacement.clone(),
            });
        }

        RedactedText {
            original: text.to_string(),
            redacted,
            redaction_count: redactions.len(),
            redactions,
        }
    }

    /// Check if text contains PII.
    pub fn contains_pii(&self, text: &str) -> bool {
        let result = self.detect(text);
        !result.detected_types.is_empty()
    }
}

fn hash_text(text: &str) -> u32 {
    let mut hash: u32 = 0;
    for byte in text.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u32);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_detection() {
        let detector = PiiDetector::new(RedactionConfig::default());

        let result = detector.detect("Contact me at test@example.com");
        assert!(result.detected_types.contains(&PiiType::Email));
        assert_eq!(result.matches.len(), 1);
    }

    #[test]
    fn test_phone_detection() {
        let detector = PiiDetector::new(RedactionConfig::default());

        let result = detector.detect("Call me at 555-123-4567");
        assert!(result.detected_types.contains(&PiiType::Phone));
    }

    #[test]
    fn test_redaction() {
        let detector = PiiDetector::new(RedactionConfig::default());

        let redacted = detector.redact("Email me at john@example.com or call 555-123-4567");

        assert!(redacted.redacted.contains("[EMAIL]"));
        assert!(redacted.redacted.contains("[PHONE]"));
        assert_eq!(redacted.redaction_count, 2);
    }

    #[test]
    fn test_no_pii() {
        let detector = PiiDetector::new(RedactionConfig::default());

        let result = detector.detect("Hello, how are you?");
        assert!(result.detected_types.is_empty());
    }
}
