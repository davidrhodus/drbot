//! Smart clipboard integration for drbot.
//!
//! Monitors clipboard and provides intelligent actions based on content.
//!
//! # Features
//!
//! - Clipboard monitoring and history
//! - Content type detection
//! - Smart actions based on content
//! - Cross-platform support
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_clipboard::{ClipboardManager, ClipboardConfig};
//!
//! async fn example() {
//!     let manager = ClipboardManager::new(ClipboardConfig::default());
//!
//!     // Get current clipboard content
//!     if let Some(content) = manager.get().await {
//!         println!("Content type: {:?}", content.content_type);
//!         println!("Text: {}", content.text.unwrap_or_default());
//!     }
//!
//!     // Set clipboard content
//!     manager.set_text("Hello, World!").await;
//! }
//! ```

mod actions;
mod content;
mod history;
mod manager;

pub use actions::{ClipboardAction, SmartAction};
pub use content::{ClipboardContent, ContentType};
pub use history::{ClipboardHistory, HistoryEntry};
pub use manager::{ClipboardConfig, ClipboardManager};

/// Result type for clipboard operations.
pub type Result<T> = std::result::Result<T, ClipboardError>;

/// Clipboard errors.
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("Clipboard access failed: {0}")]
    AccessFailed(String),
    #[error("Content type not supported")]
    UnsupportedContent,
    #[error("Clipboard is empty")]
    Empty,
    #[error("Platform not supported")]
    PlatformNotSupported,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_clipboard_manager() {
        let manager = ClipboardManager::new(ClipboardConfig::default());
        // Basic smoke test
        let _ = manager.get().await;
    }
}
