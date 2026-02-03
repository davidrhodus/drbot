//! Automation suggestions based on detected patterns.
//!
//! Analyzes user behavior to suggest automations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A detected pattern in user behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedPattern {
    /// Pattern ID.
    pub id: Uuid,
    /// Pattern type.
    pub pattern_type: PatternType,
    /// Description of the pattern.
    pub description: String,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// Number of occurrences.
    pub occurrences: u32,
    /// Pattern data.
    pub data: PatternData,
    /// When first detected.
    pub first_seen: DateTime<Utc>,
    /// When last observed.
    pub last_seen: DateTime<Utc>,
}

/// Pattern types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    /// Repeated question/request.
    RepeatedRequest,
    /// Time-based activity.
    TimeBasedActivity,
    /// Sequential actions.
    SequentialActions,
    /// App switching pattern.
    AppSwitching,
    /// Communication pattern.
    CommunicationPattern,
    /// Search pattern.
    SearchPattern,
}

/// Pattern-specific data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatternData {
    /// Repeated request data.
    RepeatedRequest {
        query_template: String,
        variations: Vec<String>,
        typical_response: String,
    },
    /// Time-based activity data.
    TimeBasedActivity {
        typical_hour: u8,
        typical_minute: u8,
        days_of_week: Vec<String>,
        activity: String,
    },
    /// Sequential actions data.
    SequentialActions {
        steps: Vec<String>,
        typical_duration_secs: u64,
    },
    /// App switching data.
    AppSwitching {
        app_sequence: Vec<String>,
        context: String,
    },
    /// Communication pattern data.
    CommunicationPattern {
        contacts: Vec<String>,
        typical_time: String,
        channel: String,
    },
    /// Search pattern data.
    SearchPattern {
        query_patterns: Vec<String>,
        typical_sources: Vec<String>,
    },
}

/// Suggested automation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationSuggestion {
    /// Suggestion ID.
    pub id: Uuid,
    /// Source pattern ID.
    pub pattern_id: Uuid,
    /// Suggestion title.
    pub title: String,
    /// Description.
    pub description: String,
    /// Confidence score.
    pub confidence: f32,
    /// Estimated time saved per occurrence (seconds).
    pub time_saved_secs: u64,
    /// Automation type.
    pub automation_type: AutomationType,
    /// Configuration for the automation.
    pub config: AutomationConfig,
    /// Status.
    pub status: SuggestionStatus,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl AutomationSuggestion {
    /// Create a new suggestion from a pattern.
    pub fn from_pattern(pattern: &DetectedPattern) -> Option<Self> {
        let (title, description, automation_type, config) = match &pattern.data {
            PatternData::RepeatedRequest {
                query_template,
                typical_response,
                ..
            } => (
                "Quick Answer".to_string(),
                format!("Auto-answer for: {}", query_template),
                AutomationType::QuickResponse,
                AutomationConfig::QuickResponse {
                    trigger_phrases: vec![query_template.clone()],
                    response_template: typical_response.clone(),
                },
            ),
            PatternData::TimeBasedActivity {
                typical_hour,
                typical_minute,
                activity,
                ..
            } => (
                format!("Scheduled: {}", activity),
                format!("Run at {:02}:{:02}", typical_hour, typical_minute),
                AutomationType::ScheduledTask,
                AutomationConfig::ScheduledTask {
                    cron: format!("{} {} * * *", typical_minute, typical_hour),
                    action: activity.clone(),
                },
            ),
            PatternData::SequentialActions { steps, .. } => (
                "Workflow".to_string(),
                format!("{} step workflow", steps.len()),
                AutomationType::Workflow,
                AutomationConfig::Workflow {
                    steps: steps.clone(),
                },
            ),
            _ => return None,
        };

        Some(Self {
            id: Uuid::new_v4(),
            pattern_id: pattern.id,
            title,
            description,
            confidence: pattern.confidence,
            time_saved_secs: 30 * pattern.occurrences as u64,
            automation_type,
            config,
            status: SuggestionStatus::Pending,
            created_at: Utc::now(),
        })
    }
}

/// Automation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationType {
    /// Quick response to common questions.
    QuickResponse,
    /// Scheduled task.
    ScheduledTask,
    /// Multi-step workflow.
    Workflow,
    /// Notification/reminder.
    Reminder,
    /// Data aggregation.
    Aggregation,
    /// Integration trigger.
    Integration,
}

/// Automation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutomationConfig {
    /// Quick response config.
    QuickResponse {
        trigger_phrases: Vec<String>,
        response_template: String,
    },
    /// Scheduled task config.
    ScheduledTask { cron: String, action: String },
    /// Workflow config.
    Workflow { steps: Vec<String> },
    /// Reminder config.
    Reminder { message: String, schedule: String },
}

/// Suggestion status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionStatus {
    /// Awaiting user review.
    Pending,
    /// Accepted by user.
    Accepted,
    /// Rejected by user.
    Rejected,
    /// Dismissed (don't show again).
    Dismissed,
    /// Active automation.
    Active,
}

/// Pattern detector configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternDetectorConfig {
    /// Minimum occurrences to detect a pattern.
    pub min_occurrences: u32,
    /// Minimum confidence to suggest automation.
    pub min_confidence: f32,
    /// Time window for pattern detection (hours).
    pub time_window_hours: u32,
    /// Enable repeated request detection.
    pub detect_repeated_requests: bool,
    /// Enable time-based pattern detection.
    pub detect_time_patterns: bool,
    /// Enable sequence detection.
    pub detect_sequences: bool,
}

impl Default for PatternDetectorConfig {
    fn default() -> Self {
        Self {
            min_occurrences: 3,
            min_confidence: 0.7,
            time_window_hours: 168, // 1 week
            detect_repeated_requests: true,
            detect_time_patterns: true,
            detect_sequences: true,
        }
    }
}

