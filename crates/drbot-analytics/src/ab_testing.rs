//! A/B testing for model comparisons.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A/B test variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestVariant {
    /// Variant ID.
    pub id: String,
    /// Variant name.
    pub name: String,
    /// Model to use.
    pub model: String,
    /// Weight (for traffic allocation).
    pub weight: f64,
    /// Total samples.
    pub samples: u64,
    /// Successful samples.
    pub successes: u64,
    /// Total latency (for average calculation).
    pub total_latency_ms: u64,
    /// Total tokens used.
    pub total_tokens: u64,
    /// User ratings (if collected).
    pub ratings: Vec<u8>,
}

impl TestVariant {
    /// Create a new variant.
    pub fn new(name: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            model: model.into(),
            weight: 1.0,
            samples: 0,
            successes: 0,
            total_latency_ms: 0,
            total_tokens: 0,
            ratings: Vec::new(),
        }
    }

    /// Set weight.
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = weight;
        self
    }

    /// Record a sample.
    pub fn record_sample(&mut self, success: bool, latency_ms: u64, tokens: u64) {
        self.samples += 1;
        if success {
            self.successes += 1;
        }
        self.total_latency_ms += latency_ms;
        self.total_tokens += tokens;
    }

    /// Record a rating (1-5).
    pub fn record_rating(&mut self, rating: u8) {
        if (1..=5).contains(&rating) {
            self.ratings.push(rating);
        }
    }

    /// Get success rate.
    pub fn success_rate(&self) -> f64 {
        if self.samples == 0 {
            0.0
        } else {
            self.successes as f64 / self.samples as f64
        }
    }

    /// Get average latency.
    pub fn avg_latency_ms(&self) -> f64 {
        if self.samples == 0 {
            0.0
        } else {
            self.total_latency_ms as f64 / self.samples as f64
        }
    }

    /// Get average tokens per request.
    pub fn avg_tokens(&self) -> f64 {
        if self.samples == 0 {
            0.0
        } else {
            self.total_tokens as f64 / self.samples as f64
        }
    }

    /// Get average rating.
    pub fn avg_rating(&self) -> f64 {
        if self.ratings.is_empty() {
            0.0
        } else {
            self.ratings.iter().map(|&r| r as f64).sum::<f64>() / self.ratings.len() as f64
        }
    }
}

/// Test result comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Test ID.
    pub test_id: String,
    /// Test name.
    pub test_name: String,
    /// Winner variant ID (if any).
    pub winner: Option<String>,
    /// Confidence level.
    pub confidence: f64,
    /// Variant results.
    pub variants: Vec<VariantResult>,
    /// Recommendation.
    pub recommendation: String,
}

/// Single variant result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantResult {
    /// Variant ID.
    pub id: String,
    /// Variant name.
    pub name: String,
    /// Success rate.
    pub success_rate: f64,
    /// Average latency.
    pub avg_latency_ms: f64,
    /// Average tokens.
    pub avg_tokens: f64,
    /// Average rating.
    pub avg_rating: f64,
    /// Sample count.
    pub samples: u64,
}

impl From<&TestVariant> for VariantResult {
    fn from(variant: &TestVariant) -> Self {
        Self {
            id: variant.id.clone(),
            name: variant.name.clone(),
            success_rate: variant.success_rate(),
            avg_latency_ms: variant.avg_latency_ms(),
            avg_tokens: variant.avg_tokens(),
            avg_rating: variant.avg_rating(),
            samples: variant.samples,
        }
    }
}

/// An A/B test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ABTest {
    /// Test ID.
    pub id: String,
    /// Test name.
    pub name: String,
    /// Test description.
    pub description: Option<String>,
    /// Test variants.
    pub variants: Vec<TestVariant>,
    /// Test status.
    pub status: TestStatus,
    /// Start time.
    pub started_at: chrono::DateTime<chrono::Utc>,
    /// End time.
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Minimum samples per variant.
    pub min_samples: u64,
    /// Metric to optimize.
    pub primary_metric: PrimaryMetric,
}

/// Test status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestStatus {
    /// Test is running.
    Running,
    /// Test is paused.
    Paused,
    /// Test is complete.
    Completed,
    /// Test was cancelled.
    Cancelled,
}

/// Primary metric to optimize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PrimaryMetric {
    /// Optimize for success rate.
    #[default]
    SuccessRate,
    /// Optimize for latency.
    Latency,
    /// Optimize for token efficiency.
    TokenEfficiency,
    /// Optimize for user rating.
    UserRating,
}

