//! Mouse control for desktop automation.
//!
//! Provides mouse movement, clicking, and dragging operations.

use serde::{Deserialize, Serialize};

/// Mouse button types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    /// Left mouse button (primary).
    Left,
    /// Right mouse button (secondary).
    Right,
    /// Middle mouse button (scroll wheel click).
    Middle,
}

impl Default for MouseButton {
    fn default() -> Self {
        Self::Left
    }
}

/// Mouse action types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MouseAction {
    /// Move mouse to absolute position.
    Move { x: i32, y: i32 },
    /// Move mouse relative to current position.
    MoveRelative { dx: i32, dy: i32 },
    /// Click at current position.
    Click { button: MouseButton },
    /// Double click at current position.
    DoubleClick { button: MouseButton },
    /// Triple click at current position (select line/paragraph).
    TripleClick { button: MouseButton },
    /// Press and hold button.
    Down { button: MouseButton },
    /// Release button.
    Up { button: MouseButton },
    /// Scroll wheel.
    Scroll { dx: i32, dy: i32 },
    /// Drag from current position to target.
    Drag {
        to_x: i32,
        to_y: i32,
        button: MouseButton,
    },
}

/// Current mouse state.
#[derive(Debug, Clone, Default)]
pub struct MouseState {
    /// Current X position.
    pub x: i32,
    /// Current Y position.
    pub y: i32,
    /// Currently pressed buttons.
    pub pressed: Vec<MouseButton>,
}

/// Mouse controller for executing mouse actions.
#[derive(Debug)]
pub struct MouseController {
    state: MouseState,
}

impl MouseController {
    /// Create a new mouse controller.
    pub fn new() -> Self {
        Self {
            state: MouseState::default(),
        }
    }

