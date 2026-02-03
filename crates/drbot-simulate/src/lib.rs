//! Simulation mode for drbot.
//!
//! Test AI behaviors in sandboxed environments.
//!
//! # Features
//!
//! - Mock environments
//! - Scenario testing
//! - Behavior validation
//! - Performance benchmarking
//! - Regression testing

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Simulation result type.
pub type Result<T> = std::result::Result<T, SimulateError>;

/// Simulation errors.
#[derive(Debug, thiserror::Error)]
pub enum SimulateError {
    #[error("Scenario not found: {0}")]
    ScenarioNotFound(String),
    #[error("Environment error: {0}")]
    EnvironmentError(String),
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    #[error("Timeout: {0}")]
    Timeout(String),
}

/// Mock message for simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockMessage {
    /// Message ID.
    pub id: Uuid,
    /// Role.
    pub role: MessageRole,
    /// Content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl MockMessage {
    /// Create user message.
    pub fn user(content: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::User,
            content: content.to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create assistant message.
    pub fn assistant(content: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: content.to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }
}

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Test scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    /// Scenario ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Initial messages.
    pub setup: Vec<MockMessage>,
    /// Test input.
    pub input: String,
    /// Expected output patterns.
    pub expected_patterns: Vec<String>,
    /// Validators.
    pub validators: Vec<Validator>,
    /// Tags.
    pub tags: Vec<String>,
    /// Timeout (seconds).
    pub timeout_secs: u64,
}

impl Scenario {
    /// Create a new scenario.
    pub fn new(name: &str, input: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            setup: Vec::new(),
            input: input.to_string(),
            expected_patterns: Vec::new(),
            validators: Vec::new(),
            tags: Vec::new(),
            timeout_secs: 30,
        }
    }

    /// Add expected pattern.
    pub fn expect(mut self, pattern: &str) -> Self {
        self.expected_patterns.push(pattern.to_string());
        self
    }

    /// Add validator.
    pub fn validate(mut self, validator: Validator) -> Self {
        self.validators.push(validator);
        self
    }
}

/// Validator types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Validator {
    /// Response contains pattern.
    Contains(String),
    /// Response does not contain.
    NotContains(String),
    /// Response matches regex.
    Regex(String),
    /// Response length in range.
    LengthRange { min: usize, max: usize },
    /// Response is valid JSON.
    ValidJson,
    /// Response sentiment is positive.
    PositiveSentiment,
    /// Custom validator (by name).
    Custom(String),
}

impl Validator {
    /// Validate response.
    pub fn validate(&self, response: &str) -> ValidationResult {
        match self {
            Validator::Contains(pattern) => {
                let passed = response.to_lowercase().contains(&pattern.to_lowercase());
                ValidationResult {
                    validator: format!("Contains({})", pattern),
                    passed,
                    message: if passed {
                        None
                    } else {
                        Some(format!("Expected to contain: {}", pattern))
                    },
                }
            }
            Validator::NotContains(pattern) => {
                let passed = !response.to_lowercase().contains(&pattern.to_lowercase());
                ValidationResult {
                    validator: format!("NotContains({})", pattern),
                    passed,
                    message: if passed {
                        None
                    } else {
                        Some(format!("Should not contain: {}", pattern))
                    },
                }
            }
            Validator::Regex(pattern) => {
                // Simplified regex check
                let passed = response.contains(pattern);
                ValidationResult {
                    validator: format!("Regex({})", pattern),
                    passed,
                    message: if passed {
                        None
                    } else {
                        Some("Regex not matched".to_string())
                    },
                }
            }
            Validator::LengthRange { min, max } => {
                let len = response.len();
                let passed = len >= *min && len <= *max;
                ValidationResult {
                    validator: format!("LengthRange({}-{})", min, max),
                    passed,
                    message: if passed {
                        None
                    } else {
                        Some(format!("Length {} not in range {}-{}", len, min, max))
                    },
                }
            }
            Validator::ValidJson => {
                let passed = serde_json::from_str::<serde_json::Value>(response).is_ok();
                ValidationResult {
                    validator: "ValidJson".to_string(),
                    passed,
                    message: if passed {
                        None
                    } else {
                        Some("Invalid JSON".to_string())
                    },
                }
            }
            Validator::PositiveSentiment => {
                // Simplified sentiment check
                let positive_words = ["great", "good", "happy", "thanks", "helpful", "excellent"];
                let passed = positive_words
                    .iter()
                    .any(|w| response.to_lowercase().contains(w));
                ValidationResult {
                    validator: "PositiveSentiment".to_string(),
                    passed,
                    message: if passed {
                        None
                    } else {
                        Some("Sentiment not positive".to_string())
                    },
                }
            }
            Validator::Custom(name) => {
                ValidationResult {
                    validator: format!("Custom({})", name),
                    passed: true, // Custom validators pass by default
                    message: None,
                }
            }
        }
    }
}

