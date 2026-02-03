//! Thumbs up/down feedback for continuous improvement.
//!
//! This crate provides:
//! - Feedback collection
//! - Quality tracking
//! - Improvement suggestions
//! - A/B testing support

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Feedback errors.
#[derive(Debug, Error)]
pub enum FeedbackError {
    #[error("Feedback submission failed: {0}")]
    SubmissionFailed(String),

    #[error("Response not found: {0}")]
    ResponseNotFound(String),

    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
}

/// Result type for feedback operations.
pub type Result<T> = std::result::Result<T, FeedbackError>;

/// Feedback entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackEntry {
    /// Feedback identifier.
    pub id: String,
    /// Response identifier.
    pub response_id: String,
    /// User identifier.
    pub user_id: String,
    /// Feedback type.
    pub feedback_type: FeedbackType,
    /// Rating (1-5).
    pub rating: Option<i32>,
    /// Comment.
    pub comment: Option<String>,
    /// Categories.
    pub categories: Vec<FeedbackCategory>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Context.
    pub context: FeedbackContext,
}

/// Feedback types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackType {
    ThumbsUp,
    ThumbsDown,
    Rating,
    Report,
    Suggestion,
}

/// Feedback categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeedbackCategory {
    Accuracy,
    Relevance,
    Completeness,
    Clarity,
    Tone,
    Speed,
    Helpfulness,
    Safety,
    Other,
}

/// Feedback context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedbackContext {
    /// Original prompt.
    pub prompt: String,
    /// Response given.
    pub response: String,
    /// Model used.
    pub model: Option<String>,
    /// Additional metadata.
    pub metadata: HashMap<String, String>,
}

/// Quality metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// Period start.
    pub period_start: DateTime<Utc>,
    /// Period end.
    pub period_end: DateTime<Utc>,
    /// Total feedback count.
    pub total_feedback: usize,
    /// Positive feedback count.
    pub positive_count: usize,
    /// Negative feedback count.
    pub negative_count: usize,
    /// Average rating.
    pub avg_rating: f64,
    /// Satisfaction rate.
    pub satisfaction_rate: f64,
    /// By category.
    pub by_category: HashMap<FeedbackCategory, CategoryMetrics>,
    /// By model.
    pub by_model: HashMap<String, ModelMetrics>,
}

/// Category metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CategoryMetrics {
    /// Positive count.
    pub positive: usize,
    /// Negative count.
    pub negative: usize,
    /// Total.
    pub total: usize,
}

/// Model metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelMetrics {
    /// Positive count.
    pub positive: usize,
    /// Negative count.
    pub negative: usize,
    /// Average rating.
    pub avg_rating: f64,
    /// Total.
    pub total: usize,
}

/// Improvement suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementSuggestion {
    /// Suggestion identifier.
    pub id: String,
    /// Area.
    pub area: FeedbackCategory,
    /// Description.
    pub description: String,
    /// Priority.
    pub priority: Priority,
    /// Based on feedback count.
    pub feedback_count: usize,
    /// Generated at.
    pub generated_at: DateTime<Utc>,
}

/// Priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

/// Feedback analyzer provider.
#[async_trait]
pub trait FeedbackAnalyzer: Send + Sync {
    /// Analyze feedback patterns.
    async fn analyze_patterns(
        &self,
        feedback: &[FeedbackEntry],
    ) -> Result<Vec<ImprovementSuggestion>>;

    /// Categorize feedback comment.
    async fn categorize_comment(&self, comment: &str) -> Result<Vec<FeedbackCategory>>;
}

/// The feedback engine.
pub struct FeedbackEngine {
    /// Feedback analyzer.
    analyzer: Arc<dyn FeedbackAnalyzer>,
    /// Feedback entries.
    feedback: Arc<RwLock<Vec<FeedbackEntry>>>,
    /// Suggestions cache.
    suggestions: Arc<RwLock<Vec<ImprovementSuggestion>>>,
}

