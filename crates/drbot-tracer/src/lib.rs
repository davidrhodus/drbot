//! Distributed tracing utilities for drbot.
//!
//! This crate provides:
//! - Trace context propagation
//! - Sampling strategies
//! - Trace exporters
//! - OpenTelemetry-compatible data structures

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;

/// Tracer error types.
#[derive(Error, Debug)]
pub enum TracerError {
    #[error("Trace not found: {0}")]
    TraceNotFound(String),

    #[error("Invalid trace context")]
    InvalidContext,

    #[error("Export failed: {0}")]
    ExportFailed(String),

    #[error("Sampler error: {0}")]
    SamplerError(String),
}

/// Result type for tracer operations.
pub type Result<T> = std::result::Result<T, TracerError>;

/// Trace ID (128-bit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId {
    high: u64,
    low: u64,
}

impl TraceId {
    /// Generate new trace ID.
    pub fn new() -> Self {
        static HIGH: AtomicU64 = AtomicU64::new(1);
        static LOW: AtomicU64 = AtomicU64::new(1);

        Self {
            high: HIGH.fetch_add(1, Ordering::SeqCst),
            low: LOW.fetch_add(1, Ordering::SeqCst),
        }
    }

    /// Create from bytes.
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        let high = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let low = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        Self { high, low }
    }

    /// Convert to bytes.
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..8].copy_from_slice(&self.high.to_be_bytes());
        bytes[8..16].copy_from_slice(&self.low.to_be_bytes());
        bytes
    }

    /// Check if valid (non-zero).
    pub fn is_valid(&self) -> bool {
        self.high != 0 || self.low != 0
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:016x}{:016x}", self.high, self.low)
    }
}

/// Span ID (64-bit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(u64);

impl SpanId {
    /// Generate new span ID.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    /// Create from bytes.
    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        Self(u64::from_be_bytes(bytes))
    }

    /// Convert to bytes.
    pub fn to_bytes(&self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    /// Check if valid (non-zero).
    pub fn is_valid(&self) -> bool {
        self.0 != 0
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

/// Trace flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceFlags(u8);

impl TraceFlags {
    /// No flags.
    pub const NONE: Self = Self(0);
    /// Sampled flag.
    pub const SAMPLED: Self = Self(1);

    /// Check if sampled.
    pub fn is_sampled(&self) -> bool {
        self.0 & 1 != 0
    }

    /// Set sampled flag.
    pub fn set_sampled(&mut self, sampled: bool) {
        if sampled {
            self.0 |= 1;
        } else {
            self.0 &= !1;
        }
    }
}

impl Default for TraceFlags {
    fn default() -> Self {
        Self::NONE
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
    pub flags: TraceFlags,
    /// Trace state (vendor-specific).
    pub trace_state: HashMap<String, String>,
}

impl TraceContext {
    /// Create new context.
    pub fn new() -> Self {
        Self {
            trace_id: TraceId::new(),
            span_id: SpanId::new(),
            flags: TraceFlags::SAMPLED,
            trace_state: HashMap::new(),
        }
    }

    /// Create child context.
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id,
            span_id: SpanId::new(),
            flags: self.flags,
            trace_state: self.trace_state.clone(),
        }
    }

    /// Encode as W3C traceparent header.
    pub fn to_traceparent(&self) -> String {
        format!("00-{}-{}-{:02x}", self.trace_id, self.span_id, self.flags.0)
    }

    /// Parse from W3C traceparent header.
    pub fn from_traceparent(header: &str) -> Result<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() < 4 || parts[0] != "00" {
            return Err(TracerError::InvalidContext);
        }

        let trace_bytes = hex_decode(parts[1]).map_err(|_| TracerError::InvalidContext)?;
        if trace_bytes.len() != 16 {
            return Err(TracerError::InvalidContext);
        }

        let span_bytes = hex_decode(parts[2]).map_err(|_| TracerError::InvalidContext)?;
        if span_bytes.len() != 8 {
            return Err(TracerError::InvalidContext);
        }

        let flags = u8::from_str_radix(parts[3], 16).map_err(|_| TracerError::InvalidContext)?;

        Ok(Self {
            trace_id: TraceId::from_bytes(trace_bytes.try_into().unwrap()),
            span_id: SpanId::from_bytes(span_bytes.try_into().unwrap()),
            flags: TraceFlags(flags),
            trace_state: HashMap::new(),
        })
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Decode hex string to bytes.
fn hex_decode(s: &str) -> std::result::Result<Vec<u8>, ()> {
    if s.len() % 2 != 0 {
        return Err(());
    }

    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

/// Span kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpanKind {
    /// Internal operation.
    Internal,
    /// Server handling request.
    Server,
    /// Client making request.
    Client,
    /// Producer sending message.
    Producer,
    /// Consumer receiving message.
    Consumer,
}

impl Default for SpanKind {
    fn default() -> Self {
        Self::Internal
    }
}

/// Span status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusCode {
    Unset,
    Ok,
    Error,
}

impl Default for StatusCode {
    fn default() -> Self {
        Self::Unset
    }
}

/// Span data for export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanData {
    /// Context.
    pub context: TraceContext,
    /// Parent span ID.
    pub parent_span_id: Option<SpanId>,
    /// Name.
    pub name: String,
    /// Kind.
    pub kind: SpanKind,
    /// Start time.
    pub start_time: DateTime<Utc>,
    /// End time.
    pub end_time: DateTime<Utc>,
    /// Duration.
    pub duration: Duration,
    /// Status code.
    pub status: StatusCode,
    /// Status message.
    pub status_message: Option<String>,
    /// Attributes.
    pub attributes: HashMap<String, serde_json::Value>,
    /// Events.
    pub events: Vec<SpanEvent>,
    /// Links.
    pub links: Vec<SpanLink>,
}

