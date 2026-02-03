//! Canvas component implementations.

mod card;
mod chart;
mod form;
mod interactive;
mod layout;
mod table;

pub use card::*;
pub use chart::*;
pub use form::*;
pub use interactive::*;
pub use layout::*;
pub use table::*;

use crate::Result;
use async_trait::async_trait;
use drbot_canvas_protocol::{ComponentId, ComponentSpec, ComponentType};
use serde_json::Value;
use std::collections::HashMap;

/// Component trait - base interface for all canvas components.
#[async_trait]
pub trait Component: Send + Sync {
    /// Get the component ID.
    fn id(&self) -> &ComponentId;

    /// Get the component type.
    fn component_type(&self) -> ComponentType;

    /// Render the component to a specification.
    fn render(&self) -> ComponentSpec;

    /// Handle an event.
    async fn handle_event(&mut self, event_type: &str, data: Value) -> Result<Option<Value>>;

    /// Get component properties.
    fn props(&self) -> &HashMap<String, Value>;

    /// Set a property.
    fn set_prop(&mut self, key: &str, value: Value);

    /// Get child components.
    fn children(&self) -> &[Box<dyn Component>] {
        &[]
    }

    /// Add a child component.
    fn add_child(&mut self, _child: Box<dyn Component>) {
        // Default: no-op for non-container components
    }

    /// Remove a child component.
    fn remove_child(&mut self, _id: &ComponentId) -> Option<Box<dyn Component>> {
        None
    }
}

/// Base component properties shared by all components.
#[derive(Debug, Clone, Default)]
pub struct BaseProps {
    /// Component ID.
    pub id: ComponentId,
    /// Custom properties.
    pub props: HashMap<String, Value>,
    /// Visibility.
    pub visible: bool,
    /// Enabled state.
    pub enabled: bool,
    /// Loading state.
    pub loading: bool,
}

impl BaseProps {
    /// Create new base props with an ID.
    pub fn new(id: ComponentId) -> Self {
        Self {
            id,
            props: HashMap::new(),
            visible: true,
            enabled: true,
            loading: false,
        }
    }

    /// Set a property.
    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        self.props.insert(key.into(), value);
    }

    /// Get a property.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.props.get(key)
    }

    /// Get a string property.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.props.get(key).and_then(|v| v.as_str())
    }

    /// Get a bool property.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.props.get(key).and_then(|v| v.as_bool())
    }
}

/// Component factory for creating components from specs.
pub struct ComponentFactory;

impl ComponentFactory {
    /// Create a component from a specification.
    pub fn create(spec: &ComponentSpec) -> Result<Box<dyn Component>> {
        match &spec.component_type {
            ComponentType::Container => Ok(Box::new(ContainerComponent::from_spec(spec)?)),
            ComponentType::Card => Ok(Box::new(CardComponent::from_spec(spec)?)),
            ComponentType::Text => Ok(Box::new(TextComponent::from_spec(spec)?)),
            ComponentType::Button => Ok(Box::new(ButtonComponent::from_spec(spec)?)),
            ComponentType::Input => Ok(Box::new(InputComponent::from_spec(spec)?)),
            ComponentType::Form => Ok(Box::new(FormComponent::from_spec(spec)?)),
            ComponentType::Table => Ok(Box::new(TableComponent::from_spec(spec)?)),
            ComponentType::Chart => Ok(Box::new(ChartComponent::from_spec(spec)?)),
            ComponentType::Toggle => Ok(Box::new(ToggleComponent::from_spec(spec)?)),
            ComponentType::Select => Ok(Box::new(SelectComponent::from_spec(spec)?)),
            ComponentType::Progress => Ok(Box::new(ProgressComponent::from_spec(spec)?)),
            ComponentType::Divider => Ok(Box::new(DividerComponent::from_spec(spec)?)),
            ComponentType::Spacer => Ok(Box::new(SpacerComponent::from_spec(spec)?)),
            ComponentType::Image => Ok(Box::new(ImageComponent::from_spec(spec)?)),
            ComponentType::Custom(name) => Err(crate::CanvasError::InvalidOperation(format!(
                "Custom component type '{}' not supported by factory",
                name
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_props() {
        let mut props = BaseProps::new(ComponentId::from_str("test"));
        props.set("foo", serde_json::json!("bar"));

        assert_eq!(props.get_str("foo"), Some("bar"));
        assert!(props.visible);
        assert!(props.enabled);
    }
}
