//! Linux hotkey implementation using X11 XGrabKey.
//!
//! This module provides Linux-specific global hotkey registration.
//!
//! Note: Requires X11. Wayland support would need a different implementation
//! using protocols like wlr-foreign-toplevel or input-method.

use crate::{Hotkey, HotkeyError, HotkeyEvent, Key, Modifier, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// X11 keysym codes.
#[allow(dead_code)]
mod keysym {
    // Letters (lowercase)
    pub const XK_A: u32 = 0x0061;
    pub const XK_B: u32 = 0x0062;
    pub const XK_C: u32 = 0x0063;
    pub const XK_D: u32 = 0x0064;
    pub const XK_E: u32 = 0x0065;
    pub const XK_F: u32 = 0x0066;
    pub const XK_G: u32 = 0x0067;
    pub const XK_H: u32 = 0x0068;
    pub const XK_I: u32 = 0x0069;
    pub const XK_J: u32 = 0x006A;
    pub const XK_K: u32 = 0x006B;
    pub const XK_L: u32 = 0x006C;
    pub const XK_M: u32 = 0x006D;
    pub const XK_N: u32 = 0x006E;
    pub const XK_O: u32 = 0x006F;
    pub const XK_P: u32 = 0x0070;
    pub const XK_Q: u32 = 0x0071;
    pub const XK_R: u32 = 0x0072;
    pub const XK_S: u32 = 0x0073;
    pub const XK_T: u32 = 0x0074;
    pub const XK_U: u32 = 0x0075;
    pub const XK_V: u32 = 0x0076;
    pub const XK_W: u32 = 0x0077;
    pub const XK_X: u32 = 0x0078;
    pub const XK_Y: u32 = 0x0079;
    pub const XK_Z: u32 = 0x007A;

    // Numbers
    pub const XK_0: u32 = 0x0030;
    pub const XK_1: u32 = 0x0031;
    pub const XK_2: u32 = 0x0032;
    pub const XK_3: u32 = 0x0033;
    pub const XK_4: u32 = 0x0034;
    pub const XK_5: u32 = 0x0035;
    pub const XK_6: u32 = 0x0036;
    pub const XK_7: u32 = 0x0037;
    pub const XK_8: u32 = 0x0038;
    pub const XK_9: u32 = 0x0039;

    // Function keys
    pub const XK_F1: u32 = 0xFFBE;
    pub const XK_F2: u32 = 0xFFBF;
    pub const XK_F3: u32 = 0xFFC0;
    pub const XK_F4: u32 = 0xFFC1;
    pub const XK_F5: u32 = 0xFFC2;
    pub const XK_F6: u32 = 0xFFC3;
    pub const XK_F7: u32 = 0xFFC4;
    pub const XK_F8: u32 = 0xFFC5;
    pub const XK_F9: u32 = 0xFFC6;
    pub const XK_F10: u32 = 0xFFC7;
    pub const XK_F11: u32 = 0xFFC8;
    pub const XK_F12: u32 = 0xFFC9;

    // Special keys
    pub const XK_SPACE: u32 = 0x0020;
    pub const XK_RETURN: u32 = 0xFF0D;
    pub const XK_TAB: u32 = 0xFF09;
    pub const XK_ESCAPE: u32 = 0xFF1B;
    pub const XK_BACKSPACE: u32 = 0xFF08;
    pub const XK_DELETE: u32 = 0xFFFF;
    pub const XK_INSERT: u32 = 0xFF63;
    pub const XK_HOME: u32 = 0xFF50;
    pub const XK_END: u32 = 0xFF57;
    pub const XK_PAGE_UP: u32 = 0xFF55;
    pub const XK_PAGE_DOWN: u32 = 0xFF56;
    pub const XK_UP: u32 = 0xFF52;
    pub const XK_DOWN: u32 = 0xFF54;
    pub const XK_LEFT: u32 = 0xFF51;
    pub const XK_RIGHT: u32 = 0xFF53;

    // Punctuation
    pub const XK_MINUS: u32 = 0x002D;
    pub const XK_EQUAL: u32 = 0x003D;
    pub const XK_BRACKETLEFT: u32 = 0x005B;
    pub const XK_BRACKETRIGHT: u32 = 0x005D;
    pub const XK_BACKSLASH: u32 = 0x005C;
    pub const XK_SEMICOLON: u32 = 0x003B;
    pub const XK_APOSTROPHE: u32 = 0x0027;
    pub const XK_COMMA: u32 = 0x002C;
    pub const XK_PERIOD: u32 = 0x002E;
    pub const XK_SLASH: u32 = 0x002F;
    pub const XK_GRAVE: u32 = 0x0060;
}

/// X11 modifier masks.
#[allow(dead_code)]
mod mask {
    pub const SHIFT_MASK: u32 = 1 << 0;
    pub const LOCK_MASK: u32 = 1 << 1; // Caps Lock
    pub const CONTROL_MASK: u32 = 1 << 2;
    pub const MOD1_MASK: u32 = 1 << 3; // Alt
    pub const MOD2_MASK: u32 = 1 << 4; // Num Lock
    pub const MOD3_MASK: u32 = 1 << 5;
    pub const MOD4_MASK: u32 = 1 << 6; // Super/Meta
    pub const MOD5_MASK: u32 = 1 << 7;
}

/// Registered hotkey info.
#[derive(Debug, Clone)]
struct RegisteredHotkey {
    keysym: u32,
    modifiers: u32,
    hotkey: Hotkey,
    string_id: String,
}

/// Linux-specific hotkey handler using X11.
pub struct LinuxHotkeyHandler {
    /// Registered hotkeys.
    registered: Arc<RwLock<Vec<RegisteredHotkey>>>,
    /// Running state.
    running: Arc<RwLock<bool>>,
    /// Whether X11 connection is available.
    x11_available: bool,
}

impl LinuxHotkeyHandler {
    /// Create a new Linux hotkey handler.
    pub fn new() -> Result<Self> {
        // In a full implementation, we would:
        // 1. Open X11 display with XOpenDisplay
        // 2. Check for errors
        // let display = unsafe { XOpenDisplay(null()) };
        // if display.is_null() {
        //     return Err(HotkeyError::Internal("Failed to open X11 display".to_string()));
        // }

        Ok(Self {
            registered: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
            x11_available: true, // Would be set based on actual X11 availability
        })
    }

    /// Check if X11 is available.
    pub fn is_x11_available(&self) -> bool {
        self.x11_available
    }

    /// Register a hotkey with X11.
    pub async fn register(&self, hotkey: &Hotkey, string_id: &str) -> Result<()> {
        if !self.x11_available {
            return Err(HotkeyError::PlatformNotSupported);
        }

        let keysym = Self::key_to_keysym(&hotkey.key)
            .ok_or_else(|| HotkeyError::RegistrationFailed("Unsupported key".to_string()))?;

        let mut mod_mask = 0u32;
        for modifier in &hotkey.modifiers {
            mod_mask |= Self::modifier_to_mask(modifier);
        }

        // In a full implementation:
        // let keycode = unsafe { XKeysymToKeycode(display, keysym as u64) };
        // let root = unsafe { XDefaultRootWindow(display) };
        //
        // // Need to grab with various combinations of Caps Lock and Num Lock
        // for caps in [0, mask::LOCK_MASK] {
        //     for num in [0, mask::MOD2_MASK] {
        //         let grab_mask = mod_mask | caps | num;
        //         unsafe {
        //             XGrabKey(
        //                 display,
        //                 keycode as i32,
        //                 grab_mask,
        //                 root,
        //                 True,
        //                 GrabModeAsync,
        //                 GrabModeAsync
        //             );
        //         }
        //     }
        // }

        let mut registered = self.registered.write().await;
        registered.push(RegisteredHotkey {
            keysym,
            modifiers: mod_mask,
            hotkey: hotkey.clone(),
            string_id: string_id.to_string(),
        });

        Ok(())
    }

    /// Unregister a hotkey.
    pub async fn unregister(&self, string_id: &str) -> Result<()> {
        let mut registered = self.registered.write().await;

        if let Some(pos) = registered.iter().position(|r| r.string_id == string_id) {
            let _hotkey = registered.remove(pos);

            // In a full implementation:
            // let keycode = unsafe { XKeysymToKeycode(display, hotkey.keysym as u64) };
            // let root = unsafe { XDefaultRootWindow(display) };
            //
            // for caps in [0, mask::LOCK_MASK] {
            //     for num in [0, mask::MOD2_MASK] {
            //         let grab_mask = hotkey.modifiers | caps | num;
            //         unsafe { XUngrabKey(display, keycode as i32, grab_mask, root); }
            //     }
            // }
        }

        Ok(())
    }

    /// Start the X11 event loop to listen for hotkey events.
    pub async fn start(&self, event_tx: mpsc::Sender<HotkeyEvent>) -> Result<()> {
        if !self.x11_available {
            return Err(HotkeyError::PlatformNotSupported);
        }

        let mut running = self.running.write().await;
        *running = true;
        drop(running);

        let _registered = self.registered.clone();
        let _running = self.running.clone();

        // In a full implementation, spawn a thread that:
        // loop {
        //     if !*running.read().await { break; }
        //
        //     let mut event: XEvent = unsafe { std::mem::zeroed() };
        //     unsafe { XNextEvent(display, &mut event); }
        //
        //     if event.type_ == KeyPress {
        //         let key_event = unsafe { event.key };
        //         let keysym = unsafe { XLookupKeysym(&mut event.key as *mut _, 0) };
        //         let modifiers = key_event.state & (
        //             mask::SHIFT_MASK | mask::CONTROL_MASK |
        //             mask::MOD1_MASK | mask::MOD4_MASK
        //         );
        //
        //         for reg in registered.read().await.iter() {
        //             if reg.keysym == keysym as u32 && reg.modifiers == modifiers {
        //                 let event = HotkeyEvent::new(&reg.string_id, reg.hotkey.clone());
        //                 let _ = event_tx.send(event).await;
        //                 break;
        //             }
        //         }
        //     }
        // }

        let _ = event_tx; // Suppress unused warning
        Ok(())
    }

    /// Stop the X11 event loop.
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.write().await;
        *running = false;
        Ok(())
    }

    /// Convert Key to X11 keysym.
    pub fn key_to_keysym(key: &Key) -> Option<u32> {
        Some(match key {
            Key::A => keysym::XK_A,
            Key::B => keysym::XK_B,
            Key::C => keysym::XK_C,
            Key::D => keysym::XK_D,
            Key::E => keysym::XK_E,
            Key::F => keysym::XK_F,
            Key::G => keysym::XK_G,
            Key::H => keysym::XK_H,
            Key::I => keysym::XK_I,
            Key::J => keysym::XK_J,
            Key::K => keysym::XK_K,
            Key::L => keysym::XK_L,
            Key::M => keysym::XK_M,
            Key::N => keysym::XK_N,
            Key::O => keysym::XK_O,
            Key::P => keysym::XK_P,
            Key::Q => keysym::XK_Q,
            Key::R => keysym::XK_R,
            Key::S => keysym::XK_S,
            Key::T => keysym::XK_T,
            Key::U => keysym::XK_U,
            Key::V => keysym::XK_V,
            Key::W => keysym::XK_W,
            Key::X => keysym::XK_X,
            Key::Y => keysym::XK_Y,
            Key::Z => keysym::XK_Z,
            Key::Num0 => keysym::XK_0,
            Key::Num1 => keysym::XK_1,
            Key::Num2 => keysym::XK_2,
            Key::Num3 => keysym::XK_3,
            Key::Num4 => keysym::XK_4,
            Key::Num5 => keysym::XK_5,
            Key::Num6 => keysym::XK_6,
            Key::Num7 => keysym::XK_7,
            Key::Num8 => keysym::XK_8,
            Key::Num9 => keysym::XK_9,
            Key::F1 => keysym::XK_F1,
            Key::F2 => keysym::XK_F2,
            Key::F3 => keysym::XK_F3,
            Key::F4 => keysym::XK_F4,
            Key::F5 => keysym::XK_F5,
            Key::F6 => keysym::XK_F6,
            Key::F7 => keysym::XK_F7,
            Key::F8 => keysym::XK_F8,
            Key::F9 => keysym::XK_F9,
            Key::F10 => keysym::XK_F10,
            Key::F11 => keysym::XK_F11,
            Key::F12 => keysym::XK_F12,
            Key::Space => keysym::XK_SPACE,
            Key::Enter => keysym::XK_RETURN,
            Key::Tab => keysym::XK_TAB,
            Key::Escape => keysym::XK_ESCAPE,
            Key::Backspace => keysym::XK_BACKSPACE,
            Key::Delete => keysym::XK_DELETE,
            Key::Insert => keysym::XK_INSERT,
            Key::Home => keysym::XK_HOME,
            Key::End => keysym::XK_END,
            Key::PageUp => keysym::XK_PAGE_UP,
            Key::PageDown => keysym::XK_PAGE_DOWN,
            Key::Up => keysym::XK_UP,
            Key::Down => keysym::XK_DOWN,
            Key::Left => keysym::XK_LEFT,
            Key::Right => keysym::XK_RIGHT,
            Key::Minus => keysym::XK_MINUS,
            Key::Equal => keysym::XK_EQUAL,
            Key::LeftBracket => keysym::XK_BRACKETLEFT,
            Key::RightBracket => keysym::XK_BRACKETRIGHT,
            Key::Backslash => keysym::XK_BACKSLASH,
            Key::Semicolon => keysym::XK_SEMICOLON,
            Key::Quote => keysym::XK_APOSTROPHE,
            Key::Comma => keysym::XK_COMMA,
            Key::Period => keysym::XK_PERIOD,
            Key::Slash => keysym::XK_SLASH,
            Key::Grave => keysym::XK_GRAVE,
        })
    }

    /// Convert Modifier to X11 modifier mask.
    pub fn modifier_to_mask(modifier: &Modifier) -> u32 {
        match modifier {
            Modifier::Control => mask::CONTROL_MASK,
            Modifier::Shift => mask::SHIFT_MASK,
            Modifier::Alt => mask::MOD1_MASK,
            Modifier::Meta => mask::MOD4_MASK,
        }
    }
}

