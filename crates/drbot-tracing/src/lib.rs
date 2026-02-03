//! Distributed tracing for drbot.
//!
//! This crate provides distributed tracing capabilities including:
//! - Span creation and management
//! - Context propagation
//! - Trace exporters
//! - Sampling strategies

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Tracing error types.
#[derive(Error, Debug)]
pub enum TracingError {
    #[error("Span not found: {0}")]
    SpanNotFound(String),

    #[error("Invalid parent span: {0}")]
    InvalidParent(String),

    #[error("Export failed: {0}")]
    ExportFailed(String),

    #[error("Context propagation error: {0}")]
    PropagationError(String),
}

/// Result type for tracing operations.
pub type Result<T> = std::result::Result<T, TracingError>;

/// Trace ID - unique identifier for a distributed trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(u128);

impl TraceId {
    /// Generate a new random trace ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4().as_u128())
    }

    /// Create from bytes.
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(u128::from_be_bytes(bytes))
    }

    /// Convert to bytes.
    pub fn to_bytes(&self) -> [u8; 16] {
        self.0.to_be_bytes()
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        format!("{:032x}", self.0)
    }

    /// Parse from hex string.
    pub fn from_hex(s: &str) -> Option<Self> {
        u128::from_str_radix(s, 16).ok().map(Self)
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Span ID - unique identifier for a span within a trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(u64);

impl SpanId {
    /// Generate a new random span ID.
    pub fn new() -> Self {
        Self(rand_u64())
    }

    /// Create from bytes.
    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        Self(u64::from_be_bytes(bytes))
    }

    /// Convert to bytes.
    pub fn to_bytes(&self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        format!("{:016x}", self.0)
    }

    /// Parse from hex string.
    pub fn from_hex(s: &str) -> Option<Self> {
        u64::from_str_radix(s, 16).ok().map(Self)
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Simple pseudo-random u64 generator.
fn rand_u64() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    time.wrapping_mul(31).wrapping_add(count)
}

/// Span kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanKind {
    /// Internal operation.
    Internal,
    /// Server handling a request.
    Server,
    /// Client making a request.
    Client,
    /// Producer sending a message.
    Producer,
    /// Consumer receiving a message.
    Consumer,
}

impl Default for SpanKind {
    fn default() -> Self {
        Self::Internal
    }
}

/// Span status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanStatus {
    /// Unset status.
    Unset,
    /// Operation completed successfully.
    Ok,
    /// Operation failed with error.
    Error(String),
}

impl Default for SpanStatus {
    fn default() -> Self {
        Self::Unset
    }
}

/// Attribute value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AttributeValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    StringArray(Vec<String>),
    IntArray(Vec<i64>),
}

impl From<&str> for AttributeValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<String> for AttributeValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<i64> for AttributeValue {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<f64> for AttributeValue {
    fn from(f: f64) -> Self {
        Self::Float(f)
    }
}

impl From<bool> for AttributeValue {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

/// A span event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// Event name.
    pub name: String,
    /// Event timestamp.
    pub timestamp: DateTime<Utc>,
    /// Event attributes.
    pub attributes: HashMap<String, AttributeValue>,
}

impl SpanEvent {
    /// Create a new event.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            timestamp: Utc::now(),
            attributes: HashMap::new(),
        }
    }

    /// Add an attribute.
    pub fn with_attribute(
        mut self,
        key: impl Into<String>,
        value: impl Into<AttributeValue>,
    ) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

/// A span link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanLink {
    /// Linked trace ID.
    pub trace_id: TraceId,
    /// Linked span ID.
    pub span_id: SpanId,
    /// Link attributes.
    pub attributes: HashMap<String, AttributeValue>,
}

/// Span data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanData {
    /// Trace ID.
    pub trace_id: TraceId,
    /// Span ID.
    pub span_id: SpanId,
    /// Parent span ID.
    pub parent_span_id: Option<SpanId>,
    /// Span name.
    pub name: String,
    /// Span kind.
    pub kind: SpanKind,
    /// Start time.
    pub start_time: DateTime<Utc>,
    /// End time.
    pub end_time: Option<DateTime<Utc>>,
    /// Status.
    pub status: SpanStatus,
    /// Attributes.
    pub attributes: HashMap<String, AttributeValue>,
    /// Events.
    pub events: Vec<SpanEvent>,
    /// Links.
    pub links: Vec<SpanLink>,
}

