//! Workflow actions.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Action types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    /// Send a message.
    SendMessage,
    /// Make an HTTP request.
    HttpRequest,
    /// Execute code.
    ExecuteCode,
    /// Call AI model.
    AiCall,
    /// Wait/delay.
    Wait,
    /// Conditional branch.
    Condition,
    /// Loop.
    Loop,
    /// Set variable.
    SetVariable,
    /// Log.
    Log,
}

/// A workflow action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Action ID.
    pub id: Uuid,
    /// Action type.
    pub action_type: ActionType,
    /// Action name.
    pub name: String,
    /// Action configuration.
    pub config: ActionConfig,
    /// Whether to continue on error.
    pub continue_on_error: bool,
    /// Retry count.
    pub retries: u32,
}

impl Action {
    /// Create a new action.
    pub fn new(action_type: ActionType, name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            action_type,
            name: name.to_string(),
            config: ActionConfig::default(),
            continue_on_error: false,
            retries: 0,
        }
    }

    /// Create a send message action.
    pub fn send_message(channel: &str, message: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            action_type: ActionType::SendMessage,
            name: "Send Message".to_string(),
            config: ActionConfig {
                channel: Some(channel.to_string()),
                message: Some(message.to_string()),
                ..Default::default()
            },
            continue_on_error: false,
            retries: 0,
        }
    }

    /// Create an HTTP request action.
    pub fn http_request(method: &str, url: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            action_type: ActionType::HttpRequest,
            name: "HTTP Request".to_string(),
            config: ActionConfig {
                method: Some(method.to_string()),
                url: Some(url.to_string()),
                ..Default::default()
            },
            continue_on_error: false,
            retries: 1,
        }
    }

    /// Create an AI call action.
    pub fn ai_call(prompt: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            action_type: ActionType::AiCall,
            name: "AI Call".to_string(),
            config: ActionConfig {
                prompt: Some(prompt.to_string()),
                ..Default::default()
            },
            continue_on_error: false,
            retries: 1,
        }
    }

    /// Create a wait action.
    pub fn wait(duration_secs: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            action_type: ActionType::Wait,
            name: "Wait".to_string(),
            config: ActionConfig {
                duration_secs: Some(duration_secs),
                ..Default::default()
            },
            continue_on_error: true,
            retries: 0,
        }
    }

    /// Create a log action.
    pub fn log(message: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            action_type: ActionType::Log,
            name: "Log".to_string(),
            config: ActionConfig {
                message: Some(message.to_string()),
                ..Default::default()
            },
            continue_on_error: true,
            retries: 0,
        }
    }

    /// Set continue on error.
    pub fn with_continue_on_error(mut self, continue_on_error: bool) -> Self {
        self.continue_on_error = continue_on_error;
        self
    }

    /// Set retry count.
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }
}

/// Action configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActionConfig {
    /// Target channel.
    pub channel: Option<String>,
    /// Message content.
    pub message: Option<String>,
    /// HTTP method.
    pub method: Option<String>,
    /// URL.
    pub url: Option<String>,
    /// Request body.
    pub body: Option<String>,
    /// Headers.
    pub headers: Option<std::collections::HashMap<String, String>>,
    /// Code to execute.
    pub code: Option<String>,
    /// Language for code execution.
    pub language: Option<String>,
    /// AI prompt.
    pub prompt: Option<String>,
    /// AI model.
    pub model: Option<String>,
    /// Wait duration.
    pub duration_secs: Option<u64>,
    /// Condition expression.
    pub condition: Option<String>,
    /// Variable name.
    pub variable: Option<String>,
    /// Variable value.
    pub value: Option<serde_json::Value>,
}

/// Result of action execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// Action ID.
    pub action_id: Uuid,
    /// Whether action succeeded.
    pub success: bool,
    /// Output data.
    pub output: Option<serde_json::Value>,
    /// Error message.
    pub error: Option<String>,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
}

impl ActionResult {
    /// Create a success result.
    pub fn success(action_id: Uuid, output: serde_json::Value, duration_ms: u64) -> Self {
        Self {
            action_id,
            success: true,
            output: Some(output),
            error: None,
            duration_ms,
        }
    }

    /// Create a failure result.
    pub fn failure(action_id: Uuid, error: &str, duration_ms: u64) -> Self {
        Self {
            action_id,
            success: false,
            output: None,
            error: Some(error.to_string()),
            duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_creation() {
        let action = Action::send_message("general", "Hello!");
        assert_eq!(action.action_type, ActionType::SendMessage);
        assert_eq!(action.config.channel, Some("general".to_string()));
    }

    #[test]
    fn test_http_action() {
        let action = Action::http_request("GET", "https://api.example.com").with_retries(3);

        assert_eq!(action.action_type, ActionType::HttpRequest);
        assert_eq!(action.retries, 3);
    }

    #[test]
    fn test_action_result() {
        let id = Uuid::new_v4();
        let result = ActionResult::success(id, serde_json::json!({"ok": true}), 100);

        assert!(result.success);
        assert!(result.output.is_some());
    }
}
