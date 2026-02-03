//! Performance profiling utilities for drbot.
//!
//! This crate provides:
//! - Timer measurements
//! - Memory tracking
//! - CPU profiling helpers
//! - Flamegraph support

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;

/// Profiler error types.
#[derive(Error, Debug)]
pub enum ProfilerError {
    #[error("Timer not found: {0}")]
    TimerNotFound(String),

    #[error("Profile not started")]
    NotStarted,

    #[error("Profile already running")]
    AlreadyRunning,
}

/// Result type for profiler operations.
pub type Result<T> = std::result::Result<T, ProfilerError>;

/// Simple timer measurement.
#[derive(Debug)]
pub struct Timer {
    start: Instant,
    name: String,
}

impl Timer {
    /// Start a new timer.
    pub fn start(name: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            name: name.into(),
        }
    }

    /// Get elapsed time.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Stop and return elapsed time.
    pub fn stop(self) -> TimerResult {
        TimerResult {
            name: self.name,
            duration: self.start.elapsed(),
            timestamp: Utc::now(),
        }
    }

    /// Get timer name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Timer result after stopping.
#[derive(Debug, Clone)]
pub struct TimerResult {
    /// Timer name.
    pub name: String,
    /// Duration.
    pub duration: Duration,
    /// Timestamp when stopped.
    pub timestamp: DateTime<Utc>,
}

impl TimerResult {
    /// Get duration in milliseconds.
    pub fn millis(&self) -> f64 {
        self.duration.as_secs_f64() * 1000.0
    }

    /// Get duration in microseconds.
    pub fn micros(&self) -> f64 {
        self.duration.as_secs_f64() * 1_000_000.0
    }

    /// Get duration in nanoseconds.
    pub fn nanos(&self) -> u128 {
        self.duration.as_nanos()
    }
}

/// Scoped timer that reports when dropped.
pub struct ScopedTimer {
    timer: Option<Timer>,
    callback: Option<Box<dyn FnOnce(TimerResult) + Send>>,
}

impl ScopedTimer {
    /// Create new scoped timer.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            timer: Some(Timer::start(name)),
            callback: None,
        }
    }

    /// Set callback on completion.
    pub fn on_complete<F>(mut self, f: F) -> Self
    where
        F: FnOnce(TimerResult) + Send + 'static,
    {
        self.callback = Some(Box::new(f));
        self
    }

    /// Cancel the timer without reporting.
    pub fn cancel(mut self) {
        self.timer = None;
    }
}

impl Drop for ScopedTimer {
    fn drop(&mut self) {
        if let Some(timer) = self.timer.take() {
            let result = timer.stop();
            if let Some(callback) = self.callback.take() {
                callback(result);
            }
        }
    }
}

/// Statistics for a profiled operation.
#[derive(Debug, Clone, Default)]
pub struct ProfileStats {
    /// Number of measurements.
    pub count: u64,
    /// Total duration.
    pub total: Duration,
    /// Minimum duration.
    pub min: Option<Duration>,
    /// Maximum duration.
    pub max: Option<Duration>,
    /// Mean duration.
    pub mean: Option<Duration>,
}

impl ProfileStats {
    /// Add a measurement.
    pub fn record(&mut self, duration: Duration) {
        self.count += 1;
        self.total += duration;

        self.min = Some(self.min.map_or(duration, |m| m.min(duration)));
        self.max = Some(self.max.map_or(duration, |m| m.max(duration)));
        self.mean = Some(self.total / self.count as u32);
    }

    /// Get mean in milliseconds.
    pub fn mean_millis(&self) -> Option<f64> {
        self.mean.map(|d| d.as_secs_f64() * 1000.0)
    }

    /// Get total in milliseconds.
    pub fn total_millis(&self) -> f64 {
        self.total.as_secs_f64() * 1000.0
    }
}

/// Profiler for collecting measurements.
#[derive(Debug, Clone)]
pub struct Profiler {
    name: String,
    stats: Arc<Mutex<HashMap<String, ProfileStats>>>,
    enabled: bool,
}