impl SpanData {
    /// Get span duration.
    pub fn duration(&self) -> Option<chrono::Duration> {
        self.end_time.map(|end| end - self.start_time)
    }

    /// Get duration in milliseconds.
    pub fn duration_ms(&self) -> Option<i64> {
        self.duration().map(|d| d.num_milliseconds())
    }
}

/// Span builder.
pub struct SpanBuilder {
    trace_id: TraceId,
    parent_span_id: Option<SpanId>,
    name: String,
    kind: SpanKind,
    attributes: HashMap<String, AttributeValue>,
    links: Vec<SpanLink>,
}

impl SpanBuilder {
    /// Create a new span builder.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            trace_id: TraceId::new(),
            parent_span_id: None,
            name: name.into(),
            kind: SpanKind::Internal,
            attributes: HashMap::new(),
            links: Vec::new(),
        }
    }

    /// Set the trace ID.
    pub fn trace_id(mut self, trace_id: TraceId) -> Self {
        self.trace_id = trace_id;
        self
    }

    /// Set the parent span ID.
    pub fn parent(mut self, span_id: SpanId) -> Self {
        self.parent_span_id = Some(span_id);
        self
    }

    /// Set the span kind.
    pub fn kind(mut self, kind: SpanKind) -> Self {
        self.kind = kind;
        self
    }

    /// Add an attribute.
    pub fn attribute(mut self, key: impl Into<String>, value: impl Into<AttributeValue>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Add a link.
    pub fn link(mut self, trace_id: TraceId, span_id: SpanId) -> Self {
        self.links.push(SpanLink {
            trace_id,
            span_id,
            attributes: HashMap::new(),
        });
        self
    }

    /// Start the span.
    pub fn start(self) -> Span {
        let span_id = SpanId::new();
        let data = SpanData {
            trace_id: self.trace_id,
            span_id,
            parent_span_id: self.parent_span_id,
            name: self.name,
            kind: self.kind,
            start_time: Utc::now(),
            end_time: None,
            status: SpanStatus::Unset,
            attributes: self.attributes,
            events: Vec::new(),
            links: self.links,
        };

        Span {
            data: Arc::new(RwLock::new(data)),
        }
    }
}

/// An active span.
#[derive(Clone)]
pub struct Span {
    data: Arc<RwLock<SpanData>>,
}

impl Span {
    /// Create a new root span.
    pub fn new(name: impl Into<String>) -> Self {
        SpanBuilder::new(name).start()
    }

    /// Create a child span.
    pub async fn child(&self, name: impl Into<String>) -> Self {
        let data = self.data.read().await;
        SpanBuilder::new(name)
            .trace_id(data.trace_id)
            .parent(data.span_id)
            .start()
    }

    /// Get the trace ID.
    pub async fn trace_id(&self) -> TraceId {
        self.data.read().await.trace_id
    }

    /// Get the span ID.
    pub async fn span_id(&self) -> SpanId {
        self.data.read().await.span_id
    }

    /// Set an attribute.
    pub async fn set_attribute(&self, key: impl Into<String>, value: impl Into<AttributeValue>) {
        let mut data = self.data.write().await;
        data.attributes.insert(key.into(), value.into());
    }

    /// Add an event.
    pub async fn add_event(&self, event: SpanEvent) {
        let mut data = self.data.write().await;
        data.events.push(event);
    }

    /// Record an exception.
    pub async fn record_exception(&self, error: &str) {
        let event = SpanEvent::new("exception").with_attribute("exception.message", error);
        self.add_event(event).await;
    }

    /// Set the status to OK.
    pub async fn set_ok(&self) {
        let mut data = self.data.write().await;
        data.status = SpanStatus::Ok;
    }

    /// Set the status to error.
    pub async fn set_error(&self, message: impl Into<String>) {
        let mut data = self.data.write().await;
        data.status = SpanStatus::Error(message.into());
    }

    /// End the span.
    pub async fn end(&self) {
        let mut data = self.data.write().await;
        if data.end_time.is_none() {
            data.end_time = Some(Utc::now());
        }
    }

