//! Channel utilities for drbot.
//!
//! This crate provides:
//! - Channel wrappers
//! - Buffered channels
//! - Fan-out/fan-in patterns
//! - Channel combinators

use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

/// Channel error types.
#[derive(Error, Debug)]
pub enum ChannelError {
    #[error("Channel closed")]
    Closed,

    #[error("Send failed")]
    SendFailed,

    #[error("Receive failed")]
    ReceiveFailed,

    #[error("Timeout")]
    Timeout,
}

/// Result type for channel operations.
pub type Result<T> = std::result::Result<T, ChannelError>;

/// Bounded channel with capacity.
pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = mpsc::channel(capacity);
    (Sender { inner: tx }, Receiver { inner: rx })
}

/// Unbounded channel.
pub fn unbounded<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        UnboundedSender { inner: tx },
        UnboundedReceiver { inner: rx },
    )
}

/// Oneshot channel for single response.
pub fn oneshot<T>() -> (OneshotSender<T>, OneshotReceiver<T>) {
    let (tx, rx) = oneshot::channel();
    (
        OneshotSender { inner: Some(tx) },
        OneshotReceiver { inner: rx },
    )
}

/// Bounded sender.
#[derive(Debug)]
pub struct Sender<T> {
    inner: mpsc::Sender<T>,
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Sender<T> {
    /// Send a value.
    pub async fn send(&self, value: T) -> Result<()> {
        self.inner
            .send(value)
            .await
            .map_err(|_| ChannelError::SendFailed)
    }

    /// Try to send without blocking.
    pub fn try_send(&self, value: T) -> Result<()> {
        self.inner
            .try_send(value)
            .map_err(|_| ChannelError::SendFailed)
    }

    /// Check if channel is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    /// Get remaining capacity.
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}

/// Bounded receiver.
pub struct Receiver<T> {
    inner: mpsc::Receiver<T>,
}

impl<T> Receiver<T> {
    /// Receive a value.
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await
    }

    /// Try to receive without blocking.
    pub fn try_recv(&mut self) -> Result<T> {
        self.inner
            .try_recv()
            .map_err(|_| ChannelError::ReceiveFailed)
    }

    /// Close the receiver.
    pub fn close(&mut self) {
        self.inner.close()
    }
}

/// Unbounded sender.
#[derive(Debug)]
pub struct UnboundedSender<T> {
    inner: mpsc::UnboundedSender<T>,
}

impl<T> Clone for UnboundedSender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> UnboundedSender<T> {
    /// Send a value.
    pub fn send(&self, value: T) -> Result<()> {
        self.inner.send(value).map_err(|_| ChannelError::SendFailed)
    }

    /// Check if channel is closed.
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
}

/// Unbounded receiver.
pub struct UnboundedReceiver<T> {
    inner: mpsc::UnboundedReceiver<T>,
}

impl<T> UnboundedReceiver<T> {
    /// Receive a value.
    pub async fn recv(&mut self) -> Option<T> {
        self.inner.recv().await
    }

    /// Try to receive without blocking.
    pub fn try_recv(&mut self) -> Result<T> {
        self.inner
            .try_recv()
            .map_err(|_| ChannelError::ReceiveFailed)
    }

    /// Close the receiver.
    pub fn close(&mut self) {
        self.inner.close()
    }
}

/// Oneshot sender.
pub struct OneshotSender<T> {
    inner: Option<oneshot::Sender<T>>,
}

impl<T> OneshotSender<T> {
    /// Send a value (consumes sender).
    pub fn send(mut self, value: T) -> Result<()> {
        self.inner
            .take()
            .ok_or(ChannelError::Closed)?
            .send(value)
            .map_err(|_| ChannelError::SendFailed)
    }

    /// Check if receiver was dropped.
    pub fn is_closed(&self) -> bool {
        self.inner.as_ref().map_or(true, |s| s.is_closed())
    }
}

/// Oneshot receiver.
pub struct OneshotReceiver<T> {
    inner: oneshot::Receiver<T>,
}

impl<T> OneshotReceiver<T> {
    /// Receive the value.
    pub async fn recv(self) -> Result<T> {
        self.inner.await.map_err(|_| ChannelError::ReceiveFailed)
    }

    /// Try to receive without blocking.
    pub fn try_recv(&mut self) -> Result<T> {
        self.inner
            .try_recv()
            .map_err(|_| ChannelError::ReceiveFailed)
    }
}

/// Fan-out distributor.
pub struct FanOut<T: Clone> {
    senders: Vec<Sender<T>>,
}

impl<T: Clone> FanOut<T> {
    /// Create new fan-out.
    pub fn new() -> Self {
        Self {
            senders: Vec::new(),
        }
    }

    /// Add output channel.
    pub fn add(&mut self, sender: Sender<T>) {
        self.senders.push(sender);
    }

    /// Send to all outputs.
    pub async fn send(&self, value: T) -> Result<()> {
        for (i, sender) in self.senders.iter().enumerate() {
            let val = if i == self.senders.len() - 1 {
                value.clone()
            } else {
                value.clone()
            };
            sender.send(val).await?;
        }
        Ok(())
    }

