//! Async channels for drbot.
//!
//! This crate provides:
//! - Bounded and unbounded channels
//! - Broadcast channels
//! - Priority channels
//! - Select over multiple channels

use std::collections::VecDeque;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex, Notify};

/// Channel error types.
#[derive(Error, Debug)]
pub enum ChannelError {
    #[error("Channel closed")]
    Closed,

    #[error("Channel full")]
    Full,

    #[error("Receive timeout")]
    Timeout,

    #[error("Send error")]
    SendError,
}

/// Result type for channel operations.
pub type Result<T> = std::result::Result<T, ChannelError>;

/// Bounded MPSC channel.
pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::channel(capacity);
    (Sender { inner: tx }, Receiver { inner: rx })
}

/// Unbounded MPSC channel.
pub fn unbounded<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        UnboundedSender { inner: tx },
        UnboundedReceiver { inner: rx },
    )
}

/// Bounded channel sender.
#[derive(Clone)]
pub struct Sender<T> {
    inner: mpsc::Sender<T>,
}

impl<T> Sender<T> {
    /// Send value.
    pub async fn send(&self, value: T) -> Result<()> {
        self.inner
            .send(value)
            .await
            .map_err(|_| ChannelError::Closed)
    }

    /// Try to send without blocking.
    pub fn try_send(&self, value: T) -> Result<()> {
        self.inner.try_send(value).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => ChannelError::Full,
            mpsc::error::TrySendError::Closed(_) => ChannelError::Closed,
        })
    }

    /// Check if channel is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Get number of available permits.
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}

/// Bounded channel receiver.
pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
}

impl<T> Receiver<T> {
    /// Receive value.
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await
    }

    /// Try to receive without blocking.
    pub fn try_recv(&mut self) -> Result<T> {
        self.inner.try_recv().map_err(|e| match e {
            mpsc::error::TryRecvError::Empty => ChannelError::Timeout,
            mpsc::error::TryRecvError::Disconnected => ChannelError::Closed,
        })
    }

    /// Receive with timeout.
    pub async fn recv_timeout(&mut self, timeout: std::time::Duration) -> Result<T> {
        tokio::time::timeout(timeout, self.inner.recv())
            .await
            .map_err(|_| ChannelError::Timeout)?
            .ok_or(ChannelError::Closed)
    }

    /// Close the channel.
    pub fn close(&mut self) {
        self.inner.close();
    }
}

/// Unbounded channel sender.
#[derive(Clone)]
pub struct UnboundedSender<T> {
    inner: mpsc::UnboundedSender<T>,
}

impl<T> UnboundedSender<T> {
    /// Send value.
    pub fn send(&self, value: T) -> Result<()> {
        self.inner.send(value).map_err(|_| ChannelError::Closed)
    }

    /// Check if closed.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

/// Unbounded channel receiver.
pub struct UnboundedReceiver<T> {
    inner: mpsc::UnboundedReceiver<T>,
}

impl<T> UnboundedReceiver<T> {
    /// Receive value.
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await
    }

    /// Try to receive.
    pub fn try_recv(&mut self) -> Result<T> {
        self.inner.try_recv().map_err(|e| match e {
            mpsc::error::TryRecvError::Empty => ChannelError::Timeout,
            mpsc::error::TryRecvError::Disconnected => ChannelError::Closed,
        })
    }
}

/// Oneshot channel for single value.
pub fn oneshot<T>() -> (OneshotSender<T>, OneshotReceiver<T>) {
    let (tx, rx) = oneshot::channel();
    (
        OneshotSender { inner: Some(tx) },
        OneshotReceiver { inner: rx },
    )
}

/// Oneshot sender.
pub struct OneshotSender<T> {
    inner: Option<oneshot::Sender<T>>,
}

impl<T> OneshotSender<T> {
    /// Send value (consumes sender).
    pub fn send(mut self, value: T) -> Result<()> {
        self.inner
            .take()
            .ok_or(ChannelError::Closed)?
            .send(value)
            .map_err(|_| ChannelError::Closed)
    }
}

/// Oneshot receiver.
pub struct OneshotReceiver<T> {
    inner: oneshot::Receiver<T>,
}

impl<T> OneshotReceiver<T> {
    /// Receive value.
    pub async fn recv(self) -> Result<T> {
        self.inner.await.map_err(|_| ChannelError::Closed)
    }

    /// Try to receive.
    pub fn try_recv(&mut self) -> Result<T> {
        self.inner.try_recv().map_err(|_| ChannelError::Closed)
    }
}

