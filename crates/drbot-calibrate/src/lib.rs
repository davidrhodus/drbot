//! Response calibration for drbot.
//!
//! Calibrate response confidence and quality.
//!
//! # Features
//!
//! - Confidence scoring
//! - Response quality assessment
//! - Calibration history tracking
//! - Threshold management

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Calibration result type.
pub type Result<T> = std::result::Result<T, CalibrationError>;

/// Calibration errors.
#[derive(Debug, thiserror::Error)]
pub enum CalibrationError {
    #[error("Calibration failed: {0}")]
    Failed(String),
    #[error("Invalid threshold: {0}")]
    InvalidThreshold(f32),
    #[error("No calibration data")]
    NoData,
}

/// Calibrated response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibratedResponse {
    /// Response ID.
    pub id: Uuid,
    /// Original response.
    pub response: String,
    /// Overall confidence score (0-1).
    pub confidence: f32,
    /// Quality score (0-1).
    pub quality: f32,
    /// Individual dimension scores.
    pub dimensions: CalibrationDimensions,
    /// Calibration warnings.
    pub warnings: Vec<CalibrationWarning>,
    /// Suggested improvements.
    pub suggestions: Vec<String>,
    /// Calibrated at.
    pub calibrated_at: DateTime<Utc>,
}

/// Calibration dimensions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CalibrationDimensions {
    /// Factual accuracy confidence.
    pub accuracy: f32,
    /// Completeness.
    pub completeness: f32,
    /// Clarity.
    pub clarity: f32,
    /// Relevance to query.
    pub relevance: f32,
    /// Consistency.
    pub consistency: f32,
    /// Helpfulness.
    pub helpfulness: f32,
    /// Safety.
    pub safety: f32,
}

impl CalibrationDimensions {
    /// Calculate overall score.
    pub fn overall(&self) -> f32 {
        let scores = [
            self.accuracy,
            self.completeness,
            self.clarity,
            self.relevance,
            self.consistency,
            self.helpfulness,
            self.safety,
        ];

        scores.iter().sum::<f32>() / scores.len() as f32
    }

    /// Get weakest dimension.
    pub fn weakest(&self) -> (&str, f32) {
        let dims = [
            ("accuracy", self.accuracy),
            ("completeness", self.completeness),
            ("clarity", self.clarity),
            ("relevance", self.relevance),
            ("consistency", self.consistency),
            ("helpfulness", self.helpfulness),
            ("safety", self.safety),
        ];

        dims.iter()
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .copied()
            .unwrap_or(("unknown", 0.0))
    }
}

/// Calibration warning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationWarning {
    /// Warning type.
    pub warning_type: WarningType,
    /// Severity (0-1).
    pub severity: f32,
    /// Description.
    pub message: String,
    /// Location in response.
    pub location: Option<(usize, usize)>,
}

/// Warning types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningType {
    LowConfidence,
    Uncertainty,
    PotentialError,
    IncompleteInfo,
    AmbiguousStatement,
    MissingCitation,
    Speculation,
    OutOfScope,
}

/// Calibration configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationConfig {
    /// Enable calibration.
    pub enabled: bool,
    /// Minimum acceptable confidence.
    pub min_confidence: f32,
    /// Minimum acceptable quality.
    pub min_quality: f32,
    /// Dimension weights.
    pub weights: HashMap<String, f32>,
    /// Warning thresholds.
    pub warning_thresholds: HashMap<String, f32>,
}

impl Default for CalibrationConfig {
    fn default() -> Self {
        let mut weights = HashMap::new();
        weights.insert("accuracy".to_string(), 1.0);
        weights.insert("completeness".to_string(), 0.8);
        weights.insert("clarity".to_string(), 0.7);
        weights.insert("relevance".to_string(), 0.9);
        weights.insert("consistency".to_string(), 0.8);
        weights.insert("helpfulness".to_string(), 0.7);
        weights.insert("safety".to_string(), 1.0);

        let mut warning_thresholds = HashMap::new();
        warning_thresholds.insert("low_confidence".to_string(), 0.6);
        warning_thresholds.insert("uncertainty".to_string(), 0.5);

        Self {
            enabled: true,
            min_confidence: 0.7,
            min_quality: 0.6,
            weights,
            warning_thresholds,
        }
    }
}