/// Span event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEvent {
    /// Name.
    pub name: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Attributes.
    pub attributes: HashMap<String, serde_json::Value>,
}

/// Link to another span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanLink {
    /// Linked trace ID.
    pub trace_id: TraceId,
    /// Linked span ID.
    pub span_id: SpanId,
    /// Attributes.
    pub attributes: HashMap<String, serde_json::Value>,
}

/// Sampling decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingDecision {
    /// Don't record.
    Drop,
    /// Record but don't sample.
    RecordOnly,
    /// Record and sample.
    RecordAndSample,
}

/// Sampler trait.
pub trait Sampler: Send + Sync {
    /// Make sampling decision.
    fn should_sample(&self, context: &TraceContext, name: &str) -> SamplingDecision;
}

/// Always sample.
#[derive(Debug, Clone, Default)]
pub struct AlwaysSampler;

impl Sampler for AlwaysSampler {
    fn should_sample(&self, _context: &TraceContext, _name: &str) -> SamplingDecision {
        SamplingDecision::RecordAndSample
    }
}

/// Never sample.
#[derive(Debug, Clone, Default)]
pub struct NeverSampler;

impl Sampler for NeverSampler {
    fn should_sample(&self, _context: &TraceContext, _name: &str) -> SamplingDecision {
        SamplingDecision::Drop
    }
}

/// Ratio-based sampler.
#[derive(Debug, Clone)]
pub struct RatioSampler {
    ratio: f64,
}

impl RatioSampler {
    /// Create with ratio (0.0 to 1.0).
    pub fn new(ratio: f64) -> Self {
        Self {
            ratio: ratio.clamp(0.0, 1.0),
        }
    }
}

impl Sampler for RatioSampler {
    fn should_sample(&self, context: &TraceContext, _name: &str) -> SamplingDecision {
        // Use trace ID for deterministic sampling
        let threshold = (self.ratio * u64::MAX as f64) as u64;
        if context.trace_id.low <= threshold {
            SamplingDecision::RecordAndSample
        } else {
            SamplingDecision::Drop
        }
    }
}

/// Span exporter trait.
pub trait SpanExporter: Send + Sync {
    /// Export spans.
    fn export(&self, spans: &[SpanData]) -> Result<()>;
}

