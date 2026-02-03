//! Time-aware intelligence for drbot.
//!
//! Provides temporal awareness and predictions:
//! - User pattern learning by time
//! - Predictive task scheduling
//! - Deadline awareness
//! - Historical context
//! - Time-series trend analysis

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, NaiveTime, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Result type for temporal operations.
pub type Result<T> = std::result::Result<T, TemporalError>;

/// Temporal errors.
#[derive(Debug, thiserror::Error)]
pub enum TemporalError {
    #[error("Insufficient data: {0}")]
    InsufficientData(String),
    #[error("Pattern not found: {0}")]
    PatternNotFound(String),
    #[error("Invalid time range: {0}")]
    InvalidTimeRange(String),
    #[error("Prediction failed: {0}")]
    PredictionFailed(String),
}

/// Time-based user activity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    /// Activity ID.
    pub id: Uuid,
    /// Activity type.
    pub activity_type: String,
    /// Activity description.
    pub description: Option<String>,
    /// When it occurred.
    pub timestamp: DateTime<Utc>,
    /// Duration if applicable.
    pub duration: Option<Duration>,
    /// Associated context.
    pub context: HashMap<String, serde_json::Value>,
}

impl Activity {
    /// Create a new activity.
    pub fn new(activity_type: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            activity_type: activity_type.into(),
            description: None,
            timestamp: Utc::now(),
            duration: None,
            context: HashMap::new(),
        }
    }

    /// Set timestamp.
    pub fn at(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Set duration.
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }
}

/// Time pattern detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimePattern {
    /// Pattern ID.
    pub id: Uuid,
    /// Pattern type.
    pub pattern_type: PatternType,
    /// Activity type this pattern is for.
    pub activity_type: String,
    /// Time slots when this typically occurs.
    pub time_slots: Vec<TimeSlot>,
    /// Confidence in this pattern.
    pub confidence: f32,
    /// Number of observations.
    pub observations: usize,
    /// Last updated.
    pub updated_at: DateTime<Utc>,
}

/// Type of pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    /// Happens at specific time daily.
    Daily,
    /// Happens on specific weekdays.
    Weekly,
    /// Happens monthly (e.g., 1st of month).
    Monthly,
    /// Happens periodically with interval.
    Periodic,
    /// Triggered by another event.
    Triggered,
}

/// Time slot when activity typically occurs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSlot {
    /// Days of week (if applicable).
    pub days: Option<Vec<Weekday>>,
    /// Start time.
    pub start_time: NaiveTime,
    /// End time.
    pub end_time: NaiveTime,
    /// Probability of occurrence.
    pub probability: f32,
}

/// Deadline or scheduled event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deadline {
    /// Deadline ID.
    pub id: Uuid,
    /// Description.
    pub description: String,
    /// Due date/time.
    pub due: DateTime<Utc>,
    /// Priority.
    pub priority: Priority,
    /// Status.
    pub status: DeadlineStatus,
    /// Estimated effort.
    pub estimated_effort: Option<Duration>,
    /// Related tasks.
    pub related_tasks: Vec<String>,
    /// Reminders.
    pub reminders: Vec<Reminder>,
}

impl Deadline {
    /// Create a new deadline.
    pub fn new(description: impl Into<String>, due: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            description: description.into(),
            due,
            priority: Priority::Medium,
            status: DeadlineStatus::Pending,
            estimated_effort: None,
            related_tasks: Vec::new(),
            reminders: Vec::new(),
        }
    }

    /// Time remaining until deadline.
    pub fn time_remaining(&self) -> Option<Duration> {
        let now = Utc::now();
        if self.due > now {
            Some(self.due - now)
        } else {
            None
        }
    }

    /// Is this deadline overdue?
    pub fn is_overdue(&self) -> bool {
        self.due < Utc::now() && self.status == DeadlineStatus::Pending
    }
}

/// Priority level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

/// Deadline status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadlineStatus {
    Pending,
    InProgress,
    Completed,
    Missed,
    Cancelled,
}

/// Reminder for deadline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    /// When to remind.
    pub at: DateTime<Utc>,
    /// Has been sent.
    pub sent: bool,
    /// Reminder message.
    pub message: Option<String>,
}

/// Prediction of future activity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    /// Activity type predicted.
    pub activity_type: String,
    /// Predicted time.
    pub predicted_time: DateTime<Utc>,
    /// Confidence.
    pub confidence: f32,
    /// Based on pattern.
    pub based_on: Option<Uuid>,
    /// Suggested actions.
    pub suggestions: Vec<String>,
}