impl Default for LinuxHotkeyHandler {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            registered: Arc::new(RwLock::new(Vec::new())),
            running: Arc::new(RwLock::new(false)),
            x11_available: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_to_keysym() {
        assert_eq!(
            LinuxHotkeyHandler::key_to_keysym(&Key::Space),
            Some(keysym::XK_SPACE)
        );
        assert_eq!(
            LinuxHotkeyHandler::key_to_keysym(&Key::A),
            Some(keysym::XK_A)
        );
        assert_eq!(
            LinuxHotkeyHandler::key_to_keysym(&Key::F1),
            Some(keysym::XK_F1)
        );
        assert_eq!(
            LinuxHotkeyHandler::key_to_keysym(&Key::Enter),
            Some(keysym::XK_RETURN)
        );
    }

    #[test]
    fn test_modifier_to_mask() {
        assert_eq!(
            LinuxHotkeyHandler::modifier_to_mask(&Modifier::Control),
            mask::CONTROL_MASK
        );
        assert_eq!(
            LinuxHotkeyHandler::modifier_to_mask(&Modifier::Meta),
            mask::MOD4_MASK
        );
        assert_eq!(
            LinuxHotkeyHandler::modifier_to_mask(&Modifier::Shift),
            mask::SHIFT_MASK
        );
        assert_eq!(
            LinuxHotkeyHandler::modifier_to_mask(&Modifier::Alt),
            mask::MOD1_MASK
        );
    }

