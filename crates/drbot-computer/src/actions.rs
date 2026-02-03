//! Action definitions for computer use.
//!
//! Defines the various actions that can be performed on the desktop.

use crate::keyboard::{KeyCode, Modifiers};
use crate::mouse::MouseButton;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A computer action that can be executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    // Mouse actions
    /// Click at a position.
    Click { x: i32, y: i32, button: MouseButton },
    /// Double click at a position.
    DoubleClick { x: i32, y: i32, button: MouseButton },
    /// Move mouse to position.
    MoveMouse { x: i32, y: i32 },
    /// Drag from one position to another.
    Drag {
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        button: MouseButton,
    },
    /// Scroll at current position.
    Scroll { dx: i32, dy: i32 },

    // Keyboard actions
    /// Type text.
    Type { text: String },
    /// Press a key with optional modifiers.
    KeyPress {
        key: KeyCode,
        modifiers: Vec<Modifiers>,
    },
    /// Press a keyboard shortcut (e.g., Cmd+C).
    Shortcut {
        keys: Vec<KeyCode>,
        modifiers: Vec<Modifiers>,
    },

    // Wait/delay
    /// Wait for a duration.
    Wait { duration_ms: u64 },
    /// Wait for an element to appear (requires screen analysis).
    WaitForElement {
        description: String,
        timeout_ms: u64,
    },

    // Screenshot
    /// Take a screenshot.
    Screenshot { region: Option<ScreenRegion> },

    // Application control
    /// Open an application.
    OpenApp { name: String },
    /// Focus an application window.
    FocusApp { name: String },
    /// Close the frontmost window.
    CloseWindow,

    // Composite actions
    /// Execute multiple actions in sequence.
    Sequence { actions: Vec<Action> },
    /// Execute an action conditionally.
    Conditional {
        condition: Box<Condition>,
        then_action: Box<Action>,
        else_action: Option<Box<Action>>,
    },
}

/// A region of the screen.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScreenRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Conditions for conditional actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    /// Check if an element exists on screen.
    ElementExists { description: String },
    /// Check if text is visible on screen.
    TextVisible { text: String },
    /// Check if a window is focused.
    WindowFocused { app_name: String },
    /// Always true (for unconditional execution).
    Always,
}

/// Result of executing an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Whether the action succeeded.
    pub success: bool,
    /// Action that was executed.
    pub action: Action,
    /// Duration the action took.
    pub duration_ms: u64,
    /// Screenshot taken after action (if enabled).
    pub screenshot: Option<Vec<u8>>,
    /// Any error message.
    pub error: Option<String>,
    /// Additional data from the action.
    pub data: Option<serde_json::Value>,
}

impl ActionResult {
    /// Create a successful result.
    pub fn success(action: Action, duration_ms: u64) -> Self {
        Self {
            success: true,
            action,
            duration_ms,
            screenshot: None,
            error: None,
            data: None,
        }
    }

    /// Create a failed result.
    pub fn failure(action: Action, error: impl Into<String>) -> Self {
        Self {
            success: false,
            action,
            duration_ms: 0,
            screenshot: None,
            error: Some(error.into()),
            data: None,
        }
    }

    /// Add a screenshot to the result.
    pub fn with_screenshot(mut self, screenshot: Vec<u8>) -> Self {
        self.screenshot = Some(screenshot);
        self
    }

    /// Add data to the result.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// A sequence of actions to execute.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionSequence {
    /// Actions in the sequence.
    pub actions: Vec<Action>,
    /// Name of the sequence.
    pub name: Option<String>,
    /// Description of what this sequence does.
    pub description: Option<String>,
}

impl ActionSequence {
    /// Create a new empty sequence.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a named sequence.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Default::default()
        }
    }

    /// Add an action to the sequence.
    pub fn add(mut self, action: Action) -> Self {
        self.actions.push(action);
        self
    }

    /// Add a click action.
    pub fn click(self, x: i32, y: i32) -> Self {
        self.add(Action::Click {
            x,
            y,
            button: MouseButton::Left,
        })
    }

    /// Add a right click action.
    pub fn right_click(self, x: i32, y: i32) -> Self {
        self.add(Action::Click {
            x,
            y,
            button: MouseButton::Right,
        })
    }

    /// Add a type action.
    pub fn type_text(self, text: impl Into<String>) -> Self {
        self.add(Action::Type { text: text.into() })
    }

    /// Add a key press action.
    pub fn key(self, key: KeyCode) -> Self {
        self.add(Action::KeyPress {
            key,
            modifiers: vec![],
        })
    }

    /// Add a keyboard shortcut.
    pub fn shortcut(self, key: KeyCode, modifier: Modifiers) -> Self {
        self.add(Action::KeyPress {
            key,
            modifiers: vec![modifier],
        })
    }

    /// Add a wait action.
    pub fn wait(self, duration: Duration) -> Self {
        self.add(Action::Wait {
            duration_ms: duration.as_millis() as u64,
        })
    }

    /// Add a screenshot action.
    pub fn screenshot(self) -> Self {
        self.add(Action::Screenshot { region: None })
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Get the number of actions in the sequence.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Check if the sequence is empty.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

impl From<Vec<Action>> for ActionSequence {
    fn from(actions: Vec<Action>) -> Self {
        Self {
            actions,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_sequence_builder() {
        let seq = ActionSequence::named("test")
            .click(100, 100)
            .type_text("hello")
            .key(KeyCode::Enter)
            .wait(Duration::from_millis(100));

        assert_eq!(seq.name, Some("test".to_string()));
        assert_eq!(seq.len(), 4);
    }

    #[test]
    fn test_action_result_success() {
        let result = ActionResult::success(
            Action::Click {
                x: 0,
                y: 0,
                button: MouseButton::Left,
            },
            100,
        );
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_action_result_failure() {
        let result = ActionResult::failure(
            Action::Click {
                x: 0,
                y: 0,
                button: MouseButton::Left,
            },
            "test error",
        );
        assert!(!result.success);
        assert_eq!(result.error, Some("test error".to_string()));
    }
}