impl FeedbackEngine {
    /// Create a new feedback engine.
    pub fn new(analyzer: Arc<dyn FeedbackAnalyzer>) -> Self {
        Self {
            analyzer,
            feedback: Arc::new(RwLock::new(Vec::new())),
            suggestions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Submit feedback.
    pub async fn submit(
        &self,
        response_id: &str,
        user_id: &str,
        feedback_type: FeedbackType,
        rating: Option<i32>,
        comment: Option<String>,
        context: FeedbackContext,
    ) -> Result<String> {
        // Categorize comment if provided
        let categories = if let Some(c) = &comment {
            self.analyzer.categorize_comment(c).await?
        } else {
            vec![]
        };

        let entry = FeedbackEntry {
            id: Uuid::new_v4().to_string(),
            response_id: response_id.to_string(),
            user_id: user_id.to_string(),
            feedback_type,
            rating,
            comment,
            categories,
            timestamp: Utc::now(),
            context,
        };

        let id = entry.id.clone();
        let mut feedback = self.feedback.write().await;
        feedback.push(entry);

        // Keep last 100000 entries
        if feedback.len() > 100000 {
            feedback.drain(0..10000);
        }

        Ok(id)
    }

    /// Submit thumbs up.
    pub async fn thumbs_up(
        &self,
        response_id: &str,
        user_id: &str,
        context: FeedbackContext,
    ) -> Result<String> {
        self.submit(
            response_id,
            user_id,
            FeedbackType::ThumbsUp,
            None,
            None,
            context,
        )
        .await
    }

    /// Submit thumbs down.
    pub async fn thumbs_down(
        &self,
        response_id: &str,
        user_id: &str,
        comment: Option<String>,
        context: FeedbackContext,
    ) -> Result<String> {
        self.submit(
            response_id,
            user_id,
            FeedbackType::ThumbsDown,
            None,
            comment,
            context,
        )
        .await
    }

    /// Get quality metrics for a period.
    pub async fn get_metrics(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> QualityMetrics {
        let feedback = self.feedback.read().await;
        let filtered: Vec<_> = feedback
            .iter()
            .filter(|f| f.timestamp >= start && f.timestamp <= end)
            .collect();

        let total = filtered.len();
        let positive = filtered
            .iter()
            .filter(|f| f.feedback_type == FeedbackType::ThumbsUp)
            .count();
        let negative = filtered
            .iter()
            .filter(|f| f.feedback_type == FeedbackType::ThumbsDown)
            .count();

        let ratings: Vec<_> = filtered.iter().filter_map(|f| f.rating).collect();
        let avg_rating = if ratings.is_empty() {
            0.0
        } else {
            ratings.iter().sum::<i32>() as f64 / ratings.len() as f64
        };

        let satisfaction_rate = if total > 0 {
            positive as f64 / total as f64
        } else {
            0.0
        };

        // By category
        let mut by_category: HashMap<FeedbackCategory, CategoryMetrics> = HashMap::new();
        for entry in &filtered {
            for cat in &entry.categories {
                let metrics = by_category.entry(*cat).or_default();
                metrics.total += 1;
                if entry.feedback_type == FeedbackType::ThumbsUp {
                    metrics.positive += 1;
                } else if entry.feedback_type == FeedbackType::ThumbsDown {
                    metrics.negative += 1;
                }
            }
        }

        // By model
        let mut by_model: HashMap<String, ModelMetrics> = HashMap::new();
        for entry in &filtered {
            if let Some(model) = &entry.context.model {
                let metrics = by_model.entry(model.clone()).or_default();
                metrics.total += 1;
                if entry.feedback_type == FeedbackType::ThumbsUp {
                    metrics.positive += 1;
                } else if entry.feedback_type == FeedbackType::ThumbsDown {
                    metrics.negative += 1;
                }
                if let Some(r) = entry.rating {
                    let n = metrics.total as f64;
                    metrics.avg_rating = ((metrics.avg_rating * (n - 1.0)) + r as f64) / n;
                }
            }
        }

        QualityMetrics {
            period_start: start,
            period_end: end,
            total_feedback: total,
            positive_count: positive,
            negative_count: negative,
            avg_rating,
            satisfaction_rate,
            by_category,
            by_model,
        }
    }

    /// Generate improvement suggestions.
    pub async fn generate_suggestions(&self) -> Result<Vec<ImprovementSuggestion>> {
        let feedback = self.feedback.read().await;
        let suggestions = self.analyzer.analyze_patterns(&feedback).await?;

        let mut stored = self.suggestions.write().await;
        *stored = suggestions.clone();

        Ok(suggestions)
    }

    /// Get cached suggestions.
    pub async fn get_suggestions(&self) -> Vec<ImprovementSuggestion> {
        let suggestions = self.suggestions.read().await;
        suggestions.clone()
    }

    /// Get feedback for a response.
    pub async fn get_response_feedback(&self, response_id: &str) -> Vec<FeedbackEntry> {
        let feedback = self.feedback.read().await;
        feedback
            .iter()
            .filter(|f| f.response_id == response_id)
            .cloned()
            .collect()
    }

    /// Get user's feedback history.
    pub async fn get_user_feedback(&self, user_id: &str, limit: usize) -> Vec<FeedbackEntry> {
        let feedback = self.feedback.read().await;
        feedback
            .iter()
            .filter(|f| f.user_id == user_id)
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get negative feedback for analysis.
    pub async fn get_negative_feedback(&self, limit: usize) -> Vec<FeedbackEntry> {
        let feedback = self.feedback.read().await;
        feedback
            .iter()
            .filter(|f| f.feedback_type == FeedbackType::ThumbsDown)
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAnalyzer;

    #[async_trait]
    impl FeedbackAnalyzer for MockAnalyzer {
        async fn analyze_patterns(
            &self,
            feedback: &[FeedbackEntry],
        ) -> Result<Vec<ImprovementSuggestion>> {
            let negative_count = feedback
                .iter()
                .filter(|f| f.feedback_type == FeedbackType::ThumbsDown)
                .count();

            if negative_count > 2 {
                Ok(vec![ImprovementSuggestion {
                    id: Uuid::new_v4().to_string(),
                    area: FeedbackCategory::Accuracy,
                    description: "High negative feedback rate detected".to_string(),
                    priority: Priority::High,
                    feedback_count: negative_count,
                    generated_at: Utc::now(),
                }])
            } else {
                Ok(vec![])
            }
        }

        async fn categorize_comment(&self, comment: &str) -> Result<Vec<FeedbackCategory>> {
            let mut categories = Vec::new();
            let lower = comment.to_lowercase();

            if lower.contains("wrong") || lower.contains("incorrect") {
                categories.push(FeedbackCategory::Accuracy);
            }
            if lower.contains("unclear") || lower.contains("confusing") {
                categories.push(FeedbackCategory::Clarity);
            }

            Ok(categories)
        }
    }

    fn create_context() -> FeedbackContext {
        FeedbackContext {
            prompt: "Test prompt".to_string(),
            response: "Test response".to_string(),
            model: Some("gpt-4".to_string()),
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_submit_thumbs_up() {
        let analyzer = Arc::new(MockAnalyzer);
        let engine = FeedbackEngine::new(analyzer);

        let id = engine
            .thumbs_up("resp1", "user1", create_context())
            .await
            .unwrap();
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn test_submit_thumbs_down() {
        let analyzer = Arc::new(MockAnalyzer);
        let engine = FeedbackEngine::new(analyzer);

        let id = engine
            .thumbs_down(
                "resp1",
                "user1",
                Some("This was wrong".to_string()),
                create_context(),
            )
            .await
            .unwrap();

        let feedback = engine.get_response_feedback("resp1").await;
        assert_eq!(feedback.len(), 1);
        assert!(feedback[0].categories.contains(&FeedbackCategory::Accuracy));
    }

    #[tokio::test]
    async fn test_get_metrics() {
        let analyzer = Arc::new(MockAnalyzer);
        let engine = FeedbackEngine::new(analyzer);

        engine
            .thumbs_up("resp1", "user1", create_context())
            .await
            .unwrap();
        engine
            .thumbs_up("resp2", "user1", create_context())
            .await
            .unwrap();
        engine
            .thumbs_down("resp3", "user1", None, create_context())
            .await
            .unwrap();

        let metrics = engine
            .get_metrics(
                Utc::now() - chrono::Duration::hours(1),
                Utc::now() + chrono::Duration::hours(1),
            )
            .await;

        assert_eq!(metrics.total_feedback, 3);
        assert_eq!(metrics.positive_count, 2);
        assert_eq!(metrics.negative_count, 1);
    }

    #[tokio::test]
    async fn test_generate_suggestions() {
        let analyzer = Arc::new(MockAnalyzer);
        let engine = FeedbackEngine::new(analyzer);

        for i in 0..5 {
            engine
                .thumbs_down(&format!("resp{}", i), "user1", None, create_context())
                .await
                .unwrap();
        }

        let suggestions = engine.generate_suggestions().await.unwrap();
        assert!(!suggestions.is_empty());
    }
}
