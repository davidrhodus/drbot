//! Keyboard control for desktop automation.
//!
//! Provides keyboard input simulation including key presses and text typing.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Keyboard modifier keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifiers {
    /// Shift key.
    Shift,
    /// Control key.
    Control,
    /// Alt/Option key.
    Alt,
    /// Command key (macOS) / Windows key.
    Meta,
    /// Caps Lock.
    CapsLock,
    /// Function key.
    Fn,
}

/// Virtual key codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    // Letters
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    // Numbers
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,

    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Navigation
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,

    // Editing
    Backspace,
    Delete,
    Insert,
    Enter,
    Tab,
    Space,
    Escape,

    // Modifiers (for completeness)
    Shift,
    Control,
    Alt,
    Meta,
    CapsLock,
    Fn,

    // Punctuation
    Comma,
    Period,
    Slash,
    Backslash,
    Semicolon,
    Quote,
    BracketLeft,
    BracketRight,
    Minus,
    Equal,
    Grave,

    // Other
    PrintScreen,
    ScrollLock,
    Pause,
    NumLock,

    // Numpad
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    NumpadEnter,
}

impl KeyCode {
    /// Convert to macOS virtual key code.
    #[cfg(target_os = "macos")]
    pub fn to_macos_keycode(&self) -> u16 {
        match self {
            KeyCode::A => 0x00,
            KeyCode::S => 0x01,
            KeyCode::D => 0x02,
            KeyCode::F => 0x03,
            KeyCode::H => 0x04,
            KeyCode::G => 0x05,
            KeyCode::Z => 0x06,
            KeyCode::X => 0x07,
            KeyCode::C => 0x08,
            KeyCode::V => 0x09,
            KeyCode::B => 0x0B,
            KeyCode::Q => 0x0C,
            KeyCode::W => 0x0D,
            KeyCode::E => 0x0E,
            KeyCode::R => 0x0F,
            KeyCode::Y => 0x10,
            KeyCode::T => 0x11,
            KeyCode::Key1 => 0x12,
            KeyCode::Key2 => 0x13,
            KeyCode::Key3 => 0x14,
            KeyCode::Key4 => 0x15,
            KeyCode::Key6 => 0x16,
            KeyCode::Key5 => 0x17,
            KeyCode::Equal => 0x18,
            KeyCode::Key9 => 0x19,
            KeyCode::Key7 => 0x1A,
            KeyCode::Minus => 0x1B,
            KeyCode::Key8 => 0x1C,
            KeyCode::Key0 => 0x1D,
            KeyCode::BracketRight => 0x1E,
            KeyCode::O => 0x1F,
            KeyCode::U => 0x20,
            KeyCode::BracketLeft => 0x21,
            KeyCode::I => 0x22,
            KeyCode::P => 0x23,
            KeyCode::Enter => 0x24,
            KeyCode::L => 0x25,
            KeyCode::J => 0x26,
            KeyCode::Quote => 0x27,
            KeyCode::K => 0x28,
            KeyCode::Semicolon => 0x29,
            KeyCode::Backslash => 0x2A,
            KeyCode::Comma => 0x2B,
            KeyCode::Slash => 0x2C,
            KeyCode::N => 0x2D,
            KeyCode::M => 0x2E,
            KeyCode::Period => 0x2F,
            KeyCode::Tab => 0x30,
            KeyCode::Space => 0x31,
            KeyCode::Grave => 0x32,
            KeyCode::Backspace => 0x33,
            KeyCode::Escape => 0x35,
            KeyCode::Meta => 0x37,
            KeyCode::Shift => 0x38,
            KeyCode::CapsLock => 0x39,
            KeyCode::Alt => 0x3A,
            KeyCode::Control => 0x3B,
            KeyCode::F1 => 0x7A,
            KeyCode::F2 => 0x78,
            KeyCode::F3 => 0x63,
            KeyCode::F4 => 0x76,
            KeyCode::F5 => 0x60,
            KeyCode::F6 => 0x61,
            KeyCode::F7 => 0x62,
            KeyCode::F8 => 0x64,
            KeyCode::F9 => 0x65,
            KeyCode::F10 => 0x6D,
            KeyCode::F11 => 0x67,
            KeyCode::F12 => 0x6F,
            KeyCode::Up => 0x7E,
            KeyCode::Down => 0x7D,
            KeyCode::Left => 0x7B,
            KeyCode::Right => 0x7C,
            KeyCode::Home => 0x73,
            KeyCode::End => 0x77,
            KeyCode::PageUp => 0x74,
            KeyCode::PageDown => 0x79,
            KeyCode::Delete => 0x75,
            _ => 0x00, // Default fallback
        }
    }
}

/// Keyboard action types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyboardAction {
    /// Type a string of text.
    Type { text: String },
    /// Press and release a key.
    Press {
        key: KeyCode,
        modifiers: Vec<Modifiers>,
    },
    /// Hold down a key.
    Down { key: KeyCode },
    /// Release a key.
    Up { key: KeyCode },
    /// Press a key combination (e.g., Cmd+C).
    Combo {
        keys: Vec<KeyCode>,
        modifiers: Vec<Modifiers>,
    },
}

