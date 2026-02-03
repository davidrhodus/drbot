//! Mediator pattern utilities for drbot.
//!
//! This crate provides:
//! - Mediator trait for coordinating components
//! - Event-based mediation
//! - Request/Response mediation

use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Mediator error types.
#[derive(Error, Debug)]
pub enum MediatorError {
    #[error("Handler not found: {0}")]
    HandlerNotFound(String),

    #[error("Mediation failed: {0}")]
    MediationFailed(String),

    #[error("Invalid request")]
    InvalidRequest,
}

/// Result type for mediator operations.
pub type Result<T> = std::result::Result<T, MediatorError>;

/// Mediator trait for coordinating colleagues.
pub trait Mediator: Send + Sync {
    /// Message type.
    type Message;

    /// Notify mediator of event.
    fn notify(&self, sender: &str, message: Self::Message);
}

/// Colleague that participates in mediation.
pub trait Colleague: Send + Sync {
    /// Message type.
    type Message;

    /// Receive message from mediator.
    fn receive(&self, message: Self::Message);

    /// Get colleague name.
    fn name(&self) -> &str;
}

/// Request handler trait.
pub trait RequestHandler<R, T>: Send + Sync {
    /// Handle request and return response.
    fn handle(&self, request: R) -> Result<T>;
}

/// Event mediator for broadcasting events.
pub struct EventMediator<E: Clone + Send + Sync> {
    handlers: std::sync::RwLock<HashMap<String, Vec<Arc<dyn Fn(E) + Send + Sync>>>>,
}

impl<E: Clone + Send + Sync> EventMediator<E> {
    /// Create new event mediator.
    pub fn new() -> Self {
        Self {
            handlers: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Subscribe to event type.
    pub fn subscribe<F>(&self, event_type: impl Into<String>, handler: F)
    where
        F: Fn(E) + Send + Sync + 'static,
    {
        let mut handlers = self.handlers.write().unwrap();
        handlers
            .entry(event_type.into())
            .or_insert_with(Vec::new)
            .push(Arc::new(handler));
    }

    /// Publish event to subscribers.
    pub fn publish(&self, event_type: &str, event: E) {
        let handlers = self.handlers.read().unwrap();
        if let Some(handlers) = handlers.get(event_type) {
            for handler in handlers {
                handler(event.clone());
            }
        }
    }

    /// Get subscriber count for event type.
    pub fn subscriber_count(&self, event_type: &str) -> usize {
        self.handlers
            .read()
            .unwrap()
            .get(event_type)
            .map(|h| h.len())
            .unwrap_or(0)
    }

    /// Clear all subscribers.
    pub fn clear(&self) {
        self.handlers.write().unwrap().clear();
    }
}

impl<E: Clone + Send + Sync> Default for EventMediator<E> {
    fn default() -> Self {
        Self::new()
    }
}

/// Request mediator for request/response pattern.
pub struct RequestMediator<R, T> {
    handlers: std::sync::RwLock<HashMap<String, Arc<dyn RequestHandler<R, T>>>>,
}

impl<R: Send + Sync, T: Send + Sync> RequestMediator<R, T> {
    /// Create new request mediator.
    pub fn new() -> Self {
        Self {
            handlers: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Register handler.
    pub fn register(&self, name: impl Into<String>, handler: Arc<dyn RequestHandler<R, T>>) {
        self.handlers.write().unwrap().insert(name.into(), handler);
    }

    /// Send request to handler.
    pub fn send(&self, handler_name: &str, request: R) -> Result<T> {
        let handlers = self.handlers.read().unwrap();
        let handler = handlers
            .get(handler_name)
            .ok_or_else(|| MediatorError::HandlerNotFound(handler_name.to_string()))?;
        handler.handle(request)
    }

    /// Check if handler exists.
    pub fn has_handler(&self, name: &str) -> bool {
        self.handlers.read().unwrap().contains_key(name)
    }

    /// Get handler names.
    pub fn handler_names(&self) -> Vec<String> {
        self.handlers.read().unwrap().keys().cloned().collect()
    }
}

impl<R: Send + Sync, T: Send + Sync> Default for RequestMediator<R, T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Function-based request handler.
pub struct FnHandler<R, T, F: Fn(R) -> Result<T> + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<(R, T)>,
}

impl<R, T, F: Fn(R) -> Result<T> + Send + Sync> FnHandler<R, T, F> {
    /// Create new function handler.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<R: Send + Sync, T: Send + Sync, F: Fn(R) -> Result<T> + Send + Sync> RequestHandler<R, T>
    for FnHandler<R, T, F>
{
    fn handle(&self, request: R) -> Result<T> {
        (self.func)(request)
    }
}

/// Chat room mediator example.
pub struct ChatRoom {
    participants: std::sync::RwLock<HashMap<String, Arc<dyn Fn(&str, &str) + Send + Sync>>>,
}

impl ChatRoom {
    /// Create new chat room.
    pub fn new() -> Self {
        Self {
            participants: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Add participant.
    pub fn add_participant<F>(&self, name: impl Into<String>, handler: F)
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        self.participants
            .write()
            .unwrap()
            .insert(name.into(), Arc::new(handler));
    }

    /// Remove participant.
    pub fn remove_participant(&self, name: &str) {
        self.participants.write().unwrap().remove(name);
    }

    /// Send message from one participant to another.
    pub fn send_message(&self, from: &str, to: &str, message: &str) -> Result<()> {
        let participants = self.participants.read().unwrap();
        let handler = participants
            .get(to)
            .ok_or_else(|| MediatorError::HandlerNotFound(to.to_string()))?;
        handler(from, message);
        Ok(())
    }

    /// Broadcast message to all participants.
    pub fn broadcast(&self, from: &str, message: &str) {
        let participants = self.participants.read().unwrap();
        for (name, handler) in participants.iter() {
            if name != from {
                handler(from, message);
            }
        }
    }

    /// Get participant count.
    pub fn participant_count(&self) -> usize {
        self.participants.read().unwrap().len()
    }
}

impl Default for ChatRoom {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create request handler.
pub fn handler<R: Send + Sync + 'static, T: Send + Sync + 'static, F>(
    func: F,
) -> Arc<dyn RequestHandler<R, T>>
where
    F: Fn(R) -> Result<T> + Send + Sync + 'static,
{
    Arc::new(FnHandler::new(func))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn test_event_mediator() {
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let mediator = EventMediator::<i32>::new();
        mediator.subscribe("increment", move |n| {
            counter_clone.fetch_add(n, Ordering::SeqCst);
        });

        mediator.publish("increment", 5);
        mediator.publish("increment", 3);

        assert_eq!(counter.load(Ordering::SeqCst), 8);
    }

    #[test]
    fn test_request_mediator() {
        let mediator = RequestMediator::new();
        mediator.register("double", handler(|x: i32| Ok(x * 2)));

        assert_eq!(mediator.send("double", 21).unwrap(), 42);
        assert!(mediator.send("missing", 1).is_err());
    }

    #[test]
    fn test_chat_room() {
        let messages = Arc::new(std::sync::Mutex::new(Vec::new()));
        let messages_clone = messages.clone();

        let chat = ChatRoom::new();
        chat.add_participant("alice", move |from, msg| {
            messages_clone
                .lock()
                .unwrap()
                .push(format!("{}: {}", from, msg));
        });
        chat.add_participant("bob", |_, _| {});

        chat.send_message("bob", "alice", "Hello!").unwrap();

        let msgs = messages.lock().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0], "bob: Hello!");
    }
}
