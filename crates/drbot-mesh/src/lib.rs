//! Multi-device mesh networking for drbot
//!
//! Device discovery, session handoff, and collaborative features.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Error, Debug)]
pub enum MeshError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Handoff failed: {0}")]
    HandoffFailed(String),
    #[error("Sync failed: {0}")]
    SyncFailed(String),
    #[error("Not authorized: {0}")]
    NotAuthorized(String),
    #[error("Provider error: {0}")]
    ProviderError(String),
}

pub type Result<T> = std::result::Result<T, MeshError>;

// ============================================================================
// Device Management
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub platform: Platform,
    pub capabilities: Vec<DeviceCapability>,
    pub status: DeviceStatus,
    pub last_seen: u64,
    pub ip_address: Option<String>,
    pub owner_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceType {
    Desktop,
    Laptop,
    Phone,
    Tablet,
    Watch,
    Speaker,
    TV,
    Server,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Platform {
    MacOS,
    iOS,
    Windows,
    Linux,
    Android,
    Web,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceCapability {
    Display,
    Audio,
    Microphone,
    Camera,
    Keyboard,
    Touch,
    Notifications,
    BackgroundProcessing,
    LocalInference,
    FileAccess,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum DeviceStatus {
    Online,
    Away,
    Busy,
    Offline,
    Unreachable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRegistration {
    pub name: String,
    pub device_type: DeviceType,
    pub platform: Platform,
    pub capabilities: Vec<DeviceCapability>,
    pub push_token: Option<String>,
}

// ============================================================================
// Session Handoff
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: String,
    pub active_device_id: String,
    pub context: SessionContext,
    pub created_at: u64,
    pub updated_at: u64,
    pub history: Vec<SessionHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContext {
    pub messages: Vec<Message>,
    pub current_task: Option<String>,
    pub workspace: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistoryEntry {
    pub device_id: String,
    pub device_name: String,
    pub started_at: u64,
    pub ended_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffRequest {
    pub session_id: String,
    pub from_device_id: String,
    pub to_device_id: String,
    pub preserve_context: bool,
    pub notify_user: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffResult {
    pub success: bool,
    pub session_id: String,
    pub new_device_id: String,
    pub context_transferred: bool,
    pub error: Option<String>,
}

// ============================================================================
// Collaboration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborativeSession {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub participants: Vec<Participant>,
    pub shared_context: SharedContext,
    pub permissions: SessionPermissions,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub user_id: String,
    pub device_id: String,
    pub role: ParticipantRole,
    pub joined_at: u64,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ParticipantRole {
    Owner,
    Editor,
    Viewer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedContext {
    pub messages: Vec<Message>,
    pub documents: Vec<SharedDocument>,
    pub cursor_positions: HashMap<String, CursorPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedDocument {
    pub id: String,
    pub name: String,
    pub content: String,
    pub last_modified_by: String,
    pub last_modified_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPosition {
    pub document_id: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPermissions {
    pub can_edit: bool,
    pub can_invite: bool,
    pub can_remove_participants: bool,
    pub can_share_documents: bool,
}

// ============================================================================
// Synchronization
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub device_id: String,
    pub last_sync: u64,
    pub sync_version: u64,
    pub pending_changes: Vec<Change>,
    pub conflicts: Vec<Conflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub id: String,
    pub change_type: ChangeType,
    pub entity_type: String,
    pub entity_id: String,
    pub data: serde_json::Value,
    pub timestamp: u64,
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeType {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub local_change: Change,
    pub remote_change: Change,
    pub resolution: Option<ConflictResolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConflictResolution {
    UseLocal,
    UseRemote,
    Merge,
    Manual,
}

// ============================================================================
// Provider Trait
// ============================================================================

#[async_trait]
pub trait MeshProvider: Send + Sync {
    async fn discover_devices(&self) -> Result<Vec<Device>>;
    async fn connect_device(&self, device_id: &str) -> Result<()>;
    async fn disconnect_device(&self, device_id: &str) -> Result<()>;
    async fn send_message(&self, device_id: &str, message: &[u8]) -> Result<()>;
    async fn handoff_session(&self, request: HandoffRequest) -> Result<HandoffResult>;
    async fn sync_state(&self, state: &SyncState) -> Result<Vec<Change>>;
}

// ============================================================================
// Mesh Engine
// ============================================================================

pub struct MeshEngine {
    provider: Arc<dyn MeshProvider>,
    local_device_id: String,
    devices: Arc<RwLock<HashMap<String, Device>>>,
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    collaborative_sessions: Arc<RwLock<HashMap<String, CollaborativeSession>>>,
    sync_states: Arc<RwLock<HashMap<String, SyncState>>>,
}

impl MeshEngine {
    pub fn new(provider: Arc<dyn MeshProvider>, local_device_id: String) -> Self {
        Self {
            provider,
            local_device_id,
            devices: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            collaborative_sessions: Arc::new(RwLock::new(HashMap::new())),
            sync_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    pub fn local_device_id(&self) -> &str {
        &self.local_device_id
    }

    // Device Discovery
    pub async fn discover_devices(&self) -> Result<Vec<Device>> {
        let devices = self.provider.discover_devices().await?;

        let mut cache = self.devices.write().await;
        for device in &devices {
            cache.insert(device.id.clone(), device.clone());
        }

        Ok(devices)
    }

    pub async fn get_device(&self, device_id: &str) -> Result<Device> {
        let devices = self.devices.read().await;
        devices
            .get(device_id)
            .cloned()
            .ok_or_else(|| MeshError::DeviceNotFound(device_id.to_string()))
    }

    pub async fn get_online_devices(&self) -> Vec<Device> {
        let devices = self.devices.read().await;
        devices
            .values()
            .filter(|d| d.status == DeviceStatus::Online)
            .cloned()
            .collect()
    }

    pub async fn get_devices_by_capability(&self, capability: DeviceCapability) -> Vec<Device> {
        let devices = self.devices.read().await;
        devices
            .values()
            .filter(|d| d.capabilities.contains(&capability))
            .cloned()
            .collect()
    }

    pub async fn connect_to_device(&self, device_id: &str) -> Result<()> {
        self.provider.connect_device(device_id).await?;

        let mut devices = self.devices.write().await;
        if let Some(device) = devices.get_mut(device_id) {
            device.status = DeviceStatus::Online;
            device.last_seen = Self::now();
        }

        Ok(())
    }

    pub async fn disconnect_from_device(&self, device_id: &str) -> Result<()> {
        self.provider.disconnect_device(device_id).await?;

        let mut devices = self.devices.write().await;
        if let Some(device) = devices.get_mut(device_id) {
            device.status = DeviceStatus::Offline;
        }

        Ok(())
    }

    // Session Management
    pub async fn create_session(&self, user_id: &str) -> Result<Session> {
        let session = Session {
            id: format!("session-{}", Self::now()),
            user_id: user_id.to_string(),
            active_device_id: self.local_device_id.clone(),
            context: SessionContext {
                messages: vec![],
                current_task: None,
                workspace: None,
                metadata: HashMap::new(),
            },
            created_at: Self::now(),
            updated_at: Self::now(),
            history: vec![SessionHistoryEntry {
                device_id: self.local_device_id.clone(),
                device_name: "Local".to_string(),
                started_at: Self::now(),
                ended_at: None,
            }],
        };

        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());

        Ok(session)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Session> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| MeshError::DeviceNotFound(format!("Session {}", session_id)))
    }

    pub async fn add_message_to_session(
        &self,
        session_id: &str,
        role: MessageRole,
        content: &str,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| MeshError::DeviceNotFound(format!("Session {}", session_id)))?;

        session.context.messages.push(Message {
            id: format!("msg-{}", Self::now()),
            role,
            content: content.to_string(),
            timestamp: Self::now(),
        });
        session.updated_at = Self::now();

        Ok(())
    }

    // Session Handoff
    pub async fn handoff_to_device(
        &self,
        session_id: &str,
        target_device_id: &str,
    ) -> Result<HandoffResult> {
        let session = self.get_session(session_id).await?;

        let request = HandoffRequest {
            session_id: session_id.to_string(),
            from_device_id: session.active_device_id.clone(),
            to_device_id: target_device_id.to_string(),
            preserve_context: true,
            notify_user: true,
        };

        let result = self.provider.handoff_session(request).await?;

        if result.success {
            let mut sessions = self.sessions.write().await;
            if let Some(s) = sessions.get_mut(session_id) {
                // Update history
                if let Some(last) = s.history.last_mut() {
                    last.ended_at = Some(Self::now());
                }

                let device = self
                    .get_device(target_device_id)
                    .await
                    .map(|d| d.name)
                    .unwrap_or_else(|_| "Unknown".to_string());

                s.history.push(SessionHistoryEntry {
                    device_id: target_device_id.to_string(),
                    device_name: device,
                    started_at: Self::now(),
                    ended_at: None,
                });

                s.active_device_id = target_device_id.to_string();
                s.updated_at = Self::now();
            }
        }

        Ok(result)
    }

    pub async fn request_handoff_here(&self, session_id: &str) -> Result<HandoffResult> {
        self.handoff_to_device(session_id, &self.local_device_id.clone())
            .await
    }

    pub async fn find_best_handoff_target(&self, session_id: &str) -> Result<Option<Device>> {
        let session = self.get_session(session_id).await?;
        let online = self.get_online_devices().await;

        // Find best device that's not the current one
        let best = online
            .into_iter()
            .filter(|d| d.id != session.active_device_id)
            .max_by_key(|d| {
                let mut score = 0;
                if d.capabilities.contains(&DeviceCapability::Display) {
                    score += 10;
                }
                if d.capabilities.contains(&DeviceCapability::Keyboard) {
                    score += 5;
                }
                if d.capabilities.contains(&DeviceCapability::LocalInference) {
                    score += 3;
                }
                score
            });

        Ok(best)
    }

    // Collaboration
    pub async fn create_collaborative_session(
        &self,
        name: &str,
        owner_id: &str,
    ) -> Result<CollaborativeSession> {
        let session = CollaborativeSession {
            id: format!("collab-{}", Self::now()),
            name: name.to_string(),
            owner_id: owner_id.to_string(),
            participants: vec![Participant {
                user_id: owner_id.to_string(),
                device_id: self.local_device_id.clone(),
                role: ParticipantRole::Owner,
                joined_at: Self::now(),
                active: true,
            }],
            shared_context: SharedContext {
                messages: vec![],
                documents: vec![],
                cursor_positions: HashMap::new(),
            },
            permissions: SessionPermissions {
                can_edit: true,
                can_invite: true,
                can_remove_participants: true,
                can_share_documents: true,
            },
            created_at: Self::now(),
        };

        let mut sessions = self.collaborative_sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());

        Ok(session)
    }

    pub async fn join_collaborative_session(
        &self,
        session_id: &str,
        user_id: &str,
        role: ParticipantRole,
    ) -> Result<()> {
        let mut sessions = self.collaborative_sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| MeshError::DeviceNotFound(format!("Session {}", session_id)))?;

        // Check if already joined
        if session.participants.iter().any(|p| p.user_id == user_id) {
            return Ok(());
        }

        session.participants.push(Participant {
            user_id: user_id.to_string(),
            device_id: self.local_device_id.clone(),
            role,
            joined_at: Self::now(),
            active: true,
        });

        Ok(())
    }

    pub async fn leave_collaborative_session(&self, session_id: &str, user_id: &str) -> Result<()> {
        let mut sessions = self.collaborative_sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| MeshError::DeviceNotFound(format!("Session {}", session_id)))?;

        session.participants.retain(|p| p.user_id != user_id);

        Ok(())
    }

    pub async fn share_document(
        &self,
        session_id: &str,
        name: &str,
        content: &str,
    ) -> Result<SharedDocument> {
        let mut sessions = self.collaborative_sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| MeshError::DeviceNotFound(format!("Session {}", session_id)))?;

        let doc = SharedDocument {
            id: format!("doc-{}", Self::now()),
            name: name.to_string(),
            content: content.to_string(),
            last_modified_by: self.local_device_id.clone(),
            last_modified_at: Self::now(),
        };

        session.shared_context.documents.push(doc.clone());

        Ok(doc)
    }

    // Synchronization
    pub async fn initialize_sync(&self) -> Result<SyncState> {
        let state = SyncState {
            device_id: self.local_device_id.clone(),
            last_sync: Self::now(),
            sync_version: 0,
            pending_changes: vec![],
            conflicts: vec![],
        };

        let mut states = self.sync_states.write().await;
        states.insert(self.local_device_id.clone(), state.clone());

        Ok(state)
    }

    pub async fn record_change(&self, change: Change) -> Result<()> {
        let mut states = self.sync_states.write().await;
        let state = states
            .get_mut(&self.local_device_id)
            .ok_or_else(|| MeshError::SyncFailed("Sync not initialized".to_string()))?;

        state.pending_changes.push(change);

        Ok(())
    }

    pub async fn sync(&self) -> Result<Vec<Change>> {
        let state = {
            let states = self.sync_states.read().await;
            states
                .get(&self.local_device_id)
                .cloned()
                .ok_or_else(|| MeshError::SyncFailed("Sync not initialized".to_string()))?
        };

        let remote_changes = self.provider.sync_state(&state).await?;

        // Update sync state
        {
            let mut states = self.sync_states.write().await;
            if let Some(s) = states.get_mut(&self.local_device_id) {
                s.last_sync = Self::now();
                s.sync_version += 1;
                s.pending_changes.clear();
            }
        }

        Ok(remote_changes)
    }

    pub async fn get_pending_changes(&self) -> Vec<Change> {
        let states = self.sync_states.read().await;
        states
            .get(&self.local_device_id)
            .map(|s| s.pending_changes.clone())
            .unwrap_or_default()
    }

    pub async fn resolve_conflict(
        &self,
        conflict_id: &str,
        resolution: ConflictResolution,
    ) -> Result<()> {
        let mut states = self.sync_states.write().await;
        let state = states
            .get_mut(&self.local_device_id)
            .ok_or_else(|| MeshError::SyncFailed("Sync not initialized".to_string()))?;

        if let Some(conflict) = state.conflicts.iter_mut().find(|c| c.id == conflict_id) {
            conflict.resolution = Some(resolution);
        }

        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        devices: Vec<Device>,
    }

    impl MockProvider {
        fn new() -> Self {
            Self {
                devices: vec![
                    Device {
                        id: "device-1".to_string(),
                        name: "MacBook".to_string(),
                        device_type: DeviceType::Laptop,
                        platform: Platform::MacOS,
                        capabilities: vec![
                            DeviceCapability::Display,
                            DeviceCapability::Keyboard,
                            DeviceCapability::LocalInference,
                        ],
                        status: DeviceStatus::Online,
                        last_seen: 0,
                        ip_address: Some("192.168.1.10".to_string()),
                        owner_id: "user-1".to_string(),
                    },
                    Device {
                        id: "device-2".to_string(),
                        name: "iPhone".to_string(),
                        device_type: DeviceType::Phone,
                        platform: Platform::iOS,
                        capabilities: vec![
                            DeviceCapability::Display,
                            DeviceCapability::Touch,
                            DeviceCapability::Notifications,
                        ],
                        status: DeviceStatus::Online,
                        last_seen: 0,
                        ip_address: Some("192.168.1.11".to_string()),
                        owner_id: "user-1".to_string(),
                    },
                ],
            }
        }
    }

    #[async_trait]
    impl MeshProvider for MockProvider {
        async fn discover_devices(&self) -> Result<Vec<Device>> {
            Ok(self.devices.clone())
        }

        async fn connect_device(&self, _device_id: &str) -> Result<()> {
            Ok(())
        }

        async fn disconnect_device(&self, _device_id: &str) -> Result<()> {
            Ok(())
        }

        async fn send_message(&self, _device_id: &str, _message: &[u8]) -> Result<()> {
            Ok(())
        }

        async fn handoff_session(&self, request: HandoffRequest) -> Result<HandoffResult> {
            Ok(HandoffResult {
                success: true,
                session_id: request.session_id,
                new_device_id: request.to_device_id,
                context_transferred: request.preserve_context,
                error: None,
            })
        }

        async fn sync_state(&self, _state: &SyncState) -> Result<Vec<Change>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_device_discovery() {
        let provider = Arc::new(MockProvider::new());
        let engine = MeshEngine::new(provider, "local".to_string());

        let devices = engine.discover_devices().await.unwrap();
        assert_eq!(devices.len(), 2);
    }

    #[tokio::test]
    async fn test_online_devices() {
        let provider = Arc::new(MockProvider::new());
        let engine = MeshEngine::new(provider, "local".to_string());

        engine.discover_devices().await.unwrap();
        let online = engine.get_online_devices().await;
        assert_eq!(online.len(), 2);
    }

    #[tokio::test]
    async fn test_devices_by_capability() {
        let provider = Arc::new(MockProvider::new());
        let engine = MeshEngine::new(provider, "local".to_string());

        engine.discover_devices().await.unwrap();
        let with_keyboard = engine
            .get_devices_by_capability(DeviceCapability::Keyboard)
            .await;
        assert_eq!(with_keyboard.len(), 1);
        assert_eq!(with_keyboard[0].name, "MacBook");
    }

    #[tokio::test]
    async fn test_session_creation() {
        let provider = Arc::new(MockProvider::new());
        let engine = MeshEngine::new(provider, "local".to_string());

        let session = engine.create_session("user-1").await.unwrap();
        assert_eq!(session.user_id, "user-1");
        assert_eq!(session.active_device_id, "local");
    }

    #[tokio::test]
    async fn test_session_messages() {
        let provider = Arc::new(MockProvider::new());
        let engine = MeshEngine::new(provider, "local".to_string());

        let session = engine.create_session("user-1").await.unwrap();

        engine
            .add_message_to_session(&session.id, MessageRole::User, "Hello")
            .await
            .unwrap();
        engine
            .add_message_to_session(&session.id, MessageRole::Assistant, "Hi!")
            .await
            .unwrap();

        let updated = engine.get_session(&session.id).await.unwrap();
        assert_eq!(updated.context.messages.len(), 2);
    }

    #[tokio::test]
    async fn test_session_handoff() {
        let provider = Arc::new(MockProvider::new());
        let engine = MeshEngine::new(provider, "local".to_string());

        engine.discover_devices().await.unwrap();
        let session = engine.create_session("user-1").await.unwrap();

        let result = engine
            .handoff_to_device(&session.id, "device-2")
            .await
            .unwrap();
        assert!(result.success);

        let updated = engine.get_session(&session.id).await.unwrap();
        assert_eq!(updated.active_device_id, "device-2");
        assert_eq!(updated.history.len(), 2);
    }

    #[tokio::test]
    async fn test_collaborative_session() {
        let provider = Arc::new(MockProvider::new());
        let engine = MeshEngine::new(provider, "local".to_string());

        let session = engine
            .create_collaborative_session("Team Chat", "user-1")
            .await
            .unwrap();
        assert_eq!(session.participants.len(), 1);

        engine
            .join_collaborative_session(&session.id, "user-2", ParticipantRole::Editor)
            .await
            .unwrap();

        let mut sessions = engine.collaborative_sessions.write().await;
        let updated = sessions.get(&session.id).unwrap();
        assert_eq!(updated.participants.len(), 2);
    }

    #[tokio::test]
    async fn test_document_sharing() {
        let provider = Arc::new(MockProvider::new());
        let engine = MeshEngine::new(provider, "local".to_string());

        let session = engine
            .create_collaborative_session("Work", "user-1")
            .await
            .unwrap();

        let doc = engine
            .share_document(&session.id, "notes.txt", "Hello world")
            .await
            .unwrap();
        assert_eq!(doc.name, "notes.txt");
    }

    #[tokio::test]
    async fn test_sync() {
        let provider = Arc::new(MockProvider::new());
        let engine = MeshEngine::new(provider, "local".to_string());

        engine.initialize_sync().await.unwrap();

        engine
            .record_change(Change {
                id: "change-1".to_string(),
                change_type: ChangeType::Create,
                entity_type: "session".to_string(),
                entity_id: "session-1".to_string(),
                data: serde_json::json!({}),
                timestamp: 0,
                device_id: "local".to_string(),
            })
            .await
            .unwrap();

        let pending = engine.get_pending_changes().await;
        assert_eq!(pending.len(), 1);

        let remote = engine.sync().await.unwrap();
        assert!(remote.is_empty());

        let pending_after = engine.get_pending_changes().await;
        assert!(pending_after.is_empty());
    }
}
