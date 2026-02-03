//! AI observability and debugging for drbot.
//!
//! Provides comprehensive visibility into AI operations.
//!
//! # Features
//!
//! - Distributed tracing across agent chains
//! - Token counting and cost tracking
//! - Latency monitoring and histograms
//! - Request/response logging
//! - Performance profiling
//! - Error tracking and alerting

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Observability result type.
pub type Result<T> = std::result::Result<T, ObserveError>;

/// Observability errors.
#[derive(Debug, thiserror::Error)]
pub enum ObserveError {
    #[error("Trace not found: {0}")]
    TraceNotFound(Uuid),
    #[error("Span not found: {0}")]
    SpanNotFound(Uuid),
    #[error("Metric not found: {0}")]
    MetricNotFound(String),
}

/// A distributed trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    /// Trace ID.
    pub id: Uuid,
    /// Root span ID.
    pub root_span_id: Uuid,
    /// All spans in this trace.
    pub spans: Vec<Span>,
    /// Trace-level metadata.
    pub metadata: HashMap<String, String>,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Ended at.
    pub ended_at: Option<DateTime<Utc>>,
    /// Total duration (ms).
    pub duration_ms: Option<u64>,
}

impl Trace {
    /// Create a new trace.
    pub fn new() -> Self {
        let root_span = Span::new("root");
        let trace_id = Uuid::new_v4();

        Self {
            id: trace_id,
            root_span_id: root_span.id,
            spans: vec![root_span],
            metadata: HashMap::new(),
            started_at: Utc::now(),
            ended_at: None,
            duration_ms: None,
        }
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// End the trace.
    pub fn end(&mut self) {
        let now = Utc::now();
        self.ended_at = Some(now);
        self.duration_ms = Some((now - self.started_at).num_milliseconds() as u64);
    }

    /// Get total token count.
    pub fn total_tokens(&self) -> TokenCount {
        let mut total = TokenCount::default();
        for span in &self.spans {
            if let Some(tokens) = &span.tokens {
                total.input += tokens.input;
                total.output += tokens.output;
                total.total += tokens.total;
            }
        }
        total
    }

    /// Get total cost.
    pub fn total_cost(&self) -> f64 {
        self.spans.iter().filter_map(|s| s.cost).sum()
    }
}

impl Default for Trace {
    fn default() -> Self {
        Self::new()
    }
}

/// A span within a trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    /// Span ID.
    pub id: Uuid,
    /// Parent span ID.
    pub parent_id: Option<Uuid>,
    /// Span name/operation.
    pub name: String,
    /// Span kind.
    pub kind: SpanKind,
    /// Span status.
    pub status: SpanStatus,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Ended at.
    pub ended_at: Option<DateTime<Utc>>,
    /// Duration (ms).
    pub duration_ms: Option<u64>,
    /// Token counts.
    pub tokens: Option<TokenCount>,
    /// Cost in USD.
    pub cost: Option<f64>,
    /// Model used.
    pub model: Option<String>,
    /// Provider used.
    pub provider: Option<String>,
    /// Attributes.
    pub attributes: HashMap<String, serde_json::Value>,
    /// Events within the span.
    pub events: Vec<SpanEvent>,
}

impl Span {
    /// Create a new span.
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            parent_id: None,
            name: name.to_string(),
            kind: SpanKind::Internal,
            status: SpanStatus::Unset,
            started_at: Utc::now(),
            ended_at: None,
            duration_ms: None,
            tokens: None,
            cost: None,
            model: None,
            provider: None,
            attributes: HashMap::new(),
            events: Vec::new(),
        }
    }

    /// Set parent span.
    pub fn with_parent(mut self, parent_id: Uuid) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Set span kind.
    pub fn with_kind(mut self, kind: SpanKind) -> Self {
        self.kind = kind;
        self
    }

    /// Set model.
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = Some(model.to_string());
        self
    }

    /// Set provider.
    pub fn with_provider(mut self, provider: &str) -> Self {
        self.provider = Some(provider.to_string());
        self
    }

    /// Add attribute.
    pub fn set_attribute(&mut self, key: &str, value: serde_json::Value) {
        self.attributes.insert(key.to_string(), value);
    }

    /// Add event.
    pub fn add_event(&mut self, name: &str, attributes: HashMap<String, serde_json::Value>) {
        self.events.push(SpanEvent {
            name: name.to_string(),
            timestamp: Utc::now(),
            attributes,
        });
    }

    /// Set tokens.
    pub fn set_tokens(&mut self, input: u64, output: u64) {
        self.tokens = Some(TokenCount {
            input,
            output,
            total: input + output,
        });
    }

    /// Set cost.
    pub fn set_cost(&mut self, cost: f64) {
        self.cost = Some(cost);
    }

    /// End the span.
    pub fn end(&mut self) {
        let now = Utc::now();
        self.ended_at = Some(now);
        self.duration_ms = Some((now - self.started_at).num_milliseconds() as u64);
    }

    /// End with status.
    pub fn end_with_status(&mut self, status: SpanStatus) {
        self.status = status;
        self.end();
    }
}

