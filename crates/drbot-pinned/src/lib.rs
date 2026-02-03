//! Pinned context for drbot.
//!
//! Keep important information always visible.
//!
//! # Features
//!
//! - Pin important messages
//! - Context persistence
//! - Priority ordering
//! - Automatic relevance tracking

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Pinned context result type.
pub type Result<T> = std::result::Result<T, PinnedError>;

/// Pinned context errors.
#[derive(Debug, thiserror::Error)]
pub enum PinnedError {
    #[error("Item not found: {0}")]
    NotFound(Uuid),
    #[error("Maximum pins reached")]
    MaxPinsReached,
    #[error("Invalid priority")]
    InvalidPriority,
    #[error("Already pinned")]
    AlreadyPinned,
}

/// A pinned item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedItem {
    /// Item ID.
    pub id: Uuid,
    /// Content.
    pub content: String,
    /// Item type.
    pub item_type: PinnedType,
    /// Priority (higher = more important).
    pub priority: i32,
    /// Tags.
    pub tags: Vec<String>,
    /// Source.
    pub source: Option<String>,
    /// Access count.
    pub access_count: u64,
    /// Last accessed.
    pub last_accessed: Option<DateTime<Utc>>,
    /// Pinned at.
    pub pinned_at: DateTime<Utc>,
    /// Expires at.
    pub expires_at: Option<DateTime<Utc>>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PinnedItem {
    /// Create a new pinned item.
    pub fn new(content: &str, item_type: PinnedType) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.to_string(),
            item_type,
            priority: 0,
            tags: Vec::new(),
            source: None,
            access_count: 0,
            last_accessed: None,
            pinned_at: Utc::now(),
            expires_at: None,
            metadata: HashMap::new(),
        }
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Add tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set source.
    pub fn with_source(mut self, source: &str) -> Self {
        self.source = Some(source.to_string());
        self
    }

    /// Set expiration.
    pub fn with_expiration(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Record access.
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Some(Utc::now());
    }

    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            Utc::now() > expires
        } else {
            false
        }
    }
}

/// Pinned item types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinnedType {
    /// Important message.
    Message,
    /// Reference information.
    Reference,
    /// Instruction or guideline.
    Instruction,
    /// Reminder.
    Reminder,
    /// Note.
    Note,
    /// Context.
    Context,
    /// Other.
    Other,
}

/// Pinned context configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedConfig {
    /// Maximum pinned items.
    pub max_pins: usize,
    /// Auto-remove expired.
    pub auto_remove_expired: bool,
    /// Track access.
    pub track_access: bool,
    /// Default expiration hours.
    pub default_expiration_hours: Option<u64>,
}

impl Default for PinnedConfig {
    fn default() -> Self {
        Self {
            max_pins: 20,
            auto_remove_expired: true,
            track_access: true,
            default_expiration_hours: None,
        }
    }
}

/// Pinned context manager.
pub struct PinnedContextManager {
    config: PinnedConfig,
    items: Arc<RwLock<HashMap<Uuid, PinnedItem>>>,
    order: Arc<RwLock<Vec<Uuid>>>,
}

