//! Scheduler for proactive actions.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// A scheduled action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledAction {
    /// Action ID.
    pub id: Uuid,
    /// When to execute.
    pub scheduled_for: DateTime<Utc>,
    /// Action type.
    pub action_type: String,
    /// Action payload.
    pub payload: serde_json::Value,
    /// Target channel.
    pub channel_id: Option<String>,
    /// Target user.
    pub user_id: Option<String>,
    /// Whether action is recurring.
    pub recurring: bool,
    /// Recurrence interval (if recurring).
    pub recurrence_interval: Option<Duration>,
}

impl ScheduledAction {
    /// Create a one-time action.
    pub fn once(action_type: &str, scheduled_for: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            scheduled_for,
            action_type: action_type.to_string(),
            payload: serde_json::Value::Null,
            channel_id: None,
            user_id: None,
            recurring: false,
            recurrence_interval: None,
        }
    }

    /// Create a recurring action.
    pub fn recurring(action_type: &str, first_run: DateTime<Utc>, interval: Duration) -> Self {
        Self {
            id: Uuid::new_v4(),
            scheduled_for: first_run,
            action_type: action_type.to_string(),
            payload: serde_json::Value::Null,
            channel_id: None,
            user_id: None,
            recurring: true,
            recurrence_interval: Some(interval),
        }
    }

    /// Set payload.
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }

    /// Set target channel.
    pub fn for_channel(mut self, channel_id: &str) -> Self {
        self.channel_id = Some(channel_id.to_string());
        self
    }

    /// Set target user.
    pub fn for_user(mut self, user_id: &str) -> Self {
        self.user_id = Some(user_id.to_string());
        self
    }

    /// Create next occurrence for recurring actions.
    pub fn next_occurrence(&self) -> Option<ScheduledAction> {
        if !self.recurring {
            return None;
        }

        let interval = self.recurrence_interval?;
        let next_time = self.scheduled_for + interval;

        Some(ScheduledAction {
            id: Uuid::new_v4(),
            scheduled_for: next_time,
            ..self.clone()
        })
    }
}

// Ordering for BinaryHeap (min-heap by scheduled time)
impl Eq for ScheduledAction {}

impl PartialEq for ScheduledAction {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for ScheduledAction {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap
        other.scheduled_for.cmp(&self.scheduled_for)
    }
}

impl PartialOrd for ScheduledAction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Scheduler for managing timed actions.
pub struct Scheduler {
    queue: Arc<RwLock<BinaryHeap<ScheduledAction>>>,
}

impl Scheduler {
    /// Create a new scheduler.
    pub fn new() -> Self {
        Self {
            queue: Arc::new(RwLock::new(BinaryHeap::new())),
        }
    }

    /// Schedule an action.
    pub async fn schedule(&self, action: ScheduledAction) {
        let mut queue = self.queue.write().await;
        queue.push(action);
    }

    /// Get due actions.
    pub async fn get_due(&self) -> Vec<ScheduledAction> {
        let now = Utc::now();
        let mut queue = self.queue.write().await;
        let mut due = Vec::new();

        while let Some(action) = queue.peek() {
            if action.scheduled_for <= now {
                if let Some(action) = queue.pop() {
                    // Schedule next occurrence if recurring
                    if let Some(next) = action.next_occurrence() {
                        queue.push(next);
                    }
                    due.push(action);
                }
            } else {
                break;
            }
        }

        due
    }

    /// Get next action time.
    pub async fn next_action_time(&self) -> Option<DateTime<Utc>> {
        let queue = self.queue.read().await;
        queue.peek().map(|a| a.scheduled_for)
    }

    /// Cancel an action.
    pub async fn cancel(&self, id: Uuid) -> bool {
        let mut queue = self.queue.write().await;
        let original_len = queue.len();

        // Rebuild heap without the cancelled action
        let remaining: Vec<_> = queue.drain().filter(|a| a.id != id).collect();
        *queue = remaining.into_iter().collect();

        queue.len() < original_len
    }

    /// Get scheduled action count.
    pub async fn count(&self) -> usize {
        self.queue.read().await.len()
    }

    /// Clear all scheduled actions.
    pub async fn clear(&self) {
        let mut queue = self.queue.write().await;
        queue.clear();
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduled_action() {
        let action = ScheduledAction::once("test", Utc::now())
            .with_payload(serde_json::json!({"key": "value"}))
            .for_channel("channel1");

        assert!(!action.recurring);
        assert_eq!(action.channel_id, Some("channel1".to_string()));
    }

    #[test]
    fn test_recurring_action() {
        let action = ScheduledAction::recurring("daily", Utc::now(), Duration::days(1));

        assert!(action.recurring);
        let next = action.next_occurrence().unwrap();
        assert!(next.scheduled_for > action.scheduled_for);
    }

    #[tokio::test]
    async fn test_scheduler() {
        let scheduler = Scheduler::new();

        // Schedule actions in past and future
        let past = ScheduledAction::once("past", Utc::now() - Duration::hours(1));
        let future = ScheduledAction::once("future", Utc::now() + Duration::hours(1));

        scheduler.schedule(past).await;
        scheduler.schedule(future).await;

        let due = scheduler.get_due().await;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].action_type, "past");
    }
}