/// Console exporter (prints to stderr).
#[derive(Debug, Default)]
pub struct ConsoleExporter;

impl SpanExporter for ConsoleExporter {
    fn export(&self, spans: &[SpanData]) -> Result<()> {
        for span in spans {
            eprintln!(
                "[{}] {} {} ({:?}) - {}ms",
                span.context.trace_id,
                span.context.span_id,
                span.name,
                span.kind,
                span.duration.as_millis()
            );
        }
        Ok(())
    }
}

/// In-memory exporter for testing.
#[derive(Debug, Default)]
pub struct InMemoryExporter {
    spans: Arc<Mutex<Vec<SpanData>>>,
}

impl InMemoryExporter {
    /// Create new exporter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get exported spans.
    pub fn spans(&self) -> Vec<SpanData> {
        self.spans.lock().unwrap().clone()
    }

    /// Clear exported spans.
    pub fn clear(&self) {
        self.spans.lock().unwrap().clear();
    }
}

impl SpanExporter for InMemoryExporter {
    fn export(&self, spans: &[SpanData]) -> Result<()> {
        self.spans.lock().unwrap().extend(spans.iter().cloned());
        Ok(())
    }
}

/// Tracer configuration.
#[derive(Debug, Clone)]
pub struct TracerConfig {
    /// Service name.
    pub service_name: String,
    /// Service version.
    pub service_version: Option<String>,
    /// Max attributes per span.
    pub max_attributes: usize,
    /// Max events per span.
    pub max_events: usize,
    /// Max links per span.
    pub max_links: usize,
}

impl Default for TracerConfig {
    fn default() -> Self {
        Self {
            service_name: "unknown".to_string(),
            service_version: None,
            max_attributes: 128,
            max_events: 128,
            max_links: 128,
        }
    }
}

impl TracerConfig {
    /// Create with service name.
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_id() {
        let id = TraceId::new();
        assert!(id.is_valid());

        let bytes = id.to_bytes();
        let id2 = TraceId::from_bytes(bytes);
        assert_eq!(id, id2);
    }

    #[test]
    fn test_span_id() {
        let id = SpanId::new();
        assert!(id.is_valid());

        let bytes = id.to_bytes();
        let id2 = SpanId::from_bytes(bytes);
        assert_eq!(id, id2);
    }

    #[test]
    fn test_trace_context() {
        let ctx = TraceContext::new();
        let traceparent = ctx.to_traceparent();

        let parsed = TraceContext::from_traceparent(&traceparent).unwrap();
        assert_eq!(parsed.trace_id, ctx.trace_id);
        assert_eq!(parsed.span_id, ctx.span_id);
    }

    #[test]
    fn test_trace_context_child() {
        let parent = TraceContext::new();
        let child = parent.child();

        assert_eq!(child.trace_id, parent.trace_id);
        assert_ne!(child.span_id, parent.span_id);
    }

    #[test]
    fn test_always_sampler() {
        let sampler = AlwaysSampler;
        let ctx = TraceContext::new();
        assert_eq!(
            sampler.should_sample(&ctx, "test"),
            SamplingDecision::RecordAndSample
        );
    }

    #[test]
    fn test_never_sampler() {
        let sampler = NeverSampler;
        let ctx = TraceContext::new();
        assert_eq!(sampler.should_sample(&ctx, "test"), SamplingDecision::Drop);
    }

    #[test]
    fn test_in_memory_exporter() {
        let exporter = InMemoryExporter::new();
        let span = SpanData {
            context: TraceContext::new(),
            parent_span_id: None,
            name: "test".to_string(),
            kind: SpanKind::Internal,
            start_time: Utc::now(),
            end_time: Utc::now(),
            duration: Duration::from_millis(100),
            status: StatusCode::Ok,
            status_message: None,
            attributes: HashMap::new(),
            events: vec![],
            links: vec![],
        };

        exporter.export(&[span]).unwrap();
        assert_eq!(exporter.spans().len(), 1);
    }
}
