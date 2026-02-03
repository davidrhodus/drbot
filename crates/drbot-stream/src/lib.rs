//! Response streaming control for drbot.
//!
//! Control and manipulate streaming responses.
//!
//! # Features
//!
//! - Pause/resume streaming
//! - Stream transformation
//! - Progress tracking
//! - Rate limiting
//! - Stream multiplexing

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

/// Stream result type.
pub type Result<T> = std::result::Result<T, StreamError>;

/// Stream errors.
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("Stream not found: {0}")]
    NotFound(String),
    #[error("Stream cancelled")]
    Cancelled,
    #[error("Stream paused")]
    Paused,
    #[error("Rate limited")]
    RateLimited,
    #[error("Transform error: {0}")]
    TransformError(String),
    #[error("Channel closed")]
    ChannelClosed,
}

/// Stream chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Chunk ID.
    pub id: Uuid,
    /// Stream ID.
    pub stream_id: Uuid,
    /// Content.
    pub content: String,
    /// Chunk index.
    pub index: usize,
    /// Is final chunk.
    pub is_final: bool,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl StreamChunk {
    /// Create a new chunk.
    pub fn new(stream_id: Uuid, content: &str, index: usize) -> Self {
        Self {
            id: Uuid::new_v4(),
            stream_id,
            content: content.to_string(),
            index,
            is_final: false,
            metadata: HashMap::new(),
            timestamp: Utc::now(),
        }
    }

    /// Mark as final.
    pub fn final_chunk(mut self) -> Self {
        self.is_final = true;
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: &str, value: serde_json::Value) -> Self {
        self.metadata.insert(key.to_string(), value);
        self
    }
}

/// Stream state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamState {
    Active,
    Paused,
    Cancelled,
    Completed,
    Error,
}

/// Stream info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    /// Stream ID.
    pub id: Uuid,
    /// Stream name.
    pub name: String,
    /// State.
    pub state: StreamState,
    /// Total chunks received.
    pub chunks_received: usize,
    /// Total bytes received.
    pub bytes_received: usize,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
    /// Current rate (chunks/sec).
    pub rate: f64,
}

/// Controllable stream.
pub struct ControllableStream {
    id: Uuid,
    name: String,
    state: Arc<RwLock<StreamState>>,
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
    chunks_received: Arc<AtomicUsize>,
    bytes_received: Arc<AtomicUsize>,
    started_at: DateTime<Utc>,
    completed_at: Arc<RwLock<Option<DateTime<Utc>>>>,
    tx: mpsc::Sender<StreamChunk>,
    rx: Arc<RwLock<mpsc::Receiver<StreamChunk>>>,
    event_tx: broadcast::Sender<StreamEvent>,
}

impl ControllableStream {
    /// Create a new controllable stream.
    pub fn new(name: &str) -> Self {
        let (tx, rx) = mpsc::channel(100);
        let (event_tx, _) = broadcast::channel(100);

        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            state: Arc::new(RwLock::new(StreamState::Active)),
            paused: Arc::new(AtomicBool::new(false)),
            cancelled: Arc::new(AtomicBool::new(false)),
            chunks_received: Arc::new(AtomicUsize::new(0)),
            bytes_received: Arc::new(AtomicUsize::new(0)),
            started_at: Utc::now(),
            completed_at: Arc::new(RwLock::new(None)),
            tx,
            rx: Arc::new(RwLock::new(rx)),
            event_tx,
        }
    }

    /// Get stream ID.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Get stream name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get current state.
    pub async fn state(&self) -> StreamState {
        *self.state.read().await
    }

    /// Pause the stream.
    pub async fn pause(&self) -> Result<()> {
        self.paused.store(true, Ordering::SeqCst);
        *self.state.write().await = StreamState::Paused;
        let _ = self
            .event_tx
            .send(StreamEvent::Paused { stream_id: self.id });
        Ok(())
    }

    /// Resume the stream.
    pub async fn resume(&self) -> Result<()> {
        self.paused.store(false, Ordering::SeqCst);
        *self.state.write().await = StreamState::Active;
        let _ = self
            .event_tx
            .send(StreamEvent::Resumed { stream_id: self.id });
        Ok(())
    }

    /// Cancel the stream.
    pub async fn cancel(&self) -> Result<()> {
        self.cancelled.store(true, Ordering::SeqCst);
        *self.state.write().await = StreamState::Cancelled;
        let _ = self
            .event_tx
            .send(StreamEvent::Cancelled { stream_id: self.id });
        Ok(())
    }

    /// Check if paused.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Check if cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Push a chunk to the stream.
    pub async fn push(&self, chunk: StreamChunk) -> Result<()> {
        if self.is_cancelled() {
            return Err(StreamError::Cancelled);
        }

        // Wait if paused
        while self.is_paused() {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            if self.is_cancelled() {
                return Err(StreamError::Cancelled);
            }
        }

        self.chunks_received.fetch_add(1, Ordering::SeqCst);
        self.bytes_received
            .fetch_add(chunk.content.len(), Ordering::SeqCst);

        let is_final = chunk.is_final;

        self.tx
            .send(chunk.clone())
            .await
            .map_err(|_| StreamError::ChannelClosed)?;

        let _ = self.event_tx.send(StreamEvent::ChunkReceived {
            stream_id: self.id,
            chunk: chunk.clone(),
        });

        if is_final {
            *self.state.write().await = StreamState::Completed;
            *self.completed_at.write().await = Some(Utc::now());
            let _ = self
                .event_tx
                .send(StreamEvent::Completed { stream_id: self.id });
        }

        Ok(())
    }

    /// Get the next chunk.
    pub async fn next(&self) -> Option<StreamChunk> {
        self.rx.write().await.recv().await
    }

    /// Subscribe to stream events.
    pub fn subscribe(&self) -> broadcast::Receiver<StreamEvent> {
        self.event_tx.subscribe()
    }

    /// Get stream info.
    pub async fn info(&self) -> StreamInfo {
        let elapsed = Utc::now()
            .signed_duration_since(self.started_at)
            .num_seconds() as f64;
        let rate = if elapsed > 0.0 {
            self.chunks_received.load(Ordering::SeqCst) as f64 / elapsed
        } else {
            0.0
        };

        StreamInfo {
            id: self.id,
            name: self.name.clone(),
            state: *self.state.read().await,
            chunks_received: self.chunks_received.load(Ordering::SeqCst),
            bytes_received: self.bytes_received.load(Ordering::SeqCst),
            started_at: self.started_at,
            completed_at: *self.completed_at.read().await,
            rate,
        }
    }
}

