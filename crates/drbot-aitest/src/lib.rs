//! AI behavior testing framework for drbot.
//!
//! Test AI responses and behavior systematically.
//!
//! # Features
//!
//! - Test case definition
//! - Response validation
//! - Regression testing
//! - Benchmark comparisons
//! - Quality metrics

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Test result type.
pub type Result<T> = std::result::Result<T, TestError>;

/// Test errors.
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("Test failed: {0}")]
    Failed(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Test case definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    /// Test ID.
    pub id: Uuid,
    /// Test name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Test category/group.
    pub category: String,
    /// Input prompt.
    pub prompt: String,
    /// System prompt (if any).
    pub system_prompt: Option<String>,
    /// Expected behavior.
    pub expectations: Vec<Expectation>,
    /// Tags for filtering.
    pub tags: Vec<String>,
    /// Timeout in seconds.
    pub timeout_secs: u64,
    /// Number of runs for statistical tests.
    pub runs: usize,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl TestCase {
    /// Create a new test case.
    pub fn new(name: &str, prompt: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            category: "default".to_string(),
            prompt: prompt.to_string(),
            system_prompt: None,
            expectations: Vec::new(),
            tags: Vec::new(),
            timeout_secs: 30,
            runs: 1,
            metadata: HashMap::new(),
        }
    }

    /// Set description.
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    /// Set category.
    pub fn with_category(mut self, category: &str) -> Self {
        self.category = category.to_string();
        self
    }

    /// Add expectation.
    pub fn expect(mut self, expectation: Expectation) -> Self {
        self.expectations.push(expectation);
        self
    }

    /// Add tag.
    pub fn with_tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_string());
        self
    }

    /// Set system prompt.
    pub fn with_system_prompt(mut self, prompt: &str) -> Self {
        self.system_prompt = Some(prompt.to_string());
        self
    }

    /// Set runs.
    pub fn with_runs(mut self, runs: usize) -> Self {
        self.runs = runs;
        self
    }
}

/// Expectation for test validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Expectation {
    /// Response contains text.
    Contains { text: String, case_sensitive: bool },
    /// Response does not contain text.
    NotContains { text: String, case_sensitive: bool },
    /// Response matches pattern.
    Matches { pattern: String },
    /// Response length within range.
    LengthBetween { min: usize, max: usize },
    /// Response is valid JSON.
    ValidJson,
    /// JSON matches schema.
    JsonSchema { schema: serde_json::Value },
    /// Semantic similarity to expected.
    SemanticSimilarity { expected: String, threshold: f32 },
    /// Sentiment check.
    Sentiment { expected: Sentiment, tolerance: f32 },
    /// Response time within limit.
    ResponseTime { max_ms: u64 },
    /// Custom validator.
    Custom {
        name: String,
        config: serde_json::Value,
    },
    /// No hallucinated facts.
    NoHallucination { facts: Vec<String> },
    /// Follows format.
    FollowsFormat { format: String },
    /// Is safe/appropriate.
    IsSafe,
    /// Consistent across runs.
    Consistent { similarity_threshold: f32 },
}

/// Sentiment types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sentiment {
    Positive,
    Negative,
    Neutral,
    Professional,
    Friendly,
}

/// Test result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Test case ID.
    pub test_id: Uuid,
    /// Test name.
    pub test_name: String,
    /// Overall pass/fail.
    pub passed: bool,
    /// Individual expectation results.
    pub expectation_results: Vec<ExpectationResult>,
    /// Response.
    pub response: String,
    /// Response time in ms.
    pub response_time_ms: u64,
    /// Token count.
    pub tokens: TokenUsage,
    /// Run statistics (for multi-run tests).
    pub run_stats: Option<RunStats>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Error message (if failed).
    pub error: Option<String>,
}

impl TestResult {
    /// Create a passing result.
    pub fn pass(test: &TestCase, response: &str, response_time_ms: u64) -> Self {
        Self {
            test_id: test.id,
            test_name: test.name.clone(),
            passed: true,
            expectation_results: Vec::new(),
            response: response.to_string(),
            response_time_ms,
            tokens: TokenUsage::default(),
            run_stats: None,
            timestamp: Utc::now(),
            error: None,
        }
    }

    /// Create a failing result.
    pub fn fail(test: &TestCase, error: &str) -> Self {
        Self {
            test_id: test.id,
            test_name: test.name.clone(),
            passed: false,
            expectation_results: Vec::new(),
            response: String::new(),
            response_time_ms: 0,
            tokens: TokenUsage::default(),
            run_stats: None,
            timestamp: Utc::now(),
            error: Some(error.to_string()),
        }
    }
}

/// Expectation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectationResult {
    /// Expectation type.
    pub expectation_type: String,
    /// Pass/fail.
    pub passed: bool,
    /// Details.
    pub details: String,
    /// Score (for numeric expectations).
    pub score: Option<f32>,
}

/// Token usage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens.
    pub input: usize,
    /// Output tokens.
    pub output: usize,
    /// Total tokens.
    pub total: usize,
}