/// Calibration context.
#[derive(Debug, Clone, Default)]
pub struct CalibrationContext {
    /// Original query.
    pub query: String,
    /// Previous responses.
    pub history: Vec<String>,
    /// Known facts.
    pub facts: Vec<String>,
    /// Domain.
    pub domain: Option<String>,
    /// Custom metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Trait for calibrators.
#[async_trait]
pub trait Calibrator: Send + Sync {
    /// Calibrate a response.
    async fn calibrate(
        &self,
        response: &str,
        context: &CalibrationContext,
    ) -> Result<CalibratedResponse>;
}

/// Calibration history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationEntry {
    /// Entry ID.
    pub id: Uuid,
    /// Response ID.
    pub response_id: Uuid,
    /// Query.
    pub query: String,
    /// Predicted confidence.
    pub predicted_confidence: f32,
    /// Actual correctness (if known).
    pub actual_correct: Option<bool>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Calibration statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CalibrationStats {
    /// Total calibrations.
    pub total: u64,
    /// Average confidence.
    pub avg_confidence: f32,
    /// Average quality.
    pub avg_quality: f32,
    /// Calibration accuracy (predicted vs actual).
    pub calibration_accuracy: f32,
    /// Confidence by dimension.
    pub by_dimension: HashMap<String, f32>,
    /// Warning counts.
    pub warning_counts: HashMap<WarningType, u64>,
}

/// Response calibrator engine.
pub struct CalibrationEngine<C: Calibrator> {
    config: CalibrationConfig,
    calibrator: C,
    history: Arc<RwLock<Vec<CalibrationEntry>>>,
    stats: Arc<RwLock<CalibrationStats>>,
}

impl<C: Calibrator> CalibrationEngine<C> {
    /// Create a new calibration engine.
    pub fn new(config: CalibrationConfig, calibrator: C) -> Self {
        Self {
            config,
            calibrator,
            history: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(CalibrationStats::default())),
        }
    }

    /// Calibrate a response.
    pub async fn calibrate(
        &self,
        response: &str,
        context: &CalibrationContext,
    ) -> Result<CalibratedResponse> {
        if !self.config.enabled {
            return Ok(CalibratedResponse {
                id: Uuid::new_v4(),
                response: response.to_string(),
                confidence: 1.0,
                quality: 1.0,
                dimensions: CalibrationDimensions::default(),
                warnings: Vec::new(),
                suggestions: Vec::new(),
                calibrated_at: Utc::now(),
            });
        }

        let result = self.calibrator.calibrate(response, context).await?;

        // Record history
        self.history.write().await.push(CalibrationEntry {
            id: Uuid::new_v4(),
            response_id: result.id,
            query: context.query.clone(),
            predicted_confidence: result.confidence,
            actual_correct: None,
            timestamp: Utc::now(),
        });

        // Update stats
        self.update_stats(&result).await;

        Ok(result)
    }

    async fn update_stats(&self, result: &CalibratedResponse) {
        let mut stats = self.stats.write().await;

        stats.total += 1;

        // Update averages
        let n = stats.total as f32;
        stats.avg_confidence = (stats.avg_confidence * (n - 1.0) + result.confidence) / n;
        stats.avg_quality = (stats.avg_quality * (n - 1.0) + result.quality) / n;

        // Update dimension averages
        let dims = [
            ("accuracy", result.dimensions.accuracy),
            ("completeness", result.dimensions.completeness),
            ("clarity", result.dimensions.clarity),
            ("relevance", result.dimensions.relevance),
            ("consistency", result.dimensions.consistency),
            ("helpfulness", result.dimensions.helpfulness),
            ("safety", result.dimensions.safety),
        ];

        for (name, score) in dims {
            let current = stats.by_dimension.get(name).copied().unwrap_or(0.0);
            let updated = (current * (n - 1.0) + score) / n;
            stats.by_dimension.insert(name.to_string(), updated);
        }

        // Update warning counts
        for warning in &result.warnings {
            *stats
                .warning_counts
                .entry(warning.warning_type)
                .or_insert(0) += 1;
        }
    }

    /// Record actual correctness for calibration.
    pub async fn record_feedback(&self, response_id: Uuid, correct: bool) {
        let mut history = self.history.write().await;
        if let Some(entry) = history.iter_mut().find(|e| e.response_id == response_id) {
            entry.actual_correct = Some(correct);
        }

        // Recalculate calibration accuracy
        let with_feedback: Vec<_> = history
            .iter()
            .filter(|e| e.actual_correct.is_some())
            .collect();
        if !with_feedback.is_empty() {
            let correct_count: usize = with_feedback
                .iter()
                .filter(|e| {
                    let predicted_correct = e.predicted_confidence >= self.config.min_confidence;
                    let actual = e.actual_correct.unwrap_or(false);
                    predicted_correct == actual
                })
                .count();

            let mut stats = self.stats.write().await;
            stats.calibration_accuracy = correct_count as f32 / with_feedback.len() as f32;
        }
    }

