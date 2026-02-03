//! Batch processing utilities for drbot.
//!
//! This crate provides:
//! - Batch operations
//! - Chunked processing
//! - Parallel batch execution
//! - Progress tracking

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Semaphore;
use uuid::Uuid;

/// Batch error types.
#[derive(Error, Debug)]
pub enum BatchError {
    #[error("Batch failed: {0}")]
    Failed(String),

    #[error("Item error at index {index}: {message}")]
    ItemError { index: usize, message: String },

    #[error("Cancelled")]
    Cancelled,

    #[error("Timeout")]
    Timeout,

    #[error("Resource exhausted")]
    ResourceExhausted,
}

/// Result type for batch operations.
pub type Result<T> = std::result::Result<T, BatchError>;

/// Batch item result.
#[derive(Debug, Clone)]
pub enum ItemResult<T> {
    /// Success.
    Success(T),
    /// Failure.
    Failure(String),
    /// Skipped.
    Skipped,
}

impl<T> ItemResult<T> {
    /// Check if success.
    pub fn is_success(&self) -> bool {
        matches!(self, ItemResult::Success(_))
    }

    /// Get success value.
    pub fn success(self) -> Option<T> {
        match self {
            ItemResult::Success(v) => Some(v),
            _ => None,
        }
    }

    /// Map success value.
    pub fn map<U, F>(self, f: F) -> ItemResult<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            ItemResult::Success(v) => ItemResult::Success(f(v)),
            ItemResult::Failure(e) => ItemResult::Failure(e),
            ItemResult::Skipped => ItemResult::Skipped,
        }
    }
}

/// Batch result.
#[derive(Debug, Clone)]
pub struct BatchResult<T> {
    /// Item results.
    pub items: Vec<ItemResult<T>>,
    /// Total processed.
    pub processed: u64,
    /// Successful count.
    pub succeeded: u64,
    /// Failed count.
    pub failed: u64,
    /// Skipped count.
    pub skipped: u64,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

impl<T> BatchResult<T> {
    /// Create new result.
    pub fn new(items: Vec<ItemResult<T>>, duration_ms: u64) -> Self {
        let mut succeeded = 0;
        let mut failed = 0;
        let mut skipped = 0;

        for item in &items {
            match item {
                ItemResult::Success(_) => succeeded += 1,
                ItemResult::Failure(_) => failed += 1,
                ItemResult::Skipped => skipped += 1,
            }
        }

        Self {
            processed: items.len() as u64,
            items,
            succeeded,
            failed,
            skipped,
            duration_ms,
        }
    }

    /// Get successful items.
    pub fn successes(&self) -> Vec<&T> {
        self.items
            .iter()
            .filter_map(|r| match r {
                ItemResult::Success(v) => Some(v),
                _ => None,
            })
            .collect()
    }

    /// Get failure messages.
    pub fn failures(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter_map(|r| match r {
                ItemResult::Failure(e) => Some(e.as_str()),
                _ => None,
            })
            .collect()
    }

    /// All succeeded.
    pub fn all_succeeded(&self) -> bool {
        self.failed == 0
    }
}

/// Batch processor trait.
#[async_trait]
pub trait BatchProcessor<T, R>: Send + Sync {
    /// Process a single item.
    async fn process(&self, item: T) -> Result<R>;
}

/// Function-based processor.
pub struct FnProcessor<F> {
    f: F,
}

impl<F> FnProcessor<F> {
    /// Create new processor.
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

#[async_trait]
impl<T, R, F, Fut> BatchProcessor<T, R> for FnProcessor<F>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync,
    Fut: std::future::Future<Output = Result<R>> + Send,
{
    async fn process(&self, item: T) -> Result<R> {
        (self.f)(item).await
    }
}

/// Batch configuration.
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Chunk size.
    pub chunk_size: usize,
    /// Max concurrent operations.
    pub concurrency: usize,
    /// Continue on error.
    pub continue_on_error: bool,
    /// Retry failed items.
    pub retry_count: u32,
    /// Retry delay in milliseconds.
    pub retry_delay_ms: u64,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            chunk_size: 100,
            concurrency: 10,
            continue_on_error: true,
            retry_count: 0,
            retry_delay_ms: 1000,
        }
    }
}

impl BatchConfig {
    /// Set chunk size.
    pub fn chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }

    /// Set concurrency.
    pub fn concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency;
        self
    }

    /// Set continue on error.
    pub fn continue_on_error(mut self, continue_on_error: bool) -> Self {
        self.continue_on_error = continue_on_error;
        self
    }

    /// Set retry count.
    pub fn retry(mut self, count: u32) -> Self {
        self.retry_count = count;
        self
    }
}

/// Progress callback.
pub type ProgressCallback = Box<dyn Fn(Progress) + Send + Sync>;