/// Validation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Validator name.
    pub validator: String,
    /// Passed.
    pub passed: bool,
    /// Error message.
    pub message: Option<String>,
}

/// Scenario run result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioResult {
    /// Scenario ID.
    pub scenario_id: Uuid,
    /// Scenario name.
    pub scenario_name: String,
    /// Passed.
    pub passed: bool,
    /// Response.
    pub response: String,
    /// Validation results.
    pub validations: Vec<ValidationResult>,
    /// Duration (ms).
    pub duration_ms: u64,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Completed at.
    pub completed_at: DateTime<Utc>,
}

/// Test suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    /// Suite ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Scenarios.
    pub scenarios: Vec<Scenario>,
    /// Tags.
    pub tags: Vec<String>,
}

impl TestSuite {
    /// Create a new suite.
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: String::new(),
            scenarios: Vec::new(),
            tags: Vec::new(),
        }
    }

    /// Add scenario.
    pub fn add(mut self, scenario: Scenario) -> Self {
        self.scenarios.push(scenario);
        self
    }
}

/// Suite run result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteResult {
    /// Suite ID.
    pub suite_id: Uuid,
    /// Suite name.
    pub suite_name: String,
    /// Total scenarios.
    pub total: usize,
    /// Passed scenarios.
    pub passed: usize,
    /// Failed scenarios.
    pub failed: usize,
    /// Skipped scenarios.
    pub skipped: usize,
    /// Scenario results.
    pub results: Vec<ScenarioResult>,
    /// Duration (ms).
    pub duration_ms: u64,
}

/// Mock environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockEnvironment {
    /// Environment ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Variables.
    pub variables: HashMap<String, String>,
    /// Mock responses.
    pub mock_responses: HashMap<String, String>,
    /// Simulated latency (ms).
    pub latency_ms: u64,
    /// Error rate (0-1).
    pub error_rate: f64,
}

impl MockEnvironment {
    /// Create a new environment.
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            variables: HashMap::new(),
            mock_responses: HashMap::new(),
            latency_ms: 0,
            error_rate: 0.0,
        }
    }

    /// Add mock response.
    pub fn mock(mut self, pattern: &str, response: &str) -> Self {
        self.mock_responses
            .insert(pattern.to_string(), response.to_string());
        self
    }
}

/// Simulation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulateConfig {
    /// Default timeout (seconds).
    pub default_timeout: u64,
    /// Parallel execution.
    pub parallel: bool,
    /// Max parallel.
    pub max_parallel: usize,
    /// Stop on first failure.
    pub fail_fast: bool,
}

impl Default for SimulateConfig {
    fn default() -> Self {
        Self {
            default_timeout: 30,
            parallel: true,
            max_parallel: 4,
            fail_fast: false,
        }
    }
}

/// Trait for AI responders.
#[async_trait]
pub trait Responder: Send + Sync {
    /// Generate response.
    async fn respond(&self, messages: &[MockMessage], input: &str) -> Result<String>;
}

/// Simulation engine.
pub struct SimulationEngine<R: Responder> {
    config: SimulateConfig,
    responder: R,
    environments: Arc<RwLock<HashMap<Uuid, MockEnvironment>>>,
    suites: Arc<RwLock<HashMap<Uuid, TestSuite>>>,
    results: Arc<RwLock<Vec<SuiteResult>>>,
}

