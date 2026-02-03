//! Collaborative conversations for drbot.
//!
//! Enables multi-user conversations with shared context.

mod participant;
mod permissions;
mod session;
mod sync;

pub use participant::{Participant, ParticipantRole};
pub use permissions::{Permission, Permissions};
pub use session::{CollabSession, SessionState};
pub use sync::{SyncEvent, SyncManager};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Collab result.
pub type Result<T> = std::result::Result<T, CollabError>;

/// Collab errors.
#[derive(Debug, thiserror::Error)]
pub enum CollabError {
    #[error("Session not found: {0}")]
    SessionNotFound(Uuid),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Participant not found: {0}")]
    ParticipantNotFound(String),
    #[error("Sync error: {0}")]
    SyncError(String),
}

/// Collab configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabConfig {
    /// Maximum participants per session.
    pub max_participants: usize,
    /// Session timeout in seconds.
    pub session_timeout_secs: u64,
    /// Allow anonymous participants.
    pub allow_anonymous: bool,
    /// Sync interval in milliseconds.
    pub sync_interval_ms: u64,
}

impl Default for CollabConfig {
    fn default() -> Self {
        Self {
            max_participants: 10,
            session_timeout_secs: 3600,
            allow_anonymous: false,
            sync_interval_ms: 500,
        }
    }
}

/// A message in a collaborative session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabMessage {
    /// Message ID.
    pub id: Uuid,
    /// Session ID.
    pub session_id: Uuid,
    /// Sender participant ID.
    pub sender_id: String,
    /// Content.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Whether this is a bot response.
    pub is_bot_response: bool,
    /// Reply to message ID (if any).
    pub reply_to: Option<Uuid>,
}

impl CollabMessage {
    /// Create a new collab message.
    pub fn new(session_id: Uuid, sender_id: &str, content: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            sender_id: sender_id.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            is_bot_response: false,
            reply_to: None,
        }
    }

    /// Mark as bot response.
    pub fn as_bot_response(mut self) -> Self {
        self.is_bot_response = true;
        self
    }

    /// Set reply to.
    pub fn replying_to(mut self, message_id: Uuid) -> Self {
        self.reply_to = Some(message_id);
        self
    }
}

/// Invitation to join a collab session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInvite {
    /// Invite ID.
    pub id: Uuid,
    /// Session ID.
    pub session_id: Uuid,
    /// Inviter user ID.
    pub inviter_id: String,
    /// Invitee user ID or email.
    pub invitee: String,
    /// Role to assign.
    pub role: ParticipantRole,
    /// Expiration time.
    pub expires_at: DateTime<Utc>,
    /// Whether the invite has been accepted.
    pub accepted: bool,
}

impl SessionInvite {
    /// Create a new invite.
    pub fn new(session_id: Uuid, inviter_id: &str, invitee: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            inviter_id: inviter_id.to_string(),
            invitee: invitee.to_string(),
            role: ParticipantRole::Participant,
            expires_at: Utc::now() + chrono::Duration::days(7),
            accepted: false,
        }
    }

    /// Set the role for the invitee.
    pub fn with_role(mut self, role: ParticipantRole) -> Self {
        self.role = role;
        self
    }

    /// Check if the invite is expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collab_config_default() {
        let config = CollabConfig::default();
        assert_eq!(config.max_participants, 10);
        assert!(!config.allow_anonymous);
    }

    #[test]
    fn test_collab_message() {
        let session_id = Uuid::new_v4();
        let msg = CollabMessage::new(session_id, "user1", "Hello!").as_bot_response();

        assert!(msg.is_bot_response);
        assert_eq!(msg.sender_id, "user1");
    }

    #[test]
    fn test_session_invite() {
        let session_id = Uuid::new_v4();
        let invite = SessionInvite::new(session_id, "host", "guest@example.com")
            .with_role(ParticipantRole::Viewer);

        assert!(!invite.is_expired());
        assert!(!invite.accepted);
    }
}
