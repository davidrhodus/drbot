//! Real-time collaboration for drbot.
//!
//! Multi-user sessions with shared context.
//!
//! # Features
//!
//! - Multi-user sessions
//! - Presence tracking
//! - Shared context
//! - Collaborative editing
//! - Real-time sync

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Realtime result type.
pub type Result<T> = std::result::Result<T, RealtimeError>;

/// Realtime errors.
#[derive(Debug, thiserror::Error)]
pub enum RealtimeError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("User not in session: {0}")]
    UserNotInSession(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Session full")]
    SessionFull,
    #[error("Sync failed: {0}")]
    SyncFailed(String),
}

/// Collaborative session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborativeSession {
    /// Session ID.
    pub id: Uuid,
    /// Session name.
    pub name: String,
    /// Session owner.
    pub owner_id: String,
    /// Participants.
    pub participants: Vec<Participant>,
    /// Shared context.
    pub shared_context: SharedContext,
    /// Session settings.
    pub settings: SessionSettings,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Last activity.
    pub last_activity: DateTime<Utc>,
}

impl CollaborativeSession {
    /// Create a new session.
    pub fn new(name: &str, owner_id: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            owner_id: owner_id.to_string(),
            participants: vec![Participant::new(owner_id, Role::Owner)],
            shared_context: SharedContext::default(),
            settings: SessionSettings::default(),
            created_at: now,
            last_activity: now,
        }
    }

    /// Add a participant.
    pub fn add_participant(&mut self, user_id: &str, role: Role) -> Result<()> {
        if self.participants.len() >= self.settings.max_participants {
            return Err(RealtimeError::SessionFull);
        }

        if !self.participants.iter().any(|p| p.user_id == user_id) {
            self.participants.push(Participant::new(user_id, role));
        }
        self.last_activity = Utc::now();
        Ok(())
    }

    /// Remove a participant.
    pub fn remove_participant(&mut self, user_id: &str) {
        self.participants.retain(|p| p.user_id != user_id);
        self.last_activity = Utc::now();
    }

    /// Check if user is participant.
    pub fn is_participant(&self, user_id: &str) -> bool {
        self.participants.iter().any(|p| p.user_id == user_id)
    }

    /// Get participant by ID.
    pub fn get_participant(&self, user_id: &str) -> Option<&Participant> {
        self.participants.iter().find(|p| p.user_id == user_id)
    }
}

/// Session participant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    /// User ID.
    pub user_id: String,
    /// Display name.
    pub display_name: Option<String>,
    /// Role.
    pub role: Role,
    /// Presence status.
    pub presence: Presence,
    /// Current cursor position.
    pub cursor: Option<CursorPosition>,
    /// Joined at.
    pub joined_at: DateTime<Utc>,
    /// Last seen.
    pub last_seen: DateTime<Utc>,
}

impl Participant {
    /// Create a new participant.
    pub fn new(user_id: &str, role: Role) -> Self {
        let now = Utc::now();
        Self {
            user_id: user_id.to_string(),
            display_name: None,
            role,
            presence: Presence::Online,
            cursor: None,
            joined_at: now,
            last_seen: now,
        }
    }

    /// Update presence.
    pub fn update_presence(&mut self, presence: Presence) {
        self.presence = presence;
        self.last_seen = Utc::now();
    }
}

/// Participant role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Admin,
    Editor,
    Viewer,
}

impl Role {
    /// Check if role can edit.
    pub fn can_edit(&self) -> bool {
        matches!(self, Role::Owner | Role::Admin | Role::Editor)
    }

    /// Check if role can manage.
    pub fn can_manage(&self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }
}

/// Presence status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Presence {
    Online,
    Away,
    Busy,
    Offline,
}

/// Cursor position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorPosition {
    /// Message index.
    pub message_index: usize,
    /// Character offset.
    pub offset: usize,
    /// Selection end (if selecting).
    pub selection_end: Option<usize>,
}

/// Shared context across session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharedContext {
    /// Shared messages.
    pub messages: Vec<SharedMessage>,
    /// Shared documents.
    pub documents: Vec<SharedDocument>,
    /// Shared variables.
    pub variables: HashMap<String, serde_json::Value>,
    /// Annotations.
    pub annotations: Vec<Annotation>,
}

/// Shared message in session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedMessage {
    /// Message ID.
    pub id: Uuid,
    /// Author.
    pub author_id: String,
    /// Content.
    pub content: String,
    /// Message type.
    pub message_type: MessageType,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Edits.
    pub edits: Vec<MessageEdit>,
}

/// Message type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    User,
    Assistant,
    System,
    Annotation,
}

