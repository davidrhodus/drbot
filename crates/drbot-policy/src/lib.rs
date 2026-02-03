//! Policy management and enforcement for drbot.
//!
//! This crate provides:
//! - Policy definitions
//! - Policy enforcement
//! - Decision making
//! - Audit logging

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Policy error types.
#[derive(Error, Debug)]
pub enum PolicyError {
    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("Policy not found: {0}")]
    PolicyNotFound(String),

    #[error("Invalid policy: {0}")]
    InvalidPolicy(String),

    #[error("Evaluation error: {0}")]
    EvaluationError(String),
}

/// Result type for policy operations.
pub type Result<T> = std::result::Result<T, PolicyError>;

/// Policy effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    /// Allow the action.
    Allow,
    /// Deny the action.
    Deny,
}

/// Policy decision.
#[derive(Debug, Clone)]
pub struct Decision {
    /// The effect.
    pub effect: Effect,
    /// Reason for the decision.
    pub reason: Option<String>,
    /// Policy that made the decision.
    pub policy_id: Option<String>,
}

impl Decision {
    /// Create allow decision.
    pub fn allow() -> Self {
        Self {
            effect: Effect::Allow,
            reason: None,
            policy_id: None,
        }
    }

    /// Create deny decision.
    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            effect: Effect::Deny,
            reason: Some(reason.into()),
            policy_id: None,
        }
    }

    /// Set policy ID.
    pub fn with_policy(mut self, id: impl Into<String>) -> Self {
        self.policy_id = Some(id.into());
        self
    }

    /// Check if allowed.
    pub fn is_allowed(&self) -> bool {
        self.effect == Effect::Allow
    }

    /// Check if denied.
    pub fn is_denied(&self) -> bool {
        self.effect == Effect::Deny
    }
}

/// Policy statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statement {
    /// Statement ID.
    pub id: Option<String>,
    /// Effect.
    pub effect: Effect,
    /// Principals (who).
    pub principals: Vec<String>,
    /// Actions (what).
    pub actions: Vec<String>,
    /// Resources (on what).
    pub resources: Vec<String>,
    /// Conditions.
    #[serde(default)]
    pub conditions: HashMap<String, serde_json::Value>,
}

impl Statement {
    /// Create new statement.
    pub fn new(effect: Effect) -> Self {
        Self {
            id: None,
            effect,
            principals: Vec::new(),
            actions: Vec::new(),
            resources: Vec::new(),
            conditions: HashMap::new(),
        }
    }

    /// Set ID.
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Add principal.
    pub fn principal(mut self, principal: impl Into<String>) -> Self {
        self.principals.push(principal.into());
        self
    }

    /// Add action.
    pub fn action(mut self, action: impl Into<String>) -> Self {
        self.actions.push(action.into());
        self
    }

    /// Add resource.
    pub fn resource(mut self, resource: impl Into<String>) -> Self {
        self.resources.push(resource.into());
        self
    }

    /// Add condition.
    pub fn condition(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.conditions.insert(key.into(), value);
        self
    }

    /// Check if matches request.
    pub fn matches(&self, request: &Request) -> bool {
        self.matches_principal(&request.principal)
            && self.matches_action(&request.action)
            && self.matches_resource(&request.resource)
            && self.matches_conditions(&request.context)
    }

    fn matches_principal(&self, principal: &str) -> bool {
        self.principals.is_empty() || self.principals.iter().any(|p| p == "*" || p == principal)
    }

    fn matches_action(&self, action: &str) -> bool {
        self.actions.is_empty() || self.actions.iter().any(|a| match_pattern(a, action))
    }

    fn matches_resource(&self, resource: &str) -> bool {
        self.resources.is_empty() || self.resources.iter().any(|r| match_pattern(r, resource))
    }

