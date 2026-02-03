//! Layout components.

use super::{BaseProps, Component};
use crate::Result;
use async_trait::async_trait;
use drbot_canvas_protocol::{
    ComponentId, ComponentSpec, ComponentType, LayoutDirection, LayoutSpec, StyleSpec,
};
use serde_json::Value;
use std::collections::HashMap;

/// Container component for layout.
pub struct ContainerComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    layout: LayoutSpec,
    children: Vec<Box<dyn Component>>,
}

impl ContainerComponent {
    /// Create a new container.
    pub fn new(id: ComponentId) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            layout: LayoutSpec::default(),
            children: Vec::new(),
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let mut container = Self::new(spec.id.clone());
        container.style = spec.style.clone();
        container.layout = spec.layout.clone().unwrap_or_default();

        for child_spec in &spec.children {
            let child = super::ComponentFactory::create(child_spec)?;
            container.children.push(child);
        }

        for (key, value) in &spec.props {
            container.base.set(key.clone(), value.clone());
        }

        Ok(container)
    }

    /// Set layout direction.
    pub fn direction(mut self, direction: LayoutDirection) -> Self {
        self.layout.direction = direction;
        self
    }

    /// Set gap.
    pub fn gap(mut self, gap: impl Into<String>) -> Self {
        self.layout.gap = Some(gap.into());
        self
    }

    /// Set style.
    pub fn style(mut self, style: StyleSpec) -> Self {
        self.style = Some(style);
        self
    }
}

#[async_trait]
impl Component for ContainerComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Container
    }

    fn render(&self) -> ComponentSpec {
        let children: Vec<ComponentSpec> = self.children.iter().map(|c| c.render()).collect();

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Container,
            props: self.base.props.clone(),
            style: self.style.clone(),
            layout: Some(self.layout.clone()),
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
        self.base.set(key, value);
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

/// Text component for displaying text.
pub struct TextComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    content: String,
}

impl TextComponent {
    /// Create a new text component.
    pub fn new(id: ComponentId, content: impl Into<String>) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            content: content.into(),
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let content = spec
            .props
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut text = Self::new(spec.id.clone(), content);
        text.style = spec.style.clone();

        for (key, value) in &spec.props {
            text.base.set(key.clone(), value.clone());
        }

        Ok(text)
    }

    /// Set the text content.
    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
    }

    /// Set style.
    pub fn style(mut self, style: StyleSpec) -> Self {
        self.style = Some(style);
        self
    }
}

#[async_trait]
impl Component for TextComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Text
    }

    fn render(&self) -> ComponentSpec {
        let mut props = self.base.props.clone();
        props.insert("content".to_string(), serde_json::json!(self.content));

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Text,
            props,
            style: self.style.clone(),
            layout: None,
            children: vec![],
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
        if key == "content" {
            if let Some(s) = value.as_str() {
                self.content = s.to_string();
            }
        } else {
            self.base.set(key, value);
        }
    }
}

/// Divider component.
pub struct DividerComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
}

impl DividerComponent {
    /// Create a new divider.
    pub fn new(id: ComponentId) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let mut divider = Self::new(spec.id.clone());
        divider.style = spec.style.clone();
        Ok(divider)
    }
}

#[async_trait]
impl Component for DividerComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Divider
    }

    fn render(&self) -> ComponentSpec {
        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Divider,
            props: self.base.props.clone(),
            style: self.style.clone(),
            layout: None,
            children: vec![],
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
        self.base.set(key, value);
    }
}

/// Spacer component.
pub struct SpacerComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
}

impl SpacerComponent {
    /// Create a new spacer.
    pub fn new(id: ComponentId) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let mut spacer = Self::new(spec.id.clone());
        spacer.style = spec.style.clone();
        Ok(spacer)
    }

    /// Set size.
    pub fn size(mut self, size: impl Into<String>) -> Self {
        self.style = Some(self.style.unwrap_or_default().height(size.into()));
        self
    }
}

#[async_trait]
impl Component for SpacerComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Spacer
    }

    fn render(&self) -> ComponentSpec {
        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Spacer,
            props: self.base.props.clone(),
            style: self.style.clone(),
            layout: None,
            children: vec![],
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
        self.base.set(key, value);
    }
}

/// Image component.
pub struct ImageComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    src: String,
    alt: Option<String>,
}

impl ImageComponent {
    /// Create a new image.
    pub fn new(id: ComponentId, src: impl Into<String>) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            src: src.into(),
            alt: None,
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let src = spec
            .props
            .get("src")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let alt = spec
            .props
            .get("alt")
            .and_then(|v| v.as_str())
            .map(String::from);

        let mut image = Self::new(spec.id.clone(), src);
        image.style = spec.style.clone();
        image.alt = alt;

        Ok(image)
    }

    /// Set alt text.
    pub fn alt(mut self, alt: impl Into<String>) -> Self {
        self.alt = Some(alt.into());
        self
    }
}

#[async_trait]
impl Component for ImageComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Image
    }

    fn render(&self) -> ComponentSpec {
        let mut props = self.base.props.clone();
        props.insert("src".to_string(), serde_json::json!(self.src));
        if let Some(ref alt) = self.alt {
            props.insert("alt".to_string(), serde_json::json!(alt));
        }

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Image,
            props,
            style: self.style.clone(),
            layout: None,
            children: vec![],
            events: vec!["click".to_string(), "load".to_string(), "error".to_string()],
        }
    }

    async fn handle_event(&mut self, _event_type: &str, _data: Value) -> Result<Option<Value>> {
        Ok(None)
    }

    fn props(&self) -> &HashMap<String, Value> {
        &self.base.props
    }

    fn set_prop(&mut self, key: &str, value: Value) {
        if key == "src" {
            if let Some(s) = value.as_str() {
                self.src = s.to_string();
            }
        } else if key == "alt" {
            self.alt = value.as_str().map(String::from);
        } else {
            self.base.set(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_component() {
        let container = ContainerComponent::new(ComponentId::from_str("root"))
            .direction(LayoutDirection::Column)
            .gap("16px");

        let spec = container.render();
        assert_eq!(spec.component_type, ComponentType::Container);
        assert!(spec.layout.is_some());
    }

    #[test]
    fn test_text_component() {
        let text = TextComponent::new(ComponentId::from_str("text1"), "Hello World");
        let spec = text.render();

        assert_eq!(spec.component_type, ComponentType::Text);
        assert_eq!(spec.props.get("content").unwrap(), "Hello World");
    }
}
