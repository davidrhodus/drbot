//! Continuous learning for drbot.
//!
//! Adaptive improvement system.
//!
//! # Features
//!
//! - Feedback collection
//! - Performance tracking
//! - Model fine-tuning suggestions
//! - Behavior adaptation
//! - A/B testing

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Evolve result type.
pub type Result<T> = std::result::Result<T, EvolveError>;

/// Evolve errors.
#[derive(Debug, thiserror::Error)]
pub enum EvolveError {
    #[error("Experiment not found: {0}")]
    ExperimentNotFound(String),
    #[error("Insufficient data: {0}")]
    InsufficientData(String),
    #[error("Adaptation failed: {0}")]
    AdaptationFailed(String),
}

/// Feedback type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackType {
    ThumbsUp,
    ThumbsDown,
    Rating,
    Edit,
    Retry,
    Copy,
    Share,
    Report,
}

/// Feedback entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    /// Feedback ID.
    pub id: Uuid,
    /// Session ID.
    pub session_id: String,
    /// Message ID.
    pub message_id: String,
    /// Feedback type.
    pub feedback_type: FeedbackType,
    /// Rating (1-5).
    pub rating: Option<u8>,
    /// User comment.
    pub comment: Option<String>,
    /// Original response.
    pub original_response: String,
    /// Edited response (if applicable).
    pub edited_response: Option<String>,
    /// Context.
    pub context: HashMap<String, String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl Feedback {
    /// Create new feedback.
    pub fn new(
        session_id: &str,
        message_id: &str,
        feedback_type: FeedbackType,
        original: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id: session_id.to_string(),
            message_id: message_id.to_string(),
            feedback_type,
            rating: None,
            comment: None,
            original_response: original.to_string(),
            edited_response: None,
            context: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Is positive feedback.
    pub fn is_positive(&self) -> bool {
        match self.feedback_type {
            FeedbackType::ThumbsUp | FeedbackType::Copy | FeedbackType::Share => true,
            FeedbackType::Rating => self.rating.map(|r| r >= 4).unwrap_or(false),
            _ => false,
        }
    }
}

/// Performance metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    /// Metric name.
    pub name: String,
    /// Value.
    pub value: f64,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Tags.
    pub tags: HashMap<String, String>,
}

/// Behavior pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorPattern {
    /// Pattern ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Triggers.
    pub triggers: Vec<String>,
    /// Response template.
    pub response_template: Option<String>,
    /// Confidence.
    pub confidence: f64,
    /// Sample count.
    pub sample_count: usize,
    /// Success rate.
    pub success_rate: f64,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

/// Experiment variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    /// Variant ID.
    pub id: String,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Configuration.
    pub config: HashMap<String, String>,
    /// Weight (for traffic allocation).
    pub weight: f64,
}

/// Experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    /// Experiment ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Variants.
    pub variants: Vec<Variant>,
    /// Status.
    pub status: ExperimentStatus,
    /// Primary metric.
    pub primary_metric: String,
    /// Start time.
    pub started_at: Option<DateTime<Utc>>,
    /// End time.
    pub ended_at: Option<DateTime<Utc>>,
    /// Results.
    pub results: Option<ExperimentResults>,
}

/// Experiment status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    Draft,
    Running,
    Paused,
    Completed,
    Cancelled,
}

/// Experiment results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResults {
    /// Samples per variant.
    pub samples: HashMap<String, usize>,
    /// Metrics per variant.
    pub metrics: HashMap<String, HashMap<String, f64>>,
    /// Winner variant.
    pub winner: Option<String>,
    /// Statistical significance.
    pub significance: f64,
}

/// Adaptation suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Adaptation {
    /// Adaptation ID.
    pub id: Uuid,
    /// Type.
    pub adaptation_type: AdaptationType,
    /// Description.
    pub description: String,
    /// Reasoning.
    pub reasoning: String,
    /// Confidence.
    pub confidence: f64,
    /// Impact estimate.
    pub estimated_impact: f64,
    /// Applied.
    pub applied: bool,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// Adaptation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdaptationType {
    PromptTweak,
    TemperatureAdjust,
    ModelSwitch,
    ResponseFormat,
    ContextLength,
    Custom,
}

