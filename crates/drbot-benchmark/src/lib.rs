//! AI benchmarks for drbot.
//!
//! Measure and track AI performance.
//!
//! # Features
//!
//! - Response quality metrics
//! - Latency tracking
//! - Accuracy measurement
//! - Comparative benchmarks

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Benchmark result type.
pub type Result<T> = std::result::Result<T, BenchmarkError>;

/// Benchmark errors.
#[derive(Debug, thiserror::Error)]
pub enum BenchmarkError {
    #[error("Benchmark failed: {0}")]
    Failed(String),
    #[error("Benchmark not found: {0}")]
    NotFound(Uuid),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Timeout")]
    Timeout,
}

/// Benchmark run result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRun {
    /// Run ID.
    pub id: Uuid,
    /// Benchmark name.
    pub name: String,
    /// Model/provider tested.
    pub model: String,
    /// Results.
    pub results: Vec<TestResult>,
    /// Aggregate metrics.
    pub metrics: AggregateMetrics,
    /// Configuration used.
    pub config: BenchmarkConfig,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Completed at.
    pub completed_at: DateTime<Utc>,
    /// Duration in ms.
    pub duration_ms: u64,
}

/// Individual test result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Test ID.
    pub id: Uuid,
    /// Test name.
    pub name: String,
    /// Input.
    pub input: String,
    /// Expected output.
    pub expected: Option<String>,
    /// Actual output.
    pub actual: String,
    /// Passed.
    pub passed: bool,
    /// Score (0-1).
    pub score: f32,
    /// Latency in ms.
    pub latency_ms: u64,
    /// Token count.
    pub tokens: usize,
    /// Metrics.
    pub metrics: TestMetrics,
}

/// Test-level metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestMetrics {
    /// Accuracy score.
    pub accuracy: f32,
    /// Relevance score.
    pub relevance: f32,
    /// Coherence score.
    pub coherence: f32,
    /// Fluency score.
    pub fluency: f32,
    /// Safety score.
    pub safety: f32,
}

impl TestMetrics {
    /// Calculate overall score.
    pub fn overall(&self) -> f32 {
        let scores = [
            self.accuracy,
            self.relevance,
            self.coherence,
            self.fluency,
            self.safety,
        ];
        scores.iter().sum::<f32>() / scores.len() as f32
    }
}

/// Aggregate metrics for a benchmark run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AggregateMetrics {
    /// Total tests.
    pub total_tests: usize,
    /// Passed tests.
    pub passed_tests: usize,
    /// Pass rate.
    pub pass_rate: f32,
    /// Average score.
    pub avg_score: f32,
    /// Average latency in ms.
    pub avg_latency_ms: f32,
    /// P50 latency.
    pub p50_latency_ms: u64,
    /// P95 latency.
    pub p95_latency_ms: u64,
    /// P99 latency.
    pub p99_latency_ms: u64,
    /// Total tokens.
    pub total_tokens: usize,
    /// Tokens per second.
    pub tokens_per_second: f32,
    /// Per-metric averages.
    pub by_metric: HashMap<String, f32>,
}

/// Benchmark configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// Benchmark name.
    pub name: String,
    /// Number of runs per test.
    pub runs_per_test: usize,
    /// Timeout per test in ms.
    pub timeout_ms: u64,
    /// Warmup runs.
    pub warmup_runs: usize,
    /// Parallel tests.
    pub parallel: usize,
    /// Score threshold for pass.
    pub pass_threshold: f32,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            runs_per_test: 1,
            timeout_ms: 30000,
            warmup_runs: 1,
            parallel: 1,
            pass_threshold: 0.7,
        }
    }
}

/// A benchmark test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    /// Test name.
    pub name: String,
    /// Test category.
    pub category: String,
    /// Input prompt.
    pub input: String,
    /// Expected output (for exact match).
    pub expected: Option<String>,
    /// Keywords to check for.
    pub keywords: Vec<String>,
    /// Custom validator.
    pub validator: Option<String>,
    /// Difficulty.
    pub difficulty: Difficulty,
}

impl TestCase {
    /// Create a new test case.
    pub fn new(name: &str, category: &str, input: &str) -> Self {
        Self {
            name: name.to_string(),
            category: category.to_string(),
            input: input.to_string(),
            expected: None,
            keywords: Vec::new(),
            validator: None,
            difficulty: Difficulty::Medium,
        }
    }