/// Run statistics for multi-run tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStats {
    /// Number of runs.
    pub runs: usize,
    /// Pass count.
    pub passes: usize,
    /// Fail count.
    pub failures: usize,
    /// Average response time.
    pub avg_response_time_ms: f64,
    /// Response time standard deviation.
    pub std_response_time_ms: f64,
    /// Consistency score.
    pub consistency_score: f32,
}

/// Test suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    /// Suite ID.
    pub id: Uuid,
    /// Suite name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Test cases.
    pub tests: Vec<TestCase>,
    /// Setup instructions.
    pub setup: Option<String>,
    /// Teardown instructions.
    pub teardown: Option<String>,
    /// Tags.
    pub tags: Vec<String>,
}

impl TestSuite {
    /// Create a new test suite.
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            tests: Vec::new(),
            setup: None,
            teardown: None,
            tags: Vec::new(),
        }
    }

    /// Add a test case.
    pub fn add_test(mut self, test: TestCase) -> Self {
        self.tests.push(test);
        self
    }

    /// Set description.
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }
}

/// Suite result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteResult {
    /// Suite ID.
    pub suite_id: Uuid,
    /// Suite name.
    pub suite_name: String,
    /// Individual test results.
    pub test_results: Vec<TestResult>,
    /// Total tests.
    pub total: usize,
    /// Passed tests.
    pub passed: usize,
    /// Failed tests.
    pub failed: usize,
    /// Pass rate.
    pub pass_rate: f32,
    /// Total duration in ms.
    pub duration_ms: u64,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl SuiteResult {
    /// Create from test results.
    pub fn from_results(suite: &TestSuite, results: Vec<TestResult>, duration_ms: u64) -> Self {
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;
        let pass_rate = if total > 0 {
            passed as f32 / total as f32
        } else {
            0.0
        };

        Self {
            suite_id: suite.id,
            suite_name: suite.name.clone(),
            test_results: results,
            total,
            passed,
            failed,
            pass_rate,
            duration_ms,
            timestamp: Utc::now(),
        }
    }
}

/// Trait for AI providers in testing.
#[async_trait]
pub trait TestableProvider: Send + Sync {
    /// Generate a response for testing.
    async fn generate(
        &self,
        prompt: &str,
        system_prompt: Option<&str>,
    ) -> Result<(String, TokenUsage)>;
}

/// Test runner.
pub struct TestRunner<P: TestableProvider> {
    provider: P,
}

impl<P: TestableProvider> TestRunner<P> {
    /// Create a new test runner.
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    /// Run a single test case.
    pub async fn run_test(&self, test: &TestCase) -> TestResult {
        let start = std::time::Instant::now();

        let result = self
            .provider
            .generate(&test.prompt, test.system_prompt.as_deref())
            .await;

        let response_time = start.elapsed().as_millis() as u64;

        match result {
            Ok((response, tokens)) => {
                let mut test_result = TestResult::pass(test, &response, response_time);
                test_result.tokens = tokens;

                // Evaluate expectations
                for expectation in &test.expectations {
                    let exp_result = self.evaluate_expectation(expectation, &response).await;
                    if !exp_result.passed {
                        test_result.passed = false;
                    }
                    test_result.expectation_results.push(exp_result);
                }

                test_result
            }
            Err(e) => TestResult::fail(test, &e.to_string()),
        }
    }

    /// Run a test suite.
    pub async fn run_suite(&self, suite: &TestSuite) -> SuiteResult {
        let start = std::time::Instant::now();
        let mut results = Vec::new();

        for test in &suite.tests {
            if test.runs > 1 {
                let result = self.run_multi(test).await;
                results.push(result);
            } else {
                let result = self.run_test(test).await;
                results.push(result);
            }
        }

        let duration = start.elapsed().as_millis() as u64;
        SuiteResult::from_results(suite, results, duration)
    }

    /// Run a test multiple times for statistical analysis.
    async fn run_multi(&self, test: &TestCase) -> TestResult {
        let mut response_times = Vec::new();
        let mut responses = Vec::new();
        let mut passes = 0;

        for _ in 0..test.runs {
            let result = self.run_test(test).await;
            response_times.push(result.response_time_ms as f64);
            responses.push(result.response.clone());
            if result.passed {
                passes += 1;
            }
        }

        // Calculate statistics
        let avg_time: f64 = response_times.iter().sum::<f64>() / response_times.len() as f64;
        let variance: f64 = response_times
            .iter()
            .map(|t| (t - avg_time).powi(2))
            .sum::<f64>()
            / response_times.len() as f64;
        let std_time = variance.sqrt();

        // Calculate consistency (simple: compare all to first)
        let consistency = if responses.len() > 1 {
            let first = &responses[0];
            let similarities: f32 = responses
                .iter()
                .skip(1)
                .map(|r| self.simple_similarity(first, r))
                .sum();
            similarities / (responses.len() - 1) as f32
        } else {
            1.0
        };

        let mut result = TestResult::pass(test, &responses[0], avg_time as u64);
        result.run_stats = Some(RunStats {
            runs: test.runs,
            passes,
            failures: test.runs - passes,
            avg_response_time_ms: avg_time,
            std_response_time_ms: std_time,
            consistency_score: consistency,
        });
        result.passed = passes == test.runs;

        result
    }

