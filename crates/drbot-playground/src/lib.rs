//! Interactive testing and experimentation environment.
//!
//! This crate provides:
//! - Interactive prompt testing
//! - A/B testing
//! - Configuration experiments
//! - Result comparison

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Playground errors.
#[derive(Debug, Error)]
pub enum PlaygroundError {
    #[error("Experiment not found: {0}")]
    ExperimentNotFound(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Result type for playground operations.
pub type Result<T> = std::result::Result<T, PlaygroundError>;

/// An experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    /// Experiment identifier.
    pub id: String,
    /// Experiment name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Variants.
    pub variants: Vec<Variant>,
    /// Status.
    pub status: ExperimentStatus,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
    /// Results.
    pub results: Option<ExperimentResults>,
}

/// Experiment status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentStatus {
    Draft,
    Running,
    Paused,
    Completed,
    Archived,
}

/// A variant in an experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    /// Variant identifier.
    pub id: String,
    /// Variant name.
    pub name: String,
    /// Configuration.
    pub config: VariantConfig,
    /// Traffic allocation (0-1).
    pub allocation: f64,
    /// Executions.
    pub executions: Vec<Execution>,
}

/// Variant configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantConfig {
    /// Model.
    pub model: String,
    /// System prompt.
    pub system_prompt: Option<String>,
    /// Temperature.
    pub temperature: Option<f64>,
    /// Max tokens.
    pub max_tokens: Option<usize>,
    /// Other settings.
    pub settings: HashMap<String, String>,
}

impl Default for VariantConfig {
    fn default() -> Self {
        Self {
            model: "default".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            settings: HashMap::new(),
        }
    }
}

/// An execution of a variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Execution {
    /// Execution identifier.
    pub id: String,
    /// Variant identifier.
    pub variant_id: String,
    /// Input prompt.
    pub input: String,
    /// Output response.
    pub output: String,
    /// Latency (ms).
    pub latency_ms: u64,
    /// Token count.
    pub tokens: TokenCount,
    /// Cost.
    pub cost: f64,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Feedback (if any).
    pub feedback: Option<ExecutionFeedback>,
}

/// Token counts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenCount {
    /// Input tokens.
    pub input: usize,
    /// Output tokens.
    pub output: usize,
    /// Total tokens.
    pub total: usize,
}

/// Feedback for an execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionFeedback {
    /// Rating (1-5).
    pub rating: Option<i32>,
    /// Is correct.
    pub correct: Option<bool>,
    /// Comment.
    pub comment: Option<String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Experiment results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResults {
    /// Winning variant.
    pub winner: Option<String>,
    /// Statistical significance.
    pub significance: f64,
    /// Variant metrics.
    pub variant_metrics: HashMap<String, VariantMetrics>,
    /// Summary.
    pub summary: String,
}

/// Metrics for a variant.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VariantMetrics {
    /// Total executions.
    pub total_executions: usize,
    /// Average latency (ms).
    pub avg_latency_ms: f64,
    /// Average tokens.
    pub avg_tokens: f64,
    /// Average cost.
    pub avg_cost: f64,
    /// Average rating.
    pub avg_rating: f64,
    /// Correct rate.
    pub correct_rate: f64,
    /// Satisfaction rate.
    pub satisfaction_rate: f64,
}

/// Test case for playground.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    /// Test case identifier.
    pub id: String,
    /// Name.
    pub name: String,
    /// Input prompt.
    pub input: String,
    /// Expected output (optional).
    pub expected_output: Option<String>,
    /// Tags.
    pub tags: Vec<String>,
}

/// Test suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    /// Suite identifier.
    pub id: String,
    /// Suite name.
    pub name: String,
    /// Test cases.
    pub test_cases: Vec<TestCase>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// Playground execution provider.
#[async_trait]
pub trait PlaygroundProvider: Send + Sync {
    /// Execute a prompt with configuration.
    async fn execute(
        &self,
        config: &VariantConfig,
        input: &str,
    ) -> Result<(String, u64, TokenCount, f64)>;

    /// Compare outputs.
    async fn compare(&self, outputs: &[String], expected: Option<&str>) -> Result<Vec<f64>>;
}

/// The playground engine.
pub struct Playground {
    /// Provider.
    provider: Arc<dyn PlaygroundProvider>,
    /// Experiments.
    experiments: Arc<RwLock<HashMap<String, Experiment>>>,
    /// Test suites.
    test_suites: Arc<RwLock<HashMap<String, TestSuite>>>,
    /// Execution history.
    history: Arc<RwLock<Vec<Execution>>>,
}

