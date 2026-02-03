//! Signal/slot pattern for drbot.
//!
//! This crate provides:
//! - Signal emitters
//! - Slot connections
//! - Connection management
//! - Type-safe signals

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Slot error types.
#[derive(Error, Debug)]
pub enum SlotError {
    #[error("Connection not found")]
    NotFound,

    #[error("Signal disconnected")]
    Disconnected,

    #[error("Slot invocation failed")]
    InvocationFailed,
}

/// Result type for slot operations.
pub type Result<T> = std::result::Result<T, SlotError>;

/// Connection ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(u64);

impl ConnectionId {
    /// Generate new ID.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Connection handle.
pub struct Connection {
    id: ConnectionId,
    disconnect: Option<Box<dyn FnOnce() + Send>>,
}

impl Connection {
    /// Get ID.
    pub fn id(&self) -> ConnectionId {
        self.id
    }

    /// Disconnect.
    pub fn disconnect(mut self) {
        if let Some(f) = self.disconnect.take() {
            f();
        }
    }

    /// Create detached connection (won't auto-disconnect on drop).
    pub fn detach(mut self) {
        self.disconnect = None;
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(f) = self.disconnect.take() {
            f();
        }
    }
}

/// Slot function wrapper.
type SlotFn<T> = Arc<dyn Fn(&T) + Send + Sync>;

/// Signal emitter.
pub struct Signal<T> {
    slots: Arc<Mutex<Vec<(ConnectionId, SlotFn<T>)>>>,
}

impl<T: 'static> Signal<T> {
    /// Create new signal.
    pub fn new() -> Self {
        Self {
            slots: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Connect slot.
    pub fn connect<F>(&self, slot: F) -> Connection
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let id = ConnectionId::new();
        self.slots.lock().unwrap().push((id, Arc::new(slot)));

        let slots = Arc::downgrade(&self.slots);
        Connection {
            id,
            disconnect: Some(Box::new(move || {
                if let Some(slots) = slots.upgrade() {
                    slots.lock().unwrap().retain(|(i, _)| *i != id);
                }
            })),
        }
    }

    /// Emit signal.
    pub fn emit(&self, value: &T) {
        let slots = self.slots.lock().unwrap();
        for (_, slot) in slots.iter() {
            slot(value);
        }
    }

    /// Disconnect by ID.
    pub fn disconnect(&self, id: ConnectionId) -> bool {
        let mut slots = self.slots.lock().unwrap();
        let len_before = slots.len();
        slots.retain(|(i, _)| *i != id);
        slots.len() < len_before
    }

    /// Disconnect all.
    pub fn disconnect_all(&self) {
        self.slots.lock().unwrap().clear();
    }

    /// Get connection count.
    pub fn connection_count(&self) -> usize {
        self.slots.lock().unwrap().len()
    }

    /// Check if has connections.
    pub fn has_connections(&self) -> bool {
        !self.slots.lock().unwrap().is_empty()
    }
}

impl<T: 'static> Default for Signal<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            slots: self.slots.clone(),
        }
    }
}

/// Signal with no arguments.
pub struct Signal0 {
    slots: Arc<Mutex<Vec<(ConnectionId, Arc<dyn Fn() + Send + Sync>)>>>,
}

