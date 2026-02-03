//! Canvas state management.

use crate::{CanvasError, Component, ComponentFactory, Result};
use drbot_canvas_protocol::{
    CanvasId, CanvasMeta, ComponentId, ComponentSpec, Delta, DeltaUpdate, SessionId,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Canvas state container.
pub struct CanvasState {
    /// Canvas metadata.
    pub meta: CanvasMeta,
    /// Root components.
    pub components: Vec<Box<dyn Component>>,
    /// Component index for quick lookup.
    component_index: HashMap<ComponentId, Vec<usize>>,
    /// Current sequence number for delta updates.
    pub sequence: u64,
}

impl CanvasState {
    /// Create a new canvas state.
    pub fn new(name: impl Into<String>, session_id: SessionId) -> Self {
        Self {
            meta: CanvasMeta::new(name, session_id),
            components: Vec::new(),
            component_index: HashMap::new(),
            sequence: 0,
        }
    }

    /// Get the canvas ID.
    pub fn id(&self) -> CanvasId {
        self.meta.id
    }

    /// Add a component.
    pub fn add_component(&mut self, component: Box<dyn Component>) {
        let id = component.id().clone();
        let index = self.components.len();
        self.components.push(component);
        self.component_index.insert(id, vec![index]);
        self.sequence += 1;
        self.meta.updated_at = chrono::Utc::now();
    }

    /// Remove a component by ID.
    pub fn remove_component(&mut self, id: &ComponentId) -> Option<Box<dyn Component>> {
        if let Some(indices) = self.component_index.remove(id) {
            if let Some(&index) = indices.first() {
                if index < self.components.len() {
                    self.sequence += 1;
                    self.meta.updated_at = chrono::Utc::now();
                    return Some(self.components.remove(index));
                }
            }
        }
        None
    }

    /// Get a component by ID.
    pub fn get_component(&self, id: &ComponentId) -> Option<&dyn Component> {
        Self::find_component_recursive(&self.components, id)
    }

    /// Get a mutable component by ID.
    pub fn get_component_mut(&mut self, id: &ComponentId) -> Option<&mut dyn Component> {
        Self::find_component_mut_recursive(&mut self.components, id)
    }

    fn find_component_recursive<'a>(
        components: &'a [Box<dyn Component>],
        id: &ComponentId,
    ) -> Option<&'a dyn Component> {
        for component in components {
            if component.id() == id {
                return Some(component.as_ref());
            }
            if let Some(found) = Self::find_component_recursive(component.children(), id) {
                return Some(found);
            }
        }
        None
    }

    fn find_component_mut_recursive<'a>(
        components: &'a mut [Box<dyn Component>],
        id: &ComponentId,
    ) -> Option<&'a mut dyn Component> {
        for component in components.iter_mut() {
            if component.id() == id {
                return Some(component.as_mut());
            }
            // Note: Can't recurse into children mutably without unsafe
            // This is a simplified implementation
        }
        None
    }

    /// Render all components to specifications.
    pub fn render(&self) -> Vec<ComponentSpec> {
        self.components.iter().map(|c| c.render()).collect()
    }

    /// Apply a delta update.
    pub fn apply_delta(&mut self, delta: &Delta) -> Result<()> {
        match delta {
            Delta::Insert {
                parent_id,
                index,
                component,
            } => {
                let new_component = ComponentFactory::create(component)?;
                if parent_id.0 == "root" {
                    let idx = (*index).min(self.components.len());
                    self.components.insert(idx, new_component);
                } else {
                    // Find parent and insert
                    if let Some(parent) = self.get_component_mut(parent_id) {
                        parent.add_child(new_component);
                    } else {
                        return Err(CanvasError::ComponentNotFound(parent_id.clone()));
                    }
                }
            }
            Delta::Remove { component_id } => {
                self.remove_component(component_id);
            }
            Delta::UpdateProps {
                component_id,
                props,
            } => {
                if let Some(component) = self.get_component_mut(component_id) {
                    for (key, value) in props {
                        component.set_prop(key, value.clone());
                    }
                } else {
                    return Err(CanvasError::ComponentNotFound(component_id.clone()));
                }
            }
            Delta::Replace {
                component_id,
                component,
            } => {
                self.remove_component(component_id);
                let new_component = ComponentFactory::create(component)?;
                self.add_component(new_component);
            }
            Delta::SetText { component_id, text } => {
                if let Some(component) = self.get_component_mut(component_id) {
                    component.set_prop("content", serde_json::json!(text));
                } else {
                    return Err(CanvasError::ComponentNotFound(component_id.clone()));
                }
            }
            Delta::SetValue {
                component_id,
                value,
            } => {
                if let Some(component) = self.get_component_mut(component_id) {
                    component.set_prop("value", value.clone());
                } else {
                    return Err(CanvasError::ComponentNotFound(component_id.clone()));
                }
            }
            _ => {
                // Handle other delta types as needed
            }
        }

        self.sequence += 1;
        self.meta.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Apply a batch of deltas.
    pub fn apply_deltas(&mut self, update: &DeltaUpdate) -> Result<()> {
        for delta in &update.deltas {
            self.apply_delta(delta)?;
        }
        Ok(())
    }

    /// Clear all components.
    pub fn clear(&mut self) {
        self.components.clear();
        self.component_index.clear();
        self.sequence += 1;
        self.meta.updated_at = chrono::Utc::now();
    }
}

