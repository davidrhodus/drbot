//! Span and scope tracking for drbot.
//!
//! This crate provides:
//! - Span creation and management
//! - Nested span tracking
//! - Context propagation
//! - Span events and attributes

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Span error types.
#[derive(Error, Debug)]
pub enum SpanError {
    #[error("Span not found: {0}")]
    NotFound(String),

    #[error("Span already closed")]
    AlreadyClosed,

    #[error("Invalid parent span")]
    InvalidParent,
}

/// Result type for span operations.
pub type Result<T> = std::result::Result<T, SpanError>;

/// Unique span identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanId(u64);

impl SpanId {
    /// Generate new span ID.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    /// Create from raw value.
    pub fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Get raw value.
    pub fn as_raw(&self) -> u64 {
        self.0
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// Trace identifier (groups related spans).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceId(u64);

impl TraceId {
    /// Generate new trace ID.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    /// Create from raw value.
    pub fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// Get raw value.
    pub fn as_raw(&self) -> u64 {
        self.0
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// Span status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanStatus {
    /// Span is active.
    Active,
    /// Span completed successfully.
    Ok,
    /// Span completed with error.
    Error,
    /// Span was cancelled.
    Cancelled,
}

/// Span event.
#[derive(Debug, Clone)]
pub struct SpanEvent {
    /// Event name.
    pub name: String,
    /// Event timestamp.
    pub timestamp: DateTime<Utc>,
    /// Event attributes.
    pub attributes: HashMap<String, SpanValue>,
}

impl SpanEvent {
    /// Create new event.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            timestamp: Utc::now(),
            attributes: HashMap::new(),
        }
    }

    /// Add attribute.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<SpanValue>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

/// Span attribute value.
#[derive(Debug, Clone)]
pub enum SpanValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Bytes(Vec<u8>),
}

impl From<&str> for SpanValue {
    fn from(s: &str) -> Self {
        SpanValue::String(s.to_string())
    }
}

impl From<String> for SpanValue {
    fn from(s: String) -> Self {
        SpanValue::String(s)
    }
}

impl From<i64> for SpanValue {
    fn from(n: i64) -> Self {
        SpanValue::Int(n)
    }
}

impl From<i32> for SpanValue {
    fn from(n: i32) -> Self {
        SpanValue::Int(n as i64)
    }
}

impl From<f64> for SpanValue {
    fn from(n: f64) -> Self {
        SpanValue::Float(n)
    }
}

impl From<bool> for SpanValue {
    fn from(b: bool) -> Self {
        SpanValue::Bool(b)
    }
}

/// Span data.
#[derive(Debug, Clone)]
pub struct SpanData {
    /// Span ID.
    pub id: SpanId,
    /// Trace ID.
    pub trace_id: TraceId,
    /// Parent span ID.
    pub parent_id: Option<SpanId>,
    /// Span name.
    pub name: String,
    /// Start time.
    pub start_time: DateTime<Utc>,
    /// End time.
    pub end_time: Option<DateTime<Utc>>,
    /// Duration.
    pub duration: Option<Duration>,
    /// Status.
    pub status: SpanStatus,
    /// Attributes.
    pub attributes: HashMap<String, SpanValue>,
    /// Events.
    pub events: Vec<SpanEvent>,
}

/// Active span handle.
pub struct Span {
    data: Arc<Mutex<SpanData>>,
    start: Instant,
}

impl Span {
    /// Create new span.
    pub fn new(name: impl Into<String>) -> Self {
        Self::with_trace(TraceId::new(), None, name)
    }

    /// Create span with trace context.
    pub fn with_trace(
        trace_id: TraceId,
        parent_id: Option<SpanId>,
        name: impl Into<String>,
    ) -> Self {
        let data = SpanData {
            id: SpanId::new(),
            trace_id,
            parent_id,
            name: name.into(),
            start_time: Utc::now(),
            end_time: None,
            duration: None,
            status: SpanStatus::Active,
            attributes: HashMap::new(),
            events: Vec::new(),
        };

        Self {
            data: Arc::new(Mutex::new(data)),
            start: Instant::now(),
        }
    }

    /// Create child span.
    pub fn child(&self, name: impl Into<String>) -> Self {
        let data = self.data.lock().unwrap();
        Self::with_trace(data.trace_id, Some(data.id), name)
    }

    /// Get span ID.
    pub fn id(&self) -> SpanId {
        self.data.lock().unwrap().id
    }

    /// Get trace ID.
    pub fn trace_id(&self) -> TraceId {
        self.data.lock().unwrap().trace_id
    }

    /// Get parent ID.
    pub fn parent_id(&self) -> Option<SpanId> {
        self.data.lock().unwrap().parent_id
    }

    /// Get name.
    pub fn name(&self) -> String {
        self.data.lock().unwrap().name.clone()
    }

    /// Set attribute.
    pub fn set_attribute(&self, key: impl Into<String>, value: impl Into<SpanValue>) {
        self.data
            .lock()
            .unwrap()
            .attributes
            .insert(key.into(), value.into());
    }

    /// Add event.
    pub fn add_event(&self, event: SpanEvent) {
        self.data.lock().unwrap().events.push(event);
    }

    /// Add named event.
    pub fn event(&self, name: impl Into<String>) {
        self.add_event(SpanEvent::new(name));
    }

    /// Set status.
    pub fn set_status(&self, status: SpanStatus) {
        self.data.lock().unwrap().status = status;
    }

