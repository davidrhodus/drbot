//! Hook types and definitions.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Hook execution timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookTiming {
    /// Execute before the main operation.
    Pre,
    /// Execute after the main operation.
    Post,
}

/// Hook event type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HookEvent {
    /// Message received from user.
    MessageReceived,
    /// Message about to be sent to AI provider.
    MessageToProvider,
    /// Response received from AI provider.
    ProviderResponse,
    /// Message about to be sent to channel.
    MessageToChannel,
    /// Session created.
    SessionCreated,
    /// Session ended.
    SessionEnded,
    /// Custom event.
    Custom(String),
}

/// Hook context passed to handlers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookContext {
    /// Event that triggered the hook.
    pub event: HookEvent,
    /// Session ID.
    pub session_id: Option<String>,
    /// Channel ID.
    pub channel_id: Option<String>,
    /// User ID.
    pub user_id: Option<String>,
    /// Message content (if applicable).
    pub message: Option<String>,
    /// Additional metadata.
    pub metadata: serde_json::Value,
}

impl HookContext {
    /// Create a new hook context.
    pub fn new(event: HookEvent) -> Self {
        Self {
            event,
            session_id: None,
            channel_id: None,
            user_id: None,
            message: None,
            metadata: serde_json::Value::Object(Default::default()),
        }
    }

    /// Set session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set channel ID.
    pub fn with_channel(mut self, channel_id: impl Into<String>) -> Self {
        self.channel_id = Some(channel_id.into());
        self
    }

    /// Set user ID.
    pub fn with_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set message content.
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Set metadata.
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self
    }
}

/// Hook result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    /// Whether to continue processing.
    pub continue_processing: bool,
    /// Modified message (if any).
    pub modified_message: Option<String>,
    /// Modified metadata.
    pub modified_metadata: Option<serde_json::Value>,
    /// Error message (if hook failed).
    pub error: Option<String>,
}

impl HookResult {
    /// Create a success result that continues processing.
    pub fn ok() -> Self {
        Self {
            continue_processing: true,
            modified_message: None,
            modified_metadata: None,
            error: None,
        }
    }

    /// Create a result that modifies the message.
    pub fn with_message(message: impl Into<String>) -> Self {
        Self {
            continue_processing: true,
            modified_message: Some(message.into()),
            modified_metadata: None,
            error: None,
        }
    }

    /// Create a result that stops processing.
    pub fn stop() -> Self {
        Self {
            continue_processing: false,
            modified_message: None,
            modified_metadata: None,
            error: None,
        }
    }

    /// Create an error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            continue_processing: false,
            modified_message: None,
            modified_metadata: None,
            error: Some(message.into()),
        }
    }
}

/// Hook handler trait.
#[async_trait]
pub trait Hook: Send + Sync {
    /// Get hook name.
    fn name(&self) -> &str;

    /// Get events this hook handles.
    fn events(&self) -> Vec<HookEvent>;

    /// Get hook timing.
    fn timing(&self) -> HookTiming;

    /// Get hook priority (lower = runs first).
    fn priority(&self) -> i32 {
        0
    }

    /// Whether the hook is enabled.
    fn enabled(&self) -> bool {
        true
    }

    /// Execute the hook.
    async fn execute(&self, context: &HookContext) -> HookResult;
}

/// Boxed hook type.
pub type BoxedHook = Arc<dyn Hook>;

/// Hook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Hook name.
    pub name: String,
    /// Events to handle.
    pub events: Vec<HookEvent>,
    /// Hook timing.
    pub timing: HookTiming,
    /// Priority.
    pub priority: i32,
    /// Whether enabled.
    pub enabled: bool,
    /// Hook-specific configuration.
    pub config: serde_json::Value,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            events: vec![],
            timing: HookTiming::Pre,
            priority: 0,
            enabled: true,
            config: serde_json::Value::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_context_builder() {
        let ctx = HookContext::new(HookEvent::MessageReceived)
            .with_session("session-1")
            .with_channel("channel-1")
            .with_user("user-1")
            .with_message("hello");

        assert_eq!(ctx.session_id, Some("session-1".to_string()));
        assert_eq!(ctx.channel_id, Some("channel-1".to_string()));
        assert_eq!(ctx.user_id, Some("user-1".to_string()));
        assert_eq!(ctx.message, Some("hello".to_string()));
    }

    #[test]
    fn test_hook_result_ok() {
        let result = HookResult::ok();
        assert!(result.continue_processing);
        assert!(result.modified_message.is_none());
        assert!(result.error.is_none());
    }

    #[test]
    fn test_hook_result_with_message() {
        let result = HookResult::with_message("modified");
        assert!(result.continue_processing);
        assert_eq!(result.modified_message, Some("modified".to_string()));
    }

    #[test]
    fn test_hook_result_stop() {
        let result = HookResult::stop();
        assert!(!result.continue_processing);
    }

    #[test]
    fn test_hook_result_error() {
        let result = HookResult::error("something went wrong");
        assert!(!result.continue_processing);
        assert_eq!(result.error, Some("something went wrong".to_string()));
    }

    #[test]
    fn test_hook_config_default() {
        let config = HookConfig::default();
        assert!(config.enabled);
        assert_eq!(config.priority, 0);
    }
}
