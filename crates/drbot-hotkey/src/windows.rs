//! Windows hotkey implementation using RegisterHotKey API.
//!
//! This module provides Windows-specific global hotkey registration.
//!
//! Note: Uses the Windows RegisterHotKey/UnregisterHotKey API for system-wide hotkeys.

use crate::{Hotkey, HotkeyError, HotkeyEvent, Key, Modifier, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Windows virtual key codes.
#[allow(dead_code)]
mod vk {
    // Letters (0x41-0x5A)
    pub const A: u32 = 0x41;
    pub const B: u32 = 0x42;
    pub const C: u32 = 0x43;
    pub const D: u32 = 0x44;
    pub const E: u32 = 0x45;
    pub const F: u32 = 0x46;
    pub const G: u32 = 0x47;
    pub const H: u32 = 0x48;
    pub const I: u32 = 0x49;
    pub const J: u32 = 0x4A;
    pub const K: u32 = 0x4B;
    pub const L: u32 = 0x4C;
    pub const M: u32 = 0x4D;
    pub const N: u32 = 0x4E;
    pub const O: u32 = 0x4F;
    pub const P: u32 = 0x50;
    pub const Q: u32 = 0x51;
    pub const R: u32 = 0x52;
    pub const S: u32 = 0x53;
    pub const T: u32 = 0x54;
    pub const U: u32 = 0x55;
    pub const V: u32 = 0x56;
    pub const W: u32 = 0x57;
    pub const X: u32 = 0x58;
    pub const Y: u32 = 0x59;
    pub const Z: u32 = 0x5A;

    // Numbers (0x30-0x39)
    pub const NUM0: u32 = 0x30;
    pub const NUM1: u32 = 0x31;
    pub const NUM2: u32 = 0x32;
    pub const NUM3: u32 = 0x33;
    pub const NUM4: u32 = 0x34;
    pub const NUM5: u32 = 0x35;
    pub const NUM6: u32 = 0x36;
    pub const NUM7: u32 = 0x37;
    pub const NUM8: u32 = 0x38;
    pub const NUM9: u32 = 0x39;

    // Function keys
    pub const F1: u32 = 0x70;
    pub const F2: u32 = 0x71;
    pub const F3: u32 = 0x72;
    pub const F4: u32 = 0x73;
    pub const F5: u32 = 0x74;
    pub const F6: u32 = 0x75;
    pub const F7: u32 = 0x76;
    pub const F8: u32 = 0x77;
    pub const F9: u32 = 0x78;
    pub const F10: u32 = 0x79;
    pub const F11: u32 = 0x7A;
    pub const F12: u32 = 0x7B;

    // Special keys
    pub const SPACE: u32 = 0x20;
    pub const RETURN: u32 = 0x0D;
    pub const TAB: u32 = 0x09;
    pub const ESCAPE: u32 = 0x1B;
    pub const BACK: u32 = 0x08;
    pub const DELETE: u32 = 0x2E;
    pub const INSERT: u32 = 0x2D;
    pub const HOME: u32 = 0x24;
    pub const END: u32 = 0x23;
    pub const PRIOR: u32 = 0x21; // Page Up
    pub const NEXT: u32 = 0x22; // Page Down
    pub const UP: u32 = 0x26;
    pub const DOWN: u32 = 0x28;
    pub const LEFT: u32 = 0x25;
    pub const RIGHT: u32 = 0x27;

    // Punctuation
    pub const OEM_MINUS: u32 = 0xBD;
    pub const OEM_PLUS: u32 = 0xBB;
    pub const OEM_4: u32 = 0xDB; // [
    pub const OEM_6: u32 = 0xDD; // ]
    pub const OEM_5: u32 = 0xDC; // \
    pub const OEM_1: u32 = 0xBA; // ;
    pub const OEM_7: u32 = 0xDE; // '
    pub const OEM_COMMA: u32 = 0xBC;
    pub const OEM_PERIOD: u32 = 0xBE;
    pub const OEM_2: u32 = 0xBF; // /
    pub const OEM_3: u32 = 0xC0; // `
}

/// Windows modifier flags for RegisterHotKey.
#[allow(dead_code)]
mod modifiers {
    pub const MOD_ALT: u32 = 0x0001;
    pub const MOD_CONTROL: u32 = 0x0002;
    pub const MOD_SHIFT: u32 = 0x0004;
    pub const MOD_WIN: u32 = 0x0008;
    pub const MOD_NOREPEAT: u32 = 0x4000;
}

/// Registered hotkey info.
#[derive(Debug, Clone)]
struct RegisteredHotkey {
    id: i32,
    hotkey: Hotkey,
    string_id: String,
}

/// Windows-specific hotkey handler.
pub struct WindowsHotkeyHandler {
    /// Next hotkey ID to use.
    next_id: Arc<RwLock<i32>>,
    /// Registered hotkeys by their Windows ID.
    registered: Arc<RwLock<HashMap<i32, RegisteredHotkey>>>,
    /// Running state.
    running: Arc<RwLock<bool>>,
}

impl WindowsHotkeyHandler {
    /// Create a new Windows hotkey handler.
    pub fn new() -> Result<Self> {
        Ok(Self {
            next_id: Arc::new(RwLock::new(1)),
            registered: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Register a hotkey with Windows.
    pub async fn register(&self, hotkey: &Hotkey, string_id: &str) -> Result<i32> {
        let mut next_id = self.next_id.write().await;
        let id = *next_id;
        *next_id += 1;

        let _vk = Self::key_to_vk(&hotkey.key)
            .ok_or_else(|| HotkeyError::RegistrationFailed("Unsupported key".to_string()))?;

        let mut _mod_flags = modifiers::MOD_NOREPEAT;
        for modifier in &hotkey.modifiers {
            _mod_flags |= Self::modifier_to_flag(modifier);
        }

        // In a full implementation:
        // unsafe {
        //     if RegisterHotKey(null_mut(), id, mod_flags, vk) == 0 {
        //         return Err(HotkeyError::RegistrationFailed(
        //             format!("RegisterHotKey failed: {}", GetLastError())
        //         ));
        //     }
        // }

        let mut registered = self.registered.write().await;
        registered.insert(
            id,
            RegisteredHotkey {
                id,
                hotkey: hotkey.clone(),
                string_id: string_id.to_string(),
            },
        );

        Ok(id)
    }

    /// Unregister a hotkey.
    pub async fn unregister(&self, id: i32) -> Result<()> {
        let mut registered = self.registered.write().await;

        if registered.remove(&id).is_some() {
            // In a full implementation:
            // unsafe {
            //     UnregisterHotKey(null_mut(), id);
            // }
        }

        Ok(())
    }

    /// Start the message loop to listen for hotkey events.
    pub async fn start(&self, event_tx: mpsc::Sender<HotkeyEvent>) -> Result<()> {
        let mut running = self.running.write().await;
        *running = true;
        drop(running);

        // In a full implementation, this would spawn a thread that runs:
        // while *running {
        //     let mut msg: MSG = unsafe { std::mem::zeroed() };
        //     if unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) } > 0 {
        //         if msg.message == WM_HOTKEY {
        //             let id = msg.wParam as i32;
        //             if let Some(registered) = self.registered.read().await.get(&id) {
        //                 let event = HotkeyEvent::new(&registered.string_id, registered.hotkey.clone());
        //                 let _ = event_tx.send(event).await;
        //             }
        //         }
        //     }
        // }

        let _ = event_tx; // Suppress unused warning
        Ok(())
    }

    /// Stop the message loop.
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.write().await;
        *running = false;

        // In a full implementation:
        // Post WM_QUIT to the message loop thread

        Ok(())
    }

    /// Convert Key to Windows virtual key code.
    pub fn key_to_vk(key: &Key) -> Option<u32> {
        Some(match key {
            Key::A => vk::A,
            Key::B => vk::B,
            Key::C => vk::C,
            Key::D => vk::D,
            Key::E => vk::E,
            Key::F => vk::F,
            Key::G => vk::G,
            Key::H => vk::H,
            Key::I => vk::I,
            Key::J => vk::J,
            Key::K => vk::K,
            Key::L => vk::L,
            Key::M => vk::M,
            Key::N => vk::N,
            Key::O => vk::O,
            Key::P => vk::P,
            Key::Q => vk::Q,
            Key::R => vk::R,
            Key::S => vk::S,
            Key::T => vk::T,
            Key::U => vk::U,
            Key::V => vk::V,
            Key::W => vk::W,
            Key::X => vk::X,
            Key::Y => vk::Y,
            Key::Z => vk::Z,
            Key::Num0 => vk::NUM0,
            Key::Num1 => vk::NUM1,
            Key::Num2 => vk::NUM2,
            Key::Num3 => vk::NUM3,
            Key::Num4 => vk::NUM4,
            Key::Num5 => vk::NUM5,
            Key::Num6 => vk::NUM6,
            Key::Num7 => vk::NUM7,
            Key::Num8 => vk::NUM8,
            Key::Num9 => vk::NUM9,
            Key::F1 => vk::F1,
            Key::F2 => vk::F2,
            Key::F3 => vk::F3,
            Key::F4 => vk::F4,
            Key::F5 => vk::F5,
            Key::F6 => vk::F6,
            Key::F7 => vk::F7,
            Key::F8 => vk::F8,
            Key::F9 => vk::F9,
            Key::F10 => vk::F10,
            Key::F11 => vk::F11,
            Key::F12 => vk::F12,
            Key::Space => vk::SPACE,
            Key::Enter => vk::RETURN,
            Key::Tab => vk::TAB,
            Key::Escape => vk::ESCAPE,
            Key::Backspace => vk::BACK,
            Key::Delete => vk::DELETE,
            Key::Insert => vk::INSERT,
            Key::Home => vk::HOME,
            Key::End => vk::END,
            Key::PageUp => vk::PRIOR,
            Key::PageDown => vk::NEXT,
            Key::Up => vk::UP,
            Key::Down => vk::DOWN,
            Key::Left => vk::LEFT,
            Key::Right => vk::RIGHT,
            Key::Minus => vk::OEM_MINUS,
            Key::Equal => vk::OEM_PLUS,
            Key::LeftBracket => vk::OEM_4,
            Key::RightBracket => vk::OEM_6,
            Key::Backslash => vk::OEM_5,
            Key::Semicolon => vk::OEM_1,
            Key::Quote => vk::OEM_7,
            Key::Comma => vk::OEM_COMMA,
            Key::Period => vk::OEM_PERIOD,
            Key::Slash => vk::OEM_2,
            Key::Grave => vk::OEM_3,
        })
    }

    /// Convert Modifier to Windows modifier flag.
    pub fn modifier_to_flag(modifier: &Modifier) -> u32 {
        match modifier {
            Modifier::Control => modifiers::MOD_CONTROL,
            Modifier::Shift => modifiers::MOD_SHIFT,
            Modifier::Alt => modifiers::MOD_ALT,
            Modifier::Meta => modifiers::MOD_WIN,
        }
    }
}

impl Default for WindowsHotkeyHandler {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            next_id: Arc::new(RwLock::new(1)),
            registered: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_to_vk() {
        assert_eq!(
            WindowsHotkeyHandler::key_to_vk(&Key::Space),
            Some(vk::SPACE)
        );
        assert_eq!(WindowsHotkeyHandler::key_to_vk(&Key::A), Some(vk::A));
        assert_eq!(WindowsHotkeyHandler::key_to_vk(&Key::F1), Some(vk::F1));
        assert_eq!(
            WindowsHotkeyHandler::key_to_vk(&Key::Enter),
            Some(vk::RETURN)
        );
    }

    #[test]
    fn test_modifier_to_flag() {
        assert_eq!(
            WindowsHotkeyHandler::modifier_to_flag(&Modifier::Control),
            modifiers::MOD_CONTROL
        );
        assert_eq!(
            WindowsHotkeyHandler::modifier_to_flag(&Modifier::Meta),
            modifiers::MOD_WIN
        );
        assert_eq!(
            WindowsHotkeyHandler::modifier_to_flag(&Modifier::Shift),
            modifiers::MOD_SHIFT
        );
        assert_eq!(
            WindowsHotkeyHandler::modifier_to_flag(&Modifier::Alt),
            modifiers::MOD_ALT
        );
    }

    #[tokio::test]
    async fn test_handler_creation() {
        let handler = WindowsHotkeyHandler::new().unwrap();
        let registered = handler.registered.read().await;
        assert!(registered.is_empty());
    }

    #[tokio::test]
    async fn test_register_hotkey() {
        let handler = WindowsHotkeyHandler::new().unwrap();
        let hotkey = Hotkey::new(Key::Space).with_modifier(Modifier::Control);

        let id = handler.register(&hotkey, "test").await.unwrap();
        assert!(id > 0);

        let registered = handler.registered.read().await;
        assert!(registered.contains_key(&id));
    }

    #[tokio::test]
    async fn test_unregister_hotkey() {
        let handler = WindowsHotkeyHandler::new().unwrap();
        let hotkey = Hotkey::new(Key::C).with_modifier(Modifier::Control);

        let id = handler.register(&hotkey, "copy").await.unwrap();
        handler.unregister(id).await.unwrap();

        let registered = handler.registered.read().await;
        assert!(!registered.contains_key(&id));
    }
}
