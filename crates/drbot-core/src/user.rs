//! User types for drbot.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A user in the drbot system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique user ID in drbot.
    pub id: Uuid,
    /// Display name.
    pub name: Option<String>,
    /// Email address.
    pub email: Option<String>,
    /// Platform identities linked to this user.
    pub identities: Vec<UserIdentity>,
    /// User preferences.
    #[serde(default)]
    pub preferences: UserPreferences,
    /// When the user was created.
    pub created_at: DateTime<Utc>,
    /// When the user was last active.
    pub last_active_at: Option<DateTime<Utc>>,
}

impl User {
    /// Create a new user.
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: None,
            email: None,
            identities: Vec::new(),
            preferences: UserPreferences::default(),
            created_at: Utc::now(),
            last_active_at: None,
        }
    }

    /// Create a user with a name.
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Self::new()
        }
    }

    /// Add a platform identity to the user.
    pub fn add_identity(&mut self, identity: UserIdentity) {
        self.identities.push(identity);
    }

    /// Find an identity by platform.
    pub fn identity_for_platform(&self, platform: &str) -> Option<&UserIdentity> {
        self.identities.iter().find(|i| i.platform == platform)
    }
}

impl Default for User {
    fn default() -> Self {
        Self::new()
    }
}

/// A platform identity linked to a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIdentity {
    /// Platform name (e.g., "telegram", "discord").
    pub platform: String,
    /// Platform-specific user ID.
    pub platform_user_id: String,
    /// Username on the platform.
    pub username: Option<String>,
    /// When this identity was linked.
    pub linked_at: DateTime<Utc>,
}

impl UserIdentity {
    /// Create a new platform identity.
    pub fn new(platform: impl Into<String>, platform_user_id: impl Into<String>) -> Self {
        Self {
            platform: platform.into(),
            platform_user_id: platform_user_id.into(),
            username: None,
            linked_at: Utc::now(),
        }
    }

    /// Set the username.
    pub fn with_username(mut self, username: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self
    }
}

/// User preferences.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserPreferences {
    /// Preferred AI model.
    pub preferred_model: Option<String>,
    /// Preferred language.
    pub language: Option<String>,
    /// Timezone.
    pub timezone: Option<String>,
    /// Custom system prompt.
    pub system_prompt: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let user = User::with_name("Alice");
        assert_eq!(user.name, Some("Alice".to_string()));
    }

    #[test]
    fn test_user_identity() {
        let mut user = User::new();
        user.add_identity(UserIdentity::new("telegram", "12345").with_username("alice"));

        let identity = user.identity_for_platform("telegram").unwrap();
        assert_eq!(identity.platform_user_id, "12345");
        assert_eq!(identity.username, Some("alice".to_string()));
    }
}