    /// Mark as error.
    pub fn set_error(&self, message: impl Into<String>) {
        let mut data = self.data.lock().unwrap();
        data.status = SpanStatus::Error;
        data.attributes
            .insert("error.message".into(), SpanValue::String(message.into()));
    }

    /// Get elapsed time.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// End the span.
    pub fn end(self) -> SpanData {
        self.end_with_status(SpanStatus::Ok)
    }

    /// End with specific status.
    pub fn end_with_status(self, status: SpanStatus) -> SpanData {
        let duration = self.start.elapsed();
        let mut data = self.data.lock().unwrap();

        if data.status == SpanStatus::Active {
            data.status = status;
        }
        data.end_time = Some(Utc::now());
        data.duration = Some(duration);

        data.clone()
    }

    /// Get current data (clone).
    pub fn data(&self) -> SpanData {
        self.data.lock().unwrap().clone()
    }
}

/// Span context for propagation.
#[derive(Debug, Clone)]
pub struct SpanContext {
    /// Trace ID.
    pub trace_id: TraceId,
    /// Span ID.
    pub span_id: SpanId,
    /// Baggage items.
    pub baggage: HashMap<String, String>,
}

impl SpanContext {
    /// Create from span.
    pub fn from_span(span: &Span) -> Self {
        let data = span.data.lock().unwrap();
        Self {
            trace_id: data.trace_id,
            span_id: data.id,
            baggage: HashMap::new(),
        }
    }

    /// Create new root context.
    pub fn new_root() -> Self {
        Self {
            trace_id: TraceId::new(),
            span_id: SpanId::new(),
            baggage: HashMap::new(),
        }
    }

    /// Set baggage item.
    pub fn set_baggage(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.baggage.insert(key.into(), value.into());
    }

    /// Get baggage item.
    pub fn get_baggage(&self, key: &str) -> Option<&str> {
        self.baggage.get(key).map(|s| s.as_str())
    }

    /// Encode as header value.
    pub fn to_header(&self) -> String {
        format!("{}-{}", self.trace_id, self.span_id)
    }

    /// Parse from header value.
    pub fn from_header(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() >= 2 {
            let trace_id = u64::from_str_radix(parts[0], 16).ok()?;
            let span_id = u64::from_str_radix(parts[1], 16).ok()?;
            Some(Self {
                trace_id: TraceId::from_raw(trace_id),
                span_id: SpanId::from_raw(span_id),
                baggage: HashMap::new(),
            })
        } else {
            None
        }
    }
}

/// Span collector.
#[derive(Debug, Default)]
pub struct SpanCollector {
    spans: Mutex<Vec<SpanData>>,
}

impl SpanCollector {
    /// Create new collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Collect a completed span.
    pub fn collect(&self, span: SpanData) {
        self.spans.lock().unwrap().push(span);
    }

    /// Get all collected spans.
    pub fn spans(&self) -> Vec<SpanData> {
        self.spans.lock().unwrap().clone()
    }

    /// Clear collected spans.
    pub fn clear(&self) {
        self.spans.lock().unwrap().clear();
    }

    /// Get spans by trace ID.
    pub fn by_trace(&self, trace_id: TraceId) -> Vec<SpanData> {
        self.spans
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.trace_id == trace_id)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_span_creation() {
        let span = Span::new("test-span");
        assert!(!span.name().is_empty());
        assert!(span.parent_id().is_none());
    }

    #[test]
    fn test_span_child() {
        let parent = Span::new("parent");
        let parent_id = parent.id();
        let trace_id = parent.trace_id();

        let child = parent.child("child");
        assert_eq!(child.parent_id(), Some(parent_id));
        assert_eq!(child.trace_id(), trace_id);
    }

    #[test]
    fn test_span_attributes() {
        let span = Span::new("test");
        span.set_attribute("key", "value");
        span.set_attribute("count", 42i64);

        let data = span.data();
        assert!(data.attributes.contains_key("key"));
        assert!(data.attributes.contains_key("count"));
    }

    #[test]
    fn test_span_events() {
        let span = Span::new("test");
        span.event("checkpoint-1");
        span.add_event(SpanEvent::new("checkpoint-2").with_attribute("value", 100i64));

        let data = span.data();
        assert_eq!(data.events.len(), 2);
    }

    #[test]
    fn test_span_end() {
        let span = Span::new("test");
        thread::sleep(std::time::Duration::from_millis(5));
        let data = span.end();

        assert_eq!(data.status, SpanStatus::Ok);
        assert!(data.duration.is_some());
        assert!(data.duration.unwrap() >= std::time::Duration::from_millis(5));
    }

    #[test]
    fn test_span_context() {
        let span = Span::new("test");
        let mut ctx = SpanContext::from_span(&span);
        ctx.set_baggage("user_id", "123");

        let header = ctx.to_header();
        let parsed = SpanContext::from_header(&header).unwrap();

        assert_eq!(parsed.trace_id, ctx.trace_id);
        assert_eq!(parsed.span_id, ctx.span_id);
    }

    #[test]
    fn test_collector() {
        let collector = SpanCollector::new();

        let span1 = Span::new("span1");
        let trace_id = span1.trace_id();
        collector.collect(span1.end());

        let span2 = Span::new("span2");
        collector.collect(span2.end());

        assert_eq!(collector.spans().len(), 2);
        assert_eq!(collector.by_trace(trace_id).len(), 1);
    }
}