impl Profiler {
    /// Create new profiler.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            stats: Arc::new(Mutex::new(HashMap::new())),
            enabled: true,
        }
    }

    /// Enable/disable profiler.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Start timing an operation.
    pub fn time(&self, operation: impl Into<String>) -> ProfilerTimer {
        ProfilerTimer {
            operation: operation.into(),
            start: Instant::now(),
            profiler: self.clone(),
        }
    }

    /// Record a measurement.
    pub fn record(&self, operation: impl Into<String>, duration: Duration) {
        if !self.enabled {
            return;
        }

        let mut stats = self.stats.lock().unwrap();
        stats.entry(operation.into()).or_default().record(duration);
    }

    /// Get stats for an operation.
    pub fn get_stats(&self, operation: &str) -> Option<ProfileStats> {
        self.stats.lock().unwrap().get(operation).cloned()
    }

    /// Get all stats.
    pub fn all_stats(&self) -> HashMap<String, ProfileStats> {
        self.stats.lock().unwrap().clone()
    }

    /// Reset all stats.
    pub fn reset(&self) {
        self.stats.lock().unwrap().clear();
    }

    /// Get profiler name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Generate report.
    pub fn report(&self) -> ProfileReport {
        ProfileReport {
            name: self.name.clone(),
            stats: self.all_stats(),
            timestamp: Utc::now(),
        }
    }
}

/// Timer bound to a profiler.
pub struct ProfilerTimer {
    operation: String,
    start: Instant,
    profiler: Profiler,
}

impl ProfilerTimer {
    /// Stop and record.
    pub fn stop(self) -> Duration {
        let duration = self.start.elapsed();
        self.profiler.record(&self.operation, duration);
        duration
    }

    /// Get elapsed without stopping.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

impl Drop for ProfilerTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        self.profiler.record(&self.operation, duration);
    }
}

/// Profile report.
#[derive(Debug, Clone)]
pub struct ProfileReport {
    /// Profiler name.
    pub name: String,
    /// Statistics by operation.
    pub stats: HashMap<String, ProfileStats>,
    /// Report timestamp.
    pub timestamp: DateTime<Utc>,
}

impl ProfileReport {
    /// Format as text.
    pub fn format(&self) -> String {
        let mut lines = vec![format!("Profile Report: {}", self.name)];
        lines.push("=".repeat(50));

        let mut entries: Vec<_> = self.stats.iter().collect();
        entries.sort_by(|a, b| b.1.total.cmp(&a.1.total));

        for (op, stats) in entries {
            lines.push(format!(
                "{}: count={}, total={:.2}ms, mean={:.3}ms",
                op,
                stats.count,
                stats.total_millis(),
                stats.mean_millis().unwrap_or(0.0)
            ));
        }

        lines.join("\n")
    }
}

/// Counter for tracking operations.
#[derive(Debug)]
pub struct Counter {
    name: String,
    value: AtomicU64,
}

impl Counter {
    /// Create new counter.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: AtomicU64::new(0),
        }
    }

    /// Increment by 1.
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment by n.
    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    /// Get value.
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Reset counter.
    pub fn reset(&self) {
        self.value.store(0, Ordering::Relaxed);
    }

    /// Get name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Gauge for tracking values.
#[derive(Debug)]
pub struct Gauge {
    name: String,
    value: AtomicU64,
}

impl Gauge {
    /// Create new gauge.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: AtomicU64::new(0),
        }
    }

    /// Set value.
    pub fn set(&self, value: u64) {
        self.value.store(value, Ordering::Relaxed);
    }

    /// Get value.
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Increment.
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement.
    pub fn dec(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Time a block of code.
#[macro_export]
macro_rules! time {
    ($name:expr, $block:block) => {{
        let _timer = $crate::Timer::start($name);
        let result = $block;
        let elapsed = _timer.stop();
        eprintln!("{}: {:.3}ms", $name, elapsed.millis());
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_timer() {
        let timer = Timer::start("test");
        thread::sleep(Duration::from_millis(10));
        let result = timer.stop();

        assert!(result.duration >= Duration::from_millis(10));
        assert_eq!(result.name, "test");
    }

    #[test]
    fn test_profile_stats() {
        let mut stats = ProfileStats::default();
        stats.record(Duration::from_millis(10));
        stats.record(Duration::from_millis(20));
        stats.record(Duration::from_millis(30));

        assert_eq!(stats.count, 3);
        assert_eq!(stats.min, Some(Duration::from_millis(10)));
        assert_eq!(stats.max, Some(Duration::from_millis(30)));
    }

    #[test]
    fn test_profiler() {
        let profiler = Profiler::new("test");

        {
            let _timer = profiler.time("operation");
            thread::sleep(Duration::from_millis(5));
        }

        let stats = profiler.get_stats("operation").unwrap();
        assert_eq!(stats.count, 1);
        assert!(stats.total >= Duration::from_millis(5));
    }

    #[test]
    fn test_counter() {
        let counter = Counter::new("requests");
        counter.inc();
        counter.inc();
        counter.add(5);

        assert_eq!(counter.get(), 7);
    }

    #[test]
    fn test_gauge() {
        let gauge = Gauge::new("connections");
        gauge.set(10);
        gauge.inc();
        gauge.dec();

        assert_eq!(gauge.get(), 10);
    }
}
