//! Card component.

use super::{BaseProps, Component};
use crate::Result;
use async_trait::async_trait;
use drbot_canvas_protocol::{ComponentId, ComponentSpec, ComponentType, StyleSpec};
use serde_json::Value;
use std::collections::HashMap;

/// Card component for grouping content.
pub struct CardComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    title: Option<String>,
    children: Vec<Box<dyn Component>>,
}

impl CardComponent {
    /// Create a new card.
    pub fn new(id: ComponentId) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            title: None,
            children: Vec::new(),
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let title = spec
            .props
            .get("title")
            .and_then(|v| v.as_str())
            .map(String::from);

        let mut card = Self::new(spec.id.clone());
        card.style = spec.style.clone();
        card.title = title;

        for child_spec in &spec.children {
            let child = super::ComponentFactory::create(child_spec)?;
            card.children.push(child);
        }

        for (key, value) in &spec.props {
            card.base.set(key.clone(), value.clone());
        }

        Ok(card)
    }

    /// Set the card title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set style.
    pub fn style(mut self, style: StyleSpec) -> Self {
        self.style = Some(style);
        self
    }
}

#[async_trait]
impl Component for CardComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Card
    }

    fn render(&self) -> ComponentSpec {
        let mut props = self.base.props.clone();
        if let Some(ref title) = self.title {
            props.insert("title".to_string(), serde_json::json!(title));
        }

        let children: Vec<ComponentSpec> = self.children.iter().map(|c| c.render()).collect();

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Card,
            props,
            style: self.style.clone(),
            layout: None,
            children,
            events: vec![],
        }
    }

    async fn handle_event(&mut self, _event_type: &str, _data: Value) -> Result<Option<Value>> {
        Ok(None)
    }

    fn props(&self) -> &HashMap<String, Value> {
        &self.base.props
    }

    fn set_prop(&mut self, key: &str, value: Value) {
        if key == "title" {
            self.title = value.as_str().map(String::from);
        } else {
            self.base.set(key, value);
        }
    }

    fn children(&self) -> &[Box<dyn Component>] {
        &self.children
    }

    fn add_child(&mut self, child: Box<dyn Component>) {
        self.children.push(child);
    }

    fn remove_child(&mut self, id: &ComponentId) -> Option<Box<dyn Component>> {
        if let Some(pos) = self.children.iter().position(|c| c.id() == id) {
            Some(self.children.remove(pos))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_component() {
        let card = CardComponent::new(ComponentId::from_str("card1")).title("My Card");

        let spec = card.render();
        assert_eq!(spec.component_type, ComponentType::Card);
        assert_eq!(spec.props.get("title").unwrap(), "My Card");
    }
}