/// Pattern detector.
pub struct PatternDetector {
    config: PatternDetectorConfig,
    patterns: Vec<DetectedPattern>,
    request_history: Vec<RequestRecord>,
    action_history: Vec<ActionRecord>,
}

impl PatternDetector {
    /// Create a new pattern detector.
    pub fn new(config: PatternDetectorConfig) -> Self {
        Self {
            config,
            patterns: Vec::new(),
            request_history: Vec::new(),
            action_history: Vec::new(),
        }
    }

    /// Record a user request.
    pub fn record_request(&mut self, query: &str, response: &str) {
        self.request_history.push(RequestRecord {
            query: query.to_string(),
            response: response.to_string(),
            timestamp: Utc::now(),
        });

        // Analyze for patterns
        self.analyze_request_patterns();
    }

    /// Record a user action.
    pub fn record_action(&mut self, action: &str, context: &str) {
        self.action_history.push(ActionRecord {
            action: action.to_string(),
            context: context.to_string(),
            timestamp: Utc::now(),
        });

        // Analyze for patterns
        self.analyze_action_patterns();
    }

    /// Get detected patterns.
    pub fn patterns(&self) -> &[DetectedPattern] {
        &self.patterns
    }

    /// Get automation suggestions.
    pub fn suggestions(&self) -> Vec<AutomationSuggestion> {
        self.patterns
            .iter()
            .filter(|p| p.confidence >= self.config.min_confidence)
            .filter_map(AutomationSuggestion::from_pattern)
            .collect()
    }

    fn analyze_request_patterns(&mut self) {
        if !self.config.detect_repeated_requests {
            return;
        }

        // Simple frequency-based pattern detection
        let mut query_counts: HashMap<String, (u32, String)> = HashMap::new();

        for record in &self.request_history {
            let normalized = record.query.to_lowercase();
            query_counts
                .entry(normalized)
                .and_modify(|(count, _)| *count += 1)
                .or_insert((1, record.response.clone()));
        }

        for (query, (count, response)) in query_counts {
            if count >= self.config.min_occurrences {
                let confidence = (count as f32 / self.request_history.len() as f32).min(1.0);

                if confidence >= self.config.min_confidence {
                    // Check if pattern already exists
                    if !self.patterns.iter().any(|p| {
                        matches!(&p.data, PatternData::RepeatedRequest { query_template, .. } if query_template == &query)
                    }) {
                        self.patterns.push(DetectedPattern {
                            id: Uuid::new_v4(),
                            pattern_type: PatternType::RepeatedRequest,
                            description: format!("Repeated query: {}", query),
                            confidence,
                            occurrences: count,
                            data: PatternData::RepeatedRequest {
                                query_template: query,
                                variations: Vec::new(),
                                typical_response: response,
                            },
                            first_seen: Utc::now(),
                            last_seen: Utc::now(),
                        });
                    }
                }
            }
        }
    }

    fn analyze_action_patterns(&mut self) {
        if !self.config.detect_sequences {
            return;
        }

        // Detect sequential action patterns
        if self.action_history.len() < 3 {
            return;
        }

        // Simple sequence detection (last N actions)
        let recent: Vec<_> = self
            .action_history
            .iter()
            .rev()
            .take(5)
            .map(|a| a.action.clone())
            .collect();

        // Look for this sequence in history
        let mut sequence_count = 0;
        for window in self.action_history.windows(recent.len()) {
            let window_actions: Vec<_> = window.iter().map(|a| a.action.clone()).collect();
            if window_actions == recent {
                sequence_count += 1;
            }
        }

        if sequence_count >= self.config.min_occurrences {
            let confidence = (sequence_count as f32
                / (self.action_history.len() / recent.len()) as f32)
                .min(1.0);

            if confidence >= self.config.min_confidence {
                self.patterns.push(DetectedPattern {
                    id: Uuid::new_v4(),
                    pattern_type: PatternType::SequentialActions,
                    description: format!("{} step sequence detected", recent.len()),
                    confidence,
                    occurrences: sequence_count,
                    data: PatternData::SequentialActions {
                        steps: recent,
                        typical_duration_secs: 60,
                    },
                    first_seen: Utc::now(),
                    last_seen: Utc::now(),
                });
            }
        }
    }
}

/// Request history record.
#[derive(Debug, Clone)]
struct RequestRecord {
    query: String,
    response: String,
    timestamp: DateTime<Utc>,
}

/// Action history record.
#[derive(Debug, Clone)]
struct ActionRecord {
    action: String,
    context: String,
    timestamp: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_detector() {
        let config = PatternDetectorConfig {
            min_occurrences: 2,
            min_confidence: 0.3,
            ..Default::default()
        };
        let mut detector = PatternDetector::new(config);

        // Record repeated requests
        detector.record_request("What's the weather?", "It's sunny");
        detector.record_request("What's the weather?", "It's sunny");
        detector.record_request("What's the weather?", "It's sunny");

        let patterns = detector.patterns();
        assert!(!patterns.is_empty());
    }

    #[test]
    fn test_suggestion_from_pattern() {
        let pattern = DetectedPattern {
            id: Uuid::new_v4(),
            pattern_type: PatternType::RepeatedRequest,
            description: "Test".to_string(),
            confidence: 0.8,
            occurrences: 5,
            data: PatternData::RepeatedRequest {
                query_template: "What time is it?".to_string(),
                variations: Vec::new(),
                typical_response: "Check your clock".to_string(),
            },
            first_seen: Utc::now(),
            last_seen: Utc::now(),
        };

        let suggestion = AutomationSuggestion::from_pattern(&pattern);
        assert!(suggestion.is_some());
    }
}
