//! Active learning for drbot.
//!
//! Learn from user feedback to improve over time.
//!
//! # Features
//!
//! - User feedback collection
//! - Learning from corrections
//! - Model improvement suggestions
//! - Performance tracking

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Active learning result type.
pub type Result<T> = std::result::Result<T, LearningError>;

/// Learning errors.
#[derive(Debug, thiserror::Error)]
pub enum LearningError {
    #[error("Feedback not found: {0}")]
    FeedbackNotFound(Uuid),
    #[error("Learning failed: {0}")]
    LearningFailed(String),
    #[error("No improvements available")]
    NoImprovements,
}

/// User feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    /// Feedback ID.
    pub id: Uuid,
    /// Response ID this feedback is for.
    pub response_id: Uuid,
    /// Feedback type.
    pub feedback_type: FeedbackType,
    /// Rating (0-5).
    pub rating: Option<u8>,
    /// Correction provided.
    pub correction: Option<String>,
    /// Specific issue.
    pub issue: Option<String>,
    /// User ID.
    pub user_id: String,
    /// Submitted at.
    pub submitted_at: DateTime<Utc>,
    /// Processed.
    pub processed: bool,
}

impl Feedback {
    /// Create new feedback.
    pub fn new(response_id: Uuid, feedback_type: FeedbackType, user_id: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            response_id,
            feedback_type,
            rating: None,
            correction: None,
            issue: None,
            user_id: user_id.to_string(),
            submitted_at: Utc::now(),
            processed: false,
        }
    }

    /// Set rating.
    pub fn with_rating(mut self, rating: u8) -> Self {
        self.rating = Some(rating.min(5));
        self
    }

    /// Set correction.
    pub fn with_correction(mut self, correction: &str) -> Self {
        self.correction = Some(correction.to_string());
        self
    }

    /// Set issue.
    pub fn with_issue(mut self, issue: &str) -> Self {
        self.issue = Some(issue.to_string());
        self
    }
}

/// Feedback types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackType {
    /// Response was helpful.
    Positive,
    /// Response was not helpful.
    Negative,
    /// User provided a correction.
    Correction,
    /// User flagged content.
    Flag,
    /// User provided a rating.
    Rating,
    /// User provided a suggestion.
    Suggestion,
}

/// Learning pattern identified from feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningPattern {
    /// Pattern ID.
    pub id: Uuid,
    /// Pattern type.
    pub pattern_type: PatternType,
    /// Query pattern.
    pub query_pattern: String,
    /// Issue identified.
    pub issue: String,
    /// Suggested improvement.
    pub improvement: String,
    /// Confidence in the pattern.
    pub confidence: f32,
    /// Occurrence count.
    pub occurrences: u64,
    /// First seen.
    pub first_seen: DateTime<Utc>,
    /// Last seen.
    pub last_seen: DateTime<Utc>,
}

/// Pattern types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    /// Factual error pattern.
    FactualError,
    /// Incomplete response pattern.
    Incomplete,
    /// Formatting issue.
    Formatting,
    /// Tone issue.
    Tone,
    /// Misunderstanding.
    Misunderstanding,
    /// Other.
    Other,
}

/// Improvement suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Improvement {
    /// Improvement ID.
    pub id: Uuid,
    /// Improvement type.
    pub improvement_type: ImprovementType,
    /// Description.
    pub description: String,
    /// Affected patterns.
    pub patterns: Vec<Uuid>,
    /// Expected impact (0-1).
    pub expected_impact: f32,
    /// Priority.
    pub priority: i32,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Implemented.
    pub implemented: bool,
}

/// Improvement types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImprovementType {
    /// Add to knowledge base.
    AddKnowledge,
    /// Update system prompt.
    UpdatePrompt,
    /// Add example.
    AddExample,
    /// Improve formatting.
    ImproveFormatting,
    /// Adjust tone.
    AdjustTone,
    /// Other.
    Other,
}