/// Batch progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    /// Total items.
    pub total: u64,
    /// Processed items.
    pub processed: u64,
    /// Succeeded items.
    pub succeeded: u64,
    /// Failed items.
    pub failed: u64,
    /// Percentage complete.
    pub percentage: f64,
    /// Items per second.
    pub rate: f64,
    /// Estimated time remaining in seconds.
    pub eta_seconds: Option<f64>,
}

impl Progress {
    /// Create new progress.
    pub fn new(total: u64) -> Self {
        Self {
            total,
            processed: 0,
            succeeded: 0,
            failed: 0,
            percentage: 0.0,
            rate: 0.0,
            eta_seconds: None,
        }
    }

    /// Update progress.
    pub fn update(&mut self, processed: u64, succeeded: u64, failed: u64, elapsed_ms: u64) {
        self.processed = processed;
        self.succeeded = succeeded;
        self.failed = failed;
        self.percentage = if self.total > 0 {
            (processed as f64 / self.total as f64) * 100.0
        } else {
            100.0
        };
        self.rate = if elapsed_ms > 0 {
            (processed as f64 / elapsed_ms as f64) * 1000.0
        } else {
            0.0
        };
        self.eta_seconds = if self.rate > 0.0 && processed < self.total {
            Some((self.total - processed) as f64 / self.rate)
        } else {
            None
        };
    }
}

/// Batch executor.
pub struct BatchExecutor<T, R, P: BatchProcessor<T, R>> {
    processor: Arc<P>,
    config: BatchConfig,
    progress_callback: Option<ProgressCallback>,
    _marker: std::marker::PhantomData<(T, R)>,
}

impl<T, R, P> BatchExecutor<T, R, P>
where
    T: Clone + Send + 'static,
    R: Send + 'static,
    P: BatchProcessor<T, R> + 'static,
{
    /// Create new executor.
    pub fn new(processor: P, config: BatchConfig) -> Self {
        Self {
            processor: Arc::new(processor),
            config,
            progress_callback: None,
            _marker: std::marker::PhantomData,
        }
    }

    /// Set progress callback.
    pub fn on_progress(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Execute batch.
    pub async fn execute(&self, items: Vec<T>) -> BatchResult<R> {
        let start = std::time::Instant::now();
        let total = items.len() as u64;
        let semaphore = Arc::new(Semaphore::new(self.config.concurrency));
        let processed = Arc::new(AtomicU64::new(0));
        let succeeded = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicU64::new(0));

        let mut results = Vec::with_capacity(items.len());

        for chunk in items.chunks(self.config.chunk_size) {
            let chunk_results = self
                .process_chunk(
                    chunk.iter().cloned().collect(),
                    &semaphore,
                    &processed,
                    &succeeded,
                    &failed,
                    total,
                    start,
                )
                .await;

            for result in chunk_results {
                if !self.config.continue_on_error && matches!(result, ItemResult::Failure(_)) {
                    results.push(result);
                    return BatchResult::new(results, start.elapsed().as_millis() as u64);
                }
                results.push(result);
            }
        }

        BatchResult::new(results, start.elapsed().as_millis() as u64)
    }

    async fn process_chunk(
        &self,
        items: Vec<T>,
        semaphore: &Arc<Semaphore>,
        processed: &Arc<AtomicU64>,
        succeeded: &Arc<AtomicU64>,
        failed: &Arc<AtomicU64>,
        total: u64,
        start: std::time::Instant,
    ) -> Vec<ItemResult<R>> {
        let futures: Vec<_> = items
            .into_iter()
            .map(|item| {
                let processor = self.processor.clone();
                let semaphore = semaphore.clone();
                let processed = processed.clone();
                let succeeded = succeeded.clone();
                let failed = failed.clone();
                let retry_count = self.config.retry_count;
                let retry_delay = self.config.retry_delay_ms;

                async move {
                    let _permit = semaphore.acquire().await.unwrap();

                    let mut last_error = None;
                    for attempt in 0..=retry_count {
                        match processor.process(item).await {
                            Ok(result) => {
                                processed.fetch_add(1, Ordering::SeqCst);
                                succeeded.fetch_add(1, Ordering::SeqCst);
                                return ItemResult::Success(result);
                            }
                            Err(e) => {
                                last_error = Some(e.to_string());
                                if attempt < retry_count {
                                    tokio::time::sleep(std::time::Duration::from_millis(
                                        retry_delay,
                                    ))
                                    .await;
                                }
                            }
                        }
                        // Need to return early if we can't clone T
                        break;
                    }

                    processed.fetch_add(1, Ordering::SeqCst);
                    failed.fetch_add(1, Ordering::SeqCst);
                    ItemResult::Failure(last_error.unwrap_or_else(|| "Unknown error".to_string()))
                }
            })
            .collect();

        // Can't use join_all with move closures easily, so use channels
        let mut results = Vec::new();
        for future in futures {
            results.push(future.await);
        }

        // Report progress
        if let Some(ref callback) = self.progress_callback {
            let mut progress = Progress::new(total);
            progress.update(
                processed.load(Ordering::SeqCst),
                succeeded.load(Ordering::SeqCst),
                failed.load(Ordering::SeqCst),
                start.elapsed().as_millis() as u64,
            );
            callback(progress);
        }

        results
    }
}