impl Signal0 {
    /// Create new signal.
    pub fn new() -> Self {
        Self {
            slots: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Connect slot.
    pub fn connect<F>(&self, slot: F) -> Connection
    where
        F: Fn() + Send + Sync + 'static,
    {
        let id = ConnectionId::new();
        self.slots.lock().unwrap().push((id, Arc::new(slot)));

        let slots = Arc::downgrade(&self.slots);
        Connection {
            id,
            disconnect: Some(Box::new(move || {
                if let Some(slots) = slots.upgrade() {
                    slots.lock().unwrap().retain(|(i, _)| *i != id);
                }
            })),
        }
    }

    /// Emit signal.
    pub fn emit(&self) {
        let slots = self.slots.lock().unwrap();
        for (_, slot) in slots.iter() {
            slot();
        }
    }

    /// Disconnect all.
    pub fn disconnect_all(&self) {
        self.slots.lock().unwrap().clear();
    }
}

impl Default for Signal0 {
    fn default() -> Self {
        Self::new()
    }
}

/// Signal with two arguments.
pub struct Signal2<T1, T2> {
    slots: Arc<Mutex<Vec<(ConnectionId, Arc<dyn Fn(&T1, &T2) + Send + Sync>)>>>,
}

impl<T1: 'static, T2: 'static> Signal2<T1, T2> {
    /// Create new signal.
    pub fn new() -> Self {
        Self {
            slots: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Connect slot.
    pub fn connect<F>(&self, slot: F) -> Connection
    where
        F: Fn(&T1, &T2) + Send + Sync + 'static,
    {
        let id = ConnectionId::new();
        self.slots.lock().unwrap().push((id, Arc::new(slot)));

        let slots = Arc::downgrade(&self.slots);
        Connection {
            id,
            disconnect: Some(Box::new(move || {
                if let Some(slots) = slots.upgrade() {
                    slots.lock().unwrap().retain(|(i, _)| *i != id);
                }
            })),
        }
    }

    /// Emit signal.
    pub fn emit(&self, v1: &T1, v2: &T2) {
        let slots = self.slots.lock().unwrap();
        for (_, slot) in slots.iter() {
            slot(v1, v2);
        }
    }

    /// Disconnect all.
    pub fn disconnect_all(&self) {
        self.slots.lock().unwrap().clear();
    }
}

impl<T1: 'static, T2: 'static> Default for Signal2<T1, T2> {
    fn default() -> Self {
        Self::new()
    }
}

/// Scoped connection that auto-disconnects when dropped.
pub struct ScopedConnection {
    connection: Option<Connection>,
}

impl ScopedConnection {
    /// Create from connection.
    pub fn new(connection: Connection) -> Self {
        Self {
            connection: Some(connection),
        }
    }

    /// Release without disconnecting.
    pub fn release(mut self) -> Option<Connection> {
        self.connection.take()
    }
}

impl Drop for ScopedConnection {
    fn drop(&mut self) {
        if let Some(conn) = self.connection.take() {
            conn.disconnect();
        }
    }
}

/// Connection group for managing multiple connections.
pub struct ConnectionGroup {
    connections: Vec<Connection>,
}

impl ConnectionGroup {
    /// Create new group.
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
        }
    }

    /// Add connection.
    pub fn add(&mut self, connection: Connection) {
        self.connections.push(connection);
    }

    /// Disconnect all.
    pub fn disconnect_all(self) {
        for conn in self.connections {
            conn.disconnect();
        }
    }

    /// Get count.
    pub fn len(&self) -> usize {
        self.connections.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }
}

impl Default for ConnectionGroup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI32;

    #[test]
    fn test_signal() {
        let signal = Signal::new();
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let _conn = signal.connect(move |v: &i32| {
            counter_clone.fetch_add(*v, Ordering::SeqCst);
        });

        signal.emit(&5);
        signal.emit(&3);

        assert_eq!(counter.load(Ordering::SeqCst), 8);
    }

    #[test]
    fn test_disconnect() {
        let signal = Signal::new();
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let conn = signal.connect(move |v: &i32| {
            counter_clone.fetch_add(*v, Ordering::SeqCst);
        });

        signal.emit(&5);
        conn.disconnect();
        signal.emit(&3);

        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn test_signal0() {
        let signal = Signal0::new();
        let counter = Arc::new(AtomicI32::new(0));
        let counter_clone = counter.clone();

        let _conn = signal.connect(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        signal.emit();
        signal.emit();

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_scoped_connection() {
        let signal = Signal::new();
        let counter = Arc::new(AtomicI32::new(0));

        {
            let counter_clone = counter.clone();
            let _scoped = ScopedConnection::new(signal.connect(move |v: &i32| {
                counter_clone.fetch_add(*v, Ordering::SeqCst);
            }));

            signal.emit(&5);
        }

        // Connection should be disconnected
        signal.emit(&3);
        assert_eq!(counter.load(Ordering::SeqCst), 5);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // ConnectionId Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_connection_id_unique() {
        let id1 = ConnectionId::new();
        let id2 = ConnectionId::new();

        kani::assert(id1 != id2, "ConnectionIds are unique");
    }

    #[kani::proof]
    fn proof_connection_id_default_unique() {
        let id1 = ConnectionId::default();
        let id2 = ConnectionId::default();

        kani::assert(id1 != id2, "default ConnectionIds are unique");
    }

    #[kani::proof]
    fn proof_connection_id_equality() {
        let id = ConnectionId::new();
        kani::assert(id == id, "ConnectionId equals itself");
    }

    // ========================================================================
    // Signal Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_signal_new_no_connections() {
        let signal: Signal<i32> = Signal::new();
        kani::assert(!signal.has_connections(), "new Signal has no connections");
        kani::assert(signal.connection_count() == 0, "connection_count is 0");
    }

    #[kani::proof]
    fn proof_signal_default_no_connections() {
        let signal: Signal<i32> = Signal::default();
        kani::assert(
            !signal.has_connections(),
            "default Signal has no connections",
        );
    }

    #[kani::proof]
    fn proof_signal_connect_adds_connection() {
        let signal: Signal<i32> = Signal::new();
        let _conn = signal.connect(|_| {});

        kani::assert(signal.has_connections(), "has connection after connect");
        kani::assert(signal.connection_count() == 1, "connection_count is 1");
    }

    #[kani::proof]
    fn proof_signal_disconnect_all_clears() {
        let signal: Signal<i32> = Signal::new();
        let _conn1 = signal.connect(|_| {});
        let _conn2 = signal.connect(|_| {});

        signal.disconnect_all();

        kani::assert(
            !signal.has_connections(),
            "no connections after disconnect_all",
        );
        kani::assert(signal.connection_count() == 0, "connection_count is 0");
    }

    #[kani::proof]
    fn proof_signal_disconnect_by_id() {
        let signal: Signal<i32> = Signal::new();
        let conn = signal.connect(|_| {});
        let id = conn.id();
        conn.detach(); // Don't auto-disconnect

        let removed = signal.disconnect(id);
        kani::assert(removed, "disconnect returns true for existing");
        kani::assert(signal.connection_count() == 0, "connection removed");
    }

    #[kani::proof]
    fn proof_signal_disconnect_nonexistent() {
        let signal: Signal<i32> = Signal::new();
        let fake_id = ConnectionId::new();

        let removed = signal.disconnect(fake_id);
        kani::assert(!removed, "disconnect returns false for nonexistent");
    }

    #[kani::proof]
    fn proof_signal_clone_shares_slots() {
        let signal1: Signal<i32> = Signal::new();
        let signal2 = signal1.clone();

        let _conn = signal1.connect(|_| {});

        kani::assert(signal2.has_connections(), "clone shares connections");
        kani::assert(signal2.connection_count() == 1, "clone has same count");
    }

    // ========================================================================
    // Signal0 Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_signal0_new() {
        let signal = Signal0::new();
        // Just verify it creates without panic
        signal.disconnect_all();
    }

    #[kani::proof]
    fn proof_signal0_default() {
        let signal = Signal0::default();
        signal.disconnect_all();
    }

    // ========================================================================
    // Signal2 Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_signal2_new() {
        let signal: Signal2<i32, i32> = Signal2::new();
        signal.disconnect_all();
    }

    #[kani::proof]
    fn proof_signal2_default() {
        let signal: Signal2<i32, i32> = Signal2::default();
        signal.disconnect_all();
    }

    // ========================================================================
    // Connection Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_connection_id_retrieval() {
        let signal: Signal<i32> = Signal::new();
        let conn = signal.connect(|_| {});
        let id = conn.id();

        // ID should be valid (non-zero based on implementation)
        kani::assert(id.0 > 0, "connection has valid ID");
    }