    fn matches_conditions(&self, context: &HashMap<String, serde_json::Value>) -> bool {
        for (key, expected) in &self.conditions {
            if let Some(actual) = context.get(key) {
                if actual != expected {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }
}

fn match_pattern(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        return value.starts_with(prefix);
    }

    if pattern.starts_with('*') {
        let suffix = &pattern[1..];
        return value.ends_with(suffix);
    }

    pattern == value
}

/// Policy definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    /// Policy ID.
    pub id: String,
    /// Policy name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Policy version.
    pub version: String,
    /// Statements.
    pub statements: Vec<Statement>,
}

impl Policy {
    /// Create new policy.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            version: "1".to_string(),
            statements: Vec::new(),
        }
    }

    /// Set description.
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add statement.
    pub fn statement(mut self, statement: Statement) -> Self {
        self.statements.push(statement);
        self
    }

    /// Evaluate policy against request.
    pub fn evaluate(&self, request: &Request) -> Option<Decision> {
        // Check explicit denies first
        for statement in &self.statements {
            if statement.effect == Effect::Deny && statement.matches(request) {
                return Some(
                    Decision::deny(format!("Denied by statement {:?}", statement.id))
                        .with_policy(&self.id),
                );
            }
        }

        // Check allows
        for statement in &self.statements {
            if statement.effect == Effect::Allow && statement.matches(request) {
                return Some(Decision::allow().with_policy(&self.id));
            }
        }

        None
    }
}

/// Policy request.
#[derive(Debug, Clone)]
pub struct Request {
    /// Principal making the request.
    pub principal: String,
    /// Action being performed.
    pub action: String,
    /// Resource being accessed.
    pub resource: String,
    /// Additional context.
    pub context: HashMap<String, serde_json::Value>,
}

impl Request {
    /// Create new request.
    pub fn new(
        principal: impl Into<String>,
        action: impl Into<String>,
        resource: impl Into<String>,
    ) -> Self {
        Self {
            principal: principal.into(),
            action: action.into(),
            resource: resource.into(),
            context: HashMap::new(),
        }
    }

    /// Add context.
    pub fn context(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }
}

/// Policy enforcer.
pub struct PolicyEnforcer {
    policies: Vec<Policy>,
    default_effect: Effect,
}

impl PolicyEnforcer {
    /// Create new enforcer.
    pub fn new() -> Self {
        Self {
            policies: Vec::new(),
            default_effect: Effect::Deny,
        }
    }

    /// Set default effect.
    pub fn default_effect(mut self, effect: Effect) -> Self {
        self.default_effect = effect;
        self
    }

    /// Add policy.
    pub fn add_policy(&mut self, policy: Policy) {
        self.policies.push(policy);
    }

    /// Remove policy.
    pub fn remove_policy(&mut self, id: &str) -> bool {
        let len_before = self.policies.len();
        self.policies.retain(|p| p.id != id);
        self.policies.len() < len_before
    }

    /// Evaluate request.
    pub fn evaluate(&self, request: &Request) -> Decision {
        // Check all policies
        let mut allow_decision = None;

        for policy in &self.policies {
            if let Some(decision) = policy.evaluate(request) {
                if decision.is_denied() {
                    return decision; // Explicit deny wins
                }
                if decision.is_allowed() {
                    allow_decision = Some(decision);
                }
            }
        }

        // Return allow if found, otherwise default
        allow_decision.unwrap_or_else(|| {
            if self.default_effect == Effect::Allow {
                Decision::allow()
            } else {
                Decision::deny("No matching policy")
            }
        })
    }

    /// Check if allowed (convenience method).
    pub fn is_allowed(&self, request: &Request) -> bool {
        self.evaluate(request).is_allowed()
    }

    /// Enforce (returns error if denied).
    pub fn enforce(&self, request: &Request) -> Result<()> {
        let decision = self.evaluate(request);
        if decision.is_allowed() {
            Ok(())
        } else {
            Err(PolicyError::AccessDenied(
                decision
                    .reason
                    .unwrap_or_else(|| "Access denied".to_string()),
            ))
        }
    }
}