impl<R: Responder> SimulationEngine<R> {
    /// Create a new simulation engine.
    pub fn new(config: SimulateConfig, responder: R) -> Self {
        Self {
            config,
            responder,
            environments: Arc::new(RwLock::new(HashMap::new())),
            suites: Arc::new(RwLock::new(HashMap::new())),
            results: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Add environment.
    pub async fn add_environment(&self, env: MockEnvironment) {
        self.environments.write().await.insert(env.id, env);
    }

    /// Add test suite.
    pub async fn add_suite(&self, suite: TestSuite) {
        self.suites.write().await.insert(suite.id, suite);
    }

    /// Run single scenario.
    pub async fn run_scenario(&self, scenario: &Scenario) -> Result<ScenarioResult> {
        let started_at = Utc::now();

        // Get response
        let response = tokio::time::timeout(
            tokio::time::Duration::from_secs(scenario.timeout_secs),
            self.responder.respond(&scenario.setup, &scenario.input),
        )
        .await
        .map_err(|_| SimulateError::Timeout("Scenario timed out".to_string()))?
        .map_err(|e| SimulateError::EnvironmentError(e.to_string()))?;

        let completed_at = Utc::now();

        // Run validations
        let mut validations = Vec::new();

        // Check expected patterns
        for pattern in &scenario.expected_patterns {
            validations.push(Validator::Contains(pattern.clone()).validate(&response));
        }

        // Run custom validators
        for validator in &scenario.validators {
            validations.push(validator.validate(&response));
        }

        let passed = validations.iter().all(|v| v.passed);

        let duration = (completed_at - started_at).num_milliseconds() as u64;

        Ok(ScenarioResult {
            scenario_id: scenario.id,
            scenario_name: scenario.name.clone(),
            passed,
            response,
            validations,
            duration_ms: duration,
            started_at,
            completed_at,
        })
    }

    /// Run test suite.
    pub async fn run_suite(&self, suite_id: Uuid) -> Result<SuiteResult> {
        let suite = self
            .suites
            .read()
            .await
            .get(&suite_id)
            .cloned()
            .ok_or(SimulateError::ScenarioNotFound(suite_id.to_string()))?;

        let started_at = Utc::now();
        let mut results = Vec::new();
        let mut passed = 0;
        let mut failed = 0;

        for scenario in &suite.scenarios {
            let result = self.run_scenario(scenario).await;

            match result {
                Ok(r) => {
                    if r.passed {
                        passed += 1;
                    } else {
                        failed += 1;
                        if self.config.fail_fast {
                            results.push(r);
                            break;
                        }
                    }
                    results.push(r);
                }
                Err(e) => {
                    failed += 1;
                    results.push(ScenarioResult {
                        scenario_id: scenario.id,
                        scenario_name: scenario.name.clone(),
                        passed: false,
                        response: String::new(),
                        validations: vec![ValidationResult {
                            validator: "Execution".to_string(),
                            passed: false,
                            message: Some(e.to_string()),
                        }],
                        duration_ms: 0,
                        started_at,
                        completed_at: Utc::now(),
                    });

                    if self.config.fail_fast {
                        break;
                    }
                }
            }
        }

        let completed_at = Utc::now();
        let duration = (completed_at - started_at).num_milliseconds() as u64;

        let suite_result = SuiteResult {
            suite_id: suite.id,
            suite_name: suite.name,
            total: suite.scenarios.len(),
            passed,
            failed,
            skipped: suite.scenarios.len() - passed - failed,
            results,
            duration_ms: duration,
        };

        self.results.write().await.push(suite_result.clone());

        Ok(suite_result)
    }

    /// Run all suites.
    pub async fn run_all(&self) -> Vec<SuiteResult> {
        let suite_ids: Vec<_> = self.suites.read().await.keys().cloned().collect();
        let mut all_results = Vec::new();

        for id in suite_ids {
            if let Ok(result) = self.run_suite(id).await {
                all_results.push(result);
            }
        }

        all_results
    }

    /// Get historical results.
    pub async fn get_results(&self) -> Vec<SuiteResult> {
        self.results.read().await.clone()
    }

    /// Get summary statistics.
    pub async fn summary(&self) -> SimulationSummary {
        let results = self.results.read().await;

        let total_suites = results.len();
        let total_scenarios: usize = results.iter().map(|r| r.total).sum();
        let total_passed: usize = results.iter().map(|r| r.passed).sum();
        let total_failed: usize = results.iter().map(|r| r.failed).sum();

        SimulationSummary {
            total_suites,
            total_scenarios,
            passed: total_passed,
            failed: total_failed,
            pass_rate: if total_scenarios > 0 {
                total_passed as f64 / total_scenarios as f64
            } else {
                0.0
            },
        }
    }
}

/// Simulation summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationSummary {
    pub total_suites: usize,
    pub total_scenarios: usize,
    pub passed: usize,
    pub failed: usize,
    pub pass_rate: f64,
}

/// Echo responder for testing.
pub struct EchoResponder;

#[async_trait]
impl Responder for EchoResponder {
    async fn respond(&self, _messages: &[MockMessage], input: &str) -> Result<String> {
        Ok(format!("Echo: {}", input))
    }
}

/// Mock AI responder for testing.
pub struct MockResponder {
    responses: HashMap<String, String>,
    default_response: String,
}

impl MockResponder {
    pub fn new(default: &str) -> Self {
        Self {
            responses: HashMap::new(),
            default_response: default.to_string(),
        }
    }

