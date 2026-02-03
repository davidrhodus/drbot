//! Role-based access control.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Permission.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Permission {
    /// Resource type.
    pub resource: String,
    /// Action.
    pub action: String,
}

impl Permission {
    /// Create a new permission.
    pub fn new(resource: &str, action: &str) -> Self {
        Self {
            resource: resource.to_string(),
            action: action.to_string(),
        }
    }

    /// Create a wildcard permission.
    pub fn all(resource: &str) -> Self {
        Self::new(resource, "*")
    }

    /// Check if this permission matches another.
    pub fn matches(&self, other: &Permission) -> bool {
        (self.resource == other.resource || self.resource == "*")
            && (self.action == other.action || self.action == "*")
    }
}

/// Role definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// Role name.
    pub name: String,
    /// Display name.
    pub display_name: String,
    /// Description.
    pub description: String,
    /// Permissions.
    pub permissions: HashSet<Permission>,
    /// Is built-in (cannot be modified).
    pub is_builtin: bool,
}

impl Role {
    /// Create a new role.
    pub fn new(name: &str, display_name: &str) -> Self {
        Self {
            name: name.to_string(),
            display_name: display_name.to_string(),
            description: String::new(),
            permissions: HashSet::new(),
            is_builtin: false,
        }
    }

    /// Add a permission.
    pub fn with_permission(mut self, permission: Permission) -> Self {
        self.permissions.insert(permission);
        self
    }

    /// Check if role has permission.
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions.iter().any(|p| p.matches(permission))
    }

    /// Create admin role.
    pub fn admin() -> Self {
        Self {
            name: "admin".to_string(),
            display_name: "Administrator".to_string(),
            description: "Full access to all resources".to_string(),
            permissions: [Permission::new("*", "*")].into_iter().collect(),
            is_builtin: true,
        }
    }

    /// Create member role.
    pub fn member() -> Self {
        Self {
            name: "member".to_string(),
            display_name: "Member".to_string(),
            description: "Standard member access".to_string(),
            permissions: [
                Permission::new("conversations", "read"),
                Permission::new("conversations", "write"),
                Permission::new("agents", "use"),
                Permission::new("shared", "read"),
            ]
            .into_iter()
            .collect(),
            is_builtin: true,
        }
    }

    /// Create viewer role.
    pub fn viewer() -> Self {
        Self {
            name: "viewer".to_string(),
            display_name: "Viewer".to_string(),
            description: "Read-only access".to_string(),
            permissions: [
                Permission::new("conversations", "read"),
                Permission::new("agents", "read"),
                Permission::new("shared", "read"),
            ]
            .into_iter()
            .collect(),
            is_builtin: true,
        }
    }
}

/// Role assignment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleAssignment {
    /// User ID.
    pub user_id: String,
    /// Organization ID.
    pub org_id: Uuid,
    /// Role name.
    pub role: String,
    /// Additional permissions (beyond role).
    pub extra_permissions: HashSet<Permission>,
    /// Denied permissions (override role).
    pub denied_permissions: HashSet<Permission>,
}

impl RoleAssignment {
    /// Create a new role assignment.
    pub fn new(user_id: &str, org_id: Uuid, role: &str) -> Self {
        Self {
            user_id: user_id.to_string(),
            org_id,
            role: role.to_string(),
            extra_permissions: HashSet::new(),
            denied_permissions: HashSet::new(),
        }
    }
}

/// Access control manager.
pub struct AccessControl {
    roles: HashMap<String, Role>,
    assignments: Vec<RoleAssignment>,
}

impl AccessControl {
    /// Create a new access control manager.
    pub fn new() -> Self {
        let mut roles = HashMap::new();
        roles.insert("admin".to_string(), Role::admin());
        roles.insert("member".to_string(), Role::member());
        roles.insert("viewer".to_string(), Role::viewer());

        Self {
            roles,
            assignments: Vec::new(),
        }
    }

    /// Add a custom role.
    pub fn add_role(&mut self, role: Role) {
        self.roles.insert(role.name.clone(), role);
    }

    /// Get a role.
    pub fn get_role(&self, name: &str) -> Option<&Role> {
        self.roles.get(name)
    }

    /// Assign a role.
    pub fn assign_role(&mut self, assignment: RoleAssignment) {
        // Remove existing assignment for same user/org
        self.assignments
            .retain(|a| !(a.user_id == assignment.user_id && a.org_id == assignment.org_id));
        self.assignments.push(assignment);
    }

    /// Get user's assignment for an org.
    pub fn get_assignment(&self, user_id: &str, org_id: Uuid) -> Option<&RoleAssignment> {
        self.assignments
            .iter()
            .find(|a| a.user_id == user_id && a.org_id == org_id)
    }

    /// Check if user has permission.
    pub fn check_permission(&self, user_id: &str, org_id: Uuid, permission: &Permission) -> bool {
        let assignment = match self.get_assignment(user_id, org_id) {
            Some(a) => a,
            None => return false,
        };

        // Check denied first
        if assignment
            .denied_permissions
            .iter()
            .any(|p| p.matches(permission))
        {
            return false;
        }

        // Check extra permissions
        if assignment
            .extra_permissions
            .iter()
            .any(|p| p.matches(permission))
        {
            return true;
        }

        // Check role permissions
        if let Some(role) = self.get_role(&assignment.role) {
            return role.has_permission(permission);
        }

        false
    }

    /// Get all permissions for a user.
    pub fn get_permissions(&self, user_id: &str, org_id: Uuid) -> HashSet<Permission> {
        let assignment = match self.get_assignment(user_id, org_id) {
            Some(a) => a,
            None => return HashSet::new(),
        };

        let mut permissions: HashSet<Permission> = assignment.extra_permissions.clone();

        if let Some(role) = self.get_role(&assignment.role) {
            permissions.extend(role.permissions.clone());
        }

        // Remove denied
        for denied in &assignment.denied_permissions {
            permissions.retain(|p| !denied.matches(p));
        }

        permissions
    }
}

impl Default for AccessControl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_matching() {
        let wildcard = Permission::new("*", "*");
        let specific = Permission::new("conversations", "read");

        assert!(wildcard.matches(&specific));
        assert!(!specific.matches(&wildcard));

        let action_wildcard = Permission::new("conversations", "*");
        assert!(action_wildcard.matches(&specific));
    }

    #[test]
    fn test_role() {
        let admin = Role::admin();
        assert!(admin.has_permission(&Permission::new("anything", "anything")));

        let member = Role::member();
        assert!(member.has_permission(&Permission::new("conversations", "read")));
        assert!(!member.has_permission(&Permission::new("admin", "delete")));
    }

    #[test]
    fn test_access_control() {
        let mut ac = AccessControl::new();
        let org_id = Uuid::new_v4();

        ac.assign_role(RoleAssignment::new("user1", org_id, "admin"));
        ac.assign_role(RoleAssignment::new("user2", org_id, "member"));

        assert!(ac.check_permission("user1", org_id, &Permission::new("anything", "anything")));
        assert!(ac.check_permission("user2", org_id, &Permission::new("conversations", "read")));
        assert!(!ac.check_permission("user2", org_id, &Permission::new("admin", "delete")));
    }
}