/// Evolution configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolveConfig {
    /// Enable feedback collection.
    pub collect_feedback: bool,
    /// Enable experiments.
    pub enable_experiments: bool,
    /// Minimum samples for adaptation.
    pub min_samples: usize,
    /// Confidence threshold.
    pub confidence_threshold: f64,
}

impl Default for EvolveConfig {
    fn default() -> Self {
        Self {
            collect_feedback: true,
            enable_experiments: true,
            min_samples: 100,
            confidence_threshold: 0.95,
        }
    }
}

/// Trait for learning analyzers.
#[async_trait]
pub trait LearningAnalyzer: Send + Sync {
    /// Analyze feedback patterns.
    async fn analyze_feedback(&self, feedback: &[Feedback]) -> Vec<BehaviorPattern>;
    /// Suggest adaptations.
    async fn suggest_adaptations(
        &self,
        patterns: &[BehaviorPattern],
        metrics: &[Metric],
    ) -> Vec<Adaptation>;
}

/// Evolution engine.
pub struct EvolutionEngine<A: LearningAnalyzer> {
    config: EvolveConfig,
    analyzer: A,
    feedback: Arc<RwLock<Vec<Feedback>>>,
    metrics: Arc<RwLock<Vec<Metric>>>,
    patterns: Arc<RwLock<Vec<BehaviorPattern>>>,
    experiments: Arc<RwLock<HashMap<Uuid, Experiment>>>,
    adaptations: Arc<RwLock<Vec<Adaptation>>>,
    experiment_assignments: Arc<RwLock<HashMap<String, (Uuid, String)>>>,
}

