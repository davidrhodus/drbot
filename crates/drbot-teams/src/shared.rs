//! Shared resources management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::Result;

/// Resource type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    /// Conversation.
    Conversation,
    /// Agent.
    Agent,
    /// Workflow.
    Workflow,
    /// Prompt template.
    Prompt,
    /// Knowledge base.
    Knowledge,
    /// File.
    File,
}

/// Shared resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedResource {
    /// Resource ID.
    pub id: Uuid,
    /// Organization ID.
    pub org_id: Uuid,
    /// Resource type.
    pub resource_type: ResourceType,
    /// Original resource ID.
    pub resource_id: Uuid,
    /// Shared by.
    pub shared_by: String,
    /// Name/title.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Share settings.
    pub settings: ShareSettings,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

impl SharedResource {
    /// Create a new shared resource.
    pub fn new(
        org_id: Uuid,
        resource_type: ResourceType,
        resource_id: Uuid,
        shared_by: &str,
        name: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            org_id,
            resource_type,
            resource_id,
            shared_by: shared_by.to_string(),
            name: name.to_string(),
            description: None,
            settings: ShareSettings::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

/// Share settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareSettings {
    /// Visibility.
    pub visibility: Visibility,
    /// Allow editing.
    pub allow_edit: bool,
    /// Allow forking/copying.
    pub allow_fork: bool,
    /// Specific users with access (if Private).
    pub allowed_users: Vec<String>,
    /// Specific roles with access.
    pub allowed_roles: Vec<String>,
    /// Password protection.
    pub password: Option<String>,
    /// Expiry time.
    pub expires_at: Option<DateTime<Utc>>,
}

impl Default for ShareSettings {
    fn default() -> Self {
        Self {
            visibility: Visibility::Organization,
            allow_edit: false,
            allow_fork: true,
            allowed_users: Vec::new(),
            allowed_roles: Vec::new(),
            password: None,
            expires_at: None,
        }
    }
}

/// Visibility levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Only specific users.
    Private,
    /// Organization members.
    Organization,
    /// Anyone with link.
    Public,
}

/// Share manager.
pub struct ShareManager {
    resources: Arc<RwLock<HashMap<Uuid, SharedResource>>>,
}

impl ShareManager {
    /// Create a new share manager.
    pub fn new() -> Self {
        Self {
            resources: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Share a resource.
    pub async fn share(&self, resource: SharedResource) -> Result<SharedResource> {
        let mut resources = self.resources.write().await;
        resources.insert(resource.id, resource.clone());
        Ok(resource)
    }

    /// Get a shared resource.
    pub async fn get(&self, id: Uuid) -> Option<SharedResource> {
        let resources = self.resources.read().await;
        resources.get(&id).cloned()
    }

    /// Get shared resources for an organization.
    pub async fn get_org_resources(&self, org_id: Uuid) -> Vec<SharedResource> {
        let resources = self.resources.read().await;
        resources
            .values()
            .filter(|r| r.org_id == org_id)
            .cloned()
            .collect()
    }

    /// Get shared resources by type.
    pub async fn get_by_type(
        &self,
        org_id: Uuid,
        resource_type: ResourceType,
    ) -> Vec<SharedResource> {
        let resources = self.resources.read().await;
        resources
            .values()
            .filter(|r| r.org_id == org_id && r.resource_type == resource_type)
            .cloned()
            .collect()
    }

    /// Update share settings.
    pub async fn update_settings(&self, id: Uuid, settings: ShareSettings) -> Result<()> {
        let mut resources = self.resources.write().await;
        if let Some(resource) = resources.get_mut(&id) {
            resource.settings = settings;
            resource.updated_at = Utc::now();
        }
        Ok(())
    }

    /// Unshare a resource.
    pub async fn unshare(&self, id: Uuid) -> bool {
        let mut resources = self.resources.write().await;
        resources.remove(&id).is_some()
    }

    /// Check if user can access a shared resource.
    pub fn can_access(
        &self,
        resource: &SharedResource,
        user_id: &str,
        user_role: &str,
        is_org_member: bool,
    ) -> bool {
        // Check expiry
        if let Some(expires) = resource.settings.expires_at {
            if Utc::now() > expires {
                return false;
            }
        }

        match resource.settings.visibility {
            Visibility::Public => true,
            Visibility::Organization => is_org_member,
            Visibility::Private => {
                resource.shared_by == user_id
                    || resource
                        .settings
                        .allowed_users
                        .contains(&user_id.to_string())
                    || resource
                        .settings
                        .allowed_roles
                        .contains(&user_role.to_string())
            }
        }
    }

    /// Check if user can edit a shared resource.
    pub fn can_edit(&self, resource: &SharedResource, user_id: &str) -> bool {
        resource.settings.allow_edit || resource.shared_by == user_id
    }
}

impl Default for ShareManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_share_manager() {
        let manager = ShareManager::new();
        let org_id = Uuid::new_v4();

        let resource = SharedResource::new(
            org_id,
            ResourceType::Conversation,
            Uuid::new_v4(),
            "user1",
            "Shared Conversation",
        );

        manager.share(resource.clone()).await.unwrap();

        let fetched = manager.get(resource.id).await;
        assert!(fetched.is_some());

        let org_resources = manager.get_org_resources(org_id).await;
        assert_eq!(org_resources.len(), 1);
    }

    #[test]
    fn test_access_control() {
        let manager = ShareManager::new();
        let org_id = Uuid::new_v4();

        let mut resource = SharedResource::new(
            org_id,
            ResourceType::Agent,
            Uuid::new_v4(),
            "user1",
            "Shared Agent",
        );

        // Organization visibility
        resource.settings.visibility = Visibility::Organization;
        assert!(manager.can_access(&resource, "user2", "member", true));
        assert!(!manager.can_access(&resource, "user3", "member", false));

        // Private visibility
        resource.settings.visibility = Visibility::Private;
        resource.settings.allowed_users.push("user2".to_string());
        assert!(manager.can_access(&resource, "user1", "member", true)); // Owner
        assert!(manager.can_access(&resource, "user2", "member", true)); // Allowed
        assert!(!manager.can_access(&resource, "user3", "member", true)); // Not allowed
    }
}
