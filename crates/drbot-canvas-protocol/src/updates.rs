//! Delta update types for efficient canvas updates.

use crate::{CanvasId, ComponentId, ComponentSpec};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Delta update message for efficient partial updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaUpdate {
    /// Canvas ID.
    pub canvas_id: CanvasId,
    /// Sequence number for ordering.
    pub sequence: u64,
    /// Individual deltas.
    pub deltas: Vec<Delta>,
}

impl DeltaUpdate {
    /// Create a new delta update.
    pub fn new(canvas_id: CanvasId, sequence: u64) -> Self {
        Self {
            canvas_id,
            sequence,
            deltas: Vec::new(),
        }
    }

    /// Add a delta.
    pub fn add(mut self, delta: Delta) -> Self {
        self.deltas.push(delta);
        self
    }

    /// Add an insert delta.
    pub fn insert(self, parent_id: ComponentId, index: usize, component: ComponentSpec) -> Self {
        self.add(Delta::Insert {
            parent_id,
            index,
            component,
        })
    }

    /// Add a remove delta.
    pub fn remove(self, component_id: ComponentId) -> Self {
        self.add(Delta::Remove { component_id })
    }

    /// Add a property update delta.
    pub fn update_props(
        self,
        component_id: ComponentId,
        props: HashMap<String, serde_json::Value>,
    ) -> Self {
        self.add(Delta::UpdateProps {
            component_id,
            props,
        })
    }
}

/// Individual delta operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Delta {
    /// Insert a new component.
    Insert {
        /// Parent component ID.
        parent_id: ComponentId,
        /// Index in parent's children.
        index: usize,
        /// Component to insert.
        component: ComponentSpec,
    },
    /// Remove a component.
    Remove {
        /// Component ID to remove.
        component_id: ComponentId,
    },
    /// Move a component.
    Move {
        /// Component ID to move.
        component_id: ComponentId,
        /// New parent ID.
        new_parent_id: ComponentId,
        /// New index in parent's children.
        new_index: usize,
    },
    /// Update component properties.
    UpdateProps {
        /// Component ID.
        component_id: ComponentId,
        /// Properties to update.
        props: HashMap<String, serde_json::Value>,
    },
    /// Update component style.
    UpdateStyle {
        /// Component ID.
        component_id: ComponentId,
        /// Style properties to update.
        style: HashMap<String, serde_json::Value>,
    },
    /// Replace a component entirely.
    Replace {
        /// Component ID to replace.
        component_id: ComponentId,
        /// New component specification.
        component: ComponentSpec,
    },
    /// Set component text content.
    SetText {
        /// Component ID.
        component_id: ComponentId,
        /// New text content.
        text: String,
    },
    /// Set component value.
    SetValue {
        /// Component ID.
        component_id: ComponentId,
        /// New value.
        value: serde_json::Value,
    },
}

/// Batch update for applying multiple updates atomically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchUpdate {
    /// Canvas ID.
    pub canvas_id: CanvasId,
    /// Updates to apply.
    pub updates: Vec<DeltaUpdate>,
    /// Whether to apply atomically (all or nothing).
    #[serde(default)]
    pub atomic: bool,
}

/// Update acknowledgment from client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAck {
    /// Canvas ID.
    pub canvas_id: CanvasId,
    /// Last applied sequence number.
    pub sequence: u64,
    /// Whether the update was successful.
    pub success: bool,
    /// Error message if failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl UpdateAck {
    /// Create a success acknowledgment.
    pub fn success(canvas_id: CanvasId, sequence: u64) -> Self {
        Self {
            canvas_id,
            sequence,
            success: true,
            error: None,
        }
    }

    /// Create a failure acknowledgment.
    pub fn failure(canvas_id: CanvasId, sequence: u64, error: impl Into<String>) -> Self {
        Self {
            canvas_id,
            sequence,
            success: false,
            error: Some(error.into()),
        }
    }
}

/// Sync request from client to reconcile state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    /// Canvas ID.
    pub canvas_id: CanvasId,
    /// Last known sequence number.
    pub last_sequence: u64,
}

/// Sync response with full or partial state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    /// Canvas ID.
    pub canvas_id: CanvasId,
    /// Current sequence number.
    pub sequence: u64,
    /// Sync type.
    #[serde(flatten)]
    pub sync_type: SyncType,
}

/// Type of sync response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "sync_type", rename_all = "snake_case")]
pub enum SyncType {
    /// Full state sync.
    Full {
        /// All components.
        components: Vec<ComponentSpec>,
    },
    /// Incremental sync with deltas.
    Incremental {
        /// Deltas since last sequence.
        deltas: Vec<Delta>,
    },
    /// No changes needed.
    NoChange,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComponentType;

    #[test]
    fn test_delta_update_builder() {
        let canvas_id = CanvasId::new();
        let update = DeltaUpdate::new(canvas_id, 1)
            .insert(
                ComponentId::from_str("root"),
                0,
                ComponentSpec::text(ComponentId::from_str("text1"), "Hello"),
            )
            .update_props(
                ComponentId::from_str("btn1"),
                [("disabled".to_string(), serde_json::json!(true))]
                    .into_iter()
                    .collect(),
            );

        assert_eq!(update.deltas.len(), 2);
        assert_eq!(update.sequence, 1);
    }

    #[test]
    fn test_delta_serialization() {
        let delta = Delta::SetText {
            component_id: ComponentId::from_str("text1"),
            text: "Updated text".to_string(),
        };

        let json = serde_json::to_string(&delta).unwrap();
        assert!(json.contains("\"op\":\"set_text\""));
        assert!(json.contains("\"text\":\"Updated text\""));
    }

    #[test]
    fn test_sync_response() {
        let response = SyncResponse {
            canvas_id: CanvasId::new(),
            sequence: 5,
            sync_type: SyncType::NoChange,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"sync_type\":\"no_change\""));
    }
}
