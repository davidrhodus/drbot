//! Predictive actions for drbot.
//!
//! Proactive assistance through anticipation.
//!
//! # Features
//!
//! - User behavior prediction
//! - Proactive suggestions
//! - Context-aware recommendations
//! - Smart notifications
//! - Action prediction

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Anticipate result type.
pub type Result<T> = std::result::Result<T, AnticipateError>;

/// Anticipate errors.
#[derive(Debug, thiserror::Error)]
pub enum AnticipateError {
    #[error("Insufficient data: {0}")]
    InsufficientData(String),
    #[error("Prediction failed: {0}")]
    PredictionFailed(String),
    #[error("Pattern not found: {0}")]
    PatternNotFound(String),
}

/// User action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAction {
    /// Action ID.
    pub id: Uuid,
    /// Action type.
    pub action_type: ActionType,
    /// Context when action occurred.
    pub context: ActionContext,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl UserAction {
    /// Create a new action.
    pub fn new(action_type: ActionType, context: ActionContext) -> Self {
        Self {
            id: Uuid::new_v4(),
            action_type,
            context,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }
}

/// Action types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    OpenApp(String),
    OpenFile(String),
    OpenUrl(String),
    SendMessage(String),
    CreateDocument,
    SearchQuery(String),
    ScheduleMeeting,
    SetReminder,
    Custom(String),
}

/// Context when action occurred.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionContext {
    /// Time of day (hour).
    pub hour: u32,
    /// Day of week.
    pub weekday: Weekday,
    /// Active app.
    pub active_app: Option<String>,
    /// Location type.
    pub location: Option<String>,
    /// Previous action.
    pub previous_action: Option<String>,
    /// Is in meeting.
    pub in_meeting: bool,
}

impl ActionContext {
    /// Create from current time.
    pub fn now() -> Self {
        let now = Utc::now();
        Self {
            hour: now.hour(),
            weekday: now.weekday(),
            active_app: None,
            location: None,
            previous_action: None,
            in_meeting: false,
        }
    }
}

/// Prediction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    /// Prediction ID.
    pub id: Uuid,
    /// Predicted action.
    pub action: ActionType,
    /// Confidence (0-1).
    pub confidence: f32,
    /// Reason.
    pub reason: String,
    /// Suggested time.
    pub suggested_time: Option<DateTime<Utc>>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Expires at.
    pub expires_at: DateTime<Utc>,
}

impl Prediction {
    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }
}

/// Suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// Suggestion ID.
    pub id: Uuid,
    /// Title.
    pub title: String,
    /// Description.
    pub description: String,
    /// Action to take.
    pub action: ActionType,
    /// Priority.
    pub priority: Priority,
    /// Confidence.
    pub confidence: f32,
    /// Expires at.
    pub expires_at: DateTime<Utc>,
    /// Was accepted.
    pub accepted: Option<bool>,
}

/// Priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
    Urgent,
}

/// Behavior pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorPattern {
    /// Pattern ID.
    pub id: Uuid,
    /// Pattern name.
    pub name: String,
    /// Trigger conditions.
    pub triggers: Vec<PatternTrigger>,
    /// Expected action.
    pub action: ActionType,
    /// Occurrences.
    pub occurrences: usize,
    /// Success rate.
    pub success_rate: f32,
    /// Last seen.
    pub last_seen: DateTime<Utc>,
}

/// Pattern trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternTrigger {
    /// Trigger type.
    pub trigger_type: TriggerType,
    /// Value.
    pub value: String,
}

/// Trigger types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    TimeOfDay,
    DayOfWeek,
    ActiveApp,
    Location,
    PreviousAction,
    AfterMeeting,
    Custom,
}

/// Anticipation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnticipateEvent {
    /// New prediction available.
    NewPrediction(Prediction),
    /// New suggestion.
    NewSuggestion(Suggestion),
    /// Pattern learned.
    PatternLearned(BehaviorPattern),
    /// Suggestion feedback.
    SuggestionFeedback { suggestion_id: Uuid, accepted: bool },
}

/// Anticipation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnticipateConfig {
    /// Minimum confidence for suggestions.
    pub min_confidence: f32,
    /// Maximum suggestions at once.
    pub max_suggestions: usize,
    /// History limit.
    pub history_limit: usize,
    /// Prediction horizon (minutes).
    pub prediction_horizon_mins: i64,
    /// Learning enabled.
    pub learning_enabled: bool,
}

