//! Undo/redo stack for drbot.
//!
//! Track and revert actions.
//!
//! # Features
//!
//! - Action history
//! - Undo/redo operations
//! - State snapshots
//! - Branching history

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Undo/redo result type.
pub type Result<T> = std::result::Result<T, UndoError>;

/// Undo errors.
#[derive(Debug, thiserror::Error)]
pub enum UndoError {
    #[error("Nothing to undo")]
    NothingToUndo,
    #[error("Nothing to redo")]
    NothingToRedo,
    #[error("Action not found: {0}")]
    ActionNotFound(Uuid),
    #[error("Cannot undo: {0}")]
    CannotUndo(String),
    #[error("Stack limit reached")]
    StackLimitReached,
}

/// An undoable action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Action ID.
    pub id: Uuid,
    /// Action type.
    pub action_type: String,
    /// Description.
    pub description: String,
    /// State before action.
    pub before_state: serde_json::Value,
    /// State after action.
    pub after_state: serde_json::Value,
    /// Is reversible.
    pub reversible: bool,
    /// Performed at.
    pub performed_at: DateTime<Utc>,
    /// Metadata.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Action {
    /// Create a new action.
    pub fn new(action_type: &str, description: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            action_type: action_type.to_string(),
            description: description.to_string(),
            before_state: serde_json::Value::Null,
            after_state: serde_json::Value::Null,
            reversible: true,
            performed_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Set before state.
    pub fn with_before(mut self, state: serde_json::Value) -> Self {
        self.before_state = state;
        self
    }

    /// Set after state.
    pub fn with_after(mut self, state: serde_json::Value) -> Self {
        self.after_state = state;
        self
    }

    /// Mark as non-reversible.
    pub fn non_reversible(mut self) -> Self {
        self.reversible = false;
        self
    }
}

/// Undo/redo stack configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoConfig {
    /// Maximum stack size.
    pub max_size: usize,
    /// Enable branching history.
    pub branching: bool,
    /// Group related actions.
    pub action_grouping: bool,
    /// Persist history.
    pub persist: bool,
}

impl Default for UndoConfig {
    fn default() -> Self {
        Self {
            max_size: 100,
            branching: false,
            action_grouping: true,
            persist: false,
        }
    }
}

/// Action group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionGroup {
    /// Group ID.
    pub id: Uuid,
    /// Group name.
    pub name: String,
    /// Actions in group.
    pub actions: Vec<Uuid>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl ActionGroup {
    /// Create a new group.
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            actions: Vec::new(),
            created_at: Utc::now(),
        }
    }
}

/// History branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryBranch {
    /// Branch ID.
    pub id: Uuid,
    /// Branch name.
    pub name: String,
    /// Parent branch.
    pub parent: Option<Uuid>,
    /// Branch point (action ID).
    pub branch_point: Uuid,
    /// Actions in this branch.
    pub actions: Vec<Uuid>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// Undo/redo manager.
pub struct UndoRedoManager {
    config: UndoConfig,
    actions: Arc<RwLock<HashMap<Uuid, Action>>>,
    undo_stack: Arc<RwLock<Vec<Uuid>>>,
    redo_stack: Arc<RwLock<Vec<Uuid>>>,
    groups: Arc<RwLock<HashMap<Uuid, ActionGroup>>>,
    branches: Arc<RwLock<HashMap<Uuid, HistoryBranch>>>,
    current_branch: Arc<RwLock<Option<Uuid>>>,
    current_group: Arc<RwLock<Option<Uuid>>>,
}

impl UndoRedoManager {
    /// Create a new manager.
    pub fn new(config: UndoConfig) -> Self {
        Self {
            config,
            actions: Arc::new(RwLock::new(HashMap::new())),
            undo_stack: Arc::new(RwLock::new(Vec::new())),
            redo_stack: Arc::new(RwLock::new(Vec::new())),
            groups: Arc::new(RwLock::new(HashMap::new())),
            branches: Arc::new(RwLock::new(HashMap::new())),
            current_branch: Arc::new(RwLock::new(None)),
            current_group: Arc::new(RwLock::new(None)),
        }
    }

