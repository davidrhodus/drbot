//! Event types and handlers for drbot.
//!
//! This crate provides:
//! - Event type definitions
//! - Event dispatcher
//! - Event handlers
//! - Event filtering

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Event error types.
#[derive(Error, Debug)]
pub enum EventError {
    #[error("Handler not found: {0}")]
    HandlerNotFound(String),

    #[error("Event dispatch failed: {0}")]
    DispatchFailed(String),

    #[error("Invalid event type")]
    InvalidEventType,
}

/// Result type for event operations.
pub type Result<T> = std::result::Result<T, EventError>;

/// Event ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub u64);

impl EventId {
    /// Generate new event ID.
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

/// Event metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Event ID.
    pub id: EventId,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Source identifier.
    pub source: Option<String>,
    /// Correlation ID.
    pub correlation_id: Option<String>,
    /// Custom attributes.
    pub attributes: HashMap<String, serde_json::Value>,
}

impl EventMetadata {
    /// Create new metadata.
    pub fn new() -> Self {
        Self {
            id: EventId::new(),
            timestamp: Utc::now(),
            source: None,
            correlation_id: None,
            attributes: HashMap::new(),
        }
    }

    /// Set source.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// Set correlation ID.
    pub fn with_correlation(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Set attribute.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.attributes.insert(key.into(), v);
        }
        self
    }
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// Event trait.
pub trait Event: Send + Sync + 'static {
    /// Get event name.
    fn name(&self) -> &str;

    /// Get metadata.
    fn metadata(&self) -> &EventMetadata;

    /// Get as Any for downcasting.
    fn as_any(&self) -> &dyn Any;
}

/// Base event wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseEvent<T> {
    /// Event type name.
    pub event_type: String,
    /// Event data.
    pub data: T,
    /// Metadata.
    pub metadata: EventMetadata,
}

