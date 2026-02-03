//! Main learning system implementation.

use crate::correction::{Correction, CorrectionType, StoredCorrection};
use crate::feedback::{Feedback, FeedbackStats};
use crate::patterns::{LearnedBehavior, Modification, Pattern, PatternMatcher, PatternType};
use crate::storage::{LearningStorage, MemoryLearningStorage};
use crate::{LearningError, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Learning system configuration.
#[derive(Debug, Clone)]
pub struct LearningConfig {
    /// Whether learning is enabled.
    pub enabled: bool,
    /// Minimum observations before applying a pattern.
    pub min_observations: u32,
    /// Minimum confidence to apply a pattern.
    pub min_confidence: f32,
    /// Auto-learn from corrections.
    pub auto_learn: bool,
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_observations: 3,
            min_confidence: 0.7,
            auto_learn: true,
        }
    }
}

/// Learning statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LearningStats {
    /// Total corrections recorded.
    pub total_corrections: u64,
    /// Total feedback recorded.
    pub total_feedback: u64,
    /// Total patterns learned.
    pub total_patterns: u64,
    /// Total behaviors learned.
    pub total_behaviors: u64,
    /// Patterns applied.
    pub patterns_applied: u64,
    /// Feedback stats.
    pub feedback_stats: FeedbackStats,
}

/// The learning system.
pub struct LearningSystem {
    config: LearningConfig,
    storage: Arc<dyn LearningStorage>,
    matcher: Arc<RwLock<PatternMatcher>>,
    stats: Arc<RwLock<LearningStats>>,
}

impl LearningSystem {
    /// Create a new learning system.
    pub async fn new() -> Result<Self> {
        Self::with_config(LearningConfig::default()).await
    }

    /// Create with custom config.
    pub async fn with_config(config: LearningConfig) -> Result<Self> {
        let storage = Arc::new(MemoryLearningStorage::new());
        Self::with_storage(config, storage).await
    }

    /// Create with custom storage.
    pub async fn with_storage(
        config: LearningConfig,
        storage: Arc<dyn LearningStorage>,
    ) -> Result<Self> {
        let system = Self {
            config,
            storage,
            matcher: Arc::new(RwLock::new(PatternMatcher::new())),
            stats: Arc::new(RwLock::new(LearningStats::default())),
        };

        // Load existing patterns
        system.load_patterns().await?;

        Ok(system)
    }

    /// Load patterns from storage.
    async fn load_patterns(&self) -> Result<()> {
        let patterns = self.storage.get_patterns().await?;
        let behaviors = self.storage.get_behaviors().await?;

        let mut matcher = self.matcher.write().await;
        for pattern in patterns {
            if pattern.active {
                matcher.add_pattern(pattern);
            }
        }
        for behavior in behaviors {
            if behavior.active {
                matcher.add_behavior(behavior);
            }
        }

        info!(
            "Loaded {} patterns and {} behaviors",
            matcher.pattern_count(),
            matcher.behavior_count()
        );

        Ok(())
    }

    /// Record a correction.
    pub async fn record_correction(&self, correction: Correction) -> Result<()> {
        if !self.config.enabled {
            return Err(LearningError::Disabled);
        }

        let stored = StoredCorrection::from_correction(correction.clone());
        self.storage.store_correction(stored).await?;

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.total_corrections += 1;
        }

        // Auto-learn if enabled
        if self.config.auto_learn {
            self.learn_from_correction(&correction).await?;
        }