/// Canvas state manager for multiple canvases.
pub struct CanvasStateManager {
    canvases: Arc<RwLock<HashMap<CanvasId, CanvasState>>>,
}

impl CanvasStateManager {
    /// Create a new state manager.
    pub fn new() -> Self {
        Self {
            canvases: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new canvas.
    pub async fn create_canvas(&self, name: impl Into<String>, session_id: SessionId) -> CanvasId {
        let state = CanvasState::new(name, session_id);
        let id = state.id();
        self.canvases.write().await.insert(id, state);
        id
    }

    /// Get a canvas by ID.
    pub async fn get_canvas(&self, id: &CanvasId) -> Option<CanvasState> {
        // Note: In a real implementation, we'd return a reference or clone
        self.canvases.read().await.get(id).map(|s| {
            CanvasState {
                meta: s.meta.clone(),
                components: Vec::new(), // Simplified
                component_index: HashMap::new(),
                sequence: s.sequence,
            }
        })
    }

    /// Render a canvas.
    pub async fn render(&self, id: &CanvasId) -> Result<Vec<ComponentSpec>> {
        let canvases = self.canvases.read().await;
        let canvas = canvases
            .get(id)
            .ok_or_else(|| CanvasError::CanvasNotFound(*id))?;
        Ok(canvas.render())
    }

    /// Apply deltas to a canvas.
    pub async fn apply_deltas(&self, update: &DeltaUpdate) -> Result<()> {
        let mut canvases = self.canvases.write().await;
        let canvas = canvases
            .get_mut(&update.canvas_id)
            .ok_or_else(|| CanvasError::CanvasNotFound(update.canvas_id))?;
        canvas.apply_deltas(update)
    }

    /// Destroy a canvas.
    pub async fn destroy(&self, id: &CanvasId) -> bool {
        self.canvases.write().await.remove(id).is_some()
    }

    /// List all canvas IDs for a session.
    pub async fn list_for_session(&self, session_id: &SessionId) -> Vec<CanvasId> {
        self.canvases
            .read()
            .await
            .values()
            .filter(|c| &c.meta.session_id == session_id)
            .map(|c| c.meta.id)
            .collect()
    }
}

impl Default for CanvasStateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::TextComponent;

    #[test]
    fn test_canvas_state() {
        let mut state = CanvasState::new("Test Canvas", SessionId::new());

        let text = TextComponent::new(ComponentId::from_str("text1"), "Hello");
        state.add_component(Box::new(text));

        assert_eq!(state.components.len(), 1);
        assert!(state
            .get_component(&ComponentId::from_str("text1"))
            .is_some());
    }

    #[tokio::test]
    async fn test_canvas_state_manager() {
        let manager = CanvasStateManager::new();
        let session = SessionId::new();

        let id = manager.create_canvas("Test", session.clone()).await;

        let canvases = manager.list_for_session(&session).await;
        assert_eq!(canvases.len(), 1);
        assert_eq!(canvases[0], id);
    }
}
