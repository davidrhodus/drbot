//! Benchmarking utilities for drbot.
//!
//! This crate provides:
//! - Simple benchmarking
//! - Statistics collection
//! - Comparison support
//! - Result reporting

use std::collections::HashMap;
use std::time::{Duration, Instant};
use thiserror::Error;

/// Bench error types.
#[derive(Error, Debug)]
pub enum BenchError {
    #[error("Benchmark failed: {0}")]
    Failed(String),

    #[error("Benchmark not found: {0}")]
    NotFound(String),
}

/// Result type for bench operations.
pub type Result<T> = std::result::Result<T, BenchError>;

/// Benchmark result.
#[derive(Debug, Clone)]
pub struct BenchResult {
    /// Benchmark name.
    pub name: String,
    /// Number of iterations.
    pub iterations: u64,
    /// Total duration.
    pub total_time: Duration,
    /// Average duration per iteration.
    pub avg_time: Duration,
    /// Minimum duration.
    pub min_time: Duration,
    /// Maximum duration.
    pub max_time: Duration,
    /// Standard deviation.
    pub std_dev: Duration,
    /// Operations per second.
    pub ops_per_sec: f64,
}

impl BenchResult {
    /// Get throughput in ops/sec.
    pub fn throughput(&self) -> f64 {
        self.ops_per_sec
    }

    /// Get average time in nanoseconds.
    pub fn avg_nanos(&self) -> u128 {
        self.avg_time.as_nanos()
    }

    /// Get average time in microseconds.
    pub fn avg_micros(&self) -> u128 {
        self.avg_time.as_micros()
    }

    /// Get average time in milliseconds.
    pub fn avg_millis(&self) -> u128 {
        self.avg_time.as_millis()
    }
}

/// Simple timer.
pub struct Timer {
    start: Instant,
    elapsed: Option<Duration>,
}

impl Timer {
    /// Start new timer.
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
            elapsed: None,
        }
    }

    /// Stop timer.
    pub fn stop(&mut self) -> Duration {
        let elapsed = self.start.elapsed();
        self.elapsed = Some(elapsed);
        elapsed
    }

    /// Get elapsed time (without stopping).
    pub fn elapsed(&self) -> Duration {
        self.elapsed.unwrap_or_else(|| self.start.elapsed())
    }

    /// Check if stopped.
    pub fn is_stopped(&self) -> bool {
        self.elapsed.is_some()
    }
}

/// Benchmark configuration.
#[derive(Debug, Clone)]
pub struct BenchConfig {
    /// Warm-up iterations.
    pub warmup: u64,
    /// Measurement iterations.
    pub iterations: u64,
    /// Minimum duration for benchmarking.
    pub min_duration: Duration,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            warmup: 10,
            iterations: 100,
            min_duration: Duration::from_millis(100),
        }
    }
}

impl BenchConfig {
    /// Create new config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set warmup iterations.
    pub fn warmup(mut self, n: u64) -> Self {
        self.warmup = n;
        self
    }

    /// Set iterations.
    pub fn iterations(mut self, n: u64) -> Self {
        self.iterations = n;
        self
    }

    /// Set minimum duration.
    pub fn min_duration(mut self, d: Duration) -> Self {
        self.min_duration = d;
        self
    }
}

/// Benchmark runner.
pub struct Bench {
    name: String,
    config: BenchConfig,
}

