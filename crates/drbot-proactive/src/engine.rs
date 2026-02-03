//! Proactive engine for intelligent outreach.

use crate::scheduler::Scheduler;
use crate::trigger::ProactiveTrigger;
use crate::{ProactiveConfig, ProactiveError, ProactiveMessage, Result};
use chrono::{DateTime, Duration, Timelike, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Engine configuration.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Base proactive config.
    pub config: ProactiveConfig,
    /// Check interval in seconds.
    pub check_interval_secs: u64,
    /// Queue size.
    pub queue_size: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            config: ProactiveConfig::default(),
            check_interval_secs: 60,
            queue_size: 100,
        }
    }
}

/// Proactive engine that manages intelligent outreach.
pub struct ProactiveEngine {
    config: EngineConfig,
    triggers: Arc<RwLock<Vec<ProactiveTrigger>>>,
    scheduler: Arc<Scheduler>,
    message_queue: Arc<RwLock<Vec<ProactiveMessage>>>,
    daily_counts: Arc<RwLock<HashMap<String, u32>>>,
    last_message_times: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    running: Arc<RwLock<bool>>,
}

impl ProactiveEngine {
    /// Create a new proactive engine.
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            triggers: Arc::new(RwLock::new(Vec::new())),
            scheduler: Arc::new(Scheduler::new()),
            message_queue: Arc::new(RwLock::new(Vec::new())),
            daily_counts: Arc::new(RwLock::new(HashMap::new())),
            last_message_times: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Register a trigger.
    pub async fn register_trigger(&self, trigger: ProactiveTrigger) {
        let mut triggers = self.triggers.write().await;
        triggers.push(trigger);
    }

    /// Start the engine.
    pub async fn start(&self, sender: mpsc::Sender<ProactiveMessage>) -> Result<()> {
        if !self.config.config.enabled {
            info!("Proactive engine disabled");
            return Ok(());
        }

        *self.running.write().await = true;
        info!("Starting proactive engine");

        let config = self.config.clone();
        let triggers = Arc::clone(&self.triggers);
        let message_queue = Arc::clone(&self.message_queue);
        let daily_counts = Arc::clone(&self.daily_counts);
        let last_message_times = Arc::clone(&self.last_message_times);
        let running = Arc::clone(&self.running);

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(config.check_interval_secs));

            while *running.read().await {
                interval.tick().await;

                // Check triggers
                let triggers_snapshot = triggers.read().await.clone();
                for trigger in triggers_snapshot {
                    if trigger.should_fire(&config.config).await {
                        if let Some(message) = trigger.create_message().await {
                            // Check rate limits
                            if Self::check_rate_limits(
                                &config.config,
                                &message.channel_id,
                                &daily_counts,
                                &last_message_times,
                            )
                            .await
                            {
                                if sender.send(message.clone()).await.is_ok() {
                                    Self::record_message_sent(
                                        &message.channel_id,
                                        &daily_counts,
                                        &last_message_times,
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                }

                // Process scheduled messages
                let mut queue = message_queue.write().await;
                let now = Utc::now();
                let (ready, pending): (Vec<_>, Vec<_>) =
                    queue.drain(..).partition(|m| m.scheduled_for <= now);

                *queue = pending;

                for message in ready {
                    if Self::check_rate_limits(
                        &config.config,
                        &message.channel_id,
                        &daily_counts,
                        &last_message_times,
                    )
                    .await
                    {
                        if sender.send(message.clone()).await.is_ok() {
                            Self::record_message_sent(
                                &message.channel_id,
                                &daily_counts,
                                &last_message_times,
                            )
                            .await;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop the engine.
    pub async fn stop(&self) {
        *self.running.write().await = false;
        info!("Proactive engine stopped");
    }

    /// Queue a message for later delivery.
    pub async fn queue_message(&self, message: ProactiveMessage) -> Result<()> {
        let mut queue = self.message_queue.write().await;
        if queue.len() >= self.config.queue_size {
            return Err(ProactiveError::TriggerFailed(
                "Message queue full".to_string(),
            ));
        }
        queue.push(message);
        Ok(())
    }

    /// Check rate limits.
    async fn check_rate_limits(
        config: &ProactiveConfig,
        channel_id: &str,
        daily_counts: &RwLock<HashMap<String, u32>>,
        last_message_times: &RwLock<HashMap<String, DateTime<Utc>>>,
    ) -> bool {
        let now = Utc::now();

        // Check quiet hours
        if let (Some(start), Some(end)) = (config.quiet_hours_start, config.quiet_hours_end) {
            let hour = now.time().hour() as u8;
            if start > end {
                // Overnight quiet hours (e.g., 22-8)
                if hour >= start || hour < end {
                    debug!("Quiet hours active, skipping message");
                    return false;
                }
            } else if hour >= start && hour < end {
                debug!("Quiet hours active, skipping message");
                return false;
            }
        }

        // Check daily limit
        {
            let counts = daily_counts.read().await;
            if let Some(count) = counts.get(channel_id) {
                if *count >= config.max_daily_messages {
                    debug!("Daily message limit reached for channel {}", channel_id);
                    return false;
                }
            }
        }

        // Check minimum interval
        {
            let times = last_message_times.read().await;
            if let Some(last_time) = times.get(channel_id) {
                let elapsed = now.signed_duration_since(*last_time);
                if elapsed < Duration::seconds(config.min_interval_secs as i64) {
                    debug!("Minimum interval not met for channel {}", channel_id);
                    return false;
                }
            }
        }

        true
    }

    /// Record that a message was sent.
    async fn record_message_sent(
        channel_id: &str,
        daily_counts: &RwLock<HashMap<String, u32>>,
        last_message_times: &RwLock<HashMap<String, DateTime<Utc>>>,
    ) {
        let now = Utc::now();

        {
            let mut counts = daily_counts.write().await;
            *counts.entry(channel_id.to_string()).or_insert(0) += 1;
        }

        {
            let mut times = last_message_times.write().await;
            times.insert(channel_id.to_string(), now);
        }
    }

    /// Reset daily counts (call at midnight).
    pub async fn reset_daily_counts(&self) {
        let mut counts = self.daily_counts.write().await;
        counts.clear();
    }

    /// Get engine status.
    pub async fn status(&self) -> EngineStatus {
        let running = *self.running.read().await;
        let trigger_count = self.triggers.read().await.len();
        let queue_size = self.message_queue.read().await.len();

        EngineStatus {
            running,
            trigger_count,
            queue_size,
        }
    }
}

/// Engine status.
#[derive(Debug, Clone)]
pub struct EngineStatus {
    /// Whether engine is running.
    pub running: bool,
    /// Number of registered triggers.
    pub trigger_count: usize,
    /// Current queue size.
    pub queue_size: usize,
}

impl Default for ProactiveEngine {
    fn default() -> Self {
        Self::new(EngineConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_engine_creation() {
        let engine = ProactiveEngine::new(EngineConfig::default());
        let status = engine.status().await;

        assert!(!status.running);
        assert_eq!(status.trigger_count, 0);
    }
}
