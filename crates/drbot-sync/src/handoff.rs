//! Cross-device handoff for seamless conversations.
//!
//! Enables continuing conversations across devices.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Device information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    /// Device ID.
    pub id: Uuid,
    /// Device name.
    pub name: String,
    /// Device type.
    pub device_type: DeviceType,
    /// Platform.
    pub platform: Platform,
    /// Last seen timestamp.
    pub last_seen: DateTime<Utc>,
    /// Is currently active.
    pub is_active: bool,
    /// Is this device.
    pub is_current: bool,
    /// Capabilities.
    pub capabilities: DeviceCapabilities,
}

/// Device types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Desktop,
    Laptop,
    Phone,
    Tablet,
    Watch,
    Other,
}

/// Platform types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    MacOS,
    Windows,
    Linux,
    IOS,
    Android,
    Web,
    Other,
}

/// Device capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    /// Supports voice input.
    pub voice_input: bool,
    /// Supports voice output.
    pub voice_output: bool,
    /// Supports screen context.
    pub screen_context: bool,
    /// Supports notifications.
    pub notifications: bool,
    /// Supports background sync.
    pub background_sync: bool,
}

impl Default for DeviceCapabilities {
    fn default() -> Self {
        Self {
            voice_input: true,
            voice_output: true,
            screen_context: true,
            notifications: true,
            background_sync: true,
        }
    }
}

/// Active conversation that can be handed off.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveConversation {
    /// Conversation ID.
    pub id: Uuid,
    /// Session ID.
    pub session_id: String,
    /// Current device ID.
    pub current_device_id: Uuid,
    /// Preview of conversation.
    pub preview: String,
    /// Message count.
    pub message_count: u32,
    /// Last activity.
    pub last_activity: DateTime<Utc>,
    /// Conversation state snapshot.
    pub state: ConversationSnapshot,
}

/// Snapshot of conversation state for handoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSnapshot {
    /// Recent messages (for quick context).
    pub recent_messages: Vec<HandoffMessage>,
    /// Pending action if any.
    pub pending_action: Option<String>,
    /// Variables/context.
    pub context: HashMap<String, String>,
    /// Active workflows.
    pub active_workflows: Vec<String>,
}

/// Message for handoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffMessage {
    /// Role.
    pub role: String,
    /// Content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Handoff request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffRequest {
    /// Request ID.
    pub id: Uuid,
    /// Conversation ID.
    pub conversation_id: Uuid,
    /// Source device ID.
    pub source_device_id: Uuid,
    /// Target device ID.
    pub target_device_id: Uuid,
    /// Request timestamp.
    pub requested_at: DateTime<Utc>,
    /// Expiry time.
    pub expires_at: DateTime<Utc>,
}

/// Handoff event.
#[derive(Debug, Clone)]
pub enum HandoffEvent {
    /// Device came online.
    DeviceOnline { device: Device },
    /// Device went offline.
    DeviceOffline { device_id: Uuid },
    /// Handoff requested.
    HandoffRequested { request: HandoffRequest },
    /// Handoff accepted.
    HandoffAccepted { request_id: Uuid },
    /// Handoff rejected.
    HandoffRejected { request_id: Uuid },
    /// Handoff completed.
    HandoffCompleted {
        conversation_id: Uuid,
        from_device: Uuid,
        to_device: Uuid,
    },
    /// Conversation updated on another device.
    ConversationUpdated { conversation: ActiveConversation },
}

/// Handoff manager.
pub struct HandoffManager {
    current_device: Device,
    devices: Arc<RwLock<HashMap<Uuid, Device>>>,
    active_conversations: Arc<RwLock<HashMap<Uuid, ActiveConversation>>>,
    pending_handoffs: Arc<RwLock<HashMap<Uuid, HandoffRequest>>>,
    event_sender: broadcast::Sender<HandoffEvent>,
}

impl HandoffManager {
    /// Create a new handoff manager.
    pub fn new(current_device: Device) -> Self {
        let (sender, _) = broadcast::channel(64);

        let mut devices = HashMap::new();
        devices.insert(current_device.id, current_device.clone());

        Self {
            current_device,
            devices: Arc::new(RwLock::new(devices)),
            active_conversations: Arc::new(RwLock::new(HashMap::new())),
            pending_handoffs: Arc::new(RwLock::new(HashMap::new())),
            event_sender: sender,
        }
    }