impl Bench {
    /// Create new benchmark.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            config: BenchConfig::default(),
        }
    }

    /// Set config.
    pub fn config(mut self, config: BenchConfig) -> Self {
        self.config = config;
        self
    }

    /// Run benchmark.
    pub fn run<F>(self, mut f: F) -> BenchResult
    where
        F: FnMut(),
    {
        // Warmup
        for _ in 0..self.config.warmup {
            f();
        }

        // Measure
        let mut times = Vec::with_capacity(self.config.iterations as usize);
        let start = Instant::now();

        for _ in 0..self.config.iterations {
            let iter_start = Instant::now();
            f();
            times.push(iter_start.elapsed());
        }

        let total_time = start.elapsed();

        // Calculate stats
        let min_time = *times.iter().min().unwrap_or(&Duration::ZERO);
        let max_time = *times.iter().max().unwrap_or(&Duration::ZERO);

        let sum_nanos: u128 = times.iter().map(|d| d.as_nanos()).sum();
        let avg_nanos = sum_nanos / times.len() as u128;
        let avg_time = Duration::from_nanos(avg_nanos as u64);

        // Standard deviation
        let variance: f64 = times
            .iter()
            .map(|d| {
                let diff = d.as_nanos() as f64 - avg_nanos as f64;
                diff * diff
            })
            .sum::<f64>()
            / times.len() as f64;
        let std_dev = Duration::from_nanos(variance.sqrt() as u64);

        let ops_per_sec = if avg_nanos > 0 {
            1_000_000_000.0 / avg_nanos as f64
        } else {
            f64::INFINITY
        };

        BenchResult {
            name: self.name,
            iterations: self.config.iterations,
            total_time,
            avg_time,
            min_time,
            max_time,
            std_dev,
            ops_per_sec,
        }
    }

    /// Run benchmark with setup.
    pub fn run_with_setup<S, T, F>(self, mut setup: S, mut f: F) -> BenchResult
    where
        S: FnMut() -> T,
        F: FnMut(T),
    {
        // Warmup
        for _ in 0..self.config.warmup {
            let data = setup();
            f(data);
        }

        // Measure
        let mut times = Vec::with_capacity(self.config.iterations as usize);
        let start = Instant::now();

        for _ in 0..self.config.iterations {
            let data = setup();
            let iter_start = Instant::now();
            f(data);
            times.push(iter_start.elapsed());
        }

        let total_time = start.elapsed();

        // Calculate stats (same as above)
        let min_time = *times.iter().min().unwrap_or(&Duration::ZERO);
        let max_time = *times.iter().max().unwrap_or(&Duration::ZERO);

        let sum_nanos: u128 = times.iter().map(|d| d.as_nanos()).sum();
        let avg_nanos = sum_nanos / times.len() as u128;
        let avg_time = Duration::from_nanos(avg_nanos as u64);

        let variance: f64 = times
            .iter()
            .map(|d| {
                let diff = d.as_nanos() as f64 - avg_nanos as f64;
                diff * diff
            })
            .sum::<f64>()
            / times.len() as f64;
        let std_dev = Duration::from_nanos(variance.sqrt() as u64);

        let ops_per_sec = if avg_nanos > 0 {
            1_000_000_000.0 / avg_nanos as f64
        } else {
            f64::INFINITY
        };

        BenchResult {
            name: self.name,
            iterations: self.config.iterations,
            total_time,
            avg_time,
            min_time,
            max_time,
            std_dev,
            ops_per_sec,
        }
    }
}

/// Benchmark suite.
pub struct BenchSuite {
    name: String,
    results: Vec<BenchResult>,
    config: BenchConfig,
}

impl BenchSuite {
    /// Create new suite.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            results: Vec::new(),
            config: BenchConfig::default(),
        }
    }

    /// Set config.
    pub fn config(mut self, config: BenchConfig) -> Self {
        self.config = config;
        self
    }

    /// Add benchmark.
    pub fn bench<F>(&mut self, name: impl Into<String>, f: F)
    where
        F: FnMut(),
    {
        let result = Bench::new(name).config(self.config.clone()).run(f);
        self.results.push(result);
    }

    /// Get results.
    pub fn results(&self) -> &[BenchResult] {
        &self.results
    }

    /// Get suite name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Generate report.
    pub fn report(&self) -> String {
        let mut report = format!("Benchmark Suite: {}\n", self.name);
        report.push_str(&format!("{:-<60}\n", ""));

        for result in &self.results {
            report.push_str(&format!(
                "{:<30} {:>10.3} ns/op ({:.2} ops/sec)\n",
                result.name,
                result.avg_time.as_nanos() as f64,
                result.ops_per_sec
            ));
        }

        report
    }
}

/// Comparison between benchmark results.
#[derive(Debug, Clone)]
pub struct Comparison {
    /// Baseline result.
    pub baseline: BenchResult,
    /// Target result.
    pub target: BenchResult,
    /// Speedup ratio (>1 means target is faster).
    pub speedup: f64,
    /// Difference in ns.
    pub diff_nanos: i128,
}

