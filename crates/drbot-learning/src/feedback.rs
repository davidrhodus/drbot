//! User feedback handling.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Feedback type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackType {
    /// Thumbs up/down rating.
    ThumbsUpDown,
    /// Star rating (1-5).
    StarRating,
    /// Text feedback.
    TextFeedback,
    /// Regeneration request.
    Regenerate,
    /// Copy action (implicit positive feedback).
    Copy,
    /// Edit action (implicit feedback for correction).
    Edit,
}

/// Rating value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rating {
    /// Positive feedback.
    Positive,
    /// Negative feedback.
    Negative,
    /// Neutral (no strong opinion).
    Neutral,
    /// Star rating (1-5).
    Stars(u8),
}

impl Rating {
    /// Convert to numeric value (-1 to 1).
    pub fn to_numeric(&self) -> f32 {
        match self {
            Rating::Positive => 1.0,
            Rating::Negative => -1.0,
            Rating::Neutral => 0.0,
            Rating::Stars(s) => (*s as f32 - 3.0) / 2.0, // Maps 1-5 to -1..1
        }
    }

    /// Check if positive.
    pub fn is_positive(&self) -> bool {
        self.to_numeric() > 0.0
    }

    /// Check if negative.
    pub fn is_negative(&self) -> bool {
        self.to_numeric() < 0.0
    }
}

/// User feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    /// Feedback ID.
    pub id: String,
    /// Feedback type.
    pub feedback_type: FeedbackType,
    /// Rating.
    pub rating: Rating,
    /// Message ID this feedback relates to.
    pub message_id: Option<String>,
    /// Session ID.
    pub session_id: Option<String>,
    /// Model that generated the response.
    pub model: Option<String>,
    /// Text comment (if any).
    pub comment: Option<String>,
    /// The response that was rated.
    pub response_text: Option<String>,
    /// The prompt that generated the response.
    pub prompt_text: Option<String>,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Feedback {
    /// Create new feedback.
    pub fn new(feedback_type: FeedbackType, rating: Rating) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            feedback_type,
            rating,
            message_id: None,
            session_id: None,
            model: None,
            comment: None,
            response_text: None,
            prompt_text: None,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create thumbs up feedback.
    pub fn thumbs_up() -> Self {
        Self::new(FeedbackType::ThumbsUpDown, Rating::Positive)
    }

    /// Create thumbs down feedback.
    pub fn thumbs_down() -> Self {
        Self::new(FeedbackType::ThumbsUpDown, Rating::Negative)
    }

    /// Create star rating feedback.
    pub fn stars(rating: u8) -> Self {
        let rating = rating.clamp(1, 5);
        Self::new(FeedbackType::StarRating, Rating::Stars(rating))
    }

    /// Create regeneration feedback (implicit negative).
    pub fn regenerate() -> Self {
        Self::new(FeedbackType::Regenerate, Rating::Negative)
    }

    /// Create copy feedback (implicit positive).
    pub fn copy() -> Self {
        Self::new(FeedbackType::Copy, Rating::Positive)
    }

    /// Set message ID.
    pub fn for_message(mut self, message_id: impl Into<String>) -> Self {
        self.message_id = Some(message_id.into());
        self
    }

    /// Set session ID.
    pub fn in_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set model.
    pub fn for_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Add a comment.
    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }

    /// Set response text.
    pub fn with_response(mut self, response: impl Into<String>) -> Self {
        self.response_text = Some(response.into());
        self
    }

    /// Set prompt text.
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt_text = Some(prompt.into());
        self
    }
}

/// Aggregated feedback statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedbackStats {
    /// Total feedback count.
    pub total: u64,
    /// Positive count.
    pub positive: u64,
    /// Negative count.
    pub negative: u64,
    /// Neutral count.
    pub neutral: u64,
    /// Average rating (-1 to 1).
    pub average_rating: f32,
    /// By model.
    pub by_model: std::collections::HashMap<String, ModelFeedback>,
}

/// Feedback for a specific model.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelFeedback {
    /// Total count.
    pub total: u64,
    /// Positive count.
    pub positive: u64,
    /// Negative count.
    pub negative: u64,
    /// Average rating.
    pub average_rating: f32,
}

impl FeedbackStats {
    /// Add feedback to stats.
    pub fn add(&mut self, feedback: &Feedback) {
        self.total += 1;

        if feedback.rating.is_positive() {
            self.positive += 1;
        } else if feedback.rating.is_negative() {
            self.negative += 1;
        } else {
            self.neutral += 1;
        }

        // Update average
        let numeric = feedback.rating.to_numeric();
        self.average_rating =
            (self.average_rating * (self.total - 1) as f32 + numeric) / self.total as f32;

        // Update model stats
        if let Some(model) = &feedback.model {
            let model_stats = self.by_model.entry(model.clone()).or_default();
            model_stats.total += 1;
            if feedback.rating.is_positive() {
                model_stats.positive += 1;
            } else if feedback.rating.is_negative() {
                model_stats.negative += 1;
            }
            model_stats.average_rating =
                (model_stats.average_rating * (model_stats.total - 1) as f32 + numeric)
                    / model_stats.total as f32;
        }
    }

    /// Get satisfaction rate (0.0 to 1.0).
    pub fn satisfaction_rate(&self) -> f32 {
        if self.total == 0 {
            return 0.5;
        }
        (self.positive as f32 + self.neutral as f32 * 0.5) / self.total as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rating_numeric() {
        assert_eq!(Rating::Positive.to_numeric(), 1.0);
        assert_eq!(Rating::Negative.to_numeric(), -1.0);
        assert_eq!(Rating::Neutral.to_numeric(), 0.0);
        assert_eq!(Rating::Stars(5).to_numeric(), 1.0);
        assert_eq!(Rating::Stars(1).to_numeric(), -1.0);
    }

    #[test]
    fn test_feedback_creation() {
        let feedback = Feedback::thumbs_up()
            .for_model("gpt-4")
            .with_comment("Great response!");

        assert_eq!(feedback.rating, Rating::Positive);
        assert_eq!(feedback.model, Some("gpt-4".to_string()));
    }

    #[test]
    fn test_feedback_stats() {
        let mut stats = FeedbackStats::default();

        stats.add(&Feedback::thumbs_up().for_model("gpt-4"));
        stats.add(&Feedback::thumbs_down().for_model("gpt-4"));
        stats.add(&Feedback::thumbs_up().for_model("gpt-4"));

        assert_eq!(stats.total, 3);
        assert_eq!(stats.positive, 2);
        assert_eq!(stats.negative, 1);
    }
}