impl PinnedContextManager {
    /// Create a new manager.
    pub fn new(config: PinnedConfig) -> Self {
        Self {
            config,
            items: Arc::new(RwLock::new(HashMap::new())),
            order: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Pin an item.
    pub async fn pin(&self, mut item: PinnedItem) -> Result<Uuid> {
        // Clean up expired items first
        if self.config.auto_remove_expired {
            self.remove_expired().await;
        }

        // Check max pins
        let items = self.items.read().await;
        if items.len() >= self.config.max_pins {
            return Err(PinnedError::MaxPinsReached);
        }
        drop(items);

        // Set default expiration if configured
        if item.expires_at.is_none() {
            if let Some(hours) = self.config.default_expiration_hours {
                item.expires_at = Some(Utc::now() + chrono::Duration::hours(hours as i64));
            }
        }

        let id = item.id;

        self.items.write().await.insert(id, item);
        self.order.write().await.push(id);

        // Re-sort by priority
        self.sort_by_priority().await;

        Ok(id)
    }

    /// Unpin an item.
    pub async fn unpin(&self, id: Uuid) -> Result<PinnedItem> {
        let item = self
            .items
            .write()
            .await
            .remove(&id)
            .ok_or(PinnedError::NotFound(id))?;

        self.order.write().await.retain(|&i| i != id);

        Ok(item)
    }

    /// Get a pinned item.
    pub async fn get(&self, id: Uuid) -> Option<PinnedItem> {
        let mut items = self.items.write().await;
        if let Some(item) = items.get_mut(&id) {
            if self.config.track_access {
                item.record_access();
            }
            return Some(item.clone());
        }
        None
    }

    /// Get all pinned items in priority order.
    pub async fn get_all(&self) -> Vec<PinnedItem> {
        let order = self.order.read().await;
        let items = self.items.read().await;

        order
            .iter()
            .filter_map(|id| items.get(id).cloned())
            .collect()
    }

    /// Get items by type.
    pub async fn get_by_type(&self, item_type: PinnedType) -> Vec<PinnedItem> {
        self.items
            .read()
            .await
            .values()
            .filter(|i| i.item_type == item_type)
            .cloned()
            .collect()
    }

    /// Get items by tag.
    pub async fn get_by_tag(&self, tag: &str) -> Vec<PinnedItem> {
        self.items
            .read()
            .await
            .values()
            .filter(|i| i.tags.contains(&tag.to_string()))
            .cloned()
            .collect()
    }

    /// Search pinned items.
    pub async fn search(&self, query: &str) -> Vec<PinnedItem> {
        let query_lower = query.to_lowercase();
        self.items
            .read()
            .await
            .values()
            .filter(|i| i.content.to_lowercase().contains(&query_lower))
            .cloned()
            .collect()
    }

    /// Update priority.
    pub async fn set_priority(&self, id: Uuid, priority: i32) -> Result<()> {
        let mut items = self.items.write().await;
        let item = items.get_mut(&id).ok_or(PinnedError::NotFound(id))?;
        item.priority = priority;
        drop(items);

        self.sort_by_priority().await;
        Ok(())
    }

    /// Move item to top.
    pub async fn move_to_top(&self, id: Uuid) -> Result<()> {
        let items = self.items.read().await;
        if !items.contains_key(&id) {
            return Err(PinnedError::NotFound(id));
        }

        let max_priority = items.values().map(|i| i.priority).max().unwrap_or(0);
        drop(items);

        self.set_priority(id, max_priority + 1).await
    }

    async fn sort_by_priority(&self) {
        let items = self.items.read().await;
        let mut order = self.order.write().await;

        order.sort_by(|a, b| {
            let priority_a = items.get(a).map(|i| i.priority).unwrap_or(0);
            let priority_b = items.get(b).map(|i| i.priority).unwrap_or(0);
            priority_b.cmp(&priority_a)
        });
    }

    async fn remove_expired(&self) {
        let mut items = self.items.write().await;
        let expired: Vec<_> = items
            .values()
            .filter(|i| i.is_expired())
            .map(|i| i.id)
            .collect();

        for id in expired {
            items.remove(&id);
        }

        let mut order = self.order.write().await;
        order.retain(|id| items.contains_key(id));
    }

    /// Get context string (all pinned content).
    pub async fn get_context(&self) -> String {
        let items = self.get_all().await;

        items
            .iter()
            .map(|i| format!("[{}] {}", format!("{:?}", i.item_type), i.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Clear all pinned items.
    pub async fn clear(&self) {
        self.items.write().await.clear();
        self.order.write().await.clear();
    }

    /// Get statistics.
    pub async fn stats(&self) -> PinnedStats {
        let items = self.items.read().await;

        let mut by_type: HashMap<PinnedType, usize> = HashMap::new();
        let mut total_access = 0u64;

        for item in items.values() {
            *by_type.entry(item.item_type).or_insert(0) += 1;
            total_access += item.access_count;
        }

        let expired = items.values().filter(|i| i.is_expired()).count();

        PinnedStats {
            total_pins: items.len(),
            by_type,
            expired_count: expired,
            total_access,
        }
    }
}

/// Pinned context statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedStats {
    pub total_pins: usize,
    pub by_type: HashMap<PinnedType, usize>,
    pub expired_count: usize,
    pub total_access: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pin_unpin() {
        let manager = PinnedContextManager::new(PinnedConfig::default());

        let item = PinnedItem::new("Important note", PinnedType::Note);
        let id = manager.pin(item).await.unwrap();

        let retrieved = manager.get(id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().content, "Important note");

        let unpinned = manager.unpin(id).await.unwrap();
        assert_eq!(unpinned.content, "Important note");

        assert!(manager.get(id).await.is_none());
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let manager = PinnedContextManager::new(PinnedConfig::default());

        manager
            .pin(PinnedItem::new("Low", PinnedType::Note).with_priority(1))
            .await
            .unwrap();
        manager
            .pin(PinnedItem::new("High", PinnedType::Note).with_priority(10))
            .await
            .unwrap();
        manager
            .pin(PinnedItem::new("Medium", PinnedType::Note).with_priority(5))
            .await
            .unwrap();

        let all = manager.get_all().await;
        assert_eq!(all[0].content, "High");
        assert_eq!(all[1].content, "Medium");
        assert_eq!(all[2].content, "Low");
    }

    #[tokio::test]
    async fn test_max_pins() {
        let config = PinnedConfig {
            max_pins: 2,
            ..Default::default()
        };
        let manager = PinnedContextManager::new(config);

        manager
            .pin(PinnedItem::new("First", PinnedType::Note))
            .await
            .unwrap();
        manager
            .pin(PinnedItem::new("Second", PinnedType::Note))
            .await
            .unwrap();

        let result = manager
            .pin(PinnedItem::new("Third", PinnedType::Note))
            .await;
        assert!(matches!(result, Err(PinnedError::MaxPinsReached)));
    }

    #[tokio::test]
    async fn test_get_by_type() {
        let manager = PinnedContextManager::new(PinnedConfig::default());

        manager
            .pin(PinnedItem::new("Note 1", PinnedType::Note))
            .await
            .unwrap();
        manager
            .pin(PinnedItem::new("Reminder 1", PinnedType::Reminder))
            .await
            .unwrap();
        manager
            .pin(PinnedItem::new("Note 2", PinnedType::Note))
            .await
            .unwrap();

        let notes = manager.get_by_type(PinnedType::Note).await;
        assert_eq!(notes.len(), 2);
    }

    #[tokio::test]
    async fn test_tags() {
        let manager = PinnedContextManager::new(PinnedConfig::default());

        manager
            .pin(
                PinnedItem::new("Tagged item", PinnedType::Note)
                    .with_tags(vec!["important".to_string(), "work".to_string()]),
            )
            .await
            .unwrap();

        let by_tag = manager.get_by_tag("important").await;
        assert_eq!(by_tag.len(), 1);
    }

    #[tokio::test]
    async fn test_search() {
        let manager = PinnedContextManager::new(PinnedConfig::default());

        manager
            .pin(PinnedItem::new("Meeting at 3pm", PinnedType::Reminder))
            .await
            .unwrap();
        manager
            .pin(PinnedItem::new("Call John", PinnedType::Note))
            .await
            .unwrap();

        let results = manager.search("meeting").await;
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("Meeting"));
    }
}
