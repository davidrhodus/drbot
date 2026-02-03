//! Predictive assistance for drbot.
//!
//! Anticipates user needs and pre-computes likely requests.
//!
//! # Features
//!
//! - Predict next likely requests
//! - Pre-load relevant context
//! - Anticipatory caching
//! - Pattern-based predictions
//! - Context-aware suggestions

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Prediction result type.
pub type Result<T> = std::result::Result<T, PredictError>;

/// Prediction errors.
#[derive(Debug, thiserror::Error)]
pub enum PredictError {
    #[error("No predictions available")]
    NoPredictions,
    #[error("Context load failed: {0}")]
    ContextLoadFailed(String),
    #[error("Pattern not found: {0}")]
    PatternNotFound(String),
}

/// A predicted action or request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    /// Prediction ID.
    pub id: Uuid,
    /// What we predict the user will do.
    pub action: PredictedAction,
    /// Confidence score (0-1).
    pub confidence: f32,
    /// Reasoning for this prediction.
    pub reasoning: String,
    /// Context that triggered this prediction.
    pub trigger_context: TriggerContext,
    /// Pre-computed result (if available).
    pub precomputed: Option<serde_json::Value>,
    /// When this prediction was made.
    pub predicted_at: DateTime<Utc>,
    /// Expiration time.
    pub expires_at: DateTime<Utc>,
}

impl Prediction {
    /// Create a new prediction.
    pub fn new(action: PredictedAction, confidence: f32, reasoning: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            action,
            confidence: confidence.clamp(0.0, 1.0),
            reasoning: reasoning.to_string(),
            trigger_context: TriggerContext::default(),
            precomputed: None,
            predicted_at: now,
            expires_at: now + chrono::Duration::minutes(5),
        }
    }

    /// Set precomputed result.
    pub fn with_precomputed(mut self, result: serde_json::Value) -> Self {
        self.precomputed = Some(result);
        self
    }

    /// Set expiration.
    pub fn expires_in(mut self, minutes: i64) -> Self {
        self.expires_at = Utc::now() + chrono::Duration::minutes(minutes);
        self
    }

    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// Predicted action types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PredictedAction {
    /// User will ask a question.
    Question { topic: String },
    /// User will request a specific action.
    Action {
        action: String,
        target: Option<String>,
    },
    /// User will search for something.
    Search { query: String },
    /// User will open a file/document.
    OpenFile { path: String },
    /// User will send a message.
    SendMessage { channel: String },
    /// User will schedule something.
    Schedule { event_type: String },
    /// User will follow up on previous topic.
    FollowUp { previous_topic: String },
    /// Custom prediction.
    Custom { description: String },
}

/// Context that triggered a prediction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TriggerContext {
    /// Current application.
    pub current_app: Option<String>,
    /// Current file/document.
    pub current_file: Option<String>,
    /// Recent conversation topics.
    pub recent_topics: Vec<String>,
    /// Time of day.
    pub time_context: Option<TimeContext>,
    /// Location context.
    pub location: Option<String>,
    /// Calendar context.
    pub calendar_context: Option<CalendarContext>,
}

/// Time-based context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeContext {
    /// Day of week.
    pub day_of_week: u8,
    /// Hour of day.
    pub hour: u8,
    /// Is working hours.
    pub is_work_hours: bool,
}

/// Calendar context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarContext {
    /// Upcoming meetings.
    pub upcoming_meetings: Vec<String>,
    /// Minutes until next meeting.
    pub minutes_to_next: Option<i64>,
}

/// Historical action for learning patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalAction {
    /// Action ID.
    pub id: Uuid,
    /// What the user did.
    pub action: String,
    /// Context when action occurred.
    pub context: TriggerContext,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Session ID.
    pub session_id: Option<String>,
}

/// Learned pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Pattern ID.
    pub id: Uuid,
    /// Pattern name.
    pub name: String,
    /// Trigger conditions.
    pub triggers: Vec<PatternTrigger>,
    /// Predicted action.
    pub predicted_action: PredictedAction,
    /// Base confidence.
    pub base_confidence: f32,
    /// Times this pattern matched.
    pub match_count: u64,
    /// Times prediction was correct.
    pub correct_count: u64,
}

impl Pattern {
    /// Create a new pattern.
    pub fn new(name: &str, action: PredictedAction) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            triggers: Vec::new(),
            predicted_action: action,
            base_confidence: 0.5,
            match_count: 0,
            correct_count: 0,
        }
    }

    /// Add trigger condition.
    pub fn with_trigger(mut self, trigger: PatternTrigger) -> Self {
        self.triggers.push(trigger);
        self
    }

    /// Calculate confidence based on history.
    pub fn confidence(&self) -> f32 {
        if self.match_count == 0 {
            return self.base_confidence;
        }

        let accuracy = self.correct_count as f32 / self.match_count as f32;
        // Blend base confidence with learned accuracy
        (self.base_confidence + accuracy) / 2.0
    }

    /// Record a match.
    pub fn record_match(&mut self, was_correct: bool) {
        self.match_count += 1;
        if was_correct {
            self.correct_count += 1;
        }
    }
}

