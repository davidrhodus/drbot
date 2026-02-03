//! Role-based access control and permissions.
//!
//! This crate provides:
//! - Role management
//! - Permission system
//! - Access control policies
//! - User-role assignments

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// RBAC errors.
#[derive(Debug, Error)]
pub enum RbacError {
    #[error("Role not found: {0}")]
    RoleNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid policy: {0}")]
    InvalidPolicy(String),

    #[error("Circular dependency: {0}")]
    CircularDependency(String),
}

/// Result type for RBAC operations.
pub type Result<T> = std::result::Result<T, RbacError>;

/// A role definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// Role identifier.
    pub id: String,
    /// Role name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Permissions.
    pub permissions: HashSet<String>,
    /// Parent roles (inheritance).
    pub parents: Vec<String>,
    /// Is system role.
    pub system: bool,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// A permission definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// Permission identifier.
    pub id: String,
    /// Permission name (e.g., "messages.read").
    pub name: String,
    /// Description.
    pub description: String,
    /// Resource type.
    pub resource: String,
    /// Action.
    pub action: String,
}

/// User-role assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleAssignment {
    /// Assignment identifier.
    pub id: String,
    /// User identifier.
    pub user_id: String,
    /// Role identifier.
    pub role_id: String,
    /// Scope (optional).
    pub scope: Option<String>,
    /// Assigned at.
    pub assigned_at: DateTime<Utc>,
    /// Assigned by.
    pub assigned_by: Option<String>,
    /// Expires at.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Access check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessCheck {
    /// Is allowed.
    pub allowed: bool,
    /// Reason.
    pub reason: String,
    /// Matching role.
    pub role: Option<String>,
    /// Matching permission.
    pub permission: Option<String>,
}