    /// Check if response meets thresholds.
    pub fn meets_thresholds(&self, result: &CalibratedResponse) -> bool {
        result.confidence >= self.config.min_confidence && result.quality >= self.config.min_quality
    }

    /// Get calibration statistics.
    pub async fn stats(&self) -> CalibrationStats {
        self.stats.read().await.clone()
    }

    /// Get calibration history.
    pub async fn history(&self, limit: usize) -> Vec<CalibrationEntry> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }
}

/// Simple calibrator for testing.
pub struct SimpleCalibrator;

#[async_trait]
impl Calibrator for SimpleCalibrator {
    async fn calibrate(
        &self,
        response: &str,
        context: &CalibrationContext,
    ) -> Result<CalibratedResponse> {
        let mut warnings = Vec::new();
        let mut suggestions = Vec::new();

        // Check for uncertainty markers
        let uncertainty_phrases = ["I think", "maybe", "possibly", "not sure", "might be"];
        let uncertainty_count = uncertainty_phrases
            .iter()
            .filter(|p| response.to_lowercase().contains(*p))
            .count();

        if uncertainty_count > 0 {
            warnings.push(CalibrationWarning {
                warning_type: WarningType::Uncertainty,
                severity: (uncertainty_count as f32 * 0.2).min(1.0),
                message: format!(
                    "Response contains {} uncertainty markers",
                    uncertainty_count
                ),
                location: None,
            });
        }

        // Check response length
        let word_count = response.split_whitespace().count();
        let query_word_count = context.query.split_whitespace().count();

        let completeness = if query_word_count > 0 && word_count < query_word_count {
            suggestions.push("Consider providing a more detailed response".to_string());
            0.5
        } else if word_count > 500 {
            0.9
        } else {
            0.7
        };

        // Calculate dimensions
        let dimensions = CalibrationDimensions {
            accuracy: if warnings.is_empty() { 0.8 } else { 0.6 },
            completeness,
            clarity: if response.contains('?') && !context.query.contains('?') {
                0.7
            } else {
                0.85
            },
            relevance: 0.8,
            consistency: 0.9,
            helpfulness: 0.75,
            safety: 1.0,
        };

        let confidence = dimensions.overall() - (uncertainty_count as f32 * 0.1);
        let quality = dimensions.overall();

        Ok(CalibratedResponse {
            id: Uuid::new_v4(),
            response: response.to_string(),
            confidence: confidence.clamp(0.0, 1.0),
            quality,
            dimensions,
            warnings,
            suggestions,
            calibrated_at: Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_calibration() {
        let engine = CalibrationEngine::new(CalibrationConfig::default(), SimpleCalibrator);

        let context = CalibrationContext {
            query: "What is Rust?".to_string(),
            ..Default::default()
        };

        let result = engine
            .calibrate(
                "Rust is a systems programming language focused on safety, speed, and concurrency.",
                &context,
            )
            .await
            .unwrap();

        assert!(result.confidence > 0.5);
        assert!(result.quality > 0.5);
    }

    #[tokio::test]
    async fn test_uncertainty_detection() {
        let engine = CalibrationEngine::new(CalibrationConfig::default(), SimpleCalibrator);

        let context = CalibrationContext {
            query: "What is X?".to_string(),
            ..Default::default()
        };

        let result = engine
            .calibrate(
                "I think X might be something, but I'm not sure, possibly related to Y.",
                &context,
            )
            .await
            .unwrap();

        assert!(!result.warnings.is_empty());
        assert!(result.confidence < 0.8);
    }

    #[test]
    fn test_dimensions() {
        let dims = CalibrationDimensions {
            accuracy: 0.8,
            completeness: 0.7,
            clarity: 0.9,
            relevance: 0.85,
            consistency: 0.8,
            helpfulness: 0.75,
            safety: 1.0,
        };

        let overall = dims.overall();
        assert!(overall > 0.7 && overall < 0.9);

        let (name, score) = dims.weakest();
        assert_eq!(name, "completeness");
        assert_eq!(score, 0.7);
    }

    #[tokio::test]
    async fn test_feedback() {
        let engine = CalibrationEngine::new(CalibrationConfig::default(), SimpleCalibrator);

        let context = CalibrationContext::default();
        let result = engine.calibrate("Test response", &context).await.unwrap();

        engine.record_feedback(result.id, true).await;

        let history = engine.history(10).await;
        assert!(!history.is_empty());
        assert_eq!(history[0].actual_correct, Some(true));
    }
}