    /// Record an action.
    pub async fn record(&self, action: Action) -> Result<Uuid> {
        let id = action.id;

        // Store action
        self.actions.write().await.insert(id, action);

        // Add to undo stack
        let mut undo_stack = self.undo_stack.write().await;
        undo_stack.push(id);

        // Enforce max size
        while undo_stack.len() > self.config.max_size {
            let old_id = undo_stack.remove(0);
            self.actions.write().await.remove(&old_id);
        }

        // Clear redo stack (new action invalidates redo)
        self.redo_stack.write().await.clear();

        // Add to current group if any
        if let Some(group_id) = *self.current_group.read().await {
            if let Some(group) = self.groups.write().await.get_mut(&group_id) {
                group.actions.push(id);
            }
        }

        Ok(id)
    }

    /// Undo the last action.
    pub async fn undo(&self) -> Result<Action> {
        let mut undo_stack = self.undo_stack.write().await;
        let action_id = undo_stack.pop().ok_or(UndoError::NothingToUndo)?;

        let action = self
            .actions
            .read()
            .await
            .get(&action_id)
            .cloned()
            .ok_or(UndoError::ActionNotFound(action_id))?;

        if !action.reversible {
            // Put it back
            undo_stack.push(action_id);
            return Err(UndoError::CannotUndo(
                "Action is not reversible".to_string(),
            ));
        }

        // Move to redo stack
        self.redo_stack.write().await.push(action_id);

        Ok(action)
    }

    /// Redo the last undone action.
    pub async fn redo(&self) -> Result<Action> {
        let mut redo_stack = self.redo_stack.write().await;
        let action_id = redo_stack.pop().ok_or(UndoError::NothingToRedo)?;

        let action = self
            .actions
            .read()
            .await
            .get(&action_id)
            .cloned()
            .ok_or(UndoError::ActionNotFound(action_id))?;

        // Move back to undo stack
        self.undo_stack.write().await.push(action_id);

        Ok(action)
    }

    /// Check if can undo.
    pub async fn can_undo(&self) -> bool {
        let undo_stack = self.undo_stack.read().await;
        if undo_stack.is_empty() {
            return false;
        }

        // Check if top action is reversible
        if let Some(&id) = undo_stack.last() {
            if let Some(action) = self.actions.read().await.get(&id) {
                return action.reversible;
            }
        }

        false
    }

    /// Check if can redo.
    pub async fn can_redo(&self) -> bool {
        !self.redo_stack.read().await.is_empty()
    }

    /// Start an action group.
    pub async fn start_group(&self, name: &str) -> Uuid {
        let group = ActionGroup::new(name);
        let id = group.id;

        self.groups.write().await.insert(id, group);
        *self.current_group.write().await = Some(id);

        id
    }

    /// End the current action group.
    pub async fn end_group(&self) -> Option<ActionGroup> {
        let group_id = self.current_group.write().await.take()?;
        self.groups.read().await.get(&group_id).cloned()
    }

    /// Undo a group.
    pub async fn undo_group(&self, group_id: Uuid) -> Result<Vec<Action>> {
        let groups = self.groups.read().await;
        let group = groups
            .get(&group_id)
            .ok_or(UndoError::ActionNotFound(group_id))?;

        let mut undone = Vec::new();

        // Undo all actions in group (in reverse order)
        for &action_id in group.actions.iter().rev() {
            if let Some(action) = self.actions.read().await.get(&action_id).cloned() {
                if action.reversible {
                    // Remove from undo stack
                    self.undo_stack.write().await.retain(|&id| id != action_id);
                    // Add to redo stack
                    self.redo_stack.write().await.push(action_id);
                    undone.push(action);
                }
            }
        }

        Ok(undone)
    }

    /// Get undo stack.
    pub async fn undo_stack(&self) -> Vec<Action> {
        let stack = self.undo_stack.read().await;
        let actions = self.actions.read().await;

        stack
            .iter()
            .filter_map(|id| actions.get(id).cloned())
            .collect()
    }

    /// Get redo stack.
    pub async fn redo_stack(&self) -> Vec<Action> {
        let stack = self.redo_stack.read().await;
        let actions = self.actions.read().await;

        stack
            .iter()
            .filter_map(|id| actions.get(id).cloned())
            .collect()
    }

    /// Get action by ID.
    pub async fn get_action(&self, id: Uuid) -> Option<Action> {
        self.actions.read().await.get(&id).cloned()
    }

