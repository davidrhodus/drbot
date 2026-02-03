//! Interactive components (buttons, toggles, etc.).

use super::{BaseProps, Component};
use crate::Result;
use async_trait::async_trait;
use drbot_canvas_protocol::{ComponentId, ComponentSpec, ComponentType, StyleSpec};
use serde_json::Value;
use std::collections::HashMap;

/// Button component.
pub struct ButtonComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    label: String,
    variant: String,
}

impl ButtonComponent {
    /// Create a new button.
    pub fn new(id: ComponentId, label: impl Into<String>) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            label: label.into(),
            variant: "default".to_string(),
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let label = spec
            .props
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("Button")
            .to_string();

        let variant = spec
            .props
            .get("variant")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();

        let mut button = Self::new(spec.id.clone(), label);
        button.style = spec.style.clone();
        button.variant = variant;

        for (key, value) in &spec.props {
            button.base.set(key.clone(), value.clone());
        }

        Ok(button)
    }

    /// Set the button variant.
    pub fn variant(mut self, variant: impl Into<String>) -> Self {
        self.variant = variant.into();
        self
    }

    /// Set style.
    pub fn style(mut self, style: StyleSpec) -> Self {
        self.style = Some(style);
        self
    }
}

#[async_trait]
impl Component for ButtonComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Button
    }

    fn render(&self) -> ComponentSpec {
        let mut props = self.base.props.clone();
        props.insert("label".to_string(), serde_json::json!(self.label));
        props.insert("variant".to_string(), serde_json::json!(self.variant));
        props.insert(
            "disabled".to_string(),
            serde_json::json!(!self.base.enabled),
        );
        props.insert("loading".to_string(), serde_json::json!(self.base.loading));

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Button,
            props,
            style: self.style.clone(),
            layout: None,
            children: vec![],
            events: vec!["click".to_string()],
        }
    }

    async fn handle_event(&mut self, event_type: &str, data: Value) -> Result<Option<Value>> {
        match event_type {
            "click" => {
                if self.base.enabled && !self.base.loading {
                    Ok(Some(serde_json::json!({
                        "clicked": true,
                        "button_id": self.base.id.0,
                        "data": data
                    })))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    fn props(&self) -> &HashMap<String, Value> {
        &self.base.props
    }

    fn set_prop(&mut self, key: &str, value: Value) {
        match key {
            "label" => {
                if let Some(s) = value.as_str() {
                    self.label = s.to_string();
                }
            }
            "variant" => {
                if let Some(s) = value.as_str() {
                    self.variant = s.to_string();
                }
            }
            "disabled" => {
                if let Some(b) = value.as_bool() {
                    self.base.enabled = !b;
                }
            }
            "loading" => {
                if let Some(b) = value.as_bool() {
                    self.base.loading = b;
                }
            }
            _ => {
                self.base.set(key, value);
            }
        }
    }
}

/// Toggle/switch component.
pub struct ToggleComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    checked: bool,
    label: Option<String>,
}

impl ToggleComponent {
    /// Create a new toggle.
    pub fn new(id: ComponentId) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            checked: false,
            label: None,
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let checked = spec
            .props
            .get("checked")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let label = spec
            .props
            .get("label")
            .and_then(|v| v.as_str())
            .map(String::from);

        let mut toggle = Self::new(spec.id.clone());
        toggle.style = spec.style.clone();
        toggle.checked = checked;
        toggle.label = label;

        Ok(toggle)
    }

    /// Set initial checked state.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Set label.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

#[async_trait]
impl Component for ToggleComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Toggle
    }

    fn render(&self) -> ComponentSpec {
        let mut props = self.base.props.clone();
        props.insert("checked".to_string(), serde_json::json!(self.checked));
        props.insert(
            "disabled".to_string(),
            serde_json::json!(!self.base.enabled),
        );
        if let Some(ref label) = self.label {
            props.insert("label".to_string(), serde_json::json!(label));
        }

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Toggle,
            props,
            style: self.style.clone(),
            layout: None,
            children: vec![],
            events: vec!["change".to_string()],
        }
    }

    async fn handle_event(&mut self, event_type: &str, data: Value) -> Result<Option<Value>> {
        match event_type {
            "change" => {
                if self.base.enabled {
                    self.checked = !self.checked;
                    Ok(Some(serde_json::json!({
                        "checked": self.checked,
                        "toggle_id": self.base.id.0
                    })))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    fn props(&self) -> &HashMap<String, Value> {
        &self.base.props
    }

    fn set_prop(&mut self, key: &str, value: Value) {
        match key {
            "checked" => {
                if let Some(b) = value.as_bool() {
                    self.checked = b;
                }
            }
            "label" => {
                self.label = value.as_str().map(String::from);
            }
            _ => {
                self.base.set(key, value);
            }
        }
    }
}

/// Progress indicator component.
pub struct ProgressComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    value: f64,
    max: f64,
    indeterminate: bool,
}

impl ProgressComponent {
    /// Create a new progress indicator.
    pub fn new(id: ComponentId) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            value: 0.0,
            max: 100.0,
            indeterminate: false,
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let value = spec
            .props
            .get("value")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let max = spec
            .props
            .get("max")
            .and_then(|v| v.as_f64())
            .unwrap_or(100.0);

        let indeterminate = spec
            .props
            .get("indeterminate")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut progress = Self::new(spec.id.clone());
        progress.style = spec.style.clone();
        progress.value = value;
        progress.max = max;
        progress.indeterminate = indeterminate;

        Ok(progress)
    }

    /// Set value.
    pub fn value(mut self, value: f64) -> Self {
        self.value = value;
        self
    }

    /// Set max value.
    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    /// Set indeterminate mode.
    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }
}

#[async_trait]
impl Component for ProgressComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Progress
    }

    fn render(&self) -> ComponentSpec {
        let mut props = self.base.props.clone();
        props.insert("value".to_string(), serde_json::json!(self.value));
        props.insert("max".to_string(), serde_json::json!(self.max));
        props.insert(
            "indeterminate".to_string(),
            serde_json::json!(self.indeterminate),
        );
        props.insert(
            "percent".to_string(),
            serde_json::json!((self.value / self.max * 100.0).min(100.0)),
        );

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Progress,
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
        match key {
            "value" => {
                if let Some(v) = value.as_f64() {
                    self.value = v;
                }
            }
            "max" => {
                if let Some(v) = value.as_f64() {
                    self.max = v;
                }
            }
            "indeterminate" => {
                if let Some(b) = value.as_bool() {
                    self.indeterminate = b;
                }
            }
            _ => {
                self.base.set(key, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_component() {
        let button =
            ButtonComponent::new(ComponentId::from_str("btn1"), "Click Me").variant("primary");

        let spec = button.render();
        assert_eq!(spec.props.get("label").unwrap(), "Click Me");
        assert_eq!(spec.props.get("variant").unwrap(), "primary");
        assert!(spec.events.contains(&"click".to_string()));
    }

    #[test]
    fn test_toggle_component() {
        let toggle = ToggleComponent::new(ComponentId::from_str("toggle1"))
            .checked(true)
            .label("Enable feature");

        let spec = toggle.render();
        assert_eq!(spec.props.get("checked").unwrap(), true);
        assert!(spec.events.contains(&"change".to_string()));
    }

    #[test]
    fn test_progress_component() {
        let progress = ProgressComponent::new(ComponentId::from_str("progress1"))
            .value(50.0)
            .max(100.0);

        let spec = progress.render();
        assert_eq!(spec.props.get("value").unwrap(), 50.0);
        assert_eq!(spec.props.get("percent").unwrap(), 50.0);
    }
}
