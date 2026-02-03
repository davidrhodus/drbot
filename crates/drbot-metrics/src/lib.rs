//! Metrics collection and reporting for drbot.
//!
//! This crate provides a comprehensive metrics system for tracking application
//! performance, resource usage, and business metrics.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Metrics error types.
#[derive(Error, Debug)]
pub enum MetricsError {
    #[error("Metric not found: {0}")]
    NotFound(String),

    #[error("Invalid metric type: expected {expected}, got {actual}")]
    TypeMismatch { expected: String, actual: String },

    #[error("Export failed: {0}")]
    ExportFailed(String),

    #[error("Invalid label: {0}")]
    InvalidLabel(String),
}

/// Result type for metrics operations.
pub type Result<T> = std::result::Result<T, MetricsError>;

/// Metric type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricType {
    /// A counter that only goes up.
    Counter,
    /// A gauge that can go up or down.
    Gauge,
    /// A histogram for measuring distributions.
    Histogram,
    /// A summary with quantiles.
    Summary,
}

/// Labels for metrics.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Labels(HashMap<String, String>);

impl Labels {
    /// Create new empty labels.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Add a label.
    pub fn add(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.0.insert(key.into(), value.into());
        self
    }

    /// Get a label value.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }

    /// Check if labels are empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get all labels.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }
}

/// A counter metric that only increases.
#[derive(Debug)]
pub struct Counter {
    name: String,
    help: String,
    value: AtomicU64,
    labels: Labels,
}

impl Counter {
    /// Create a new counter.
    pub fn new(name: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            help: help.into(),
            value: AtomicU64::new(0),
            labels: Labels::new(),
        }
    }

    /// Create a counter with labels.
    pub fn with_labels(mut self, labels: Labels) -> Self {
        self.labels = labels;
        self
    }

    /// Increment the counter by 1.
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// Add a value to the counter.
    pub fn add(&self, value: u64) {
        self.value.fetch_add(value, Ordering::Relaxed);
    }

    /// Get the current value.
    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Get the counter name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the help text.
    pub fn help(&self) -> &str {
        &self.help
    }
}

/// A gauge metric that can go up or down.
#[derive(Debug)]
pub struct Gauge {
    name: String,
    help: String,
    value: AtomicI64,
    labels: Labels,
}