impl Default for PolicyEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

/// Role-based policy.
#[derive(Debug, Clone)]
pub struct Role {
    /// Role name.
    pub name: String,
    /// Permissions.
    pub permissions: Vec<String>,
}

impl Role {
    /// Create new role.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            permissions: Vec::new(),
        }
    }

    /// Add permission.
    pub fn permission(mut self, permission: impl Into<String>) -> Self {
        self.permissions.push(permission.into());
        self
    }

    /// Check if has permission.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions
            .iter()
            .any(|p| match_pattern(p, permission))
    }
}

/// Simple RBAC enforcer.
pub struct RbacEnforcer {
    roles: HashMap<String, Role>,
    user_roles: HashMap<String, Vec<String>>,
}

impl RbacEnforcer {
    /// Create new RBAC enforcer.
    pub fn new() -> Self {
        Self {
            roles: HashMap::new(),
            user_roles: HashMap::new(),
        }
    }

    /// Add role.
    pub fn add_role(&mut self, role: Role) {
        self.roles.insert(role.name.clone(), role);
    }

    /// Assign role to user.
    pub fn assign_role(&mut self, user: impl Into<String>, role: impl Into<String>) {
        self.user_roles
            .entry(user.into())
            .or_default()
            .push(role.into());
    }

    /// Check if user has permission.
    pub fn has_permission(&self, user: &str, permission: &str) -> bool {
        if let Some(role_names) = self.user_roles.get(user) {
            for role_name in role_names {
                if let Some(role) = self.roles.get(role_name) {
                    if role.has_permission(permission) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Get user roles.
    pub fn user_roles(&self, user: &str) -> Vec<&str> {
        self.user_roles
            .get(user)
            .map(|roles| roles.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
}

impl Default for RbacEnforcer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statement_matching() {
        let statement = Statement::new(Effect::Allow)
            .principal("user:alice")
            .action("read")
            .resource("doc:*");

        let request = Request::new("user:alice", "read", "doc:123");
        assert!(statement.matches(&request));

        let request2 = Request::new("user:bob", "read", "doc:123");
        assert!(!statement.matches(&request2));
    }

    #[test]
    fn test_policy_evaluation() {
        let policy = Policy::new("p1", "Test Policy")
            .statement(
                Statement::new(Effect::Allow)
                    .principal("*")
                    .action("read")
                    .resource("public:*"),
            )
            .statement(
                Statement::new(Effect::Deny)
                    .principal("*")
                    .action("*")
                    .resource("private:*"),
            );

        let request1 = Request::new("user:alice", "read", "public:doc1");
        let decision1 = policy.evaluate(&request1);
        assert!(decision1.unwrap().is_allowed());

        let request2 = Request::new("user:alice", "read", "private:doc1");
        let decision2 = policy.evaluate(&request2);
        assert!(decision2.unwrap().is_denied());
    }

    #[test]
    fn test_enforcer() {
        let mut enforcer = PolicyEnforcer::new();

        enforcer.add_policy(
            Policy::new("admin-policy", "Admin Policy").statement(
                Statement::new(Effect::Allow)
                    .principal("admin")
                    .action("*")
                    .resource("*"),
            ),
        );

        assert!(enforcer.is_allowed(&Request::new("admin", "delete", "anything")));
        assert!(!enforcer.is_allowed(&Request::new("user", "delete", "anything")));
    }

    #[test]
    fn test_rbac() {
        let mut rbac = RbacEnforcer::new();

        rbac.add_role(Role::new("editor").permission("read").permission("write"));

        rbac.add_role(Role::new("admin").permission("*"));

        rbac.assign_role("alice", "editor");
        rbac.assign_role("bob", "admin");

        assert!(rbac.has_permission("alice", "read"));
        assert!(rbac.has_permission("alice", "write"));
        assert!(!rbac.has_permission("alice", "delete"));

        assert!(rbac.has_permission("bob", "delete"));
    }
}
