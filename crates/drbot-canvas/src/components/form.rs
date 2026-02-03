//! Form components.

use super::{BaseProps, Component};
use crate::Result;
use async_trait::async_trait;
use drbot_canvas_protocol::{ComponentId, ComponentSpec, ComponentType, StyleSpec};
use serde_json::Value;
use std::collections::HashMap;

/// Input component.
pub struct InputComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    value: String,
    placeholder: Option<String>,
    input_type: String,
}

impl InputComponent {
    /// Create a new input.
    pub fn new(id: ComponentId) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            value: String::new(),
            placeholder: None,
            input_type: "text".to_string(),
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let value = spec
            .props
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let placeholder = spec
            .props
            .get("placeholder")
            .and_then(|v| v.as_str())
            .map(String::from);

        let input_type = spec
            .props
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("text")
            .to_string();

        let mut input = Self::new(spec.id.clone());
        input.style = spec.style.clone();
        input.value = value;
        input.placeholder = placeholder;
        input.input_type = input_type;

        for (key, value) in &spec.props {
            input.base.set(key.clone(), value.clone());
        }

        Ok(input)
    }

    /// Set placeholder.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set input type.
    pub fn input_type(mut self, input_type: impl Into<String>) -> Self {
        self.input_type = input_type.into();
        self
    }

    /// Set initial value.
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    /// Set style.
    pub fn style(mut self, style: StyleSpec) -> Self {
        self.style = Some(style);
        self
    }
}