impl<A: LearningAnalyzer> EvolutionEngine<A> {
    /// Create a new evolution engine.
    pub fn new(config: EvolveConfig, analyzer: A) -> Self {
        Self {
            config,
            analyzer,
            feedback: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(Vec::new())),
            patterns: Arc::new(RwLock::new(Vec::new())),
            experiments: Arc::new(RwLock::new(HashMap::new())),
            adaptations: Arc::new(RwLock::new(Vec::new())),
            experiment_assignments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record feedback.
    pub async fn record_feedback(&self, feedback: Feedback) {
        if self.config.collect_feedback {
            self.feedback.write().await.push(feedback);
        }
    }

    /// Record metric.
    pub async fn record_metric(&self, metric: Metric) {
        self.metrics.write().await.push(metric);
    }

    /// Get feedback stats.
    pub async fn feedback_stats(&self) -> FeedbackStats {
        let feedback = self.feedback.read().await;

        let positive = feedback.iter().filter(|f| f.is_positive()).count();
        let negative = feedback.iter().filter(|f| !f.is_positive()).count();
        let total = feedback.len();

        let avg_rating = {
            let ratings: Vec<_> = feedback.iter().filter_map(|f| f.rating).collect();
            if ratings.is_empty() {
                None
            } else {
                Some(ratings.iter().map(|&r| r as f64).sum::<f64>() / ratings.len() as f64)
            }
        };

        FeedbackStats {
            total,
            positive,
            negative,
            avg_rating,
            satisfaction_rate: if total > 0 {
                positive as f64 / total as f64
            } else {
                0.0
            },
        }
    }

    /// Learn from collected data.
    pub async fn learn(&self) -> Result<Vec<BehaviorPattern>> {
        let feedback = self.feedback.read().await;

        if feedback.len() < self.config.min_samples {
            return Err(EvolveError::InsufficientData(format!(
                "Need {} samples, have {}",
                self.config.min_samples,
                feedback.len()
            )));
        }

        let patterns = self.analyzer.analyze_feedback(&feedback).await;

        // Store learned patterns
        let mut stored = self.patterns.write().await;
        for pattern in &patterns {
            if !stored.iter().any(|p| p.name == pattern.name) {
                stored.push(pattern.clone());
            }
        }

        Ok(patterns)
    }

    /// Generate adaptations.
    pub async fn generate_adaptations(&self) -> Result<Vec<Adaptation>> {
        let patterns = self.patterns.read().await.clone();
        let metrics = self.metrics.read().await.clone();

        let adaptations = self.analyzer.suggest_adaptations(&patterns, &metrics).await;

        // Store adaptations
        self.adaptations.write().await.extend(adaptations.clone());

        Ok(adaptations)
    }

    /// Create experiment.
    pub async fn create_experiment(&self, experiment: Experiment) -> Result<Uuid> {
        let id = experiment.id;
        self.experiments.write().await.insert(id, experiment);
        Ok(id)
    }

    /// Start experiment.
    pub async fn start_experiment(&self, id: Uuid) -> Result<()> {
        let mut experiments = self.experiments.write().await;
        let experiment = experiments
            .get_mut(&id)
            .ok_or(EvolveError::ExperimentNotFound(id.to_string()))?;

        experiment.status = ExperimentStatus::Running;
        experiment.started_at = Some(Utc::now());

        Ok(())
    }

    /// Get experiment variant for user.
    pub async fn get_variant(&self, experiment_id: Uuid, user_id: &str) -> Result<Variant> {
        let experiments = self.experiments.read().await;
        let experiment = experiments
            .get(&experiment_id)
            .ok_or(EvolveError::ExperimentNotFound(experiment_id.to_string()))?;

        if experiment.status != ExperimentStatus::Running {
            return Err(EvolveError::ExperimentNotFound(
                "Experiment not running".to_string(),
            ));
        }

        // Check existing assignment
        let assignments = self.experiment_assignments.read().await;
        if let Some((exp_id, variant_id)) = assignments.get(user_id) {
            if *exp_id == experiment_id {
                return experiment
                    .variants
                    .iter()
                    .find(|v| v.id == *variant_id)
                    .cloned()
                    .ok_or(EvolveError::ExperimentNotFound(
                        "Variant not found".to_string(),
                    ));
            }
        }
        drop(assignments);

        // Assign new variant based on weights
        let total_weight: f64 = experiment.variants.iter().map(|v| v.weight).sum();
        let rand: f64 = simple_random();
        let mut cumulative = 0.0;

        for variant in &experiment.variants {
            cumulative += variant.weight / total_weight;
            if rand <= cumulative {
                // Store assignment
                self.experiment_assignments
                    .write()
                    .await
                    .insert(user_id.to_string(), (experiment_id, variant.id.clone()));
                return Ok(variant.clone());
            }
        }

        // Fallback to first variant
        Ok(experiment.variants[0].clone())
    }

    /// Record experiment result.
    pub async fn record_experiment_result(
        &self,
        experiment_id: Uuid,
        variant_id: &str,
        metric_name: &str,
        value: f64,
    ) -> Result<()> {
        let mut experiments = self.experiments.write().await;
        let experiment = experiments
            .get_mut(&experiment_id)
            .ok_or(EvolveError::ExperimentNotFound(experiment_id.to_string()))?;

        let results = experiment.results.get_or_insert_with(|| ExperimentResults {
            samples: HashMap::new(),
            metrics: HashMap::new(),
            winner: None,
            significance: 0.0,
        });

        *results.samples.entry(variant_id.to_string()).or_insert(0) += 1;

        let variant_metrics = results.metrics.entry(variant_id.to_string()).or_default();
        let current = variant_metrics
            .entry(metric_name.to_string())
            .or_insert(0.0);
        // Running average
        let count = *results.samples.get(variant_id).unwrap_or(&1) as f64;
        *current = (*current * (count - 1.0) + value) / count;

        Ok(())
    }

    /// Complete experiment.
    pub async fn complete_experiment(&self, id: Uuid) -> Result<ExperimentResults> {
        let mut experiments = self.experiments.write().await;
        let experiment = experiments
            .get_mut(&id)
            .ok_or(EvolveError::ExperimentNotFound(id.to_string()))?;

        experiment.status = ExperimentStatus::Completed;
        experiment.ended_at = Some(Utc::now());

        // Determine winner
        if let Some(ref mut results) = experiment.results {
            let primary = &experiment.primary_metric;

            let mut best_variant: Option<(String, f64)> = None;
            for (variant_id, metrics) in &results.metrics {
                if let Some(&value) = metrics.get(primary) {
                    if best_variant
                        .as_ref()
                        .map(|(_, v)| value > *v)
                        .unwrap_or(true)
                    {
                        best_variant = Some((variant_id.clone(), value));
                    }
                }
            }

            results.winner = best_variant.map(|(id, _)| id);
            // Simplified significance (real impl would use statistical tests)
            results.significance = if results.samples.values().all(|&s| s >= 30) {
                0.95
            } else {
                0.0
            };

            return Ok(results.clone());
        }

        Err(EvolveError::InsufficientData(
            "No results recorded".to_string(),
        ))
    }

    /// Get evolution summary.
    pub async fn summary(&self) -> EvolveSummary {
        let feedback = self.feedback.read().await;
        let patterns = self.patterns.read().await;
        let experiments = self.experiments.read().await;
        let adaptations = self.adaptations.read().await;

        let running_experiments = experiments
            .values()
            .filter(|e| e.status == ExperimentStatus::Running)
            .count();
        let applied_adaptations = adaptations.iter().filter(|a| a.applied).count();

        EvolveSummary {
            total_feedback: feedback.len(),
            learned_patterns: patterns.len(),
            running_experiments,
            total_experiments: experiments.len(),
            suggested_adaptations: adaptations.len(),
            applied_adaptations,
        }
    }
}

/// Feedback statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackStats {
    pub total: usize,
    pub positive: usize,
    pub negative: usize,
    pub avg_rating: Option<f64>,
    pub satisfaction_rate: f64,
}

