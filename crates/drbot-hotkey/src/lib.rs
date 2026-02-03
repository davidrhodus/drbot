//! Global hotkey system for drbot.
//!
//! Provides cross-platform global keyboard shortcuts for the always-on assistant.
//!
//! # Features
//!
//! - Global hotkey registration (works when drbot is in background)
//! - Multiple hotkey combinations
//! - Customizable keybindings
//! - Platform-specific implementations (macOS, Windows, Linux)
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_hotkey::{HotkeyManager, Hotkey, Modifier, Key};
//!
//! async fn example() {
//!     let mut manager = HotkeyManager::new().await.unwrap();
//!
//!     // Register Cmd+Space (macOS) or Ctrl+Space (others) to activate
//!     manager.register(
//!         Hotkey::new(Key::Space).with_modifier(Modifier::Meta),
//!         "activate",
//!     ).await.unwrap();
//!
//!     // Listen for hotkey events
//!     while let Some(event) = manager.next_event().await {
//!         println!("Hotkey pressed: {}", event.id);
//!     }
//! }
//! ```

mod manager;
mod types;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

pub use manager::HotkeyManager;
pub use types::{Hotkey, HotkeyEvent, Key, Modifier};

use serde::{Deserialize, Serialize};

/// Result type for hotkey operations.
pub type Result<T> = std::result::Result<T, HotkeyError>;

/// Hotkey errors.
#[derive(Debug, thiserror::Error)]
pub enum HotkeyError {
    #[error("Failed to register hotkey: {0}")]
    RegistrationFailed(String),
    #[error("Hotkey already registered: {0}")]
    AlreadyRegistered(String),
    #[error("Platform not supported")]
    PlatformNotSupported,
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Hotkey configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// Enable global hotkeys.
    pub enabled: bool,
    /// Activate assistant hotkey.
    pub activate: String,
    /// Quick capture hotkey.
    pub quick_capture: Option<String>,
    /// Screenshot analysis hotkey.
    pub screenshot: Option<String>,
    /// Toggle voice mode hotkey.
    pub voice_toggle: Option<String>,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            activate: if cfg!(target_os = "macos") {
                "Cmd+Space".to_string()
            } else {
                "Ctrl+Space".to_string()
            },
            quick_capture: Some(if cfg!(target_os = "macos") {
                "Cmd+Shift+C".to_string()
            } else {
                "Ctrl+Shift+C".to_string()
            }),
            screenshot: Some(if cfg!(target_os = "macos") {
                "Cmd+Shift+S".to_string()
            } else {
                "Ctrl+Shift+S".to_string()
            }),
            voice_toggle: Some(if cfg!(target_os = "macos") {
                "Cmd+Shift+V".to_string()
            } else {
                "Ctrl+Shift+V".to_string()
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HotkeyConfig::default();
        assert!(config.enabled);
    }
}
