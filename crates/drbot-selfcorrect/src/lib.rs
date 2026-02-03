//! Self-correction loop for drbot.
//!
//! AI detects and fixes its own mistakes automatically.
//!
//! # Features
//!
//! - Error detection
//! - Automatic correction
//! - Confidence-based re-evaluation
//! - Learning from corrections

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Self-correction result type.
pub type Result<T> = std::result::Result<T, CorrectionError>;

/// Correction errors.
#[derive(Debug, thiserror::Error)]
pub enum CorrectionError {
    #[error("Detection failed: {0}")]
    DetectionFailed(String),
    #[error("Correction failed: {0}")]
    CorrectionFailed(String),
    #[error("Max iterations reached")]
    MaxIterations,
    #[error("No issues detected")]
    NoIssues,
}

/// Detected issue in a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedIssue {
    /// Issue ID.
    pub id: Uuid,
    /// Issue type.
    pub issue_type: IssueType,
    /// Severity (0-1).
    pub severity: f32,
    /// Description.
    pub description: String,
    /// Location in response.
    pub location: Option<TextLocation>,
    /// Suggested fix.
    pub suggested_fix: Option<String>,
    /// Confidence in detection.
    pub confidence: f32,
}

/// Issue types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueType {
    FactualError,
    LogicalInconsistency,
    Contradiction,
    Hallucination,
    IncompleteAnswer,
    FormatError,
    ToneIssue,
    SafetyViolation,
    OutOfScope,
    Ambiguity,
}

/// Text location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextLocation {
    /// Start character.
    pub start: usize,
    /// End character.
    pub end: usize,
    /// Text snippet.
    pub snippet: String,
}

/// Correction result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionResult {
    /// Original response.
    pub original: String,
    /// Corrected response.
    pub corrected: String,
    /// Issues detected.
    pub issues: Vec<DetectedIssue>,
    /// Corrections applied.
    pub corrections: Vec<AppliedCorrection>,
    /// Number of iterations.
    pub iterations: usize,
    /// Final confidence.
    pub confidence: f32,
    /// Processing time in ms.
    pub time_ms: u64,
}

/// Applied correction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedCorrection {
    /// Issue ID.
    pub issue_id: Uuid,
    /// Original text.
    pub original_text: String,
    /// Corrected text.
    pub corrected_text: String,
    /// Correction type.
    pub correction_type: CorrectionType,
    /// Success.
    pub success: bool,
}

/// Correction types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionType {
    Replacement,
    Addition,
    Deletion,
    Rewrite,
    Clarification,
}

/// Self-correction configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionConfig {
    /// Enable self-correction.
    pub enabled: bool,
    /// Maximum correction iterations.
    pub max_iterations: usize,
    /// Minimum confidence to stop.
    pub min_confidence: f32,
    /// Severity threshold for correction.
    pub severity_threshold: f32,
    /// Issue types to check.
    pub check_types: Vec<IssueType>,
    /// Enable learning from corrections.
    pub learn_from_corrections: bool,
}

impl Default for CorrectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_iterations: 3,
            min_confidence: 0.9,
            severity_threshold: 0.5,
            check_types: vec![
                IssueType::FactualError,
                IssueType::LogicalInconsistency,
                IssueType::Contradiction,
                IssueType::Hallucination,
            ],
            learn_from_corrections: true,
        }
    }
}

/// Trait for issue detectors.
#[async_trait]
pub trait IssueDetector: Send + Sync {
    /// Detect issues in a response.
    async fn detect(
        &self,
        query: &str,
        response: &str,
        context: &DetectionContext,
    ) -> Vec<DetectedIssue>;
}

/// Trait for correctors.
#[async_trait]
pub trait Corrector: Send + Sync {
    /// Correct an issue.
    async fn correct(
        &self,
        response: &str,
        issue: &DetectedIssue,
        context: &CorrectionContext,
    ) -> Result<String>;
}

/// Detection context.
#[derive(Debug, Clone, Default)]
pub struct DetectionContext {
    /// Known facts.
    pub known_facts: Vec<String>,
    /// Previous responses.
    pub previous_responses: Vec<String>,
    /// Domain knowledge.
    pub domain: Option<String>,
    /// Custom context.
    pub custom: HashMap<String, serde_json::Value>,
}

