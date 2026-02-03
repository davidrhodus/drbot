//! Main analytics implementation.

use crate::ab_testing::ABTest;
use crate::events::Event;
use crate::metrics::DailySummary;
use crate::storage::{AnalyticsStorage, MemoryAnalyticsStorage};
use crate::{AnalyticsError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Analytics configuration.
#[derive(Debug, Clone)]
pub struct AnalyticsConfig {
    /// Whether analytics is enabled.
    pub enabled: bool,
    /// Data retention days.
    pub retention_days: u32,
    /// Batch size for writes.
    pub batch_size: usize,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_days: 90,
            batch_size: 100,
        }
    }
}

/// Analytics service.
pub struct Analytics {
    config: AnalyticsConfig,
    storage: Arc<dyn AnalyticsStorage>,
    tests: Arc<RwLock<HashMap<String, ABTest>>>,
}

impl Analytics {
    /// Create a new analytics service.
    pub async fn new() -> Result<Self> {
        Self::with_config(AnalyticsConfig::default()).await
    }

    /// Create with custom config.
    pub async fn with_config(config: AnalyticsConfig) -> Result<Self> {
        let storage = Arc::new(MemoryAnalyticsStorage::new());
        Ok(Self {
            config,
            storage,
            tests: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create with custom storage.
    pub fn with_storage(config: AnalyticsConfig, storage: Arc<dyn AnalyticsStorage>) -> Self {
        Self {
            config,
            storage,
            tests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Track an event.
    pub async fn track(&self, event: Event) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        debug!("Tracking event: {:?}", event.event_type);
        self.storage.store_event(event).await
    }

    /// Get daily summary for today.
    pub async fn daily_summary(&self) -> Result<DailySummary> {
        let today = chrono::Utc::now().date_naive();
        self.storage.get_daily_summary(today).await
    }

    /// Get daily summary for a specific date.
    pub async fn summary_for_date(&self, date: chrono::NaiveDate) -> Result<DailySummary> {
        self.storage.get_daily_summary(date).await
    }

    /// Get summaries for a date range.
    pub async fn summaries(
        &self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> Result<Vec<DailySummary>> {
        self.storage.get_summaries(start, end).await
    }

    /// Get events for a time range.
    pub async fn events(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Event>> {
        self.storage.get_events(start, end).await
    }

    /// Create an A/B test.
    pub async fn create_test(&self, test: ABTest) -> Result<String> {
        let test_id = test.id.clone();
        let mut tests = self.tests.write().await;
        tests.insert(test_id.clone(), test);
        info!("Created A/B test: {}", test_id);
        Ok(test_id)
    }

    /// Get an A/B test.
    pub async fn get_test(&self, test_id: &str) -> Option<ABTest> {
        let tests = self.tests.read().await;
        tests.get(test_id).cloned()
    }

    /// List all A/B tests.
    pub async fn list_tests(&self) -> Vec<ABTest> {
        let tests = self.tests.read().await;
        tests.values().cloned().collect()
    }

    /// Select a variant for a test.
    pub async fn select_variant(&self, test_id: &str) -> Option<String> {
        let tests = self.tests.read().await;
        tests
            .get(test_id)
            .and_then(|t| t.select_variant())
            .map(|v| v.id.clone())
    }

    /// Record a test sample.
    pub async fn record_sample(
        &self,
        test_id: &str,
        variant_id: &str,
        success: bool,
        latency_ms: u64,
        tokens: u64,
    ) -> Result<()> {
        let mut tests = self.tests.write().await;
        if let Some(test) = tests.get_mut(test_id) {
            if let Some(variant) = test.get_variant_mut(variant_id) {
                variant.record_sample(success, latency_ms, tokens);
                debug!(
                    "Recorded sample for test {} variant {}",
                    test_id, variant_id
                );
            }
        }
        Ok(())
    }

    /// Get test results.
    pub async fn test_results(&self, test_id: &str) -> Option<crate::ab_testing::TestResult> {
        let tests = self.tests.read().await;
        tests.get(test_id).map(|t| t.results())
    }

    /// Run cleanup to remove old data.
    pub async fn cleanup(&self) -> Result<usize> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(self.config.retention_days as i64);
        let removed = self.storage.cleanup(cutoff).await?;
        info!("Cleaned up {} old events", removed);
        Ok(removed)
    }

    /// Check if analytics is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the config.
    pub fn config(&self) -> &AnalyticsConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ab_testing::TestVariant;

    #[tokio::test]
    async fn test_analytics_tracking() {
        let analytics = Analytics::new().await.unwrap();

        analytics.track(Event::message("user", 50)).await.unwrap();
        analytics
            .track(Event::message("assistant", 100))
            .await
            .unwrap();

        let summary = analytics.daily_summary().await.unwrap();
        assert_eq!(summary.message_count, 2);
    }

    #[tokio::test]
    async fn test_ab_testing() {
        let analytics = Analytics::new().await.unwrap();

        let test = ABTest::new("Test")
            .add_variant(TestVariant::new("A", "model-a"))
            .add_variant(TestVariant::new("B", "model-b"));

        let test_id = analytics.create_test(test).await.unwrap();

        // Select variant
        let variant = analytics.select_variant(&test_id).await;
        assert!(variant.is_some());

        // Record sample
        analytics
            .record_sample(&test_id, &variant.unwrap(), true, 100, 50)
            .await
            .unwrap();

        // Get results
        let results = analytics.test_results(&test_id).await.unwrap();
        assert_eq!(results.variants.len(), 2);
    }
}
