//! Next action prediction and intelligent pre-fetching.
//!
//! This crate provides predictive capabilities that:
//! - Predict user's next likely actions
//! - Pre-fetch resources proactively
//! - Learn from usage patterns
//! - Reduce latency through anticipation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Prediction errors.
#[derive(Debug, Error)]
pub enum PredictiveError {
    #[error("Prediction failed: {0}")]
    PredictionFailed(String),

    #[error("Insufficient history: {0}")]
    InsufficientHistory(String),

    #[error("Pre-fetch failed: {0}")]
    PrefetchFailed(String),

    #[error("Provider error: {0}")]
    ProviderError(String),
}

/// Result type for predictive operations.
pub type Result<T> = std::result::Result<T, PredictiveError>;

/// A user action that can be predicted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Action identifier.
    pub id: String,
    /// Action type.
    pub action_type: ActionType,
    /// Action parameters.
    pub parameters: HashMap<String, serde_json::Value>,
    /// Context when action occurred.
    pub context: ActionContext,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// User who performed the action.
    pub user_id: String,
}

/// Types of user actions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ActionType {
    /// Query/question.
    Query,
    /// Navigation action.
    Navigate,
    /// Search action.
    Search,
    /// File operation.
    FileOperation,
    /// Command execution.
    Command,
    /// Settings change.
    Settings,
    /// Content creation.
    Create,
    /// Content modification.
    Modify,
    /// Export/download.
    Export,
    /// Custom action.
    Custom(String),
}

/// Context surrounding an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionContext {
    /// Current location/page.
    pub location: Option<String>,
    /// Previous actions.
    pub previous_actions: Vec<String>,
    /// Time of day.
    pub time_of_day: TimeOfDay,
    /// Day of week.
    pub day_of_week: DayOfWeek,
    /// Session duration so far.
    pub session_duration_secs: u64,
    /// Additional context.
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Time of day buckets.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TimeOfDay {
    EarlyMorning, // 5-8
    Morning,      // 8-12
    Afternoon,    // 12-17
    Evening,      // 17-21
    Night,        // 21-5
}

impl TimeOfDay {
    /// Get time of day from hour.
    pub fn from_hour(hour: u32) -> Self {
        match hour {
            5..=7 => TimeOfDay::EarlyMorning,
            8..=11 => TimeOfDay::Morning,
            12..=16 => TimeOfDay::Afternoon,
            17..=20 => TimeOfDay::Evening,
            _ => TimeOfDay::Night,
        }
    }
}

/// Day of week.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DayOfWeek {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl DayOfWeek {
    /// Check if weekend.
    pub fn is_weekend(&self) -> bool {
        matches!(self, DayOfWeek::Saturday | DayOfWeek::Sunday)
    }
}

/// A predicted action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    /// Prediction identifier.
    pub id: String,
    /// Predicted action type.
    pub action_type: ActionType,
    /// Predicted parameters.
    pub predicted_parameters: HashMap<String, serde_json::Value>,
    /// Confidence (0.0-1.0).
    pub confidence: f64,
    /// Supporting evidence.
    pub evidence: Vec<PredictionEvidence>,
    /// Resources to pre-fetch.
    pub prefetch_resources: Vec<PrefetchResource>,
    /// When this prediction was made.
    pub predicted_at: DateTime<Utc>,
    /// Expiry time.
    pub expires_at: DateTime<Utc>,
}

/// Evidence supporting a prediction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionEvidence {
    /// Evidence type.
    pub evidence_type: EvidenceType,
    /// Description.
    pub description: String,
    /// Contribution to confidence.
    pub contribution: f64,
}

/// Types of prediction evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvidenceType {
    /// Historical pattern.
    HistoricalPattern,
    /// Sequential pattern.
    SequentialPattern,
    /// Temporal pattern.
    TemporalPattern,
    /// Context similarity.
    ContextSimilarity,
    /// User preference.
    UserPreference,
    /// Common action.
    CommonAction,
}

