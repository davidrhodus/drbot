//! Visual canvas/A2UI system for drbot.
//!
//! This crate provides a component-based UI system for creating
//! interactive visual interfaces.

pub mod components;
mod events;
mod render;
mod state;
mod workspace;

pub use components::*;
pub use events::*;
pub use render::*;
pub use state::*;
pub use workspace::*;

// Re-export protocol types
pub use drbot_canvas_protocol::{
    CanvasAction, CanvasEvent, CanvasId, CanvasMeta, ComponentId, ComponentSpec, ComponentType,
    ComponentUpdate, Delta, DeltaUpdate, LayoutDirection, LayoutSpec, SessionId, StyleSpec,
    UpdateType,
};

/// Canvas errors.
#[derive(Debug, thiserror::Error)]
pub enum CanvasError {
    #[error("Canvas not found: {0}")]
    CanvasNotFound(CanvasId),
    #[error("Component not found: {0}")]
    ComponentNotFound(ComponentId),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
    #[error("Render error: {0}")]
    RenderError(String),
    #[error("Protocol error: {0}")]
    ProtocolError(#[from] drbot_canvas_protocol::ProtocolError),
}

/// Result type for canvas operations.
pub type Result<T> = std::result::Result<T, CanvasError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reexports() {
        // Verify protocol types are re-exported
        let _id = CanvasId::new();
        let _component_id = ComponentId::new();
    }
}