/// Message edit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEdit {
    /// Editor ID.
    pub editor_id: String,
    /// Previous content.
    pub previous: String,
    /// New content.
    pub new: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Shared document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedDocument {
    /// Document ID.
    pub id: Uuid,
    /// Document name.
    pub name: String,
    /// Content.
    pub content: String,
    /// MIME type.
    pub mime_type: String,
    /// Uploaded by.
    pub uploaded_by: String,
    /// Uploaded at.
    pub uploaded_at: DateTime<Utc>,
}

/// Annotation on content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    /// Annotation ID.
    pub id: Uuid,
    /// Author.
    pub author_id: String,
    /// Target message ID.
    pub target_id: Uuid,
    /// Content.
    pub content: String,
    /// Annotation type.
    pub annotation_type: AnnotationType,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// Annotation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationType {
    Comment,
    Highlight,
    Question,
    Suggestion,
}

/// Session settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSettings {
    /// Maximum participants.
    pub max_participants: usize,
    /// Allow anonymous viewers.
    pub allow_anonymous: bool,
    /// Require approval to join.
    pub require_approval: bool,
    /// Auto-save interval (seconds).
    pub auto_save_interval: u64,
    /// Message history limit.
    pub history_limit: usize,
}

impl Default for SessionSettings {
    fn default() -> Self {
        Self {
            max_participants: 10,
            allow_anonymous: false,
            require_approval: false,
            auto_save_interval: 30,
            history_limit: 1000,
        }
    }
}

/// Real-time event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RealtimeEvent {
    /// User joined.
    UserJoined { user_id: String, session_id: Uuid },
    /// User left.
    UserLeft { user_id: String, session_id: Uuid },
    /// Presence changed.
    PresenceChanged { user_id: String, presence: Presence },
    /// Cursor moved.
    CursorMoved {
        user_id: String,
        position: CursorPosition,
    },
    /// Message added.
    MessageAdded { message: SharedMessage },
    /// Message edited.
    MessageEdited { message_id: Uuid, edit: MessageEdit },
    /// Document added.
    DocumentAdded { document: SharedDocument },
    /// Annotation added.
    AnnotationAdded { annotation: Annotation },
    /// Variable changed.
    VariableChanged {
        key: String,
        value: serde_json::Value,
    },
    /// Session ended.
    SessionEnded { session_id: Uuid },
}

/// Collaboration manager.
pub struct CollaborationManager {
    sessions: Arc<RwLock<HashMap<Uuid, CollaborativeSession>>>,
    event_tx: broadcast::Sender<RealtimeEvent>,
}

impl CollaborationManager {
    /// Create a new collaboration manager.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Create a new session.
    pub async fn create_session(&self, name: &str, owner_id: &str) -> Result<CollaborativeSession> {
        let session = CollaborativeSession::new(name, owner_id);
        let id = session.id;
        self.sessions.write().await.insert(id, session.clone());
        Ok(session)
    }

    /// Get a session by ID.
    pub async fn get_session(&self, session_id: Uuid) -> Option<CollaborativeSession> {
        self.sessions.read().await.get(&session_id).cloned()
    }

