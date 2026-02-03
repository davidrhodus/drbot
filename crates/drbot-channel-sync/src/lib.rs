//! Synchronous channel utilities for drbot.
//!
//! This crate provides:
//! - Bounded/unbounded channels
//! - MPSC/SPMC channels
//! - Rendezvous channel

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use thiserror::Error;

/// Channel error types.
#[derive(Error, Debug, Clone)]
pub enum ChannelError {
    #[error("Channel closed")]
    Closed,

    #[error("Channel full")]
    Full,

    #[error("Channel empty")]
    Empty,

    #[error("Timeout")]
    Timeout,
}

/// Result type for channel operations.
pub type Result<T> = std::result::Result<T, ChannelError>;

/// Internal channel state.
struct ChannelInner<T> {
    queue: VecDeque<T>,
    closed: bool,
    capacity: Option<usize>,
}

/// Channel factory.
pub struct Channel<T>(std::marker::PhantomData<T>);

impl<T> Channel<T> {
    /// Create unbounded channel.
    pub fn unbounded() -> (Sender<T>, Receiver<T>) {
        let inner = Arc::new((
            Mutex::new(ChannelInner {
                queue: VecDeque::new(),
                closed: false,
                capacity: None,
            }),
            Condvar::new(), // Not empty
            Condvar::new(), // Not full
        ));

        (
            Sender {
                inner: inner.clone(),
            },
            Receiver { inner },
        )
    }

    /// Create bounded channel.
    pub fn bounded(capacity: usize) -> (Sender<T>, Receiver<T>) {
        let inner = Arc::new((
            Mutex::new(ChannelInner {
                queue: VecDeque::new(),
                closed: false,
                capacity: Some(capacity),
            }),
            Condvar::new(),
            Condvar::new(),
        ));

        (
            Sender {
                inner: inner.clone(),
            },
            Receiver { inner },
        )
    }
}

/// Channel sender.
pub struct Sender<T> {
    inner: Arc<(Mutex<ChannelInner<T>>, Condvar, Condvar)>,
}

impl<T> Sender<T> {
    /// Send value (blocking if bounded and full).
    pub fn send(&self, value: T) -> Result<()> {
        let mut inner = self.inner.0.lock().unwrap();

        // Wait if bounded and full
        while let Some(cap) = inner.capacity {
            if inner.closed {
                return Err(ChannelError::Closed);
            }
            if inner.queue.len() < cap {
                break;
            }
            inner = self.inner.2.wait(inner).unwrap();
        }

        if inner.closed {
            return Err(ChannelError::Closed);
        }

        inner.queue.push_back(value);
        self.inner.1.notify_one();
        Ok(())
    }

    /// Try to send (non-blocking).
    pub fn try_send(&self, value: T) -> Result<()> {
        let mut inner = self.inner.0.lock().unwrap();

        if inner.closed {
            return Err(ChannelError::Closed);
        }

        if let Some(cap) = inner.capacity {
            if inner.queue.len() >= cap {
                return Err(ChannelError::Full);
            }
        }

        inner.queue.push_back(value);
        self.inner.1.notify_one();
        Ok(())
    }

    /// Close the channel.
    pub fn close(&self) {
        let mut inner = self.inner.0.lock().unwrap();
        inner.closed = true;
        self.inner.1.notify_all();
        self.inner.2.notify_all();
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // Only close if this is the last sender
        if Arc::strong_count(&self.inner) == 2 {
            // 2 = this sender + receiver
            self.close();
        }
    }
}

/// Channel receiver.
pub struct Receiver<T> {
    inner: Arc<(Mutex<ChannelInner<T>>, Condvar, Condvar)>,
}

impl<T> Receiver<T> {
    /// Receive value (blocking).
    pub fn recv(&self) -> Result<T> {
        let mut inner = self.inner.0.lock().unwrap();

        loop {
            if let Some(value) = inner.queue.pop_front() {
                self.inner.2.notify_one();
                return Ok(value);
            }
            if inner.closed {
                return Err(ChannelError::Closed);
            }
            inner = self.inner.1.wait(inner).unwrap();
        }
    }

    /// Try to receive (non-blocking).
    pub fn try_recv(&self) -> Result<T> {
        let mut inner = self.inner.0.lock().unwrap();

        if let Some(value) = inner.queue.pop_front() {
            self.inner.2.notify_one();
            Ok(value)
        } else if inner.closed {
            Err(ChannelError::Closed)
        } else {
            Err(ChannelError::Empty)
        }
    }

    /// Receive with timeout.
    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Result<T> {
        let mut inner = self.inner.0.lock().unwrap();
        let deadline = std::time::Instant::now() + timeout;

        loop {
            if let Some(value) = inner.queue.pop_front() {
                self.inner.2.notify_one();
                return Ok(value);
            }
            if inner.closed {
                return Err(ChannelError::Closed);
            }

            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(ChannelError::Timeout);
            }

            let result = self.inner.1.wait_timeout(inner, remaining).unwrap();
            inner = result.0;
        }
    }

    /// Check if channel is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.0.lock().unwrap().queue.is_empty()
    }

    /// Get number of pending messages.
    pub fn len(&self) -> usize {
        self.inner.0.lock().unwrap().queue.len()
    }
}

/// Create iterator from receiver.
impl<T> IntoIterator for Receiver<T> {
    type Item = T;
    type IntoIter = ReceiverIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        ReceiverIter { receiver: self }
    }
}

/// Iterator over received values.
pub struct ReceiverIter<T> {
    receiver: Receiver<T>,
}

impl<T> Iterator for ReceiverIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.receiver.recv().ok()
    }
}

/// Create unbounded channel.
pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    Channel::unbounded()
}

/// Create bounded channel.
pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    Channel::bounded(capacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unbounded_channel() {
        let (tx, rx) = unbounded();

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();

        assert_eq!(rx.recv().unwrap(), 1);
        assert_eq!(rx.recv().unwrap(), 2);
        assert_eq!(rx.recv().unwrap(), 3);
    }

    #[test]
    fn test_bounded_channel() {
        let (tx, rx) = bounded(2);

        tx.try_send(1).unwrap();
        tx.try_send(2).unwrap();
        assert!(tx.try_send(3).is_err()); // Full

        rx.recv().unwrap();
        tx.try_send(3).unwrap(); // Now has space
    }

    #[test]
    fn test_channel_close() {
        let (tx, rx) = unbounded::<i32>();

        tx.close();

        assert!(tx.send(1).is_err());
        assert!(rx.recv().is_err());
    }

    #[test]
    fn test_channel_iter() {
        let (tx, rx) = unbounded();

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();
        tx.close();

        let values: Vec<_> = rx.into_iter().collect();
        assert_eq!(values, vec![1, 2, 3]);
    }
}