    // ========================================================================
    // ScopedConnection Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_scoped_connection_new() {
        let signal: Signal<i32> = Signal::new();
        let conn = signal.connect(|_| {});

        let scoped = ScopedConnection::new(conn);
        kani::assert(
            scoped.connection.is_some(),
            "new ScopedConnection has connection",
        );
    }

    #[kani::proof]
    fn proof_scoped_connection_release() {
        let signal: Signal<i32> = Signal::new();
        let conn = signal.connect(|_| {});

        let scoped = ScopedConnection::new(conn);
        let released = scoped.release();

        kani::assert(released.is_some(), "release returns connection");
    }

    // ========================================================================
    // ConnectionGroup Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_connection_group_new_empty() {
        let group = ConnectionGroup::new();
        kani::assert(group.is_empty(), "new ConnectionGroup is empty");
        kani::assert(group.len() == 0, "len is 0");
    }

    #[kani::proof]
    fn proof_connection_group_default_empty() {
        let group = ConnectionGroup::default();
        kani::assert(group.is_empty(), "default ConnectionGroup is empty");
    }

    #[kani::proof]
    fn proof_connection_group_add() {
        let signal: Signal<i32> = Signal::new();
        let conn = signal.connect(|_| {});

        let mut group = ConnectionGroup::new();
        group.add(conn);

        kani::assert(!group.is_empty(), "not empty after add");
        kani::assert(group.len() == 1, "len is 1");
    }

    #[kani::proof]
    fn proof_connection_group_multiple_add() {
        let signal: Signal<i32> = Signal::new();
        let conn1 = signal.connect(|_| {});
        let conn2 = signal.connect(|_| {});

        let mut group = ConnectionGroup::new();
        group.add(conn1);
        group.add(conn2);

        kani::assert(group.len() == 2, "len is 2 after two adds");
    }
}