/// Span kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    /// Internal operation.
    Internal,
    /// LLM call.
    LlmCall,
    /// Tool call.
    ToolCall,
    /// Embedding generation.
    Embedding,
    /// Retrieval operation.
    Retrieval,
    /// Agent operation.
    Agent,
    /// HTTP request.
    Http,
}

/// Span status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    /// Not set.
    Unset,
    /// Completed successfully.
    Ok,
    /// Error occurred.
    Error,
}

/// Event within a span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// Event name.
    pub name: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Event attributes.
    pub attributes: HashMap<String, serde_json::Value>,
}

/// Token count.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenCount {
    /// Input tokens.
    pub input: u64,
    /// Output tokens.
    pub output: u64,
    /// Total tokens.
    pub total: u64,
}

/// Metric types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Metric {
    /// Counter (monotonically increasing).
    Counter(CounterMetric),
    /// Gauge (point-in-time value).
    Gauge(GaugeMetric),
    /// Histogram (distribution of values).
    Histogram(HistogramMetric),
}

/// Counter metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterMetric {
    /// Metric name.
    pub name: String,
    /// Current value.
    pub value: u64,
    /// Labels.
    pub labels: HashMap<String, String>,
}

/// Gauge metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaugeMetric {
    /// Metric name.
    pub name: String,
    /// Current value.
    pub value: f64,
    /// Labels.
    pub labels: HashMap<String, String>,
}

/// Histogram metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramMetric {
    /// Metric name.
    pub name: String,
    /// Bucket boundaries.
    pub buckets: Vec<f64>,
    /// Bucket counts.
    pub counts: Vec<u64>,
    /// Sum of all values.
    pub sum: f64,
    /// Total count.
    pub count: u64,
    /// Labels.
    pub labels: HashMap<String, String>,
}

impl HistogramMetric {
    /// Create a new histogram with default buckets.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            buckets: vec![
                5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0,
            ],
            counts: vec![0; 10],
            sum: 0.0,
            count: 0,
            labels: HashMap::new(),
        }
    }

    /// Record a value.
    pub fn observe(&mut self, value: f64) {
        self.sum += value;
        self.count += 1;

        for (i, bucket) in self.buckets.iter().enumerate() {
            if value <= *bucket {
                self.counts[i] += 1;
                break;
            }
        }
    }

    /// Get percentile (approximate).
    pub fn percentile(&self, p: f64) -> f64 {
        if self.count == 0 {
            return 0.0;
        }

        let target = (self.count as f64 * p) as u64;
        let mut cumulative = 0u64;

        for (i, count) in self.counts.iter().enumerate() {
            cumulative += count;
            if cumulative >= target {
                return self.buckets[i];
            }
        }

        *self.buckets.last().unwrap_or(&0.0)
    }
}

/// Observability configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserveConfig {
    /// Enable tracing.
    pub enable_tracing: bool,
    /// Enable metrics.
    pub enable_metrics: bool,
    /// Sample rate (0.0 - 1.0).
    pub sample_rate: f64,
    /// Max traces to keep.
    pub max_traces: usize,
    /// Export interval (seconds).
    pub export_interval_secs: u64,
    /// Log requests/responses.
    pub log_requests: bool,
    /// Log sensitive data.
    pub log_sensitive: bool,
}

