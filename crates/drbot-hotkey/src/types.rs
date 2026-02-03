//! Hotkey types and definitions.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Keyboard key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
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
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,

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

    // Special keys
    Space,
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,

    // Punctuation
    Minus,
    Equal,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Period,
    Slash,
    Grave,
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Space => write!(f, "Space"),
            Key::Enter => write!(f, "Enter"),
            Key::Tab => write!(f, "Tab"),
            Key::Escape => write!(f, "Escape"),
            Key::Backspace => write!(f, "Backspace"),
            Key::Delete => write!(f, "Delete"),
            _ => write!(f, "{:?}", self),
        }
    }
}

impl Key {
    /// Parse a key from string.
    pub fn from_str(s: &str) -> Option<Key> {
        match s.to_lowercase().as_str() {
            "a" => Some(Key::A),
            "b" => Some(Key::B),
            "c" => Some(Key::C),
            "d" => Some(Key::D),
            "e" => Some(Key::E),
            "f" => Some(Key::F),
            "g" => Some(Key::G),
            "h" => Some(Key::H),
            "i" => Some(Key::I),
            "j" => Some(Key::J),
            "k" => Some(Key::K),
            "l" => Some(Key::L),
            "m" => Some(Key::M),
            "n" => Some(Key::N),
            "o" => Some(Key::O),
            "p" => Some(Key::P),
            "q" => Some(Key::Q),
            "r" => Some(Key::R),
            "s" => Some(Key::S),
            "t" => Some(Key::T),
            "u" => Some(Key::U),
            "v" => Some(Key::V),
            "w" => Some(Key::W),
            "x" => Some(Key::X),
            "y" => Some(Key::Y),
            "z" => Some(Key::Z),
            "0" => Some(Key::Num0),
            "1" => Some(Key::Num1),
            "2" => Some(Key::Num2),
            "3" => Some(Key::Num3),
            "4" => Some(Key::Num4),
            "5" => Some(Key::Num5),
            "6" => Some(Key::Num6),
            "7" => Some(Key::Num7),
            "8" => Some(Key::Num8),
            "9" => Some(Key::Num9),
            "space" => Some(Key::Space),
            "enter" | "return" => Some(Key::Enter),
            "tab" => Some(Key::Tab),
            "escape" | "esc" => Some(Key::Escape),
            "backspace" => Some(Key::Backspace),
            "delete" | "del" => Some(Key::Delete),
            "f1" => Some(Key::F1),
            "f2" => Some(Key::F2),
            "f3" => Some(Key::F3),
            "f4" => Some(Key::F4),
            "f5" => Some(Key::F5),
            "f6" => Some(Key::F6),
            "f7" => Some(Key::F7),
            "f8" => Some(Key::F8),
            "f9" => Some(Key::F9),
            "f10" => Some(Key::F10),
            "f11" => Some(Key::F11),
            "f12" => Some(Key::F12),
            "up" => Some(Key::Up),
            "down" => Some(Key::Down),
            "left" => Some(Key::Left),
            "right" => Some(Key::Right),
            "home" => Some(Key::Home),
            "end" => Some(Key::End),
            "pageup" | "pgup" => Some(Key::PageUp),
            "pagedown" | "pgdn" => Some(Key::PageDown),
            _ => None,
        }
    }
}

/// Modifier key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifier {
    /// Control key (Ctrl)
    Control,
    /// Shift key
    Shift,
    /// Alt/Option key
    Alt,
    /// Meta key (Cmd on macOS, Win on Windows)
    Meta,
}

impl fmt::Display for Modifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Modifier::Control => write!(f, "Ctrl"),
            Modifier::Shift => write!(f, "Shift"),
            Modifier::Alt => {
                if cfg!(target_os = "macos") {
                    write!(f, "Option")
                } else {
                    write!(f, "Alt")
                }
            }
            Modifier::Meta => {
                if cfg!(target_os = "macos") {
                    write!(f, "Cmd")
                } else {
                    write!(f, "Win")
                }
            }
        }
    }
}

impl Modifier {
    /// Parse a modifier from string.
    pub fn from_str(s: &str) -> Option<Modifier> {
        match s.to_lowercase().as_str() {
            "ctrl" | "control" => Some(Modifier::Control),
            "shift" => Some(Modifier::Shift),
            "alt" | "option" | "opt" => Some(Modifier::Alt),
            "meta" | "cmd" | "command" | "win" | "super" => Some(Modifier::Meta),
            _ => None,
        }
    }
}

/// A hotkey combination.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hotkey {
    /// The main key.
    pub key: Key,
    /// Required modifiers.
    pub modifiers: Vec<Modifier>,
}

impl Hotkey {
    /// Create a new hotkey with just a key.
    pub fn new(key: Key) -> Self {
        Self {
            key,
            modifiers: Vec::new(),
        }
    }

    /// Add a modifier.
    pub fn with_modifier(mut self, modifier: Modifier) -> Self {
        if !self.modifiers.contains(&modifier) {
            self.modifiers.push(modifier);
        }
        self
    }

    /// Parse a hotkey from string like "Cmd+Shift+C".
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();

        if parts.is_empty() {
            return None;
        }

        // Last part is the key
        let key = Key::from_str(parts.last()?)?;

        // All other parts are modifiers
        let mut modifiers = Vec::new();
        for part in &parts[..parts.len() - 1] {
            if let Some(modifier) = Modifier::from_str(part) {
                modifiers.push(modifier);
            }
        }

        Some(Hotkey { key, modifiers })
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self
            .modifiers
            .iter()
            .map(|m| m.to_string())
            .chain(std::iter::once(self.key.to_string()))
            .collect();
        write!(f, "{}", parts.join("+"))
    }
}

/// Event emitted when a hotkey is pressed.
#[derive(Debug, Clone)]
pub struct HotkeyEvent {
    /// ID of the registered hotkey.
    pub id: String,
    /// The hotkey that was pressed.
    pub hotkey: Hotkey,
    /// Timestamp when pressed.
    pub timestamp: std::time::Instant,
}

impl HotkeyEvent {
    /// Create a new hotkey event.
    pub fn new(id: impl Into<String>, hotkey: Hotkey) -> Self {
        Self {
            id: id.into(),
            hotkey,
            timestamp: std::time::Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotkey_parse() {
        let hk = Hotkey::parse("Cmd+Space").unwrap();
        assert_eq!(hk.key, Key::Space);
        assert!(hk.modifiers.contains(&Modifier::Meta));
    }

    #[test]
    fn test_hotkey_parse_multiple_modifiers() {
        let hk = Hotkey::parse("Ctrl+Shift+C").unwrap();
        assert_eq!(hk.key, Key::C);
        assert!(hk.modifiers.contains(&Modifier::Control));
        assert!(hk.modifiers.contains(&Modifier::Shift));
    }

    #[test]
    fn test_hotkey_display() {
        let hk = Hotkey::new(Key::Space).with_modifier(Modifier::Meta);
        let display = hk.to_string();
        assert!(display.contains("Space"));
    }

    #[test]
    fn test_key_parse() {
        assert_eq!(Key::from_str("a"), Some(Key::A));
        assert_eq!(Key::from_str("SPACE"), Some(Key::Space));
        assert_eq!(Key::from_str("f1"), Some(Key::F1));
    }

    #[test]
    fn test_modifier_parse() {
        assert_eq!(Modifier::from_str("ctrl"), Some(Modifier::Control));
        assert_eq!(Modifier::from_str("Cmd"), Some(Modifier::Meta));
        assert_eq!(Modifier::from_str("Option"), Some(Modifier::Alt));
    }
}
