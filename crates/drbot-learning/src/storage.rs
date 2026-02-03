//! Storage backend for learning data.

use crate::correction::StoredCorrection;
use crate::feedback::Feedback;
use crate::patterns::{LearnedBehavior, Pattern};
use crate::{LearningError, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for learning storage.
#[async_trait]
pub trait LearningStorage: Send + Sync {
    /// Store a correction.
    async fn store_correction(&self, correction: StoredCorrection) -> Result<()>;

    /// Get corrections.
    async fn get_corrections(&self) -> Result<Vec<StoredCorrection>>;

    /// Store feedback.
    async fn store_feedback(&self, feedback: Feedback) -> Result<()>;

    /// Get feedback.
    async fn get_feedback(&self) -> Result<Vec<Feedback>>;

    /// Store a pattern.
    async fn store_pattern(&self, pattern: Pattern) -> Result<()>;

    /// Get patterns.
    async fn get_patterns(&self) -> Result<Vec<Pattern>>;

    /// Update a pattern.
    async fn update_pattern(&self, pattern: Pattern) -> Result<()>;

    /// Store a behavior.
    async fn store_behavior(&self, behavior: LearnedBehavior) -> Result<()>;

    /// Get behaviors.
    async fn get_behaviors(&self) -> Result<Vec<LearnedBehavior>>;

    /// Clear all data.
    async fn clear(&self) -> Result<()>;
}

/// In-memory learning storage.
#[derive(Debug, Default)]
pub struct MemoryLearningStorage {
    corrections: Arc<RwLock<Vec<StoredCorrection>>>,
    feedback: Arc<RwLock<Vec<Feedback>>>,
    patterns: Arc<RwLock<Vec<Pattern>>>,
    behaviors: Arc<RwLock<Vec<LearnedBehavior>>>,
}

impl MemoryLearningStorage {
    /// Create a new memory storage.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl LearningStorage for MemoryLearningStorage {
    async fn store_correction(&self, correction: StoredCorrection) -> Result<()> {
        let mut corrections = self.corrections.write().await;
        corrections.push(correction);
        Ok(())
    }

    async fn get_corrections(&self) -> Result<Vec<StoredCorrection>> {
        let corrections = self.corrections.read().await;
        Ok(corrections.clone())
    }

    async fn store_feedback(&self, feedback: Feedback) -> Result<()> {
        let mut fb = self.feedback.write().await;
        fb.push(feedback);
        Ok(())
    }

    async fn get_feedback(&self) -> Result<Vec<Feedback>> {
        let feedback = self.feedback.read().await;
        Ok(feedback.clone())
    }

    async fn store_pattern(&self, pattern: Pattern) -> Result<()> {
        let mut patterns = self.patterns.write().await;
        patterns.push(pattern);
        Ok(())
    }

    async fn get_patterns(&self) -> Result<Vec<Pattern>> {
        let patterns = self.patterns.read().await;
        Ok(patterns.clone())
    }

    async fn update_pattern(&self, updated: Pattern) -> Result<()> {
        let mut patterns = self.patterns.write().await;
        if let Some(pattern) = patterns.iter_mut().find(|p| p.id == updated.id) {
            *pattern = updated;
        }
        Ok(())
    }

    async fn store_behavior(&self, behavior: LearnedBehavior) -> Result<()> {
        let mut behaviors = self.behaviors.write().await;
        behaviors.push(behavior);
        Ok(())
    }

    async fn get_behaviors(&self) -> Result<Vec<LearnedBehavior>> {
        let behaviors = self.behaviors.read().await;
        Ok(behaviors.clone())
    }

    async fn clear(&self) -> Result<()> {
        self.corrections.write().await.clear();
        self.feedback.write().await.clear();
        self.patterns.write().await.clear();
        self.behaviors.write().await.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::correction::Correction;

    #[tokio::test]
    async fn test_memory_storage_corrections() {
        let storage = MemoryLearningStorage::new();

        let correction = StoredCorrection::from_correction(Correction::new("wrong", "right"));
        storage.store_correction(correction).await.unwrap();

        let corrections = storage.get_corrections().await.unwrap();
        assert_eq!(corrections.len(), 1);
    }

    #[tokio::test]
    async fn test_memory_storage_clear() {
        let storage = MemoryLearningStorage::new();

        storage
            .store_correction(StoredCorrection::from_correction(Correction::new("a", "b")))
            .await
            .unwrap();

        storage.clear().await.unwrap();

        let corrections = storage.get_corrections().await.unwrap();
        assert!(corrections.is_empty());
    }
}
