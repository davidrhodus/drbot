//! Bidirectional learning and knowledge transfer.
//!
//! This crate provides teaching capabilities:
//! - AI learns from user corrections and feedback
//! - Teaches concepts adapted to user's level
//! - Tracks learning progress
//! - Personalizes explanations

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Teaching errors.
#[derive(Debug, Error)]
pub enum TeachError {
    #[error("Learning failed: {0}")]
    LearningFailed(String),

    #[error("Concept not found: {0}")]
    ConceptNotFound(String),

    #[error("Invalid feedback: {0}")]
    InvalidFeedback(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for teaching operations.
pub type Result<T> = std::result::Result<T, TeachError>;

/// A learner profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnerProfile {
    /// Learner identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Expertise level.
    pub level: ExpertiseLevel,
    /// Learning style preference.
    pub learning_style: LearningStyle,
    /// Known concepts.
    pub known_concepts: Vec<String>,
    /// Weak areas.
    pub weak_areas: Vec<String>,
    /// Learning history.
    pub history: Vec<LearningEvent>,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

/// Expertise level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExpertiseLevel {
    Beginner,
    Elementary,
    Intermediate,
    Advanced,
    Expert,
}

/// Learning style preferences.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LearningStyle {
    /// Prefers examples and code.
    Practical,
    /// Prefers theory and concepts.
    Theoretical,
    /// Prefers step-by-step guides.
    Sequential,
    /// Prefers visual diagrams.
    Visual,
    /// Balanced approach.
    Balanced,
}

/// A learning event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningEvent {
    /// Event ID.
    pub id: String,
    /// Event type.
    pub event_type: LearningEventType,
    /// Related concept.
    pub concept: Option<String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Details.
    pub details: serde_json::Value,
}

/// Types of learning events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LearningEventType {
    /// Concept was explained.
    ConceptExplained,
    /// User asked a question.
    QuestionAsked,
    /// Correction was provided.
    CorrectionMade,
    /// User provided feedback.
    FeedbackGiven,
    /// Assessment completed.
    AssessmentCompleted,
    /// Mastery achieved.
    MasteryAchieved,
}

/// A concept in the knowledge base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    /// Concept identifier.
    pub id: String,
    /// Concept name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Prerequisites.
    pub prerequisites: Vec<String>,
    /// Difficulty level.
    pub difficulty: DifficultyLevel,
    /// Category/domain.
    pub domain: String,
    /// Related concepts.
    pub related: Vec<String>,
    /// Examples.
    pub examples: Vec<Example>,
}

/// Difficulty level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum DifficultyLevel {
    Easy,
    Medium,
    Hard,
    Expert,
}

/// An example for a concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Example {
    /// Example title.
    pub title: String,
    /// Code/content.
    pub content: String,
    /// Explanation.
    pub explanation: String,
    /// Language if code.
    pub language: Option<String>,
}

/// An explanation of a concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    /// Explanation ID.
    pub id: String,
    /// Concept being explained.
    pub concept_id: String,
    /// The explanation text.
    pub content: String,
    /// Adapted for level.
    pub level: ExpertiseLevel,
    /// Style used.
    pub style: LearningStyle,
    /// Examples included.
    pub examples: Vec<Example>,
    /// Follow-up suggestions.
    pub follow_ups: Vec<String>,
}

/// A correction from user to AI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    /// Correction ID.
    pub id: String,
    /// What was wrong.
    pub original: String,
    /// What it should be.
    pub corrected: String,
    /// Domain/context.
    pub domain: Option<String>,
    /// Explanation of why.
    pub reason: Option<String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Feedback on an explanation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    /// Feedback ID.
    pub id: String,
    /// Explanation being rated.
    pub explanation_id: String,
    /// Rating (1-5).
    pub rating: u8,
    /// What worked.
    pub positives: Vec<String>,
    /// What didn't work.
    pub negatives: Vec<String>,
    /// Suggestions.
    pub suggestions: Option<String>,
    /// Understanding achieved.
    pub understood: bool,
}

/// Provider for teaching capabilities.
#[async_trait]
pub trait TeachProvider: Send + Sync {
    /// Generate an explanation for a concept.
    async fn explain(&self, concept: &Concept, profile: &LearnerProfile) -> Result<Explanation>;

    /// Assess understanding.
    async fn assess(&self, concept: &Concept, response: &str) -> Result<AssessmentResult>;

    /// Learn from a correction.
    async fn learn_from_correction(&self, correction: &Correction) -> Result<()>;

    /// Suggest next topics.
    async fn suggest_next(&self, profile: &LearnerProfile) -> Result<Vec<String>>;
}

/// Result of an assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssessmentResult {
    /// Concept assessed.
    pub concept_id: String,
    /// Understanding score (0.0-1.0).
    pub understanding: f64,
    /// Gaps identified.
    pub gaps: Vec<String>,
    /// Mastery achieved.
    pub mastery: bool,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// The teaching engine.