    /// Join a session.
    pub async fn join_session(&self, session_id: Uuid, user_id: &str, role: Role) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| RealtimeError::SessionNotFound(session_id.to_string()))?;

        session.add_participant(user_id, role)?;

        let _ = self.event_tx.send(RealtimeEvent::UserJoined {
            user_id: user_id.to_string(),
            session_id,
        });

        Ok(())
    }

    /// Leave a session.
    pub async fn leave_session(&self, session_id: Uuid, user_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| RealtimeError::SessionNotFound(session_id.to_string()))?;

        session.remove_participant(user_id);

        let _ = self.event_tx.send(RealtimeEvent::UserLeft {
            user_id: user_id.to_string(),
            session_id,
        });

        Ok(())
    }

    /// Update presence.
    pub async fn update_presence(
        &self,
        session_id: Uuid,
        user_id: &str,
        presence: Presence,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| RealtimeError::SessionNotFound(session_id.to_string()))?;

        if let Some(participant) = session
            .participants
            .iter_mut()
            .find(|p| p.user_id == user_id)
        {
            participant.update_presence(presence);

            let _ = self.event_tx.send(RealtimeEvent::PresenceChanged {
                user_id: user_id.to_string(),
                presence,
            });
        }

        Ok(())
    }

    /// Add a message to the session.
    pub async fn add_message(
        &self,
        session_id: Uuid,
        user_id: &str,
        content: &str,
        message_type: MessageType,
    ) -> Result<SharedMessage> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| RealtimeError::SessionNotFound(session_id.to_string()))?;

        if !session.is_participant(user_id) {
            return Err(RealtimeError::UserNotInSession(user_id.to_string()));
        }

        let participant = session.get_participant(user_id).unwrap();
        if !participant.role.can_edit() && message_type != MessageType::User {
            return Err(RealtimeError::PermissionDenied(
                "Cannot add this message type".to_string(),
            ));
        }

        let message = SharedMessage {
            id: Uuid::new_v4(),
            author_id: user_id.to_string(),
            content: content.to_string(),
            message_type,
            timestamp: Utc::now(),
            edits: Vec::new(),
        };

        session.shared_context.messages.push(message.clone());
        session.last_activity = Utc::now();

        let _ = self.event_tx.send(RealtimeEvent::MessageAdded {
            message: message.clone(),
        });

        Ok(message)
    }

    /// Edit a message.
    pub async fn edit_message(
        &self,
        session_id: Uuid,
        message_id: Uuid,
        user_id: &str,
        new_content: &str,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| RealtimeError::SessionNotFound(session_id.to_string()))?;

        let participant = session
            .get_participant(user_id)
            .ok_or_else(|| RealtimeError::UserNotInSession(user_id.to_string()))?;

        if !participant.role.can_edit() {
            return Err(RealtimeError::PermissionDenied(
                "Cannot edit messages".to_string(),
            ));
        }

        if let Some(message) = session
            .shared_context
            .messages
            .iter_mut()
            .find(|m| m.id == message_id)
        {
            let edit = MessageEdit {
                editor_id: user_id.to_string(),
                previous: message.content.clone(),
                new: new_content.to_string(),
                timestamp: Utc::now(),
            };

            message.content = new_content.to_string();
            message.edits.push(edit.clone());

            let _ = self
                .event_tx
                .send(RealtimeEvent::MessageEdited { message_id, edit });
        }

        Ok(())
    }

    /// Add an annotation.
    pub async fn add_annotation(
        &self,
        session_id: Uuid,
        user_id: &str,
        target_id: Uuid,
        content: &str,
        annotation_type: AnnotationType,
    ) -> Result<Annotation> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| RealtimeError::SessionNotFound(session_id.to_string()))?;

        if !session.is_participant(user_id) {
            return Err(RealtimeError::UserNotInSession(user_id.to_string()));
        }

        let annotation = Annotation {
            id: Uuid::new_v4(),
            author_id: user_id.to_string(),
            target_id,
            content: content.to_string(),
            annotation_type,
            created_at: Utc::now(),
        };

        session.shared_context.annotations.push(annotation.clone());

        let _ = self.event_tx.send(RealtimeEvent::AnnotationAdded {
            annotation: annotation.clone(),
        });

        Ok(annotation)
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<RealtimeEvent> {
        self.event_tx.subscribe()
    }

    /// End a session.
    pub async fn end_session(&self, session_id: Uuid, user_id: &str) -> Result<()> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| RealtimeError::SessionNotFound(session_id.to_string()))?;

        if session.owner_id != user_id {
            return Err(RealtimeError::PermissionDenied(
                "Only owner can end session".to_string(),
            ));
        }

        drop(sessions);

        self.sessions.write().await.remove(&session_id);

        let _ = self
            .event_tx
            .send(RealtimeEvent::SessionEnded { session_id });

        Ok(())
    }

    /// List all sessions.
    pub async fn list_sessions(&self) -> Vec<CollaborativeSession> {
        self.sessions.read().await.values().cloned().collect()
    }

    /// List sessions for a user.
    pub async fn list_user_sessions(&self, user_id: &str) -> Vec<CollaborativeSession> {
        self.sessions
            .read()
            .await
            .values()
            .filter(|s| s.is_participant(user_id))
            .cloned()
            .collect()
    }
}

impl Default for CollaborationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_session() {
        let manager = CollaborationManager::new();
        let session = manager
            .create_session("Test Session", "user-1")
            .await
            .unwrap();

        assert_eq!(session.name, "Test Session");
        assert_eq!(session.owner_id, "user-1");
        assert_eq!(session.participants.len(), 1);
    }

    #[tokio::test]
    async fn test_join_session() {
        let manager = CollaborationManager::new();
        let session = manager.create_session("Test", "owner").await.unwrap();

        manager
            .join_session(session.id, "user-2", Role::Editor)
            .await
            .unwrap();

        let updated = manager.get_session(session.id).await.unwrap();
        assert_eq!(updated.participants.len(), 2);
    }

    #[tokio::test]
    async fn test_add_message() {
        let manager = CollaborationManager::new();
        let session = manager.create_session("Test", "owner").await.unwrap();

        let msg = manager
            .add_message(session.id, "owner", "Hello!", MessageType::User)
            .await
            .unwrap();

        assert_eq!(msg.content, "Hello!");
        assert_eq!(msg.author_id, "owner");
    }

    #[test]
    fn test_role_permissions() {
        assert!(Role::Owner.can_edit());
        assert!(Role::Admin.can_manage());
        assert!(!Role::Viewer.can_edit());
        assert!(!Role::Editor.can_manage());
    }
}