/// Stream event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Chunk received.
    ChunkReceived { stream_id: Uuid, chunk: StreamChunk },
    /// Stream paused.
    Paused { stream_id: Uuid },
    /// Stream resumed.
    Resumed { stream_id: Uuid },
    /// Stream cancelled.
    Cancelled { stream_id: Uuid },
    /// Stream completed.
    Completed { stream_id: Uuid },
    /// Stream error.
    Error { stream_id: Uuid, message: String },
}

/// Stream transformer trait.
#[async_trait]
pub trait StreamTransformer: Send + Sync {
    /// Transform a chunk.
    async fn transform(&self, chunk: StreamChunk) -> Result<StreamChunk>;
}

/// Rate limiter for streams.
pub struct StreamRateLimiter {
    max_rate: f64,
    last_chunk: Arc<RwLock<std::time::Instant>>,
    tokens: Arc<AtomicU64>,
    max_tokens: u64,
}

impl StreamRateLimiter {
    /// Create a new rate limiter.
    pub fn new(max_rate: f64) -> Self {
        Self {
            max_rate,
            last_chunk: Arc::new(RwLock::new(std::time::Instant::now())),
            tokens: Arc::new(AtomicU64::new(100)),
            max_tokens: 100,
        }
    }

    /// Check if rate limited.
    pub async fn check(&self) -> bool {
        let tokens = self.tokens.load(Ordering::SeqCst);
        if tokens > 0 {
            self.tokens.fetch_sub(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// Refill tokens.
    pub async fn refill(&self) {
        let last = *self.last_chunk.read().await;
        let elapsed = last.elapsed().as_secs_f64();
        let new_tokens = (elapsed * self.max_rate) as u64;

        if new_tokens > 0 {
            let current = self.tokens.load(Ordering::SeqCst);
            let new_total = (current + new_tokens).min(self.max_tokens);
            self.tokens.store(new_total, Ordering::SeqCst);
            *self.last_chunk.write().await = std::time::Instant::now();
        }
    }
}

/// Stream multiplexer.
pub struct StreamMultiplexer {
    streams: Arc<RwLock<HashMap<Uuid, Arc<ControllableStream>>>>,
    output_tx: broadcast::Sender<StreamChunk>,
}

impl StreamMultiplexer {
    /// Create a new multiplexer.
    pub fn new() -> Self {
        let (output_tx, _) = broadcast::channel(1000);
        Self {
            streams: Arc::new(RwLock::new(HashMap::new())),
            output_tx,
        }
    }

    /// Add a stream.
    pub async fn add(&self, stream: Arc<ControllableStream>) {
        let id = stream.id();
        self.streams.write().await.insert(id, stream.clone());

        // Forward chunks
        let output_tx = self.output_tx.clone();
        let stream_clone = stream.clone();

        tokio::spawn(async move {
            while let Some(chunk) = stream_clone.next().await {
                let _ = output_tx.send(chunk);
            }
        });
    }

    /// Remove a stream.
    pub async fn remove(&self, id: Uuid) {
        if let Some(stream) = self.streams.write().await.remove(&id) {
            let _ = stream.cancel().await;
        }
    }

    /// Subscribe to multiplexed output.
    pub fn subscribe(&self) -> broadcast::Receiver<StreamChunk> {
        self.output_tx.subscribe()
    }

    /// Get all stream infos.
    pub async fn infos(&self) -> Vec<StreamInfo> {
        let streams = self.streams.read().await;
        let mut infos = Vec::new();
        for stream in streams.values() {
            infos.push(stream.info().await);
        }
        infos
    }
}

impl Default for StreamMultiplexer {
    fn default() -> Self {
        Self::new()
    }
}

/// Stream collector for buffering.
pub struct StreamCollector {
    chunks: Arc<RwLock<Vec<StreamChunk>>>,
    complete: Arc<AtomicBool>,
}

impl StreamCollector {
    /// Create a new collector.
    pub fn new() -> Self {
        Self {
            chunks: Arc::new(RwLock::new(Vec::new())),
            complete: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Collect from a stream.
    pub async fn collect_from(&self, stream: &ControllableStream) {
        while let Some(chunk) = stream.next().await {
            let is_final = chunk.is_final;
            self.chunks.write().await.push(chunk);
            if is_final {
                self.complete.store(true, Ordering::SeqCst);
                break;
            }
        }
    }

    /// Get collected content.
    pub async fn content(&self) -> String {
        self.chunks
            .read()
            .await
            .iter()
            .map(|c| c.content.clone())
            .collect()
    }

    /// Get all chunks.
    pub async fn chunks(&self) -> Vec<StreamChunk> {
        self.chunks.read().await.clone()
    }

    /// Is collection complete.
    pub fn is_complete(&self) -> bool {
        self.complete.load(Ordering::SeqCst)
    }

    /// Clear collected data.
    pub async fn clear(&self) {
        self.chunks.write().await.clear();
        self.complete.store(false, Ordering::SeqCst);
    }
}

impl Default for StreamCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Uppercase transformer for testing.
pub struct UppercaseTransformer;

#[async_trait]
impl StreamTransformer for UppercaseTransformer {
    async fn transform(&self, mut chunk: StreamChunk) -> Result<StreamChunk> {
        chunk.content = chunk.content.to_uppercase();
        Ok(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_controllable_stream() {
        let stream = ControllableStream::new("test");

        let chunk = StreamChunk::new(stream.id(), "Hello", 0);
        stream.push(chunk).await.unwrap();

        let received = stream.next().await.unwrap();
        assert_eq!(received.content, "Hello");
    }

    #[tokio::test]
    async fn test_stream_pause_resume() {
        let stream = Arc::new(ControllableStream::new("test"));
        let stream_clone = stream.clone();

        // Start pushing in background
        tokio::spawn(async move {
            for i in 0..5 {
                let chunk = StreamChunk::new(stream_clone.id(), &format!("Chunk {}", i), i);
                stream_clone.push(chunk).await.unwrap();
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
            let final_chunk = StreamChunk::new(stream_clone.id(), "Final", 5).final_chunk();
            stream_clone.push(final_chunk).await.unwrap();
        });

        // Pause briefly
        stream.pause().await.unwrap();
        assert!(stream.is_paused());

        stream.resume().await.unwrap();
        assert!(!stream.is_paused());

        // Collect all
        let collector = StreamCollector::new();
        collector.collect_from(&stream).await;

        assert!(collector.is_complete());
    }

    #[tokio::test]
    async fn test_stream_cancel() {
        let stream = ControllableStream::new("test");

        stream.cancel().await.unwrap();

        let chunk = StreamChunk::new(stream.id(), "Hello", 0);
        let result = stream.push(chunk).await;

        assert!(matches!(result, Err(StreamError::Cancelled)));
    }

    #[tokio::test]
    async fn test_transformer() {
        let transformer = UppercaseTransformer;
        let chunk = StreamChunk::new(Uuid::new_v4(), "hello world", 0);

        let transformed = transformer.transform(chunk).await.unwrap();
        assert_eq!(transformed.content, "HELLO WORLD");
    }

    #[tokio::test]
    async fn test_stream_collector() {
        let stream = Arc::new(ControllableStream::new("test"));
        let stream_clone = stream.clone();

        tokio::spawn(async move {
            stream_clone
                .push(StreamChunk::new(stream_clone.id(), "Hello ", 0))
                .await
                .unwrap();
            stream_clone
                .push(StreamChunk::new(stream_clone.id(), "World", 1).final_chunk())
                .await
                .unwrap();
        });

        let collector = StreamCollector::new();
        collector.collect_from(&stream).await;

        assert_eq!(collector.content().await, "Hello World");
    }
}
