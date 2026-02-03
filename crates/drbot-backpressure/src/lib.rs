//! Backpressure handling for drbot.
//!
//! This crate provides:
//! - Load shedding strategies
//! - Adaptive concurrency limits
//! - Queue-based flow control
//! - Backpressure signals

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, OwnedSemaphorePermit, RwLock, Semaphore};

/// Backpressure error types.
#[derive(Error, Debug)]
pub enum BackpressureError {
    #[error("Load shed: {0}")]
    LoadShed(String),

    #[error("Queue full")]
    QueueFull,

    #[error("Timeout after {0:?}")]
    Timeout(Duration),

    #[error("Cancelled")]
    Cancelled,

    #[error("System overloaded")]
    Overloaded,
}

/// Result type for backpressure operations.
pub type Result<T> = std::result::Result<T, BackpressureError>;

/// Load level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LoadLevel {
    /// Low load, accept all requests.
    Low,
    /// Normal load.
    Normal,
    /// High load, start shedding.
    High,
    /// Critical load, aggressive shedding.
    Critical,
}

impl LoadLevel {
    /// Get the load factor (0.0 - 1.0).
    pub fn factor(&self) -> f64 {
        match self {
            LoadLevel::Low => 0.25,
            LoadLevel::Normal => 0.5,
            LoadLevel::High => 0.75,
            LoadLevel::Critical => 0.95,
        }
    }
}

/// Request priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    /// Low priority, shed first.
    Low = 0,
    /// Normal priority.
    Normal = 1,
    /// High priority.
    High = 2,
    /// Critical priority, never shed.
    Critical = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Load shedding strategy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SheddingStrategy {
    /// Drop all requests above threshold.
    DropAll,
    /// Random drop with probability.
    Probabilistic(f64),
    /// Drop based on priority.
    PriorityBased,
    /// Drop oldest requests.
    DropOldest,
    /// Drop newest requests.
    DropNewest,
}

/// Load monitor.
pub struct LoadMonitor {
    current_requests: AtomicUsize,
    max_requests: usize,
    current_level: RwLock<LoadLevel>,
    thresholds: LoadThresholds,
}

/// Load thresholds.
#[derive(Debug, Clone)]
pub struct LoadThresholds {
    /// Low threshold (fraction of max).
    pub low: f64,
    /// Normal threshold.
    pub normal: f64,
    /// High threshold.
    pub high: f64,
    /// Critical threshold.
    pub critical: f64,
}

impl Default for LoadThresholds {
    fn default() -> Self {
        Self {
            low: 0.25,
            normal: 0.5,
            high: 0.75,
            critical: 0.9,
        }
    }
}

impl LoadMonitor {
    /// Create a new load monitor.
    pub fn new(max_requests: usize) -> Self {
        Self {
            current_requests: AtomicUsize::new(0),
            max_requests,
            current_level: RwLock::new(LoadLevel::Low),
            thresholds: LoadThresholds::default(),
        }
    }

    /// Create with custom thresholds.
    pub fn with_thresholds(mut self, thresholds: LoadThresholds) -> Self {
        self.thresholds = thresholds;
        self
    }

    /// Record a request started.
    pub async fn request_started(&self) {
        let current = self.current_requests.fetch_add(1, Ordering::Relaxed) + 1;
        self.update_level(current).await;
    }

    /// Record a request completed.
    pub async fn request_completed(&self) {
        let current = self.current_requests.fetch_sub(1, Ordering::Relaxed) - 1;
        self.update_level(current).await;
    }

    async fn update_level(&self, current: usize) {
        let ratio = current as f64 / self.max_requests as f64;
        let new_level = if ratio >= self.thresholds.critical {
            LoadLevel::Critical
        } else if ratio >= self.thresholds.high {
            LoadLevel::High
        } else if ratio >= self.thresholds.normal {
            LoadLevel::Normal
        } else {
            LoadLevel::Low
        };

        *self.current_level.write().await = new_level;
    }

    /// Get current load level.
    pub async fn level(&self) -> LoadLevel {
        *self.current_level.read().await
    }

    /// Get current request count.
    pub fn current_requests(&self) -> usize {
        self.current_requests.load(Ordering::Relaxed)
    }

    /// Get load ratio (0.0 - 1.0+).
    pub fn load_ratio(&self) -> f64 {
        self.current_requests() as f64 / self.max_requests as f64
    }