#[async_trait]
impl Component for InputComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Input
    }

    fn render(&self) -> ComponentSpec {
        let mut props = self.base.props.clone();
        props.insert("value".to_string(), serde_json::json!(self.value));
        props.insert("type".to_string(), serde_json::json!(self.input_type));
        props.insert(
            "disabled".to_string(),
            serde_json::json!(!self.base.enabled),
        );
        if let Some(ref placeholder) = self.placeholder {
            props.insert("placeholder".to_string(), serde_json::json!(placeholder));
        }

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Input,
            props,
            style: self.style.clone(),
            layout: None,
            children: vec![],
            events: vec![
                "change".to_string(),
                "focus".to_string(),
                "blur".to_string(),
            ],
        }
    }

    async fn handle_event(&mut self, event_type: &str, data: Value) -> Result<Option<Value>> {
        match event_type {
            "change" => {
                if let Some(new_value) = data.get("value").and_then(|v| v.as_str()) {
                    self.value = new_value.to_string();
                    Ok(Some(serde_json::json!({
                        "input_id": self.base.id.0,
                        "value": self.value
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
            "value" => {
                if let Some(s) = value.as_str() {
                    self.value = s.to_string();
                }
            }
            "placeholder" => {
                self.placeholder = value.as_str().map(String::from);
            }
            "type" => {
                if let Some(s) = value.as_str() {
                    self.input_type = s.to_string();
                }
            }
            _ => {
                self.base.set(key, value);
            }
        }
    }
}

/// Select/dropdown component.
pub struct SelectComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    value: Option<String>,
    options: Vec<SelectOption>,
    placeholder: Option<String>,
}

/// Select option.
#[derive(Debug, Clone)]
pub struct SelectOption {
    /// Option value.
    pub value: String,
    /// Display label.
    pub label: String,
    /// Whether disabled.
    pub disabled: bool,
}

impl SelectComponent {
    /// Create a new select.
    pub fn new(id: ComponentId) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            value: None,
            options: Vec::new(),
            placeholder: None,
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let value = spec
            .props
            .get("value")
            .and_then(|v| v.as_str())
            .map(String::from);

        let placeholder = spec
            .props
            .get("placeholder")
            .and_then(|v| v.as_str())
            .map(String::from);

        let options = spec
            .props
            .get("options")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|opt| {
                        let value = opt.get("value")?.as_str()?.to_string();
                        let label = opt
                            .get("label")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&value)
                            .to_string();
                        let disabled = opt
                            .get("disabled")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        Some(SelectOption {
                            value,
                            label,
                            disabled,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut select = Self::new(spec.id.clone());
        select.style = spec.style.clone();
        select.value = value;
        select.options = options;
        select.placeholder = placeholder;

        Ok(select)
    }

    /// Add an option.
    pub fn option(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.options.push(SelectOption {
            value: value.into(),
            label: label.into(),
            disabled: false,
        });
        self
    }

    /// Set options.
    pub fn options(mut self, options: Vec<SelectOption>) -> Self {
        self.options = options;
        self
    }

    /// Set placeholder.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set initial value.
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }
}

#[async_trait]
impl Component for SelectComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Select
    }

    fn render(&self) -> ComponentSpec {
        let mut props = self.base.props.clone();

        if let Some(ref value) = self.value {
            props.insert("value".to_string(), serde_json::json!(value));
        }
        if let Some(ref placeholder) = self.placeholder {
            props.insert("placeholder".to_string(), serde_json::json!(placeholder));
        }

        let options_json: Vec<Value> = self
            .options
            .iter()
            .map(|opt| {
                serde_json::json!({
                    "value": opt.value,
                    "label": opt.label,
                    "disabled": opt.disabled
                })
            })
            .collect();
        props.insert("options".to_string(), serde_json::json!(options_json));
        props.insert(
            "disabled".to_string(),
            serde_json::json!(!self.base.enabled),
        );

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Select,
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
                if let Some(new_value) = data.get("value").and_then(|v| v.as_str()) {
                    self.value = Some(new_value.to_string());
                    Ok(Some(serde_json::json!({
                        "select_id": self.base.id.0,
                        "value": self.value
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
            "value" => {
                self.value = value.as_str().map(String::from);
            }
            "placeholder" => {
                self.placeholder = value.as_str().map(String::from);
            }
            _ => {
                self.base.set(key, value);
            }
        }
    }
}

/// Form component.
pub struct FormComponent {
    base: BaseProps,
    style: Option<StyleSpec>,
    children: Vec<Box<dyn Component>>,
}

impl FormComponent {
    /// Create a new form.
    pub fn new(id: ComponentId) -> Self {
        Self {
            base: BaseProps::new(id),
            style: None,
            children: Vec::new(),
        }
    }

    /// Create from specification.
    pub fn from_spec(spec: &ComponentSpec) -> Result<Self> {
        let mut form = Self::new(spec.id.clone());
        form.style = spec.style.clone();

        for child_spec in &spec.children {
            let child = super::ComponentFactory::create(child_spec)?;
            form.children.push(child);
        }

        Ok(form)
    }

    /// Set style.
    pub fn style(mut self, style: StyleSpec) -> Self {
        self.style = Some(style);
        self
    }

    /// Collect form values from children.
    pub fn collect_values(&self) -> HashMap<String, Value> {
        let mut values = HashMap::new();
        self.collect_values_recursive(&self.children, &mut values);
        values
    }

    fn collect_values_recursive(
        &self,
        components: &[Box<dyn Component>],
        values: &mut HashMap<String, Value>,
    ) {
        for component in components {
            match component.component_type() {
                ComponentType::Input | ComponentType::Select | ComponentType::Toggle => {
                    if let Some(value) = component.props().get("value") {
                        values.insert(component.id().0.clone(), value.clone());
                    } else if let Some(checked) = component.props().get("checked") {
                        values.insert(component.id().0.clone(), checked.clone());
                    }
                }
                _ => {}
            }
            // Recurse into children
            self.collect_values_recursive(component.children(), values);
        }
    }
}

#[async_trait]
impl Component for FormComponent {
    fn id(&self) -> &ComponentId {
        &self.base.id
    }

    fn component_type(&self) -> ComponentType {
        ComponentType::Form
    }

    fn render(&self) -> ComponentSpec {
        let children: Vec<ComponentSpec> = self.children.iter().map(|c| c.render()).collect();

        ComponentSpec {
            id: self.base.id.clone(),
            component_type: ComponentType::Form,
            props: self.base.props.clone(),
            style: self.style.clone(),
            layout: None,
            children,
            events: vec!["submit".to_string()],
        }
    }

    async fn handle_event(&mut self, event_type: &str, _data: Value) -> Result<Option<Value>> {
        match event_type {
            "submit" => {
                let values = self.collect_values();
                Ok(Some(serde_json::json!({
                    "form_id": self.base.id.0,
                    "values": values
                })))
            }
            _ => Ok(None),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_component() {
        let input = InputComponent::new(ComponentId::from_str("input1"))
            .placeholder("Enter text...")
            .input_type("email");

        let spec = input.render();
        assert_eq!(spec.props.get("type").unwrap(), "email");
        assert!(spec.events.contains(&"change".to_string()));
    }

    #[test]
    fn test_select_component() {
        let select = SelectComponent::new(ComponentId::from_str("select1"))
            .option("a", "Option A")
            .option("b", "Option B")
            .value("a");

        let spec = select.render();
        assert_eq!(spec.props.get("value").unwrap(), "a");
        assert!(spec.props.get("options").unwrap().as_array().unwrap().len() == 2);
    }
}
