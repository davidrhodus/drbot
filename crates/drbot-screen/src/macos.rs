//! macOS-specific accessibility implementation.
//!
//! Uses the Accessibility API (AXUIElement) to read screen content.

use crate::{
    AccessibilityNode, AccessibilityTree, ElementType, FocusedApp, FocusedElement, Result,
    ScreenError,
};
use std::process::Command;

/// Check if accessibility permission is granted.
pub fn check_accessibility_permission() -> bool {
    // In a full implementation, this would use:
    // AXIsProcessTrusted() from ApplicationServices

    // For now, use a simple check by trying to run AppleScript
    let output = Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get name of first process")
        .output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Get the currently focused application.
pub async fn get_focused_app() -> Option<FocusedApp> {
    // Use AppleScript to get frontmost application info
    let script = r#"
        tell application "System Events"
            set frontApp to first application process whose frontmost is true
            set appName to name of frontApp
            set appPID to unix id of frontApp
            set windowTitle to ""
            try
                set windowTitle to name of first window of frontApp
            end try
            return appName & "|" & appPID & "|" & windowTitle
        end tell
    "#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let result = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = result.trim().split('|').collect();

    if parts.len() >= 2 {
        let name = parts[0].to_string();
        let pid: u32 = parts[1].parse().unwrap_or(0);
        let window_title = parts
            .get(2)
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        let mut app = FocusedApp::new(name, pid);
        if let Some(title) = window_title {
            app = app.with_window_title(title);
        }

        // Try to get bundle ID
        if let Some(bundle_id) = get_bundle_id(pid) {
            app = app.with_bundle_id(bundle_id);
        }

        return Some(app);
    }

    None
}

/// Get bundle ID for a process.
fn get_bundle_id(pid: u32) -> Option<String> {
    let script = format!(
        r#"
        tell application "System Events"
            set theProcess to first process whose unix id is {}
            return bundle identifier of theProcess
        end tell
    "#,
        pid
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .ok()?;

    if output.status.success() {
        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !result.is_empty() && result != "missing value" {
            return Some(result);
        }
    }

    None
}

/// Get the currently focused UI element.
pub async fn get_focused_element() -> Option<FocusedElement> {
    // This would use AXUIElementCopySystemWide() and AXUIElementCopyAttributeValue
    // For now, use a stub that returns basic info from AppleScript

    let script = r#"
        tell application "System Events"
            set frontApp to first application process whose frontmost is true
            try
                set focusedElem to focused UI element of frontApp
                set elemRole to role of focusedElem
                set elemValue to ""
                try
                    set elemValue to value of focusedElem
                end try
                set elemDesc to ""
                try
                    set elemDesc to description of focusedElem
                end try
                return elemRole & "|" & elemValue & "|" & elemDesc
            on error
                return "unknown||"
            end try
        end tell
    "#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let result = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = result.trim().split('|').collect();

    if !parts.is_empty() {
        let role = parts[0];
        let element_type = role_to_element_type(role);
        let mut elem = FocusedElement::new(element_type, role);

        if parts.len() > 1 && !parts[1].is_empty() {
            elem = elem.with_value(parts[1]);
        }

        if parts.len() > 2 && !parts[2].is_empty() {
            elem.description = Some(parts[2].to_string());
        }

        return Some(elem);
    }

    None
}

/// Convert accessibility role to ElementType.
fn role_to_element_type(role: &str) -> ElementType {
    match role {
        "AXTextField" | "AXTextArea" => ElementType::TextField,
        "AXStaticText" => ElementType::StaticText,
        "AXButton" => ElementType::Button,
        "AXCheckBox" => ElementType::Checkbox,
        "AXRadioButton" => ElementType::RadioButton,
        "AXComboBox" | "AXPopUpButton" => ElementType::ComboBox,
        "AXList" | "AXRow" => ElementType::ListItem,
        "AXMenuItem" => ElementType::MenuItem,
        "AXTabGroup" | "AXTab" => ElementType::Tab,
        "AXSlider" => ElementType::Slider,
        "AXLink" => ElementType::Link,
        "AXImage" => ElementType::Image,
        "AXGroup" | "AXSplitGroup" => ElementType::Group,
        "AXWindow" | "AXSheet" => ElementType::Window,
        "AXToolbar" => ElementType::Toolbar,
        "AXScrollArea" => ElementType::ScrollArea,
        "AXWebArea" => ElementType::WebArea,
        _ => ElementType::Unknown,
    }
}

/// Get visible text from the focused application.
pub async fn get_visible_text(max_length: usize) -> Result<String> {
    // This would traverse the accessibility tree to collect all text
    // For now, use AppleScript to get text from the focused window

    let script = r#"
        tell application "System Events"
            set frontApp to first application process whose frontmost is true
            set allText to ""
            try
                set focusedWindow to first window of frontApp
                set allElements to every UI element of focusedWindow
                repeat with elem in allElements
                    try
                        set elemValue to value of elem
                        if elemValue is not missing value then
                            set allText to allText & " " & elemValue
                        end if
                    end try
                    try
                        set elemTitle to title of elem
                        if elemTitle is not missing value then
                            set allText to allText & " " & elemTitle
                        end if
                    end try
                end repeat
            end try
            return allText
        end tell
    "#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| ScreenError::Internal(e.to_string()))?;

    if !output.status.success() {
        return Err(ScreenError::NoFocusedElement);
    }

    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if text.len() > max_length {
        text.truncate(max_length);
    }

    Ok(text)
}

/// Get the accessibility tree for the focused window.
pub async fn get_accessibility_tree() -> Result<AccessibilityTree> {
    // This would use AXUIElementCopyAttributeValue to traverse the tree
    // For now, return a basic stub

    let app = get_focused_app()
        .await
        .ok_or(ScreenError::NoFocusedElement)?;

    let root = AccessibilityNode::new(1, ElementType::Window, "AXWindow");
    let mut tree = AccessibilityTree::new(root, &app.name);
    tree.window_title = app.window_title;

    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_to_element_type() {
        assert_eq!(role_to_element_type("AXTextField"), ElementType::TextField);
        assert_eq!(role_to_element_type("AXButton"), ElementType::Button);
        assert_eq!(role_to_element_type("AXUnknownRole"), ElementType::Unknown);
    }
}