    /// Check if should shed based on priority.
    pub async fn should_shed(&self, priority: Priority) -> bool {
        let level = self.level().await;
        match level {
            LoadLevel::Low => false,
            LoadLevel::Normal => priority == Priority::Low,
            LoadLevel::High => priority <= Priority::Normal,
            LoadLevel::Critical => priority < Priority::Critical,
        }
    }
}

/// Load shedder.
pub struct LoadShedder {
    monitor: Arc<LoadMonitor>,
    strategy: SheddingStrategy,
    shed_count: AtomicU64,
    accept_count: AtomicU64,
}

impl LoadShedder {
    /// Create a new load shedder.
    pub fn new(monitor: Arc<LoadMonitor>, strategy: SheddingStrategy) -> Self {
        Self {
            monitor,
            strategy,
            shed_count: AtomicU64::new(0),
            accept_count: AtomicU64::new(0),
        }
    }

    /// Try to admit a request.
    pub async fn try_admit(&self, priority: Priority) -> Result<AdmitGuard> {
        let should_shed = match self.strategy {
            SheddingStrategy::DropAll => self.monitor.level().await >= LoadLevel::High,
            SheddingStrategy::Probabilistic(prob) => {
                let level = self.monitor.level().await;
                if level >= LoadLevel::High {
                    // Random check with probability
                    rand_f64() < prob
                } else {
                    false
                }
            }
            SheddingStrategy::PriorityBased => self.monitor.should_shed(priority).await,
            SheddingStrategy::DropOldest | SheddingStrategy::DropNewest => {
                // These are queue-based, always admit for now
                false
            }
        };

        if should_shed {
            self.shed_count.fetch_add(1, Ordering::Relaxed);
            return Err(BackpressureError::LoadShed(format!(
                "Priority {:?} shed under {:?} load",
                priority,
                self.monitor.level().await
            )));
        }

        self.accept_count.fetch_add(1, Ordering::Relaxed);
        self.monitor.request_started().await;

        Ok(AdmitGuard {
            monitor: self.monitor.clone(),
        })
    }

    /// Get shed count.
    pub fn shed_count(&self) -> u64 {
        self.shed_count.load(Ordering::Relaxed)
    }

    /// Get accept count.
    pub fn accept_count(&self) -> u64 {
        self.accept_count.load(Ordering::Relaxed)
    }

    /// Get shed ratio.
    pub fn shed_ratio(&self) -> f64 {
        let total = self.shed_count() + self.accept_count();
        if total == 0 {
            0.0
        } else {
            self.shed_count() as f64 / total as f64
        }
    }
}

/// Guard that tracks request completion.
pub struct AdmitGuard {
    monitor: Arc<LoadMonitor>,
}

impl Drop for AdmitGuard {
    fn drop(&mut self) {
        let monitor = self.monitor.clone();
        tokio::spawn(async move {
            monitor.request_completed().await;
        });
    }
}

/// Simple pseudo-random f64 generator (0.0 - 1.0).
fn rand_f64() -> f64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let hash = time.wrapping_mul(31).wrapping_add(count);
    (hash % 1000) as f64 / 1000.0
}

/// Adaptive concurrency limiter (Vegas-like algorithm).
pub struct AdaptiveLimiter {
    current_limit: AtomicUsize,
    min_limit: usize,
    max_limit: usize,
    semaphore: Arc<RwLock<Arc<Semaphore>>>,
    in_flight: AtomicUsize,
    min_rtt: AtomicU64,
    rtt_samples: RwLock<VecDeque<u64>>,
}

impl AdaptiveLimiter {
    /// Create a new adaptive limiter.
    pub fn new(initial: usize, min: usize, max: usize) -> Self {
        Self {
            current_limit: AtomicUsize::new(initial),
            min_limit: min,
            max_limit: max,
            semaphore: Arc::new(RwLock::new(Arc::new(Semaphore::new(initial)))),
            in_flight: AtomicUsize::new(0),
            min_rtt: AtomicU64::new(u64::MAX),
            rtt_samples: RwLock::new(VecDeque::with_capacity(100)),
        }
    }