/// Pattern trigger condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatternTrigger {
    /// App is active.
    AppActive { app_name: String },
    /// File is open.
    FileOpen { pattern: String },
    /// Time of day.
    TimeOfDay { hour_start: u8, hour_end: u8 },
    /// Day of week.
    DayOfWeek { days: Vec<u8> },
    /// After specific action.
    AfterAction { action: String },
    /// Topic mentioned.
    TopicMentioned { topic: String },
    /// Calendar event upcoming.
    MeetingSoon { minutes: i64 },
}

impl PatternTrigger {
    /// Check if trigger matches context.
    pub fn matches(&self, context: &TriggerContext) -> bool {
        match self {
            PatternTrigger::AppActive { app_name } => context
                .current_app
                .as_ref()
                .map(|a| a == app_name)
                .unwrap_or(false),
            PatternTrigger::FileOpen { pattern } => context
                .current_file
                .as_ref()
                .map(|f| f.contains(pattern))
                .unwrap_or(false),
            PatternTrigger::TimeOfDay {
                hour_start,
                hour_end,
            } => context
                .time_context
                .as_ref()
                .map(|t| t.hour >= *hour_start && t.hour < *hour_end)
                .unwrap_or(false),
            PatternTrigger::DayOfWeek { days } => context
                .time_context
                .as_ref()
                .map(|t| days.contains(&t.day_of_week))
                .unwrap_or(false),
            PatternTrigger::TopicMentioned { topic } => {
                context.recent_topics.iter().any(|t| t.contains(topic))
            }
            PatternTrigger::MeetingSoon { minutes } => context
                .calendar_context
                .as_ref()
                .and_then(|c| c.minutes_to_next)
                .map(|m| m <= *minutes)
                .unwrap_or(false),
            _ => false,
        }
    }
}

/// Prediction configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictConfig {
    /// Enable predictions.
    pub enabled: bool,
    /// Minimum confidence to show prediction.
    pub min_confidence: f32,
    /// Maximum predictions to generate.
    pub max_predictions: usize,
    /// Enable precomputation.
    pub enable_precompute: bool,
    /// Prediction expiry (minutes).
    pub expiry_minutes: i64,
    /// Learn from user actions.
    pub learn_from_actions: bool,
}

impl Default for PredictConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: 0.6,
            max_predictions: 5,
            enable_precompute: true,
            expiry_minutes: 5,
            learn_from_actions: true,
        }
    }
}

/// Predictive assistant.
pub struct PredictiveAssistant {
    config: PredictConfig,
    patterns: Arc<RwLock<Vec<Pattern>>>,
    history: Arc<RwLock<VecDeque<HistoricalAction>>>,
    current_predictions: Arc<RwLock<Vec<Prediction>>>,
}

impl PredictiveAssistant {
    /// Create a new predictive assistant.
    pub fn new(config: PredictConfig) -> Self {
        let mut assistant = Self {
            config,
            patterns: Arc::new(RwLock::new(Vec::new())),
            history: Arc::new(RwLock::new(VecDeque::new())),
            current_predictions: Arc::new(RwLock::new(Vec::new())),
        };

        // Add default patterns
        tokio::spawn({
            let patterns = assistant.patterns.clone();
            async move {
                let mut p = patterns.write().await;
                p.extend(Self::default_patterns());
            }
        });

        assistant
    }

    fn default_patterns() -> Vec<Pattern> {
        vec![
            // Morning standup pattern
            Pattern::new(
                "morning_standup",
                PredictedAction::Action {
                    action: "prepare_standup".to_string(),
                    target: Some("daily tasks".to_string()),
                },
            )
            .with_trigger(PatternTrigger::TimeOfDay {
                hour_start: 9,
                hour_end: 10,
            })
            .with_trigger(PatternTrigger::DayOfWeek {
                days: vec![1, 2, 3, 4, 5],
            }),
            // Meeting prep pattern
            Pattern::new(
                "meeting_prep",
                PredictedAction::Action {
                    action: "prepare_meeting".to_string(),
                    target: None,
                },
            )
            .with_trigger(PatternTrigger::MeetingSoon { minutes: 15 }),
            // Code review after commit
            Pattern::new(
                "post_commit_review",
                PredictedAction::Action {
                    action: "review_changes".to_string(),
                    target: None,
                },
            )
            .with_trigger(PatternTrigger::AfterAction {
                action: "git_commit".to_string(),
            }),
        ]
    }