/// Broadcast channel.
pub fn broadcast<T: Clone>(capacity: usize) -> (BroadcastSender<T>, BroadcastReceiver<T>) {
    let (tx, rx) = broadcast::channel(capacity);
    (
        BroadcastSender { inner: tx },
        BroadcastReceiver { inner: rx },
    )
}

/// Broadcast sender.
#[derive(Clone)]
pub struct BroadcastSender<T: Clone> {
    inner: broadcast::Sender<T>,
}

impl<T: Clone> BroadcastSender<T> {
    /// Send to all receivers.
    pub fn send(&self, value: T) -> Result<usize> {
        self.inner.send(value).map_err(|_| ChannelError::Closed)
    }

    /// Subscribe to get a receiver.
    pub fn subscribe(&self) -> BroadcastReceiver<T> {
        BroadcastReceiver {
            inner: self.inner.subscribe(),
        }
    }

    /// Get receiver count.
    pub fn receiver_count(&self) -> usize {
        self.inner.receiver_count()
    }
}

/// Broadcast receiver.
pub struct BroadcastReceiver<T: Clone> {
    inner: broadcast::Receiver<T>,
}

impl<T: Clone> BroadcastReceiver<T> {
    /// Receive value.
    pub async fn recv(&mut self) -> Result<T> {
        self.inner.recv().await.map_err(|_| ChannelError::Closed)
    }
}

/// Priority channel (higher priority sent first).
pub struct PriorityChannel<T, P: Ord> {
    items: Arc<Mutex<VecDeque<(T, P)>>>,
    notify: Arc<Notify>,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl<T, P: Ord> PriorityChannel<T, P> {
    /// Create new priority channel.
    pub fn new() -> Self {
        Self {
            items: Arc::new(Mutex::new(VecDeque::new())),
            notify: Arc::new(Notify::new()),
            closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Send with priority.
    pub async fn send(&self, value: T, priority: P) -> Result<()> {
        if self.closed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ChannelError::Closed);
        }

        let mut items = self.items.lock().await;

        // Insert in priority order
        let pos = items
            .iter()
            .position(|(_, p)| *p < priority)
            .unwrap_or(items.len());
        items.insert(pos, (value, priority));

        self.notify.notify_one();
        Ok(())
    }

    /// Receive highest priority item.
    pub async fn recv(&self) -> Option<T> {
        loop {
            {
                let mut items = self.items.lock().await;
                if let Some((value, _)) = items.pop_front() {
                    return Some(value);
                }

                if self.closed.load(std::sync::atomic::Ordering::Acquire) && items.is_empty() {
                    return None;
                }
            }

            self.notify.notified().await;
        }
    }

    /// Close the channel.
    pub fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        self.notify.notify_waiters();
    }
}

impl<T, P: Ord> Default for PriorityChannel<T, P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, P: Ord> Clone for PriorityChannel<T, P> {
    fn clone(&self) -> Self {
        Self {
            items: Arc::clone(&self.items),
            notify: Arc::clone(&self.notify),
            closed: Arc::clone(&self.closed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bounded_channel() {
        let (tx, mut rx) = bounded(10);

        tx.send(42).await.unwrap();
        assert_eq!(rx.recv().await, Some(42));
    }

    #[tokio::test]
    async fn test_unbounded_channel() {
        let (tx, mut rx) = unbounded();

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();

        assert_eq!(rx.recv().await, Some(1));
        assert_eq!(rx.recv().await, Some(2));
        assert_eq!(rx.recv().await, Some(3));
    }

    #[tokio::test]
    async fn test_oneshot() {
        let (tx, rx) = oneshot();

        tx.send(42).unwrap();
        assert_eq!(rx.recv().await.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_broadcast() {
        let (tx, mut rx1) = broadcast(10);
        let mut rx2 = tx.subscribe();

        tx.send(42).unwrap();

        assert_eq!(rx1.recv().await.unwrap(), 42);
        assert_eq!(rx2.recv().await.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_priority_channel() {
        let chan = PriorityChannel::new();

        chan.send("low", 1).await.unwrap();
        chan.send("high", 10).await.unwrap();
        chan.send("medium", 5).await.unwrap();

        assert_eq!(chan.recv().await, Some("high"));
        assert_eq!(chan.recv().await, Some("medium"));
        assert_eq!(chan.recv().await, Some("low"));
    }

    #[tokio::test]
    async fn test_channel_close() {
        let (tx, mut rx) = bounded::<i32>(10);

        drop(tx);

        assert_eq!(rx.recv().await, None);
    }
}