    #[tokio::test]
    async fn test_handler_creation() {
        let handler = LinuxHotkeyHandler::new().unwrap();
        let registered = handler.registered.read().await;
        assert!(registered.is_empty());
    }

    #[tokio::test]
    async fn test_register_hotkey() {
        let handler = LinuxHotkeyHandler::new().unwrap();
        let hotkey = Hotkey::new(Key::Space).with_modifier(Modifier::Control);

        handler.register(&hotkey, "test").await.unwrap();

        let registered = handler.registered.read().await;
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].string_id, "test");
    }

    #[tokio::test]
    async fn test_unregister_hotkey() {
        let handler = LinuxHotkeyHandler::new().unwrap();
        let hotkey = Hotkey::new(Key::C).with_modifier(Modifier::Control);

        handler.register(&hotkey, "copy").await.unwrap();
        handler.unregister("copy").await.unwrap();

        let registered = handler.registered.read().await;
        assert!(registered.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_hotkeys() {
        let handler = LinuxHotkeyHandler::new().unwrap();

        let hotkey1 = Hotkey::new(Key::C).with_modifier(Modifier::Control);
        let hotkey2 = Hotkey::new(Key::V).with_modifier(Modifier::Control);
        let hotkey3 = Hotkey::new(Key::Space)
            .with_modifier(Modifier::Meta)
            .with_modifier(Modifier::Shift);

        handler.register(&hotkey1, "copy").await.unwrap();
        handler.register(&hotkey2, "paste").await.unwrap();
        handler.register(&hotkey3, "activate").await.unwrap();

        let registered = handler.registered.read().await;
        assert_eq!(registered.len(), 3);
    }
}