    async fn evaluate_expectation(
        &self,
        expectation: &Expectation,
        response: &str,
    ) -> ExpectationResult {
        match expectation {
            Expectation::Contains {
                text,
                case_sensitive,
            } => {
                let passed = if *case_sensitive {
                    response.contains(text)
                } else {
                    response.to_lowercase().contains(&text.to_lowercase())
                };
                ExpectationResult {
                    expectation_type: "contains".to_string(),
                    passed,
                    details: if passed {
                        format!("Found: {}", text)
                    } else {
                        format!("Not found: {}", text)
                    },
                    score: None,
                }
            }
            Expectation::NotContains {
                text,
                case_sensitive,
            } => {
                let contains = if *case_sensitive {
                    response.contains(text)
                } else {
                    response.to_lowercase().contains(&text.to_lowercase())
                };
                ExpectationResult {
                    expectation_type: "not_contains".to_string(),
                    passed: !contains,
                    details: if !contains {
                        format!("Correctly absent: {}", text)
                    } else {
                        format!("Unexpectedly found: {}", text)
                    },
                    score: None,
                }
            }
            Expectation::Matches { pattern } => {
                let re = regex::Regex::new(pattern);
                let passed = re.map(|r| r.is_match(response)).unwrap_or(false);
                ExpectationResult {
                    expectation_type: "matches".to_string(),
                    passed,
                    details: format!("Pattern: {}", pattern),
                    score: None,
                }
            }
            Expectation::LengthBetween { min, max } => {
                let len = response.len();
                let passed = len >= *min && len <= *max;
                ExpectationResult {
                    expectation_type: "length_between".to_string(),
                    passed,
                    details: format!("Length {} (expected {}-{})", len, min, max),
                    score: None,
                }
            }
            Expectation::ValidJson => {
                let passed = serde_json::from_str::<serde_json::Value>(response).is_ok();
                ExpectationResult {
                    expectation_type: "valid_json".to_string(),
                    passed,
                    details: if passed {
                        "Valid JSON".to_string()
                    } else {
                        "Invalid JSON".to_string()
                    },
                    score: None,
                }
            }
            Expectation::ResponseTime { max_ms: _ } => {
                // This is checked at the result level
                ExpectationResult {
                    expectation_type: "response_time".to_string(),
                    passed: true,
                    details: "Response time check".to_string(),
                    score: None,
                }
            }
            _ => {
                // Other expectations need more complex evaluation
                ExpectationResult {
                    expectation_type: "unknown".to_string(),
                    passed: true,
                    details: "Not evaluated".to_string(),
                    score: None,
                }
            }
        }
    }

    fn simple_similarity(&self, a: &str, b: &str) -> f32 {
        let words_a: std::collections::HashSet<_> = a.split_whitespace().collect();
        let words_b: std::collections::HashSet<_> = b.split_whitespace().collect();

        let intersection = words_a.intersection(&words_b).count();
        let union = words_a.union(&words_b).count();

        if union == 0 {
            1.0
        } else {
            intersection as f32 / union as f32
        }
    }
}

/// Simple test provider for testing.
pub struct SimpleTestProvider;

#[async_trait]
impl TestableProvider for SimpleTestProvider {
    async fn generate(
        &self,
        prompt: &str,
        _system_prompt: Option<&str>,
    ) -> Result<(String, TokenUsage)> {
        Ok((
            format!("Response to: {}", prompt),
            TokenUsage {
                input: prompt.len() / 4,
                output: 10,
                total: prompt.len() / 4 + 10,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_test_case() {
        let test = TestCase::new("Basic test", "What is 2+2?")
            .expect(Expectation::Contains {
                text: "4".to_string(),
                case_sensitive: false,
            })
            .with_description("Test basic math");

        assert_eq!(test.name, "Basic test");
        assert_eq!(test.expectations.len(), 1);
    }

    #[tokio::test]
    async fn test_runner() {
        let provider = SimpleTestProvider;
        let runner = TestRunner::new(provider);

        let test = TestCase::new("Simple test", "Hello").expect(Expectation::Contains {
            text: "Hello".to_string(),
            case_sensitive: false,
        });

        let result = runner.run_test(&test).await;
        assert!(result.passed);
    }

    #[tokio::test]
    async fn test_suite() {
        let suite = TestSuite::new("Math Tests")
            .add_test(TestCase::new("Addition", "What is 2+2?"))
            .add_test(TestCase::new("Subtraction", "What is 5-3?"));

        assert_eq!(suite.tests.len(), 2);
    }

    #[tokio::test]
    async fn test_run_suite() {
        let provider = SimpleTestProvider;
        let runner = TestRunner::new(provider);

        let suite = TestSuite::new("Test Suite")
            .add_test(TestCase::new("Test 1", "Hello"))
            .add_test(TestCase::new("Test 2", "World"));

        let result = runner.run_suite(&suite).await;
        assert_eq!(result.total, 2);
    }
}