        debug!("Recorded correction");
        Ok(())
    }

    /// Learn from a correction.
    async fn learn_from_correction(&self, correction: &Correction) -> Result<()> {
        // Extract difference and create pattern
        if let Some((old, new)) = correction.extract_difference() {
            let pattern_type = match correction.correction_type {
                CorrectionType::Factual => PatternType::TopicKnowledge,
                CorrectionType::Terminology => PatternType::Terminology,
                CorrectionType::Style => PatternType::Style,
                CorrectionType::Format => PatternType::Format,
                _ => PatternType::Terminology,
            };

            let pattern = Pattern::new(
                pattern_type,
                &old,
                Modification::Replace {
                    from: old.clone(),
                    to: new,
                },
            );

            self.add_pattern(pattern).await?;
        }

        Ok(())
    }

    /// Add a pattern.
    pub async fn add_pattern(&self, pattern: Pattern) -> Result<()> {
        self.storage.store_pattern(pattern.clone()).await?;

        let mut matcher = self.matcher.write().await;
        matcher.add_pattern(pattern);

        let mut stats = self.stats.write().await;
        stats.total_patterns += 1;

        Ok(())
    }

    /// Record feedback.
    pub async fn record_feedback(&self, feedback: Feedback) -> Result<()> {
        if !self.config.enabled {
            return Err(LearningError::Disabled);
        }

        self.storage.store_feedback(feedback.clone()).await?;

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.total_feedback += 1;
            stats.feedback_stats.add(&feedback);
        }

        debug!("Recorded feedback");
        Ok(())
    }

    /// Apply learning to text.
    pub async fn apply_learning(&self, text: &str) -> String {
        if !self.config.enabled {
            return text.to_string();
        }

        let matcher = self.matcher.read().await;
        let result = matcher.apply_patterns(text);

        if result != text {
            let mut stats = self.stats.write().await;
            stats.patterns_applied += 1;
        }

        result
    }

    /// Get system prompt additions.
    pub async fn get_system_additions(&self) -> Vec<String> {
        let matcher = self.matcher.read().await;
        matcher.get_system_additions()
    }

    /// Get learning stats.
    pub async fn stats(&self) -> LearningStats {
        self.stats.read().await.clone()
    }

    /// Get all corrections.
    pub async fn corrections(&self) -> Result<Vec<StoredCorrection>> {
        self.storage.get_corrections().await
    }

    /// Get all feedback.
    pub async fn feedback(&self) -> Result<Vec<Feedback>> {
        self.storage.get_feedback().await
    }

    /// Get all patterns.
    pub async fn patterns(&self) -> Result<Vec<Pattern>> {
        self.storage.get_patterns().await
    }

    /// Export learning data for fine-tuning.
    pub async fn export_for_finetuning(&self) -> Result<Vec<FineTuningExample>> {
        let corrections = self.storage.get_corrections().await?;
        let feedback = self.storage.get_feedback().await?;

        let mut examples = Vec::new();

        // Export corrections as examples
        for correction in corrections {
            if let Some(context) = &correction.correction.context {
                examples.push(FineTuningExample {
                    prompt: context.clone(),
                    bad_response: Some(correction.correction.original.clone()),
                    good_response: correction.correction.corrected.clone(),
                    rating: None,
                });
            }
        }

        // Export high-rated feedback as examples
        for fb in feedback {
            if fb.rating.is_positive() {
                if let (Some(prompt), Some(response)) = (fb.prompt_text, fb.response_text) {
                    examples.push(FineTuningExample {
                        prompt,
                        bad_response: None,
                        good_response: response,
                        rating: Some(fb.rating.to_numeric()),
                    });
                }
            }
        }

        Ok(examples)
    }

    /// Clear all learning data.
    pub async fn clear(&self) -> Result<()> {
        self.storage.clear().await?;

        let mut matcher = self.matcher.write().await;
        *matcher = PatternMatcher::new();

        let mut stats = self.stats.write().await;
        *stats = LearningStats::default();

        info!("Cleared all learning data");
        Ok(())
    }

    /// Check if learning is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the config.
    pub fn config(&self) -> &LearningConfig {
        &self.config
    }
}

/// Example for fine-tuning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuningExample {
    /// The prompt/input.
    pub prompt: String,
    /// Bad response (if correction).
    pub bad_response: Option<String>,
    /// Good/corrected response.
    pub good_response: String,
    /// Rating (if from feedback).
    pub rating: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_learning_system() {
        let learning = LearningSystem::new().await.unwrap();

        // Record correction
        learning
            .record_correction(Correction::new("wrong", "right"))
            .await
            .unwrap();

        let stats = learning.stats().await;
        assert_eq!(stats.total_corrections, 1);
    }

    #[tokio::test]
    async fn test_apply_learning() {
        let learning = LearningSystem::new().await.unwrap();

        // Add a pattern manually
        let mut pattern = Pattern::new(
            PatternType::Terminology,
            "utilize",
            Modification::Replace {
                from: "utilize".to_string(),
                to: "use".to_string(),
            },
        );
        for _ in 0..5 {
            pattern.observe();
        }
        learning.add_pattern(pattern).await.unwrap();

        let result = learning.apply_learning("Please utilize this").await;
        assert_eq!(result, "Please use this");
    }

    #[tokio::test]
    async fn test_feedback_recording() {
        let learning = LearningSystem::new().await.unwrap();

        learning
            .record_feedback(Feedback::thumbs_up())
            .await
            .unwrap();
        learning
            .record_feedback(Feedback::thumbs_down())
            .await
            .unwrap();

        let stats = learning.stats().await;
        assert_eq!(stats.total_feedback, 2);
        assert_eq!(stats.feedback_stats.positive, 1);
        assert_eq!(stats.feedback_stats.negative, 1);
    }
}