impl Gauge {
    /// Create a new gauge.
    pub fn new(name: impl Into<String>, help: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            help: help.into(),
            value: AtomicI64::new(0),
            labels: Labels::new(),
        }
    }

    /// Create a gauge with labels.
    pub fn with_labels(mut self, labels: Labels) -> Self {
        self.labels = labels;
        self
    }

    /// Set the gauge value.
    pub fn set(&self, value: i64) {
        self.value.store(value, Ordering::Relaxed);
    }

    /// Increment the gauge by 1.
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement the gauge by 1.
    pub fn dec(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    /// Add a value to the gauge.
    pub fn add(&self, value: i64) {
        self.value.fetch_add(value, Ordering::Relaxed);
    }

    /// Subtract a value from the gauge.
    pub fn sub(&self, value: i64) {
        self.value.fetch_sub(value, Ordering::Relaxed);
    }

    /// Get the current value.
    pub fn get(&self) -> i64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Get the gauge name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Histogram bucket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bucket {
    /// Upper bound of the bucket.
    pub upper_bound: f64,
    /// Count of observations in this bucket.
    pub count: u64,
}

/// A histogram for measuring distributions.
#[derive(Debug)]
pub struct Histogram {
    name: String,
    help: String,
    buckets: Vec<f64>,
    counts: Vec<AtomicU64>,
    sum: RwLock<f64>,
    count: AtomicU64,
    labels: Labels,
}

impl Histogram {
    /// Create a new histogram with default buckets.
    pub fn new(name: impl Into<String>, help: impl Into<String>) -> Self {
        Self::with_buckets(
            name,
            help,
            vec![
                0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ],
        )
    }

    /// Create a histogram with custom buckets.
    pub fn with_buckets(
        name: impl Into<String>,
        help: impl Into<String>,
        buckets: Vec<f64>,
    ) -> Self {
        let counts = buckets.iter().map(|_| AtomicU64::new(0)).collect();
        Self {
            name: name.into(),
            help: help.into(),
            buckets,
            counts,
            sum: RwLock::new(0.0),
            count: AtomicU64::new(0),
            labels: Labels::new(),
        }
    }

    /// Create linear buckets.
    pub fn linear_buckets(start: f64, width: f64, count: usize) -> Vec<f64> {
        (0..count).map(|i| start + width * i as f64).collect()
    }

    /// Create exponential buckets.
    pub fn exponential_buckets(start: f64, factor: f64, count: usize) -> Vec<f64> {
        (0..count).map(|i| start * factor.powi(i as i32)).collect()
    }

    /// Observe a value.
    pub async fn observe(&self, value: f64) {
        // Update bucket counts
        for (i, bound) in self.buckets.iter().enumerate() {
            if value <= *bound {
                self.counts[i].fetch_add(1, Ordering::Relaxed);
            }
        }

        // Update sum and count
        {
            let mut sum = self.sum.write().await;
            *sum += value;
        }
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the observation count.
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Get the histogram name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get bucket information.
    pub fn buckets(&self) -> Vec<Bucket> {
        self.buckets
            .iter()
            .zip(self.counts.iter())
            .map(|(bound, count)| Bucket {
                upper_bound: *bound,
                count: count.load(Ordering::Relaxed),
            })
            .collect()
    }
}

/// A metric value snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    /// Metric name.
    pub name: String,
    /// Metric type.
    pub metric_type: MetricType,
    /// Help text.
    pub help: String,
    /// Labels.
    pub labels: Labels,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// The value (interpretation depends on type).
    pub value: MetricData,
}

/// Metric data based on type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricData {
    /// Counter value.
    Counter(u64),
    /// Gauge value.
    Gauge(i64),
    /// Histogram data.
    Histogram {
        buckets: Vec<Bucket>,
        sum: f64,
        count: u64,
    },
    /// Summary data.
    Summary {
        quantiles: Vec<(f64, f64)>,
        sum: f64,
        count: u64,
    },
}

/// Trait for metrics exporters.
#[async_trait]
pub trait MetricsExporter: Send + Sync {
    /// Export metrics.
    async fn export(&self, metrics: &[MetricValue]) -> Result<()>;

    /// Get exporter name.
    fn name(&self) -> &str;
}

/// Prometheus text format exporter.
pub struct PrometheusExporter;

impl PrometheusExporter {
    /// Create a new Prometheus exporter.
    pub fn new() -> Self {
        Self
    }

