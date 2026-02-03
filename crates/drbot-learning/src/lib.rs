//! Learning from corrections and user feedback for drbot.
//!
//! Tracks user corrections and adapts behavior over time.
//!
//! # Features
//!
//! - Track corrections and feedback
//! - Learn patterns from user preferences
//! - Adapt responses based on history
//! - Export learning data for fine-tuning
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_learning::{LearningSystem, Correction, Feedback};
//!
//! async fn example() {
//!     let learning = LearningSystem::new().await.unwrap();
//!
//!     // Record a correction
//!     learning.record_correction(
//!         Correction::new("The capital of France is London", "The capital of France is Paris")
//!     ).await.unwrap();
//!
//!     // Apply learning to a response
//!     let improved = learning.apply_learning("What is the capital of France?").await;
//! }
//! ```

mod correction;
mod feedback;
mod learning;
mod patterns;
mod storage;

pub use correction::{Correction, CorrectionType};
pub use feedback::{Feedback, FeedbackType, Rating};
pub use learning::{LearningConfig, LearningSystem};
pub use patterns::{LearnedBehavior, Pattern, PatternMatcher};
pub use storage::{LearningStorage, MemoryLearningStorage};

/// Result type.
pub type Result<T> = std::result::Result<T, LearningError>;

/// Learning errors.
#[derive(Debug, thiserror::Error)]
pub enum LearningError {
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Pattern not found: {0}")]
    PatternNotFound(String),
    #[error("Learning disabled")]
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_learning_basic() {
        let learning = LearningSystem::new().await.unwrap();

        learning
            .record_correction(Correction {
                original: "wrong".to_string(),
                corrected: "right".to_string(),
                context: None,
                correction_type: CorrectionType::Factual,
                timestamp: chrono::Utc::now(),
            })
            .await
            .unwrap();

        let stats = learning.stats().await;
        assert_eq!(stats.total_corrections, 1);
    }
}