impl Default for ObserveConfig {
    fn default() -> Self {
        Self {
            enable_tracing: true,
            enable_metrics: true,
            sample_rate: 1.0,
            max_traces: 1000,
            export_interval_secs: 60,
            log_requests: true,
            log_sensitive: false,
        }
    }
}

/// Observer for AI operations.
pub struct Observer {
    config: ObserveConfig,
    traces: Arc<RwLock<HashMap<Uuid, Trace>>>,
    metrics: Arc<RwLock<HashMap<String, Metric>>>,
    counters: Arc<RwLock<HashMap<String, AtomicU64>>>,
}

impl Observer {
    /// Create a new observer.
    pub fn new(config: ObserveConfig) -> Self {
        Self {
            config,
            traces: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(HashMap::new())),
            counters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a new trace.
    pub async fn start_trace(&self) -> Uuid {
        let trace = Trace::new();
        let trace_id = trace.id;

        let mut traces = self.traces.write().await;

        // Enforce max traces
        if traces.len() >= self.config.max_traces {
            // Remove oldest trace
            if let Some(oldest_id) = traces
                .iter()
                .min_by_key(|(_, t)| t.started_at)
                .map(|(id, _)| *id)
            {
                traces.remove(&oldest_id);
            }
        }

        traces.insert(trace_id, trace);
        trace_id
    }

    /// Add span to trace.
    pub async fn add_span(&self, trace_id: Uuid, span: Span) -> Result<Uuid> {
        let span_id = span.id;
        let mut traces = self.traces.write().await;

        if let Some(trace) = traces.get_mut(&trace_id) {
            trace.spans.push(span);
            Ok(span_id)
        } else {
            Err(ObserveError::TraceNotFound(trace_id))
        }
    }

    /// Update span.
    pub async fn update_span<F>(&self, trace_id: Uuid, span_id: Uuid, f: F) -> Result<()>
    where
        F: FnOnce(&mut Span),
    {
        let mut traces = self.traces.write().await;

        if let Some(trace) = traces.get_mut(&trace_id) {
            if let Some(span) = trace.spans.iter_mut().find(|s| s.id == span_id) {
                f(span);
                Ok(())
            } else {
                Err(ObserveError::SpanNotFound(span_id))
            }
        } else {
            Err(ObserveError::TraceNotFound(trace_id))
        }
    }

    /// End trace.
    pub async fn end_trace(&self, trace_id: Uuid) -> Result<()> {
        let mut traces = self.traces.write().await;

        if let Some(trace) = traces.get_mut(&trace_id) {
            trace.end();
            Ok(())
        } else {
            Err(ObserveError::TraceNotFound(trace_id))
        }
    }

    /// Get trace.
    pub async fn get_trace(&self, trace_id: Uuid) -> Option<Trace> {
        self.traces.read().await.get(&trace_id).cloned()
    }

    /// Increment counter.
    pub async fn increment(&self, name: &str, value: u64) {
        let counters = self.counters.read().await;

        if let Some(counter) = counters.get(name) {
            counter.fetch_add(value, Ordering::Relaxed);
        } else {
            drop(counters);
            let mut counters = self.counters.write().await;
            counters
                .entry(name.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(value, Ordering::Relaxed);
        }
    }

    /// Record histogram observation.
    pub async fn observe_histogram(&self, name: &str, value: f64) {
        let mut metrics = self.metrics.write().await;

        let metric = metrics
            .entry(name.to_string())
            .or_insert_with(|| Metric::Histogram(HistogramMetric::new(name)));

        if let Metric::Histogram(h) = metric {
            h.observe(value);
        }
    }

    /// Set gauge value.
    pub async fn set_gauge(&self, name: &str, value: f64) {
        let mut metrics = self.metrics.write().await;

        metrics.insert(
            name.to_string(),
            Metric::Gauge(GaugeMetric {
                name: name.to_string(),
                value,
                labels: HashMap::new(),
            }),
        );
    }

    /// Get all metrics.
    pub async fn get_metrics(&self) -> Vec<Metric> {
        self.metrics.read().await.values().cloned().collect()
    }

    /// Get recent traces.
    pub async fn recent_traces(&self, limit: usize) -> Vec<Trace> {
        let traces = self.traces.read().await;
        let mut traces: Vec<_> = traces.values().cloned().collect();
        traces.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        traces.truncate(limit);
        traces
    }

    /// Get aggregate statistics.
    pub async fn stats(&self) -> ObserverStats {
        let traces = self.traces.read().await;

        let mut total_tokens = TokenCount::default();
        let mut total_cost = 0.0;
        let mut total_latency_ms = 0u64;
        let mut completed_count = 0u64;
        let mut error_count = 0u64;

        for trace in traces.values() {
            let tokens = trace.total_tokens();
            total_tokens.input += tokens.input;
            total_tokens.output += tokens.output;
            total_tokens.total += tokens.total;

            total_cost += trace.total_cost();

            if let Some(duration) = trace.duration_ms {
                total_latency_ms += duration;
            }

            for span in &trace.spans {
                if span.status == SpanStatus::Ok {
                    completed_count += 1;
                } else if span.status == SpanStatus::Error {
                    error_count += 1;
                }
            }
        }

        let trace_count = traces.len() as u64;
        let avg_latency_ms = if trace_count > 0 {
            total_latency_ms / trace_count
        } else {
            0
        };

        ObserverStats {
            trace_count,
            total_tokens,
            total_cost,
            avg_latency_ms,
            completed_count,
            error_count,
        }
    }
}

/// Observer statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverStats {
    /// Total trace count.
    pub trace_count: u64,
    /// Total tokens.
    pub total_tokens: TokenCount,
    /// Total cost.
    pub total_cost: f64,
    /// Average latency (ms).
    pub avg_latency_ms: u64,
    /// Completed operations.
    pub completed_count: u64,
    /// Error count.
    pub error_count: u64,
}

/// Convenience macro for starting a traced operation.
#[macro_export]
macro_rules! trace_operation {
    ($observer:expr, $name:expr, $body:block) => {{
        let trace_id = $observer.start_trace().await;
        let span = Span::new($name);
        let span_id = $observer.add_span(trace_id, span).await?;

        let result = $body;

        $observer
            .update_span(trace_id, span_id, |s| {
                s.end_with_status(SpanStatus::Ok);
            })
            .await?;
        $observer.end_trace(trace_id).await?;

        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_trace_creation() {
        let observer = Observer::new(ObserveConfig::default());

        let trace_id = observer.start_trace().await;
        let trace = observer.get_trace(trace_id).await.unwrap();

        assert_eq!(trace.spans.len(), 1); // Root span
    }

    #[tokio::test]
    async fn test_span_operations() {
        let observer = Observer::new(ObserveConfig::default());

        let trace_id = observer.start_trace().await;

        let span = Span::new("llm_call")
            .with_kind(SpanKind::LlmCall)
            .with_model("claude-3-opus")
            .with_provider("anthropic");

        let span_id = observer.add_span(trace_id, span).await.unwrap();

        observer
            .update_span(trace_id, span_id, |s| {
                s.set_tokens(1000, 500);
                s.set_cost(0.05);
                s.end_with_status(SpanStatus::Ok);
            })
            .await
            .unwrap();

        let trace = observer.get_trace(trace_id).await.unwrap();
        let tokens = trace.total_tokens();

        assert_eq!(tokens.input, 1000);
        assert_eq!(tokens.output, 500);
    }

    #[tokio::test]
    async fn test_histogram() {
        let observer = Observer::new(ObserveConfig::default());

        for value in [10.0, 25.0, 50.0, 100.0, 200.0] {
            observer.observe_histogram("latency_ms", value).await;
        }

        let metrics = observer.get_metrics().await;
        assert_eq!(metrics.len(), 1);

        if let Some(Metric::Histogram(h)) = metrics.first() {
            assert_eq!(h.count, 5);
        }
    }

    #[tokio::test]
    async fn test_counter() {
        let observer = Observer::new(ObserveConfig::default());

        observer.increment("requests_total", 1).await;
        observer.increment("requests_total", 1).await;
        observer.increment("requests_total", 1).await;

        // Counter is stored in AtomicU64, verify it exists
        let counters = observer.counters.read().await;
        assert!(counters.contains_key("requests_total"));
    }
}
