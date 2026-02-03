//! Correction types and handling.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type of correction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorrectionType {
    /// Factual error correction.
    Factual,
    /// Tone/style preference.
    Style,
    /// Format preference.
    Format,
    /// Terminology preference.
    Terminology,
    /// Behavior preference.
    Behavior,
    /// Other correction.
    Other,
}

impl Default for CorrectionType {
    fn default() -> Self {
        Self::Other
    }
}

/// A user correction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    /// Original (incorrect) response.
    pub original: String,
    /// Corrected response.
    pub corrected: String,
    /// Context (user message that prompted the response).
    pub context: Option<String>,
    /// Type of correction.
    pub correction_type: CorrectionType,
    /// When the correction was made.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Correction {
    /// Create a new correction.
    pub fn new(original: impl Into<String>, corrected: impl Into<String>) -> Self {
        Self {
            original: original.into(),
            corrected: corrected.into(),
            context: None,
            correction_type: CorrectionType::default(),
            timestamp: chrono::Utc::now(),
        }
    }

    /// Set context.
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Set correction type.
    pub fn with_type(mut self, correction_type: CorrectionType) -> Self {
        self.correction_type = correction_type;
        self
    }

    /// Calculate similarity between original and corrected.
    pub fn similarity_score(&self) -> f32 {
        // Simple character-level similarity
        let original_chars: Vec<char> = self.original.chars().collect();
        let corrected_chars: Vec<char> = self.corrected.chars().collect();

        if original_chars.is_empty() && corrected_chars.is_empty() {
            return 1.0;
        }

        let max_len = original_chars.len().max(corrected_chars.len());
        if max_len == 0 {
            return 1.0;
        }

        let mut matches = 0;
        for (o, c) in original_chars.iter().zip(corrected_chars.iter()) {
            if o == c {
                matches += 1;
            }
        }

        matches as f32 / max_len as f32
    }

    /// Extract the key difference between original and corrected.
    pub fn extract_difference(&self) -> Option<(String, String)> {
        let original_words: Vec<&str> = self.original.split_whitespace().collect();
        let corrected_words: Vec<&str> = self.corrected.split_whitespace().collect();

        // Find first difference
        for (i, (o, c)) in original_words
            .iter()
            .zip(corrected_words.iter())
            .enumerate()
        {
            if o != c {
                return Some((o.to_string(), c.to_string()));
            }
        }

        None
    }
}

/// Stored correction with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCorrection {
    /// Correction ID.
    pub id: String,
    /// The correction.
    pub correction: Correction,
    /// Times this correction was applied.
    pub applied_count: u32,
    /// Whether this correction is active.
    pub active: bool,
    /// Confidence in this correction (0.0 - 1.0).
    pub confidence: f32,
}

impl StoredCorrection {
    /// Create from a correction.
    pub fn from_correction(correction: Correction) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            correction,
            applied_count: 0,
            active: true,
            confidence: 0.5,
        }
    }

    /// Record that this correction was applied.
    pub fn record_application(&mut self) {
        self.applied_count += 1;
        // Increase confidence when applied
        self.confidence = (self.confidence + 0.1).min(1.0);
    }

    /// Deactivate this correction.
    pub fn deactivate(&mut self) {
        self.active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correction_creation() {
        let correction = Correction::new("London", "Paris")
            .with_context("What is the capital of France?")
            .with_type(CorrectionType::Factual);

        assert_eq!(correction.original, "London");
        assert_eq!(correction.corrected, "Paris");
        assert_eq!(correction.correction_type, CorrectionType::Factual);
    }

    #[test]
    fn test_similarity_score() {
        let correction = Correction::new("hello world", "hello there");
        let score = correction.similarity_score();
        assert!(score > 0.0 && score < 1.0);

        let identical = Correction::new("same", "same");
        assert_eq!(identical.similarity_score(), 1.0);
    }

    #[test]
    fn test_extract_difference() {
        let correction = Correction::new("The capital is London", "The capital is Paris");
        let diff = correction.extract_difference();
        assert_eq!(diff, Some(("London".to_string(), "Paris".to_string())));
    }
}