/// A resource to pre-fetch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefetchResource {
    /// Resource identifier.
    pub resource_id: String,
    /// Resource type.
    pub resource_type: ResourceType,
    /// URI/path to resource.
    pub uri: String,
    /// Priority (1-10).
    pub priority: u8,
    /// Estimated size in bytes.
    pub estimated_size: Option<u64>,
    /// Whether already cached.
    pub cached: bool,
}

/// Types of resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceType {
    /// Data/content.
    Data,
    /// API response.
    ApiResponse,
    /// File.
    File,
    /// Search results.
    SearchResults,
    /// Model/computation.
    Computation,
    /// Custom resource.
    Custom(String),
}

/// A learned pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Pattern identifier.
    pub id: String,
    /// Pattern type.
    pub pattern_type: PatternType,
    /// Action sequence in pattern.
    pub action_sequence: Vec<ActionType>,
    /// Context requirements.
    pub context_requirements: Vec<ContextRequirement>,
    /// Frequency of occurrence.
    pub frequency: u32,
    /// Confidence in pattern.
    pub confidence: f64,
    /// Last observed.
    pub last_observed: DateTime<Utc>,
}

/// Types of patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternType {
    /// Sequence of actions.
    Sequential,
    /// Time-based pattern.
    Temporal,
    /// Context-based pattern.
    Contextual,
    /// Combined pattern.
    Combined,
}

/// Context requirement for a pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRequirement {
    /// Requirement type.
    pub requirement: ContextRequirementType,
    /// Importance (0.0-1.0).
    pub importance: f64,
}

/// Types of context requirements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextRequirementType {
    /// Time of day.
    TimeOfDay(TimeOfDay),
    /// Day of week.
    DayOfWeek(DayOfWeek),
    /// Location.
    Location(String),
    /// Previous action.
    PreviousAction(ActionType),
}

/// Provider for prediction capabilities.
#[async_trait]
pub trait PredictiveProvider: Send + Sync {
    /// Predict next actions.
    async fn predict(&self, history: &[Action], context: &ActionContext)
        -> Result<Vec<Prediction>>;

    /// Learn patterns from history.
    async fn learn_patterns(&self, history: &[Action]) -> Result<Vec<Pattern>>;

    /// Generate resources to pre-fetch.
    async fn suggest_prefetch(&self, prediction: &Prediction) -> Result<Vec<PrefetchResource>>;
}

/// Provider for actually fetching resources.
#[async_trait]
pub trait PrefetchExecutor: Send + Sync {
    /// Pre-fetch a resource.
    async fn prefetch(&self, resource: &PrefetchResource) -> Result<()>;

    /// Check if resource is cached.
    async fn is_cached(&self, resource_id: &str) -> bool;
}

/// The predictive engine.
pub struct PredictiveEngine {
    /// Provider for predictions.
    provider: Arc<dyn PredictiveProvider>,
    /// Executor for pre-fetching.
    executor: Option<Arc<dyn PrefetchExecutor>>,
    /// Action history.
    history: Arc<RwLock<VecDeque<Action>>>,
    /// Learned patterns.
    patterns: Arc<RwLock<Vec<Pattern>>>,
    /// Active predictions.
    predictions: Arc<RwLock<HashMap<String, Prediction>>>,
    /// Pre-fetch cache status.
    cache_status: Arc<RwLock<HashMap<String, CacheEntry>>>,
    /// Configuration.
    config: PredictiveConfig,
}

/// Configuration for the predictive engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictiveConfig {
    /// Maximum history to keep.
    pub max_history: usize,
    /// Minimum confidence for predictions.
    pub min_confidence: f64,
    /// Maximum predictions to return.
    pub max_predictions: usize,
    /// Prediction expiry in seconds.
    pub prediction_expiry_secs: u64,
    /// Enable automatic pre-fetching.
    pub auto_prefetch: bool,
    /// Maximum prefetch size in bytes.
    pub max_prefetch_size: u64,
}