/// Correction context.
#[derive(Debug, Clone, Default)]
pub struct CorrectionContext {
    /// Original query.
    pub query: String,
    /// All detected issues.
    pub all_issues: Vec<DetectedIssue>,
    /// Correction history.
    pub history: Vec<AppliedCorrection>,
    /// Available knowledge.
    pub knowledge: Vec<String>,
}

/// Correction history for learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionHistory {
    /// History entries.
    pub entries: Vec<HistoryEntry>,
    /// Pattern statistics.
    pub patterns: HashMap<IssueType, PatternStats>,
}

/// History entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Entry ID.
    pub id: Uuid,
    /// Issue type.
    pub issue_type: IssueType,
    /// Original error pattern.
    pub error_pattern: String,
    /// Correction applied.
    pub correction: String,
    /// Was successful.
    pub successful: bool,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Pattern statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatternStats {
    /// Total occurrences.
    pub count: u64,
    /// Successful corrections.
    pub successful: u64,
    /// Average severity.
    pub avg_severity: f32,
}

/// Self-correction engine.
pub struct SelfCorrectionEngine<D: IssueDetector, C: Corrector> {
    config: CorrectionConfig,
    detector: D,
    corrector: C,
    history: Arc<RwLock<CorrectionHistory>>,
}

impl<D: IssueDetector, C: Corrector> SelfCorrectionEngine<D, C> {
    /// Create a new self-correction engine.
    pub fn new(config: CorrectionConfig, detector: D, corrector: C) -> Self {
        Self {
            config,
            detector,
            corrector,
            history: Arc::new(RwLock::new(CorrectionHistory {
                entries: Vec::new(),
                patterns: HashMap::new(),
            })),
        }
    }

    /// Run self-correction on a response.
    pub async fn correct(&self, query: &str, response: &str) -> Result<CorrectionResult> {
        if !self.config.enabled {
            return Ok(CorrectionResult {
                original: response.to_string(),
                corrected: response.to_string(),
                issues: Vec::new(),
                corrections: Vec::new(),
                iterations: 0,
                confidence: 1.0,
                time_ms: 0,
            });
        }

        let start = std::time::Instant::now();
        let mut current = response.to_string();
        let mut all_issues = Vec::new();
        let mut all_corrections = Vec::new();
        let mut iterations = 0;

        let detection_context = DetectionContext::default();

        loop {
            iterations += 1;

            // Detect issues
            let issues = self
                .detector
                .detect(query, &current, &detection_context)
                .await;

            // Filter by severity and type
            let significant_issues: Vec<_> = issues
                .into_iter()
                .filter(|i| {
                    i.severity >= self.config.severity_threshold
                        && self.config.check_types.contains(&i.issue_type)
                })
                .collect();

            if significant_issues.is_empty() {
                break;
            }

            all_issues.extend(significant_issues.clone());

            // Apply corrections
            let correction_context = CorrectionContext {
                query: query.to_string(),
                all_issues: all_issues.clone(),
                history: all_corrections.clone(),
                knowledge: Vec::new(),
            };

            for issue in significant_issues {
                match self
                    .corrector
                    .correct(&current, &issue, &correction_context)
                    .await
                {
                    Ok(corrected) => {
                        all_corrections.push(AppliedCorrection {
                            issue_id: issue.id,
                            original_text: current.clone(),
                            corrected_text: corrected.clone(),
                            correction_type: CorrectionType::Rewrite,
                            success: true,
                        });
                        current = corrected;

                        // Record for learning
                        if self.config.learn_from_corrections {
                            self.record_correction(&issue, true).await;
                        }
                    }
                    Err(_) => {
                        all_corrections.push(AppliedCorrection {
                            issue_id: issue.id,
                            original_text: String::new(),
                            corrected_text: String::new(),
                            correction_type: CorrectionType::Rewrite,
                            success: false,
                        });

                        if self.config.learn_from_corrections {
                            self.record_correction(&issue, false).await;
                        }
                    }
                }
            }

            if iterations >= self.config.max_iterations {
                break;
            }
        }

        // Calculate final confidence
        let confidence = self.calculate_confidence(&all_issues, &all_corrections);

        Ok(CorrectionResult {
            original: response.to_string(),
            corrected: current,
            issues: all_issues,
            corrections: all_corrections,
            iterations,
            confidence,
            time_ms: start.elapsed().as_millis() as u64,
        })
    }