    /// Generate predictions based on current context.
    pub async fn predict(&self, context: &TriggerContext) -> Vec<Prediction> {
        let patterns = self.patterns.read().await;
        let mut predictions = Vec::new();

        for pattern in patterns.iter() {
            // Check if all triggers match
            let all_match = pattern.triggers.iter().all(|t| t.matches(context));

            if all_match {
                let confidence = pattern.confidence();

                if confidence >= self.config.min_confidence {
                    predictions.push(Prediction::new(
                        pattern.predicted_action.clone(),
                        confidence,
                        &format!("Matched pattern: {}", pattern.name),
                    ));
                }
            }
        }

        // Sort by confidence
        predictions.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit to max predictions
        predictions.truncate(self.config.max_predictions);

        // Store current predictions
        *self.current_predictions.write().await = predictions.clone();

        predictions
    }

    /// Record a user action for learning.
    pub async fn record_action(&self, action: &str, context: TriggerContext) {
        if !self.config.learn_from_actions {
            return;
        }

        let historical = HistoricalAction {
            id: Uuid::new_v4(),
            action: action.to_string(),
            context,
            timestamp: Utc::now(),
            session_id: None,
        };

        let mut history = self.history.write().await;
        history.push_back(historical);

        // Keep only last 1000 actions
        while history.len() > 1000 {
            history.pop_front();
        }

        // Check if action matched any prediction
        let predictions = self.current_predictions.read().await;
        let mut patterns = self.patterns.write().await;

        for pattern in patterns.iter_mut() {
            let action_matches = match &pattern.predicted_action {
                PredictedAction::Action { action: a, .. } => a == action,
                _ => false,
            };

            if action_matches {
                pattern.record_match(true);
            }
        }
    }

    /// Add a custom pattern.
    pub async fn add_pattern(&self, pattern: Pattern) {
        self.patterns.write().await.push(pattern);
    }

    /// Get current predictions.
    pub async fn current_predictions(&self) -> Vec<Prediction> {
        self.current_predictions
            .read()
            .await
            .iter()
            .filter(|p| !p.is_expired())
            .cloned()
            .collect()
    }

    /// Precompute result for a prediction.
    pub async fn precompute<F, Fut>(&self, prediction_id: Uuid, compute: F) -> Result<()>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = serde_json::Value>,
    {
        let result = compute().await;

        let mut predictions = self.current_predictions.write().await;
        if let Some(p) = predictions.iter_mut().find(|p| p.id == prediction_id) {
            p.precomputed = Some(result);
            Ok(())
        } else {
            Err(PredictError::NoPredictions)
        }
    }

    /// Get pattern statistics.
    pub async fn pattern_stats(&self) -> Vec<PatternStats> {
        self.patterns
            .read()
            .await
            .iter()
            .map(|p| PatternStats {
                name: p.name.clone(),
                match_count: p.match_count,
                correct_count: p.correct_count,
                confidence: p.confidence(),
            })
            .collect()
    }
}

/// Pattern statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternStats {
    /// Pattern name.
    pub name: String,
    /// Times matched.
    pub match_count: u64,
    /// Times correct.
    pub correct_count: u64,
    /// Current confidence.
    pub confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_prediction() {
        let config = PredictConfig {
            min_confidence: 0.3, // Lower threshold for testing
            ..Default::default()
        };
        let assistant = PredictiveAssistant::new(config);

        // Add a simple pattern with higher base confidence
        let mut pattern = Pattern::new(
            "test_pattern",
            PredictedAction::Question {
                topic: "testing".to_string(),
            },
        );
        pattern.base_confidence = 0.8;
        pattern = pattern.with_trigger(PatternTrigger::AppActive {
            app_name: "VSCode".to_string(),
        });

        assistant.add_pattern(pattern).await;

        // Create matching context
        let context = TriggerContext {
            current_app: Some("VSCode".to_string()),
            ..Default::default()
        };

        let predictions = assistant.predict(&context).await;
        assert!(!predictions.is_empty());
    }

    #[test]
    fn test_pattern_confidence() {
        let mut pattern = Pattern::new(
            "test",
            PredictedAction::Custom {
                description: "test".to_string(),
            },
        );

        assert_eq!(pattern.confidence(), 0.5); // Base confidence

        pattern.record_match(true);
        pattern.record_match(true);
        pattern.record_match(false);

        // (0.5 + 0.66) / 2 ≈ 0.58
        assert!(pattern.confidence() > 0.55);
    }

    #[test]
    fn test_trigger_matching() {
        let trigger = PatternTrigger::TimeOfDay {
            hour_start: 9,
            hour_end: 17,
        };

        let context = TriggerContext {
            time_context: Some(TimeContext {
                day_of_week: 1,
                hour: 10,
                is_work_hours: true,
            }),
            ..Default::default()
        };

        assert!(trigger.matches(&context));
    }
}
