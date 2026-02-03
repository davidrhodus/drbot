//! Universal AI-enhanced clipboard for drbot.
//!
//! Cross-device clipboard with smart features.
//!
//! # Features
//!
//! - Cross-device sync
//! - Smart paste with context
//! - Clipboard history
//! - Content transformation
//! - Secure sharing

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Universal clipboard result type.
pub type Result<T> = std::result::Result<T, ClipError>;

/// Clipboard errors.
#[derive(Debug, thiserror::Error)]
pub enum ClipError {
    #[error("Item not found: {0}")]
    NotFound(String),
    #[error("Sync failed: {0}")]
    SyncFailed(String),
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
}

/// Clipboard content type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    RichText,
    Html,
    Image,
    File,
    Url,
    Code,
    Json,
    Custom,
}

/// Clipboard item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipItem {
    /// Item ID.
    pub id: Uuid,
    /// Content type.
    pub content_type: ContentType,
    /// Text content.
    pub text: Option<String>,
    /// Binary content (base64).
    pub binary: Option<String>,
    /// Source device.
    pub source_device: String,
    /// Source app.
    pub source_app: Option<String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Pinned.
    pub pinned: bool,
    /// Tags.
    pub tags: Vec<String>,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl ClipItem {
    /// Create a text item.
    pub fn text(content: &str, device: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            content_type: ContentType::Text,
            text: Some(content.to_string()),
            binary: None,
            source_device: device.to_string(),
            source_app: None,
            created_at: Utc::now(),
            pinned: false,
            tags: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Create a URL item.
    pub fn url(url: &str, device: &str) -> Self {
        let mut item = Self::text(url, device);
        item.content_type = ContentType::Url;
        item
    }

    /// Create a code item.
    pub fn code(code: &str, language: &str, device: &str) -> Self {
        let mut item = Self::text(code, device);
        item.content_type = ContentType::Code;
        item.metadata
            .insert("language".to_string(), language.to_string());
        item
    }
}

/// Smart paste suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasteSuggestion {
    /// Suggestion ID.
    pub id: Uuid,
    /// Original item.
    pub item_id: Uuid,
    /// Transformed content.
    pub content: String,
    /// Transformation applied.
    pub transformation: String,
    /// Confidence.
    pub confidence: f32,
    /// Reason.
    pub reason: String,
}

/// Device info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Device ID.
    pub id: String,
    /// Device name.
    pub name: String,
    /// Device type.
    pub device_type: DeviceType,
    /// Last seen.
    pub last_seen: DateTime<Utc>,
    /// Is online.
    pub online: bool,
}

/// Device type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Desktop,
    Laptop,
    Phone,
    Tablet,
    Unknown,
}

/// Sync event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncEvent {
    /// New item added.
    ItemAdded(ClipItem),
    /// Item deleted.
    ItemDeleted(Uuid),
    /// Item updated.
    ItemUpdated(ClipItem),
    /// Device connected.
    DeviceConnected(DeviceInfo),
    /// Device disconnected.
    DeviceDisconnected(String),
}

/// Universal clipboard configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniClipConfig {
    /// Enable sync.
    pub sync_enabled: bool,
    /// History limit.
    pub history_limit: usize,
    /// Auto-detect content type.
    pub auto_detect: bool,
    /// Enable smart paste.
    pub smart_paste: bool,
    /// Encrypt content.
    pub encrypt: bool,
    /// Sync images.
    pub sync_images: bool,
}

impl Default for UniClipConfig {
    fn default() -> Self {
        Self {
            sync_enabled: true,
            history_limit: 100,
            auto_detect: true,
            smart_paste: true,
            encrypt: true,
            sync_images: true,
        }
    }
}

/// Trait for clipboard sync providers.
#[async_trait]
pub trait SyncProvider: Send + Sync {
    /// Push item to cloud.
    async fn push(&self, item: &ClipItem) -> Result<()>;
    /// Pull items from cloud.
    async fn pull(&self, since: DateTime<Utc>) -> Result<Vec<ClipItem>>;
    /// Delete item from cloud.
    async fn delete(&self, item_id: Uuid) -> Result<()>;
}

/// Trait for smart paste transformers.
#[async_trait]
pub trait PasteTransformer: Send + Sync {
    /// Suggest smart paste options.
    async fn suggest(&self, item: &ClipItem, context: &PasteContext) -> Vec<PasteSuggestion>;
}

/// Paste context.
#[derive(Debug, Clone, Default)]
pub struct PasteContext {
    /// Target app.
    pub target_app: Option<String>,
    /// Target field type.
    pub field_type: Option<String>,
    /// Surrounding text.
    pub surrounding: Option<String>,
}

/// Universal clipboard manager.
pub struct UniClipManager<S: SyncProvider, T: PasteTransformer> {
    config: UniClipConfig,
    sync_provider: S,
    transformer: T,
    device_id: String,
    items: Arc<RwLock<Vec<ClipItem>>>,
    devices: Arc<RwLock<HashMap<String, DeviceInfo>>>,
    event_tx: broadcast::Sender<SyncEvent>,
}