    fn calculate_confidence(
        &self,
        issues: &[DetectedIssue],
        corrections: &[AppliedCorrection],
    ) -> f32 {
        if issues.is_empty() {
            return 1.0;
        }

        let successful = corrections.iter().filter(|c| c.success).count();
        let total = corrections.len();

        if total == 0 {
            return 0.5;
        }

        let correction_rate = successful as f32 / total as f32;
        let avg_severity: f32 =
            issues.iter().map(|i| i.severity).sum::<f32>() / issues.len() as f32;

        // Higher correction rate and lower severity = higher confidence
        (correction_rate * 0.6 + (1.0 - avg_severity) * 0.4).clamp(0.0, 1.0)
    }

    async fn record_correction(&self, issue: &DetectedIssue, successful: bool) {
        let mut history = self.history.write().await;

        history.entries.push(HistoryEntry {
            id: Uuid::new_v4(),
            issue_type: issue.issue_type,
            error_pattern: issue.description.clone(),
            correction: issue.suggested_fix.clone().unwrap_or_default(),
            successful,
            timestamp: Utc::now(),
        });

        // Update pattern stats
        let stats = history.patterns.entry(issue.issue_type).or_default();
        stats.count += 1;
        if successful {
            stats.successful += 1;
        }
        stats.avg_severity =
            (stats.avg_severity * (stats.count - 1) as f32 + issue.severity) / stats.count as f32;

        // Keep only last 1000 entries
        if history.entries.len() > 1000 {
            history.entries.remove(0);
        }
    }

    /// Get correction statistics.
    pub async fn stats(&self) -> HashMap<IssueType, PatternStats> {
        self.history.read().await.patterns.clone()
    }

    /// Get correction history.
    pub async fn history(&self, limit: usize) -> Vec<HistoryEntry> {
        let history = self.history.read().await;
        history.entries.iter().rev().take(limit).cloned().collect()
    }
}

/// Simple issue detector for testing.
pub struct SimpleDetector;

#[async_trait]
impl IssueDetector for SimpleDetector {
    async fn detect(
        &self,
        _query: &str,
        response: &str,
        _context: &DetectionContext,
    ) -> Vec<DetectedIssue> {
        let mut issues = Vec::new();

        // Check for common issues
        if response.contains("I don't know") || response.contains("I'm not sure") {
            issues.push(DetectedIssue {
                id: Uuid::new_v4(),
                issue_type: IssueType::IncompleteAnswer,
                severity: 0.6,
                description: "Response expresses uncertainty".to_string(),
                location: None,
                suggested_fix: Some("Provide more definitive answer".to_string()),
                confidence: 0.8,
            });
        }

        if response.len() < 10 {
            issues.push(DetectedIssue {
                id: Uuid::new_v4(),
                issue_type: IssueType::IncompleteAnswer,
                severity: 0.7,
                description: "Response is too short".to_string(),
                location: None,
                suggested_fix: Some("Expand the response".to_string()),
                confidence: 0.9,
            });
        }

        issues
    }
}

/// Simple corrector for testing.
pub struct SimpleCorrector;

#[async_trait]
impl Corrector for SimpleCorrector {
    async fn correct(
        &self,
        response: &str,
        issue: &DetectedIssue,
        _context: &CorrectionContext,
    ) -> Result<String> {
        match issue.issue_type {
            IssueType::IncompleteAnswer => Ok(format!(
                "{} [Additional context would be added here]",
                response
            )),
            _ => Ok(response.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_self_correction() {
        let config = CorrectionConfig {
            check_types: vec![IssueType::IncompleteAnswer],
            ..Default::default()
        };
        let engine = SelfCorrectionEngine::new(config, SimpleDetector, SimpleCorrector);

        let result = engine.correct("What is X?", "I don't know").await.unwrap();
        assert!(!result.issues.is_empty());
        assert!(result.corrected.len() > result.original.len());
    }

    #[tokio::test]
    async fn test_no_issues() {
        let engine =
            SelfCorrectionEngine::new(CorrectionConfig::default(), SimpleDetector, SimpleCorrector);

        let result = engine
            .correct(
                "What is 2+2?",
                "The answer is 4, which can be calculated by adding two plus two.",
            )
            .await
            .unwrap();

        assert!(result.issues.is_empty());
        assert_eq!(result.confidence, 1.0);
    }

    #[tokio::test]
    async fn test_disabled() {
        let config = CorrectionConfig {
            enabled: false,
            ..Default::default()
        };

        let engine = SelfCorrectionEngine::new(config, SimpleDetector, SimpleCorrector);

        let result = engine.correct("Q", "Short").await.unwrap();
        assert_eq!(result.iterations, 0);
    }
}