    /// Get the span data.
    pub async fn data(&self) -> SpanData {
        self.data.read().await.clone()
    }
}

/// Trace context for propagation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// Trace ID.
    pub trace_id: TraceId,
    /// Span ID.
    pub span_id: SpanId,
    /// Trace flags.
    pub trace_flags: u8,
    /// Trace state.
    pub trace_state: HashMap<String, String>,
}

impl TraceContext {
    /// Create from a span.
    pub async fn from_span(span: &Span) -> Self {
        let data = span.data.read().await;
        Self {
            trace_id: data.trace_id,
            span_id: data.span_id,
            trace_flags: 0x01, // sampled
            trace_state: HashMap::new(),
        }
    }

    /// Format as W3C traceparent header.
    pub fn to_traceparent(&self) -> String {
        format!(
            "00-{}-{}-{:02x}",
            self.trace_id.to_hex(),
            self.span_id.to_hex(),
            self.trace_flags
        )
    }

    /// Parse W3C traceparent header.
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        let trace_id = TraceId::from_hex(parts[1])?;
        let span_id = SpanId::from_hex(parts[2])?;
        let trace_flags = u8::from_str_radix(parts[3], 16).ok()?;

        Some(Self {
            trace_id,
            span_id,
            trace_flags,
            trace_state: HashMap::new(),
        })
    }

    /// Format as tracestate header.
    pub fn to_tracestate(&self) -> String {
        self.trace_state
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Sampling decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingDecision {
    /// Don't record or sample.
    Drop,
    /// Record but don't sample.
    RecordOnly,
    /// Record and sample.
    RecordAndSample,
}

/// Sampling result.
#[derive(Debug, Clone)]
pub struct SamplingResult {
    /// Decision.
    pub decision: SamplingDecision,
    /// Additional attributes.
    pub attributes: HashMap<String, AttributeValue>,
}

/// Sampler trait.
pub trait Sampler: Send + Sync {
    /// Make a sampling decision.
    fn should_sample(&self, trace_id: &TraceId, name: &str) -> SamplingResult;
}

/// Always sample.
pub struct AlwaysSampler;

impl Sampler for AlwaysSampler {
    fn should_sample(&self, _trace_id: &TraceId, _name: &str) -> SamplingResult {
        SamplingResult {
            decision: SamplingDecision::RecordAndSample,
            attributes: HashMap::new(),
        }
    }
}

/// Never sample.
pub struct NeverSampler;

impl Sampler for NeverSampler {
    fn should_sample(&self, _trace_id: &TraceId, _name: &str) -> SamplingResult {
        SamplingResult {
            decision: SamplingDecision::Drop,
            attributes: HashMap::new(),
        }
    }
}

/// Probability-based sampler.
pub struct ProbabilitySampler {
    ratio: f64,
}

impl ProbabilitySampler {
    /// Create with sampling ratio (0.0 to 1.0).
    pub fn new(ratio: f64) -> Self {
        Self {
            ratio: ratio.clamp(0.0, 1.0),
        }
    }
}

impl Sampler for ProbabilitySampler {
    fn should_sample(&self, trace_id: &TraceId, _name: &str) -> SamplingResult {
        // Use trace ID for deterministic sampling
        let threshold = (self.ratio * u64::MAX as f64) as u64;
        let decision = if (trace_id.0 as u64) < threshold {
            SamplingDecision::RecordAndSample
        } else {
            SamplingDecision::Drop
        };

        SamplingResult {
            decision,
            attributes: HashMap::new(),
        }
    }
}

/// In-memory span collector.
pub struct InMemoryCollector {
    spans: RwLock<Vec<SpanData>>,
    max_spans: usize,
}

impl InMemoryCollector {
    /// Create a new collector.
    pub fn new(max_spans: usize) -> Self {
        Self {
            spans: RwLock::new(Vec::new()),
            max_spans,
        }
    }

    /// Collect a span.
    pub async fn collect(&self, span: &Span) {
        let data = span.data().await;
        let mut spans = self.spans.write().await;

        while spans.len() >= self.max_spans {
            spans.remove(0);
        }

        spans.push(data);
    }

    /// Get all collected spans.
    pub async fn get_spans(&self) -> Vec<SpanData> {
        self.spans.read().await.clone()
    }

    /// Get spans for a specific trace.
    pub async fn get_trace(&self, trace_id: TraceId) -> Vec<SpanData> {
        self.spans
            .read()
            .await
            .iter()
            .filter(|s| s.trace_id == trace_id)
            .cloned()
            .collect()
    }

    /// Clear all spans.
    pub async fn clear(&self) {
        self.spans.write().await.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_id() {
        let id = TraceId::new();
        let hex = id.to_hex();
        assert_eq!(hex.len(), 32);

        let parsed = TraceId::from_hex(&hex).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_span_id() {
        let id = SpanId::new();
        let hex = id.to_hex();
        assert_eq!(hex.len(), 16);

        let parsed = SpanId::from_hex(&hex).unwrap();
        assert_eq!(id, parsed);
    }

    #[tokio::test]
    async fn test_span_creation() {
        let span = Span::new("test-operation");
        let data = span.data().await;

        assert_eq!(data.name, "test-operation");
        assert!(data.parent_span_id.is_none());
        assert!(data.end_time.is_none());
    }

    #[tokio::test]
    async fn test_child_span() {
        let parent = Span::new("parent");
        let child = parent.child("child").await;

        let parent_data = parent.data().await;
        let child_data = child.data().await;

        assert_eq!(parent_data.trace_id, child_data.trace_id);
        assert_eq!(child_data.parent_span_id, Some(parent_data.span_id));
    }

    #[tokio::test]
    async fn test_span_attributes() {
        let span = Span::new("test");
        span.set_attribute("key", "value").await;
        span.set_attribute("count", 42i64).await;

        let data = span.data().await;
        assert_eq!(
            data.attributes.get("key"),
            Some(&AttributeValue::String("value".to_string()))
        );
        assert_eq!(data.attributes.get("count"), Some(&AttributeValue::Int(42)));
    }

    #[tokio::test]
    async fn test_span_events() {
        let span = Span::new("test");
        span.add_event(SpanEvent::new("event1")).await;

        let data = span.data().await;
        assert_eq!(data.events.len(), 1);
        assert_eq!(data.events[0].name, "event1");
    }

    #[tokio::test]
    async fn test_span_end() {
        let span = Span::new("test");
        assert!(span.data().await.end_time.is_none());

        span.end().await;
        assert!(span.data().await.end_time.is_some());
    }

    #[test]
    fn test_traceparent() {
        let ctx = TraceContext {
            trace_id: TraceId(0x0123456789abcdef0123456789abcdef),
            span_id: SpanId(0x0123456789abcdef),
            trace_flags: 0x01,
            trace_state: HashMap::new(),
        };

        let header = ctx.to_traceparent();
        assert!(header.starts_with("00-"));

        let parsed = TraceContext::from_traceparent(&header).unwrap();
        assert_eq!(ctx.trace_id, parsed.trace_id);
        assert_eq!(ctx.span_id, parsed.span_id);
        assert_eq!(ctx.trace_flags, parsed.trace_flags);
    }

    #[test]
    fn test_always_sampler() {
        let sampler = AlwaysSampler;
        let result = sampler.should_sample(&TraceId::new(), "test");
        assert_eq!(result.decision, SamplingDecision::RecordAndSample);
    }

    #[test]
    fn test_never_sampler() {
        let sampler = NeverSampler;
        let result = sampler.should_sample(&TraceId::new(), "test");
        assert_eq!(result.decision, SamplingDecision::Drop);
    }

    #[tokio::test]
    async fn test_collector() {
        let collector = InMemoryCollector::new(100);
        let span = Span::new("test");
        span.end().await;

        collector.collect(&span).await;

        let spans = collector.get_spans().await;
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "test");
    }

    #[tokio::test]
    async fn test_collector_trace() {
        let collector = InMemoryCollector::new(100);

        let parent = Span::new("parent");
        let child = parent.child("child").await;
        let trace_id = parent.trace_id().await;

        parent.end().await;
        child.end().await;

        collector.collect(&parent).await;
        collector.collect(&child).await;

        let trace = collector.get_trace(trace_id).await;
        assert_eq!(trace.len(), 2);
    }
}