    /// Get current device.
    pub fn current_device(&self) -> &Device {
        &self.current_device
    }

    /// Subscribe to handoff events.
    pub fn subscribe(&self) -> broadcast::Receiver<HandoffEvent> {
        self.event_sender.subscribe()
    }

    /// Register a device.
    pub async fn register_device(&self, device: Device) {
        let mut devices = self.devices.write().await;
        devices.insert(device.id, device.clone());

        let _ = self
            .event_sender
            .send(HandoffEvent::DeviceOnline { device });
    }

    /// Unregister a device.
    pub async fn unregister_device(&self, device_id: Uuid) {
        let mut devices = self.devices.write().await;
        devices.remove(&device_id);

        let _ = self
            .event_sender
            .send(HandoffEvent::DeviceOffline { device_id });
    }

    /// Update device status.
    pub async fn update_device_status(&self, device_id: Uuid, is_active: bool) {
        let mut devices = self.devices.write().await;
        if let Some(device) = devices.get_mut(&device_id) {
            device.is_active = is_active;
            device.last_seen = Utc::now();
        }
    }

    /// Get all devices.
    pub async fn devices(&self) -> Vec<Device> {
        self.devices.read().await.values().cloned().collect()
    }

    /// Get online devices.
    pub async fn online_devices(&self) -> Vec<Device> {
        let devices = self.devices.read().await;
        let cutoff = Utc::now() - chrono::Duration::minutes(5);

        devices
            .values()
            .filter(|d| d.last_seen > cutoff)
            .cloned()
            .collect()
    }

    /// Register an active conversation.
    pub async fn register_conversation(&self, conversation: ActiveConversation) {
        let mut conversations = self.active_conversations.write().await;
        conversations.insert(conversation.id, conversation.clone());

        let _ = self
            .event_sender
            .send(HandoffEvent::ConversationUpdated { conversation });
    }

