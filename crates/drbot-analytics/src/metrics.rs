//! Metrics and aggregations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metric types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricType {
    /// Total message count.
    MessageCount,
    /// Total token count.
    TokenCount,
    /// Total API calls.
    ApiCallCount,
    /// Total errors.
    ErrorCount,
    /// Average latency.
    AverageLatency,
    /// Total sessions.
    SessionCount,
    /// Average session duration.
    AverageSessionDuration,
    /// Feature usage count.
    FeatureUsage(String),
    /// Model usage count.
    ModelUsage(String),
    /// Custom metric.
    Custom(String),
}

/// Metric value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricValue {
    /// Counter value.
    Count(u64),
    /// Gauge value.
    Gauge(f64),
    /// Distribution (min, max, avg, count).
    Distribution {
        min: f64,
        max: f64,
        avg: f64,
        count: u64,
    },
}

impl MetricValue {
    /// Get as count.
    pub fn as_count(&self) -> Option<u64> {
        match self {
            MetricValue::Count(c) => Some(*c),
            _ => None,
        }
    }

    /// Get as gauge.
    pub fn as_gauge(&self) -> Option<f64> {
        match self {
            MetricValue::Gauge(g) => Some(*g),
            _ => None,
        }
    }
}

/// A metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    /// Metric type.
    pub metric_type: MetricType,
    /// Metric value.
    pub value: MetricValue,
    /// Timestamp.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Labels.
    pub labels: HashMap<String, String>,
}

impl Metric {
    /// Create a new counter metric.
    pub fn counter(metric_type: MetricType, count: u64) -> Self {
        Self {
            metric_type,
            value: MetricValue::Count(count),
            timestamp: chrono::Utc::now(),
            labels: HashMap::new(),
        }
    }

    /// Create a new gauge metric.
    pub fn gauge(metric_type: MetricType, value: f64) -> Self {
        Self {
            metric_type,
            value: MetricValue::Gauge(value),
            timestamp: chrono::Utc::now(),
            labels: HashMap::new(),
        }
    }

    /// Add a label.
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}

/// Daily summary statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailySummary {
    /// Date.
    pub date: chrono::NaiveDate,
    /// Total messages.
    pub message_count: u64,
    /// User messages.
    pub user_messages: u64,
    /// Assistant messages.
    pub assistant_messages: u64,
    /// Total tokens used.
    pub tokens_used: u64,
    /// API calls made.
    pub api_calls: u64,
    /// Total errors.
    pub errors: u64,
    /// Sessions started.
    pub sessions: u64,
    /// Average latency in ms.
    pub avg_latency_ms: f64,
    /// Total session time in seconds.
    pub total_session_time_secs: u64,
    /// Model usage breakdown.
    pub model_usage: HashMap<String, u64>,
    /// Feature usage breakdown.
    pub feature_usage: HashMap<String, u64>,
    /// Estimated cost (if available).
    pub estimated_cost: Option<f64>,
}

impl DailySummary {
    /// Create a new daily summary for a date.
    pub fn new(date: chrono::NaiveDate) -> Self {
        Self {
            date,
            ..Default::default()
        }
    }

    /// Create for today.
    pub fn today() -> Self {
        Self::new(chrono::Utc::now().date_naive())
    }

    /// Merge another summary into this one.
    pub fn merge(&mut self, other: &DailySummary) {
        self.message_count += other.message_count;
        self.user_messages += other.user_messages;
        self.assistant_messages += other.assistant_messages;
        self.tokens_used += other.tokens_used;
        self.api_calls += other.api_calls;
        self.errors += other.errors;
        self.sessions += other.sessions;
        self.total_session_time_secs += other.total_session_time_secs;

        // Merge model usage
        for (model, count) in &other.model_usage {
            *self.model_usage.entry(model.clone()).or_default() += count;
        }

        // Merge feature usage
        for (feature, count) in &other.feature_usage {
            *self.feature_usage.entry(feature.clone()).or_default() += count;
        }

        // Recalculate average latency
        if other.api_calls > 0 {
            let total_calls = self.api_calls + other.api_calls;
            self.avg_latency_ms = (self.avg_latency_ms * self.api_calls as f64
                + other.avg_latency_ms * other.api_calls as f64)
                / total_calls as f64;
        }

        // Merge cost
        if let (Some(c1), Some(c2)) = (self.estimated_cost, other.estimated_cost) {
            self.estimated_cost = Some(c1 + c2);
        } else if other.estimated_cost.is_some() {
            self.estimated_cost = other.estimated_cost;
        }
    }

    /// Calculate messages per session.
    pub fn messages_per_session(&self) -> f64 {
        if self.sessions == 0 {
            0.0
        } else {
            self.message_count as f64 / self.sessions as f64
        }
    }

    /// Calculate average session duration.
    pub fn avg_session_duration_secs(&self) -> f64 {
        if self.sessions == 0 {
            0.0
        } else {
            self.total_session_time_secs as f64 / self.sessions as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_counter() {
        let metric = Metric::counter(MetricType::MessageCount, 42);
        assert_eq!(metric.value.as_count(), Some(42));
    }

    #[test]
    fn test_metric_with_label() {
        let metric = Metric::counter(MetricType::ApiCallCount, 10).with_label("model", "gpt-4");
        assert_eq!(metric.labels.get("model"), Some(&"gpt-4".to_string()));
    }

    #[test]
    fn test_daily_summary_merge() {
        let mut summary1 = DailySummary::today();
        summary1.message_count = 10;
        summary1.tokens_used = 100;

        let mut summary2 = DailySummary::today();
        summary2.message_count = 5;
        summary2.tokens_used = 50;

        summary1.merge(&summary2);

        assert_eq!(summary1.message_count, 15);
        assert_eq!(summary1.tokens_used, 150);
    }
}