/// Active learning configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningConfig {
    /// Enable active learning.
    pub enabled: bool,
    /// Minimum feedback for pattern detection.
    pub min_feedback_for_pattern: usize,
    /// Pattern confidence threshold.
    pub pattern_threshold: f32,
    /// Auto-apply low-risk improvements.
    pub auto_apply: bool,
    /// Feedback retention days.
    pub retention_days: u64,
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_feedback_for_pattern: 3,
            pattern_threshold: 0.7,
            auto_apply: false,
            retention_days: 90,
        }
    }
}

/// Trait for pattern detectors.
#[async_trait]
pub trait PatternDetector: Send + Sync {
    /// Detect patterns from feedback.
    async fn detect(&self, feedback: &[Feedback]) -> Result<Vec<LearningPattern>>;
}

/// Trait for improvement generators.
#[async_trait]
pub trait ImprovementGenerator: Send + Sync {
    /// Generate improvements from patterns.
    async fn generate(&self, patterns: &[LearningPattern]) -> Result<Vec<Improvement>>;
}

/// Active learning engine.
pub struct ActiveLearningEngine<D: PatternDetector, G: ImprovementGenerator> {
    config: LearningConfig,
    detector: D,
    generator: G,
    feedback: Arc<RwLock<Vec<Feedback>>>,
    patterns: Arc<RwLock<HashMap<Uuid, LearningPattern>>>,
    improvements: Arc<RwLock<Vec<Improvement>>>,
}

