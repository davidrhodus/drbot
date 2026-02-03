//! Permission management for collaborative sessions.

use serde::{Deserialize, Serialize};

/// Permission flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permissions {
    /// Can send messages.
    pub can_send_messages: bool,
    /// Can use AI/bot.
    pub can_use_bot: bool,
    /// Can invite others.
    pub can_invite: bool,
    /// Can kick participants.
    pub can_kick: bool,
    /// Can mute participants.
    pub can_mute: bool,
    /// Can manage session settings.
    pub can_manage_session: bool,
    /// Can manage participants.
    pub can_manage_participants: bool,
    /// Can end session.
    pub can_end_session: bool,
    /// Can view history.
    pub can_view_history: bool,
    /// Can export conversation.
    pub can_export: bool,
}

impl Permissions {
    /// Create with all permissions.
    pub fn all() -> Self {
        Self {
            can_send_messages: true,
            can_use_bot: true,
            can_invite: true,
            can_kick: true,
            can_mute: true,
            can_manage_session: true,
            can_manage_participants: true,
            can_end_session: true,
            can_view_history: true,
            can_export: true,
        }
    }

    /// Create with no permissions.
    pub fn none() -> Self {
        Self {
            can_send_messages: false,
            can_use_bot: false,
            can_invite: false,
            can_kick: false,
            can_mute: false,
            can_manage_session: false,
            can_manage_participants: false,
            can_end_session: false,
            can_view_history: false,
            can_export: false,
        }
    }

    /// Create moderator permissions.
    pub fn moderator() -> Self {
        Self {
            can_send_messages: true,
            can_use_bot: true,
            can_invite: true,
            can_kick: true,
            can_mute: true,
            can_manage_session: false,
            can_manage_participants: true,
            can_end_session: false,
            can_view_history: true,
            can_export: true,
        }
    }

    /// Create participant permissions.
    pub fn participant() -> Self {
        Self {
            can_send_messages: true,
            can_use_bot: true,
            can_invite: false,
            can_kick: false,
            can_mute: false,
            can_manage_session: false,
            can_manage_participants: false,
            can_end_session: false,
            can_view_history: true,
            can_export: false,
        }
    }

    /// Create viewer permissions.
    pub fn viewer() -> Self {
        Self {
            can_send_messages: false,
            can_use_bot: false,
            can_invite: false,
            can_kick: false,
            can_mute: false,
            can_manage_session: false,
            can_manage_participants: false,
            can_end_session: false,
            can_view_history: true,
            can_export: false,
        }
    }

    /// Check if has a specific permission.
    pub fn has_permission(&self, action: &str) -> bool {
        match action {
            "send" | "send_message" | "send_messages" => self.can_send_messages,
            "use_bot" | "bot" => self.can_use_bot,
            "invite" => self.can_invite,
            "kick" => self.can_kick,
            "mute" => self.can_mute,
            "manage_session" | "settings" => self.can_manage_session,
            "manage_participants" | "manage" => self.can_manage_participants,
            "end" | "end_session" => self.can_end_session,
            "view_history" | "history" => self.can_view_history,
            "export" => self.can_export,
            _ => false,
        }
    }

    /// Grant a permission.
    pub fn grant(&mut self, action: &str) {
        match action {
            "send" | "send_message" | "send_messages" => self.can_send_messages = true,
            "use_bot" | "bot" => self.can_use_bot = true,
            "invite" => self.can_invite = true,
            "kick" => self.can_kick = true,
            "mute" => self.can_mute = true,
            "manage_session" | "settings" => self.can_manage_session = true,
            "manage_participants" | "manage" => self.can_manage_participants = true,
            "end" | "end_session" => self.can_end_session = true,
            "view_history" | "history" => self.can_view_history = true,
            "export" => self.can_export = true,
            _ => {}
        }
    }

    /// Revoke a permission.
    pub fn revoke(&mut self, action: &str) {
        match action {
            "send" | "send_message" | "send_messages" => self.can_send_messages = false,
            "use_bot" | "bot" => self.can_use_bot = false,
            "invite" => self.can_invite = false,
            "kick" => self.can_kick = false,
            "mute" => self.can_mute = false,
            "manage_session" | "settings" => self.can_manage_session = false,
            "manage_participants" | "manage" => self.can_manage_participants = false,
            "end" | "end_session" => self.can_end_session = false,
            "view_history" | "history" => self.can_view_history = false,
            "export" => self.can_export = false,
            _ => {}
        }
    }
}

impl Default for Permissions {
    fn default() -> Self {
        Self::participant()
    }
}

/// Permission requirement for an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// Action name.
    pub action: String,
    /// Description.
    pub description: String,
    /// Whether required by default for participants.
    pub default_granted: bool,
}

impl Permission {
    /// Create a permission definition.
    pub fn new(action: &str, description: &str) -> Self {
        Self {
            action: action.to_string(),
            description: description.to_string(),
            default_granted: false,
        }
    }

    /// Mark as granted by default.
    pub fn granted_by_default(mut self) -> Self {
        self.default_granted = true;
        self
    }
}

/// Permission validator.
pub struct PermissionValidator {
    required_permissions: Vec<Permission>,
}

impl PermissionValidator {
    /// Create a new validator.
    pub fn new() -> Self {
        Self {
            required_permissions: Vec::new(),
        }
    }

    /// Add required permission.
    pub fn require(mut self, permission: Permission) -> Self {
        self.required_permissions.push(permission);
        self
    }

    /// Validate permissions.
    pub fn validate(&self, permissions: &Permissions) -> ValidationResult {
        let mut missing = Vec::new();

        for required in &self.required_permissions {
            if !permissions.has_permission(&required.action) {
                missing.push(required.action.clone());
            }
        }

        if missing.is_empty() {
            ValidationResult::Allowed
        } else {
            ValidationResult::Denied { missing }
        }
    }
}

impl Default for PermissionValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Validation result.
#[derive(Debug, Clone)]
pub enum ValidationResult {
    /// Action is allowed.
    Allowed,
    /// Action is denied.
    Denied {
        /// Missing permissions.
        missing: Vec<String>,
    },
}

impl ValidationResult {
    /// Check if allowed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, ValidationResult::Allowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permissions_all() {
        let perms = Permissions::all();
        assert!(perms.can_send_messages);
        assert!(perms.can_end_session);
    }

    #[test]
    fn test_permissions_viewer() {
        let perms = Permissions::viewer();
        assert!(!perms.can_send_messages);
        assert!(perms.can_view_history);
    }

    #[test]
    fn test_has_permission() {
        let perms = Permissions::participant();
        assert!(perms.has_permission("send"));
        assert!(!perms.has_permission("kick"));
    }

    #[test]
    fn test_grant_revoke() {
        let mut perms = Permissions::none();
        assert!(!perms.can_send_messages);

        perms.grant("send");
        assert!(perms.can_send_messages);

        perms.revoke("send");
        assert!(!perms.can_send_messages);
    }

    #[test]
    fn test_validator() {
        let validator =
            PermissionValidator::new().require(Permission::new("send", "Send messages"));

        let participant = Permissions::participant();
        assert!(validator.validate(&participant).is_allowed());

        let viewer = Permissions::viewer();
        assert!(!validator.validate(&viewer).is_allowed());
    }
}
