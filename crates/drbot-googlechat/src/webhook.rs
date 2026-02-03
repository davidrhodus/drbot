//! Webhook handler for Google Chat.

use crate::{api::Message, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Webhook event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WebhookEvent {
    /// Bot added to space.
    AddedToSpace {
        space: SpaceRef,
        user: UserRef,
        event_time: DateTime<Utc>,
    },
    /// Bot removed from space.
    RemovedFromSpace {
        space: SpaceRef,
        user: UserRef,
        event_time: DateTime<Utc>,
    },
    /// Message received.
    Message {
        space: SpaceRef,
        message: Message,
        user: UserRef,
        event_time: DateTime<Utc>,
    },
    /// Card clicked.
    CardClicked {
        space: SpaceRef,
        message: Message,
        user: UserRef,
        action: CardAction,
        event_time: DateTime<Utc>,
    },
}

/// Space reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceRef {
    /// Space resource name.
    pub name: String,
    /// Display name.
    pub display_name: Option<String>,
    /// Space type.
    #[serde(rename = "type")]
    pub space_type: Option<String>,
}

/// User reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRef {
    /// User resource name.
    pub name: String,
    /// Display name.
    pub display_name: Option<String>,
    /// Email.
    pub email: Option<String>,
    /// Avatar URL.
    pub avatar_url: Option<String>,
    /// User type (HUMAN, BOT).
    #[serde(rename = "type")]
    pub user_type: Option<String>,
}

/// Card action from a click.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardAction {
    /// Action method name.
    pub action_method_name: String,
    /// Action parameters.
    #[serde(default)]
    pub parameters: Vec<ActionParameter>,
}

/// Action parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionParameter {
    /// Parameter key.
    pub key: String,
    /// Parameter value.
    pub value: String,
}

/// Webhook handler.
pub struct WebhookHandler {
    /// Verification token (optional).
    verification_token: Option<String>,
}

impl WebhookHandler {
    /// Create a new webhook handler.
    pub fn new() -> Self {
        Self {
            verification_token: None,
        }
    }

    /// Set verification token.
    pub fn with_verification_token(mut self, token: &str) -> Self {
        self.verification_token = Some(token.to_string());
        self
    }

    /// Parse a webhook event from JSON.
    pub fn parse_event(&self, json: &str) -> Result<WebhookEvent> {
        serde_json::from_str(json).map_err(|e| crate::GoogleChatError::ApiError(e.to_string()))
    }

    /// Verify a webhook request (if verification token is configured).
    pub fn verify(&self, token: Option<&str>) -> bool {
        match (&self.verification_token, token) {
            (Some(expected), Some(provided)) => expected == provided,
            (None, _) => true,
            (Some(_), None) => false,
        }
    }

    /// Create a text response.
    pub fn text_response(text: &str) -> serde_json::Value {
        serde_json::json!({
            "text": text
        })
    }

    /// Create a card response.
    pub fn card_response(header: &str, widgets: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({
            "cards": [{
                "header": {
                    "title": header
                },
                "sections": [{
                    "widgets": widgets
                }]
            }]
        })
    }
}

impl Default for WebhookHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_handler() {
        let handler = WebhookHandler::new().with_verification_token("secret");

        assert!(handler.verify(Some("secret")));
        assert!(!handler.verify(Some("wrong")));
        assert!(!handler.verify(None));
    }

    #[test]
    fn test_text_response() {
        let response = WebhookHandler::text_response("Hello!");
        assert_eq!(response["text"], "Hello!");
    }
}