impl Default for AnticipateConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.6,
            max_suggestions: 3,
            history_limit: 10000,
            prediction_horizon_mins: 30,
            learning_enabled: true,
        }
    }
}

/// Trait for predictors.
#[async_trait]
pub trait Predictor: Send + Sync {
    /// Predict next actions.
    async fn predict(&self, context: &ActionContext, history: &[UserAction]) -> Vec<Prediction>;
}

/// Anticipation engine.
pub struct AnticipationEngine<P: Predictor> {
    config: AnticipateConfig,
    predictor: P,
    history: Arc<RwLock<VecDeque<UserAction>>>,
    patterns: Arc<RwLock<Vec<BehaviorPattern>>>,
    predictions: Arc<RwLock<Vec<Prediction>>>,
    suggestions: Arc<RwLock<Vec<Suggestion>>>,
    event_tx: broadcast::Sender<AnticipateEvent>,
}

impl<P: Predictor> AnticipationEngine<P> {
    /// Create a new engine.
    pub fn new(config: AnticipateConfig, predictor: P) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            config,
            predictor,
            history: Arc::new(RwLock::new(VecDeque::new())),
            patterns: Arc::new(RwLock::new(Vec::new())),
            predictions: Arc::new(RwLock::new(Vec::new())),
            suggestions: Arc::new(RwLock::new(Vec::new())),
            event_tx,
        }
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<AnticipateEvent> {
        self.event_tx.subscribe()
    }

    /// Record user action.
    pub async fn record_action(&self, action: UserAction) {
        {
            let mut history = self.history.write().await;
            history.push_back(action.clone());
            while history.len() > self.config.history_limit {
                history.pop_front();
            }
        }

        // Learn patterns if enabled
        if self.config.learning_enabled {
            self.learn_patterns(&action).await;
        }
    }

    async fn learn_patterns(&self, action: &UserAction) {
        let history = self.history.read().await;

        // Look for recurring patterns
        let similar_actions: Vec<_> = history
            .iter()
            .filter(|a| a.action_type == action.action_type)
            .collect();

        if similar_actions.len() >= 3 {
            // Check for time-based pattern
            let hours: Vec<u32> = similar_actions.iter().map(|a| a.context.hour).collect();

            let most_common_hour = find_most_common(&hours);

            if let Some((hour, count)) = most_common_hour {
                if count >= 3 {
                    let pattern = BehaviorPattern {
                        id: Uuid::new_v4(),
                        name: format!("{:?} at {}:00", action.action_type, hour),
                        triggers: vec![PatternTrigger {
                            trigger_type: TriggerType::TimeOfDay,
                            value: hour.to_string(),
                        }],
                        action: action.action_type.clone(),
                        occurrences: count,
                        success_rate: 0.8,
                        last_seen: Utc::now(),
                    };

                    // Check if pattern already exists
                    let mut patterns = self.patterns.write().await;
                    if !patterns.iter().any(|p| p.name == pattern.name) {
                        patterns.push(pattern.clone());
                        let _ = self.event_tx.send(AnticipateEvent::PatternLearned(pattern));
                    }
                }
            }
        }
    }

    /// Get predictions.
    pub async fn get_predictions(&self, context: &ActionContext) -> Vec<Prediction> {
        let history: Vec<_> = self.history.read().await.iter().cloned().collect();
        let mut predictions = self.predictor.predict(context, &history).await;

        // Add pattern-based predictions
        let patterns = self.patterns.read().await;
        for pattern in patterns.iter() {
            if self.pattern_matches(pattern, context) {
                predictions.push(Prediction {
                    id: Uuid::new_v4(),
                    action: pattern.action.clone(),
                    confidence: pattern.success_rate,
                    reason: format!("Based on pattern: {}", pattern.name),
                    suggested_time: Some(Utc::now()),
                    created_at: Utc::now(),
                    expires_at: Utc::now() + Duration::minutes(self.config.prediction_horizon_mins),
                });
            }
        }

        // Sort by confidence
        predictions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Filter by minimum confidence
        predictions.retain(|p| p.confidence >= self.config.min_confidence);

        // Store predictions
        *self.predictions.write().await = predictions.clone();

        predictions
    }

    fn pattern_matches(&self, pattern: &BehaviorPattern, context: &ActionContext) -> bool {
        for trigger in &pattern.triggers {
            let matches = match trigger.trigger_type {
                TriggerType::TimeOfDay => trigger
                    .value
                    .parse::<u32>()
                    .map(|h| (h as i32 - context.hour as i32).abs() <= 1)
                    .unwrap_or(false),
                TriggerType::DayOfWeek => trigger.value == format!("{:?}", context.weekday),
                TriggerType::ActiveApp => context
                    .active_app
                    .as_ref()
                    .map(|app| app.to_lowercase().contains(&trigger.value.to_lowercase()))
                    .unwrap_or(false),
                TriggerType::Location => context
                    .location
                    .as_ref()
                    .map(|loc| loc == &trigger.value)
                    .unwrap_or(false),
                _ => false,
            };

            if !matches {
                return false;
            }
        }

        true
    }

    /// Generate suggestions.
    pub async fn generate_suggestions(&self, context: &ActionContext) -> Vec<Suggestion> {
        let predictions = self.get_predictions(context).await;

        let suggestions: Vec<_> = predictions
            .into_iter()
            .take(self.config.max_suggestions)
            .map(|p| Suggestion {
                id: Uuid::new_v4(),
                title: format!("Suggested: {:?}", p.action),
                description: p.reason,
                action: p.action,
                priority: if p.confidence > 0.9 {
                    Priority::High
                } else if p.confidence > 0.75 {
                    Priority::Medium
                } else {
                    Priority::Low
                },
                confidence: p.confidence,
                expires_at: p.expires_at,
                accepted: None,
            })
            .collect();

        // Store suggestions
        *self.suggestions.write().await = suggestions.clone();

        // Emit events
        for suggestion in &suggestions {
            let _ = self
                .event_tx
                .send(AnticipateEvent::NewSuggestion(suggestion.clone()));
        }

        suggestions
    }

    /// Record suggestion feedback.
    pub async fn record_feedback(&self, suggestion_id: Uuid, accepted: bool) {
        let mut suggestions = self.suggestions.write().await;
        if let Some(suggestion) = suggestions.iter_mut().find(|s| s.id == suggestion_id) {
            suggestion.accepted = Some(accepted);

            // Update pattern success rates
            if accepted {
                let mut patterns = self.patterns.write().await;
                for pattern in patterns.iter_mut() {
                    if pattern.action == suggestion.action {
                        // Increase success rate
                        pattern.success_rate = (pattern.success_rate * 0.9) + 0.1;
                        pattern.occurrences += 1;
                    }
                }
            } else {
                let mut patterns = self.patterns.write().await;
                for pattern in patterns.iter_mut() {
                    if pattern.action == suggestion.action {
                        // Decrease success rate
                        pattern.success_rate = pattern.success_rate * 0.9;
                    }
                }
            }

            let _ = self.event_tx.send(AnticipateEvent::SuggestionFeedback {
                suggestion_id,
                accepted,
            });
        }
    }

    /// Get learned patterns.
    pub async fn get_patterns(&self) -> Vec<BehaviorPattern> {
        self.patterns.read().await.clone()
    }

    /// Get statistics.
    pub async fn stats(&self) -> AnticipateStats {
        let history = self.history.read().await;
        let patterns = self.patterns.read().await;
        let suggestions = self.suggestions.read().await;

        let accepted = suggestions
            .iter()
            .filter(|s| s.accepted == Some(true))
            .count();
        let rejected = suggestions
            .iter()
            .filter(|s| s.accepted == Some(false))
            .count();

        AnticipateStats {
            total_actions: history.len(),
            learned_patterns: patterns.len(),
            suggestions_given: suggestions.len(),
            acceptance_rate: if accepted + rejected > 0 {
                accepted as f64 / (accepted + rejected) as f64
            } else {
                0.0
            },
        }
    }
}

