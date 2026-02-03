//! Send trait utilities for drbot.
//!
//! This crate provides:
//! - Send wrappers
//! - Send assertions
//! - Thread-safe helpers

use std::marker::PhantomData;
use thiserror::Error;

/// Send error types.
#[derive(Error, Debug, Clone)]
pub enum SendError {
    #[error("Not sendable")]
    NotSendable,

    #[error("Send failed: {0}")]
    Failed(String),
}

/// Result type for send operations.
pub type Result<T> = std::result::Result<T, SendError>;

/// Assert that a type is Send.
pub fn assert_send<T: Send>() {}

/// Assert that a value is Send.
pub fn assert_send_val<T: Send>(_: &T) {}

/// Wrapper that asserts Send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssertSend<T>(pub T);

// SAFETY: User asserts this is safe.
unsafe impl<T> Send for AssertSend<T> {}

impl<T> AssertSend<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.0
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// Send bound marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct SendBound<T: Send> {
    _marker: PhantomData<T>,
}

impl<T: Send> SendBound<T> {
    /// Create new.
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

/// Not Send marker.
#[derive(Debug, Clone, Copy, Default)]
pub struct NotSend {
    _marker: PhantomData<*const ()>,
}

impl NotSend {
    /// Create new.
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

/// Sendable wrapper.
pub struct Sendable<T> {
    value: T,
}

impl<T: Send> Sendable<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self { value }
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        self.value
    }
}

/// Check if type is Send at compile time.
pub const fn is_send<T: Send>() -> bool {
    true
}

/// Send container for sharing between threads.
pub struct SendContainer<T: Send> {
    value: Option<T>,
}

impl<T: Send> SendContainer<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self { value: Some(value) }
    }

    /// Create empty.
    pub fn empty() -> Self {
        Self { value: None }
    }

    /// Take value.
    pub fn take(&mut self) -> Option<T> {
        self.value.take()
    }

    /// Put value.
    pub fn put(&mut self, value: T) {
        self.value = Some(value);
    }

    /// Has value.
    pub fn has_value(&self) -> bool {
        self.value.is_some()
    }

    /// Get reference.
    pub fn get(&self) -> Option<&T> {
        self.value.as_ref()
    }
}

impl<T: Send> Default for SendContainer<T> {
    fn default() -> Self {
        Self::empty()
    }
}

/// Send channel (single producer, single consumer).
pub struct SendChannel<T: Send> {
    buffer: std::sync::Mutex<Option<T>>,
    condvar: std::sync::Condvar,
}

impl<T: Send> SendChannel<T> {
    /// Create new.
    pub fn new() -> Self {
        Self {
            buffer: std::sync::Mutex::new(None),
            condvar: std::sync::Condvar::new(),
        }
    }

    /// Send value.
    pub fn send(&self, value: T) {
        let mut buffer = self.buffer.lock().unwrap();
        *buffer = Some(value);
        self.condvar.notify_one();
    }

    /// Try receive.
    pub fn try_recv(&self) -> Option<T> {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.take()
    }

    /// Receive (blocking).
    pub fn recv(&self) -> T {
        let mut buffer = self.buffer.lock().unwrap();
        while buffer.is_none() {
            buffer = self.condvar.wait(buffer).unwrap();
        }
        buffer.take().unwrap()
    }
}

impl<T: Send> Default for SendChannel<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_send() {
        assert_send::<i32>();
        assert_send::<String>();
        assert_send_val(&42);
    }

    #[test]
    fn test_sendable() {
        let s = Sendable::new(42);
        assert_eq!(*s.get(), 42);
    }

    #[test]
    fn test_send_container() {
        let mut c = SendContainer::new(42);
        assert!(c.has_value());
        assert_eq!(c.take(), Some(42));
        assert!(!c.has_value());
    }

    #[test]
    fn test_send_channel() {
        let channel = SendChannel::new();
        channel.send(42);
        assert_eq!(channel.recv(), 42);
    }
}