    /// Set expected output.
    pub fn with_expected(mut self, expected: &str) -> Self {
        self.expected = Some(expected.to_string());
        self
    }

    /// Add keywords to check.
    pub fn with_keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }
}

/// Test difficulty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    Expert,
}

/// Benchmark suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSuite {
    /// Suite name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Test cases.
    pub tests: Vec<TestCase>,
    /// Version.
    pub version: String,
}

impl BenchmarkSuite {
    /// Create a new suite.
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            tests: Vec::new(),
            version: "1.0.0".to_string(),
        }
    }

    /// Add a test case.
    pub fn add_test(&mut self, test: TestCase) {
        self.tests.push(test);
    }
}

/// Benchmark engine.
pub struct BenchmarkEngine {
    suites: Arc<RwLock<HashMap<String, BenchmarkSuite>>>,
    runs: Arc<RwLock<Vec<BenchmarkRun>>>,
}

impl BenchmarkEngine {
    /// Create a new benchmark engine.
    pub fn new() -> Self {
        Self {
            suites: Arc::new(RwLock::new(HashMap::new())),
            runs: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a benchmark suite.
    pub async fn register_suite(&self, suite: BenchmarkSuite) {
        self.suites.write().await.insert(suite.name.clone(), suite);
    }

    /// Run a benchmark suite.
    pub async fn run<F, Fut>(
        &self,
        suite_name: &str,
        model: &str,
        config: BenchmarkConfig,
        executor: F,
    ) -> Result<BenchmarkRun>
    where
        F: Fn(String) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<(String, usize)>> + Send,
    {
        let suites = self.suites.read().await;
        let suite = suites
            .get(suite_name)
            .ok_or_else(|| BenchmarkError::NotFound(Uuid::new_v4()))?;

        let started_at = Utc::now();
        let start_time = Instant::now();
        let mut results = Vec::new();

        // Run warmup
        for _ in 0..config.warmup_runs {
            if let Some(test) = suite.tests.first() {
                let _ = executor(test.input.clone()).await;
            }
        }

        // Run tests
        for test in &suite.tests {
            for _ in 0..config.runs_per_test {
                let test_start = Instant::now();

                let (output, tokens) = executor(test.input.clone()).await?;

                let latency_ms = test_start.elapsed().as_millis() as u64;

                let (passed, score, metrics) = self.evaluate_result(test, &output);

                results.push(TestResult {
                    id: Uuid::new_v4(),
                    name: test.name.clone(),
                    input: test.input.clone(),
                    expected: test.expected.clone(),
                    actual: output,
                    passed,
                    score,
                    latency_ms,
                    tokens,
                    metrics,
                });
            }
        }

        let duration_ms = start_time.elapsed().as_millis() as u64;
        let completed_at = Utc::now();

        let metrics = self.calculate_aggregate(&results);

        let run = BenchmarkRun {
            id: Uuid::new_v4(),
            name: suite_name.to_string(),
            model: model.to_string(),
            results,
            metrics,
            config,
            started_at,
            completed_at,
            duration_ms,
        };

        self.runs.write().await.push(run.clone());

        Ok(run)
    }

    fn evaluate_result(&self, test: &TestCase, output: &str) -> (bool, f32, TestMetrics) {
        let mut score = 0.0;
        let mut metrics = TestMetrics::default();

        // Check exact match
        if let Some(expected) = &test.expected {
            if output.trim() == expected.trim() {
                score += 0.5;
                metrics.accuracy = 1.0;
            } else {
                // Partial match
                let similarity = self.calculate_similarity(expected, output);
                score += similarity * 0.3;
                metrics.accuracy = similarity;
            }
        }

        // Check keywords
        if !test.keywords.is_empty() {
            let keyword_matches = test
                .keywords
                .iter()
                .filter(|k| output.to_lowercase().contains(&k.to_lowercase()))
                .count();
            let keyword_score = keyword_matches as f32 / test.keywords.len() as f32;
            score += keyword_score * 0.3;
            metrics.relevance = keyword_score;
        }

        // Basic quality checks
        metrics.coherence = if output.len() > 10 { 0.8 } else { 0.5 };
        metrics.fluency = if output.contains('.') || output.contains('!') {
            0.9
        } else {
            0.7
        };
        metrics.safety = 1.0; // Would use actual safety checks

        score += metrics.overall() * 0.2;
        score = score.min(1.0);

        let passed = score >= 0.7;

        (passed, score, metrics)
    }

    fn calculate_similarity(&self, a: &str, b: &str) -> f32 {
        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();
        let words_a: std::collections::HashSet<_> = a_lower.split_whitespace().collect();
        let words_b: std::collections::HashSet<_> = b_lower.split_whitespace().collect();

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            0.0
        } else {
            intersection as f32 / union as f32
        }
    }

    fn calculate_aggregate(&self, results: &[TestResult]) -> AggregateMetrics {
        if results.is_empty() {
            return AggregateMetrics::default();
        }

        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let total_score: f32 = results.iter().map(|r| r.score).sum();
        let total_latency: u64 = results.iter().map(|r| r.latency_ms).sum();
        let total_tokens: usize = results.iter().map(|r| r.tokens).sum();

        let mut latencies: Vec<_> = results.iter().map(|r| r.latency_ms).collect();
        latencies.sort();

        let p50 = latencies.get(total / 2).copied().unwrap_or(0);
        let p95 = latencies.get(total * 95 / 100).copied().unwrap_or(0);
        let p99 = latencies.get(total * 99 / 100).copied().unwrap_or(0);

        let total_time_secs = total_latency as f32 / 1000.0;
        let tokens_per_second = if total_time_secs > 0.0 {
            total_tokens as f32 / total_time_secs
        } else {
            0.0
        };

        AggregateMetrics {
            total_tests: total,
            passed_tests: passed,
            pass_rate: passed as f32 / total as f32,
            avg_score: total_score / total as f32,
            avg_latency_ms: total_latency as f32 / total as f32,
            p50_latency_ms: p50,
            p95_latency_ms: p95,
            p99_latency_ms: p99,
            total_tokens,
            tokens_per_second,
            by_metric: HashMap::new(),
        }
    }

    /// Get all runs.
    pub async fn list_runs(&self) -> Vec<BenchmarkRun> {
        self.runs.read().await.clone()
    }

    /// Get runs for a model.
    pub async fn runs_by_model(&self, model: &str) -> Vec<BenchmarkRun> {
        self.runs
            .read()
            .await
            .iter()
            .filter(|r| r.model == model)
            .cloned()
            .collect()
    }

    /// Compare models.
    pub async fn compare_models(&self, models: &[&str]) -> HashMap<String, AggregateMetrics> {
        let runs = self.runs.read().await;

        models
            .iter()
            .filter_map(|&model| {
                let model_runs: Vec<_> = runs.iter().filter(|r| r.model == model).collect();
                if model_runs.is_empty() {
                    return None;
                }

                let all_results: Vec<_> =
                    model_runs.iter().flat_map(|r| r.results.clone()).collect();
                let metrics = self.calculate_aggregate(&all_results);

                Some((model.to_string(), metrics))
            })
            .collect()
    }

    /// List suites.
    pub async fn list_suites(&self) -> Vec<BenchmarkSuite> {
        self.suites.read().await.values().cloned().collect()
    }
}

impl Default for BenchmarkEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_benchmark_run() {
        let engine = BenchmarkEngine::new();