/// Chunk iterator.
pub fn chunk<T: Clone>(items: Vec<T>, size: usize) -> impl Iterator<Item = Vec<T>> {
    items
        .into_iter()
        .collect::<Vec<_>>()
        .chunks(size)
        .map(|c| c.to_vec())
        .collect::<Vec<_>>()
        .into_iter()
}

/// Simple batch map.
pub async fn batch_map<T, R, F, Fut>(items: Vec<T>, f: F, concurrency: usize) -> Vec<Result<R>>
where
    T: Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + Clone + 'static,
    Fut: std::future::Future<Output = Result<R>> + Send,
{
    let semaphore = Arc::new(Semaphore::new(concurrency));

    let futures: Vec<_> = items
        .into_iter()
        .map(|item| {
            let semaphore = semaphore.clone();
            let f = f.clone();
            async move {
                let _permit = semaphore.acquire().await.unwrap();
                f(item).await
            }
        })
        .collect();

    join_all(futures).await
}

/// Batch job status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Batch job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJob {
    /// Job ID.
    pub id: Uuid,
    /// Job name.
    pub name: String,
    /// Status.
    pub status: JobStatus,
    /// Progress.
    pub progress: Progress,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Started at.
    pub started_at: Option<DateTime<Utc>>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
    /// Error message.
    pub error: Option<String>,
}

impl BatchJob {
    /// Create new job.
    pub fn new(name: impl Into<String>, total: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            status: JobStatus::Pending,
            progress: Progress::new(total),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error: None,
        }
    }

    /// Start job.
    pub fn start(&mut self) {
        self.status = JobStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// Complete job.
    pub fn complete(&mut self) {
        self.status = JobStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    /// Fail job.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = JobStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error = Some(error.into());
    }

    /// Cancel job.
    pub fn cancel(&mut self) {
        self.status = JobStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_item_result() {
        let success: ItemResult<i32> = ItemResult::Success(42);
        assert!(success.is_success());
        assert_eq!(success.success(), Some(42));

        let failure: ItemResult<i32> = ItemResult::Failure("error".to_string());
        assert!(!failure.is_success());
    }

    #[test]
    fn test_batch_result() {
        let items = vec![
            ItemResult::Success(1),
            ItemResult::Success(2),
            ItemResult::Failure("error".to_string()),
        ];

        let result = BatchResult::new(items, 100);

        assert_eq!(result.processed, 3);
        assert_eq!(result.succeeded, 2);
        assert_eq!(result.failed, 1);
        assert!(!result.all_succeeded());
    }

    #[test]
    fn test_progress() {
        let mut progress = Progress::new(100);
        progress.update(50, 45, 5, 5000);

        assert_eq!(progress.percentage, 50.0);
        assert_eq!(progress.rate, 10.0); // 50 items / 5 seconds
        assert!(progress.eta_seconds.is_some());
    }

    #[test]
    fn test_batch_config() {
        let config = BatchConfig::default()
            .chunk_size(50)
            .concurrency(5)
            .retry(3);

        assert_eq!(config.chunk_size, 50);
        assert_eq!(config.concurrency, 5);
        assert_eq!(config.retry_count, 3);
    }

    #[tokio::test]
    async fn test_batch_map() {
        let items = vec![1, 2, 3, 4, 5];
        let results = batch_map(items, |n| async move { Ok::<_, BatchError>(n * 2) }, 2).await;

        assert_eq!(results.len(), 5);
        assert!(results.iter().all(|r| r.is_ok()));
    }

    #[test]
    fn test_batch_job() {
        let mut job = BatchJob::new("test", 100);
        assert_eq!(job.status, JobStatus::Pending);

        job.start();
        assert_eq!(job.status, JobStatus::Running);
        assert!(job.started_at.is_some());

        job.complete();
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_chunk() {
        let items = vec![1, 2, 3, 4, 5, 6, 7];
        let chunks: Vec<Vec<i32>> = chunk(items, 3).collect();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], vec![1, 2, 3]);
        assert_eq!(chunks[1], vec![4, 5, 6]);
        assert_eq!(chunks[2], vec![7]);
    }
}
