//! Signal-slot pattern for drbot.
//!
//! This crate provides:
//! - Type-safe signals
//! - Slot connections
//! - Auto-disconnect on drop

use std::sync::{Arc, RwLock, Weak};
use thiserror::Error;

/// Signal-slot error types.
#[derive(Error, Debug, Clone)]
pub enum SignalError {
    #[error("Slot disconnected")]
    Disconnected,

    #[error("Connection not found")]
    ConnectionNotFound,
}

/// Result type for signal operations.
pub type Result<T> = std::result::Result<T, SignalError>;

/// Connection ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(usize);

/// Signal that can emit values to connected slots.
pub struct Signal<T> {
    slots: RwLock<Vec<(ConnectionId, Box<dyn Fn(&T) + Send + Sync>)>>,
    next_id: RwLock<usize>,
}

impl<T> Signal<T> {
    /// Create new signal.
    pub fn new() -> Self {
        Self {
            slots: RwLock::new(Vec::new()),
            next_id: RwLock::new(0),
        }
    }

    /// Connect slot to signal.
    pub fn connect<F>(&self, slot: F) -> Connection
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let id = {
            let mut next_id = self.next_id.write().unwrap();
            let id = ConnectionId(*next_id);
            *next_id += 1;
            id
        };

        let mut slots = self.slots.write().unwrap();
        slots.push((id, Box::new(slot)));
        Connection { id }
    }

    /// Disconnect slot.
    pub fn disconnect(&self, connection: &Connection) -> bool {
        let mut slots = self.slots.write().unwrap();
        if let Some(pos) = slots.iter().position(|(id, _)| *id == connection.id) {
            slots.remove(pos);
            true
        } else {
            false
        }
    }

    /// Emit signal to all connected slots.
    pub fn emit(&self, value: &T) {
        let slots = self.slots.read().unwrap();
        for (_, slot) in slots.iter() {
            slot(value);
        }
    }

    /// Get number of connected slots.
    pub fn connection_count(&self) -> usize {
        self.slots.read().unwrap().len()
    }

    /// Disconnect all slots.
    pub fn disconnect_all(&self) {
        let mut slots = self.slots.write().unwrap();
        slots.clear();
    }
}

impl<T> Default for Signal<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Connection handle.
#[derive(Debug)]
pub struct Connection {
    id: ConnectionId,
}

impl Connection {
    /// Get connection ID.
    pub fn id(&self) -> ConnectionId {
        self.id
    }
}

/// Shared signal.
pub type SharedSignal<T> = Arc<Signal<T>>;

/// Create shared signal.
pub fn signal<T>() -> SharedSignal<T> {
    Arc::new(Signal::new())
}

/// Scoped connection that auto-disconnects.
pub struct ScopedConnection<T> {
    signal: Weak<Signal<T>>,
    connection: Option<Connection>,
}

impl<T> ScopedConnection<T> {
    /// Create scoped connection.
    pub fn new(signal: &SharedSignal<T>, connection: Connection) -> Self {
        Self {
            signal: Arc::downgrade(signal),
            connection: Some(connection),
        }
    }

    /// Manually disconnect.
    pub fn disconnect(&mut self) {
        if let Some(connection) = self.connection.take() {
            if let Some(signal) = self.signal.upgrade() {
                signal.disconnect(&connection);
            }
        }
    }
}

impl<T> Drop for ScopedConnection<T> {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// Signal with void type (no arguments).
pub type VoidSignal = Signal<()>;

/// Create void signal.
pub fn void_signal() -> VoidSignal {
    Signal::new()
}

/// Multi-signal - emits to multiple signal types.
pub struct MultiSignal<T> {
    signals: RwLock<Vec<Weak<Signal<T>>>>,
}

impl<T> MultiSignal<T> {
    /// Create new multi-signal.
    pub fn new() -> Self {
        Self {
            signals: RwLock::new(Vec::new()),
        }
    }