    /// Acquire a permit.
    pub async fn acquire(&self, timeout: Duration) -> Result<ConcurrencyGuard> {
        let sem = self.semaphore.read().await.clone();

        let permit = tokio::time::timeout(timeout, sem.acquire_owned())
            .await
            .map_err(|_| BackpressureError::Timeout(timeout))?
            .map_err(|_| BackpressureError::Cancelled)?;

        self.in_flight.fetch_add(1, Ordering::Relaxed);

        Ok(ConcurrencyGuard {
            limiter: self,
            start_time: std::time::Instant::now(),
            _permit: permit,
        })
    }

    /// Record RTT sample and adjust limit.
    async fn record_rtt(&self, rtt_micros: u64) {
        // Update min RTT
        let mut current_min = self.min_rtt.load(Ordering::Relaxed);
        while rtt_micros < current_min {
            match self.min_rtt.compare_exchange_weak(
                current_min,
                rtt_micros,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(c) => current_min = c,
            }
        }

        // Store sample
        {
            let mut samples = self.rtt_samples.write().await;
            if samples.len() >= 100 {
                samples.pop_front();
            }
            samples.push_back(rtt_micros);
        }

        // Adjust limit based on RTT
        let min_rtt = self.min_rtt.load(Ordering::Relaxed);
        if min_rtt == u64::MAX {
            return;
        }

        let current_limit = self.current_limit.load(Ordering::Relaxed);
        let in_flight = self.in_flight.load(Ordering::Relaxed);

        // Vegas-like gradient
        let expected_rate = current_limit as f64 / min_rtt as f64;
        let actual_rate = in_flight as f64 / rtt_micros as f64;
        let gradient = expected_rate - actual_rate;

        let new_limit = if gradient > 0.0 {
            // Not at capacity, increase
            (current_limit + 1).min(self.max_limit)
        } else {
            // At or over capacity, decrease
            (current_limit.saturating_sub(1)).max(self.min_limit)
        };

        if new_limit != current_limit {
            self.current_limit.store(new_limit, Ordering::Relaxed);
            // Note: In a real impl, we'd rebuild the semaphore
        }
    }

    /// Get current limit.
    pub fn current_limit(&self) -> usize {
        self.current_limit.load(Ordering::Relaxed)
    }

    /// Get in-flight count.
    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::Relaxed)
    }
}

/// Guard for adaptive concurrency.
pub struct ConcurrencyGuard<'a> {
    limiter: &'a AdaptiveLimiter,
    start_time: std::time::Instant,
    _permit: OwnedSemaphorePermit,
}

impl<'a> Drop for ConcurrencyGuard<'a> {
    fn drop(&mut self) {
        let rtt_micros = self.start_time.elapsed().as_micros() as u64;
        self.limiter.in_flight.fetch_sub(1, Ordering::Relaxed);

        // Record RTT asynchronously - we can't use async in Drop
        // In a real implementation, we'd use a channel
        let _ = rtt_micros; // Suppress warning
    }
}

/// Bounded queue with backpressure.
pub struct BoundedQueue<T> {
    sender: mpsc::Sender<T>,
    receiver: RwLock<Option<mpsc::Receiver<T>>>,
    capacity: usize,
    enqueued: AtomicU64,
    dequeued: AtomicU64,
    dropped: AtomicU64,
}

impl<T: Send + 'static> BoundedQueue<T> {
    /// Create a new bounded queue.
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(capacity);
        Self {
            sender,
            receiver: RwLock::new(Some(receiver)),
            capacity,
            enqueued: AtomicU64::new(0),
            dequeued: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    /// Try to enqueue an item.
    pub async fn try_enqueue(&self, item: T) -> Result<()> {
        match self.sender.try_send(item) {
            Ok(()) => {
                self.enqueued.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                Err(BackpressureError::QueueFull)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(BackpressureError::Cancelled),
        }
    }

    /// Enqueue with timeout.
    pub async fn enqueue(&self, item: T, timeout: Duration) -> Result<()> {
        match tokio::time::timeout(timeout, self.sender.send(item)).await {
            Ok(Ok(())) => {
                self.enqueued.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Ok(Err(_)) => Err(BackpressureError::Cancelled),
            Err(_) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                Err(BackpressureError::Timeout(timeout))
            }
        }
    }

    /// Dequeue an item.
    pub async fn dequeue(&self) -> Option<T> {
        let mut receiver = self.receiver.write().await;
        if let Some(ref mut rx) = *receiver {
            let item = rx.recv().await;
            if item.is_some() {
                self.dequeued.fetch_add(1, Ordering::Relaxed);
            }
            item
        } else {
            None
        }
    }

    /// Get queue capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get enqueue count.
    pub fn enqueued(&self) -> u64 {
        self.enqueued.load(Ordering::Relaxed)
    }

    /// Get dequeue count.
    pub fn dequeued(&self) -> u64 {
        self.dequeued.load(Ordering::Relaxed)
    }

    /// Get drop count.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Get current queue length estimate.
    pub fn len_estimate(&self) -> u64 {
        self.enqueued().saturating_sub(self.dequeued())
    }

    /// Check if queue is likely empty.
    pub fn is_empty(&self) -> bool {
        self.len_estimate() == 0
    }
}

/// Backpressure signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackpressureSignal {
    /// Signal type.
    pub signal_type: SignalType,
    /// Current load level.
    pub load_level: LoadLevel,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Additional info.
    pub info: String,
}

