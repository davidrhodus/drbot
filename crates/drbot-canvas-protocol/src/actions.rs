//! Canvas action definitions.

use crate::{CanvasId, ComponentId, ComponentSpec};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Canvas action - operations that can be performed on a canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CanvasAction {
    /// Create a new canvas.
    Create(CreateCanvasParams),
    /// Render components to a canvas.
    Render(RenderParams),
    /// Update specific components.
    Update(UpdateParams),
    /// Remove components.
    Remove(RemoveParams),
    /// Clear all components.
    Clear(ClearParams),
    /// Destroy the canvas.
    Destroy(DestroyParams),
    /// Set canvas properties.
    SetProperty(SetPropertyParams),
}

/// Parameters for creating a canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCanvasParams {
    /// Canvas name.
    pub name: String,
    /// Initial width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
    /// Initial height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<String>,
    /// Initial components.
    #[serde(default)]
    pub components: Vec<ComponentSpec>,
}

/// Parameters for rendering components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderParams {
    /// Canvas ID.
    pub canvas_id: CanvasId,
    /// Components to render.
    pub components: Vec<ComponentSpec>,
    /// Replace existing components.
    #[serde(default)]
    pub replace: bool,
}

/// Parameters for updating components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateParams {
    /// Canvas ID.
    pub canvas_id: CanvasId,
    /// Component updates.
    pub updates: Vec<ComponentUpdate>,
}

/// Single component update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentUpdate {
    /// Component ID to update.
    pub id: ComponentId,
    /// Update type.
    #[serde(flatten)]
    pub update: UpdateType,
}

/// Type of update to apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "update_type", rename_all = "snake_case")]
pub enum UpdateType {
    /// Set properties.
    SetProps {
        props: HashMap<String, serde_json::Value>,
    },
    /// Merge properties.
    MergeProps {
        props: HashMap<String, serde_json::Value>,
    },
    /// Replace component.
    Replace { spec: ComponentSpec },
    /// Set visibility.
    SetVisibility { visible: bool },
    /// Set enabled state.
    SetEnabled { enabled: bool },
    /// Set loading state.
    SetLoading { loading: bool },
    /// Append children.
    AppendChildren { children: Vec<ComponentSpec> },
    /// Remove children.
    RemoveChildren { child_ids: Vec<ComponentId> },
}

/// Parameters for removing components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveParams {
    /// Canvas ID.
    pub canvas_id: CanvasId,
    /// Component IDs to remove.
    pub component_ids: Vec<ComponentId>,
}

/// Parameters for clearing a canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearParams {
    /// Canvas ID.
    pub canvas_id: CanvasId,
}

/// Parameters for destroying a canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestroyParams {
    /// Canvas ID.
    pub canvas_id: CanvasId,
}

/// Parameters for setting canvas properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetPropertyParams {
    /// Canvas ID.
    pub canvas_id: CanvasId,
    /// Property name.
    pub property: String,
    /// Property value.
    pub value: serde_json::Value,
}

/// Canvas event - events emitted from the canvas.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CanvasEvent {
    /// Canvas was created.
    Created { canvas_id: CanvasId, name: String },
    /// Canvas was updated.
    Updated { canvas_id: CanvasId },
    /// Canvas was destroyed.
    Destroyed { canvas_id: CanvasId },
    /// Component interaction.
    ComponentEvent {
        canvas_id: CanvasId,
        component_id: ComponentId,
        event_type: String,
        data: serde_json::Value,
    },
    /// Form submitted.
    FormSubmit {
        canvas_id: CanvasId,
        form_id: ComponentId,
        values: HashMap<String, serde_json::Value>,
    },
    /// Error occurred.
    Error {
        canvas_id: Option<CanvasId>,
        message: String,
    },
}

/// Action result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Whether the action succeeded.
    pub success: bool,
    /// Canvas ID (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canvas_id: Option<CanvasId>,
    /// Error message (if failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Additional data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl ActionResult {
    /// Create a success result.
    pub fn success() -> Self {
        Self {
            success: true,
            canvas_id: None,
            error: None,
            data: None,
        }
    }

    /// Create a success result with canvas ID.
    pub fn success_with_canvas(canvas_id: CanvasId) -> Self {
        Self {
            success: true,
            canvas_id: Some(canvas_id),
            error: None,
            data: None,
        }
    }

    /// Create an error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            canvas_id: None,
            error: Some(message.into()),
            data: None,
        }
    }

    /// Add data to the result.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_serialization() {
        let action = CanvasAction::Create(CreateCanvasParams {
            name: "Test Canvas".to_string(),
            width: Some("800px".to_string()),
            height: None,
            components: vec![],
        });

        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"action\":\"create\""));
        assert!(json.contains("\"name\":\"Test Canvas\""));
    }

    #[test]
    fn test_event_serialization() {
        let event = CanvasEvent::ComponentEvent {
            canvas_id: CanvasId::new(),
            component_id: ComponentId::from_str("btn1"),
            event_type: "click".to_string(),
            data: serde_json::json!({"x": 100, "y": 200}),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"component_event\""));
        assert!(json.contains("\"event_type\":\"click\""));
    }

    #[test]
    fn test_action_result() {
        let result = ActionResult::success_with_canvas(CanvasId::new())
            .with_data(serde_json::json!({"rendered": 5}));

        assert!(result.success);
        assert!(result.canvas_id.is_some());
        assert!(result.data.is_some());
    }
}
