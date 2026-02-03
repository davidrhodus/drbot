//! Ticker and periodic execution for drbot.
//!
//! This crate provides:
//! - Periodic tickers
//! - Interval runners
//! - Tick counting
//! - Adaptive timing

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::Notify;

/// Ticker error types.
#[derive(Error, Debug)]
pub enum TickerError {
    #[error("Already stopped")]
    AlreadyStopped,

    #[error("Invalid interval")]
    InvalidInterval,
}

/// Result type for ticker operations.
pub type Result<T> = std::result::Result<T, TickerError>;

/// Tick event.
#[derive(Debug, Clone)]
pub struct Tick {
    /// Tick number.
    pub number: u64,
    /// Time of tick.
    pub time: Instant,
    /// Interval since last tick.
    pub interval: Duration,
}

impl Tick {
    /// Create new tick.
    pub fn new(number: u64, time: Instant, interval: Duration) -> Self {
        Self {
            number,
            time,
            interval,
        }
    }
}

/// Simple ticker.
pub struct Ticker {
    interval: Duration,
    tick_count: AtomicU64,
    last_tick: std::sync::Mutex<Instant>,
    running: AtomicBool,
    notify: Arc<Notify>,
}

impl Ticker {
    /// Create new ticker.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            tick_count: AtomicU64::new(0),
            last_tick: std::sync::Mutex::new(Instant::now()),
            running: AtomicBool::new(false),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Create from milliseconds.
    pub fn from_millis(millis: u64) -> Self {
        Self::new(Duration::from_millis(millis))
    }

    /// Create from seconds.
    pub fn from_secs(secs: u64) -> Self {
        Self::new(Duration::from_secs(secs))
    }

    /// Get interval.
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Get tick count.
    pub fn tick_count(&self) -> u64 {
        self.tick_count.load(Ordering::SeqCst)
    }

    /// Check if running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Wait for next tick.
    pub async fn tick(&self) -> Tick {
        tokio::time::sleep(self.interval).await;

        let now = Instant::now();
        let number = self.tick_count.fetch_add(1, Ordering::SeqCst) + 1;

        let mut last = self.last_tick.lock().unwrap();
        let interval = now.duration_since(*last);
        *last = now;

        Tick::new(number, now, interval)
    }

    /// Stop ticker.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Reset tick count.
    pub fn reset(&self) {
        self.tick_count.store(0, Ordering::SeqCst);
    }
}

/// Adaptive ticker that adjusts interval.
pub struct AdaptiveTicker {
    target_interval: Duration,
    min_interval: Duration,
    max_interval: Duration,
    current_interval: std::sync::Mutex<Duration>,
    tick_count: AtomicU64,
}

impl AdaptiveTicker {
    /// Create new adaptive ticker.
    pub fn new(target: Duration, min: Duration, max: Duration) -> Self {
        Self {
            target_interval: target,
            min_interval: min,
            max_interval: max,
            current_interval: std::sync::Mutex::new(target),
            tick_count: AtomicU64::new(0),
        }
    }

    /// Get current interval.
    pub fn current_interval(&self) -> Duration {
        *self.current_interval.lock().unwrap()
    }

    /// Increase interval.
    pub fn increase(&self, factor: f64) {
        let mut interval = self.current_interval.lock().unwrap();
        let new_interval = Duration::from_secs_f64(interval.as_secs_f64() * factor);
        *interval = new_interval.min(self.max_interval);
    }

    /// Decrease interval.
    pub fn decrease(&self, factor: f64) {
        let mut interval = self.current_interval.lock().unwrap();
        let new_interval = Duration::from_secs_f64(interval.as_secs_f64() / factor);
        *interval = new_interval.max(self.min_interval);
    }

    /// Reset to target.
    pub fn reset(&self) {
        *self.current_interval.lock().unwrap() = self.target_interval;
    }

    /// Wait for next tick.
    pub async fn tick(&self) -> Tick {
        let interval = *self.current_interval.lock().unwrap();
        tokio::time::sleep(interval).await;

        let number = self.tick_count.fetch_add(1, Ordering::SeqCst) + 1;
        Tick::new(number, Instant::now(), interval)
    }
}