impl Comparison {
    /// Create comparison.
    pub fn new(baseline: BenchResult, target: BenchResult) -> Self {
        let baseline_nanos = baseline.avg_time.as_nanos();
        let target_nanos = target.avg_time.as_nanos();

        let speedup = if target_nanos > 0 {
            baseline_nanos as f64 / target_nanos as f64
        } else {
            f64::INFINITY
        };

        let diff_nanos = baseline_nanos as i128 - target_nanos as i128;

        Self {
            baseline,
            target,
            speedup,
            diff_nanos,
        }
    }

    /// Check if target is faster.
    pub fn is_faster(&self) -> bool {
        self.speedup > 1.0
    }

    /// Check if target is slower.
    pub fn is_slower(&self) -> bool {
        self.speedup < 1.0
    }

    /// Get percentage improvement.
    pub fn improvement_pct(&self) -> f64 {
        (self.speedup - 1.0) * 100.0
    }
}

/// Benchmark history for tracking performance over time.
pub struct BenchHistory {
    history: HashMap<String, Vec<BenchResult>>,
}

impl BenchHistory {
    /// Create new history.
    pub fn new() -> Self {
        Self {
            history: HashMap::new(),
        }
    }

    /// Record result.
    pub fn record(&mut self, result: BenchResult) {
        self.history
            .entry(result.name.clone())
            .or_default()
            .push(result);
    }

    /// Get history for benchmark.
    pub fn get(&self, name: &str) -> Option<&[BenchResult]> {
        self.history.get(name).map(|v| v.as_slice())
    }

    /// Get latest result.
    pub fn latest(&self, name: &str) -> Option<&BenchResult> {
        self.history.get(name).and_then(|v| v.last())
    }

    /// Compare latest with previous.
    pub fn compare_with_previous(&self, name: &str) -> Option<Comparison> {
        let history = self.history.get(name)?;
        if history.len() < 2 {
            return None;
        }
        let prev = history[history.len() - 2].clone();
        let latest = history.last()?.clone();
        Some(Comparison::new(prev, latest))
    }
}

impl Default for BenchHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// Measure execution time.
pub fn measure<F, R>(f: F) -> (R, Duration)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    (result, start.elapsed())
}

/// Measure average execution time.
pub fn measure_avg<F, R>(iterations: u64, mut f: F) -> Duration
where
    F: FnMut() -> R,
{
    let start = Instant::now();
    for _ in 0..iterations {
        std::hint::black_box(f());
    }
    start.elapsed() / iterations as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer() {
        let mut timer = Timer::start();
        std::thread::sleep(Duration::from_millis(10));
        let elapsed = timer.stop();

        assert!(elapsed >= Duration::from_millis(10));
        assert!(timer.is_stopped());
    }

    #[test]
    fn test_bench() {
        let result = Bench::new("test")
            .config(BenchConfig::new().iterations(10).warmup(2))
            .run(|| {
                let _ = (0..100).sum::<i32>();
            });

        assert_eq!(result.name, "test");
        assert_eq!(result.iterations, 10);
    }

    #[test]
    fn test_comparison() {
        let baseline = BenchResult {
            name: "base".to_string(),
            iterations: 100,
            total_time: Duration::from_millis(100),
            avg_time: Duration::from_micros(1000),
            min_time: Duration::from_micros(900),
            max_time: Duration::from_micros(1100),
            std_dev: Duration::from_micros(50),
            ops_per_sec: 1000.0,
        };

        let target = BenchResult {
            name: "target".to_string(),
            iterations: 100,
            total_time: Duration::from_millis(50),
            avg_time: Duration::from_micros(500),
            min_time: Duration::from_micros(450),
            max_time: Duration::from_micros(550),
            std_dev: Duration::from_micros(25),
            ops_per_sec: 2000.0,
        };

        let comparison = Comparison::new(baseline, target);
        assert!(comparison.is_faster());
        assert!((comparison.speedup - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_measure() {
        let (result, duration) = measure(|| {
            std::thread::sleep(Duration::from_millis(5));
            42
        });

        assert_eq!(result, 42);
        assert!(duration >= Duration::from_millis(5));
    }
}
