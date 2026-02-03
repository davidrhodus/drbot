//! Component specifications for canvas protocol.

use crate::ComponentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Component type enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentType {
    /// Container for layout.
    Container,
    /// Card component.
    Card,
    /// Text display.
    Text,
    /// Button.
    Button,
    /// Input field.
    Input,
    /// Form container.
    Form,
    /// Data table.
    Table,
    /// Chart visualization.
    Chart,
    /// Image display.
    Image,
    /// Toggle switch.
    Toggle,
    /// Dropdown select.
    Select,
    /// Progress indicator.
    Progress,
    /// Divider line.
    Divider,
    /// Spacer.
    Spacer,
    /// Custom component.
    Custom(String),
}

/// Style specification for components.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StyleSpec {
    /// Width (e.g., "100%", "200px", "auto").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
    /// Height.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<String>,
    /// Padding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<String>,
    /// Margin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<String>,
    /// Background color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Text color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Border.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    /// Border radius.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<String>,
    /// Font size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<String>,
    /// Font weight.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<String>,
    /// Text alignment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_align: Option<String>,
    /// Flex direction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flex_direction: Option<String>,
    /// Justify content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justify_content: Option<String>,
    /// Align items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align_items: Option<String>,
    /// Gap between items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<String>,
    /// Additional custom styles.
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

impl StyleSpec {
    /// Create a new empty style spec.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set width.
    pub fn width(mut self, width: impl Into<String>) -> Self {
        self.width = Some(width.into());
        self
    }

    /// Set height.
    pub fn height(mut self, height: impl Into<String>) -> Self {
        self.height = Some(height.into());
        self
    }

    /// Set padding.
    pub fn padding(mut self, padding: impl Into<String>) -> Self {
        self.padding = Some(padding.into());
        self
    }

    /// Set background.
    pub fn background(mut self, bg: impl Into<String>) -> Self {
        self.background = Some(bg.into());
        self
    }
}

/// Layout specification for containers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutSpec {
    /// Layout direction.
    #[serde(default)]
    pub direction: LayoutDirection,
    /// Gap between items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<String>,
    /// Wrap behavior.
    #[serde(default)]
    pub wrap: bool,
    /// Alignment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    /// Justification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justify: Option<String>,
}

/// Layout direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutDirection {
    /// Horizontal row.
    #[default]
    Row,
    /// Vertical column.
    Column,
}

/// Component specification - the wire format for components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSpec {
    /// Component ID.
    pub id: ComponentId,
    /// Component type.
    #[serde(rename = "type")]
    pub component_type: ComponentType,
    /// Component properties.
    #[serde(default)]
    pub props: HashMap<String, serde_json::Value>,
    /// Component style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleSpec>,
    /// Layout for containers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<LayoutSpec>,
    /// Child components.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ComponentSpec>,
    /// Event handlers to register.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
}

impl ComponentSpec {
    /// Create a new component spec.
    pub fn new(id: ComponentId, component_type: ComponentType) -> Self {
        Self {
            id,
            component_type,
            props: HashMap::new(),
            style: None,
            layout: None,
            children: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Create a container component.
    pub fn container(id: ComponentId) -> Self {
        Self::new(id, ComponentType::Container)
    }

    /// Create a card component.
    pub fn card(id: ComponentId) -> Self {
        Self::new(id, ComponentType::Card)
    }

    /// Create a text component.
    pub fn text(id: ComponentId, content: impl Into<String>) -> Self {
        let mut spec = Self::new(id, ComponentType::Text);
        spec.props
            .insert("content".to_string(), serde_json::json!(content.into()));
        spec
    }

    /// Create a button component.
    pub fn button(id: ComponentId, label: impl Into<String>) -> Self {
        let mut spec = Self::new(id, ComponentType::Button);
        spec.props
            .insert("label".to_string(), serde_json::json!(label.into()));
        spec.events.push("click".to_string());
        spec
    }

    /// Create an input component.
    pub fn input(id: ComponentId, placeholder: Option<&str>) -> Self {
        let mut spec = Self::new(id, ComponentType::Input);
        if let Some(ph) = placeholder {
            spec.props
                .insert("placeholder".to_string(), serde_json::json!(ph));
        }
        spec.events.push("change".to_string());
        spec
    }

    /// Set a property.
    pub fn prop(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.props.insert(key.into(), value);
        self
    }

    /// Set style.
    pub fn style(mut self, style: StyleSpec) -> Self {
        self.style = Some(style);
        self
    }

    /// Set layout.
    pub fn layout(mut self, layout: LayoutSpec) -> Self {
        self.layout = Some(layout);
        self
    }

    /// Add a child component.
    pub fn child(mut self, child: ComponentSpec) -> Self {
        self.children.push(child);
        self
    }

    /// Add multiple children.
    pub fn children(mut self, children: Vec<ComponentSpec>) -> Self {
        self.children.extend(children);
        self
    }

    /// Add an event handler.
    pub fn on(mut self, event: impl Into<String>) -> Self {
        self.events.push(event.into());
        self
    }
}

/// Table column definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableColumn {
    /// Column key (field name).
    pub key: String,
    /// Column header label.
    pub label: String,
    /// Column width.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<String>,
    /// Whether column is sortable.
    #[serde(default)]
    pub sortable: bool,
}

/// Chart type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartType {
    Line,
    Bar,
    Pie,
    Area,
    Scatter,
    Donut,
}

/// Chart data series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSeries {
    /// Series name.
    pub name: String,
    /// Data points.
    pub data: Vec<ChartDataPoint>,
    /// Series color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Chart data point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartDataPoint {
    /// X value (label or number).
    pub x: serde_json::Value,
    /// Y value.
    pub y: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_spec_builder() {
        let spec = ComponentSpec::container(ComponentId::from_str("root"))
            .style(StyleSpec::new().padding("16px"))
            .child(ComponentSpec::text(
                ComponentId::from_str("title"),
                "Hello World",
            ))
            .child(
                ComponentSpec::button(ComponentId::from_str("btn"), "Click Me")
                    .prop("variant", serde_json::json!("primary")),
            );

        assert_eq!(spec.children.len(), 2);
        assert!(spec.style.is_some());
    }

    #[test]
    fn test_style_spec_builder() {
        let style = StyleSpec::new()
            .width("100%")
            .padding("8px")
            .background("#fff");

        assert_eq!(style.width, Some("100%".to_string()));
        assert_eq!(style.padding, Some("8px".to_string()));
        assert_eq!(style.background, Some("#fff".to_string()));
    }

    #[test]
    fn test_serialization() {
        let spec = ComponentSpec::button(ComponentId::from_str("btn1"), "Submit");
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("\"type\":\"button\""));
        assert!(json.contains("\"label\":\"Submit\""));
    }
}