    /// Format metrics in Prometheus text format.
    pub fn format(metrics: &[MetricValue]) -> String {
        let mut output = String::new();

        for metric in metrics {
            // Add HELP line
            output.push_str(&format!("# HELP {} {}\n", metric.name, metric.help));

            // Add TYPE line
            let type_str = match metric.metric_type {
                MetricType::Counter => "counter",
                MetricType::Gauge => "gauge",
                MetricType::Histogram => "histogram",
                MetricType::Summary => "summary",
            };
            output.push_str(&format!("# TYPE {} {}\n", metric.name, type_str));

            // Format labels
            let labels_str = if metric.labels.is_empty() {
                String::new()
            } else {
                let pairs: Vec<String> = metric
                    .labels
                    .iter()
                    .map(|(k, v)| format!("{}=\"{}\"", k, v))
                    .collect();
                format!("{{{}}}", pairs.join(","))
            };

            // Add metric value
            match &metric.value {
                MetricData::Counter(v) => {
                    output.push_str(&format!("{}{} {}\n", metric.name, labels_str, v));
                }
                MetricData::Gauge(v) => {
                    output.push_str(&format!("{}{} {}\n", metric.name, labels_str, v));
                }
                MetricData::Histogram {
                    buckets,
                    sum,
                    count,
                } => {
                    for bucket in buckets {
                        output.push_str(&format!(
                            "{}_bucket{{le=\"{}\"{}}} {}\n",
                            metric.name,
                            bucket.upper_bound,
                            if labels_str.is_empty() {
                                String::new()
                            } else {
                                format!(",{}", &labels_str[1..labels_str.len() - 1])
                            },
                            bucket.count
                        ));
                    }
                    output.push_str(&format!("{}_sum{} {}\n", metric.name, labels_str, sum));
                    output.push_str(&format!("{}_count{} {}\n", metric.name, labels_str, count));
                }
                MetricData::Summary {
                    quantiles,
                    sum,
                    count,
                } => {
                    for (q, v) in quantiles {
                        output.push_str(&format!(
                            "{}{{quantile=\"{}\"{}}} {}\n",
                            metric.name,
                            q,
                            if labels_str.is_empty() {
                                String::new()
                            } else {
                                format!(",{}", &labels_str[1..labels_str.len() - 1])
                            },
                            v
                        ));
                    }
                    output.push_str(&format!("{}_sum{} {}\n", metric.name, labels_str, sum));
                    output.push_str(&format!("{}_count{} {}\n", metric.name, labels_str, count));
                }
            }
        }

        output
    }
}

impl Default for PrometheusExporter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetricsExporter for PrometheusExporter {
    async fn export(&self, metrics: &[MetricValue]) -> Result<()> {
        let _output = Self::format(metrics);
        // In a real implementation, this would write to a file or HTTP endpoint
        Ok(())
    }

    fn name(&self) -> &str {
        "prometheus"
    }
}

/// JSON exporter for metrics.
pub struct JsonExporter;

impl JsonExporter {
    /// Create a new JSON exporter.
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonExporter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MetricsExporter for JsonExporter {
    async fn export(&self, metrics: &[MetricValue]) -> Result<()> {
        serde_json::to_string_pretty(metrics)
            .map_err(|e| MetricsError::ExportFailed(e.to_string()))?;
        Ok(())
    }

    fn name(&self) -> &str {
        "json"
    }
}

/// Metrics registry for managing metrics.
pub struct MetricsRegistry {
    counters: RwLock<HashMap<String, Arc<Counter>>>,
    gauges: RwLock<HashMap<String, Arc<Gauge>>>,
    histograms: RwLock<HashMap<String, Arc<Histogram>>>,
    prefix: Option<String>,
}

impl MetricsRegistry {
    /// Create a new registry.
    pub fn new() -> Self {
        Self {
            counters: RwLock::new(HashMap::new()),
            gauges: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
            prefix: None,
        }
    }

