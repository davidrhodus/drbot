//! PII detection, data retention, GDPR/SOC2 compliance.
//!
//! This crate provides:
//! - PII detection and redaction
//! - Data retention policies
//! - Compliance reporting
//! - Audit trail

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Compliance errors.
#[derive(Debug, Error)]
pub enum ComplianceError {
    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    #[error("Retention period not met: {0}")]
    RetentionNotMet(String),

    #[error("PII detected: {0}")]
    PiiDetected(String),

    #[error("Compliance check failed: {0}")]
    CheckFailed(String),
}

/// Result type for compliance operations.
pub type Result<T> = std::result::Result<T, ComplianceError>;

/// Types of PII.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PiiType {
    Email,
    Phone,
    SSN,
    CreditCard,
    Name,
    Address,
    DateOfBirth,
    IpAddress,
    DriversLicense,
    Passport,
    BankAccount,
    Custom,
}

/// PII detection result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiDetection {
    /// Detection identifier.
    pub id: String,
    /// PII type.
    pub pii_type: PiiType,
    /// Original value (for internal use).
    pub value: String,
    /// Start position.
    pub start: usize,
    /// End position.
    pub end: usize,
    /// Confidence score.
    pub confidence: f64,
}

/// Redaction result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactedText {
    /// Original text.
    pub original: String,
    /// Redacted text.
    pub redacted: String,
    /// Detections.
    pub detections: Vec<PiiDetection>,
    /// Redaction method.
    pub method: RedactionMethod,
}

/// Redaction methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RedactionMethod {
    /// Replace with asterisks.
    Mask,
    /// Replace with placeholder.
    Placeholder,
    /// Remove entirely.
    Remove,
    /// Replace with hash.
    Hash,
}

/// Data retention policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Policy identifier.
    pub id: String,
    /// Policy name.
    pub name: String,
    /// Data types covered.
    pub data_types: Vec<String>,
    /// Retention period in days.
    pub retention_days: u32,
    /// Action on expiry.
    pub expiry_action: ExpiryAction,
    /// Is active.
    pub active: bool,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// Actions on data expiry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpiryAction {
    Delete,
    Archive,
    Anonymize,
    Notify,
}

/// Compliance framework.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComplianceFramework {
    GDPR,
    CCPA,
    HIPAA,
    SOC2,
    PCI,
    ISO27001,
}

/// Compliance check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceCheck {
    /// Check identifier.
    pub id: String,
    /// Framework.
    pub framework: ComplianceFramework,
    /// Check name.
    pub check_name: String,
    /// Status.
    pub status: CheckStatus,
    /// Details.
    pub details: String,
    /// Recommendations.
    pub recommendations: Vec<String>,
    /// Checked at.
    pub checked_at: DateTime<Utc>,
}

/// Check status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    Pass,
    Fail,
    Warning,
    NotApplicable,
}

/// Data subject request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSubjectRequest {
    /// Request identifier.
    pub id: String,
    /// Subject identifier.
    pub subject_id: String,
    /// Request type.
    pub request_type: DsrType,
    /// Status.
    pub status: DsrStatus,
    /// Submitted at.
    pub submitted_at: DateTime<Utc>,
    /// Due date.
    pub due_date: DateTime<Utc>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
    /// Notes.
    pub notes: Vec<String>,
}

/// DSR types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DsrType {
    Access,
    Deletion,
    Rectification,
    Portability,
    Objection,
}

/// DSR status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DsrStatus {
    Pending,
    InProgress,
    Completed,
    Rejected,
}

/// PII detector provider.
#[async_trait]
pub trait PiiDetector: Send + Sync {
    /// Detect PII in text.
    async fn detect(&self, text: &str) -> Result<Vec<PiiDetection>>;
}

/// The compliance engine.
pub struct ComplianceEngine {
    /// PII detector.
    detector: Arc<dyn PiiDetector>,
    /// Retention policies.
    policies: Arc<RwLock<HashMap<String, RetentionPolicy>>>,
    /// Compliance checks.
    checks: Arc<RwLock<Vec<ComplianceCheck>>>,
    /// Data subject requests.
    dsr_requests: Arc<RwLock<Vec<DataSubjectRequest>>>,
    /// Default redaction method.
    default_redaction: RedactionMethod,
}

impl ComplianceEngine {
    /// Create a new compliance engine.
    pub fn new(detector: Arc<dyn PiiDetector>) -> Self {
        Self {
            detector,
            policies: Arc::new(RwLock::new(HashMap::new())),
            checks: Arc::new(RwLock::new(Vec::new())),
            dsr_requests: Arc::new(RwLock::new(Vec::new())),
            default_redaction: RedactionMethod::Mask,
        }
    }

    /// Set default redaction method.
    pub fn with_redaction_method(mut self, method: RedactionMethod) -> Self {
        self.default_redaction = method;
        self
    }

    /// Detect PII in text.
    pub async fn detect_pii(&self, text: &str) -> Result<Vec<PiiDetection>> {
        self.detector.detect(text).await
    }

