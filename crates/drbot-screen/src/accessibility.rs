//! Accessibility API types and interfaces.

use serde::{Deserialize, Serialize};

/// Information about the focused application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusedApp {
    /// Application name.
    pub name: String,
    /// Bundle identifier (macOS) or executable path.
    pub bundle_id: Option<String>,
    /// Process ID.
    pub pid: u32,
    /// Active window title.
    pub window_title: Option<String>,
    /// Is the application frontmost.
    pub is_active: bool,
}

impl FocusedApp {
    /// Create a new focused app.
    pub fn new(name: impl Into<String>, pid: u32) -> Self {
        Self {
            name: name.into(),
            bundle_id: None,
            pid,
            window_title: None,
            is_active: true,
        }
    }

    /// Set bundle ID.
    pub fn with_bundle_id(mut self, bundle_id: impl Into<String>) -> Self {
        self.bundle_id = Some(bundle_id.into());
        self
    }

    /// Set window title.
    pub fn with_window_title(mut self, title: impl Into<String>) -> Self {
        self.window_title = Some(title.into());
        self
    }
}

/// Type of UI element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementType {
    /// Text field (editable).
    TextField,
    /// Static text (label).
    StaticText,
    /// Button.
    Button,
    /// Checkbox.
    Checkbox,
    /// Radio button.
    RadioButton,
    /// Dropdown/combo box.
    ComboBox,
    /// List item.
    ListItem,
    /// Table row.
    TableRow,
    /// Menu item.
    MenuItem,
    /// Tab.
    Tab,
    /// Slider.
    Slider,
    /// Link.
    Link,
    /// Image.
    Image,
    /// Group/container.
    Group,
    /// Window.
    Window,
    /// Toolbar.
    Toolbar,
    /// Scroll area.
    ScrollArea,
    /// Web area (browser content).
    WebArea,
    /// Unknown element.
    Unknown,
}

impl Default for ElementType {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Information about a focused UI element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusedElement {
    /// Element type.
    pub element_type: ElementType,
    /// Role description (from accessibility API).
    pub role: String,
    /// Element label/title.
    pub label: Option<String>,
    /// Element value (for text fields, etc.).
    pub value: Option<String>,
    /// Element description.
    pub description: Option<String>,
    /// Position on screen.
    pub position: Option<(i32, i32)>,
    /// Size.
    pub size: Option<(u32, u32)>,
    /// Is element enabled.
    pub is_enabled: bool,
    /// Is element focused.
    pub is_focused: bool,
    /// Available actions.
    pub actions: Vec<String>,
}

impl FocusedElement {
    /// Create a new focused element.
    pub fn new(element_type: ElementType, role: impl Into<String>) -> Self {
        Self {
            element_type,
            role: role.into(),
            label: None,
            value: None,
            description: None,
            position: None,
            size: None,
            is_enabled: true,
            is_focused: true,
            actions: Vec::new(),
        }
    }

    /// Set label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set value.
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Set position.
    pub fn with_position(mut self, x: i32, y: i32) -> Self {
        self.position = Some((x, y));
        self
    }

    /// Set size.
    pub fn with_size(mut self, width: u32, height: u32) -> Self {
        self.size = Some((width, height));
        self
    }
}

/// Accessibility tree node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityNode {
    /// Node ID (internal).
    pub id: u64,
    /// Element type.
    pub element_type: ElementType,
    /// Role.
    pub role: String,
    /// Label.
    pub label: Option<String>,
    /// Value.
    pub value: Option<String>,
    /// Children nodes.
    pub children: Vec<AccessibilityNode>,
    /// Depth in tree.
    pub depth: usize,
}

impl AccessibilityNode {
    /// Create a new node.
    pub fn new(id: u64, element_type: ElementType, role: impl Into<String>) -> Self {
        Self {
            id,
            element_type,
            role: role.into(),
            label: None,
            value: None,
            children: Vec::new(),
            depth: 0,
        }
    }

    /// Add a child node.
    pub fn add_child(&mut self, child: AccessibilityNode) {
        self.children.push(child);
    }

    /// Count total nodes in subtree.
    pub fn count_nodes(&self) -> usize {
        1 + self.children.iter().map(|c| c.count_nodes()).sum::<usize>()
    }

    /// Find node by ID.
    pub fn find_by_id(&self, id: u64) -> Option<&AccessibilityNode> {
        if self.id == id {
            return Some(self);
        }

        for child in &self.children {
            if let Some(node) = child.find_by_id(id) {
                return Some(node);
            }
        }

        None
    }

    /// Collect all text values from tree.
    pub fn collect_text(&self) -> Vec<String> {
        let mut texts = Vec::new();

        if let Some(label) = &self.label {
            texts.push(label.clone());
        }
        if let Some(value) = &self.value {
            texts.push(value.clone());
        }

        for child in &self.children {
            texts.extend(child.collect_text());
        }

        texts
    }
}

/// Full accessibility tree for a window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityTree {
    /// Root node.
    pub root: AccessibilityNode,
    /// Window title.
    pub window_title: Option<String>,
    /// Application name.
    pub app_name: String,
}

impl AccessibilityTree {
    /// Create a new tree.
    pub fn new(root: AccessibilityNode, app_name: impl Into<String>) -> Self {
        Self {
            root,
            window_title: None,
            app_name: app_name.into(),
        }
    }

    /// Get all visible text in the tree.
    pub fn get_all_text(&self) -> String {
        self.root.collect_text().join(" ")
    }

    /// Get total node count.
    pub fn node_count(&self) -> usize {
        self.root.count_nodes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focused_app() {
        let app = FocusedApp::new("Safari", 1234)
            .with_bundle_id("com.apple.Safari")
            .with_window_title("Apple");

        assert_eq!(app.name, "Safari");
        assert_eq!(app.bundle_id, Some("com.apple.Safari".to_string()));
    }

    #[test]
    fn test_focused_element() {
        let elem = FocusedElement::new(ElementType::TextField, "AXTextField")
            .with_label("Search")
            .with_value("hello world");

        assert_eq!(elem.element_type, ElementType::TextField);
        assert_eq!(elem.value, Some("hello world".to_string()));
    }

    #[test]
    fn test_accessibility_tree() {
        let mut root = AccessibilityNode::new(1, ElementType::Window, "AXWindow");
        root.label = Some("Main Window".to_string());

        let mut child = AccessibilityNode::new(2, ElementType::Button, "AXButton");
        child.label = Some("Click Me".to_string());

        root.add_child(child);

        let tree = AccessibilityTree::new(root, "TestApp");

        assert_eq!(tree.node_count(), 2);
        assert!(tree.get_all_text().contains("Main Window"));
        assert!(tree.get_all_text().contains("Click Me"));
    }
}