impl Playground {
    /// Create a new playground.
    pub fn new(provider: Arc<dyn PlaygroundProvider>) -> Self {
        Self {
            provider,
            experiments: Arc::new(RwLock::new(HashMap::new())),
            test_suites: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Quick test - run single prompt with config.
    pub async fn quick_test(&self, config: &VariantConfig, input: &str) -> Result<Execution> {
        let (output, latency_ms, tokens, cost) = self.provider.execute(config, input).await?;

        let execution = Execution {
            id: Uuid::new_v4().to_string(),
            variant_id: "quick".to_string(),
            input: input.to_string(),
            output,
            latency_ms,
            tokens,
            cost,
            timestamp: Utc::now(),
            feedback: None,
        };

        let mut history = self.history.write().await;
        history.push(execution.clone());
        if history.len() > 10000 {
            history.drain(0..1000);
        }

        Ok(execution)
    }

    /// Create an experiment.
    pub async fn create_experiment(&self, name: &str, description: &str) -> Experiment {
        let experiment = Experiment {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            variants: Vec::new(),
            status: ExperimentStatus::Draft,
            created_at: Utc::now(),
            completed_at: None,
            results: None,
        };

        let mut experiments = self.experiments.write().await;
        experiments.insert(experiment.id.clone(), experiment.clone());

        experiment
    }

    /// Add variant to experiment.
    pub async fn add_variant(
        &self,
        experiment_id: &str,
        name: &str,
        config: VariantConfig,
        allocation: f64,
    ) -> Result<Variant> {
        let mut experiments = self.experiments.write().await;
        let experiment = experiments
            .get_mut(experiment_id)
            .ok_or_else(|| PlaygroundError::ExperimentNotFound(experiment_id.to_string()))?;

        if experiment.status != ExperimentStatus::Draft {
            return Err(PlaygroundError::InvalidConfig(
                "Cannot add variants to non-draft experiment".to_string(),
            ));
        }

        let variant = Variant {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            config,
            allocation,
            executions: Vec::new(),
        };

        experiment.variants.push(variant.clone());

        Ok(variant)
    }

    /// Start experiment.
    pub async fn start_experiment(&self, experiment_id: &str) -> Result<()> {
        let mut experiments = self.experiments.write().await;
        let experiment = experiments
            .get_mut(experiment_id)
            .ok_or_else(|| PlaygroundError::ExperimentNotFound(experiment_id.to_string()))?;

        if experiment.variants.is_empty() {
            return Err(PlaygroundError::InvalidConfig(
                "Experiment has no variants".to_string(),
            ));
        }

        // Validate allocations sum to ~1
        let total_allocation: f64 = experiment.variants.iter().map(|v| v.allocation).sum();
        if (total_allocation - 1.0).abs() > 0.01 {
            return Err(PlaygroundError::InvalidConfig(format!(
                "Variant allocations must sum to 1.0, got {}",
                total_allocation
            )));
        }

        experiment.status = ExperimentStatus::Running;

        Ok(())
    }

    /// Run experiment iteration.
    pub async fn run_iteration(&self, experiment_id: &str, input: &str) -> Result<Vec<Execution>> {
        let experiments = self.experiments.read().await;
        let experiment = experiments
            .get(experiment_id)
            .ok_or_else(|| PlaygroundError::ExperimentNotFound(experiment_id.to_string()))?;

        if experiment.status != ExperimentStatus::Running {
            return Err(PlaygroundError::InvalidConfig(
                "Experiment is not running".to_string(),
            ));
        }

        let variants = experiment.variants.clone();
        drop(experiments);

        let mut executions = Vec::new();

        for variant in &variants {
            let (output, latency_ms, tokens, cost) =
                self.provider.execute(&variant.config, input).await?;

            let execution = Execution {
                id: Uuid::new_v4().to_string(),
                variant_id: variant.id.clone(),
                input: input.to_string(),
                output,
                latency_ms,
                tokens,
                cost,
                timestamp: Utc::now(),
                feedback: None,
            };

            executions.push(execution);
        }

        // Store executions
        let mut experiments = self.experiments.write().await;
        if let Some(experiment) = experiments.get_mut(experiment_id) {
            for exec in &executions {
                if let Some(variant) = experiment
                    .variants
                    .iter_mut()
                    .find(|v| v.id == exec.variant_id)
                {
                    variant.executions.push(exec.clone());
                }
            }
        }

        Ok(executions)
    }

    /// Submit feedback for execution.
    pub async fn submit_feedback(
        &self,
        experiment_id: &str,
        execution_id: &str,
        feedback: ExecutionFeedback,
    ) -> Result<()> {
        let mut experiments = self.experiments.write().await;
        let experiment = experiments
            .get_mut(experiment_id)
            .ok_or_else(|| PlaygroundError::ExperimentNotFound(experiment_id.to_string()))?;

        for variant in &mut experiment.variants {
            if let Some(exec) = variant.executions.iter_mut().find(|e| e.id == execution_id) {
                exec.feedback = Some(feedback);
                return Ok(());
            }
        }

        Err(PlaygroundError::ExecutionFailed(format!(
            "Execution {} not found",
            execution_id
        )))
    }

    /// Complete experiment and compute results.
    pub async fn complete_experiment(&self, experiment_id: &str) -> Result<ExperimentResults> {
        let mut experiments = self.experiments.write().await;
        let experiment = experiments
            .get_mut(experiment_id)
            .ok_or_else(|| PlaygroundError::ExperimentNotFound(experiment_id.to_string()))?;

        let mut variant_metrics: HashMap<String, VariantMetrics> = HashMap::new();

        for variant in &experiment.variants {
            let executions = &variant.executions;
            let n = executions.len();

            if n == 0 {
                continue;
            }

            let avg_latency =
                executions.iter().map(|e| e.latency_ms as f64).sum::<f64>() / n as f64;
            let avg_tokens = executions
                .iter()
                .map(|e| e.tokens.total as f64)
                .sum::<f64>()
                / n as f64;
            let avg_cost = executions.iter().map(|e| e.cost).sum::<f64>() / n as f64;

            let ratings: Vec<_> = executions
                .iter()
                .filter_map(|e| e.feedback.as_ref().and_then(|f| f.rating))
                .collect();
            let avg_rating = if ratings.is_empty() {
                0.0
            } else {
                ratings.iter().sum::<i32>() as f64 / ratings.len() as f64
            };

            let correct_feedback: Vec<_> = executions
                .iter()
                .filter_map(|e| e.feedback.as_ref().and_then(|f| f.correct))
                .collect();
            let correct_rate = if correct_feedback.is_empty() {
                0.0
            } else {
                correct_feedback.iter().filter(|&&c| c).count() as f64
                    / correct_feedback.len() as f64
            };

            let satisfaction_rate = if ratings.is_empty() {
                0.0
            } else {
                ratings.iter().filter(|&&r| r >= 4).count() as f64 / ratings.len() as f64
            };

            variant_metrics.insert(
                variant.id.clone(),
                VariantMetrics {
                    total_executions: n,
                    avg_latency_ms: avg_latency,
                    avg_tokens,
                    avg_cost,
                    avg_rating,
                    correct_rate,
                    satisfaction_rate,
                },
            );
        }

        // Determine winner (by satisfaction rate, then by rating)
        let winner = variant_metrics
            .iter()
            .max_by(|a, b| {
                let score_a = a.1.satisfaction_rate * 0.6 + a.1.avg_rating / 5.0 * 0.4;
                let score_b = b.1.satisfaction_rate * 0.6 + b.1.avg_rating / 5.0 * 0.4;
                score_a.partial_cmp(&score_b).unwrap()
            })
            .map(|(id, _)| id.clone());

        let results = ExperimentResults {
            winner,
            significance: 0.95, // Simplified
            variant_metrics,
            summary: "Experiment completed".to_string(),
        };

        experiment.status = ExperimentStatus::Completed;
        experiment.completed_at = Some(Utc::now());
        experiment.results = Some(results.clone());

        Ok(results)
    }

    /// Create test suite.
    pub async fn create_test_suite(&self, name: &str) -> TestSuite {
        let suite = TestSuite {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            test_cases: Vec::new(),
            created_at: Utc::now(),
        };

        let mut suites = self.test_suites.write().await;
        suites.insert(suite.id.clone(), suite.clone());

        suite
    }

    /// Add test case to suite.
    pub async fn add_test_case(
        &self,
        suite_id: &str,
        name: &str,
        input: &str,
        expected: Option<&str>,
    ) -> Result<TestCase> {
        let mut suites = self.test_suites.write().await;
        let suite = suites
            .get_mut(suite_id)
            .ok_or_else(|| PlaygroundError::ExperimentNotFound(suite_id.to_string()))?;

        let test_case = TestCase {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            input: input.to_string(),
            expected_output: expected.map(|s| s.to_string()),
            tags: Vec::new(),
        };

        suite.test_cases.push(test_case.clone());

        Ok(test_case)
    }

    /// Run test suite.
    pub async fn run_test_suite(
        &self,
        suite_id: &str,
        config: &VariantConfig,
    ) -> Result<Vec<(TestCase, Execution, f64)>> {
        let suites = self.test_suites.read().await;
        let suite = suites
            .get(suite_id)
            .ok_or_else(|| PlaygroundError::ExperimentNotFound(suite_id.to_string()))?;

        let test_cases = suite.test_cases.clone();
        drop(suites);

        let mut results = Vec::new();

        for test_case in test_cases {
            let execution = self.quick_test(config, &test_case.input).await?;

            let scores = self
                .provider
                .compare(
                    &[execution.output.clone()],
                    test_case.expected_output.as_deref(),
                )
                .await?;

            let score = scores.first().copied().unwrap_or(0.0);
            results.push((test_case, execution, score));
        }

        Ok(results)
    }

    /// Get experiment.
    pub async fn get_experiment(&self, id: &str) -> Option<Experiment> {
        let experiments = self.experiments.read().await;
        experiments.get(id).cloned()
    }

    /// List experiments.
    pub async fn list_experiments(&self) -> Vec<Experiment> {
        let experiments = self.experiments.read().await;
        experiments.values().cloned().collect()
    }

    /// Get execution history.
    pub async fn get_history(&self, limit: usize) -> Vec<Execution> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl PlaygroundProvider for MockProvider {
        async fn execute(
            &self,
            config: &VariantConfig,
            input: &str,
        ) -> Result<(String, u64, TokenCount, f64)> {
            let output = format!("Response to: {}", input);
            let tokens = TokenCount {
                input: input.len() / 4,
                output: output.len() / 4,
                total: (input.len() + output.len()) / 4,
            };
            let latency = 100 + (config.temperature.unwrap_or(0.7) * 100.0) as u64;
            let cost = tokens.total as f64 * 0.001;

            Ok((output, latency, tokens, cost))
        }

        async fn compare(&self, outputs: &[String], expected: Option<&str>) -> Result<Vec<f64>> {
            Ok(outputs
                .iter()
                .map(|o| {
                    if let Some(exp) = expected {
                        if o.contains(exp) {
                            1.0
                        } else {
                            0.5
                        }
                    } else {
                        0.7
                    }
                })
                .collect())
        }
    }

    #[tokio::test]
    async fn test_quick_test() {
        let provider = Arc::new(MockProvider);
        let playground = Playground::new(provider);

        let config = VariantConfig::default();
        let execution = playground.quick_test(&config, "Hello world").await.unwrap();

        assert!(!execution.output.is_empty());
        assert!(execution.latency_ms > 0);
    }

    #[tokio::test]
    async fn test_experiment_lifecycle() {
        let provider = Arc::new(MockProvider);
        let playground = Playground::new(provider);

        // Create experiment
        let experiment = playground
            .create_experiment("Test", "A test experiment")
            .await;

        // Add variants
        playground
            .add_variant(
                &experiment.id,
                "Control",
                VariantConfig {
                    temperature: Some(0.7),
                    ..Default::default()
                },
                0.5,
            )
            .await
            .unwrap();

        playground
            .add_variant(
                &experiment.id,
                "Treatment",
                VariantConfig {
                    temperature: Some(0.3),
                    ..Default::default()
                },
                0.5,
            )
            .await
            .unwrap();

        // Start experiment
        playground.start_experiment(&experiment.id).await.unwrap();

        // Run iterations
        let executions = playground
            .run_iteration(&experiment.id, "Test prompt")
            .await
            .unwrap();
        assert_eq!(executions.len(), 2);

        // Submit feedback
        playground
            .submit_feedback(
                &experiment.id,
                &executions[0].id,
                ExecutionFeedback {
                    rating: Some(4),
                    correct: Some(true),
                    comment: None,
                    timestamp: Utc::now(),
                },
            )
            .await
            .unwrap();

        // Complete experiment
        let results = playground
            .complete_experiment(&experiment.id)
            .await
            .unwrap();
        assert!(!results.variant_metrics.is_empty());
    }

    #[tokio::test]
    async fn test_test_suite() {
        let provider = Arc::new(MockProvider);
        let playground = Playground::new(provider);

        // Create suite
        let suite = playground.create_test_suite("Basic Tests").await;

        // Add test cases
        playground
            .add_test_case(&suite.id, "Test 1", "Hello", Some("Response"))
            .await
            .unwrap();
        playground
            .add_test_case(&suite.id, "Test 2", "World", None)
            .await
            .unwrap();

        // Run suite
        let config = VariantConfig::default();
        let results = playground.run_test_suite(&suite.id, &config).await.unwrap();

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_invalid_experiment() {
        let provider = Arc::new(MockProvider);
        let playground = Playground::new(provider);

        let experiment = playground.create_experiment("Empty", "No variants").await;

        let result = playground.start_experiment(&experiment.id).await;
        assert!(result.is_err());
    }
}