impl<S: SyncProvider, T: PasteTransformer> UniClipManager<S, T> {
    /// Create a new universal clipboard manager.
    pub fn new(config: UniClipConfig, device_id: &str, sync_provider: S, transformer: T) -> Self {
        let (event_tx, _) = broadcast::channel(100);

        Self {
            config,
            sync_provider,
            transformer,
            device_id: device_id.to_string(),
            items: Arc::new(RwLock::new(Vec::new())),
            devices: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Copy content to clipboard.
    pub async fn copy(&self, content: &str) -> Result<ClipItem> {
        let content_type = if self.config.auto_detect {
            Self::detect_type(content)
        } else {
            ContentType::Text
        };

        let mut item = ClipItem::text(content, &self.device_id);
        item.content_type = content_type;

        self.add_item(item.clone()).await?;
        Ok(item)
    }

    /// Add an item.
    pub async fn add_item(&self, item: ClipItem) -> Result<()> {
        {
            let mut items = self.items.write().await;

            // Remove duplicates
            items.retain(|i| i.text != item.text);

            // Add new item at front
            items.insert(0, item.clone());

            // Trim history
            if items.len() > self.config.history_limit {
                items.truncate(self.config.history_limit);
            }
        }

        // Sync to cloud
        if self.config.sync_enabled {
            self.sync_provider.push(&item).await?;
        }

        // Notify
        let _ = self.event_tx.send(SyncEvent::ItemAdded(item));

        Ok(())
    }

    /// Get current clipboard content.
    pub async fn get_current(&self) -> Option<ClipItem> {
        self.items.read().await.first().cloned()
    }

    /// Get clipboard history.
    pub async fn history(&self, limit: usize) -> Vec<ClipItem> {
        self.items
            .read()
            .await
            .iter()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Search clipboard history.
    pub async fn search(&self, query: &str) -> Vec<ClipItem> {
        let query_lower = query.to_lowercase();
        self.items
            .read()
            .await
            .iter()
            .filter(|item| {
                item.text
                    .as_ref()
                    .map(|t| t.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// Get smart paste suggestions.
    pub async fn smart_paste(&self, context: &PasteContext) -> Vec<PasteSuggestion> {
        let current = match self.get_current().await {
            Some(item) => item,
            None => return Vec::new(),
        };

        self.transformer.suggest(&current, context).await
    }

    /// Pin an item.
    pub async fn pin(&self, item_id: Uuid) -> Result<()> {
        let mut items = self.items.write().await;
        if let Some(item) = items.iter_mut().find(|i| i.id == item_id) {
            item.pinned = true;
            let _ = self.event_tx.send(SyncEvent::ItemUpdated(item.clone()));
        }
        Ok(())
    }

    /// Delete an item.
    pub async fn delete(&self, item_id: Uuid) -> Result<()> {
        self.items.write().await.retain(|i| i.id != item_id);

        if self.config.sync_enabled {
            self.sync_provider.delete(item_id).await?;
        }

        let _ = self.event_tx.send(SyncEvent::ItemDeleted(item_id));
        Ok(())
    }

    /// Sync with cloud.
    pub async fn sync(&self) -> Result<usize> {
        if !self.config.sync_enabled {
            return Ok(0);
        }

        let last_sync = self
            .items
            .read()
            .await
            .first()
            .map(|i| i.created_at)
            .unwrap_or_else(|| Utc::now() - chrono::Duration::days(1));

        let remote_items = self.sync_provider.pull(last_sync).await?;
        let count = remote_items.len();

        for item in remote_items {
            if item.source_device != self.device_id {
                self.add_item(item).await?;
            }
        }

        Ok(count)
    }

    /// Subscribe to sync events.
    pub fn subscribe(&self) -> broadcast::Receiver<SyncEvent> {
        self.event_tx.subscribe()
    }

    /// Get connected devices.
    pub async fn devices(&self) -> Vec<DeviceInfo> {
        self.devices.read().await.values().cloned().collect()
    }

    /// Detect content type.
    fn detect_type(content: &str) -> ContentType {
        let trimmed = content.trim();

        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return ContentType::Url;
        }

        if trimmed.starts_with('{') && trimmed.ends_with('}') {
            if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
                return ContentType::Json;
            }
        }

        if trimmed.starts_with('<') && trimmed.ends_with('>') {
            return ContentType::Html;
        }

        if trimmed.contains("fn ") || trimmed.contains("def ") || trimmed.contains("function ") {
            return ContentType::Code;
        }

        ContentType::Text
    }

    /// Get statistics.
    pub async fn stats(&self) -> ClipStats {
        let items = self.items.read().await;
        let devices = self.devices.read().await;

        let pinned = items.iter().filter(|i| i.pinned).count();

        let mut by_type: HashMap<ContentType, usize> = HashMap::new();
        for item in items.iter() {
            *by_type.entry(item.content_type).or_insert(0) += 1;
        }

        ClipStats {
            total_items: items.len(),
            pinned_count: pinned,
            device_count: devices.len(),
            by_type,
        }
    }
}

/// Clipboard statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipStats {
    pub total_items: usize,
    pub pinned_count: usize,
    pub device_count: usize,
    pub by_type: HashMap<ContentType, usize>,
}

/// Simple sync provider for testing.
pub struct LocalSync {
    items: Arc<RwLock<Vec<ClipItem>>>,
}

impl LocalSync {
    pub fn new() -> Self {
        Self {
            items: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for LocalSync {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyncProvider for LocalSync {
    async fn push(&self, item: &ClipItem) -> Result<()> {
        self.items.write().await.push(item.clone());
        Ok(())
    }

    async fn pull(&self, since: DateTime<Utc>) -> Result<Vec<ClipItem>> {
        Ok(self
            .items
            .read()
            .await
            .iter()
            .filter(|i| i.created_at > since)
            .cloned()
            .collect())
    }

    async fn delete(&self, item_id: Uuid) -> Result<()> {
        self.items.write().await.retain(|i| i.id != item_id);
        Ok(())
    }
}

/// Simple paste transformer for testing.
pub struct SimplePasteTransformer;

#[async_trait]
impl PasteTransformer for SimplePasteTransformer {
    async fn suggest(&self, item: &ClipItem, context: &PasteContext) -> Vec<PasteSuggestion> {
        let mut suggestions = Vec::new();

        if let Some(text) = &item.text {
            // URL to markdown link
            if item.content_type == ContentType::Url {
                suggestions.push(PasteSuggestion {
                    id: Uuid::new_v4(),
                    item_id: item.id,
                    content: format!("[Link]({})", text),
                    transformation: "markdown_link".to_string(),
                    confidence: 0.8,
                    reason: "Convert URL to Markdown link".to_string(),
                });
            }

            // Code with language hint
            if item.content_type == ContentType::Code {
                let lang = item
                    .metadata
                    .get("language")
                    .map(|s| s.as_str())
                    .unwrap_or("text");
                suggestions.push(PasteSuggestion {
                    id: Uuid::new_v4(),
                    item_id: item.id,
                    content: format!("```{}\n{}\n```", lang, text),
                    transformation: "code_block".to_string(),
                    confidence: 0.9,
                    reason: "Wrap in code block".to_string(),
                });
            }

            // Plain text (always available)
            if !suggestions.iter().any(|s| s.content == *text) {
                suggestions.push(PasteSuggestion {
                    id: Uuid::new_v4(),
                    item_id: item.id,
                    content: text.clone(),
                    transformation: "plain".to_string(),
                    confidence: 1.0,
                    reason: "Paste as-is".to_string(),
                });
            }
        }

        let _ = context; // Suppress unused warning
        suggestions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_copy() {
        let manager = UniClipManager::new(
            UniClipConfig::default(),
            "test-device",
            LocalSync::new(),
            SimplePasteTransformer,
        );

        let item = manager.copy("Hello, World!").await.unwrap();
        assert_eq!(item.content_type, ContentType::Text);

        let current = manager.get_current().await.unwrap();
        assert_eq!(current.text, Some("Hello, World!".to_string()));
    }

    #[tokio::test]
    async fn test_url_detection() {
        let manager = UniClipManager::new(
            UniClipConfig::default(),
            "test-device",
            LocalSync::new(),
            SimplePasteTransformer,
        );

        let item = manager.copy("https://example.com").await.unwrap();
        assert_eq!(item.content_type, ContentType::Url);
    }

    #[tokio::test]
    async fn test_history() {
        let manager = UniClipManager::new(
            UniClipConfig::default(),
            "test-device",
            LocalSync::new(),
            SimplePasteTransformer,
        );

        manager.copy("First").await.unwrap();
        manager.copy("Second").await.unwrap();
        manager.copy("Third").await.unwrap();

        let history = manager.history(10).await;
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].text, Some("Third".to_string()));
    }

    #[tokio::test]
    async fn test_smart_paste() {
        let manager = UniClipManager::new(
            UniClipConfig::default(),
            "test-device",
            LocalSync::new(),
            SimplePasteTransformer,
        );

        manager.copy("https://example.com").await.unwrap();

        let suggestions = manager.smart_paste(&PasteContext::default()).await;
        assert!(suggestions
            .iter()
            .any(|s| s.transformation == "markdown_link"));
    }

    #[tokio::test]
    async fn test_search() {
        let manager = UniClipManager::new(
            UniClipConfig::default(),
            "test-device",
            LocalSync::new(),
            SimplePasteTransformer,
        );

        manager.copy("apple pie recipe").await.unwrap();
        manager.copy("orange juice").await.unwrap();
        manager.copy("apple cider").await.unwrap();

        let results = manager.search("apple").await;
        assert_eq!(results.len(), 2);
    }
}
