//! Canvas wire protocol definitions for drbot.
//!
//! This crate defines the protocol types used for canvas communication
//! between the backend and frontend.

mod actions;
mod components;
mod updates;

pub use actions::*;
pub use components::*;
pub use updates::*;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canvas protocol errors.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("Invalid component type: {0}")]
    InvalidComponentType(String),
    #[error("Invalid action: {0}")]
    InvalidAction(String),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Result type for protocol operations.
pub type Result<T> = std::result::Result<T, ProtocolError>;

/// Canvas identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CanvasId(pub Uuid);

impl CanvasId {
    /// Create a new canvas ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CanvasId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CanvasId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Component identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComponentId(pub String);

impl ComponentId {
    /// Create a new component ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from string.
    pub fn from_str(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl Default for ComponentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ComponentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Session identifier for canvas ownership.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    /// Create a new session ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Canvas metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasMeta {
    /// Canvas ID.
    pub id: CanvasId,
    /// Canvas name.
    pub name: String,
    /// Session that owns this canvas.
    pub session_id: SessionId,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last update timestamp.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl CanvasMeta {
    /// Create new canvas metadata.
    pub fn new(name: impl Into<String>, session_id: SessionId) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: CanvasId::new(),
            name: name.into(),
            session_id,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Protocol version.
pub const PROTOCOL_VERSION: &str = "1.0.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_id() {
        let id1 = CanvasId::new();
        let id2 = CanvasId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_component_id() {
        let id = ComponentId::new();
        assert!(!id.0.is_empty());
    }

    #[test]
    fn test_canvas_meta() {
        let session = SessionId::new();
        let meta = CanvasMeta::new("Test Canvas", session.clone());
        assert_eq!(meta.name, "Test Canvas");
        assert_eq!(meta.session_id, session);
    }
}
