//! Canvas rendering.

use crate::{CanvasState, Component, Result};
use drbot_canvas_protocol::{CanvasId, ComponentSpec, Delta, DeltaUpdate};
use std::collections::HashMap;

/// Renderer for producing component output.
pub struct CanvasRenderer;

impl CanvasRenderer {
    /// Render a canvas state to component specifications.
    pub fn render(state: &CanvasState) -> Vec<ComponentSpec> {
        state.components.iter().map(|c| c.render()).collect()
    }

    /// Render a single component tree.
    pub fn render_component(component: &dyn Component) -> ComponentSpec {
        component.render()
    }

    /// Compute delta updates between two states.
    pub fn diff(old: &[ComponentSpec], new: &[ComponentSpec]) -> Vec<Delta> {
        let mut deltas = Vec::new();

        // Build index of old components
        let old_index: HashMap<&String, (usize, &ComponentSpec)> = old
            .iter()
            .enumerate()
            .map(|(i, spec)| (&spec.id.0, (i, spec)))
            .collect();

        // Build index of new components
        let new_index: HashMap<&String, (usize, &ComponentSpec)> = new
            .iter()
            .enumerate()
            .map(|(i, spec)| (&spec.id.0, (i, spec)))
            .collect();

        // Find removed components
        for (id, _) in &old_index {
            if !new_index.contains_key(id) {
                deltas.push(Delta::Remove {
                    component_id: drbot_canvas_protocol::ComponentId((*id).clone()),
                });
            }
        }

        // Find added and updated components
        for (id, (new_idx, new_spec)) in &new_index {
            if let Some((old_idx, old_spec)) = old_index.get(id) {
                // Component exists - check for changes
                if Self::specs_differ(old_spec, new_spec) {
                    // Props changed
                    if old_spec.props != new_spec.props {
                        deltas.push(Delta::UpdateProps {
                            component_id: new_spec.id.clone(),
                            props: new_spec.props.clone(),
                        });
                    }
                    // Check for child changes recursively
                    let child_deltas = Self::diff(&old_spec.children, &new_spec.children);
                    deltas.extend(child_deltas);
                }
                // Check for position change
                if old_idx != new_idx {
                    // Would need a Move delta if positions changed
                }
            } else {
                // New component
                deltas.push(Delta::Insert {
                    parent_id: drbot_canvas_protocol::ComponentId("root".to_string()),
                    index: *new_idx,
                    component: (*new_spec).clone(),
                });
            }
        }

        deltas
    }

    /// Check if two component specs differ.
    fn specs_differ(a: &ComponentSpec, b: &ComponentSpec) -> bool {
        a.component_type != b.component_type
            || a.props != b.props
            || a.style != b.style
            || a.layout != b.layout
            || a.children.len() != b.children.len()
    }

    /// Create a delta update from differences.
    pub fn create_delta_update(
        canvas_id: CanvasId,
        sequence: u64,
        old: &[ComponentSpec],
        new: &[ComponentSpec],
    ) -> DeltaUpdate {
        let deltas = Self::diff(old, new);
        DeltaUpdate {
            canvas_id,
            sequence,
            deltas,
        }
    }
}

/// Render options.
#[derive(Debug, Clone, Default)]
pub struct RenderOptions {
    /// Include component IDs in output.
    pub include_ids: bool,
    /// Include style information.
    pub include_styles: bool,
    /// Include event handlers.
    pub include_events: bool,
    /// Pretty print output.
    pub pretty: bool,
}

impl RenderOptions {
    /// Create default options.
    pub fn new() -> Self {
        Self {
            include_ids: true,
            include_styles: true,
            include_events: true,
            pretty: false,
        }
    }

    /// Enable pretty printing.
    pub fn pretty(mut self) -> Self {
        self.pretty = true;
        self
    }
}

/// JSON renderer for component specs.
pub struct JsonRenderer;

impl JsonRenderer {
    /// Render components to JSON.
    pub fn render(specs: &[ComponentSpec], options: &RenderOptions) -> Result<String> {
        let json = if options.pretty {
            serde_json::to_string_pretty(specs)
        } else {
            serde_json::to_string(specs)
        };

        json.map_err(|e| crate::CanvasError::RenderError(e.to_string()))
    }

    /// Render a single component to JSON.
    pub fn render_one(spec: &ComponentSpec, options: &RenderOptions) -> Result<String> {
        let json = if options.pretty {
            serde_json::to_string_pretty(spec)
        } else {
            serde_json::to_string(spec)
        };

        json.map_err(|e| crate::CanvasError::RenderError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drbot_canvas_protocol::{ComponentId, ComponentType};

    #[test]
    fn test_diff_empty() {
        let old: Vec<ComponentSpec> = vec![];
        let new: Vec<ComponentSpec> = vec![];
        let deltas = CanvasRenderer::diff(&old, &new);
        assert!(deltas.is_empty());
    }

    #[test]
    fn test_diff_add() {
        let old: Vec<ComponentSpec> = vec![];
        let new = vec![ComponentSpec::text(ComponentId::from_str("text1"), "Hello")];

        let deltas = CanvasRenderer::diff(&old, &new);
        assert_eq!(deltas.len(), 1);
        assert!(matches!(deltas[0], Delta::Insert { .. }));
    }

    #[test]
    fn test_diff_remove() {
        let old = vec![ComponentSpec::text(ComponentId::from_str("text1"), "Hello")];
        let new: Vec<ComponentSpec> = vec![];

        let deltas = CanvasRenderer::diff(&old, &new);
        assert_eq!(deltas.len(), 1);
        assert!(matches!(deltas[0], Delta::Remove { .. }));
    }

    #[test]
    fn test_json_renderer() {
        let specs = vec![ComponentSpec::text(ComponentId::from_str("text1"), "Hello")];
        let options = RenderOptions::new();

        let json = JsonRenderer::render(&specs, &options).unwrap();
        assert!(json.contains("text"));
        assert!(json.contains("Hello"));
    }
}
