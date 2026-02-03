//! Canvas workspace management.

use crate::{CanvasError, CanvasState, CanvasStateManager, EventDispatcher, Result};
use drbot_canvas_protocol::{CanvasId, ComponentSpec, SessionId};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A workspace containing multiple canvases for a session.
pub struct Workspace {
    /// Session ID.
    session_id: SessionId,
    /// Workspace name.
    name: String,
    /// Active canvas ID.
    active_canvas: Option<CanvasId>,
    /// Canvas states.
    canvases: HashMap<CanvasId, CanvasState>,
    /// Event dispatcher.
    events: Arc<EventDispatcher>,
}

impl Workspace {
    /// Create a new workspace.
    pub fn new(session_id: SessionId, name: impl Into<String>) -> Self {
        Self {
            session_id,
            name: name.into(),
            active_canvas: None,
            canvases: HashMap::new(),
            events: Arc::new(EventDispatcher::new()),
        }
    }

    /// Get the session ID.
    pub fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Get the workspace name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Create a new canvas in the workspace.
    pub fn create_canvas(&mut self, name: impl Into<String>) -> CanvasId {
        let state = CanvasState::new(name, self.session_id.clone());
        let id = state.id();
        self.canvases.insert(id, state);

        // Set as active if first canvas
        if self.active_canvas.is_none() {
            self.active_canvas = Some(id);
        }

        id
    }

    /// Get a canvas by ID.
    pub fn get_canvas(&self, id: &CanvasId) -> Option<&CanvasState> {
        self.canvases.get(id)
    }

    /// Get a mutable canvas by ID.
    pub fn get_canvas_mut(&mut self, id: &CanvasId) -> Option<&mut CanvasState> {
        self.canvases.get_mut(id)
    }

    /// Get the active canvas.
    pub fn active_canvas(&self) -> Option<&CanvasState> {
        self.active_canvas
            .as_ref()
            .and_then(|id| self.canvases.get(id))
    }

    /// Get the active canvas ID.
    pub fn active_canvas_id(&self) -> Option<CanvasId> {
        self.active_canvas
    }

    /// Set the active canvas.
    pub fn set_active_canvas(&mut self, id: CanvasId) -> Result<()> {
        if self.canvases.contains_key(&id) {
            self.active_canvas = Some(id);
            Ok(())
        } else {
            Err(CanvasError::CanvasNotFound(id))
        }
    }

    /// Destroy a canvas.
    pub fn destroy_canvas(&mut self, id: &CanvasId) -> bool {
        let removed = self.canvases.remove(id).is_some();

        // Clear active if it was the destroyed canvas
        if self.active_canvas == Some(*id) {
            self.active_canvas = self.canvases.keys().next().copied();
        }

        removed
    }

    /// List all canvas IDs.
    pub fn list_canvases(&self) -> Vec<CanvasId> {
        self.canvases.keys().copied().collect()
    }

    /// Get the event dispatcher.
    pub fn events(&self) -> Arc<EventDispatcher> {
        self.events.clone()
    }

    /// Render all canvases.
    pub fn render_all(&self) -> HashMap<CanvasId, Vec<ComponentSpec>> {
        self.canvases
            .iter()
            .map(|(id, state)| (*id, state.render()))
            .collect()
    }
}

/// Workspace manager for multiple sessions.
pub struct WorkspaceManager {
    workspaces: Arc<RwLock<HashMap<SessionId, Workspace>>>,
    canvas_manager: Arc<CanvasStateManager>,
}

impl WorkspaceManager {
    /// Create a new workspace manager.
    pub fn new() -> Self {
        Self {
            workspaces: Arc::new(RwLock::new(HashMap::new())),
            canvas_manager: Arc::new(CanvasStateManager::new()),
        }
    }

    /// Get or create a workspace for a session.
    pub async fn get_or_create(&self, session_id: SessionId) -> SessionId {
        let mut workspaces = self.workspaces.write().await;
        if !workspaces.contains_key(&session_id) {
            let workspace = Workspace::new(session_id.clone(), "Default");
            workspaces.insert(session_id.clone(), workspace);
        }
        session_id
    }

    /// Get a workspace by session ID.
    pub async fn get(&self, session_id: &SessionId) -> Option<Workspace> {
        // Note: In a real implementation, we'd return a reference
        let workspaces = self.workspaces.read().await;
        workspaces.get(session_id).map(|w| {
            Workspace {
                session_id: w.session_id.clone(),
                name: w.name.clone(),
                active_canvas: w.active_canvas,
                canvases: HashMap::new(), // Simplified
                events: w.events.clone(),
            }
        })
    }

    /// Create a canvas in a session's workspace.
    pub async fn create_canvas(
        &self,
        session_id: &SessionId,
        name: impl Into<String>,
    ) -> Result<CanvasId> {
        let mut workspaces = self.workspaces.write().await;
        let workspace = workspaces
            .get_mut(session_id)
            .ok_or_else(|| CanvasError::InvalidOperation("Session not found".to_string()))?;

        Ok(workspace.create_canvas(name))
    }

    /// Destroy a session's workspace.
    pub async fn destroy_workspace(&self, session_id: &SessionId) -> bool {
        self.workspaces.write().await.remove(session_id).is_some()
    }

    /// Get the underlying canvas manager.
    pub fn canvas_manager(&self) -> Arc<CanvasStateManager> {
        self.canvas_manager.clone()
    }
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace() {
        let session = SessionId::new();
        let mut workspace = Workspace::new(session, "Test Workspace");

        let canvas_id = workspace.create_canvas("Canvas 1");
        assert!(workspace.get_canvas(&canvas_id).is_some());
        assert_eq!(workspace.active_canvas_id(), Some(canvas_id));

        let canvas2_id = workspace.create_canvas("Canvas 2");
        workspace.set_active_canvas(canvas2_id).unwrap();
        assert_eq!(workspace.active_canvas_id(), Some(canvas2_id));

        assert!(workspace.destroy_canvas(&canvas2_id));
        assert_eq!(workspace.active_canvas_id(), Some(canvas_id));
    }

    #[tokio::test]
    async fn test_workspace_manager() {
        let manager = WorkspaceManager::new();
        let session = SessionId::new();

        manager.get_or_create(session.clone()).await;

        let canvas_id = manager
            .create_canvas(&session, "Test Canvas")
            .await
            .unwrap();

        let workspace = manager.get(&session).await;
        assert!(workspace.is_some());
    }
}