    /// Get active conversations.
    pub async fn active_conversations(&self) -> Vec<ActiveConversation> {
        self.active_conversations
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// Request handoff to another device.
    pub async fn request_handoff(
        &self,
        conversation_id: Uuid,
        target_device_id: Uuid,
    ) -> crate::Result<HandoffRequest> {
        // Check if conversation exists
        let conversations = self.active_conversations.read().await;
        if !conversations.contains_key(&conversation_id) {
            return Err(crate::SyncError::Conflict(
                "Conversation not found".to_string(),
            ));
        }

        // Check if target device exists
        let devices = self.devices.read().await;
        if !devices.contains_key(&target_device_id) {
            return Err(crate::SyncError::DeviceNotLinked);
        }

        let request = HandoffRequest {
            id: Uuid::new_v4(),
            conversation_id,
            source_device_id: self.current_device.id,
            target_device_id,
            requested_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
        };

        let mut pending = self.pending_handoffs.write().await;
        pending.insert(request.id, request.clone());

        let _ = self.event_sender.send(HandoffEvent::HandoffRequested {
            request: request.clone(),
        });

        Ok(request)
    }

    /// Accept a handoff request.
    pub async fn accept_handoff(&self, request_id: Uuid) -> crate::Result<ActiveConversation> {
        let mut pending = self.pending_handoffs.write().await;
        let request = pending
            .remove(&request_id)
            .ok_or_else(|| crate::SyncError::Conflict("Request not found".to_string()))?;

        // Check not expired
        if request.expires_at < Utc::now() {
            return Err(crate::SyncError::Conflict("Request expired".to_string()));
        }

        // Get conversation
        let mut conversations = self.active_conversations.write().await;
        let mut conversation = conversations
            .get(&request.conversation_id)
            .cloned()
            .ok_or_else(|| crate::SyncError::Conflict("Conversation not found".to_string()))?;

        // Update conversation ownership
        conversation.current_device_id = request.target_device_id;
        conversation.last_activity = Utc::now();
        conversations.insert(conversation.id, conversation.clone());

        let _ = self
            .event_sender
            .send(HandoffEvent::HandoffAccepted { request_id });
        let _ = self.event_sender.send(HandoffEvent::HandoffCompleted {
            conversation_id: request.conversation_id,
            from_device: request.source_device_id,
            to_device: request.target_device_id,
        });

        Ok(conversation)
    }

    /// Reject a handoff request.
    pub async fn reject_handoff(&self, request_id: Uuid) -> bool {
        let mut pending = self.pending_handoffs.write().await;
        if pending.remove(&request_id).is_some() {
            let _ = self
                .event_sender
                .send(HandoffEvent::HandoffRejected { request_id });
            true
        } else {
            false
        }
    }

    /// Get pending handoff requests for this device.
    pub async fn pending_requests(&self) -> Vec<HandoffRequest> {
        let pending = self.pending_handoffs.read().await;
        let now = Utc::now();

        pending
            .values()
            .filter(|r| r.target_device_id == self.current_device.id && r.expires_at > now)
            .cloned()
            .collect()
    }

    /// Clean up expired requests.
    pub async fn cleanup_expired(&self) {
        let mut pending = self.pending_handoffs.write().await;
        let now = Utc::now();

        pending.retain(|_, r| r.expires_at > now);
    }

    /// Push conversation update to other devices.
    pub async fn push_update(&self, conversation: ActiveConversation) {
        // Update local state
        let mut conversations = self.active_conversations.write().await;
        conversations.insert(conversation.id, conversation.clone());

        // Notify listeners
        let _ = self
            .event_sender
            .send(HandoffEvent::ConversationUpdated { conversation });

        // In a real implementation, this would push to a sync server
    }
}

/// Create current device info.
pub fn create_current_device(name: &str) -> Device {
    Device {
        id: Uuid::new_v4(),
        name: name.to_string(),
        device_type: detect_device_type(),
        platform: detect_platform(),
        last_seen: Utc::now(),
        is_active: true,
        is_current: true,
        capabilities: DeviceCapabilities::default(),
    }
}

fn detect_device_type() -> DeviceType {
    #[cfg(target_os = "macos")]
    {
        DeviceType::Laptop // Could be desktop, but laptop is more common
    }
    #[cfg(target_os = "ios")]
    {
        DeviceType::Phone // Could be tablet
    }
    #[cfg(target_os = "android")]
    {
        DeviceType::Phone
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
    {
        DeviceType::Desktop
    }
}

fn detect_platform() -> Platform {
    #[cfg(target_os = "macos")]
    {
        Platform::MacOS
    }
    #[cfg(target_os = "windows")]
    {
        Platform::Windows
    }
    #[cfg(target_os = "linux")]
    {
        Platform::Linux
    }
    #[cfg(target_os = "ios")]
    {
        Platform::IOS
    }
    #[cfg(target_os = "android")]
    {
        Platform::Android
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux",
        target_os = "ios",
        target_os = "android"
    )))]
    {
        Platform::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handoff_manager() {
        let device = create_current_device("Test Device");
        let manager = HandoffManager::new(device.clone());

        // Register another device
        let other_device = Device {
            id: Uuid::new_v4(),
            name: "Other Device".to_string(),
            device_type: DeviceType::Phone,
            platform: Platform::IOS,
            last_seen: Utc::now(),
            is_active: true,
            is_current: false,
            capabilities: DeviceCapabilities::default(),
        };
        manager.register_device(other_device.clone()).await;

        let devices = manager.devices().await;
        assert_eq!(devices.len(), 2);

        // Create a conversation
        let conversation = ActiveConversation {
            id: Uuid::new_v4(),
            session_id: "session1".to_string(),
            current_device_id: device.id,
            preview: "Hello world".to_string(),
            message_count: 5,
            last_activity: Utc::now(),
            state: ConversationSnapshot {
                recent_messages: Vec::new(),
                pending_action: None,
                context: HashMap::new(),
                active_workflows: Vec::new(),
            },
        };
        manager.register_conversation(conversation.clone()).await;

        // Request handoff
        let request = manager
            .request_handoff(conversation.id, other_device.id)
            .await
            .unwrap();

        let pending = manager.pending_requests().await;
        // Pending is from perspective of target device, which isn't us
        assert_eq!(pending.len(), 0);
    }

    #[test]
    fn test_device_creation() {
        let device = create_current_device("My Mac");

        assert_eq!(device.name, "My Mac");
        assert!(device.is_current);
        assert!(device.is_active);
    }
}