/// Evolution summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolveSummary {
    pub total_feedback: usize,
    pub learned_patterns: usize,
    pub running_experiments: usize,
    pub total_experiments: usize,
    pub suggested_adaptations: usize,
    pub applied_adaptations: usize,
}

/// Simple pseudo-random (for testing).
fn simple_random() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    (nanos % 1000) as f64 / 1000.0
}

/// Simple learning analyzer for testing.
pub struct SimpleAnalyzer;

#[async_trait]
impl LearningAnalyzer for SimpleAnalyzer {
    async fn analyze_feedback(&self, feedback: &[Feedback]) -> Vec<BehaviorPattern> {
        // Group by positive/negative
        let positive: Vec<_> = feedback.iter().filter(|f| f.is_positive()).collect();

        if positive.len() >= 10 {
            vec![BehaviorPattern {
                id: Uuid::new_v4(),
                name: "Helpful responses".to_string(),
                description: "Responses that users found helpful".to_string(),
                triggers: vec!["question".to_string(), "help".to_string()],
                response_template: None,
                confidence: positive.len() as f64 / feedback.len() as f64,
                sample_count: positive.len(),
                success_rate: positive.len() as f64 / feedback.len() as f64,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }]
        } else {
            Vec::new()
        }
    }

    async fn suggest_adaptations(
        &self,
        patterns: &[BehaviorPattern],
        _metrics: &[Metric],
    ) -> Vec<Adaptation> {
        patterns
            .iter()
            .filter(|p| p.success_rate < 0.8)
            .map(|p| Adaptation {
                id: Uuid::new_v4(),
                adaptation_type: AdaptationType::PromptTweak,
                description: format!("Improve pattern: {}", p.name),
                reasoning: format!("Success rate {} below threshold", p.success_rate),
                confidence: 0.7,
                estimated_impact: 0.1,
                applied: false,
                created_at: Utc::now(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_feedback() {
        let engine = EvolutionEngine::new(EvolveConfig::default(), SimpleAnalyzer);

        let feedback = Feedback::new("sess1", "msg1", FeedbackType::ThumbsUp, "Hello");
        engine.record_feedback(feedback).await;

        let stats = engine.feedback_stats().await;
        assert_eq!(stats.total, 1);
        assert_eq!(stats.positive, 1);
    }

    #[tokio::test]
    async fn test_feedback_stats() {
        let engine = EvolutionEngine::new(EvolveConfig::default(), SimpleAnalyzer);

        for i in 0..10 {
            let ft = if i < 7 {
                FeedbackType::ThumbsUp
            } else {
                FeedbackType::ThumbsDown
            };
            engine
                .record_feedback(Feedback::new("s", &i.to_string(), ft, "test"))
                .await;
        }

        let stats = engine.feedback_stats().await;
        assert_eq!(stats.total, 10);
        assert_eq!(stats.positive, 7);
        assert!((stats.satisfaction_rate - 0.7).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_create_experiment() {
        let engine = EvolutionEngine::new(EvolveConfig::default(), SimpleAnalyzer);

        let experiment = Experiment {
            id: Uuid::new_v4(),
            name: "Test Experiment".to_string(),
            description: "Testing A/B".to_string(),
            variants: vec![
                Variant {
                    id: "A".to_string(),
                    name: "Control".to_string(),
                    description: "".to_string(),
                    config: HashMap::new(),
                    weight: 0.5,
                },
                Variant {
                    id: "B".to_string(),
                    name: "Treatment".to_string(),
                    description: "".to_string(),
                    config: HashMap::new(),
                    weight: 0.5,
                },
            ],
            status: ExperimentStatus::Draft,
            primary_metric: "satisfaction".to_string(),
            started_at: None,
            ended_at: None,
            results: None,
        };

        let id = engine.create_experiment(experiment).await.unwrap();
        engine.start_experiment(id).await.unwrap();

        let summary = engine.summary().await;
        assert_eq!(summary.running_experiments, 1);
    }

    #[tokio::test]
    async fn test_get_variant() {
        let engine = EvolutionEngine::new(EvolveConfig::default(), SimpleAnalyzer);

        let exp_id = Uuid::new_v4();
        let experiment = Experiment {
            id: exp_id,
            name: "Test".to_string(),
            description: "".to_string(),
            variants: vec![
                Variant {
                    id: "A".to_string(),
                    name: "A".to_string(),
                    description: "".to_string(),
                    config: HashMap::new(),
                    weight: 1.0,
                },
                Variant {
                    id: "B".to_string(),
                    name: "B".to_string(),
                    description: "".to_string(),
                    config: HashMap::new(),
                    weight: 1.0,
                },
            ],
            status: ExperimentStatus::Running,
            primary_metric: "clicks".to_string(),
            started_at: Some(Utc::now()),
            ended_at: None,
            results: None,
        };

        engine.create_experiment(experiment).await.unwrap();

        let variant1 = engine.get_variant(exp_id, "user1").await.unwrap();
        let variant2 = engine.get_variant(exp_id, "user1").await.unwrap();

        // Same user should get same variant
        assert_eq!(variant1.id, variant2.id);
    }

    #[tokio::test]
    async fn test_insufficient_data() {
        let config = EvolveConfig {
            min_samples: 100,
            ..Default::default()
        };
        let engine = EvolutionEngine::new(config, SimpleAnalyzer);

        // Only 10 samples
        for i in 0..10 {
            engine
                .record_feedback(Feedback::new(
                    "s",
                    &i.to_string(),
                    FeedbackType::ThumbsUp,
                    "test",
                ))
                .await;
        }

        let result = engine.learn().await;
        assert!(matches!(result, Err(EvolveError::InsufficientData(_))));
    }
}