    /// Clear all history.
    pub async fn clear(&self) {
        self.actions.write().await.clear();
        self.undo_stack.write().await.clear();
        self.redo_stack.write().await.clear();
        self.groups.write().await.clear();
    }

    /// Create a branch at current position.
    pub async fn create_branch(&self, name: &str) -> Result<Uuid> {
        if !self.config.branching {
            return Err(UndoError::CannotUndo("Branching not enabled".to_string()));
        }

        let undo_stack = self.undo_stack.read().await;
        let branch_point = undo_stack.last().copied().ok_or(UndoError::NothingToUndo)?;

        let branch = HistoryBranch {
            id: Uuid::new_v4(),
            name: name.to_string(),
            parent: *self.current_branch.read().await,
            branch_point,
            actions: Vec::new(),
            created_at: Utc::now(),
        };

        let id = branch.id;
        self.branches.write().await.insert(id, branch);
        *self.current_branch.write().await = Some(id);

        Ok(id)
    }

    /// Switch to a branch.
    pub async fn switch_branch(&self, branch_id: Uuid) -> Result<()> {
        if !self.branches.read().await.contains_key(&branch_id) {
            return Err(UndoError::ActionNotFound(branch_id));
        }

        *self.current_branch.write().await = Some(branch_id);
        Ok(())
    }

    /// Get statistics.
    pub async fn stats(&self) -> UndoStats {
        let undo_stack = self.undo_stack.read().await;
        let redo_stack = self.redo_stack.read().await;
        let groups = self.groups.read().await;
        let branches = self.branches.read().await;

        UndoStats {
            undo_count: undo_stack.len(),
            redo_count: redo_stack.len(),
            group_count: groups.len(),
            branch_count: branches.len(),
            total_actions: self.actions.read().await.len(),
        }
    }
}

/// Undo statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoStats {
    pub undo_count: usize,
    pub redo_count: usize,
    pub group_count: usize,
    pub branch_count: usize,
    pub total_actions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_undo_redo() {
        let manager = UndoRedoManager::new(UndoConfig::default());

        let action = Action::new("edit", "Edit document")
            .with_before(serde_json::json!({"text": "hello"}))
            .with_after(serde_json::json!({"text": "hello world"}));

        manager.record(action).await.unwrap();

        assert!(manager.can_undo().await);
        assert!(!manager.can_redo().await);

        let undone = manager.undo().await.unwrap();
        assert_eq!(undone.action_type, "edit");

        assert!(!manager.can_undo().await);
        assert!(manager.can_redo().await);

        let redone = manager.redo().await.unwrap();
        assert_eq!(redone.action_type, "edit");
    }

    #[tokio::test]
    async fn test_action_groups() {
        let manager = UndoRedoManager::new(UndoConfig::default());

        let group_id = manager.start_group("batch edit").await;

        manager
            .record(Action::new("edit1", "First edit"))
            .await
            .unwrap();
        manager
            .record(Action::new("edit2", "Second edit"))
            .await
            .unwrap();

        let group = manager.end_group().await.unwrap();
        assert_eq!(group.actions.len(), 2);

        let undone = manager.undo_group(group_id).await.unwrap();
        assert_eq!(undone.len(), 2);
    }

    #[tokio::test]
    async fn test_non_reversible() {
        let manager = UndoRedoManager::new(UndoConfig::default());

        let action = Action::new("delete", "Delete file").non_reversible();
        manager.record(action).await.unwrap();

        let result = manager.undo().await;
        assert!(matches!(result, Err(UndoError::CannotUndo(_))));
    }

    #[tokio::test]
    async fn test_max_size() {
        let config = UndoConfig {
            max_size: 5,
            ..Default::default()
        };
        let manager = UndoRedoManager::new(config);

        for i in 0..10 {
            manager
                .record(Action::new(&format!("action{}", i), ""))
                .await
                .unwrap();
        }

        let stats = manager.stats().await;
        assert_eq!(stats.undo_count, 5);
    }

    #[tokio::test]
    async fn test_clear() {
        let manager = UndoRedoManager::new(UndoConfig::default());

        manager.record(Action::new("action", "Test")).await.unwrap();
        assert!(manager.can_undo().await);

        manager.clear().await;

        assert!(!manager.can_undo().await);
        let stats = manager.stats().await;
        assert_eq!(stats.total_actions, 0);
    }
}
