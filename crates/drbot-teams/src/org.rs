//! Organization management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{Result, TeamsError};

/// Organization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    /// Organization ID.
    pub id: Uuid,
    /// Organization name.
    pub name: String,
    /// Slug (URL-safe identifier).
    pub slug: String,
    /// Description.
    pub description: Option<String>,
    /// Avatar URL.
    pub avatar_url: Option<String>,
    /// Organization status.
    pub status: OrgStatus,
    /// Configuration.
    pub config: OrgConfig,
    /// Owner user ID.
    pub owner_id: String,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

impl Organization {
    /// Create a new organization.
    pub fn new(name: &str, owner_id: &str) -> Self {
        let slug = name.to_lowercase().replace(' ', "-");

        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            slug,
            description: None,
            avatar_url: None,
            status: OrgStatus::Active,
            config: OrgConfig::default(),
            owner_id: owner_id.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// Organization status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgStatus {
    /// Active organization.
    Active,
    /// Suspended.
    Suspended,
    /// Pending activation.
    Pending,
    /// Deleted.
    Deleted,
}

/// Organization configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgConfig {
    /// Default role for new members.
    pub default_role: String,
    /// Allow member invites.
    pub allow_invites: bool,
    /// Require 2FA.
    pub require_2fa: bool,
    /// Allowed providers.
    pub allowed_providers: Vec<String>,
    /// API access enabled.
    pub api_enabled: bool,
    /// SSO configuration.
    pub sso_config: Option<SsoConfig>,
}

impl Default for OrgConfig {
    fn default() -> Self {
        Self {
            default_role: "member".to_string(),
            allow_invites: true,
            require_2fa: false,
            allowed_providers: Vec::new(),
            api_enabled: true,
            sso_config: None,
        }
    }
}

/// SSO configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoConfig {
    /// SSO provider.
    pub provider: String,
    /// Client ID.
    pub client_id: String,
    /// Tenant/domain.
    pub tenant: Option<String>,
}

/// Organization member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgMember {
    /// User ID.
    pub user_id: String,
    /// Organization ID.
    pub org_id: Uuid,
    /// Display name.
    pub display_name: String,
    /// Email.
    pub email: String,
    /// Role.
    pub role: String,
    /// Joined at.
    pub joined_at: DateTime<Utc>,
    /// Last active.
    pub last_active: Option<DateTime<Utc>>,
    /// Is active.
    pub is_active: bool,
}

impl OrgMember {
    /// Create a new member.
    pub fn new(user_id: &str, org_id: Uuid, email: &str, role: &str) -> Self {
        Self {
            user_id: user_id.to_string(),
            org_id,
            display_name: email.split('@').next().unwrap_or("User").to_string(),
            email: email.to_string(),
            role: role.to_string(),
            joined_at: Utc::now(),
            last_active: None,
            is_active: true,
        }
    }
}

/// Organization manager.
pub struct OrgManager {
    orgs: Arc<RwLock<HashMap<Uuid, Organization>>>,
    members: Arc<RwLock<HashMap<Uuid, Vec<OrgMember>>>>,
}

impl OrgManager {
    /// Create a new organization manager.
    pub fn new() -> Self {
        Self {
            orgs: Arc::new(RwLock::new(HashMap::new())),
            members: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create an organization.
    pub async fn create_org(&self, name: &str, owner_id: &str) -> Result<Organization> {
        let org = Organization::new(name, owner_id);

        // Add owner as admin member
        let owner_member = OrgMember::new(owner_id, org.id, "owner@example.com", "admin");

        let mut orgs = self.orgs.write().await;
        let mut members = self.members.write().await;

        orgs.insert(org.id, org.clone());
        members.insert(org.id, vec![owner_member]);

        Ok(org)
    }

    /// Get an organization by ID.
    pub async fn get_org(&self, id: Uuid) -> Result<Organization> {
        let orgs = self.orgs.read().await;
        orgs.get(&id).cloned().ok_or(TeamsError::OrgNotFound(id))
    }

    /// Get organization by slug.
    pub async fn get_by_slug(&self, slug: &str) -> Option<Organization> {
        let orgs = self.orgs.read().await;
        orgs.values().find(|o| o.slug == slug).cloned()
    }

    /// Update an organization.
    pub async fn update_org(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Organization> {
        let mut orgs = self.orgs.write().await;
        let org = orgs.get_mut(&id).ok_or(TeamsError::OrgNotFound(id))?;

        if let Some(name) = name {
            org.name = name.to_string();
        }
        if let Some(desc) = description {
            org.description = Some(desc.to_string());
        }
        org.updated_at = Utc::now();

        Ok(org.clone())
    }

    /// Delete an organization.
    pub async fn delete_org(&self, id: Uuid) -> Result<()> {
        let mut orgs = self.orgs.write().await;
        let mut members = self.members.write().await;

        orgs.remove(&id);
        members.remove(&id);

        Ok(())
    }

    /// Add a member.
    pub async fn add_member(&self, org_id: Uuid, member: OrgMember) -> Result<()> {
        let mut members = self.members.write().await;
        let org_members = members.entry(org_id).or_insert_with(Vec::new);
        org_members.push(member);
        Ok(())
    }

    /// Remove a member.
    pub async fn remove_member(&self, org_id: Uuid, user_id: &str) -> Result<()> {
        let mut members = self.members.write().await;
        if let Some(org_members) = members.get_mut(&org_id) {
            org_members.retain(|m| m.user_id != user_id);
        }
        Ok(())
    }

    /// Get members.
    pub async fn get_members(&self, org_id: Uuid) -> Result<Vec<OrgMember>> {
        let members = self.members.read().await;
        Ok(members.get(&org_id).cloned().unwrap_or_default())
    }

    /// Get user's organizations.
    pub async fn get_user_orgs(&self, user_id: &str) -> Vec<Organization> {
        let orgs = self.orgs.read().await;
        let members = self.members.read().await;

        let mut user_orgs = Vec::new();
        for (org_id, org_members) in members.iter() {
            if org_members.iter().any(|m| m.user_id == user_id) {
                if let Some(org) = orgs.get(org_id) {
                    user_orgs.push(org.clone());
                }
            }
        }

        user_orgs
    }

    /// Update member role.
    pub async fn update_member_role(&self, org_id: Uuid, user_id: &str, role: &str) -> Result<()> {
        let mut members = self.members.write().await;
        if let Some(org_members) = members.get_mut(&org_id) {
            if let Some(member) = org_members.iter_mut().find(|m| m.user_id == user_id) {
                member.role = role.to_string();
                return Ok(());
            }
        }
        Err(TeamsError::MemberNotFound(user_id.to_string()))
    }
}

impl Default for OrgManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_org() {
        let manager = OrgManager::new();
        let org = manager.create_org("Test Org", "user1").await.unwrap();

        assert_eq!(org.name, "Test Org");
        assert_eq!(org.slug, "test-org");
        assert_eq!(org.owner_id, "user1");

        let members = manager.get_members(org.id).await.unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].role, "admin");
    }

    #[tokio::test]
    async fn test_member_management() {
        let manager = OrgManager::new();
        let org = manager.create_org("Test", "owner").await.unwrap();

        let member = OrgMember::new("user2", org.id, "user2@example.com", "member");
        manager.add_member(org.id, member).await.unwrap();

        let members = manager.get_members(org.id).await.unwrap();
        assert_eq!(members.len(), 2);

        manager.remove_member(org.id, "user2").await.unwrap();
        let members = manager.get_members(org.id).await.unwrap();
        assert_eq!(members.len(), 1);
    }
}
