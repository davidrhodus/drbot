//! Main screen context interface.

use crate::{
    AccessibilityNode, AccessibilityTree, CaptureOptions, Config, FocusedApp, FocusedElement,
    Result, ScreenError, Screenshot,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Screen context configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScreenContextConfig {
    /// Configuration options.
    pub config: Config,
}

/// Main screen context provider.
pub struct ScreenContext {
    config: Config,
    /// Cached focused app.
    cached_app: Arc<RwLock<Option<FocusedApp>>>,
    /// Cached focused element.
    cached_element: Arc<RwLock<Option<FocusedElement>>>,
}

impl ScreenContext {
    /// Create a new screen context.
    pub async fn new() -> Result<Self> {
        Self::with_config(Config::default()).await
    }

    /// Create with custom configuration.
    pub async fn with_config(config: Config) -> Result<Self> {
        // Check for accessibility permissions on macOS
        #[cfg(target_os = "macos")]
        {
            if config.enable_accessibility && !crate::macos::check_accessibility_permission() {
                return Err(ScreenError::PermissionDenied);
            }
        }

        info!("Screen context initialized");

        Ok(Self {
            config,
            cached_app: Arc::new(RwLock::new(None)),
            cached_element: Arc::new(RwLock::new(None)),
        })
    }

    /// Get the currently focused application.
    pub async fn get_focused_app(&self) -> Option<FocusedApp> {
        if !self.config.enable_accessibility {
            return None;
        }

        #[cfg(target_os = "macos")]
        {
            crate::macos::get_focused_app().await
        }

        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }

    /// Get the currently focused UI element.
    pub async fn get_focused_element(&self) -> Option<FocusedElement> {
        if !self.config.enable_accessibility {
            return None;
        }

        #[cfg(target_os = "macos")]
        {
            crate::macos::get_focused_element().await
        }

        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }

    /// Get visible text from the current screen context.
    pub async fn get_visible_text(&self) -> Result<String> {
        if !self.config.enable_accessibility {
            return Err(ScreenError::PermissionDenied);
        }

        #[cfg(target_os = "macos")]
        {
            crate::macos::get_visible_text(self.config.max_text_length).await
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(ScreenError::PlatformNotSupported)
        }
    }

    /// Get the accessibility tree for the focused window.
    pub async fn get_accessibility_tree(&self) -> Result<AccessibilityTree> {
        if !self.config.include_hierarchy {
            return Err(ScreenError::Internal("Hierarchy not enabled".to_string()));
        }

        #[cfg(target_os = "macos")]
        {
            crate::macos::get_accessibility_tree().await
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(ScreenError::PlatformNotSupported)
        }
    }

    /// Capture a screenshot.
    pub async fn capture_screen(&self) -> Result<Screenshot> {
        self.capture_screen_with_options(CaptureOptions::default())
            .await
    }

    /// Capture a screenshot with options.
    pub async fn capture_screen_with_options(&self, options: CaptureOptions) -> Result<Screenshot> {
        if !self.config.enable_screenshots {
            return Err(ScreenError::PermissionDenied);
        }

        crate::capture::capture_screen(options).await
    }

    /// Capture a specific window.
    pub async fn capture_window(&self, window_id: u32) -> Result<Screenshot> {
        crate::capture::capture_window(window_id).await
    }

    /// Get a summary of the current screen context.
    pub async fn get_context_summary(&self) -> ScreenContextSummary {
        let app = self.get_focused_app().await;
        let element = self.get_focused_element().await;
        let text = self.get_visible_text().await.ok();

        ScreenContextSummary {
            app_name: app.as_ref().map(|a| a.name.clone()),
            app_bundle_id: app.as_ref().and_then(|a| a.bundle_id.clone()),
            window_title: app.as_ref().and_then(|a| a.window_title.clone()),
            focused_element_type: element.as_ref().map(|e| format!("{:?}", e.element_type)),
            focused_element_value: element.as_ref().and_then(|e| e.value.clone()),
            visible_text_preview: text.map(|t| {
                if t.len() > 500 {
                    format!("{}...", &t[..500])
                } else {
                    t
                }
            }),
        }
    }

    /// Start continuous context monitoring.
    pub async fn start_monitoring(&self) -> tokio::sync::mpsc::Receiver<ScreenContextSummary> {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let interval_ms = self.config.capture_interval_ms;
        let config = self.config.clone();

        tokio::spawn(async move {
            let ctx = match ScreenContext::with_config(config).await {
                Ok(c) => c,
                Err(_) => return,
            };

            loop {
                let summary = ctx.get_context_summary().await;
                if tx.send(summary).await.is_err() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
            }
        });

        rx
    }
}

/// Summary of current screen context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenContextSummary {
    /// Focused application name.
    pub app_name: Option<String>,
    /// Application bundle ID (macOS).
    pub app_bundle_id: Option<String>,
    /// Active window title.
    pub window_title: Option<String>,
    /// Type of focused UI element.
    pub focused_element_type: Option<String>,
    /// Value of focused element (if applicable).
    pub focused_element_value: Option<String>,
    /// Preview of visible text.
    pub visible_text_preview: Option<String>,
}

impl ScreenContextSummary {
    /// Format as a prompt-friendly string.
    pub fn to_prompt(&self) -> String {
        let mut parts = Vec::new();

        if let Some(app) = &self.app_name {
            let window = self.window_title.as_deref().unwrap_or("unknown");
            parts.push(format!("User is in {} (window: {})", app, window));
        }

        if let Some(element_type) = &self.focused_element_type {
            if let Some(value) = &self.focused_element_value {
                parts.push(format!("Focused on {} with value: {}", element_type, value));
            } else {
                parts.push(format!("Focused on {}", element_type));
            }
        }

        if let Some(text) = &self.visible_text_preview {
            parts.push(format!("Visible text: {}", text));
        }

        parts.join(". ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_to_prompt() {
        let summary = ScreenContextSummary {
            app_name: Some("VS Code".to_string()),
            app_bundle_id: Some("com.microsoft.VSCode".to_string()),
            window_title: Some("main.rs - drbot".to_string()),
            focused_element_type: Some("TextField".to_string()),
            focused_element_value: Some("let x = 5".to_string()),
            visible_text_preview: None,
        };

        let prompt = summary.to_prompt();
        assert!(prompt.contains("VS Code"));
        assert!(prompt.contains("TextField"));
    }
}