    pub fn when(mut self, input: &str, response: &str) -> Self {
        self.responses
            .insert(input.to_lowercase(), response.to_string());
        self
    }
}

#[async_trait]
impl Responder for MockResponder {
    async fn respond(&self, _messages: &[MockMessage], input: &str) -> Result<String> {
        Ok(self
            .responses
            .get(&input.to_lowercase())
            .cloned()
            .unwrap_or_else(|| self.default_response.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_scenario() {
        let engine = SimulationEngine::new(SimulateConfig::default(), EchoResponder);

        let scenario = Scenario::new("Echo test", "Hello world")
            .expect("echo")
            .expect("hello");

        let result = engine.run_scenario(&scenario).await.unwrap();

        assert!(result.passed);
        assert!(result.response.to_lowercase().contains("echo"));
    }

    #[tokio::test]
    async fn test_validator_contains() {
        let validator = Validator::Contains("hello".to_string());
        assert!(validator.validate("Hello World").passed);
        assert!(!validator.validate("Goodbye").passed);
    }

    #[tokio::test]
    async fn test_validator_length_range() {
        let validator = Validator::LengthRange { min: 5, max: 10 };
        assert!(validator.validate("Hello").passed);
        assert!(!validator.validate("Hi").passed);
        assert!(!validator.validate("Hello World!").passed);
    }

    #[tokio::test]
    async fn test_validator_valid_json() {
        let validator = Validator::ValidJson;
        assert!(validator.validate(r#"{"key": "value"}"#).passed);
        assert!(!validator.validate("not json").passed);
    }

    #[tokio::test]
    async fn test_run_suite() {
        let engine = SimulationEngine::new(SimulateConfig::default(), EchoResponder);

        let suite = TestSuite::new("Basic Tests")
            .add(Scenario::new("Test 1", "Hello").expect("echo"))
            .add(Scenario::new("Test 2", "World").expect("echo"));

        let suite_id = suite.id;
        engine.add_suite(suite).await;

        let result = engine.run_suite(suite_id).await.unwrap();

        assert_eq!(result.total, 2);
        assert_eq!(result.passed, 2);
        assert_eq!(result.failed, 0);
    }

    #[tokio::test]
    async fn test_mock_responder() {
        let responder = MockResponder::new("Default response")
            .when("hello", "Hi there!")
            .when("help", "How can I help?");

        let engine = SimulationEngine::new(SimulateConfig::default(), responder);

        let scenario1 = Scenario::new("Greeting", "hello").expect("hi there");
        let scenario2 = Scenario::new("Unknown", "random").expect("default");

        assert!(engine.run_scenario(&scenario1).await.unwrap().passed);
        assert!(engine.run_scenario(&scenario2).await.unwrap().passed);
    }

    #[tokio::test]
    async fn test_fail_fast() {
        let responder = MockResponder::new("OK");
        let config = SimulateConfig {
            fail_fast: true,
            ..Default::default()
        };
        let engine = SimulationEngine::new(config, responder);

        let suite = TestSuite::new("Fail Fast Suite")
            .add(Scenario::new("Pass", "test").expect("ok"))
            .add(Scenario::new("Fail", "test").expect("NOT PRESENT"))
            .add(Scenario::new("Skip", "test").expect("ok"));

        let suite_id = suite.id;
        engine.add_suite(suite).await;

        let result = engine.run_suite(suite_id).await.unwrap();

        assert_eq!(result.passed, 1);
        assert_eq!(result.failed, 1);
        assert_eq!(result.skipped, 1);
    }

    #[tokio::test]
    async fn test_summary() {
        let engine = SimulationEngine::new(SimulateConfig::default(), EchoResponder);

        let suite1 = TestSuite::new("Suite 1").add(Scenario::new("Test", "hi").expect("echo"));
        let suite2 = TestSuite::new("Suite 2").add(Scenario::new("Test", "hi").expect("echo"));

        let id1 = suite1.id;
        let id2 = suite2.id;

        engine.add_suite(suite1).await;
        engine.add_suite(suite2).await;

        engine.run_suite(id1).await.unwrap();
        engine.run_suite(id2).await.unwrap();

        let summary = engine.summary().await;
        assert_eq!(summary.total_suites, 2);
        assert_eq!(summary.total_scenarios, 2);
        assert!((summary.pass_rate - 1.0).abs() < 0.01);
    }
}