    /// Get current mouse position.
    #[cfg(target_os = "macos")]
    pub fn get_position(&self) -> crate::Result<(i32, i32)> {
        use core_graphics::event::CGEvent;
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create event source".into())
        })?;

        let event = CGEvent::new(source)
            .map_err(|_| crate::ComputerError::ActionFailed("Failed to create event".into()))?;

        let location = event.location();
        Ok((location.x as i32, location.y as i32))
    }

    #[cfg(not(target_os = "macos"))]
    pub fn get_position(&self) -> crate::Result<(i32, i32)> {
        Err(crate::ComputerError::PlatformNotSupported)
    }

    /// Move mouse to absolute position.
    #[cfg(target_os = "macos")]
    pub fn move_to(&mut self, x: i32, y: i32) -> crate::Result<()> {
        use core_graphics::event::{CGEvent, CGEventType, CGMouseButton};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        use core_graphics::geometry::CGPoint;

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create event source".into())
        })?;

        let point = CGPoint::new(x as f64, y as f64);
        let event =
            CGEvent::new_mouse_event(source, CGEventType::MouseMoved, point, CGMouseButton::Left)
                .map_err(|_| {
                crate::ComputerError::ActionFailed("Failed to create mouse event".into())
            })?;

        event.post(core_graphics::event::CGEventTapLocation::HID);

        self.state.x = x;
        self.state.y = y;
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn move_to(&mut self, _x: i32, _y: i32) -> crate::Result<()> {
        Err(crate::ComputerError::PlatformNotSupported)
    }

    /// Click at current position.
    #[cfg(target_os = "macos")]
    pub fn click(&mut self, button: MouseButton) -> crate::Result<()> {
        use core_graphics::event::{CGEvent, CGEventType, CGMouseButton};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        use core_graphics::geometry::CGPoint;

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create event source".into())
        })?;

        let point = CGPoint::new(self.state.x as f64, self.state.y as f64);

        let (cg_button, down_type, up_type) = match button {
            MouseButton::Left => (
                CGMouseButton::Left,
                CGEventType::LeftMouseDown,
                CGEventType::LeftMouseUp,
            ),
            MouseButton::Right => (
                CGMouseButton::Right,
                CGEventType::RightMouseDown,
                CGEventType::RightMouseUp,
            ),
            MouseButton::Middle => (
                CGMouseButton::Center,
                CGEventType::OtherMouseDown,
                CGEventType::OtherMouseUp,
            ),
        };

        // Mouse down
        let down_event = CGEvent::new_mouse_event(source.clone(), down_type, point, cg_button)
            .map_err(|_| {
                crate::ComputerError::ActionFailed("Failed to create mouse down event".into())
            })?;
        down_event.post(core_graphics::event::CGEventTapLocation::HID);

        // Mouse up
        let up_event =
            CGEvent::new_mouse_event(source, up_type, point, cg_button).map_err(|_| {
                crate::ComputerError::ActionFailed("Failed to create mouse up event".into())
            })?;
        up_event.post(core_graphics::event::CGEventTapLocation::HID);

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn click(&mut self, _button: MouseButton) -> crate::Result<()> {
        Err(crate::ComputerError::PlatformNotSupported)
    }

    /// Press and hold a mouse button.
    #[cfg(target_os = "macos")]
    pub fn mouse_down(&mut self, button: MouseButton) -> crate::Result<()> {
        use core_graphics::event::{CGEvent, CGEventType, CGMouseButton};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        use core_graphics::geometry::CGPoint;

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create event source".into())
        })?;

        let point = CGPoint::new(self.state.x as f64, self.state.y as f64);

        let (cg_button, down_type) = match button {
            MouseButton::Left => (CGMouseButton::Left, CGEventType::LeftMouseDown),
            MouseButton::Right => (CGMouseButton::Right, CGEventType::RightMouseDown),
            MouseButton::Middle => (CGMouseButton::Center, CGEventType::OtherMouseDown),
        };

        let down_event =
            CGEvent::new_mouse_event(source, down_type, point, cg_button).map_err(|_| {
                crate::ComputerError::ActionFailed("Failed to create mouse down event".into())
            })?;
        down_event.post(core_graphics::event::CGEventTapLocation::HID);

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn mouse_down(&mut self, _button: MouseButton) -> crate::Result<()> {
        Err(crate::ComputerError::PlatformNotSupported)
    }

    /// Release a mouse button.
    #[cfg(target_os = "macos")]
    pub fn mouse_up(&mut self, button: MouseButton) -> crate::Result<()> {
        use core_graphics::event::{CGEvent, CGEventType, CGMouseButton};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        use core_graphics::geometry::CGPoint;

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create event source".into())
        })?;

        let point = CGPoint::new(self.state.x as f64, self.state.y as f64);

        let (cg_button, up_type) = match button {
            MouseButton::Left => (CGMouseButton::Left, CGEventType::LeftMouseUp),
            MouseButton::Right => (CGMouseButton::Right, CGEventType::RightMouseUp),
            MouseButton::Middle => (CGMouseButton::Center, CGEventType::OtherMouseUp),
        };

        let up_event =
            CGEvent::new_mouse_event(source, up_type, point, cg_button).map_err(|_| {
                crate::ComputerError::ActionFailed("Failed to create mouse up event".into())
            })?;
        up_event.post(core_graphics::event::CGEventTapLocation::HID);

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn mouse_up(&mut self, _button: MouseButton) -> crate::Result<()> {
        Err(crate::ComputerError::PlatformNotSupported)
    }

    /// Scroll the mouse wheel.
    #[cfg(target_os = "macos")]
    pub fn scroll(&mut self, dx: i32, dy: i32) -> crate::Result<()> {
        use core_graphics::event::{CGEvent, CGEventType};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create event source".into())
        })?;

        // Create a base event and set it as scroll wheel type
        let event = CGEvent::new(source)
            .map_err(|_| crate::ComputerError::ActionFailed("Failed to create event".into()))?;

        // Set the event type to scroll wheel
        event.set_type(CGEventType::ScrollWheel);

        // Set scroll wheel delta fields using raw field IDs
        // kCGScrollWheelEventDeltaAxis1 = 11 (vertical)
        // kCGScrollWheelEventDeltaAxis2 = 12 (horizontal)
        const SCROLL_WHEEL_DELTA_AXIS1: u32 = 11;
        const SCROLL_WHEEL_DELTA_AXIS2: u32 = 12;

        event.set_integer_value_field(SCROLL_WHEEL_DELTA_AXIS1, dy as i64);
        event.set_integer_value_field(SCROLL_WHEEL_DELTA_AXIS2, dx as i64);

        event.post(core_graphics::event::CGEventTapLocation::HID);

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn scroll(&mut self, _dx: i32, _dy: i32) -> crate::Result<()> {
        Err(crate::ComputerError::PlatformNotSupported)
    }

    /// Execute a mouse action.
    pub fn execute(&mut self, action: MouseAction) -> crate::Result<()> {
        match action {
            MouseAction::Move { x, y } => self.move_to(x, y),
            MouseAction::MoveRelative { dx, dy } => {
                let (x, y) = self.get_position()?;
                self.move_to(x + dx, y + dy)
            }
            MouseAction::Click { button } => self.click(button),
            MouseAction::DoubleClick { button } => {
                self.click(button)?;
                std::thread::sleep(std::time::Duration::from_millis(50));
                self.click(button)
            }
            MouseAction::TripleClick { button } => {
                self.click(button)?;
                std::thread::sleep(std::time::Duration::from_millis(50));
                self.click(button)?;
                std::thread::sleep(std::time::Duration::from_millis(50));
                self.click(button)
            }
            MouseAction::Down { button } => {
                self.state.pressed.push(button);
                self.mouse_down(button)
            }
            MouseAction::Up { button } => {
                self.state.pressed.retain(|b| *b != button);
                self.mouse_up(button)
            }
            MouseAction::Scroll { dx, dy } => self.scroll(dx, dy),
            MouseAction::Drag { to_x, to_y, button } => {
                self.execute(MouseAction::Down { button })?;
                self.move_to(to_x, to_y)?;
                self.execute(MouseAction::Up { button })
            }
        }
    }

    /// Get current state.
    pub fn state(&self) -> &MouseState {
        &self.state
    }
}

impl Default for MouseController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouse_button_default() {
        assert_eq!(MouseButton::default(), MouseButton::Left);
    }

    #[test]
    fn test_mouse_controller_new() {
        let controller = MouseController::new();
        assert_eq!(controller.state.x, 0);
        assert_eq!(controller.state.y, 0);
        assert!(controller.state.pressed.is_empty());
    }
}