fn find_most_common<T: Eq + std::hash::Hash + Clone>(items: &[T]) -> Option<(T, usize)> {
    let mut counts: HashMap<T, usize> = HashMap::new();
    for item in items {
        *counts.entry(item.clone()).or_insert(0) += 1;
    }
    counts.into_iter().max_by_key(|(_, count)| *count)
}

/// Anticipate statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnticipateStats {
    pub total_actions: usize,
    pub learned_patterns: usize,
    pub suggestions_given: usize,
    pub acceptance_rate: f64,
}

/// Simple predictor based on frequency.
pub struct FrequencyPredictor;

#[async_trait]
impl Predictor for FrequencyPredictor {
    async fn predict(&self, context: &ActionContext, history: &[UserAction]) -> Vec<Prediction> {
        // Count action frequencies at this hour
        let mut action_counts: HashMap<ActionType, usize> = HashMap::new();

        for action in history {
            if (action.context.hour as i32 - context.hour as i32).abs() <= 1 {
                *action_counts.entry(action.action_type.clone()).or_insert(0) += 1;
            }
        }

        let total: usize = action_counts.values().sum();
        if total == 0 {
            return Vec::new();
        }

        action_counts
            .into_iter()
            .filter(|(_, count)| *count >= 2) // At least 2 occurrences
            .map(|(action, count)| {
                let confidence = count as f32 / total as f32;
                Prediction {
                    id: Uuid::new_v4(),
                    action,
                    confidence,
                    reason: format!("You've done this {} times around this hour", count),
                    suggested_time: Some(Utc::now()),
                    created_at: Utc::now(),
                    expires_at: Utc::now() + Duration::minutes(30),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_action() {
        let engine = AnticipationEngine::new(AnticipateConfig::default(), FrequencyPredictor);

        let action = UserAction::new(
            ActionType::OpenApp("VSCode".to_string()),
            ActionContext::now(),
        );
        engine.record_action(action).await;

        let stats = engine.stats().await;
        assert_eq!(stats.total_actions, 1);
    }

    #[tokio::test]
    async fn test_predictions() {
        let engine = AnticipationEngine::new(AnticipateConfig::default(), FrequencyPredictor);

        let context = ActionContext::now();

        // Record some actions at current hour
        for _ in 0..5 {
            engine
                .record_action(UserAction::new(
                    ActionType::OpenApp("Terminal".to_string()),
                    context.clone(),
                ))
                .await;
        }

        let predictions = engine.get_predictions(&context).await;
        assert!(!predictions.is_empty());
    }

    #[tokio::test]
    async fn test_pattern_learning() {
        let engine = AnticipationEngine::new(AnticipateConfig::default(), FrequencyPredictor);

        let context = ActionContext::now();

        // Record same action multiple times
        for _ in 0..5 {
            engine
                .record_action(UserAction::new(
                    ActionType::OpenUrl("https://github.com".to_string()),
                    context.clone(),
                ))
                .await;
        }

        let patterns = engine.get_patterns().await;
        assert!(!patterns.is_empty());
    }

    #[tokio::test]
    async fn test_suggestions() {
        let engine = AnticipationEngine::new(AnticipateConfig::default(), FrequencyPredictor);

        let context = ActionContext::now();

        for _ in 0..5 {
            engine
                .record_action(UserAction::new(
                    ActionType::OpenApp("Slack".to_string()),
                    context.clone(),
                ))
                .await;
        }

        let suggestions = engine.generate_suggestions(&context).await;
        // May or may not have suggestions depending on confidence
        assert!(suggestions.len() <= 3); // Max suggestions limit
    }

    #[tokio::test]
    async fn test_feedback() {
        let engine = AnticipationEngine::new(AnticipateConfig::default(), FrequencyPredictor);

        let context = ActionContext::now();

        for _ in 0..5 {
            engine
                .record_action(UserAction::new(ActionType::CreateDocument, context.clone()))
                .await;
        }

        let suggestions = engine.generate_suggestions(&context).await;
        if let Some(suggestion) = suggestions.first() {
            engine.record_feedback(suggestion.id, true).await;

            let stats = engine.stats().await;
            assert!(stats.acceptance_rate > 0.0);
        }
    }

    #[tokio::test]
    async fn test_pattern_matching() {
        let config = AnticipateConfig::default();
        let engine = AnticipationEngine::new(config, FrequencyPredictor);

        let pattern = BehaviorPattern {
            id: Uuid::new_v4(),
            name: "Morning standup".to_string(),
            triggers: vec![PatternTrigger {
                trigger_type: TriggerType::TimeOfDay,
                value: "9".to_string(),
            }],
            action: ActionType::OpenApp("Zoom".to_string()),
            occurrences: 5,
            success_rate: 0.8,
            last_seen: Utc::now(),
        };

        engine.patterns.write().await.push(pattern);

        let mut context = ActionContext::now();
        context.hour = 9;

        let predictions = engine.get_predictions(&context).await;
        assert!(predictions
            .iter()
            .any(|p| matches!(&p.action, ActionType::OpenApp(app) if app == "Zoom")));
    }
}
