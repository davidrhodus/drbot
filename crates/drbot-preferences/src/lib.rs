//! Implicit preference learning from interactions.
//!
//! This crate provides:
//! - Preference detection
//! - Preference storage
//! - Preference application
//! - Learning from feedback

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Preference errors.
#[derive(Debug, Error)]
pub enum PreferenceError {
    #[error("Learning failed: {0}")]
    LearningFailed(String),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),
}

/// Result type for preference operations.
pub type Result<T> = std::result::Result<T, PreferenceError>;

/// User preferences profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferenceProfile {
    /// Profile identifier.
    pub id: String,
    /// User identifier.
    pub user_id: String,
    /// Learned preferences.
    pub preferences: HashMap<String, Preference>,
    /// Interaction count.
    pub interaction_count: usize,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

/// A learned preference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preference {
    /// Preference key.
    pub key: String,
    /// Preference value.
    pub value: PreferenceValue,
    /// Confidence score.
    pub confidence: f64,
    /// Learning source.
    pub source: LearningSource,
    /// Times observed.
    pub observations: usize,
    /// Last observed.
    pub last_observed: DateTime<Utc>,
}

/// Preference values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreferenceValue {
    Boolean(bool),
    String(String),
    Number(f64),
    List(Vec<String>),
    Choice {
        selected: String,
        options: Vec<String>,
    },
}

/// Learning sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LearningSource {
    Explicit,
    Implicit,
    Feedback,
    Behavior,
    Default,
}

/// Interaction for learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interaction {
    /// Interaction identifier.
    pub id: String,
    /// User identifier.
    pub user_id: String,
    /// Interaction type.
    pub interaction_type: InteractionType,
    /// Context.
    pub context: HashMap<String, String>,
    /// User input.
    pub input: String,
    /// System output.
    pub output: String,
    /// Feedback.
    pub feedback: Option<Feedback>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Interaction types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractionType {
    Query,
    Command,
    Edit,
    Regenerate,
    Accept,
    Reject,
}

/// User feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    /// Feedback type.
    pub feedback_type: FeedbackType,
    /// Rating (if applicable).
    pub rating: Option<i32>,
    /// Comment.
    pub comment: Option<String>,
}

/// Feedback types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackType {
    ThumbsUp,
    ThumbsDown,
    Rating,
    Correction,
    Suggestion,
}

/// Provider for preference learning.
#[async_trait]
pub trait PreferenceLearner: Send + Sync {
    /// Extract preferences from interaction.
    async fn extract_preferences(
        &self,
        interaction: &Interaction,
    ) -> Result<Vec<(String, PreferenceValue)>>;

    /// Apply preferences to output.
    async fn apply_preferences(
        &self,
        output: &str,
        preferences: &HashMap<String, Preference>,
    ) -> Result<String>;
}

/// The preference engine.
pub struct PreferenceEngine {
    /// Preference learner.
    learner: Arc<dyn PreferenceLearner>,
    /// User profiles.
    profiles: Arc<RwLock<HashMap<String, PreferenceProfile>>>,
    /// Interaction history.
    interactions: Arc<RwLock<Vec<Interaction>>>,
}