impl<D: PatternDetector, G: ImprovementGenerator> ActiveLearningEngine<D, G> {
    /// Create a new active learning engine.
    pub fn new(config: LearningConfig, detector: D, generator: G) -> Self {
        Self {
            config,
            detector,
            generator,
            feedback: Arc::new(RwLock::new(Vec::new())),
            patterns: Arc::new(RwLock::new(HashMap::new())),
            improvements: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Submit feedback.
    pub async fn submit_feedback(&self, feedback: Feedback) -> Result<Uuid> {
        let id = feedback.id;
        self.feedback.write().await.push(feedback);

        // Trigger pattern detection if enough feedback
        let feedback_count = self.feedback.read().await.len();
        if feedback_count >= self.config.min_feedback_for_pattern {
            self.detect_patterns().await?;
        }

        Ok(id)
    }

    /// Detect patterns from accumulated feedback.
    pub async fn detect_patterns(&self) -> Result<Vec<LearningPattern>> {
        let feedback = self.feedback.read().await;
        let unprocessed: Vec<_> = feedback.iter().filter(|f| !f.processed).cloned().collect();

        if unprocessed.is_empty() {
            return Ok(Vec::new());
        }

        drop(feedback);

        let new_patterns = self.detector.detect(&unprocessed).await?;

        // Merge with existing patterns
        let mut patterns = self.patterns.write().await;
        for mut pattern in new_patterns.clone() {
            if let Some(existing) = patterns
                .values_mut()
                .find(|p| p.query_pattern == pattern.query_pattern)
            {
                existing.occurrences += pattern.occurrences;
                existing.last_seen = pattern.last_seen;
                existing.confidence = (existing.confidence + pattern.confidence) / 2.0;
            } else {
                patterns.insert(pattern.id, pattern);
            }
        }

        // Mark feedback as processed
        let mut feedback = self.feedback.write().await;
        for f in feedback.iter_mut() {
            f.processed = true;
        }

        Ok(new_patterns)
    }

    /// Generate improvement suggestions.
    pub async fn generate_improvements(&self) -> Result<Vec<Improvement>> {
        let patterns = self.patterns.read().await;
        let confident_patterns: Vec<_> = patterns
            .values()
            .filter(|p| p.confidence >= self.config.pattern_threshold)
            .cloned()
            .collect();

        if confident_patterns.is_empty() {
            return Err(LearningError::NoImprovements);
        }

        let improvements = self.generator.generate(&confident_patterns).await?;
        self.improvements.write().await.extend(improvements.clone());

        Ok(improvements)
    }

    /// Get all feedback.
    pub async fn list_feedback(&self) -> Vec<Feedback> {
        self.feedback.read().await.clone()
    }

    /// Get feedback by type.
    pub async fn feedback_by_type(&self, feedback_type: FeedbackType) -> Vec<Feedback> {
        self.feedback
            .read()
            .await
            .iter()
            .filter(|f| f.feedback_type == feedback_type)
            .cloned()
            .collect()
    }

    /// Get all patterns.
    pub async fn list_patterns(&self) -> Vec<LearningPattern> {
        self.patterns.read().await.values().cloned().collect()
    }

    /// Get all improvements.
    pub async fn list_improvements(&self) -> Vec<Improvement> {
        self.improvements.read().await.clone()
    }

    /// Mark improvement as implemented.
    pub async fn implement(&self, improvement_id: Uuid) {
        let mut improvements = self.improvements.write().await;
        if let Some(imp) = improvements.iter_mut().find(|i| i.id == improvement_id) {
            imp.implemented = true;
        }
    }

    /// Get statistics.
    pub async fn stats(&self) -> LearningStats {
        let feedback = self.feedback.read().await;
        let patterns = self.patterns.read().await;
        let improvements = self.improvements.read().await;

        let mut by_type: HashMap<FeedbackType, u64> = HashMap::new();
        let mut positive = 0;
        let mut negative = 0;

        for f in feedback.iter() {
            *by_type.entry(f.feedback_type).or_insert(0) += 1;
            match f.feedback_type {
                FeedbackType::Positive => positive += 1,
                FeedbackType::Negative => negative += 1,
                _ => {}
            }
        }

        let satisfaction_rate = if positive + negative > 0 {
            positive as f32 / (positive + negative) as f32
        } else {
            0.0
        };

        LearningStats {
            total_feedback: feedback.len(),
            feedback_by_type: by_type,
            total_patterns: patterns.len(),
            total_improvements: improvements.len(),
            implemented_improvements: improvements.iter().filter(|i| i.implemented).count(),
            satisfaction_rate,
        }
    }
}

/// Learning statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningStats {
    pub total_feedback: usize,
    pub feedback_by_type: HashMap<FeedbackType, u64>,
    pub total_patterns: usize,
    pub total_improvements: usize,
    pub implemented_improvements: usize,
    pub satisfaction_rate: f32,
}

/// Simple pattern detector for testing.
pub struct SimpleDetector;

#[async_trait]
impl PatternDetector for SimpleDetector {
    async fn detect(&self, feedback: &[Feedback]) -> Result<Vec<LearningPattern>> {
        let mut patterns = Vec::new();

        // Group negative feedback by issue
        let negative: Vec<_> = feedback
            .iter()
            .filter(|f| {
                f.feedback_type == FeedbackType::Negative
                    || f.feedback_type == FeedbackType::Correction
            })
            .collect();

        if negative.len() >= 2 {
            let corrections: Vec<_> = negative
                .iter()
                .filter_map(|f| f.correction.as_ref())
                .collect();

            if !corrections.is_empty() {
                patterns.push(LearningPattern {
                    id: Uuid::new_v4(),
                    pattern_type: PatternType::FactualError,
                    query_pattern: "general".to_string(),
                    issue: "Multiple corrections provided".to_string(),
                    improvement: "Review and update knowledge base".to_string(),
                    confidence: 0.7,
                    occurrences: negative.len() as u64,
                    first_seen: negative
                        .first()
                        .map(|f| f.submitted_at)
                        .unwrap_or_else(Utc::now),
                    last_seen: Utc::now(),
                });
            }
        }

        Ok(patterns)
    }
}

/// Simple improvement generator for testing.
pub struct SimpleGenerator;

#[async_trait]
impl ImprovementGenerator for SimpleGenerator {
    async fn generate(&self, patterns: &[LearningPattern]) -> Result<Vec<Improvement>> {
        let mut improvements = Vec::new();

        for pattern in patterns {
            improvements.push(Improvement {
                id: Uuid::new_v4(),
                improvement_type: match pattern.pattern_type {
                    PatternType::FactualError => ImprovementType::AddKnowledge,
                    PatternType::Incomplete => ImprovementType::AddExample,
                    PatternType::Formatting => ImprovementType::ImproveFormatting,
                    PatternType::Tone => ImprovementType::AdjustTone,
                    _ => ImprovementType::Other,
                },
                description: pattern.improvement.clone(),
                patterns: vec![pattern.id],
                expected_impact: pattern.confidence * 0.5,
                priority: (pattern.occurrences as i32).min(10),
                created_at: Utc::now(),
                implemented: false,
            });
        }

        Ok(improvements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_feedback_submission() {
        let engine =
            ActiveLearningEngine::new(LearningConfig::default(), SimpleDetector, SimpleGenerator);

        let feedback = Feedback::new(Uuid::new_v4(), FeedbackType::Positive, "user1");
        let id = engine.submit_feedback(feedback).await.unwrap();

        let all_feedback = engine.list_feedback().await;
        assert_eq!(all_feedback.len(), 1);
        assert_eq!(all_feedback[0].id, id);
    }

    #[tokio::test]
    async fn test_pattern_detection() {
        let config = LearningConfig {
            min_feedback_for_pattern: 2,
            ..Default::default()
        };

        let engine = ActiveLearningEngine::new(config, SimpleDetector, SimpleGenerator);

        // Submit multiple negative feedback
        for i in 0..3 {
            let feedback = Feedback::new(
                Uuid::new_v4(),
                FeedbackType::Correction,
                &format!("user{}", i),
            )
            .with_correction("Corrected response");
            engine.submit_feedback(feedback).await.unwrap();
        }

        // Patterns are detected during submit_feedback when threshold is reached
        // Check the stored patterns instead of calling detect_patterns again
        let stats = engine.stats().await;
        assert!(stats.total_patterns > 0);
    }

    #[tokio::test]
    async fn test_improvement_generation() {
        let config = LearningConfig {
            min_feedback_for_pattern: 2,
            pattern_threshold: 0.5,
            ..Default::default()
        };

        let engine = ActiveLearningEngine::new(config, SimpleDetector, SimpleGenerator);

        // Add some feedback to generate patterns
        for i in 0..3 {
            let feedback = Feedback::new(
                Uuid::new_v4(),
                FeedbackType::Negative,
                &format!("user{}", i),
            )
            .with_correction("Fix this");
            engine.submit_feedback(feedback).await.unwrap();
        }

        engine.detect_patterns().await.unwrap();
        let improvements = engine.generate_improvements().await.unwrap();

        assert!(!improvements.is_empty());
    }

    #[tokio::test]
    async fn test_stats() {
        let engine =
            ActiveLearningEngine::new(LearningConfig::default(), SimpleDetector, SimpleGenerator);

        engine
            .submit_feedback(Feedback::new(
                Uuid::new_v4(),
                FeedbackType::Positive,
                "user1",
            ))
            .await
            .unwrap();
        engine
            .submit_feedback(Feedback::new(
                Uuid::new_v4(),
                FeedbackType::Positive,
                "user2",
            ))
            .await
            .unwrap();
        engine
            .submit_feedback(Feedback::new(
                Uuid::new_v4(),
                FeedbackType::Negative,
                "user3",
            ))
            .await
            .unwrap();

        let stats = engine.stats().await;
        assert_eq!(stats.total_feedback, 3);
        assert!((stats.satisfaction_rate - 0.666).abs() < 0.01);
    }
}