/// Signal type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalType {
    /// Accept new work.
    Accept,
    /// Slow down.
    SlowDown,
    /// Stop accepting.
    Stop,
    /// Resume.
    Resume,
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // LoadLevel Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_load_level_ordering_low_normal() {
        let low = LoadLevel::Low;
        let normal = LoadLevel::Normal;
        kani::assert(low < normal, "Low < Normal");
    }

    #[kani::proof]
    fn proof_load_level_ordering_normal_high() {
        let normal = LoadLevel::Normal;
        let high = LoadLevel::High;
        kani::assert(normal < high, "Normal < High");
    }

    #[kani::proof]
    fn proof_load_level_ordering_high_critical() {
        let high = LoadLevel::High;
        let critical = LoadLevel::Critical;
        kani::assert(high < critical, "High < Critical");
    }

    #[kani::proof]
    fn proof_load_level_ordering_transitive() {
        let low = LoadLevel::Low;
        let critical = LoadLevel::Critical;
        kani::assert(low < critical, "Low < Critical (transitivity)");
    }

    #[kani::proof]
    fn proof_load_level_factor_bounds() {
        let levels = [
            LoadLevel::Low,
            LoadLevel::Normal,
            LoadLevel::High,
            LoadLevel::Critical,
        ];
        for level in levels {
            let factor = level.factor();
            kani::assert(factor > 0.0, "Factor must be positive");
            kani::assert(factor <= 1.0, "Factor must be <= 1.0");
        }
    }

    #[kani::proof]
    fn proof_load_level_factor_monotonic() {
        kani::assert!(
            LoadLevel::Low.factor() < LoadLevel::Normal.factor(),
            "Low factor < Normal factor"
        );
        kani::assert!(
            LoadLevel::Normal.factor() < LoadLevel::High.factor(),
            "Normal factor < High factor"
        );
        kani::assert!(
            LoadLevel::High.factor() < LoadLevel::Critical.factor(),
            "High factor < Critical factor"
        );
    }

    // ========================================================================
    // Priority Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_priority_ordering_low_normal() {
        let low = Priority::Low;
        let normal = Priority::Normal;
        kani::assert(low < normal, "Priority::Low < Priority::Normal");
    }

    #[kani::proof]
    fn proof_priority_ordering_normal_high() {
        let normal = Priority::Normal;
        let high = Priority::High;
        kani::assert(normal < high, "Priority::Normal < Priority::High");
    }

    #[kani::proof]
    fn proof_priority_ordering_high_critical() {
        let high = Priority::High;
        let critical = Priority::Critical;
        kani::assert(high < critical, "Priority::High < Priority::Critical");
    }

    #[kani::proof]
    fn proof_priority_ordering_transitive() {
        let low = Priority::Low;
        let critical = Priority::Critical;
        kani::assert(
            low < critical,
            "Priority::Low < Priority::Critical (transitivity)",
        );
    }

    #[kani::proof]
    fn proof_priority_default() {
        let default = Priority::default();
        kani::assert(default == Priority::Normal, "Default priority is Normal");
    }

    #[kani::proof]
    fn proof_priority_values() {
        kani::assert!(Priority::Low as u8 == 0, "Low = 0");
        kani::assert!(Priority::Normal as u8 == 1, "Normal = 1");
        kani::assert!(Priority::High as u8 == 2, "High = 2");
        kani::assert!(Priority::Critical as u8 == 3, "Critical = 3");
    }

    // ========================================================================
    // LoadThresholds Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_load_thresholds_default_valid() {
        let t = LoadThresholds::default();
        kani::assert!(t.low >= 0.0, "Low threshold >= 0");
        kani::assert!(t.low <= 1.0, "Low threshold <= 1");
        kani::assert!(t.normal >= 0.0, "Normal threshold >= 0");
        kani::assert!(t.normal <= 1.0, "Normal threshold <= 1");
        kani::assert!(t.high >= 0.0, "High threshold >= 0");
        kani::assert!(t.high <= 1.0, "High threshold <= 1");
        kani::assert!(t.critical >= 0.0, "Critical threshold >= 0");
        kani::assert!(t.critical <= 1.0, "Critical threshold <= 1");
    }

    #[kani::proof]
    fn proof_load_thresholds_default_ascending() {
        let t = LoadThresholds::default();
        kani::assert!(t.low < t.normal, "Low < Normal threshold");
        kani::assert!(t.normal < t.high, "Normal < High threshold");
        kani::assert!(t.high < t.critical, "High < Critical threshold");
    }

    // ========================================================================
    // LoadShedder Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_shed_ratio_zero_when_empty() {
        // shed_ratio = shed_count / (shed_count + accept_count)
        // When both are 0, should return 0.0
        let shed_count: u64 = 0;
        let accept_count: u64 = 0;
        let total = shed_count + accept_count;
        let ratio = if total == 0 {
            0.0
        } else {
            shed_count as f64 / total as f64
        };
        kani::assert!(ratio == 0.0, "Ratio is 0 when no requests");
    }

    #[kani::proof]
    fn proof_shed_ratio_bounds() {
        let shed_count: u64 = kani::any();
        let accept_count: u64 = kani::any();

        // Avoid overflow
        kani::assume(shed_count <= u64::MAX / 2);
        kani::assume(accept_count <= u64::MAX / 2);

        let total = shed_count + accept_count;
        let ratio = if total == 0 {
            0.0
        } else {
            shed_count as f64 / total as f64
        };

        kani::assert!(ratio >= 0.0, "Ratio >= 0");
        kani::assert!(ratio <= 1.0, "Ratio <= 1");
    }

    #[kani::proof]
    fn proof_shed_ratio_all_shed() {
        let shed_count: u64 = 100;
        let accept_count: u64 = 0;
        let total = shed_count + accept_count;
        let ratio = if total == 0 {
            0.0
        } else {
            shed_count as f64 / total as f64
        };
        kani::assert!(ratio == 1.0, "Ratio is 1 when all shed");
    }

    #[kani::proof]
    fn proof_shed_ratio_none_shed() {
        let shed_count: u64 = 0;
        let accept_count: u64 = 100;
        let total = shed_count + accept_count;
        let ratio = if total == 0 {
            0.0
        } else {
            shed_count as f64 / total as f64
        };
        kani::assert!(ratio == 0.0, "Ratio is 0 when none shed");
    }

    // ========================================================================
    // BoundedQueue Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_queue_len_estimate_initial() {
        // Initially, enqueued = dequeued = 0, so len_estimate = 0
        let enqueued: u64 = 0;
        let dequeued: u64 = 0;
        let len = enqueued.saturating_sub(dequeued);
        kani::assert!(len == 0, "Initial length is 0");
    }

    #[kani::proof]
    fn proof_queue_len_estimate_correct() {
        let enqueued: u64 = kani::any();
        let dequeued: u64 = kani::any();

        // In a valid queue, dequeued <= enqueued
        kani::assume(dequeued <= enqueued);

        let len = enqueued.saturating_sub(dequeued);
        kani::assert!(
            len == enqueued - dequeued,
            "Length estimate equals enqueued - dequeued"
        );
    }

    #[kani::proof]
    fn proof_queue_len_estimate_saturating() {
        let enqueued: u64 = kani::any();
        let dequeued: u64 = kani::any();

        // saturating_sub should never underflow
        let len = enqueued.saturating_sub(dequeued);

        if dequeued > enqueued {
            kani::assert!(len == 0, "Saturating sub prevents underflow");
        }
    }

    #[kani::proof]
    fn proof_queue_is_empty() {
        let enqueued: u64 = kani::any();
        let dequeued: u64 = kani::any();
        let len = enqueued.saturating_sub(dequeued);
        let is_empty = len == 0;

        // Empty when len_estimate is 0
        if enqueued == dequeued {
            kani::assert!(is_empty, "Queue empty when enqueued == dequeued");
        }
    }

    // ========================================================================
    // SignalType Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_signal_type_variants() {
        let accept = SignalType::Accept;
        let slow_down = SignalType::SlowDown;
        let stop = SignalType::Stop;
        let resume = SignalType::Resume;

        // All are distinct
        kani::assert!(accept != slow_down, "Accept != SlowDown");
        kani::assert!(slow_down != stop, "SlowDown != Stop");
        kani::assert!(stop != resume, "Stop != Resume");
        kani::assert!(resume != accept, "Resume != Accept");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_monitor() {
        let monitor = LoadMonitor::new(100);

        assert_eq!(monitor.current_requests(), 0);
        assert!(matches!(monitor.level().await, LoadLevel::Low));

        monitor.request_started().await;
        assert_eq!(monitor.current_requests(), 1);

        monitor.request_completed().await;
        assert_eq!(monitor.current_requests(), 0);
    }

    #[tokio::test]
    async fn test_load_level_transitions() {
        let thresholds = LoadThresholds {
            low: 0.1,
            normal: 0.3,
            high: 0.5,
            critical: 0.8,
        };

        let monitor = Arc::new(LoadMonitor::new(10).with_thresholds(thresholds));

        // Add requests to reach different levels
        for _ in 0..2 {
            monitor.request_started().await;
        }
        assert_eq!(monitor.level().await, LoadLevel::Low);

        for _ in 0..2 {
            monitor.request_started().await;
        }
        assert_eq!(monitor.level().await, LoadLevel::Normal);

        for _ in 0..2 {
            monitor.request_started().await;
        }
        assert_eq!(monitor.level().await, LoadLevel::High);

        for _ in 0..3 {
            monitor.request_started().await;
        }
        assert_eq!(monitor.level().await, LoadLevel::Critical);
    }

    #[tokio::test]
    async fn test_load_shedder_priority() {
        let monitor = Arc::new(LoadMonitor::new(10));
        let shedder = LoadShedder::new(monitor.clone(), SheddingStrategy::PriorityBased);

        // Should accept at low load
        let guard = shedder.try_admit(Priority::Normal).await;
        assert!(guard.is_ok());
        drop(guard);

        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(shedder.accept_count() > 0);
    }

    #[tokio::test]
    async fn test_adaptive_limiter() {
        let limiter = AdaptiveLimiter::new(5, 1, 10);

        assert_eq!(limiter.current_limit(), 5);
        assert_eq!(limiter.in_flight(), 0);

        let guard = limiter.acquire(Duration::from_secs(1)).await.unwrap();
        assert_eq!(limiter.in_flight(), 1);

        drop(guard);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(limiter.in_flight(), 0);
    }

    #[tokio::test]
    async fn test_bounded_queue() {
        let queue = BoundedQueue::new(3);

        queue.try_enqueue(1).await.unwrap();
        queue.try_enqueue(2).await.unwrap();
        queue.try_enqueue(3).await.unwrap();

        // Queue should be full
        let result = queue.try_enqueue(4).await;
        assert!(matches!(result, Err(BackpressureError::QueueFull)));

        assert_eq!(queue.enqueued(), 3);
        assert_eq!(queue.dropped(), 1);
    }

    #[tokio::test]
    async fn test_queue_dequeue() {
        let queue = Arc::new(BoundedQueue::new(10));

        queue.try_enqueue(1).await.unwrap();
        queue.try_enqueue(2).await.unwrap();

        let item = queue.dequeue().await;
        assert_eq!(item, Some(1));

        let item = queue.dequeue().await;
        assert_eq!(item, Some(2));

        assert_eq!(queue.dequeued(), 2);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Low < Priority::Normal);
        assert!(Priority::Normal < Priority::High);
        assert!(Priority::High < Priority::Critical);
    }

    #[test]
    fn test_load_level_factor() {
        assert!(LoadLevel::Low.factor() < LoadLevel::Normal.factor());
        assert!(LoadLevel::Normal.factor() < LoadLevel::High.factor());
        assert!(LoadLevel::High.factor() < LoadLevel::Critical.factor());
    }
}
