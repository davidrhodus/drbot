//! Metering and metrics utilities for drbot.
//!
//! This crate provides:
//! - Counter metrics
//! - Gauge metrics
//! - Histogram metrics
//! - Timer utilities

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::RwLock;
use thiserror::Error;

/// Meter error types.
#[derive(Error, Debug, Clone)]
pub enum MeterError {
    #[error("Metric not found: {0}")]
    NotFound(String),

    #[error("Invalid metric type")]
    InvalidType,
}

/// Result type for meter operations.
pub type Result<T> = std::result::Result<T, MeterError>;

/// Counter metric that only increases.
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    /// Create new counter.
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    /// Increment by 1.
    pub fn inc(&self) {
        self.add(1);
    }

    /// Add value.
    pub fn add(&self, value: u64) {
        self.value.fetch_add(value, Ordering::Relaxed);
    }

    /// Get current value.
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Reset to zero.
    pub fn reset(&self) {
        self.value.store(0, Ordering::Relaxed);
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

/// Gauge metric that can go up or down.
pub struct Gauge {
    value: AtomicI64,
}

impl Gauge {
    /// Create new gauge.
    pub fn new() -> Self {
        Self {
            value: AtomicI64::new(0),
        }
    }

    /// Create with initial value.
    pub fn with_value(value: i64) -> Self {
        Self {
            value: AtomicI64::new(value),
        }
    }

    /// Set value.
    pub fn set(&self, value: i64) {
        self.value.store(value, Ordering::Relaxed);
    }

    /// Increment by 1.
    pub fn inc(&self) {
        self.add(1);
    }

    /// Decrement by 1.
    pub fn dec(&self) {
        self.sub(1);
    }

    /// Add value.
    pub fn add(&self, value: i64) {
        self.value.fetch_add(value, Ordering::Relaxed);
    }

    /// Subtract value.
    pub fn sub(&self, value: i64) {
        self.value.fetch_sub(value, Ordering::Relaxed);
    }

    /// Get current value.
    pub fn get(&self) -> i64 {
        self.value.load(Ordering::Relaxed)
    }
}

impl Default for Gauge {
    fn default() -> Self {
        Self::new()
    }
}

/// Histogram metric for value distribution.
pub struct Histogram {
    values: RwLock<Vec<f64>>,
    sum: AtomicU64,
    count: AtomicU64,
}

impl Histogram {
    /// Create new histogram.
    pub fn new() -> Self {
        Self {
            values: RwLock::new(Vec::new()),
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    /// Observe a value.
    pub fn observe(&self, value: f64) {
        self.values.write().unwrap().push(value);
        self.sum.fetch_add(value.to_bits(), Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get count of observations.
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Get sum of observations.
    pub fn sum(&self) -> f64 {
        f64::from_bits(self.sum.load(Ordering::Relaxed))
    }

    /// Get mean.
    pub fn mean(&self) -> f64 {
        let count = self.count();
        if count == 0 {
            0.0
        } else {
            self.sum() / count as f64
        }
    }

    /// Get percentile (0-100).
    pub fn percentile(&self, p: f64) -> Option<f64> {
        let values = self.values.read().unwrap();
        if values.is_empty() {
            return None;
        }

        let mut sorted: Vec<f64> = values.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let idx = ((p / 100.0) * (sorted.len() - 1) as f64) as usize;
        Some(sorted[idx])
    }

    /// Get min value.
    pub fn min(&self) -> Option<f64> {
        self.values
            .read()
            .unwrap()
            .iter()
            .copied()
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Get max value.
    pub fn max(&self) -> Option<f64> {
        self.values
            .read()
            .unwrap()
            .iter()
            .copied()
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Reset histogram.
    pub fn reset(&self) {
        self.values.write().unwrap().clear();
        self.sum.store(0, Ordering::Relaxed);
        self.count.store(0, Ordering::Relaxed);
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer for measuring durations.
pub struct Timer {
    start: std::time::Instant,
}

impl Timer {
    /// Start new timer.
    pub fn start() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }

    /// Get elapsed duration.
    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }

    /// Get elapsed milliseconds.
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed().as_millis() as u64
    }

    /// Get elapsed microseconds.
    pub fn elapsed_us(&self) -> u64 {
        self.elapsed().as_micros() as u64
    }

    /// Stop and return elapsed milliseconds.
    pub fn stop(self) -> u64 {
        self.elapsed_ms()
    }

    /// Stop and observe in histogram.
    pub fn observe_in(self, histogram: &Histogram) {
        histogram.observe(self.elapsed_ms() as f64);
    }
}

/// Rate meter for tracking rate of events.
pub struct RateMeter {
    count: AtomicU64,
    start_time: std::time::Instant,
}

impl RateMeter {
    /// Create new rate meter.
    pub fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
            start_time: std::time::Instant::now(),
        }
    }

    /// Record an event.
    pub fn mark(&self) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record multiple events.
    pub fn mark_n(&self, n: u64) {
        self.count.fetch_add(n, Ordering::Relaxed);
    }

    /// Get current rate (events per second).
    pub fn rate(&self) -> f64 {
        let count = self.count.load(Ordering::Relaxed) as f64;
        let secs = self.start_time.elapsed().as_secs_f64();
        if secs > 0.0 {
            count / secs
        } else {
            0.0
        }
    }

    /// Get count.
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

impl Default for RateMeter {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics registry.
pub struct MetricsRegistry {
    counters: RwLock<HashMap<String, Counter>>,
    gauges: RwLock<HashMap<String, Gauge>>,
    histograms: RwLock<HashMap<String, Histogram>>,
}

impl MetricsRegistry {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            counters: RwLock::new(HashMap::new()),
            gauges: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
        }
    }

    /// Get or create counter.
    pub fn counter(&self, name: &str) -> &Counter {
        let mut counters = self.counters.write().unwrap();
        if !counters.contains_key(name) {
            counters.insert(name.to_string(), Counter::new());
        }
        // Safety: we know the key exists and registry outlives reference
        unsafe { &*(counters.get(name).unwrap() as *const Counter) }
    }

    /// Get or create gauge.
    pub fn gauge(&self, name: &str) -> &Gauge {
        let mut gauges = self.gauges.write().unwrap();
        if !gauges.contains_key(name) {
            gauges.insert(name.to_string(), Gauge::new());
        }
        unsafe { &*(gauges.get(name).unwrap() as *const Gauge) }
    }

    /// Get or create histogram.
    pub fn histogram(&self, name: &str) -> &Histogram {
        let mut histograms = self.histograms.write().unwrap();
        if !histograms.contains_key(name) {
            histograms.insert(name.to_string(), Histogram::new());
        }
        unsafe { &*(histograms.get(name).unwrap() as *const Histogram) }
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let counter = Counter::new();
        assert_eq!(counter.get(), 0);

        counter.inc();
        assert_eq!(counter.get(), 1);

        counter.add(5);
        assert_eq!(counter.get(), 6);

        counter.reset();
        assert_eq!(counter.get(), 0);
    }

    #[test]
    fn test_gauge() {
        let gauge = Gauge::new();
        assert_eq!(gauge.get(), 0);

        gauge.set(10);
        assert_eq!(gauge.get(), 10);

        gauge.inc();
        assert_eq!(gauge.get(), 11);

        gauge.dec();
        assert_eq!(gauge.get(), 10);

        gauge.sub(5);
        assert_eq!(gauge.get(), 5);
    }

    #[test]
    fn test_histogram() {
        let hist = Histogram::new();

        hist.observe(1.0);
        hist.observe(2.0);
        hist.observe(3.0);
        hist.observe(4.0);
        hist.observe(5.0);

        assert_eq!(hist.count(), 5);
        assert_eq!(hist.mean(), 3.0);
        assert_eq!(hist.min(), Some(1.0));
        assert_eq!(hist.max(), Some(5.0));
        assert_eq!(hist.percentile(50.0), Some(3.0));
    }

    #[test]
    fn test_timer() {
        let timer = Timer::start();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = timer.stop();
        assert!(elapsed >= 10);
    }

    #[test]
    fn test_rate_meter() {
        let meter = RateMeter::new();
        meter.mark();
        meter.mark();
        meter.mark();

        assert_eq!(meter.count(), 3);
        assert!(meter.rate() > 0.0);
    }
}