    /// Get number of outputs.
    pub fn output_count(&self) -> usize {
        self.senders.len()
    }
}

impl<T: Clone> Default for FanOut<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Fan-in collector.
pub struct FanIn<T> {
    receiver: Receiver<T>,
    #[allow(dead_code)]
    sender: Sender<T>,
}

impl<T: Send + 'static> FanIn<T> {
    /// Create new fan-in with capacity.
    pub fn new(capacity: usize) -> (Self, FanInSender<T>) {
        let (tx, rx) = bounded(capacity);
        let sender = FanInSender { inner: tx.clone() };
        (
            Self {
                receiver: rx,
                sender: tx,
            },
            sender,
        )
    }

    /// Receive next value.
    pub async fn recv(&mut self) -> Option<T> {
        self.receiver.recv().await
    }
}

/// Fan-in sender (cloneable).
#[derive(Clone)]
pub struct FanInSender<T> {
    inner: Sender<T>,
}

impl<T> FanInSender<T> {
    /// Send a value.
    pub async fn send(&self, value: T) -> Result<()> {
        self.inner.send(value).await
    }
}

/// Request-response channel.
pub struct RequestChannel<Req, Resp> {
    sender: Sender<(Req, OneshotSender<Resp>)>,
}

impl<Req, Resp> RequestChannel<Req, Resp> {
    /// Create new request channel.
    pub fn new(capacity: usize) -> (Self, RequestReceiver<Req, Resp>) {
        let (tx, rx) = bounded(capacity);
        (Self { sender: tx }, RequestReceiver { receiver: rx })
    }

    /// Send request and get response.
    pub async fn request(&self, req: Req) -> Result<Resp> {
        let (resp_tx, resp_rx) = oneshot();
        self.sender.send((req, resp_tx)).await?;
        resp_rx.recv().await
    }
}

impl<Req, Resp> Clone for RequestChannel<Req, Resp> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

/// Request receiver.
pub struct RequestReceiver<Req, Resp> {
    receiver: Receiver<(Req, OneshotSender<Resp>)>,
}

impl<Req, Resp> RequestReceiver<Req, Resp> {
    /// Receive next request.
    pub async fn recv(&mut self) -> Option<(Req, OneshotSender<Resp>)> {
        self.receiver.recv().await
    }
}

/// Buffered channel with automatic batching.
pub struct BatchChannel<T> {
    sender: Sender<T>,
    batch_size: usize,
}

impl<T> BatchChannel<T> {
    /// Create new batch channel.
    pub fn new(capacity: usize, batch_size: usize) -> (Self, BatchReceiver<T>) {
        let (tx, rx) = bounded(capacity);
        (
            Self {
                sender: tx,
                batch_size,
            },
            BatchReceiver {
                receiver: rx,
                batch_size,
                buffer: Vec::with_capacity(batch_size),
            },
        )
    }

    /// Send a value.
    pub async fn send(&self, value: T) -> Result<()> {
        self.sender.send(value).await
    }

    /// Get batch size.
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }
}

impl<T> Clone for BatchChannel<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            batch_size: self.batch_size,
        }
    }
}

/// Batch receiver.
pub struct BatchReceiver<T> {
    receiver: Receiver<T>,
    batch_size: usize,
    buffer: Vec<T>,
}

impl<T> BatchReceiver<T> {
    /// Receive a batch.
    pub async fn recv_batch(&mut self) -> Option<Vec<T>> {
        // Fill buffer up to batch_size
        while self.buffer.len() < self.batch_size {
            match self.receiver.recv().await {
                Some(item) => self.buffer.push(item),
                None if self.buffer.is_empty() => return None,
                None => break,
            }
        }

        if self.buffer.is_empty() {
            None
        } else {
            Some(std::mem::replace(
                &mut self.buffer,
                Vec::with_capacity(self.batch_size),
            ))
        }
    }

    /// Receive single item.
    pub async fn recv(&mut self) -> Option<T> {
        self.receiver.recv().await
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
    async fn test_oneshot_channel() {
        let (tx, rx) = oneshot();

        tx.send("hello").unwrap();
        assert_eq!(rx.recv().await.unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_fan_in() {
        let (mut fan_in, sender) = FanIn::new(10);

        let s1 = sender.clone();
        let s2 = sender.clone();

        s1.send(1).await.unwrap();
        s2.send(2).await.unwrap();

        let v1 = fan_in.recv().await.unwrap();
        let v2 = fan_in.recv().await.unwrap();

        assert!((v1 == 1 && v2 == 2) || (v1 == 2 && v2 == 1));
    }

    #[tokio::test]
    async fn test_request_channel() {
        let (client, mut server) = RequestChannel::<i32, i32>::new(10);

        tokio::spawn(async move {
            while let Some((req, resp)) = server.recv().await {
                resp.send(req * 2).unwrap();
            }
        });

        let result = client.request(21).await.unwrap();
        assert_eq!(result, 42);
    }
}