/// Jittered ticker for avoiding thundering herd.
pub struct JitteredTicker {
    base_interval: Duration,
    jitter_range: Duration,
    tick_count: AtomicU64,
}

impl JitteredTicker {
    /// Create new jittered ticker.
    pub fn new(base_interval: Duration, jitter_range: Duration) -> Self {
        Self {
            base_interval,
            jitter_range,
            tick_count: AtomicU64::new(0),
        }
    }

    /// Wait for next tick.
    pub async fn tick(&self) -> Tick {
        let jitter_ms = if self.jitter_range.as_millis() > 0 {
            // Simple random using time
            let r = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos();
            (r as u64 % self.jitter_range.as_millis() as u64) as i64
                - (self.jitter_range.as_millis() as i64 / 2)
        } else {
            0
        };

        let actual_interval = Duration::from_millis(
            (self.base_interval.as_millis() as i64 + jitter_ms).max(1) as u64,
        );

        tokio::time::sleep(actual_interval).await;

        let number = self.tick_count.fetch_add(1, Ordering::SeqCst) + 1;
        Tick::new(number, Instant::now(), actual_interval)
    }

    /// Get base interval.
    pub fn base_interval(&self) -> Duration {
        self.base_interval
    }
}

/// Counted ticker that stops after N ticks.
pub struct CountedTicker {
    inner: Ticker,
    max_ticks: u64,
}

impl CountedTicker {
    /// Create new counted ticker.
    pub fn new(interval: Duration, max_ticks: u64) -> Self {
        Self {
            inner: Ticker::new(interval),
            max_ticks,
        }
    }

    /// Wait for next tick (None if finished).
    pub async fn tick(&self) -> Option<Tick> {
        if self.inner.tick_count() >= self.max_ticks {
            return None;
        }
        Some(self.inner.tick().await)
    }

    /// Get remaining ticks.
    pub fn remaining(&self) -> u64 {
        self.max_ticks.saturating_sub(self.inner.tick_count())
    }

    /// Check if finished.
    pub fn is_finished(&self) -> bool {
        self.inner.tick_count() >= self.max_ticks
    }
}

/// Ticker handle for controlling a ticker.
pub struct TickerHandle {
    running: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl TickerHandle {
    /// Stop the ticker.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    /// Check if running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Spawn a ticker task.
pub fn spawn_ticker<F, Fut>(interval: Duration, f: F) -> TickerHandle
where
    F: Fn(Tick) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    let running = Arc::new(AtomicBool::new(true));
    let notify = Arc::new(Notify::new());

    let running_clone = running.clone();
    let notify_clone = notify.clone();

    tokio::spawn(async move {
        let ticker = Ticker::new(interval);

        while running_clone.load(Ordering::SeqCst) {
            tokio::select! {
                tick = ticker.tick() => {
                    f(tick).await;
                }
                _ = notify_clone.notified() => {
                    break;
                }
            }
        }
    });

    TickerHandle { running, notify }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ticker() {
        let ticker = Ticker::from_millis(10);

        let tick = ticker.tick().await;
        assert_eq!(tick.number, 1);

        let tick = ticker.tick().await;
        assert_eq!(tick.number, 2);
    }

    #[tokio::test]
    async fn test_counted_ticker() {
        let ticker = CountedTicker::new(Duration::from_millis(10), 3);

        assert_eq!(ticker.remaining(), 3);

        ticker.tick().await;
        assert_eq!(ticker.remaining(), 2);

        ticker.tick().await;
        ticker.tick().await;

        assert!(ticker.is_finished());
        assert!(ticker.tick().await.is_none());
    }

    #[test]
    fn test_adaptive_ticker() {
        let ticker = AdaptiveTicker::new(
            Duration::from_secs(1),
            Duration::from_millis(100),
            Duration::from_secs(10),
        );

        assert_eq!(ticker.current_interval(), Duration::from_secs(1));

        ticker.increase(2.0);
        assert_eq!(ticker.current_interval(), Duration::from_secs(2));

        ticker.decrease(2.0);
        assert_eq!(ticker.current_interval(), Duration::from_secs(1));
    }
}
