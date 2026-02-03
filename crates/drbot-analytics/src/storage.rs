//! Storage backend for analytics.

use crate::events::Event;
use crate::metrics::{DailySummary, Metric};
use crate::{AnalyticsError, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for analytics storage.
#[async_trait]
pub trait AnalyticsStorage: Send + Sync {
    /// Store an event.
    async fn store_event(&self, event: Event) -> Result<()>;

    /// Get events for a date range.
    async fn get_events(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Event>>;

    /// Store a metric.
    async fn store_metric(&self, metric: Metric) -> Result<()>;

    /// Get daily summary.
    async fn get_daily_summary(&self, date: chrono::NaiveDate) -> Result<DailySummary>;

    /// Get all daily summaries in range.
    async fn get_summaries(
        &self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> Result<Vec<DailySummary>>;

    /// Clear old data.
    async fn cleanup(&self, before: chrono::DateTime<chrono::Utc>) -> Result<usize>;
}

/// In-memory analytics storage.
#[derive(Debug, Default)]
pub struct MemoryAnalyticsStorage {
    events: Arc<RwLock<Vec<Event>>>,
    metrics: Arc<RwLock<Vec<Metric>>>,
    summaries: Arc<RwLock<HashMap<chrono::NaiveDate, DailySummary>>>,
}

impl MemoryAnalyticsStorage {
    /// Create a new memory storage.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl AnalyticsStorage for MemoryAnalyticsStorage {
    async fn store_event(&self, event: Event) -> Result<()> {
        let mut events = self.events.write().await;
        events.push(event.clone());

        // Update daily summary
        let date = event.timestamp.date_naive();
        let mut summaries = self.summaries.write().await;
        let summary = summaries
            .entry(date)
            .or_insert_with(|| DailySummary::new(date));

        // Update based on event type
        use crate::events::EventType;
        match &event.event_type {
            EventType::Message => {
                summary.message_count += 1;
                if event.get_property_str("role") == Some("user") {
                    summary.user_messages += 1;
                } else {
                    summary.assistant_messages += 1;
                }
                if let Some(tokens) = event.get_property_num("tokens") {
                    summary.tokens_used += tokens as u64;
                }
            }
            EventType::ApiCall => {
                summary.api_calls += 1;
                if let Some(model) = &event.model {
                    *summary.model_usage.entry(model.clone()).or_default() += 1;
                }
                if let Some(latency) = event.get_property_num("latency_ms") {
                    // Update running average
                    summary.avg_latency_ms =
                        (summary.avg_latency_ms * (summary.api_calls - 1) as f64 + latency)
                            / summary.api_calls as f64;
                }
            }
            EventType::SessionStart => {
                summary.sessions += 1;
            }
            EventType::SessionEnd => {
                if let Some(duration) = event.get_property_num("duration_secs") {
                    summary.total_session_time_secs += duration as u64;
                }
            }
            EventType::Error => {
                summary.errors += 1;
            }
            EventType::FeatureUsed => {
                if let Some(feature) = event.get_property_str("feature") {
                    *summary
                        .feature_usage
                        .entry(feature.to_string())
                        .or_default() += 1;
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn get_events(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Event>> {
        let events = self.events.read().await;
        Ok(events
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .cloned()
            .collect())
    }

    async fn store_metric(&self, metric: Metric) -> Result<()> {
        let mut metrics = self.metrics.write().await;
        metrics.push(metric);
        Ok(())
    }

    async fn get_daily_summary(&self, date: chrono::NaiveDate) -> Result<DailySummary> {
        let summaries = self.summaries.read().await;
        Ok(summaries
            .get(&date)
            .cloned()
            .unwrap_or_else(|| DailySummary::new(date)))
    }

    async fn get_summaries(
        &self,
        start: chrono::NaiveDate,
        end: chrono::NaiveDate,
    ) -> Result<Vec<DailySummary>> {
        let summaries = self.summaries.read().await;
        let mut result: Vec<_> = summaries
            .iter()
            .filter(|(date, _)| **date >= start && **date <= end)
            .map(|(_, s)| s.clone())
            .collect();

        result.sort_by_key(|s| s.date);
        Ok(result)
    }

    async fn cleanup(&self, before: chrono::DateTime<chrono::Utc>) -> Result<usize> {
        let mut events = self.events.write().await;
        let before_len = events.len();
        events.retain(|e| e.timestamp >= before);
        let removed = before_len - events.len();

        // Also clean up old summaries
        let before_date = before.date_naive();
        let mut summaries = self.summaries.write().await;
        summaries.retain(|date, _| *date >= before_date);

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Event;

    #[tokio::test]
    async fn test_memory_storage_events() {
        let storage = MemoryAnalyticsStorage::new();

        let event = Event::message("user", 100);
        storage.store_event(event).await.unwrap();

        let now = chrono::Utc::now();
        let events = storage
            .get_events(
                now - chrono::Duration::hours(1),
                now + chrono::Duration::hours(1),
            )
            .await
            .unwrap();

        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn test_daily_summary_updates() {
        let storage = MemoryAnalyticsStorage::new();

        storage
            .store_event(Event::message("user", 50))
            .await
            .unwrap();
        storage
            .store_event(Event::message("assistant", 100))
            .await
            .unwrap();
        storage
            .store_event(Event::api_call("gpt-4", 50, 100, 500))
            .await
            .unwrap();

        let summary = storage
            .get_daily_summary(chrono::Utc::now().date_naive())
            .await
            .unwrap();

        assert_eq!(summary.message_count, 2);
        assert_eq!(summary.user_messages, 1);
        assert_eq!(summary.assistant_messages, 1);
        assert_eq!(summary.api_calls, 1);
    }
}