    /// Add signal to emit to.
    pub fn add(&self, signal: &SharedSignal<T>) {
        let mut signals = self.signals.write().unwrap();
        signals.push(Arc::downgrade(signal));
    }

    /// Emit to all connected signals.
    pub fn emit(&self, value: &T) {
        let signals = self.signals.read().unwrap();
        for weak in signals.iter() {
            if let Some(signal) = weak.upgrade() {
                signal.emit(value);
            }
        }
    }

    /// Clean up dead references.
    pub fn cleanup(&self) {
        let mut signals = self.signals.write().unwrap();
        signals.retain(|w| w.strong_count() > 0);
    }
}

impl<T> Default for MultiSignal<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Slot wrapper for method-like callbacks.
pub struct Slot<T, R> {
    handler: Box<dyn Fn(&T) -> R + Send + Sync>,
}

impl<T, R> Slot<T, R> {
    /// Create new slot.
    pub fn new<F>(handler: F) -> Self
    where
        F: Fn(&T) -> R + Send + Sync + 'static,
    {
        Self {
            handler: Box::new(handler),
        }
    }

    /// Invoke slot.
    pub fn invoke(&self, value: &T) -> R {
        (self.handler)(value)
    }
}

/// Queued connection for deferred slot invocation.
pub struct QueuedSignal<T: Clone> {
    queue: RwLock<Vec<T>>,
    signal: Signal<T>,
}

impl<T: Clone> QueuedSignal<T> {
    /// Create new queued signal.
    pub fn new() -> Self {
        Self {
            queue: RwLock::new(Vec::new()),
            signal: Signal::new(),
        }
    }

    /// Connect slot.
    pub fn connect<F>(&self, slot: F) -> Connection
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.signal.connect(slot)
    }

    /// Queue value for later emission.
    pub fn queue(&self, value: T) {
        let mut queue = self.queue.write().unwrap();
        queue.push(value);
    }

    /// Emit immediately.
    pub fn emit(&self, value: &T) {
        self.signal.emit(value);
    }

    /// Flush queued values.
    pub fn flush(&self) {
        let values: Vec<T> = {
            let mut queue = self.queue.write().unwrap();
            std::mem::take(&mut *queue)
        };
        for value in &values {
            self.signal.emit(value);
        }
    }

    /// Get queue size.
    pub fn queue_size(&self) -> usize {
        self.queue.read().unwrap().len()
    }
}

impl<T: Clone> Default for QueuedSignal<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn test_signal_slot() {
        let signal: Signal<i32> = Signal::new();
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        signal.connect(move |v| {
            c.fetch_add(*v, Ordering::SeqCst);
        });

        signal.emit(&5);
        signal.emit(&3);

        assert_eq!(counter.load(Ordering::SeqCst), 8);
    }

    #[test]
    fn test_disconnect() {
        let signal: Signal<i32> = Signal::new();
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        let conn = signal.connect(move |v| {
            c.fetch_add(*v, Ordering::SeqCst);
        });

        signal.emit(&10);
        signal.disconnect(&conn);
        signal.emit(&10);

        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn test_scoped_connection() {
        let signal = signal::<i32>();
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        {
            let conn = signal.connect(move |v| {
                c.fetch_add(*v, Ordering::SeqCst);
            });
            let _scoped = ScopedConnection::new(&signal, conn);
            signal.emit(&10);
        } // scoped connection drops here

        signal.emit(&10);
        assert_eq!(counter.load(Ordering::SeqCst), 10); // Only first emit counted
    }

    #[test]
    fn test_queued_signal() {
        let signal: QueuedSignal<i32> = QueuedSignal::new();
        let counter = Arc::new(AtomicI32::new(0));

        let c = counter.clone();
        signal.connect(move |v| {
            c.fetch_add(*v, Ordering::SeqCst);
        });

        signal.queue(5);
        signal.queue(3);
        assert_eq!(counter.load(Ordering::SeqCst), 0); // Not emitted yet

        signal.flush();
        assert_eq!(counter.load(Ordering::SeqCst), 8);
    }
}
