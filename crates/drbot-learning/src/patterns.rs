//! Pattern detection and learned behaviors.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A learned pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Pattern ID.
    pub id: String,
    /// Pattern type.
    pub pattern_type: PatternType,
    /// Trigger (what activates this pattern).
    pub trigger: String,
    /// Response modification.
    pub modification: Modification,
    /// Confidence (0.0 - 1.0).
    pub confidence: f32,
    /// Times this pattern was observed.
    pub observations: u32,
    /// Times this pattern was applied.
    pub applications: u32,
    /// Whether this pattern is active.
    pub active: bool,
    /// Created timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last updated timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Pattern type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternType {
    /// Terminology preference (use X instead of Y).
    Terminology,
    /// Style preference (formal, casual, etc.).
    Style,
    /// Format preference (bullets, paragraphs, etc.).
    Format,
    /// Length preference (brief, detailed).
    Length,
    /// Topic-specific knowledge.
    TopicKnowledge,
    /// Behavioral preference.
    Behavior,
}

/// How to modify the response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Modification {
    /// Replace text.
    Replace { from: String, to: String },
    /// Prepend text.
    Prepend(String),
    /// Append text.
    Append(String),
    /// Change style.
    StyleChange(StyleModification),
    /// Add instruction to system prompt.
    SystemInstruction(String),
}

/// Style modification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StyleModification {
    MoreFormal,
    MoreCasual,
    MoreConcise,
    MoreDetailed,
    MoreTechnical,
    LessTechnical,
}

impl Pattern {
    /// Create a new pattern.
    pub fn new(
        pattern_type: PatternType,
        trigger: impl Into<String>,
        modification: Modification,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            pattern_type,
            trigger: trigger.into(),
            modification,
            confidence: 0.5,
            observations: 1,
            applications: 0,
            active: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// Record an observation of this pattern.
    pub fn observe(&mut self) {
        self.observations += 1;
        self.confidence = (self.confidence + 0.1).min(1.0);
        self.updated_at = chrono::Utc::now();
    }

    /// Record that this pattern was applied.
    pub fn apply(&mut self) {
        self.applications += 1;
        self.updated_at = chrono::Utc::now();
    }

    /// Deactivate this pattern.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.updated_at = chrono::Utc::now();
    }

    /// Check if pattern should be applied based on confidence.
    pub fn should_apply(&self) -> bool {
        self.active && self.confidence >= 0.7 && self.observations >= 3
    }
}

/// A learned behavior from multiple patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedBehavior {
    /// Behavior ID.
    pub id: String,
    /// Behavior description.
    pub description: String,
    /// Related pattern IDs.
    pub pattern_ids: Vec<String>,
    /// Combined confidence.
    pub confidence: f32,
    /// System prompt addition.
    pub system_addition: Option<String>,
    /// Active.
    pub active: bool,
}

impl LearnedBehavior {
    /// Create a new learned behavior.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            description: description.into(),
            pattern_ids: Vec::new(),
            confidence: 0.5,
            system_addition: None,
            active: true,
        }
    }

    /// Add a pattern.
    pub fn add_pattern(&mut self, pattern_id: impl Into<String>) {
        self.pattern_ids.push(pattern_id.into());
    }

    /// Set system prompt addition.
    pub fn with_system_addition(mut self, addition: impl Into<String>) -> Self {
        self.system_addition = Some(addition.into());
        self
    }
}

/// Pattern matcher for detecting patterns in text.
#[derive(Debug, Default)]
pub struct PatternMatcher {
    patterns: Vec<Pattern>,
    behaviors: Vec<LearnedBehavior>,
}

impl PatternMatcher {
    /// Create a new pattern matcher.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a pattern.
    pub fn add_pattern(&mut self, pattern: Pattern) {
        self.patterns.push(pattern);
    }

    /// Add a behavior.
    pub fn add_behavior(&mut self, behavior: LearnedBehavior) {
        self.behaviors.push(behavior);
    }

    /// Find matching patterns for input text.
    pub fn find_matches(&self, text: &str) -> Vec<&Pattern> {
        let text_lower = text.to_lowercase();
        self.patterns
            .iter()
            .filter(|p| p.active && text_lower.contains(&p.trigger.to_lowercase()))
            .collect()
    }

    /// Apply patterns to modify text.
    pub fn apply_patterns(&self, text: &str) -> String {
        let mut result = text.to_string();

        for pattern in self.find_matches(text) {
            if pattern.should_apply() {
                result = self.apply_modification(&result, &pattern.modification);
            }
        }

        result
    }

    fn apply_modification(&self, text: &str, modification: &Modification) -> String {
        match modification {
            Modification::Replace { from, to } => text.replace(from, to),
            Modification::Prepend(prefix) => format!("{}{}", prefix, text),
            Modification::Append(suffix) => format!("{}{}", text, suffix),
            Modification::StyleChange(_) => text.to_string(), // Would need more complex handling
            Modification::SystemInstruction(_) => text.to_string(), // Handled at prompt level
        }
    }

    /// Get system prompt additions from active behaviors.
    pub fn get_system_additions(&self) -> Vec<String> {
        self.behaviors
            .iter()
            .filter(|b| b.active && b.confidence >= 0.7)
            .filter_map(|b| b.system_addition.clone())
            .collect()
    }

    /// Get all active patterns.
    pub fn active_patterns(&self) -> Vec<&Pattern> {
        self.patterns.iter().filter(|p| p.active).collect()
    }

    /// Get pattern count.
    pub fn pattern_count(&self) -> usize {
        self.patterns.len()
    }

    /// Get behavior count.
    pub fn behavior_count(&self) -> usize {
        self.behaviors.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_creation() {
        let pattern = Pattern::new(
            PatternType::Terminology,
            "utilize",
            Modification::Replace {
                from: "utilize".to_string(),
                to: "use".to_string(),
            },
        );

        assert_eq!(pattern.pattern_type, PatternType::Terminology);
        assert!(!pattern.should_apply()); // Not enough observations yet
    }

    #[test]
    fn test_pattern_confidence() {
        let mut pattern = Pattern::new(
            PatternType::Style,
            "test",
            Modification::Prepend("Note: ".to_string()),
        );

        // Observe multiple times
        for _ in 0..5 {
            pattern.observe();
        }

        assert!(pattern.confidence > 0.7);
        assert!(pattern.should_apply());
    }

    #[test]
    fn test_pattern_matching() {
        let mut matcher = PatternMatcher::new();

        let mut pattern = Pattern::new(
            PatternType::Terminology,
            "utilize",
            Modification::Replace {
                from: "utilize".to_string(),
                to: "use".to_string(),
            },
        );
        // Make it applicable
        for _ in 0..5 {
            pattern.observe();
        }
        matcher.add_pattern(pattern);

        let result = matcher.apply_patterns("Please utilize this tool");
        assert_eq!(result, "Please use this tool");
    }
}
