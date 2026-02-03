//! Screen context reading for drbot.
//!
//! Provides access to on-screen content using accessibility APIs and
//! screenshot capabilities for visual context.
//!
//! # Features
//!
//! - Read text from focused application
//! - Get current window information
//! - Capture screenshots
//! - Extract text from images (OCR integration ready)
//! - Accessibility tree traversal
//!
//! # Permissions Required
//!
//! - macOS: Accessibility permission in System Preferences
//! - Windows: UI Automation access (usually granted)
//! - Linux: X11 or AT-SPI access
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_screen::{ScreenContext, FocusedElement};
//!
//! async fn example() {
//!     let ctx = ScreenContext::new().await.unwrap();
//!
//!     // Get focused application info
//!     if let Some(app) = ctx.get_focused_app().await {
//!         println!("Focused app: {}", app.name);
//!     }
//!
//!     // Get text from focused element
//!     if let Some(element) = ctx.get_focused_element().await {
//!         println!("Focused element: {:?}", element);
//!     }
//!
//!     // Capture screenshot
//!     let screenshot = ctx.capture_screen().await.unwrap();
//! }
//! ```

mod accessibility;
mod capture;
mod context;
mod smart_context;

#[cfg(target_os = "macos")]
mod macos;

pub use accessibility::{
    AccessibilityNode, AccessibilityTree, ElementType, FocusedApp, FocusedElement,
};
pub use capture::{CaptureOptions, Screenshot};
pub use context::{ScreenContext, ScreenContextConfig};
pub use smart_context::{
    AppPattern, ContextContent, ContextSuggestion, ContextType, RelatedResource, ResourceType,
    SmartContext, SmartContextExtractor, SuggestionType,
};

use serde::{Deserialize, Serialize};

/// Result type for screen operations.
pub type Result<T> = std::result::Result<T, ScreenError>;

/// Screen-related errors.
#[derive(Debug, thiserror::Error)]
pub enum ScreenError {
    #[error("Accessibility permission denied")]
    PermissionDenied,
    #[error("No focused element")]
    NoFocusedElement,
    #[error("Screenshot failed: {0}")]
    ScreenshotFailed(String),
    #[error("Platform not supported")]
    PlatformNotSupported,
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Screen context configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Enable accessibility reading.
    pub enable_accessibility: bool,
    /// Enable screenshot capture.
    pub enable_screenshots: bool,
    /// Maximum text to extract from screen.
    pub max_text_length: usize,
    /// Include UI hierarchy.
    pub include_hierarchy: bool,
    /// Capture interval for continuous monitoring (ms).
    pub capture_interval_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enable_accessibility: true,
            enable_screenshots: true,
            max_text_length: 10000,
            include_hierarchy: false,
            capture_interval_ms: 1000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.enable_accessibility);
        assert!(config.enable_screenshots);
    }
}
