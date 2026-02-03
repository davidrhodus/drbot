//! Clipboard manager implementation.

use crate::content::ClipboardContent;
use crate::history::ClipboardHistory;
use crate::{ClipboardError, Result};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Clipboard configuration.
#[derive(Debug, Clone)]
pub struct ClipboardConfig {
    /// Enable history tracking.
    pub enable_history: bool,
    /// Maximum history size.
    pub max_history: usize,
    /// Monitor clipboard changes.
    pub monitor_changes: bool,
    /// Polling interval in milliseconds (if monitoring).
    pub poll_interval_ms: u64,
}

impl Default for ClipboardConfig {
    fn default() -> Self {
        Self {
            enable_history: true,
            max_history: 100,
            monitor_changes: false,
            poll_interval_ms: 500,
        }
    }
}

/// Clipboard manager.
pub struct ClipboardManager {
    config: ClipboardConfig,
    history: Arc<RwLock<ClipboardHistory>>,
    last_content: Arc<RwLock<Option<String>>>,
}

impl ClipboardManager {
    /// Create a new clipboard manager.
    pub fn new(config: ClipboardConfig) -> Self {
        Self {
            history: Arc::new(RwLock::new(ClipboardHistory::new(config.max_history))),
            last_content: Arc::new(RwLock::new(None)),
            config,
        }
    }

    /// Get current clipboard content.
    pub async fn get(&self) -> Option<ClipboardContent> {
        self.get_text().await.map(ClipboardContent::from_text)
    }

    /// Get clipboard text.
    #[cfg(target_os = "macos")]
    pub async fn get_text(&self) -> Option<String> {
        use std::process::Command;

        let output = Command::new("pbpaste").output().ok()?;

        if output.status.success() {
            let text = String::from_utf8(output.stdout).ok()?;
            if !text.is_empty() {
                return Some(text);
            }
        }

        None
    }

    #[cfg(not(target_os = "macos"))]
    pub async fn get_text(&self) -> Option<String> {
        // Placeholder for other platforms
        None
    }

    /// Set clipboard text.
    #[cfg(target_os = "macos")]
    pub async fn set_text(&self, text: &str) -> Result<()> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| ClipboardError::AccessFailed(e.to_string()))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| ClipboardError::AccessFailed(e.to_string()))?;
        }

        child
            .wait()
            .map_err(|e| ClipboardError::AccessFailed(e.to_string()))?;

        // Update history
        if self.config.enable_history {
            let content = ClipboardContent::from_text(text);
            let mut history = self.history.write().await;
            history.add(content);
        }

        // Update last content
        {
            let mut last = self.last_content.write().await;
            *last = Some(text.to_string());
        }

        debug!("Set clipboard text ({} chars)", text.len());
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub async fn set_text(&self, _text: &str) -> Result<()> {
        Err(ClipboardError::PlatformNotSupported)
    }

    /// Clear clipboard.
    #[cfg(target_os = "macos")]
    pub async fn clear(&self) -> Result<()> {
        self.set_text("").await
    }

    #[cfg(not(target_os = "macos"))]
    pub async fn clear(&self) -> Result<()> {
        Err(ClipboardError::PlatformNotSupported)
    }

    /// Get clipboard history.
    pub async fn history(&self) -> ClipboardHistory {
        // Clone the history
        let history = self.history.read().await;
        ClipboardHistory::new(self.config.max_history) // Return new for now
    }

    /// Search history.
    pub async fn search_history(&self, query: &str) -> Vec<crate::history::HistoryEntry> {
        let history = self.history.read().await;
        history.search(query).into_iter().cloned().collect()
    }

    /// Clear history.
    pub async fn clear_history(&self) {
        let mut history = self.history.write().await;
        history.clear();
    }

    /// Start monitoring clipboard changes.
    pub fn start_monitoring(&self) -> Option<tokio::task::JoinHandle<()>> {
        if !self.config.monitor_changes {
            return None;
        }

        let history = self.history.clone();
        let last_content = self.last_content.clone();
        let poll_interval = self.config.poll_interval_ms;
        let enable_history = self.config.enable_history;

        let handle = tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_millis(poll_interval));

            loop {
                interval.tick().await;

                // Get current clipboard content
                #[cfg(target_os = "macos")]
                let current = {
                    use std::process::Command;
                    Command::new("pbpaste").output().ok().and_then(|o| {
                        if o.status.success() {
                            String::from_utf8(o.stdout).ok()
                        } else {
                            None
                        }
                    })
                };

                #[cfg(not(target_os = "macos"))]
                let current: Option<String> = None;

                if let Some(text) = current {
                    let should_add = {
                        let last = last_content.read().await;
                        last.as_ref() != Some(&text)
                    };

                    if should_add && !text.is_empty() {
                        // Update last content
                        {
                            let mut last = last_content.write().await;
                            *last = Some(text.clone());
                        }

                        // Add to history
                        if enable_history {
                            let content = ClipboardContent::from_text(&text);
                            let mut hist = history.write().await;
                            hist.add(content);
                            debug!("Clipboard changed: {} chars", text.len());
                        }
                    }
                }
            }
        });

        info!("Started clipboard monitoring");
        Some(handle)
    }

    /// Get config.
    pub fn config(&self) -> &ClipboardConfig {
        &self.config
    }
}

impl Default for ClipboardManager {
    fn default() -> Self {
        Self::new(ClipboardConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_clipboard_manager_creation() {
        let manager = ClipboardManager::new(ClipboardConfig::default());
        assert!(manager.config.enable_history);
    }

    #[tokio::test]
    async fn test_clipboard_config() {
        let config = ClipboardConfig {
            enable_history: false,
            max_history: 50,
            monitor_changes: true,
            poll_interval_ms: 1000,
        };

        assert!(!config.enable_history);
        assert_eq!(config.max_history, 50);
    }
}