/// Keyboard controller for executing keyboard actions.
#[derive(Debug)]
pub struct KeyboardController {
    /// Currently held keys.
    held_keys: HashSet<KeyCode>,
    /// Currently held modifiers.
    held_modifiers: HashSet<Modifiers>,
}

impl KeyboardController {
    /// Create a new keyboard controller.
    pub fn new() -> Self {
        Self {
            held_keys: HashSet::new(),
            held_modifiers: HashSet::new(),
        }
    }

    /// Type a string of text.
    #[cfg(target_os = "macos")]
    pub fn type_text(&mut self, text: &str) -> crate::Result<()> {
        use core_graphics::event::CGEvent;
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create event source".into())
        })?;

        for ch in text.chars() {
            // Create a keyboard event for typing
            let event = CGEvent::new_keyboard_event(source.clone(), 0, true).map_err(|_| {
                crate::ComputerError::ActionFailed("Failed to create keyboard event".into())
            })?;

            // Set the unicode character
            let mut buf = [0u16; 2];
            let chars: &[u16] = ch.encode_utf16(&mut buf);
            event.set_string_from_utf16_unchecked(chars);

            event.post(core_graphics::event::CGEventTapLocation::HID);

            // Small delay between characters
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn type_text(&mut self, _text: &str) -> crate::Result<()> {
        Err(crate::ComputerError::PlatformNotSupported)
    }

    /// Press a key with modifiers.
    #[cfg(target_os = "macos")]
    pub fn press_key(&mut self, key: KeyCode, modifiers: &[Modifiers]) -> crate::Result<()> {
        use core_graphics::event::{CGEvent, CGEventFlags};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create event source".into())
        })?;

        let keycode = key.to_macos_keycode();

        // Key down
        let down_event =
            CGEvent::new_keyboard_event(source.clone(), keycode, true).map_err(|_| {
                crate::ComputerError::ActionFailed("Failed to create key down event".into())
            })?;

        // Set modifier flags
        let mut flags = CGEventFlags::empty();
        for modifier in modifiers {
            flags |= match modifier {
                Modifiers::Shift => CGEventFlags::CGEventFlagShift,
                Modifiers::Control => CGEventFlags::CGEventFlagControl,
                Modifiers::Alt => CGEventFlags::CGEventFlagAlternate,
                Modifiers::Meta => CGEventFlags::CGEventFlagCommand,
                _ => CGEventFlags::empty(),
            };
        }
        down_event.set_flags(flags);
        down_event.post(core_graphics::event::CGEventTapLocation::HID);

        // Key up
        let up_event = CGEvent::new_keyboard_event(source, keycode, false).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create key up event".into())
        })?;
        up_event.set_flags(flags);
        up_event.post(core_graphics::event::CGEventTapLocation::HID);

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn press_key(&mut self, _key: KeyCode, _modifiers: &[Modifiers]) -> crate::Result<()> {
        Err(crate::ComputerError::PlatformNotSupported)
    }

    /// Hold down a key.
    #[cfg(target_os = "macos")]
    pub fn key_down(&mut self, key: KeyCode) -> crate::Result<()> {
        use core_graphics::event::CGEvent;
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create event source".into())
        })?;

        let keycode = key.to_macos_keycode();

        let down_event = CGEvent::new_keyboard_event(source, keycode, true).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create key down event".into())
        })?;

        down_event.post(core_graphics::event::CGEventTapLocation::HID);

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn key_down(&mut self, _key: KeyCode) -> crate::Result<()> {
        Err(crate::ComputerError::PlatformNotSupported)
    }

    /// Release a key.
    #[cfg(target_os = "macos")]
    pub fn key_up(&mut self, key: KeyCode) -> crate::Result<()> {
        use core_graphics::event::CGEvent;
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create event source".into())
        })?;

        let keycode = key.to_macos_keycode();

        let up_event = CGEvent::new_keyboard_event(source, keycode, false).map_err(|_| {
            crate::ComputerError::ActionFailed("Failed to create key up event".into())
        })?;

        up_event.post(core_graphics::event::CGEventTapLocation::HID);

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn key_up(&mut self, _key: KeyCode) -> crate::Result<()> {
        Err(crate::ComputerError::PlatformNotSupported)
    }

    /// Execute a keyboard action.
    pub fn execute(&mut self, action: KeyboardAction) -> crate::Result<()> {
        match action {
            KeyboardAction::Type { text } => self.type_text(&text),
            KeyboardAction::Press { key, modifiers } => self.press_key(key, &modifiers),
            KeyboardAction::Down { key } => {
                self.held_keys.insert(key);
                self.key_down(key)
            }
            KeyboardAction::Up { key } => {
                self.held_keys.remove(&key);
                self.key_up(key)
            }
            KeyboardAction::Combo { keys, modifiers } => {
                for key in keys {
                    self.press_key(key, &modifiers)?;
                }
                Ok(())
            }
        }
    }
}

impl Default for KeyboardController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyboard_controller_new() {
        let controller = KeyboardController::new();
        assert!(controller.held_keys.is_empty());
        assert!(controller.held_modifiers.is_empty());
    }

    #[test]
    fn test_key_code_variants() {
        // Ensure common key codes exist
        let _ = KeyCode::A;
        let _ = KeyCode::Enter;
        let _ = KeyCode::Space;
        let _ = KeyCode::Escape;
    }
}
