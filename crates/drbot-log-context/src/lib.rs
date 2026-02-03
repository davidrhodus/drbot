//! Log context utilities for drbot.
//!
//! This crate provides:
//! - Structured logging context
//! - Context propagation
//! - Span-like context tracking

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Context error types.
#[derive(Error, Debug, Clone)]
pub enum ContextError {
    #[error("Context error: {0}")]
    Error(String),

    #[error("Key not found: {0}")]
    KeyNotFound(String),
}

/// Result type for context operations.
pub type Result<T> = std::result::Result<T, ContextError>;

/// Log context with key-value pairs.
#[derive(Debug, Clone, Default)]
pub struct LogContext {
    fields: HashMap<String, String>,
}

impl LogContext {
    /// Create new empty context.
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
        }
    }

    /// Create with initial field.
    pub fn with<K: Into<String>, V: Into<String>>(key: K, value: V) -> Self {
        let mut ctx = Self::new();
        ctx.set(key, value);
        ctx
    }

    /// Set field.
    pub fn set<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) {
        self.fields.insert(key.into(), value.into());
    }

    /// Get field.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.fields.get(key)
    }

    /// Remove field.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.fields.remove(key)
    }

    /// Check if field exists.
    pub fn contains(&self, key: &str) -> bool {
        self.fields.contains_key(key)
    }

    /// Get all fields.
    pub fn fields(&self) -> &HashMap<String, String> {
        &self.fields
    }

    /// Iterate over fields.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.fields.iter()
    }

    /// Merge another context.
    pub fn merge(&mut self, other: &LogContext) {
        for (k, v) in &other.fields {
            self.fields.insert(k.clone(), v.clone());
        }
    }

    /// Clone with additional field.
    pub fn with_field<K: Into<String>, V: Into<String>>(&self, key: K, value: V) -> Self {
        let mut ctx = self.clone();
        ctx.set(key, value);
        ctx
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Get number of fields.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Clear all fields.
    pub fn clear(&mut self) {
        self.fields.clear();
    }
}

/// Context builder.
#[derive(Debug, Default)]
pub struct ContextBuilder {
    fields: HashMap<String, String>,
}

impl ContextBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add field.
    pub fn field<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Add request ID.
    pub fn request_id<S: Into<String>>(self, id: S) -> Self {
        self.field("request_id", id)
    }

    /// Add user ID.
    pub fn user_id<S: Into<String>>(self, id: S) -> Self {
        self.field("user_id", id)
    }

    /// Add trace ID.
    pub fn trace_id<S: Into<String>>(self, id: S) -> Self {
        self.field("trace_id", id)
    }

    /// Add span ID.
    pub fn span_id<S: Into<String>>(self, id: S) -> Self {
        self.field("span_id", id)
    }

    /// Build context.
    pub fn build(self) -> LogContext {
        LogContext {
            fields: self.fields,
        }
    }
}

/// Span for tracking execution context.
#[derive(Debug)]
pub struct Span {
    name: String,
    context: LogContext,
    start_time: std::time::Instant,
}

impl Span {
    /// Create new span.
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            context: LogContext::new(),
            start_time: std::time::Instant::now(),
        }
    }

    /// Create named span with parent context.
    pub fn child<S: Into<String>>(name: S, parent: &LogContext) -> Self {
        let mut span = Self::new(name);
        span.context.merge(parent);
        span
    }

    /// Get span name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get elapsed time.
    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    /// Get elapsed milliseconds.
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed().as_millis() as u64
    }

    /// Set field.
    pub fn set<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) {
        self.context.set(key, value);
    }

    /// Get context.
    pub fn context(&self) -> &LogContext {
        &self.context
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        // Could log span completion here
    }
}

/// Scoped context that restores on drop.
pub struct ScopedContext {
    previous: Option<LogContext>,
    current: Arc<RwLock<Option<LogContext>>>,
}

impl ScopedContext {
    /// Enter scoped context.
    pub fn enter(store: &Arc<RwLock<Option<LogContext>>>, context: LogContext) -> Self {
        let previous = store.write().unwrap().replace(context);
        Self {
            previous,
            current: store.clone(),
        }
    }
}

impl Drop for ScopedContext {
    fn drop(&mut self) {
        *self.current.write().unwrap() = self.previous.take();
    }
}

/// Context store for thread-local or shared context.
#[derive(Debug, Default)]
pub struct ContextStore {
    context: Arc<RwLock<Option<LogContext>>>,
}

impl ContextStore {
    /// Create new context store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get current context.
    pub fn current(&self) -> Option<LogContext> {
        self.context.read().unwrap().clone()
    }

    /// Set context.
    pub fn set(&self, context: LogContext) {
        *self.context.write().unwrap() = Some(context);
    }

    /// Clear context.
    pub fn clear(&self) {
        *self.context.write().unwrap() = None;
    }

    /// Enter scoped context.
    pub fn scope(&self, context: LogContext) -> ScopedContext {
        ScopedContext::enter(&self.context, context)
    }

    /// Get field from current context.
    pub fn get(&self, key: &str) -> Option<String> {
        self.context
            .read()
            .unwrap()
            .as_ref()
            .and_then(|c| c.get(key).cloned())
    }
}

impl Clone for ContextStore {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
        }
    }
}

/// Common context field names.
pub mod fields {
    pub const REQUEST_ID: &str = "request_id";
    pub const TRACE_ID: &str = "trace_id";
    pub const SPAN_ID: &str = "span_id";
    pub const USER_ID: &str = "user_id";
    pub const SESSION_ID: &str = "session_id";
    pub const CORRELATION_ID: &str = "correlation_id";
    pub const SERVICE: &str = "service";
    pub const VERSION: &str = "version";
    pub const ENVIRONMENT: &str = "environment";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_context() {
        let mut ctx = LogContext::new();
        ctx.set("key", "value");

        assert_eq!(ctx.get("key"), Some(&"value".to_string()));
        assert!(ctx.contains("key"));
        assert!(!ctx.contains("other"));
    }

    #[test]
    fn test_context_builder() {
        let ctx = ContextBuilder::new()
            .request_id("req-123")
            .user_id("user-456")
            .field("custom", "value")
            .build();

        assert_eq!(ctx.get("request_id"), Some(&"req-123".to_string()));
        assert_eq!(ctx.get("user_id"), Some(&"user-456".to_string()));
        assert_eq!(ctx.get("custom"), Some(&"value".to_string()));
    }

    #[test]
    fn test_context_merge() {
        let mut ctx1 = LogContext::with("a", "1");
        let ctx2 = LogContext::with("b", "2");

        ctx1.merge(&ctx2);

        assert_eq!(ctx1.get("a"), Some(&"1".to_string()));
        assert_eq!(ctx1.get("b"), Some(&"2".to_string()));
    }

    #[test]
    fn test_span() {
        let mut span = Span::new("test_operation");
        span.set("key", "value");

        assert_eq!(span.name(), "test_operation");
        assert!(span.elapsed_ms() < 1000);
        assert_eq!(span.context().get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_context_store() {
        let store = ContextStore::new();

        assert!(store.current().is_none());

        store.set(LogContext::with("key", "value"));
        assert!(store.current().is_some());
        assert_eq!(store.get("key"), Some("value".to_string()));

        {
            let _scope = store.scope(LogContext::with("key", "scoped"));
            assert_eq!(store.get("key"), Some("scoped".to_string()));
        }

        assert_eq!(store.get("key"), Some("value".to_string()));
    }
}