    /// Redact PII from text.
    pub async fn redact(&self, text: &str) -> Result<RedactedText> {
        let detections = self.detect_pii(text).await?;

        if detections.is_empty() {
            return Ok(RedactedText {
                original: text.to_string(),
                redacted: text.to_string(),
                detections: vec![],
                method: self.default_redaction,
            });
        }

        // Sort by position (reverse for replacement)
        let mut sorted = detections.clone();
        sorted.sort_by(|a, b| b.start.cmp(&a.start));

        let mut redacted = text.to_string();
        for detection in &sorted {
            let replacement = match self.default_redaction {
                RedactionMethod::Mask => "*".repeat(detection.end - detection.start),
                RedactionMethod::Placeholder => format!("[{}]", detection.pii_type.as_str()),
                RedactionMethod::Remove => String::new(),
                RedactionMethod::Hash => format!("[HASH:{}]", &detection.id[..8]),
            };
            redacted.replace_range(detection.start..detection.end, &replacement);
        }

        Ok(RedactedText {
            original: text.to_string(),
            redacted,
            detections,
            method: self.default_redaction,
        })
    }

    /// Create a retention policy.
    pub async fn create_policy(
        &self,
        name: &str,
        data_types: Vec<String>,
        retention_days: u32,
        expiry_action: ExpiryAction,
    ) -> String {
        let policy = RetentionPolicy {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            data_types,
            retention_days,
            expiry_action,
            active: true,
            created_at: Utc::now(),
        };

        let id = policy.id.clone();
        let mut policies = self.policies.write().await;
        policies.insert(id.clone(), policy);

        id
    }

    /// Check if data should be retained.
    pub async fn check_retention(
        &self,
        data_type: &str,
        created_at: DateTime<Utc>,
    ) -> Option<(RetentionPolicy, bool)> {
        let policies = self.policies.read().await;

        for policy in policies.values() {
            if policy.active && policy.data_types.contains(&data_type.to_string()) {
                let expiry = created_at + Duration::days(policy.retention_days as i64);
                let should_delete = Utc::now() > expiry;
                return Some((policy.clone(), should_delete));
            }
        }

        None
    }

    /// Run compliance check.
    pub async fn run_check(
        &self,
        framework: ComplianceFramework,
        check_name: &str,
        pass: bool,
        details: &str,
        recommendations: Vec<String>,
    ) -> String {
        let check = ComplianceCheck {
            id: Uuid::new_v4().to_string(),
            framework,
            check_name: check_name.to_string(),
            status: if pass {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            details: details.to_string(),
            recommendations,
            checked_at: Utc::now(),
        };

        let id = check.id.clone();
        let mut checks = self.checks.write().await;
        checks.push(check);

        id
    }

    /// Create a data subject request.
    pub async fn create_dsr(&self, subject_id: &str, request_type: DsrType) -> String {
        let request = DataSubjectRequest {
            id: Uuid::new_v4().to_string(),
            subject_id: subject_id.to_string(),
            request_type,
            status: DsrStatus::Pending,
            submitted_at: Utc::now(),
            due_date: Utc::now() + Duration::days(30), // GDPR requires 30 days
            completed_at: None,
            notes: Vec::new(),
        };

        let id = request.id.clone();
        let mut requests = self.dsr_requests.write().await;
        requests.push(request);

        id
    }

    /// Update DSR status.
    pub async fn update_dsr(
        &self,
        dsr_id: &str,
        status: DsrStatus,
        note: Option<String>,
    ) -> Result<()> {
        let mut requests = self.dsr_requests.write().await;

        let request = requests
            .iter_mut()
            .find(|r| r.id == dsr_id)
            .ok_or_else(|| ComplianceError::CheckFailed("DSR not found".to_string()))?;

        request.status = status;
        if status == DsrStatus::Completed {
            request.completed_at = Some(Utc::now());
        }
        if let Some(n) = note {
            request.notes.push(n);
        }

        Ok(())
    }

    /// Get pending DSRs.
    pub async fn get_pending_dsrs(&self) -> Vec<DataSubjectRequest> {
        let requests = self.dsr_requests.read().await;
        requests
            .iter()
            .filter(|r| r.status == DsrStatus::Pending || r.status == DsrStatus::InProgress)
            .cloned()
            .collect()
    }

    /// Get overdue DSRs.
    pub async fn get_overdue_dsrs(&self) -> Vec<DataSubjectRequest> {
        let now = Utc::now();
        let requests = self.dsr_requests.read().await;
        requests
            .iter()
            .filter(|r| r.status != DsrStatus::Completed && r.status != DsrStatus::Rejected)
            .filter(|r| r.due_date < now)
            .cloned()
            .collect()
    }

    /// Generate compliance report.
    pub async fn generate_report(&self, framework: ComplianceFramework) -> ComplianceReport {
        let checks = self.checks.read().await;
        let framework_checks: Vec<_> = checks
            .iter()
            .filter(|c| c.framework == framework)
            .cloned()
            .collect();

        let total = framework_checks.len();
        let passed = framework_checks
            .iter()
            .filter(|c| c.status == CheckStatus::Pass)
            .count();
        let failed = framework_checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .count();
        let warnings = framework_checks
            .iter()
            .filter(|c| c.status == CheckStatus::Warning)
            .count();

        ComplianceReport {
            framework,
            total_checks: total,
            passed,
            failed,
            warnings,
            compliance_score: if total > 0 {
                (passed as f64 / total as f64) * 100.0
            } else {
                0.0
            },
            checks: framework_checks,
            generated_at: Utc::now(),
        }
    }

    /// Get retention policies.
    pub async fn get_policies(&self) -> Vec<RetentionPolicy> {
        let policies = self.policies.read().await;
        policies.values().cloned().collect()
    }
}

/// Compliance report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReport {
    /// Framework.
    pub framework: ComplianceFramework,
    /// Total checks.
    pub total_checks: usize,
    /// Passed.
    pub passed: usize,
    /// Failed.
    pub failed: usize,
    /// Warnings.
    pub warnings: usize,
    /// Compliance score.
    pub compliance_score: f64,
    /// Individual checks.
    pub checks: Vec<ComplianceCheck>,
    /// Generated at.
    pub generated_at: DateTime<Utc>,
}