/// Historical context for a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalContext {
    /// Query topic.
    pub topic: String,
    /// Relevant past interactions.
    pub past_interactions: Vec<PastInteraction>,
    /// Timeline summary.
    pub timeline: Vec<TimelineEvent>,
    /// Trends observed.
    pub trends: Vec<Trend>,
}

/// Past interaction related to topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastInteraction {
    /// When it occurred.
    pub timestamp: DateTime<Utc>,
    /// What was discussed/done.
    pub summary: String,
    /// Outcome if any.
    pub outcome: Option<String>,
    /// Relevance score.
    pub relevance: f32,
}

/// Event in timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Event description.
    pub description: String,
    /// Event type.
    pub event_type: String,
}

/// Observed trend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trend {
    /// Trend description.
    pub description: String,
    /// Direction (increasing, decreasing, stable).
    pub direction: TrendDirection,
    /// Magnitude.
    pub magnitude: f32,
    /// Time period.
    pub period: String,
}

/// Trend direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrendDirection {
    Increasing,
    Decreasing,
    Stable,
    Fluctuating,
}

/// Schedule suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleSuggestion {
    /// Suggested time.
    pub suggested_time: DateTime<Utc>,
    /// Why this time.
    pub reason: String,
    /// Confidence.
    pub confidence: f32,
    /// Alternative times.
    pub alternatives: Vec<DateTime<Utc>>,
    /// Conflicts if any.
    pub conflicts: Vec<String>,
}

/// Trait for temporal providers.
#[async_trait]
pub trait TemporalProvider: Send + Sync {
    /// Detect patterns from activities.
    async fn detect_patterns(&self, activities: &[Activity]) -> Result<Vec<TimePattern>>;
    /// Predict next occurrence.
    async fn predict_next(
        &self,
        activity_type: &str,
        patterns: &[TimePattern],
    ) -> Result<Prediction>;
    /// Get optimal schedule time.
    async fn suggest_schedule(
        &self,
        task: &str,
        duration: Duration,
        constraints: &[TimeConstraint],
    ) -> Result<ScheduleSuggestion>;
    /// Get historical context.
    async fn get_history(&self, topic: &str, activities: &[Activity]) -> Result<HistoricalContext>;
    /// Analyze trends.
    async fn analyze_trends(&self, activities: &[Activity], metric: &str) -> Result<Vec<Trend>>;
}

/// Time constraint for scheduling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeConstraint {
    /// Constraint type.
    pub constraint_type: ConstraintType,
    /// Value.
    pub value: serde_json::Value,
}

/// Type of constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintType {
    /// Must be before deadline.
    Before,
    /// Must be after time.
    After,
    /// Must be on certain days.
    OnDays,
    /// Must be within time range.
    WithinHours,
    /// Avoid certain times.
    Avoid,
    /// Prefer certain times.
    Prefer,
}

/// Temporal engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalConfig {
    /// Minimum observations for pattern.
    pub min_observations: usize,
    /// Pattern confidence threshold.
    pub confidence_threshold: f32,
    /// History lookback days.
    pub history_days: u32,
    /// Prediction horizon days.
    pub prediction_horizon: u32,
}

impl Default for TemporalConfig {
    fn default() -> Self {
        Self {
            min_observations: 3,
            confidence_threshold: 0.6,
            history_days: 90,
            prediction_horizon: 14,
        }
    }
}

/// Temporal intelligence engine.
pub struct TemporalEngine<P: TemporalProvider> {
    config: TemporalConfig,
    provider: P,
    activities: Arc<RwLock<Vec<Activity>>>,
    patterns: Arc<RwLock<HashMap<String, Vec<TimePattern>>>>,
    deadlines: Arc<RwLock<Vec<Deadline>>>,
}