impl ABTest {
    /// Create a new A/B test.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: None,
            variants: Vec::new(),
            status: TestStatus::Running,
            started_at: chrono::Utc::now(),
            ended_at: None,
            min_samples: 100,
            primary_metric: PrimaryMetric::SuccessRate,
        }
    }

    /// Add a variant.
    pub fn add_variant(mut self, variant: TestVariant) -> Self {
        self.variants.push(variant);
        self
    }

    /// Set description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set minimum samples.
    pub fn with_min_samples(mut self, min_samples: u64) -> Self {
        self.min_samples = min_samples;
        self
    }

    /// Set primary metric.
    pub fn with_primary_metric(mut self, metric: PrimaryMetric) -> Self {
        self.primary_metric = metric;
        self
    }

    /// Select a variant based on weights.
    pub fn select_variant(&self) -> Option<&TestVariant> {
        if self.variants.is_empty() || self.status != TestStatus::Running {
            return None;
        }

        let total_weight: f64 = self.variants.iter().map(|v| v.weight).sum();
        let mut random = rand_value() * total_weight;

        for variant in &self.variants {
            random -= variant.weight;
            if random <= 0.0 {
                return Some(variant);
            }
        }

        self.variants.last()
    }

    /// Get a variant by ID.
    pub fn get_variant(&self, id: &str) -> Option<&TestVariant> {
        self.variants.iter().find(|v| v.id == id)
    }

    /// Get a mutable variant by ID.
    pub fn get_variant_mut(&mut self, id: &str) -> Option<&mut TestVariant> {
        self.variants.iter_mut().find(|v| v.id == id)
    }

    /// Check if test has enough samples.
    pub fn has_enough_samples(&self) -> bool {
        self.variants.iter().all(|v| v.samples >= self.min_samples)
    }

    /// Get test results.
    pub fn results(&self) -> TestResult {
        let variant_results: Vec<VariantResult> = self.variants.iter().map(|v| v.into()).collect();

        // Determine winner based on primary metric
        let winner = self.determine_winner();
        let confidence = self.calculate_confidence();

        let recommendation = if let Some(ref w) = winner {
            if confidence >= 0.95 {
                format!("Use variant '{}' with high confidence", w)
            } else if confidence >= 0.8 {
                format!("Variant '{}' is leading, but more data needed", w)
            } else {
                "Continue test - no clear winner yet".to_string()
            }
        } else {
            "No winner determined".to_string()
        };

        TestResult {
            test_id: self.id.clone(),
            test_name: self.name.clone(),
            winner,
            confidence,
            variants: variant_results,
            recommendation,
        }
    }

    fn determine_winner(&self) -> Option<String> {
        if self.variants.is_empty() {
            return None;
        }

        let best = match self.primary_metric {
            PrimaryMetric::SuccessRate => self
                .variants
                .iter()
                .max_by(|a, b| a.success_rate().partial_cmp(&b.success_rate()).unwrap()),
            PrimaryMetric::Latency => self
                .variants
                .iter()
                .min_by(|a, b| a.avg_latency_ms().partial_cmp(&b.avg_latency_ms()).unwrap()),
            PrimaryMetric::TokenEfficiency => self
                .variants
                .iter()
                .min_by(|a, b| a.avg_tokens().partial_cmp(&b.avg_tokens()).unwrap()),
            PrimaryMetric::UserRating => self
                .variants
                .iter()
                .max_by(|a, b| a.avg_rating().partial_cmp(&b.avg_rating()).unwrap()),
        };

        best.map(|v| v.name.clone())
    }

    fn calculate_confidence(&self) -> f64 {
        // Simplified confidence calculation
        if !self.has_enough_samples() {
            return 0.0;
        }

        let min_samples = self.variants.iter().map(|v| v.samples).min().unwrap_or(0);
        let confidence_from_samples = (min_samples as f64 / self.min_samples as f64).min(1.0);

        confidence_from_samples * 0.95 // Cap at 95% confidence
    }

    /// End the test.
    pub fn end(&mut self) {
        self.status = TestStatus::Completed;
        self.ended_at = Some(chrono::Utc::now());
    }
}

/// Simple random value generator.
fn rand_value() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let nanos = duration.subsec_nanos() as f64;
    nanos / 1_000_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_creation() {
        let variant = TestVariant::new("Control", "gpt-4");
        assert_eq!(variant.name, "Control");
        assert_eq!(variant.model, "gpt-4");
    }

    #[test]
    fn test_variant_recording() {
        let mut variant = TestVariant::new("Test", "gpt-4");
        variant.record_sample(true, 100, 50);
        variant.record_sample(false, 200, 75);

        assert_eq!(variant.samples, 2);
        assert_eq!(variant.successes, 1);
        assert_eq!(variant.success_rate(), 0.5);
        assert_eq!(variant.avg_latency_ms(), 150.0);
    }

    #[test]
    fn test_ab_test_creation() {
        let test = ABTest::new("Model Comparison")
            .add_variant(TestVariant::new("Control", "gpt-4"))
            .add_variant(TestVariant::new("Experiment", "gpt-4-turbo"));

        assert_eq!(test.variants.len(), 2);
        assert_eq!(test.status, TestStatus::Running);
    }
}