impl Default for PredictiveConfig {
    fn default() -> Self {
        Self {
            max_history: 1000,
            min_confidence: 0.5,
            max_predictions: 5,
            prediction_expiry_secs: 300,
            auto_prefetch: true,
            max_prefetch_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// A cache entry for pre-fetched resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Resource ID.
    pub resource_id: String,
    /// When cached.
    pub cached_at: DateTime<Utc>,
    /// Cache hits.
    pub hits: u32,
    /// Size in bytes.
    pub size: u64,
}

impl PredictiveEngine {
    /// Create a new predictive engine.
    pub fn new(provider: Arc<dyn PredictiveProvider>, config: PredictiveConfig) -> Self {
        Self {
            provider,
            executor: None,
            history: Arc::new(RwLock::new(VecDeque::new())),
            patterns: Arc::new(RwLock::new(Vec::new())),
            predictions: Arc::new(RwLock::new(HashMap::new())),
            cache_status: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Set the prefetch executor.
    pub fn with_executor(mut self, executor: Arc<dyn PrefetchExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Record an action.
    pub async fn record_action(&self, action: Action) -> Result<Vec<Prediction>> {
        // Add to history
        let mut history = self.history.write().await;
        history.push_back(action.clone());
        if history.len() > self.config.max_history {
            history.pop_front();
        }

        // Build context from recent actions
        let recent: Vec<_> = history.iter().rev().take(10).cloned().collect();
        drop(history);

        // Generate predictions
        let context = ActionContext {
            location: action.context.location.clone(),
            previous_actions: recent
                .iter()
                .map(|a| format!("{:?}", a.action_type))
                .collect(),
            time_of_day: action.context.time_of_day,
            day_of_week: action.context.day_of_week,
            session_duration_secs: action.context.session_duration_secs,
            metadata: HashMap::new(),
        };

        self.predict(&context).await
    }

    /// Generate predictions for current context.
    pub async fn predict(&self, context: &ActionContext) -> Result<Vec<Prediction>> {
        let history: Vec<_> = {
            let h = self.history.read().await;
            h.iter().cloned().collect()
        };

        if history.len() < 3 {
            return Err(PredictiveError::InsufficientHistory(
                "Need at least 3 actions to make predictions".to_string(),
            ));
        }

        let mut predictions = self.provider.predict(&history, context).await?;

        // Filter by confidence
        predictions.retain(|p| p.confidence >= self.config.min_confidence);

        // Limit predictions
        predictions.truncate(self.config.max_predictions);

        // Apply configured expiry time
        let expiry =
            Utc::now() + chrono::Duration::seconds(self.config.prediction_expiry_secs as i64);
        for prediction in &mut predictions {
            prediction.expires_at = expiry;
        }

        // Store predictions
        let mut stored = self.predictions.write().await;
        for prediction in &predictions {
            stored.insert(prediction.id.clone(), prediction.clone());
        }

        // Auto prefetch if enabled
        if self.config.auto_prefetch {
            self.auto_prefetch(&predictions).await?;
        }

        Ok(predictions)
    }

    /// Automatically pre-fetch resources for predictions.
    async fn auto_prefetch(&self, predictions: &[Prediction]) -> Result<()> {
        let executor = match &self.executor {
            Some(e) => e,
            None => return Ok(()),
        };

        let mut total_size = 0u64;

        for prediction in predictions {
            if prediction.confidence < 0.7 {
                continue;
            }

            for resource in &prediction.prefetch_resources {
                if resource.cached {
                    continue;
                }

                if let Some(size) = resource.estimated_size {
                    if total_size + size > self.config.max_prefetch_size {
                        continue;
                    }
                    total_size += size;
                }

                if let Err(e) = executor.prefetch(resource).await {
                    tracing::warn!("Prefetch failed for {}: {}", resource.resource_id, e);
                } else {
                    let mut cache = self.cache_status.write().await;
                    cache.insert(
                        resource.resource_id.clone(),
                        CacheEntry {
                            resource_id: resource.resource_id.clone(),
                            cached_at: Utc::now(),
                            hits: 0,
                            size: resource.estimated_size.unwrap_or(0),
                        },
                    );
                }
            }
        }

        Ok(())
    }

    /// Learn patterns from history.
    pub async fn learn(&self) -> Result<Vec<Pattern>> {
        let history: Vec<_> = {
            let h = self.history.read().await;
            h.iter().cloned().collect()
        };

        let new_patterns = self.provider.learn_patterns(&history).await?;

        let mut patterns = self.patterns.write().await;
        for pattern in &new_patterns {
            // Update existing or add new
            if let Some(existing) = patterns
                .iter_mut()
                .find(|p| p.action_sequence == pattern.action_sequence)
            {
                existing.frequency = pattern.frequency;
                existing.confidence = pattern.confidence;
                existing.last_observed = pattern.last_observed;
            } else {
                patterns.push(pattern.clone());
            }
        }

        Ok(new_patterns)
    }

    /// Get current predictions.
    pub async fn get_predictions(&self) -> Vec<Prediction> {
        let now = Utc::now();
        let predictions = self.predictions.read().await;
        predictions
            .values()
            .filter(|p| p.expires_at > now)
            .cloned()
            .collect()
    }

    /// Get learned patterns.
    pub async fn get_patterns(&self) -> Vec<Pattern> {
        let patterns = self.patterns.read().await;
        patterns.clone()
    }

    /// Record a cache hit.
    pub async fn record_cache_hit(&self, resource_id: &str) {
        let mut cache = self.cache_status.write().await;
        if let Some(entry) = cache.get_mut(resource_id) {
            entry.hits += 1;
        }
    }

    /// Get prediction accuracy metrics.
    pub async fn get_metrics(&self) -> PredictionMetrics {
        let predictions = self.predictions.read().await;
        let patterns = self.patterns.read().await;
        let cache = self.cache_status.read().await;

        let total_hits: u32 = cache.values().map(|e| e.hits).sum();
        let total_size: u64 = cache.values().map(|e| e.size).sum();

        PredictionMetrics {
            total_predictions: predictions.len(),
            active_predictions: predictions
                .values()
                .filter(|p| p.expires_at > Utc::now())
                .count(),
            learned_patterns: patterns.len(),
            cache_entries: cache.len(),
            cache_hits: total_hits,
            cache_size_bytes: total_size,
        }
    }

    /// Clear expired predictions.
    pub async fn cleanup(&self) {
        let now = Utc::now();

        let mut predictions = self.predictions.write().await;
        predictions.retain(|_, p| p.expires_at > now);
    }
}

/// Prediction accuracy metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionMetrics {
    /// Total predictions made.
    pub total_predictions: usize,
    /// Currently active predictions.
    pub active_predictions: usize,
    /// Number of learned patterns.
    pub learned_patterns: usize,
    /// Number of cached resources.
    pub cache_entries: usize,
    /// Total cache hits.
    pub cache_hits: u32,
    /// Total cache size.
    pub cache_size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider;

    #[async_trait]
    impl PredictiveProvider for MockProvider {
        async fn predict(
            &self,
            history: &[Action],
            _context: &ActionContext,
        ) -> Result<Vec<Prediction>> {
            // Simple prediction: repeat last action type
            let last_type = history
                .last()
                .map(|a| a.action_type.clone())
                .unwrap_or(ActionType::Query);

            Ok(vec![Prediction {
                id: Uuid::new_v4().to_string(),
                action_type: last_type,
                predicted_parameters: HashMap::new(),
                confidence: 0.75,
                evidence: vec![PredictionEvidence {
                    evidence_type: EvidenceType::SequentialPattern,
                    description: "Repeated action pattern".to_string(),
                    contribution: 0.75,
                }],
                prefetch_resources: vec![],
                predicted_at: Utc::now(),
                expires_at: Utc::now() + chrono::Duration::minutes(5),
            }])
        }

        async fn learn_patterns(&self, history: &[Action]) -> Result<Vec<Pattern>> {
            // Find simple sequential patterns
            let mut patterns = Vec::new();

            if history.len() >= 2 {
                patterns.push(Pattern {
                    id: Uuid::new_v4().to_string(),
                    pattern_type: PatternType::Sequential,
                    action_sequence: history
                        .iter()
                        .take(2)
                        .map(|a| a.action_type.clone())
                        .collect(),
                    context_requirements: vec![],
                    frequency: 1,
                    confidence: 0.6,
                    last_observed: Utc::now(),
                });
            }

            Ok(patterns)
        }

        async fn suggest_prefetch(
            &self,
            _prediction: &Prediction,
        ) -> Result<Vec<PrefetchResource>> {
            Ok(vec![])
        }
    }

    fn create_test_action(action_type: ActionType) -> Action {
        Action {
            id: Uuid::new_v4().to_string(),
            action_type,
            parameters: HashMap::new(),
            context: ActionContext {
                location: Some("/home".to_string()),
                previous_actions: vec![],
                time_of_day: TimeOfDay::Morning,
                day_of_week: DayOfWeek::Monday,
                session_duration_secs: 120,
                metadata: HashMap::new(),
            },
            timestamp: Utc::now(),
            user_id: "user1".to_string(),
        }
    }

    #[tokio::test]
    async fn test_record_action() {
        let provider = Arc::new(MockProvider);
        let engine = PredictiveEngine::new(provider, PredictiveConfig::default());

        // Record enough actions
        engine
            .record_action(create_test_action(ActionType::Query))
            .await
            .ok();
        engine
            .record_action(create_test_action(ActionType::Search))
            .await
            .ok();
        let predictions = engine
            .record_action(create_test_action(ActionType::Navigate))
            .await
            .unwrap();

        assert!(!predictions.is_empty());
    }

    #[tokio::test]
    async fn test_learn_patterns() {
        let provider = Arc::new(MockProvider);
        let engine = PredictiveEngine::new(provider, PredictiveConfig::default());

        // Record actions
        engine
            .record_action(create_test_action(ActionType::Query))
            .await
            .ok();
        engine
            .record_action(create_test_action(ActionType::Search))
            .await
            .ok();
        engine
            .record_action(create_test_action(ActionType::Navigate))
            .await
            .ok();

        let patterns = engine.learn().await.unwrap();
        assert!(!patterns.is_empty());
    }

    #[tokio::test]
    async fn test_prediction_expiry() {
        let provider = Arc::new(MockProvider);
        let config = PredictiveConfig {
            prediction_expiry_secs: 0, // Expire immediately
            ..Default::default()
        };
        let engine = PredictiveEngine::new(provider, config);

        // Record actions
        engine
            .record_action(create_test_action(ActionType::Query))
            .await
            .ok();
        engine
            .record_action(create_test_action(ActionType::Search))
            .await
            .ok();
        engine
            .record_action(create_test_action(ActionType::Navigate))
            .await
            .ok();

        // Wait a moment
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let predictions = engine.get_predictions().await;
        assert!(predictions.is_empty()); // Should be expired
    }

    #[tokio::test]
    async fn test_metrics() {
        let provider = Arc::new(MockProvider);
        let engine = PredictiveEngine::new(provider, PredictiveConfig::default());

        engine
            .record_action(create_test_action(ActionType::Query))
            .await
            .ok();
        engine
            .record_action(create_test_action(ActionType::Search))
            .await
            .ok();
        engine
            .record_action(create_test_action(ActionType::Navigate))
            .await
            .ok();

        let metrics = engine.get_metrics().await;
        assert!(metrics.total_predictions > 0);
    }

    #[test]
    fn test_time_of_day() {
        assert_eq!(TimeOfDay::from_hour(6), TimeOfDay::EarlyMorning);
        assert_eq!(TimeOfDay::from_hour(10), TimeOfDay::Morning);
        assert_eq!(TimeOfDay::from_hour(14), TimeOfDay::Afternoon);
        assert_eq!(TimeOfDay::from_hour(19), TimeOfDay::Evening);
        assert_eq!(TimeOfDay::from_hour(23), TimeOfDay::Night);
    }

    #[test]
    fn test_day_of_week() {
        assert!(!DayOfWeek::Monday.is_weekend());
        assert!(DayOfWeek::Saturday.is_weekend());
        assert!(DayOfWeek::Sunday.is_weekend());
    }
}