    /// Create a registry with a prefix.
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            counters: RwLock::new(HashMap::new()),
            gauges: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
            prefix: Some(prefix.into()),
        }
    }

    /// Get the full metric name with prefix.
    fn full_name(&self, name: &str) -> String {
        match &self.prefix {
            Some(p) => format!("{}_{}", p, name),
            None => name.to_string(),
        }
    }

    /// Register a counter.
    pub async fn register_counter(
        &self,
        name: impl Into<String>,
        help: impl Into<String>,
    ) -> Arc<Counter> {
        let name = name.into();
        let full_name = self.full_name(&name);
        let counter = Arc::new(Counter::new(full_name.clone(), help));

        let mut counters = self.counters.write().await;
        counters.insert(full_name.clone(), counter.clone());
        counter
    }

    /// Get an existing counter.
    pub async fn get_counter(&self, name: &str) -> Option<Arc<Counter>> {
        let full_name = self.full_name(name);
        let counters = self.counters.read().await;
        counters.get(&full_name).cloned()
    }

    /// Register a gauge.
    pub async fn register_gauge(
        &self,
        name: impl Into<String>,
        help: impl Into<String>,
    ) -> Arc<Gauge> {
        let name = name.into();
        let full_name = self.full_name(&name);
        let gauge = Arc::new(Gauge::new(full_name.clone(), help));

        let mut gauges = self.gauges.write().await;
        gauges.insert(full_name.clone(), gauge.clone());
        gauge
    }

    /// Get an existing gauge.
    pub async fn get_gauge(&self, name: &str) -> Option<Arc<Gauge>> {
        let full_name = self.full_name(name);
        let gauges = self.gauges.read().await;
        gauges.get(&full_name).cloned()
    }

    /// Register a histogram.
    pub async fn register_histogram(
        &self,
        name: impl Into<String>,
        help: impl Into<String>,
    ) -> Arc<Histogram> {
        let name = name.into();
        let full_name = self.full_name(&name);
        let histogram = Arc::new(Histogram::new(full_name.clone(), help));

        let mut histograms = self.histograms.write().await;
        histograms.insert(full_name.clone(), histogram.clone());
        histogram
    }

    /// Register a histogram with custom buckets.
    pub async fn register_histogram_with_buckets(
        &self,
        name: impl Into<String>,
        help: impl Into<String>,
        buckets: Vec<f64>,
    ) -> Arc<Histogram> {
        let name = name.into();
        let full_name = self.full_name(&name);
        let histogram = Arc::new(Histogram::with_buckets(full_name.clone(), help, buckets));

        let mut histograms = self.histograms.write().await;
        histograms.insert(full_name.clone(), histogram.clone());
        histogram
    }

    /// Get an existing histogram.
    pub async fn get_histogram(&self, name: &str) -> Option<Arc<Histogram>> {
        let full_name = self.full_name(name);
        let histograms = self.histograms.read().await;
        histograms.get(&full_name).cloned()
    }

    /// Collect all metrics.
    pub async fn collect(&self) -> Vec<MetricValue> {
        let mut values = Vec::new();
        let now = Utc::now();

        // Collect counters
        {
            let counters = self.counters.read().await;
            for counter in counters.values() {
                values.push(MetricValue {
                    name: counter.name().to_string(),
                    metric_type: MetricType::Counter,
                    help: counter.help().to_string(),
                    labels: Labels::new(),
                    timestamp: now,
                    value: MetricData::Counter(counter.get()),
                });
            }
        }

        // Collect gauges
        {
            let gauges = self.gauges.read().await;
            for gauge in gauges.values() {
                values.push(MetricValue {
                    name: gauge.name().to_string(),
                    metric_type: MetricType::Gauge,
                    help: String::new(),
                    labels: Labels::new(),
                    timestamp: now,
                    value: MetricData::Gauge(gauge.get()),
                });
            }
        }

        // Collect histograms
        {
            let histograms = self.histograms.read().await;
            for histogram in histograms.values() {
                let buckets = histogram.buckets();
                let sum = {
                    let s = histogram.sum.read().await;
                    *s
                };
                values.push(MetricValue {
                    name: histogram.name().to_string(),
                    metric_type: MetricType::Histogram,
                    help: String::new(),
                    labels: Labels::new(),
                    timestamp: now,
                    value: MetricData::Histogram {
                        buckets,
                        sum,
                        count: histogram.count(),
                    },
                });
            }
        }

        values
    }

    /// Export metrics using an exporter.
    pub async fn export(&self, exporter: &dyn MetricsExporter) -> Result<()> {
        let metrics = self.collect().await;
        exporter.export(&metrics).await
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer for measuring durations.
pub struct Timer {
    start: std::time::Instant,
    histogram: Arc<Histogram>,
}

impl Timer {
    /// Create a new timer.
    pub fn new(histogram: Arc<Histogram>) -> Self {
        Self {
            start: std::time::Instant::now(),
            histogram,
        }
    }

    /// Stop the timer and record the duration.
    pub async fn stop(self) {
        let duration = self.start.elapsed().as_secs_f64();
        self.histogram.observe(duration).await;
    }

    /// Get elapsed time without stopping.
    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let counter = Counter::new("test_counter", "A test counter");
        assert_eq!(counter.get(), 0);

        counter.inc();
        assert_eq!(counter.get(), 1);

        counter.add(5);
        assert_eq!(counter.get(), 6);
    }

    #[test]
    fn test_gauge() {
        let gauge = Gauge::new("test_gauge", "A test gauge");
        assert_eq!(gauge.get(), 0);

        gauge.set(100);
        assert_eq!(gauge.get(), 100);

        gauge.inc();
        assert_eq!(gauge.get(), 101);

        gauge.dec();
        assert_eq!(gauge.get(), 100);

        gauge.add(10);
        assert_eq!(gauge.get(), 110);

        gauge.sub(20);
        assert_eq!(gauge.get(), 90);
    }

    #[tokio::test]
    async fn test_histogram() {
        let histogram = Histogram::new("test_histogram", "A test histogram");

        histogram.observe(0.1).await;
        histogram.observe(0.5).await;
        histogram.observe(1.0).await;

        assert_eq!(histogram.count(), 3);

        let buckets = histogram.buckets();
        assert!(!buckets.is_empty());
    }

    #[test]
    fn test_linear_buckets() {
        let buckets = Histogram::linear_buckets(0.0, 0.5, 5);
        assert_eq!(buckets, vec![0.0, 0.5, 1.0, 1.5, 2.0]);
    }

    #[test]
    fn test_exponential_buckets() {
        let buckets = Histogram::exponential_buckets(1.0, 2.0, 4);
        assert_eq!(buckets, vec![1.0, 2.0, 4.0, 8.0]);
    }

    #[tokio::test]
    async fn test_registry() {
        let registry = MetricsRegistry::new();

        let counter = registry
            .register_counter("requests_total", "Total requests")
            .await;
        counter.inc();

        let gauge = registry
            .register_gauge("active_connections", "Active connections")
            .await;
        gauge.set(42);

        let histogram = registry
            .register_histogram("request_duration", "Request duration")
            .await;
        histogram.observe(0.1).await;

        let metrics = registry.collect().await;
        assert_eq!(metrics.len(), 3);
    }

    #[tokio::test]
    async fn test_registry_with_prefix() {
        let registry = MetricsRegistry::with_prefix("myapp");

        let counter = registry.register_counter("requests", "Requests").await;
        assert_eq!(counter.name(), "myapp_requests");
    }

    #[test]
    fn test_labels() {
        let labels = Labels::new().add("method", "GET").add("path", "/api/users");

        assert_eq!(labels.get("method"), Some("GET"));
        assert_eq!(labels.get("path"), Some("/api/users"));
        assert_eq!(labels.get("unknown"), None);
    }

    #[test]
    fn test_prometheus_format() {
        let metrics = vec![MetricValue {
            name: "http_requests_total".to_string(),
            metric_type: MetricType::Counter,
            help: "Total HTTP requests".to_string(),
            labels: Labels::new().add("method", "GET"),
            timestamp: Utc::now(),
            value: MetricData::Counter(100),
        }];

        let output = PrometheusExporter::format(&metrics);
        assert!(output.contains("# HELP http_requests_total"));
        assert!(output.contains("# TYPE http_requests_total counter"));
        assert!(output.contains("http_requests_total{method=\"GET\"} 100"));
    }

    #[tokio::test]
    async fn test_timer() {
        let histogram = Arc::new(Histogram::new("operation_duration", "Operation duration"));
        let timer = Timer::new(histogram.clone());

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        timer.stop().await;

        assert_eq!(histogram.count(), 1);
    }
}