impl<P: TemporalProvider> TemporalEngine<P> {
    /// Create new engine.
    pub fn new(config: TemporalConfig, provider: P) -> Self {
        Self {
            config,
            provider,
            activities: Arc::new(RwLock::new(Vec::new())),
            patterns: Arc::new(RwLock::new(HashMap::new())),
            deadlines: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Record an activity.
    pub async fn record_activity(&self, activity: Activity) {
        self.activities.write().await.push(activity);
    }

    /// Learn patterns from recorded activities.
    pub async fn learn_patterns(&self) -> Result<Vec<TimePattern>> {
        let activities = self.activities.read().await;
        let patterns = self.provider.detect_patterns(&activities).await?;

        // Store patterns by activity type
        let mut pattern_map = self.patterns.write().await;
        for pattern in &patterns {
            pattern_map
                .entry(pattern.activity_type.clone())
                .or_default()
                .push(pattern.clone());
        }

        Ok(patterns)
    }

    /// Predict when something will happen.
    pub async fn predict(&self, activity_type: &str) -> Result<Prediction> {
        let patterns = self.patterns.read().await;
        let activity_patterns = patterns
            .get(activity_type)
            .ok_or_else(|| TemporalError::PatternNotFound(activity_type.to_string()))?;

        self.provider
            .predict_next(activity_type, activity_patterns)
            .await
    }

    /// Get suggestions for predictions.
    pub async fn get_predictions(&self) -> Result<Vec<Prediction>> {
        let patterns = self.patterns.read().await;
        let mut predictions = Vec::new();

        for (activity_type, activity_patterns) in patterns.iter() {
            if let Ok(prediction) = self
                .provider
                .predict_next(activity_type, activity_patterns)
                .await
            {
                if prediction.confidence >= self.config.confidence_threshold {
                    predictions.push(prediction);
                }
            }
        }

        // Sort by predicted time
        predictions.sort_by_key(|p| p.predicted_time);
        Ok(predictions)
    }

    /// Add a deadline.
    pub async fn add_deadline(&self, deadline: Deadline) {
        self.deadlines.write().await.push(deadline);
    }

    /// Get upcoming deadlines.
    pub async fn get_upcoming_deadlines(&self, within: Duration) -> Vec<Deadline> {
        let now = Utc::now();
        let cutoff = now + within;

        self.deadlines
            .read()
            .await
            .iter()
            .filter(|d| d.status == DeadlineStatus::Pending && d.due >= now && d.due <= cutoff)
            .cloned()
            .collect()
    }

    /// Get overdue deadlines.
    pub async fn get_overdue(&self) -> Vec<Deadline> {
        self.deadlines
            .read()
            .await
            .iter()
            .filter(|d| d.is_overdue())
            .cloned()
            .collect()
    }

    /// Find best time for task.
    pub async fn find_best_time(
        &self,
        task: &str,
        duration: Duration,
        constraints: Vec<TimeConstraint>,
    ) -> Result<ScheduleSuggestion> {
        self.provider
            .suggest_schedule(task, duration, &constraints)
            .await
    }

    /// Get historical context.
    pub async fn get_context(&self, topic: &str) -> Result<HistoricalContext> {
        let activities = self.activities.read().await;
        self.provider.get_history(topic, &activities).await
    }

    /// Get "last time" information.
    pub async fn last_time(&self, activity_type: &str) -> Option<Activity> {
        self.activities
            .read()
            .await
            .iter()
            .filter(|a| a.activity_type == activity_type)
            .max_by_key(|a| a.timestamp)
            .cloned()
    }

    /// Analyze trends.
    pub async fn trends(&self, metric: &str) -> Result<Vec<Trend>> {
        let activities = self.activities.read().await;
        self.provider.analyze_trends(&activities, metric).await
    }
}

/// Mock temporal provider for testing.
pub struct MockTemporalProvider;

#[async_trait]
impl TemporalProvider for MockTemporalProvider {
    async fn detect_patterns(&self, activities: &[Activity]) -> Result<Vec<TimePattern>> {
        if activities.is_empty() {
            return Err(TemporalError::InsufficientData("No activities".into()));
        }

        // Group by activity type and detect simple daily patterns
        let mut patterns = Vec::new();

        let activity_types: Vec<_> = activities
            .iter()
            .map(|a| a.activity_type.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        for activity_type in activity_types {
            let count = activities
                .iter()
                .filter(|a| a.activity_type == activity_type)
                .count();
            if count >= 2 {
                patterns.push(TimePattern {
                    id: Uuid::new_v4(),
                    pattern_type: PatternType::Daily,
                    activity_type,
                    time_slots: vec![TimeSlot {
                        days: None,
                        start_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                        end_time: NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
                        probability: 0.8,
                    }],
                    confidence: 0.7,
                    observations: count,
                    updated_at: Utc::now(),
                });
            }
        }

        Ok(patterns)
    }

    async fn predict_next(
        &self,
        activity_type: &str,
        patterns: &[TimePattern],
    ) -> Result<Prediction> {
        let pattern = patterns
            .first()
            .ok_or_else(|| TemporalError::PatternNotFound(activity_type.to_string()))?;

        let now = Utc::now();
        let tomorrow = now + Duration::days(1);
        let predicted_time = tomorrow
            .with_hour(9)
            .unwrap()
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap();

        Ok(Prediction {
            activity_type: activity_type.to_string(),
            predicted_time,
            confidence: pattern.confidence,
            based_on: Some(pattern.id),
            suggestions: vec!["Prepare in advance".to_string()],
        })
    }

    async fn suggest_schedule(
        &self,
        task: &str,
        duration: Duration,
        _constraints: &[TimeConstraint],
    ) -> Result<ScheduleSuggestion> {
        let now = Utc::now();
        let suggested = now + Duration::hours(2);

        Ok(ScheduleSuggestion {
            suggested_time: suggested,
            reason: format!(
                "Good time slot available for {} ({} mins)",
                task,
                duration.num_minutes()
            ),
            confidence: 0.8,
            alternatives: vec![now + Duration::hours(4), now + Duration::days(1)],
            conflicts: vec![],
        })
    }

    async fn get_history(&self, topic: &str, activities: &[Activity]) -> Result<HistoricalContext> {
        let relevant: Vec<_> = activities
            .iter()
            .filter(|a| {
                a.activity_type.contains(topic)
                    || a.description
                        .as_ref()
                        .map(|d| d.contains(topic))
                        .unwrap_or(false)
            })
            .collect();

        Ok(HistoricalContext {
            topic: topic.to_string(),
            past_interactions: relevant
                .iter()
                .map(|a| PastInteraction {
                    timestamp: a.timestamp,
                    summary: a
                        .description
                        .clone()
                        .unwrap_or_else(|| a.activity_type.clone()),
                    outcome: None,
                    relevance: 0.8,
                })
                .collect(),
            timeline: relevant
                .iter()
                .map(|a| TimelineEvent {
                    timestamp: a.timestamp,
                    description: a.activity_type.clone(),
                    event_type: "activity".to_string(),
                })
                .collect(),
            trends: vec![],
        })
    }

    async fn analyze_trends(&self, activities: &[Activity], metric: &str) -> Result<Vec<Trend>> {
        Ok(vec![Trend {
            description: format!("Trend in {}", metric),
            direction: TrendDirection::Stable,
            magnitude: 0.1,
            period: "30 days".to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_activity() {
        let engine = TemporalEngine::new(TemporalConfig::default(), MockTemporalProvider);

        engine.record_activity(Activity::new("coding")).await;
        engine.record_activity(Activity::new("coding")).await;
        engine.record_activity(Activity::new("meeting")).await;

        let patterns = engine.learn_patterns().await.unwrap();
        assert!(!patterns.is_empty());
    }

    #[tokio::test]
    async fn test_predict() {
        let engine = TemporalEngine::new(TemporalConfig::default(), MockTemporalProvider);

        engine.record_activity(Activity::new("standup")).await;
        engine.record_activity(Activity::new("standup")).await;
        engine.learn_patterns().await.unwrap();

        let prediction = engine.predict("standup").await.unwrap();
        assert!(prediction.confidence > 0.0);
    }

    #[tokio::test]
    async fn test_deadlines() {
        let engine = TemporalEngine::new(TemporalConfig::default(), MockTemporalProvider);

        let deadline = Deadline::new("Project due", Utc::now() + Duration::days(7));
        engine.add_deadline(deadline).await;

        let upcoming = engine.get_upcoming_deadlines(Duration::days(14)).await;
        assert_eq!(upcoming.len(), 1);
    }

    #[tokio::test]
    async fn test_overdue() {
        let engine = TemporalEngine::new(TemporalConfig::default(), MockTemporalProvider);

        let mut deadline = Deadline::new("Past deadline", Utc::now() - Duration::days(1));
        deadline.status = DeadlineStatus::Pending;
        engine.add_deadline(deadline).await;

        let overdue = engine.get_overdue().await;
        assert_eq!(overdue.len(), 1);
    }

    #[tokio::test]
    async fn test_find_best_time() {
        let engine = TemporalEngine::new(TemporalConfig::default(), MockTemporalProvider);

        let suggestion = engine
            .find_best_time("meeting", Duration::hours(1), vec![])
            .await
            .unwrap();

        assert!(suggestion.confidence > 0.0);
    }

    #[tokio::test]
    async fn test_last_time() {
        let engine = TemporalEngine::new(TemporalConfig::default(), MockTemporalProvider);

        engine.record_activity(Activity::new("review")).await;

        let last = engine.last_time("review").await;
        assert!(last.is_some());
    }
}