pub struct TeachingEngine {
    /// Provider for teaching.
    provider: Arc<dyn TeachProvider>,
    /// Learner profiles.
    profiles: Arc<RwLock<HashMap<String, LearnerProfile>>>,
    /// Concept knowledge base.
    concepts: Arc<RwLock<HashMap<String, Concept>>>,
    /// Learned corrections.
    corrections: Arc<RwLock<Vec<Correction>>>,
}

impl TeachingEngine {
    /// Create a new teaching engine.
    pub fn new(provider: Arc<dyn TeachProvider>) -> Self {
        Self {
            provider,
            profiles: Arc::new(RwLock::new(HashMap::new())),
            concepts: Arc::new(RwLock::new(HashMap::new())),
            corrections: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create or get a learner profile.
    pub async fn get_or_create_profile(&self, id: &str, name: &str) -> LearnerProfile {
        let mut profiles = self.profiles.write().await;

        profiles
            .entry(id.to_string())
            .or_insert_with(|| LearnerProfile {
                id: id.to_string(),
                name: name.to_string(),
                level: ExpertiseLevel::Beginner,
                learning_style: LearningStyle::Balanced,
                known_concepts: Vec::new(),
                weak_areas: Vec::new(),
                history: Vec::new(),
                created_at: Utc::now(),
            })
            .clone()
    }

    /// Register a concept.
    pub async fn register_concept(&self, concept: Concept) -> Result<()> {
        let mut concepts = self.concepts.write().await;
        concepts.insert(concept.id.clone(), concept);
        Ok(())
    }

    /// Explain a concept to a learner.
    pub async fn explain(&self, concept_id: &str, learner_id: &str) -> Result<Explanation> {
        let concepts = self.concepts.read().await;
        let concept = concepts
            .get(concept_id)
            .ok_or_else(|| TeachError::ConceptNotFound(concept_id.to_string()))?
            .clone();
        drop(concepts);

        let profile = {
            let profiles = self.profiles.read().await;
            profiles
                .get(learner_id)
                .cloned()
                .unwrap_or_else(|| LearnerProfile {
                    id: learner_id.to_string(),
                    name: "Unknown".to_string(),
                    level: ExpertiseLevel::Beginner,
                    learning_style: LearningStyle::Balanced,
                    known_concepts: Vec::new(),
                    weak_areas: Vec::new(),
                    history: Vec::new(),
                    created_at: Utc::now(),
                })
        };

        let explanation = self.provider.explain(&concept, &profile).await?;

        // Record learning event
        let mut profiles = self.profiles.write().await;
        if let Some(p) = profiles.get_mut(learner_id) {
            p.history.push(LearningEvent {
                id: Uuid::new_v4().to_string(),
                event_type: LearningEventType::ConceptExplained,
                concept: Some(concept_id.to_string()),
                timestamp: Utc::now(),
                details: serde_json::json!({}),
            });
        }

        Ok(explanation)
    }

    /// Process a correction.
    pub async fn process_correction(&self, correction: Correction) -> Result<()> {
        self.provider.learn_from_correction(&correction).await?;

        let mut corrections = self.corrections.write().await;
        corrections.push(correction);

        Ok(())
    }

    /// Process feedback on an explanation.
    pub async fn process_feedback(&self, learner_id: &str, feedback: Feedback) -> Result<()> {
        let mut profiles = self.profiles.write().await;

        if let Some(profile) = profiles.get_mut(learner_id) {
            profile.history.push(LearningEvent {
                id: Uuid::new_v4().to_string(),
                event_type: LearningEventType::FeedbackGiven,
                concept: None,
                timestamp: Utc::now(),
                details: serde_json::json!({
                    "rating": feedback.rating,
                    "understood": feedback.understood,
                }),
            });

            // Update level if consistently high ratings
            if feedback.understood && feedback.rating >= 4 {
                // Potentially level up
            }
        }

        Ok(())
    }

    /// Assess learner's understanding.
    pub async fn assess(
        &self,
        concept_id: &str,
        learner_id: &str,
        response: &str,
    ) -> Result<AssessmentResult> {
        let concepts = self.concepts.read().await;
        let concept = concepts
            .get(concept_id)
            .ok_or_else(|| TeachError::ConceptNotFound(concept_id.to_string()))?
            .clone();
        drop(concepts);

        let result = self.provider.assess(&concept, response).await?;

        // Update profile
        let mut profiles = self.profiles.write().await;
        if let Some(profile) = profiles.get_mut(learner_id) {
            profile.history.push(LearningEvent {
                id: Uuid::new_v4().to_string(),
                event_type: LearningEventType::AssessmentCompleted,
                concept: Some(concept_id.to_string()),
                timestamp: Utc::now(),
                details: serde_json::json!({
                    "understanding": result.understanding,
                    "mastery": result.mastery,
                }),
            });

            if result.mastery && !profile.known_concepts.contains(&concept_id.to_string()) {
                profile.known_concepts.push(concept_id.to_string());
                profile.history.push(LearningEvent {
                    id: Uuid::new_v4().to_string(),
                    event_type: LearningEventType::MasteryAchieved,
                    concept: Some(concept_id.to_string()),
                    timestamp: Utc::now(),
                    details: serde_json::json!({}),
                });
            }
        }

        Ok(result)
    }

    /// Get suggested next topics.
    pub async fn suggest_next(&self, learner_id: &str) -> Result<Vec<String>> {
        let profile = {
            let profiles = self.profiles.read().await;
            profiles
                .get(learner_id)
                .cloned()
                .ok_or_else(|| TeachError::LearningFailed("Profile not found".to_string()))?
        };

        self.provider.suggest_next(&profile).await
    }

    /// Get learner's progress.
    pub async fn get_progress(&self, learner_id: &str) -> Option<LearnerProgress> {
        let profiles = self.profiles.read().await;
        let concepts = self.concepts.read().await;

        profiles.get(learner_id).map(|p| LearnerProgress {
            learner_id: learner_id.to_string(),
            level: p.level,
            concepts_mastered: p.known_concepts.len(),
            total_concepts: concepts.len(),
            recent_activity: p.history.len(),
        })
    }
}

/// Learner progress summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnerProgress {
    /// Learner ID.
    pub learner_id: String,
    /// Current level.
    pub level: ExpertiseLevel,
    /// Concepts mastered.
    pub concepts_mastered: usize,
    /// Total concepts available.
    pub total_concepts: usize,
    /// Recent activity count.
    pub recent_activity: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl TeachProvider for MockProvider {
        async fn explain(
            &self,
            concept: &Concept,
            profile: &LearnerProfile,
        ) -> Result<Explanation> {
            Ok(Explanation {
                id: Uuid::new_v4().to_string(),
                concept_id: concept.id.clone(),
                content: format!(
                    "Explanation of {} for {} level",
                    concept.name,
                    format!("{:?}", profile.level)
                ),
                level: profile.level,
                style: profile.learning_style,
                examples: concept.examples.clone(),
                follow_ups: vec![],
            })
        }

        async fn assess(&self, concept: &Concept, _response: &str) -> Result<AssessmentResult> {
            Ok(AssessmentResult {
                concept_id: concept.id.clone(),
                understanding: 0.8,
                gaps: vec![],
                mastery: true,
                recommendations: vec![],
            })
        }

        async fn learn_from_correction(&self, _correction: &Correction) -> Result<()> {
            Ok(())
        }

        async fn suggest_next(&self, _profile: &LearnerProfile) -> Result<Vec<String>> {
            Ok(vec!["advanced_topic".to_string()])
        }
    }

    #[tokio::test]
    async fn test_create_profile() {
        let provider = Arc::new(MockProvider);
        let engine = TeachingEngine::new(provider);

        let profile = engine.get_or_create_profile("user1", "Alice").await;
        assert_eq!(profile.name, "Alice");
        assert_eq!(profile.level, ExpertiseLevel::Beginner);
    }

    #[tokio::test]
    async fn test_explain_concept() {
        let provider = Arc::new(MockProvider);
        let engine = TeachingEngine::new(provider);

        engine
            .register_concept(Concept {
                id: "c1".to_string(),
                name: "Variables".to_string(),
                description: "How to use variables".to_string(),
                prerequisites: vec![],
                difficulty: DifficultyLevel::Easy,
                domain: "programming".to_string(),
                related: vec![],
                examples: vec![],
            })
            .await
            .unwrap();

        engine.get_or_create_profile("user1", "Alice").await;

        let explanation = engine.explain("c1", "user1").await.unwrap();
        assert!(explanation.content.contains("Variables"));
    }

    #[tokio::test]
    async fn test_assess() {
        let provider = Arc::new(MockProvider);
        let engine = TeachingEngine::new(provider);

        engine
            .register_concept(Concept {
                id: "c1".to_string(),
                name: "Test".to_string(),
                description: "Test concept".to_string(),
                prerequisites: vec![],
                difficulty: DifficultyLevel::Easy,
                domain: "test".to_string(),
                related: vec![],
                examples: vec![],
            })
            .await
            .unwrap();

        engine.get_or_create_profile("user1", "Bob").await;

        let result = engine.assess("c1", "user1", "my answer").await.unwrap();
        assert!(result.mastery);
    }

    #[test]
    fn test_expertise_levels() {
        assert!(ExpertiseLevel::Expert > ExpertiseLevel::Advanced);
        assert!(ExpertiseLevel::Advanced > ExpertiseLevel::Intermediate);
    }
}