impl PiiType {
    /// Get string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            PiiType::Email => "EMAIL",
            PiiType::Phone => "PHONE",
            PiiType::SSN => "SSN",
            PiiType::CreditCard => "CREDIT_CARD",
            PiiType::Name => "NAME",
            PiiType::Address => "ADDRESS",
            PiiType::DateOfBirth => "DOB",
            PiiType::IpAddress => "IP",
            PiiType::DriversLicense => "DL",
            PiiType::Passport => "PASSPORT",
            PiiType::BankAccount => "BANK",
            PiiType::Custom => "CUSTOM",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockDetector;

    #[async_trait]
    impl PiiDetector for MockDetector {
        async fn detect(&self, text: &str) -> Result<Vec<PiiDetection>> {
            let mut detections = Vec::new();

            // Simple email detection
            if let Some(start) = text.find('@') {
                let email_start = text[..start]
                    .rfind(char::is_whitespace)
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let email_end = text[start..]
                    .find(char::is_whitespace)
                    .map(|i| start + i)
                    .unwrap_or(text.len());

                detections.push(PiiDetection {
                    id: Uuid::new_v4().to_string(),
                    pii_type: PiiType::Email,
                    value: text[email_start..email_end].to_string(),
                    start: email_start,
                    end: email_end,
                    confidence: 0.95,
                });
            }

            Ok(detections)
        }
    }

    #[tokio::test]
    async fn test_detect_pii() {
        let detector = Arc::new(MockDetector);
        let engine = ComplianceEngine::new(detector);

        let detections = engine
            .detect_pii("Contact me at test@example.com")
            .await
            .unwrap();
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].pii_type, PiiType::Email);
    }

    #[tokio::test]
    async fn test_redact() {
        let detector = Arc::new(MockDetector);
        let engine = ComplianceEngine::new(detector);

        let result = engine.redact("Contact test@example.com now").await.unwrap();
        assert!(!result.redacted.contains("test@example.com"));
    }

    #[tokio::test]
    async fn test_retention_policy() {
        let detector = Arc::new(MockDetector);
        let engine = ComplianceEngine::new(detector);

        engine
            .create_policy(
                "Messages",
                vec!["message".to_string()],
                90,
                ExpiryAction::Delete,
            )
            .await;

        let old_date = Utc::now() - Duration::days(100);
        let result = engine.check_retention("message", old_date).await;

        assert!(result.is_some());
        assert!(result.unwrap().1); // Should be deleted
    }

    #[tokio::test]
    async fn test_dsr() {
        let detector = Arc::new(MockDetector);
        let engine = ComplianceEngine::new(detector);

        let id = engine.create_dsr("user123", DsrType::Access).await;
        engine
            .update_dsr(&id, DsrStatus::InProgress, Some("Processing".to_string()))
            .await
            .unwrap();

        let pending = engine.get_pending_dsrs().await;
        assert_eq!(pending.len(), 1);
    }

    #[tokio::test]
    async fn test_compliance_report() {
        let detector = Arc::new(MockDetector);
        let engine = ComplianceEngine::new(detector);

        engine
            .run_check(
                ComplianceFramework::GDPR,
                "Encryption",
                true,
                "Data is encrypted",
                vec![],
            )
            .await;
        engine
            .run_check(
                ComplianceFramework::GDPR,
                "Access Control",
                false,
                "Needs improvement",
                vec!["Implement MFA".to_string()],
            )
            .await;

        let report = engine.generate_report(ComplianceFramework::GDPR).await;
        assert_eq!(report.total_checks, 2);
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 1);
    }
}