impl<T: Clone + Send + Sync + 'static> BaseEvent<T> {
    /// Create new event.
    pub fn new(event_type: impl Into<String>, data: T) -> Self {
        Self {
            event_type: event_type.into(),
            data,
            metadata: EventMetadata::new(),
        }
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: EventMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

impl<T: Clone + Send + Sync + 'static> Event for BaseEvent<T> {
    fn name(&self) -> &str {
        &self.event_type
    }

    fn metadata(&self) -> &EventMetadata {
        &self.metadata
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Event handler trait.
pub trait EventHandler: Send + Sync {
    /// Handle event.
    fn handle(&self, event: &dyn Event) -> Result<()>;

    /// Get handler ID.
    fn id(&self) -> &str;
}

/// Function-based event handler.
pub struct FnHandler<F> {
    id: String,
    handler: F,
}

impl<F> FnHandler<F>
where
    F: Fn(&dyn Event) -> Result<()> + Send + Sync,
{
    /// Create new function handler.
    pub fn new(id: impl Into<String>, handler: F) -> Self {
        Self {
            id: id.into(),
            handler,
        }
    }
}

impl<F> EventHandler for FnHandler<F>
where
    F: Fn(&dyn Event) -> Result<()> + Send + Sync,
{
    fn handle(&self, event: &dyn Event) -> Result<()> {
        (self.handler)(event)
    }

    fn id(&self) -> &str {
        &self.id
    }
}

/// Event dispatcher.
pub struct EventDispatcher {
    handlers: Mutex<HashMap<String, Vec<Arc<dyn EventHandler>>>>,
    global_handlers: Mutex<Vec<Arc<dyn EventHandler>>>,
}

impl EventDispatcher {
    /// Create new dispatcher.
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(HashMap::new()),
            global_handlers: Mutex::new(Vec::new()),
        }
    }

    /// Register handler for event type.
    pub fn on(&self, event_type: impl Into<String>, handler: Arc<dyn EventHandler>) {
        let mut handlers = self.handlers.lock().unwrap();
        handlers.entry(event_type.into()).or_default().push(handler);
    }

    /// Register global handler (receives all events).
    pub fn on_all(&self, handler: Arc<dyn EventHandler>) {
        self.global_handlers.lock().unwrap().push(handler);
    }

    /// Unregister handler by ID.
    pub fn off(&self, event_type: &str, handler_id: &str) {
        let mut handlers = self.handlers.lock().unwrap();
        if let Some(list) = handlers.get_mut(event_type) {
            list.retain(|h| h.id() != handler_id);
        }
    }

    /// Dispatch event.
    pub fn dispatch(&self, event: &dyn Event) -> Result<()> {
        let event_name = event.name().to_string();

        // Global handlers
        {
            let handlers = self.global_handlers.lock().unwrap();
            for handler in handlers.iter() {
                handler.handle(event)?;
            }
        }

        // Type-specific handlers
        {
            let handlers = self.handlers.lock().unwrap();
            if let Some(list) = handlers.get(&event_name) {
                for handler in list {
                    handler.handle(event)?;
                }
            }
        }

        Ok(())
    }

    /// Get handler count for event type.
    pub fn handler_count(&self, event_type: &str) -> usize {
        self.handlers
            .lock()
            .unwrap()
            .get(event_type)
            .map_or(0, |v| v.len())
    }
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Event filter.
pub struct EventFilter {
    event_types: Option<Vec<String>>,
    source_pattern: Option<String>,
    min_time: Option<DateTime<Utc>>,
    max_time: Option<DateTime<Utc>>,
}

impl EventFilter {
    /// Create new filter.
    pub fn new() -> Self {
        Self {
            event_types: None,
            source_pattern: None,
            min_time: None,
            max_time: None,
        }
    }

    /// Filter by event types.
    pub fn event_types(mut self, types: Vec<String>) -> Self {
        self.event_types = Some(types);
        self
    }

    /// Filter by source pattern.
    pub fn source(mut self, pattern: impl Into<String>) -> Self {
        self.source_pattern = Some(pattern.into());
        self
    }

    /// Filter by time range.
    pub fn time_range(mut self, min: DateTime<Utc>, max: DateTime<Utc>) -> Self {
        self.min_time = Some(min);
        self.max_time = Some(max);
        self
    }

    /// Check if event matches filter.
    pub fn matches(&self, event: &dyn Event) -> bool {
        // Check event type
        if let Some(ref types) = self.event_types {
            if !types.contains(&event.name().to_string()) {
                return false;
            }
        }

        let meta = event.metadata();

        // Check source
        if let Some(ref pattern) = self.source_pattern {
            match &meta.source {
                Some(source) if source.contains(pattern) => {}
                _ => return false,
            }
        }

        // Check time range
        if let Some(min) = self.min_time {
            if meta.timestamp < min {
                return false;
            }
        }
        if let Some(max) = self.max_time {
            if meta.timestamp > max {
                return false;
            }
        }

        true
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Event store for recording events.
#[derive(Debug, Default)]
pub struct EventStore {
    events: Mutex<Vec<StoredEvent>>,
}

/// Stored event (serialized).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub event_type: String,
    pub metadata: EventMetadata,
    pub payload: serde_json::Value,
}

impl EventStore {
    /// Create new store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store event.
    pub fn store(&self, event: &dyn Event, payload: serde_json::Value) {
        let stored = StoredEvent {
            event_type: event.name().to_string(),
            metadata: event.metadata().clone(),
            payload,
        };
        self.events.lock().unwrap().push(stored);
    }

    /// Get all events.
    pub fn all(&self) -> Vec<StoredEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Get events by type.
    pub fn by_type(&self, event_type: &str) -> Vec<StoredEvent> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.event_type == event_type)
            .cloned()
            .collect()
    }

    /// Clear all events.
    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }

    /// Get event count.
    pub fn count(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_id() {
        let id1 = EventId::new();
        let id2 = EventId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_base_event() {
        let event = BaseEvent::new("test.created", "test data");
        assert_eq!(event.name(), "test.created");
        assert_eq!(event.data, "test data");
    }

    #[test]
    fn test_dispatcher() {
        let dispatcher = EventDispatcher::new();

        let called = Arc::new(Mutex::new(false));
        let called_clone = called.clone();

        let handler = Arc::new(FnHandler::new("test_handler", move |_event| {
            *called_clone.lock().unwrap() = true;
            Ok(())
        }));

        dispatcher.on("test", handler);

        let event = BaseEvent::new("test", "data");
        dispatcher.dispatch(&event).unwrap();

        assert!(*called.lock().unwrap());
    }

    #[test]
    fn test_event_filter() {
        let filter = EventFilter::new().event_types(vec!["test".to_string()]);

        let event = BaseEvent::new("test", "data");
        assert!(filter.matches(&event));

        let event2 = BaseEvent::new("other", "data");
        assert!(!filter.matches(&event2));
    }

    #[test]
    fn test_event_store() {
        let store = EventStore::new();
        let event = BaseEvent::new("test", "data");

        store.store(&event, serde_json::json!({"key": "value"}));

        assert_eq!(store.count(), 1);
        assert_eq!(store.by_type("test").len(), 1);
    }
}