        let mut suite = BenchmarkSuite::new("basic", "Basic tests");
        suite.add_test(
            TestCase::new("math", "arithmetic", "What is 2 + 2?")
                .with_expected("4")
                .with_keywords(vec!["four".to_string(), "4".to_string()]),
        );

        engine.register_suite(suite).await;

        // Output should match expected to get a passing score
        let executor = |_input: String| async { Ok(("4".to_string(), 10)) };

        let run = engine
            .run("basic", "test-model", BenchmarkConfig::default(), executor)
            .await
            .unwrap();

        assert_eq!(run.results.len(), 1);
        assert!(run.metrics.pass_rate > 0.0);
    }

    #[test]
    fn test_test_metrics() {
        let metrics = TestMetrics {
            accuracy: 0.9,
            relevance: 0.8,
            coherence: 0.85,
            fluency: 0.9,
            safety: 1.0,
        };

        let overall = metrics.overall();
        assert!(overall > 0.8 && overall < 1.0);
    }

    #[tokio::test]
    async fn test_suite_registration() {
        let engine = BenchmarkEngine::new();

        let suite = BenchmarkSuite::new("test", "Test suite");
        engine.register_suite(suite).await;

        let suites = engine.list_suites().await;
        assert_eq!(suites.len(), 1);
        assert_eq!(suites[0].name, "test");
    }
}
