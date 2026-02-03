//! Event emitter pattern for drbot.
//!
//! This crate provides:
//! - Type-safe event emission
//! - Multiple listeners per event
//! - One-time listeners

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use thiserror::Error;

/// Event emitter error types.
#[derive(Error, Debug, Clone)]
pub enum EmitterError {
    #[error("Handler not found")]
    HandlerNotFound,

    #[error("Event type mismatch")]
    TypeMismatch,
}

/// Result type for emitter operations.
pub type Result<T> = std::result::Result<T, EmitterError>;

/// Event listener ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListenerId(usize);

/// Type-erased event handler.
type AnyHandler = Box<dyn Fn(&dyn Any) + Send + Sync>;

/// Simple event emitter.
pub struct EventEmitter {
    handlers: RwLock<HashMap<TypeId, Vec<(ListenerId, AnyHandler)>>>,
    next_id: Mutex<usize>,
}

impl EventEmitter {
    /// Create new event emitter.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            next_id: Mutex::new(0),
        }
    }

    /// Add event listener.
    pub fn on<E: 'static, F>(&self, handler: F) -> ListenerId
    where
        F: Fn(&E) + Send + Sync + 'static,
    {
        let id = {
            let mut next_id = self.next_id.lock().unwrap();
            let id = ListenerId(*next_id);
            *next_id += 1;
            id
        };

        let type_id = TypeId::of::<E>();
        let wrapped: AnyHandler = Box::new(move |event: &dyn Any| {
            if let Some(e) = event.downcast_ref::<E>() {
                handler(e);
            }
        });

        let mut handlers = self.handlers.write().unwrap();
        handlers.entry(type_id).or_default().push((id, wrapped));
        id
    }

    /// Remove event listener.
    pub fn off(&self, id: ListenerId) -> bool {
        let mut handlers = self.handlers.write().unwrap();
        for list in handlers.values_mut() {
            if let Some(pos) = list.iter().position(|(lid, _)| *lid == id) {
                list.remove(pos);
                return true;
            }
        }
        false
    }

    /// Emit event to all listeners.
    pub fn emit<E: 'static>(&self, event: &E) {
        let type_id = TypeId::of::<E>();
        let handlers = self.handlers.read().unwrap();
        if let Some(list) = handlers.get(&type_id) {
            for (_, handler) in list {
                handler(event);
            }
        }
    }

    /// Get number of listeners for event type.
    pub fn listener_count<E: 'static>(&self) -> usize {
        let type_id = TypeId::of::<E>();
        let handlers = self.handlers.read().unwrap();
        handlers.get(&type_id).map(|l| l.len()).unwrap_or(0)
    }

    /// Remove all listeners for event type.
    pub fn clear<E: 'static>(&self) {
        let type_id = TypeId::of::<E>();
        let mut handlers = self.handlers.write().unwrap();
        handlers.remove(&type_id);
    }

    /// Remove all listeners.
    pub fn clear_all(&self) {
        let mut handlers = self.handlers.write().unwrap();
        handlers.clear();
    }
}

impl Default for EventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe event emitter with shared ownership.
pub type SharedEmitter = Arc<EventEmitter>;

/// Create shared emitter.
pub fn shared_emitter() -> SharedEmitter {
    Arc::new(EventEmitter::new())
}

/// One-time event listener (fires once then removes itself).
pub struct OnceEmitter {
    inner: EventEmitter,
    fired: RwLock<HashMap<TypeId, bool>>,
}

impl OnceEmitter {
    /// Create new once emitter.
    pub fn new() -> Self {
        Self {
            inner: EventEmitter::new(),
            fired: RwLock::new(HashMap::new()),
        }
    }

    /// Add one-time listener.
    pub fn once<E: 'static, F>(&self, handler: F) -> ListenerId
    where
        F: Fn(&E) + Send + Sync + 'static,
    {
        self.inner.on(handler)
    }

    /// Emit event (fires handlers once then removes them).
    pub fn emit<E: 'static>(&self, event: &E) {
        let type_id = TypeId::of::<E>();

        // Check if already fired
        {
            let fired = self.fired.read().unwrap();
            if fired.get(&type_id).copied().unwrap_or(false) {
                return;
            }
        }

        // Mark as fired
        {
            let mut fired = self.fired.write().unwrap();
            fired.insert(type_id, true);
        }

        // Emit and clear
        self.inner.emit(event);
        self.inner.clear::<E>();
    }

    /// Reset so event can fire again.
    pub fn reset<E: 'static>(&self) {
        let type_id = TypeId::of::<E>();
        let mut fired = self.fired.write().unwrap();
        fired.remove(&type_id);
    }
}

impl Default for OnceEmitter {
    fn default() -> Self {
        Self::new()
    }
}

/// Named event emitter using string keys.
pub struct NamedEmitter {
    handlers: RwLock<HashMap<String, Vec<(ListenerId, Box<dyn Fn(&str) + Send + Sync>)>>>,
    next_id: Mutex<usize>,
}

impl NamedEmitter {
    /// Create new named emitter.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            next_id: Mutex::new(0),
        }
    }

    /// Add listener for named event.
    pub fn on<F>(&self, name: &str, handler: F) -> ListenerId
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        let id = {
            let mut next_id = self.next_id.lock().unwrap();
            let id = ListenerId(*next_id);
            *next_id += 1;
            id
        };

        let mut handlers = self.handlers.write().unwrap();
        handlers
            .entry(name.to_string())
            .or_default()
            .push((id, Box::new(handler)));
        id
    }

    /// Emit named event with data.
    pub fn emit(&self, name: &str, data: &str) {
        let handlers = self.handlers.read().unwrap();
        if let Some(list) = handlers.get(name) {
            for (_, handler) in list {
                handler(data);
            }
        }
    }

    /// Remove listener.
    pub fn off(&self, id: ListenerId) -> bool {
        let mut handlers = self.handlers.write().unwrap();
        for list in handlers.values_mut() {
            if let Some(pos) = list.iter().position(|(lid, _)| *lid == id) {
                list.remove(pos);
                return true;
            }
        }
        false
    }
}

impl Default for NamedEmitter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};

    struct TestEvent(i32);

    #[test]
    fn test_event_emitter() {
        let emitter = EventEmitter::new();
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        emitter.on(move |e: &TestEvent| {
            c.fetch_add(e.0, Ordering::SeqCst);
        });

        emitter.emit(&TestEvent(5));
        emitter.emit(&TestEvent(3));

        assert_eq!(counter.load(Ordering::SeqCst), 8);
    }

    #[test]
    fn test_remove_listener() {
        let emitter = EventEmitter::new();
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        let id = emitter.on(move |e: &TestEvent| {
            c.fetch_add(e.0, Ordering::SeqCst);
        });

        emitter.emit(&TestEvent(5));
        assert!(emitter.off(id));
        emitter.emit(&TestEvent(5));

        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn test_named_emitter() {
        let emitter = NamedEmitter::new();
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        emitter.on("increment", move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        });

        emitter.emit("increment", "");
        emitter.emit("increment", "");

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
