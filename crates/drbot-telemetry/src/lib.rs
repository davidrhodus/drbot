//! Telemetry, analytics, and observability for drbot
//!
//! Performance profiling, usage analytics, and A/B testing support.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum TelemetryError {
    #[error("Event recording failed: {0}")]
    RecordingFailed(String),
    #[error("Export failed: {0}")]
    ExportFailed(String),
    #[error("Experiment not found: {0}")]
    ExperimentNotFound(String),
    #[error("Invalid metric: {0}")]
    InvalidMetric(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

pub type Result<T> = std::result::Result<T, TelemetryError>;

// ============================================================================
// Events and Metrics
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub name: String,
    pub category: EventCategory,
    pub properties: HashMap<String, serde_json::Value>,
    pub timestamp: u64,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventCategory {
    User,
    System,
    Performance,
    Error,
    Business,
    Feature,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub value: MetricValue,
    pub tags: HashMap<String, String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricValue {
    Counter(i64),
    Gauge(f64),
    Histogram(f64),
    Summary { sum: f64, count: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDefinition {
    pub name: String,
    pub metric_type: MetricType,
    pub description: String,
    pub unit: Option<String>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
}

// ============================================================================
// Performance Profiling
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub trace_id: String,
    pub spans: Vec<Span>,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub operation_name: String,
    pub service_name: String,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub duration_ms: Option<u64>,
    pub status: SpanStatus,
    pub tags: HashMap<String, String>,
    pub logs: Vec<SpanLog>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SpanStatus {
    Ok,
    Error,
    Unset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanLog {
    pub timestamp: u64,
    pub message: String,
    pub level: LogLevel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub period_start: u64,
    pub period_end: u64,
    pub metrics: PerformanceMetrics,
    pub top_slow_operations: Vec<SlowOperation>,
    pub error_summary: ErrorSummary,
    pub resource_usage: ResourceUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub total_requests: u64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub error_rate: f64,
    pub throughput_per_second: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowOperation {
    pub operation: String,
    pub avg_duration_ms: f64,
    pub max_duration_ms: f64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSummary {
    pub total_errors: u64,
    pub error_types: HashMap<String, u64>,
    pub most_common_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub avg_cpu_percent: f64,
    pub peak_cpu_percent: f64,
    pub avg_memory_mb: u64,
    pub peak_memory_mb: u64,
}

// ============================================================================
// Usage Analytics
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageReport {
    pub period_start: u64,
    pub period_end: u64,
    pub active_users: ActiveUsers,
    pub feature_usage: HashMap<String, FeatureUsage>,
    pub retention: RetentionMetrics,
    pub engagement: EngagementMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveUsers {
    pub daily: u64,
    pub weekly: u64,
    pub monthly: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureUsage {
    pub feature_name: String,
    pub total_uses: u64,
    pub unique_users: u64,
    pub avg_uses_per_user: f64,
    pub trend: Trend,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Trend {
    Increasing,
    Stable,
    Decreasing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionMetrics {
    pub day1: f64,
    pub day7: f64,
    pub day30: f64,
    pub churn_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngagementMetrics {
    pub avg_session_duration_seconds: u64,
    pub avg_sessions_per_user: f64,
    pub avg_actions_per_session: f64,
}

// ============================================================================
// A/B Testing
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: ExperimentStatus,
    pub variants: Vec<Variant>,
    pub targeting: TargetingRules,
    pub metrics: Vec<String>,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub ended_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ExperimentStatus {
    Draft,
    Running,
    Paused,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    pub id: String,
    pub name: String,
    pub weight: f64,
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetingRules {
    pub percentage: f64,
    pub user_segments: Vec<String>,
    pub device_types: Vec<String>,
    pub platforms: Vec<String>,
    pub custom_rules: Vec<CustomRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRule {
    pub attribute: String,
    pub operator: RuleOperator,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RuleOperator {
    Equals,
    NotEquals,
    Contains,
    GreaterThan,
    LessThan,
    In,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentAssignment {
    pub experiment_id: String,
    pub variant_id: String,
    pub user_id: String,
    pub assigned_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentResults {
    pub experiment_id: String,
    pub variant_results: Vec<VariantResult>,
    pub winner: Option<String>,
    pub confidence: f64,
    pub sample_size: u64,
    pub duration_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantResult {
    pub variant_id: String,
    pub variant_name: String,
    pub sample_size: u64,
    pub metrics: HashMap<String, MetricResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricResult {
    pub value: f64,
    pub confidence_interval: (f64, f64),
    pub relative_change: Option<f64>,
    pub is_significant: bool,
}

// ============================================================================
// Provider Trait
// ============================================================================

#[async_trait]
pub trait TelemetryProvider: Send + Sync {
    async fn record_event(&self, event: Event) -> Result<()>;
    async fn record_metric(&self, metric: Metric) -> Result<()>;
    async fn start_trace(&self, operation: &str) -> Result<Trace>;
    async fn export_events(&self, since: u64) -> Result<Vec<Event>>;
    async fn get_experiment(&self, experiment_id: &str) -> Result<Experiment>;
    async fn assign_experiment(
        &self,
        experiment_id: &str,
        user_id: &str,
    ) -> Result<ExperimentAssignment>;
}

// ============================================================================
// Telemetry Engine
// ============================================================================

pub struct TelemetryEngine {
    provider: Arc<dyn TelemetryProvider>,
    events: Arc<RwLock<Vec<Event>>>,
    metrics: Arc<RwLock<HashMap<String, Vec<Metric>>>>,
    traces: Arc<RwLock<HashMap<String, Trace>>>,
    experiments: Arc<RwLock<HashMap<String, Experiment>>>,
    assignments: Arc<RwLock<HashMap<String, ExperimentAssignment>>>,
    counters: Arc<RwLock<HashMap<String, i64>>>,
    gauges: Arc<RwLock<HashMap<String, f64>>>,
    next_event_id: Arc<RwLock<u64>>,
}

impl TelemetryEngine {
    pub fn new(provider: Arc<dyn TelemetryProvider>) -> Self {
        Self {
            provider,
            events: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(HashMap::new())),
            traces: Arc::new(RwLock::new(HashMap::new())),
            experiments: Arc::new(RwLock::new(HashMap::new())),
            assignments: Arc::new(RwLock::new(HashMap::new())),
            counters: Arc::new(RwLock::new(HashMap::new())),
            gauges: Arc::new(RwLock::new(HashMap::new())),
            next_event_id: Arc::new(RwLock::new(1)),
        }
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    async fn generate_event_id(&self) -> String {
        let mut id = self.next_event_id.write().await;
        let event_id = format!("event-{}", *id);
        *id += 1;
        event_id
    }

    // Event Recording
    pub async fn track(
        &self,
        name: &str,
        category: EventCategory,
        properties: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let event = Event {
            id: self.generate_event_id().await,
            name: name.to_string(),
            category,
            properties,
            timestamp: Self::now(),
            user_id: None,
            session_id: None,
            device_id: None,
        };

        self.provider.record_event(event.clone()).await?;

        let mut events = self.events.write().await;
        events.push(event);

        Ok(())
    }

    pub async fn track_user_event(
        &self,
        user_id: &str,
        name: &str,
        properties: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let event = Event {
            id: self.generate_event_id().await,
            name: name.to_string(),
            category: EventCategory::User,
            properties,
            timestamp: Self::now(),
            user_id: Some(user_id.to_string()),
            session_id: None,
            device_id: None,
        };

        self.provider.record_event(event.clone()).await?;

        let mut events = self.events.write().await;
        events.push(event);

        Ok(())
    }

    pub async fn track_error(
        &self,
        error: &str,
        context: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let mut props = context;
        props.insert(
            "error".to_string(),
            serde_json::Value::String(error.to_string()),
        );

        self.track("error", EventCategory::Error, props).await
    }

    pub async fn track_feature(&self, feature: &str, action: &str) -> Result<()> {
        let mut props = HashMap::new();
        props.insert(
            "action".to_string(),
            serde_json::Value::String(action.to_string()),
        );

        self.track(
            &format!("feature.{}", feature),
            EventCategory::Feature,
            props,
        )
        .await
    }

    // Metrics
    pub async fn increment(&self, name: &str, value: i64) -> Result<()> {
        let mut counters = self.counters.write().await;
        let counter = counters.entry(name.to_string()).or_insert(0);
        *counter += value;

        let metric = Metric {
            name: name.to_string(),
            value: MetricValue::Counter(*counter),
            tags: HashMap::new(),
            timestamp: Self::now(),
        };

        self.provider.record_metric(metric).await
    }

    pub async fn decrement(&self, name: &str, value: i64) -> Result<()> {
        self.increment(name, -value).await
    }

    pub async fn gauge(&self, name: &str, value: f64) -> Result<()> {
        {
            let mut gauges = self.gauges.write().await;
            gauges.insert(name.to_string(), value);
        }

        let metric = Metric {
            name: name.to_string(),
            value: MetricValue::Gauge(value),
            tags: HashMap::new(),
            timestamp: Self::now(),
        };

        self.provider.record_metric(metric).await
    }

    pub async fn histogram(&self, name: &str, value: f64) -> Result<()> {
        let metric = Metric {
            name: name.to_string(),
            value: MetricValue::Histogram(value),
            tags: HashMap::new(),
            timestamp: Self::now(),
        };

        let mut metrics = self.metrics.write().await;
        metrics
            .entry(name.to_string())
            .or_default()
            .push(metric.clone());

        self.provider.record_metric(metric).await
    }

    pub async fn timing(&self, name: &str, duration_ms: f64) -> Result<()> {
        self.histogram(&format!("{}.duration_ms", name), duration_ms)
            .await
    }

    // Tracing
    pub async fn start_trace(&self, operation: &str) -> Result<String> {
        let trace = self.provider.start_trace(operation).await?;
        let trace_id = trace.trace_id.clone();

        let mut traces = self.traces.write().await;
        traces.insert(trace_id.clone(), trace);

        Ok(trace_id)
    }

    pub async fn start_span(
        &self,
        trace_id: &str,
        operation: &str,
        parent_span_id: Option<String>,
    ) -> Result<String> {
        let span_id = format!("span-{}", Self::now());

        let span = Span {
            span_id: span_id.clone(),
            parent_span_id,
            operation_name: operation.to_string(),
            service_name: "drbot".to_string(),
            start_time: Self::now(),
            end_time: None,
            duration_ms: None,
            status: SpanStatus::Unset,
            tags: HashMap::new(),
            logs: vec![],
        };

        let mut traces = self.traces.write().await;
        if let Some(trace) = traces.get_mut(trace_id) {
            trace.spans.push(span);
        }

        Ok(span_id)
    }

    pub async fn end_span(&self, trace_id: &str, span_id: &str, status: SpanStatus) -> Result<()> {
        let mut traces = self.traces.write().await;
        if let Some(trace) = traces.get_mut(trace_id) {
            if let Some(span) = trace.spans.iter_mut().find(|s| s.span_id == span_id) {
                let now = Self::now();
                span.end_time = Some(now);
                span.duration_ms = Some((now - span.start_time) * 1000);
                span.status = status;
            }
        }
        Ok(())
    }

    pub async fn end_trace(&self, trace_id: &str) -> Result<Option<Trace>> {
        let mut traces = self.traces.write().await;
        if let Some(trace) = traces.get_mut(trace_id) {
            trace.end_time = Some(Self::now());
            return Ok(Some(trace.clone()));
        }
        Ok(None)
    }

    // A/B Testing
    pub async fn create_experiment(
        &self,
        name: &str,
        variants: Vec<Variant>,
    ) -> Result<Experiment> {
        let experiment = Experiment {
            id: format!("exp-{}", Self::now()),
            name: name.to_string(),
            description: String::new(),
            status: ExperimentStatus::Draft,
            variants,
            targeting: TargetingRules {
                percentage: 100.0,
                user_segments: vec![],
                device_types: vec![],
                platforms: vec![],
                custom_rules: vec![],
            },
            metrics: vec![],
            created_at: Self::now(),
            started_at: None,
            ended_at: None,
        };

        let mut experiments = self.experiments.write().await;
        experiments.insert(experiment.id.clone(), experiment.clone());

        Ok(experiment)
    }

    pub async fn start_experiment(&self, experiment_id: &str) -> Result<()> {
        let mut experiments = self.experiments.write().await;
        let experiment = experiments
            .get_mut(experiment_id)
            .ok_or_else(|| TelemetryError::ExperimentNotFound(experiment_id.to_string()))?;

        experiment.status = ExperimentStatus::Running;
        experiment.started_at = Some(Self::now());

        Ok(())
    }

    pub async fn stop_experiment(&self, experiment_id: &str) -> Result<()> {
        let mut experiments = self.experiments.write().await;
        let experiment = experiments
            .get_mut(experiment_id)
            .ok_or_else(|| TelemetryError::ExperimentNotFound(experiment_id.to_string()))?;

        experiment.status = ExperimentStatus::Completed;
        experiment.ended_at = Some(Self::now());

        Ok(())
    }

    pub async fn get_variant(&self, experiment_id: &str, user_id: &str) -> Result<Variant> {
        // Check existing assignment
        {
            let assignments = self.assignments.read().await;
            let key = format!("{}:{}", experiment_id, user_id);
            if let Some(assignment) = assignments.get(&key) {
                let experiments = self.experiments.read().await;
                if let Some(exp) = experiments.get(experiment_id) {
                    if let Some(variant) =
                        exp.variants.iter().find(|v| v.id == assignment.variant_id)
                    {
                        return Ok(variant.clone());
                    }
                }
            }
        }

        // Create new assignment
        let assignment = self
            .provider
            .assign_experiment(experiment_id, user_id)
            .await?;

        let experiments = self.experiments.read().await;
        let experiment = experiments
            .get(experiment_id)
            .ok_or_else(|| TelemetryError::ExperimentNotFound(experiment_id.to_string()))?;

        let variant = experiment
            .variants
            .iter()
            .find(|v| v.id == assignment.variant_id)
            .cloned()
            .ok_or_else(|| TelemetryError::ExperimentNotFound("Variant not found".to_string()))?;

        // Store assignment
        {
            let mut assignments = self.assignments.write().await;
            let key = format!("{}:{}", experiment_id, user_id);
            assignments.insert(key, assignment);
        }

        Ok(variant)
    }

    pub async fn track_experiment_event(
        &self,
        experiment_id: &str,
        user_id: &str,
        metric: &str,
        value: f64,
    ) -> Result<()> {
        let assignments = self.assignments.read().await;
        let key = format!("{}:{}", experiment_id, user_id);

        if let Some(assignment) = assignments.get(&key) {
            let mut props = HashMap::new();
            props.insert(
                "experiment_id".to_string(),
                serde_json::Value::String(experiment_id.to_string()),
            );
            props.insert(
                "variant_id".to_string(),
                serde_json::Value::String(assignment.variant_id.clone()),
            );
            props.insert(
                "metric".to_string(),
                serde_json::Value::String(metric.to_string()),
            );
            props.insert("value".to_string(), serde_json::json!(value));

            self.track("experiment.metric", EventCategory::Business, props)
                .await?;
        }

        Ok(())
    }

    // Analytics
    pub async fn get_event_count(&self, event_name: &str, since: u64) -> usize {
        let events = self.events.read().await;
        events
            .iter()
            .filter(|e| e.name == event_name && e.timestamp >= since)
            .count()
    }

    pub async fn get_unique_users(&self, since: u64) -> usize {
        let events = self.events.read().await;
        let users: std::collections::HashSet<_> = events
            .iter()
            .filter(|e| e.timestamp >= since && e.user_id.is_some())
            .filter_map(|e| e.user_id.as_ref())
            .collect();
        users.len()
    }

    pub async fn export_events(&self, since: u64) -> Result<Vec<Event>> {
        self.provider.export_events(since).await
    }

    // Helpers
    pub async fn get_counter(&self, name: &str) -> i64 {
        let counters = self.counters.read().await;
        counters.get(name).copied().unwrap_or(0)
    }

    pub async fn get_gauge(&self, name: &str) -> Option<f64> {
        let gauges = self.gauges.read().await;
        gauges.get(name).copied()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        experiments: HashMap<String, Experiment>,
    }

    impl MockProvider {
        fn new() -> Self {
            let mut experiments = HashMap::new();
            experiments.insert(
                "exp-1".to_string(),
                Experiment {
                    id: "exp-1".to_string(),
                    name: "Test Experiment".to_string(),
                    description: "A test".to_string(),
                    status: ExperimentStatus::Running,
                    variants: vec![
                        Variant {
                            id: "control".to_string(),
                            name: "Control".to_string(),
                            weight: 0.5,
                            config: HashMap::new(),
                        },
                        Variant {
                            id: "treatment".to_string(),
                            name: "Treatment".to_string(),
                            weight: 0.5,
                            config: HashMap::new(),
                        },
                    ],
                    targeting: TargetingRules {
                        percentage: 100.0,
                        user_segments: vec![],
                        device_types: vec![],
                        platforms: vec![],
                        custom_rules: vec![],
                    },
                    metrics: vec!["conversion".to_string()],
                    created_at: 0,
                    started_at: Some(0),
                    ended_at: None,
                },
            );
            Self { experiments }
        }
    }

    #[async_trait]
    impl TelemetryProvider for MockProvider {
        async fn record_event(&self, _event: Event) -> Result<()> {
            Ok(())
        }

        async fn record_metric(&self, _metric: Metric) -> Result<()> {
            Ok(())
        }

        async fn start_trace(&self, operation: &str) -> Result<Trace> {
            Ok(Trace {
                trace_id: "trace-1".to_string(),
                spans: vec![Span {
                    span_id: "span-root".to_string(),
                    parent_span_id: None,
                    operation_name: operation.to_string(),
                    service_name: "drbot".to_string(),
                    start_time: 0,
                    end_time: None,
                    duration_ms: None,
                    status: SpanStatus::Unset,
                    tags: HashMap::new(),
                    logs: vec![],
                }],
                start_time: 0,
                end_time: None,
                metadata: HashMap::new(),
            })
        }

        async fn export_events(&self, _since: u64) -> Result<Vec<Event>> {
            Ok(vec![])
        }

        async fn get_experiment(&self, experiment_id: &str) -> Result<Experiment> {
            self.experiments
                .get(experiment_id)
                .cloned()
                .ok_or_else(|| TelemetryError::ExperimentNotFound(experiment_id.to_string()))
        }

        async fn assign_experiment(
            &self,
            experiment_id: &str,
            user_id: &str,
        ) -> Result<ExperimentAssignment> {
            let exp = self
                .experiments
                .get(experiment_id)
                .ok_or_else(|| TelemetryError::ExperimentNotFound(experiment_id.to_string()))?;

            // Simple deterministic assignment based on user_id hash
            let hash: usize = user_id.bytes().map(|b| b as usize).sum();
            let variant_index = hash % exp.variants.len();

            Ok(ExperimentAssignment {
                experiment_id: experiment_id.to_string(),
                variant_id: exp.variants[variant_index].id.clone(),
                user_id: user_id.to_string(),
                assigned_at: 0,
            })
        }
    }

    #[tokio::test]
    async fn test_event_tracking() {
        let provider = Arc::new(MockProvider::new());
        let engine = TelemetryEngine::new(provider);

        engine
            .track("test_event", EventCategory::User, HashMap::new())
            .await
            .unwrap();

        let events = engine.events.read().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "test_event");
    }

    #[tokio::test]
    async fn test_counter_metrics() {
        let provider = Arc::new(MockProvider::new());
        let engine = TelemetryEngine::new(provider);

        engine.increment("requests", 5).await.unwrap();
        assert_eq!(engine.get_counter("requests").await, 5);

        engine.increment("requests", 3).await.unwrap();
        assert_eq!(engine.get_counter("requests").await, 8);

        engine.decrement("requests", 2).await.unwrap();
        assert_eq!(engine.get_counter("requests").await, 6);
    }

    #[tokio::test]
    async fn test_gauge_metrics() {
        let provider = Arc::new(MockProvider::new());
        let engine = TelemetryEngine::new(provider);

        engine.gauge("cpu_usage", 45.5).await.unwrap();
        assert_eq!(engine.get_gauge("cpu_usage").await, Some(45.5));

        engine.gauge("cpu_usage", 62.3).await.unwrap();
        assert_eq!(engine.get_gauge("cpu_usage").await, Some(62.3));
    }

    #[tokio::test]
    async fn test_tracing() {
        let provider = Arc::new(MockProvider::new());
        let engine = TelemetryEngine::new(provider);

        let trace_id = engine.start_trace("main_operation").await.unwrap();
        let span_id = engine
            .start_span(&trace_id, "sub_operation", None)
            .await
            .unwrap();

        engine
            .end_span(&trace_id, &span_id, SpanStatus::Ok)
            .await
            .unwrap();
        let trace = engine.end_trace(&trace_id).await.unwrap();

        assert!(trace.is_some());
        let trace = trace.unwrap();
        assert_eq!(trace.spans.len(), 2); // root + child
    }

    #[tokio::test]
    async fn test_experiment_creation() {
        let provider = Arc::new(MockProvider::new());
        let engine = TelemetryEngine::new(provider);

        let variants = vec![
            Variant {
                id: "a".to_string(),
                name: "A".to_string(),
                weight: 0.5,
                config: HashMap::new(),
            },
            Variant {
                id: "b".to_string(),
                name: "B".to_string(),
                weight: 0.5,
                config: HashMap::new(),
            },
        ];

        let experiment = engine
            .create_experiment("New Feature", variants)
            .await
            .unwrap();
        assert_eq!(experiment.name, "New Feature");
        assert_eq!(experiment.variants.len(), 2);
        assert_eq!(experiment.status, ExperimentStatus::Draft);
    }

    #[tokio::test]
    async fn test_experiment_lifecycle() {
        let provider = Arc::new(MockProvider::new());
        let engine = TelemetryEngine::new(provider);

        let variants = vec![Variant {
            id: "a".to_string(),
            name: "A".to_string(),
            weight: 1.0,
            config: HashMap::new(),
        }];

        let experiment = engine.create_experiment("Test", variants).await.unwrap();

        engine.start_experiment(&experiment.id).await.unwrap();
        let experiments = engine.experiments.read().await;
        assert_eq!(
            experiments.get(&experiment.id).unwrap().status,
            ExperimentStatus::Running
        );
        drop(experiments);

        engine.stop_experiment(&experiment.id).await.unwrap();
        let experiments = engine.experiments.read().await;
        assert_eq!(
            experiments.get(&experiment.id).unwrap().status,
            ExperimentStatus::Completed
        );
    }

    #[tokio::test]
    async fn test_variant_assignment() {
        let provider = Arc::new(MockProvider::new());
        let engine = TelemetryEngine::new(provider);

        // Pre-load the experiment from mock
        {
            let mut experiments = engine.experiments.write().await;
            experiments.insert(
                "exp-1".to_string(),
                Experiment {
                    id: "exp-1".to_string(),
                    name: "Test".to_string(),
                    description: String::new(),
                    status: ExperimentStatus::Running,
                    variants: vec![
                        Variant {
                            id: "control".to_string(),
                            name: "Control".to_string(),
                            weight: 0.5,
                            config: HashMap::new(),
                        },
                        Variant {
                            id: "treatment".to_string(),
                            name: "Treatment".to_string(),
                            weight: 0.5,
                            config: HashMap::new(),
                        },
                    ],
                    targeting: TargetingRules {
                        percentage: 100.0,
                        user_segments: vec![],
                        device_types: vec![],
                        platforms: vec![],
                        custom_rules: vec![],
                    },
                    metrics: vec![],
                    created_at: 0,
                    started_at: Some(0),
                    ended_at: None,
                },
            );
        }

        let variant1 = engine.get_variant("exp-1", "user-1").await.unwrap();
        let variant2 = engine.get_variant("exp-1", "user-1").await.unwrap();

        // Same user should get same variant
        assert_eq!(variant1.id, variant2.id);
    }

    #[tokio::test]
    async fn test_feature_tracking() {
        let provider = Arc::new(MockProvider::new());
        let engine = TelemetryEngine::new(provider);

        engine.track_feature("chat", "opened").await.unwrap();
        engine.track_feature("chat", "message_sent").await.unwrap();

        let events = engine.events.read().await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].category, EventCategory::Feature);
    }

    #[tokio::test]
    async fn test_error_tracking() {
        let provider = Arc::new(MockProvider::new());
        let engine = TelemetryEngine::new(provider);

        let mut context = HashMap::new();
        context.insert(
            "user_id".to_string(),
            serde_json::Value::String("user-1".to_string()),
        );

        engine
            .track_error("Connection timeout", context)
            .await
            .unwrap();

        let events = engine.events.read().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].category, EventCategory::Error);
    }
}