/// The RBAC manager.
pub struct RbacManager {
    /// Roles.
    roles: Arc<RwLock<HashMap<String, Role>>>,
    /// Permissions.
    permissions: Arc<RwLock<HashMap<String, Permission>>>,
    /// Assignments.
    assignments: Arc<RwLock<Vec<RoleAssignment>>>,
    /// Permission cache (user_id -> permissions).
    cache: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl RbacManager {
    /// Create a new RBAC manager.
    pub fn new() -> Self {
        Self {
            roles: Arc::new(RwLock::new(HashMap::new())),
            permissions: Arc::new(RwLock::new(HashMap::new())),
            assignments: Arc::new(RwLock::new(Vec::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a role.
    pub async fn create_role(
        &self,
        name: &str,
        description: &str,
        permissions: Vec<String>,
        parents: Vec<String>,
    ) -> Result<String> {
        // Check for circular dependencies
        if !parents.is_empty() {
            self.check_circular(&name.to_string(), &parents).await?;
        }

        let role = Role {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            permissions: permissions.into_iter().collect(),
            parents,
            system: false,
            created_at: Utc::now(),
        };

        let id = role.id.clone();
        let mut roles = self.roles.write().await;
        roles.insert(id.clone(), role);

        // Invalidate cache
        self.invalidate_cache().await;

        Ok(id)
    }

    /// Check for circular dependencies.
    async fn check_circular(&self, role_name: &str, parents: &[String]) -> Result<()> {
        let roles = self.roles.read().await;
        let mut visited = HashSet::new();
        let mut stack = parents.to_vec();

        while let Some(parent) = stack.pop() {
            if parent == role_name {
                return Err(RbacError::CircularDependency(format!(
                    "Role '{}' would create a circular dependency",
                    role_name
                )));
            }
            if visited.insert(parent.clone()) {
                if let Some(role) = roles.values().find(|r| r.name == parent) {
                    stack.extend(role.parents.clone());
                }
            }
        }

        Ok(())
    }

    /// Delete a role.
    pub async fn delete_role(&self, role_id: &str) -> Result<()> {
        let mut roles = self.roles.write().await;
        let role = roles
            .remove(role_id)
            .ok_or_else(|| RbacError::RoleNotFound(role_id.to_string()))?;

        if role.system {
            roles.insert(role_id.to_string(), role);
            return Err(RbacError::InvalidPolicy(
                "Cannot delete system role".to_string(),
            ));
        }
        drop(roles);

        // Remove assignments
        let mut assignments = self.assignments.write().await;
        assignments.retain(|a| a.role_id != role_id);

        // Invalidate cache
        self.invalidate_cache().await;

        Ok(())
    }

    /// Create a permission.
    pub async fn create_permission(
        &self,
        name: &str,
        description: &str,
        resource: &str,
        action: &str,
    ) -> String {
        let permission = Permission {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            resource: resource.to_string(),
            action: action.to_string(),
        };

        let id = permission.id.clone();
        let mut permissions = self.permissions.write().await;
        permissions.insert(id.clone(), permission);

        id
    }

    /// Assign role to user.
    pub async fn assign_role(
        &self,
        user_id: &str,
        role_id: &str,
        scope: Option<String>,
        assigned_by: Option<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<String> {
        // Verify role exists
        let roles = self.roles.read().await;
        if !roles.contains_key(role_id) {
            return Err(RbacError::RoleNotFound(role_id.to_string()));
        }
        drop(roles);

        let assignment = RoleAssignment {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            role_id: role_id.to_string(),
            scope,
            assigned_at: Utc::now(),
            assigned_by,
            expires_at,
        };

        let id = assignment.id.clone();
        let mut assignments = self.assignments.write().await;
        assignments.push(assignment);

        // Invalidate cache for user
        let mut cache = self.cache.write().await;
        cache.remove(user_id);

        Ok(id)
    }

    /// Revoke role from user.
    pub async fn revoke_role(&self, user_id: &str, role_id: &str) -> Result<()> {
        let mut assignments = self.assignments.write().await;
        let len_before = assignments.len();
        assignments.retain(|a| !(a.user_id == user_id && a.role_id == role_id));

        if assignments.len() == len_before {
            return Err(RbacError::RoleNotFound(format!(
                "No assignment found for user {} and role {}",
                user_id, role_id
            )));
        }

        // Invalidate cache for user
        let mut cache = self.cache.write().await;
        cache.remove(user_id);

        Ok(())
    }

    /// Check if user has permission.
    pub async fn check_permission(&self, user_id: &str, permission: &str) -> AccessCheck {
        let permissions = self.get_user_permissions(user_id).await;

        if permissions.contains(permission) || permissions.contains("*") {
            return AccessCheck {
                allowed: true,
                reason: "Permission granted".to_string(),
                role: None,
                permission: Some(permission.to_string()),
            };
        }

        // Check wildcard patterns
        let parts: Vec<&str> = permission.split('.').collect();
        if parts.len() > 1 {
            let wildcard = format!("{}.*", parts[0]);
            if permissions.contains(&wildcard) {
                return AccessCheck {
                    allowed: true,
                    reason: "Permission granted via wildcard".to_string(),
                    role: None,
                    permission: Some(wildcard),
                };
            }
        }

        AccessCheck {
            allowed: false,
            reason: format!("Permission '{}' not granted", permission),
            role: None,
            permission: None,
        }
    }

    /// Get all permissions for a user.
    pub async fn get_user_permissions(&self, user_id: &str) -> HashSet<String> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(perms) = cache.get(user_id) {
                return perms.clone();
            }
        }

        // Calculate permissions
        let assignments = self.assignments.read().await;
        let roles = self.roles.read().await;
        let now = Utc::now();

        let user_roles: Vec<_> = assignments
            .iter()
            .filter(|a| a.user_id == user_id)
            .filter(|a| a.expires_at.map_or(true, |e| e > now))
            .map(|a| &a.role_id)
            .collect();

        let mut permissions = HashSet::new();
        let mut processed_roles = HashSet::new();
        let mut role_stack: Vec<String> = user_roles.into_iter().cloned().collect();

        while let Some(role_id) = role_stack.pop() {
            if processed_roles.insert(role_id.clone()) {
                if let Some(role) = roles.get(&role_id) {
                    permissions.extend(role.permissions.clone());
                    role_stack.extend(role.parents.iter().filter_map(|name| {
                        roles
                            .values()
                            .find(|r| &r.name == name)
                            .map(|r| r.id.clone())
                    }));
                }
            }
        }
        drop(assignments);
        drop(roles);

        // Cache
        let mut cache = self.cache.write().await;
        cache.insert(user_id.to_string(), permissions.clone());

        permissions
    }

    /// Get user's roles.
    pub async fn get_user_roles(&self, user_id: &str) -> Vec<Role> {
        let assignments = self.assignments.read().await;
        let roles = self.roles.read().await;
        let now = Utc::now();

        assignments
            .iter()
            .filter(|a| a.user_id == user_id)
            .filter(|a| a.expires_at.map_or(true, |e| e > now))
            .filter_map(|a| roles.get(&a.role_id).cloned())
            .collect()
    }

    /// Get role by ID.
    pub async fn get_role(&self, role_id: &str) -> Option<Role> {
        let roles = self.roles.read().await;
        roles.get(role_id).cloned()
    }

    /// List all roles.
    pub async fn list_roles(&self) -> Vec<Role> {
        let roles = self.roles.read().await;
        roles.values().cloned().collect()
    }

    /// Invalidate permission cache.
    async fn invalidate_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Add permissions to role.
    pub async fn add_permissions(&self, role_id: &str, permissions: Vec<String>) -> Result<()> {
        let mut roles = self.roles.write().await;
        let role = roles
            .get_mut(role_id)
            .ok_or_else(|| RbacError::RoleNotFound(role_id.to_string()))?;

        role.permissions.extend(permissions);
        drop(roles);

        self.invalidate_cache().await;
        Ok(())
    }

    /// Remove permissions from role.
    pub async fn remove_permissions(&self, role_id: &str, permissions: Vec<String>) -> Result<()> {
        let mut roles = self.roles.write().await;
        let role = roles
            .get_mut(role_id)
            .ok_or_else(|| RbacError::RoleNotFound(role_id.to_string()))?;

        for perm in permissions {
            role.permissions.remove(&perm);
        }
        drop(roles);

        self.invalidate_cache().await;
        Ok(())
    }
}

impl Default for RbacManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_role() {
        let rbac = RbacManager::new();

        let role_id = rbac
            .create_role("admin", "Administrator role", vec!["*".to_string()], vec![])
            .await
            .unwrap();

        let role = rbac.get_role(&role_id).await.unwrap();
        assert_eq!(role.name, "admin");
    }

    #[tokio::test]
    async fn test_assign_and_check() {
        let rbac = RbacManager::new();

        let role_id = rbac
            .create_role(
                "editor",
                "Editor role",
                vec!["posts.read".to_string(), "posts.write".to_string()],
                vec![],
            )
            .await
            .unwrap();

        rbac.assign_role("user1", &role_id, None, None, None)
            .await
            .unwrap();

        let check = rbac.check_permission("user1", "posts.read").await;
        assert!(check.allowed);

        let check = rbac.check_permission("user1", "users.delete").await;
        assert!(!check.allowed);
    }

    #[tokio::test]
    async fn test_role_inheritance() {
        let rbac = RbacManager::new();

        rbac.create_role(
            "reader",
            "Reader role",
            vec!["posts.read".to_string()],
            vec![],
        )
        .await
        .unwrap();

        let editor_id = rbac
            .create_role(
                "editor",
                "Editor role",
                vec!["posts.write".to_string()],
                vec!["reader".to_string()],
            )
            .await
            .unwrap();

        rbac.assign_role("user1", &editor_id, None, None, None)
            .await
            .unwrap();

        // Should have both read (inherited) and write
        let check = rbac.check_permission("user1", "posts.read").await;
        assert!(check.allowed);

        let check = rbac.check_permission("user1", "posts.write").await;
        assert!(check.allowed);
    }

    #[tokio::test]
    async fn test_wildcard_permission() {
        let rbac = RbacManager::new();

        let role_id = rbac
            .create_role("admin", "Admin", vec!["*".to_string()], vec![])
            .await
            .unwrap();

        rbac.assign_role("admin1", &role_id, None, None, None)
            .await
            .unwrap();

        let check = rbac.check_permission("admin1", "anything.at.all").await;
        assert!(check.allowed);
    }

    #[tokio::test]
    async fn test_revoke_role() {
        let rbac = RbacManager::new();

        let role_id = rbac
            .create_role("test", "Test", vec!["test.perm".to_string()], vec![])
            .await
            .unwrap();

        rbac.assign_role("user1", &role_id, None, None, None)
            .await
            .unwrap();
        rbac.revoke_role("user1", &role_id).await.unwrap();

        let check = rbac.check_permission("user1", "test.perm").await;
        assert!(!check.allowed);
    }
}
