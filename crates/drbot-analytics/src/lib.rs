//! Usage analytics and insights for drbot.
//!
//! Tracks usage patterns, model performance, and provides insights.
//!
//! # Features
//!
//! - Usage tracking (messages, tokens, costs)
//! - Model performance metrics
//! - A/B testing for model comparisons
//! - Privacy-preserving local analytics
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_analytics::{Analytics, Event, MetricType};
//!
//! async fn example() {
//!     let analytics = Analytics::new().await.unwrap();
//!
//!     // Track an event
//!     analytics.track(Event::message("user", 50)).await;
//!
//!     // Get metrics
//!     let daily = analytics.daily_summary().await.unwrap();
//!     println!("Messages today: {}", daily.message_count);
//! }
//! ```

mod ab_testing;
mod analytics;
mod events;
mod metrics;
mod storage;

pub use ab_testing::{ABTest, TestResult, TestVariant};
pub use analytics::{Analytics, AnalyticsConfig};
pub use events::{Event, EventType};
pub use metrics::{DailySummary, Metric, MetricType, MetricValue};
pub use storage::{AnalyticsStorage, MemoryAnalyticsStorage};

/// Result type.
pub type Result<T> = std::result::Result<T, AnalyticsError>;

/// Analytics errors.
#[derive(Debug, thiserror::Error)]
pub enum AnalyticsError {
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Invalid metric: {0}")]
    InvalidMetric(String),
    #[error("Test not found: {0}")]
    TestNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_analytics_basic() {
        let analytics = Analytics::new().await.unwrap();
        analytics.track(Event::message("user", 10)).await.unwrap();

        let summary = analytics.daily_summary().await.unwrap();
        assert_eq!(summary.message_count, 1);
    }
}
