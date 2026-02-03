//! Team and organization support for drbot.
//!
//! Provides multi-tenant capabilities for teams and organizations.
//!
//! # Features
//!
//! - Organization management
//! - Role-based access control (RBAC)
//! - Shared conversations and agents
//! - Usage tracking and billing per team

mod billing;
mod org;
mod roles;
mod shared;

pub use billing::{BillingPeriod, UsageRecord, UsageSummary, UsageTracker};
pub use org::{OrgConfig, OrgManager, OrgMember, OrgStatus, Organization};
pub use roles::{AccessControl, Permission, Role, RoleAssignment};
pub use shared::{ResourceType, ShareManager, ShareSettings, SharedResource};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Teams result type.
pub type Result<T> = std::result::Result<T, TeamsError>;

/// Teams errors.
#[derive(Debug, thiserror::Error)]
pub enum TeamsError {
    #[error("Organization not found: {0}")]
    OrgNotFound(Uuid),
    #[error("Member not found: {0}")]
    MemberNotFound(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Quota exceeded: {0}")]
    QuotaExceeded(String),
}

/// Teams configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsConfig {
    /// Enable teams feature.
    pub enabled: bool,
    /// Maximum members per organization.
    pub max_members: usize,
    /// Maximum shared resources.
    pub max_shared_resources: usize,
    /// Enable usage tracking.
    pub track_usage: bool,
}

impl Default for TeamsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_members: 50,
            max_shared_resources: 100,
            track_usage: true,
        }
    }
}

/// Team invitation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invitation {
    /// Invitation ID.
    pub id: Uuid,
    /// Organization ID.
    pub org_id: Uuid,
    /// Invitee email.
    pub email: String,
    /// Assigned role.
    pub role: String,
    /// Invited by.
    pub invited_by: String,
    /// Expiry time.
    pub expires_at: DateTime<Utc>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl Invitation {
    /// Create a new invitation.
    pub fn new(org_id: Uuid, email: &str, role: &str, invited_by: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            org_id,
            email: email.to_string(),
            role: role.to_string(),
            invited_by: invited_by.to_string(),
            expires_at: Utc::now() + chrono::Duration::days(7),
            created_at: Utc::now(),
        }
    }

    /// Check if expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_teams_config_default() {
        let config = TeamsConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_members, 50);
    }

    #[test]
    fn test_invitation() {
        let inv = Invitation::new(Uuid::new_v4(), "test@example.com", "member", "admin");
        assert!(!inv.is_expired());
    }
}
