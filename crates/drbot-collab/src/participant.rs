//! Participant management.

use crate::permissions::Permissions;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Participant role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    /// Session host (full permissions).
    Host,
    /// Moderator (can manage participants).
    Moderator,
    /// Regular participant.
    Participant,
    /// Viewer (read-only).
    Viewer,
}

impl ParticipantRole {
    /// Get default permissions for this role.
    pub fn default_permissions(&self) -> Permissions {
        match self {
            ParticipantRole::Host => Permissions::all(),
            ParticipantRole::Moderator => Permissions::moderator(),
            ParticipantRole::Participant => Permissions::participant(),
            ParticipantRole::Viewer => Permissions::viewer(),
        }
    }
}

/// A participant in a collaborative session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    /// User ID.
    pub id: String,
    /// Display name.
    pub display_name: String,
    /// Role.
    pub role: ParticipantRole,
    /// Permissions.
    pub permissions: Permissions,
    /// Joined timestamp.
    pub joined_at: DateTime<Utc>,
    /// Last active timestamp.
    pub last_active: DateTime<Utc>,
    /// Whether currently online.
    pub online: bool,
    /// Typing indicator.
    pub typing: bool,
    /// Custom metadata.
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

impl Participant {
    /// Create a new participant.
    pub fn new(id: &str, display_name: &str) -> Self {
        let now = Utc::now();
        let role = ParticipantRole::Participant;

        Self {
            id: id.to_string(),
            display_name: display_name.to_string(),
            role,
            permissions: role.default_permissions(),
            joined_at: now,
            last_active: now,
            online: true,
            typing: false,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Set role.
    pub fn with_role(mut self, role: ParticipantRole) -> Self {
        self.role = role;
        self.permissions = role.default_permissions();
        self
    }

    /// Set custom permissions.
    pub fn with_permissions(mut self, permissions: Permissions) -> Self {
        self.permissions = permissions;
        self
    }

    /// Check if participant can send messages.
    pub fn can_send(&self) -> bool {
        self.permissions.can_send_messages
    }

    /// Check if participant can manage others.
    pub fn can_manage(&self) -> bool {
        self.permissions.can_manage_participants
    }

    /// Update last active time.
    pub fn touch(&mut self) {
        self.last_active = Utc::now();
    }

    /// Set online status.
    pub fn set_online(&mut self, online: bool) {
        self.online = online;
        if online {
            self.touch();
        }
    }

    /// Set typing status.
    pub fn set_typing(&mut self, typing: bool) {
        self.typing = typing;
        self.touch();
    }

    /// Set metadata value.
    pub fn set_meta(&mut self, key: &str, value: serde_json::Value) {
        self.metadata.insert(key.to_string(), value);
    }

    /// Get metadata value.
    pub fn get_meta(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }

    /// Get participant info.
    pub fn info(&self) -> ParticipantInfo {
        ParticipantInfo {
            id: self.id.clone(),
            display_name: self.display_name.clone(),
            role: self.role,
            online: self.online,
            typing: self.typing,
        }
    }
}

/// Participant info (lightweight summary).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantInfo {
    /// User ID.
    pub id: String,
    /// Display name.
    pub display_name: String,
    /// Role.
    pub role: ParticipantRole,
    /// Whether online.
    pub online: bool,
    /// Whether typing.
    pub typing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_participant_creation() {
        let participant = Participant::new("user1", "User One");
        assert_eq!(participant.id, "user1");
        assert_eq!(participant.role, ParticipantRole::Participant);
        assert!(participant.online);
    }

    #[test]
    fn test_participant_roles() {
        let host = Participant::new("host", "Host").with_role(ParticipantRole::Host);
        assert!(host.can_send());
        assert!(host.can_manage());

        let viewer = Participant::new("viewer", "Viewer").with_role(ParticipantRole::Viewer);
        assert!(!viewer.can_send());
        assert!(!viewer.can_manage());
    }

    #[test]
    fn test_participant_metadata() {
        let mut participant = Participant::new("user1", "User");
        participant.set_meta("color", serde_json::json!("blue"));

        assert_eq!(
            participant.get_meta("color"),
            Some(&serde_json::json!("blue"))
        );
    }
}