impl PreferenceEngine {
    /// Create a new preference engine.
    pub fn new(learner: Arc<dyn PreferenceLearner>) -> Self {
        Self {
            learner,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            interactions: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Record an interaction.
    pub async fn record_interaction(
        &self,
        user_id: &str,
        interaction_type: InteractionType,
        input: &str,
        output: &str,
        context: HashMap<String, String>,
        feedback: Option<Feedback>,
    ) -> Result<()> {
        let interaction = Interaction {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            interaction_type,
            context,
            input: input.to_string(),
            output: output.to_string(),
            feedback,
            timestamp: Utc::now(),
        };

        // Store interaction
        let mut interactions = self.interactions.write().await;
        interactions.push(interaction.clone());
        if interactions.len() > 10000 {
            interactions.drain(0..1000);
        }
        drop(interactions);

        // Learn from interaction
        self.learn_from_interaction(&interaction).await?;

        Ok(())
    }

    /// Learn from an interaction.
    async fn learn_from_interaction(&self, interaction: &Interaction) -> Result<()> {
        let extracted = self.learner.extract_preferences(interaction).await?;

        if extracted.is_empty() {
            return Ok(());
        }

        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .entry(interaction.user_id.clone())
            .or_insert_with(|| PreferenceProfile {
                id: Uuid::new_v4().to_string(),
                user_id: interaction.user_id.clone(),
                preferences: HashMap::new(),
                interaction_count: 0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });

        profile.interaction_count += 1;
        profile.updated_at = Utc::now();

        for (key, value) in extracted {
            let source = match &interaction.feedback {
                Some(_) => LearningSource::Feedback,
                None => LearningSource::Implicit,
            };

            if let Some(existing) = profile.preferences.get_mut(&key) {
                existing.observations += 1;
                existing.last_observed = Utc::now();
                existing.confidence = (existing.confidence * 0.9 + 0.1).min(1.0);
                existing.value = value;
            } else {
                profile.preferences.insert(
                    key.clone(),
                    Preference {
                        key,
                        value,
                        confidence: 0.5,
                        source,
                        observations: 1,
                        last_observed: Utc::now(),
                    },
                );
            }
        }

        Ok(())
    }

    /// Set an explicit preference.
    pub async fn set_preference(
        &self,
        user_id: &str,
        key: &str,
        value: PreferenceValue,
    ) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        let profile = profiles
            .entry(user_id.to_string())
            .or_insert_with(|| PreferenceProfile {
                id: Uuid::new_v4().to_string(),
                user_id: user_id.to_string(),
                preferences: HashMap::new(),
                interaction_count: 0,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            });

        profile.preferences.insert(
            key.to_string(),
            Preference {
                key: key.to_string(),
                value,
                confidence: 1.0,
                source: LearningSource::Explicit,
                observations: 1,
                last_observed: Utc::now(),
            },
        );

        profile.updated_at = Utc::now();

        Ok(())
    }

    /// Get a preference.
    pub async fn get_preference(&self, user_id: &str, key: &str) -> Option<Preference> {
        let profiles = self.profiles.read().await;
        profiles
            .get(user_id)
            .and_then(|p| p.preferences.get(key).cloned())
    }

    /// Get all preferences for a user.
    pub async fn get_preferences(&self, user_id: &str) -> Option<HashMap<String, Preference>> {
        let profiles = self.profiles.read().await;
        profiles.get(user_id).map(|p| p.preferences.clone())
    }

    /// Apply preferences to output.
    pub async fn apply(&self, user_id: &str, output: &str) -> Result<String> {
        let profiles = self.profiles.read().await;
        let prefs = profiles
            .get(user_id)
            .map(|p| p.preferences.clone())
            .unwrap_or_default();
        drop(profiles);

        self.learner.apply_preferences(output, &prefs).await
    }

    /// Get preference profile.
    pub async fn get_profile(&self, user_id: &str) -> Option<PreferenceProfile> {
        let profiles = self.profiles.read().await;
        profiles.get(user_id).cloned()
    }

    /// Clear preferences for a user.
    pub async fn clear_preferences(&self, user_id: &str) {
        let mut profiles = self.profiles.write().await;
        profiles.remove(user_id);
    }

    /// Get top preferences by confidence.
    pub async fn get_top_preferences(&self, user_id: &str, limit: usize) -> Vec<Preference> {
        let profiles = self.profiles.read().await;
        if let Some(profile) = profiles.get(user_id) {
            let mut prefs: Vec<_> = profile.preferences.values().cloned().collect();
            prefs.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
            prefs.truncate(limit);
            prefs
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockLearner;

    #[async_trait]
    impl PreferenceLearner for MockLearner {
        async fn extract_preferences(
            &self,
            interaction: &Interaction,
        ) -> Result<Vec<(String, PreferenceValue)>> {
            let mut prefs = Vec::new();

            if interaction.input.to_lowercase().contains("brief") {
                prefs.push((
                    "response_length".to_string(),
                    PreferenceValue::String("brief".to_string()),
                ));
            }

            if interaction
                .feedback
                .as_ref()
                .map_or(false, |f| f.feedback_type == FeedbackType::ThumbsUp)
            {
                prefs.push((
                    "format_approved".to_string(),
                    PreferenceValue::Boolean(true),
                ));
            }

            Ok(prefs)
        }

        async fn apply_preferences(
            &self,
            output: &str,
            preferences: &HashMap<String, Preference>,
        ) -> Result<String> {
            let mut result = output.to_string();

            if let Some(pref) = preferences.get("response_length") {
                if let PreferenceValue::String(s) = &pref.value {
                    if s == "brief" {
                        // Simulate shortening
                        result = result.chars().take(100).collect();
                    }
                }
            }

            Ok(result)
        }
    }

    #[tokio::test]
    async fn test_set_preference() {
        let learner = Arc::new(MockLearner);
        let engine = PreferenceEngine::new(learner);

        engine
            .set_preference(
                "user1",
                "theme",
                PreferenceValue::String("dark".to_string()),
            )
            .await
            .unwrap();

        let pref = engine.get_preference("user1", "theme").await.unwrap();
        assert!(matches!(pref.value, PreferenceValue::String(s) if s == "dark"));
    }

    #[tokio::test]
    async fn test_learn_from_interaction() {
        let learner = Arc::new(MockLearner);
        let engine = PreferenceEngine::new(learner);

        engine
            .record_interaction(
                "user1",
                InteractionType::Query,
                "Give me a brief summary",
                "Here is a summary...",
                HashMap::new(),
                None,
            )
            .await
            .unwrap();

        let pref = engine
            .get_preference("user1", "response_length")
            .await
            .unwrap();
        assert!(matches!(pref.value, PreferenceValue::String(s) if s == "brief"));
    }

    #[tokio::test]
    async fn test_feedback_learning() {
        let learner = Arc::new(MockLearner);
        let engine = PreferenceEngine::new(learner);

        engine
            .record_interaction(
                "user1",
                InteractionType::Query,
                "Test",
                "Response",
                HashMap::new(),
                Some(Feedback {
                    feedback_type: FeedbackType::ThumbsUp,
                    rating: None,
                    comment: None,
                }),
            )
            .await
            .unwrap();

        let pref = engine
            .get_preference("user1", "format_approved")
            .await
            .unwrap();
        assert!(matches!(pref.value, PreferenceValue::Boolean(true)));
    }

    #[tokio::test]
    async fn test_apply_preferences() {
        let learner = Arc::new(MockLearner);
        let engine = PreferenceEngine::new(learner);

        engine
            .set_preference(
                "user1",
                "response_length",
                PreferenceValue::String("brief".to_string()),
            )
            .await
            .unwrap();

        let long_output =
            "This is a very long output that should be shortened based on user preferences."
                .repeat(10);
        let result = engine.apply("user1", &long_output).await.unwrap();

        assert!(result.len() <= 100);
    }
}
