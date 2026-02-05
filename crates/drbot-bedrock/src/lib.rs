//! AWS Bedrock provider for drbot.
//!
//! This crate provides integration with AWS Bedrock for Claude and other models.
//!
//! # Authentication
//!
//! Uses AWS credentials from environment variables:
//! - `AWS_ACCESS_KEY_ID`
//! - `AWS_SECRET_ACCESS_KEY`
//! - `AWS_SESSION_TOKEN` (optional, for temporary credentials)
//! - `AWS_REGION` (optional, defaults to us-east-1)

mod api;
mod signing;

pub use api::*;
pub use signing::{AwsCredentials, AwsSigner};

use async_trait::async_trait;
use drbot_core::message::{Message, Role};
use drbot_providers::{
    ChatOptions, ChatResponse as ProviderChatResponse, ModelInfo as ProviderModelInfo, Provider,
    StreamEvent, Usage,
};
use futures::stream::Stream;
use reqwest::Client;
use std::collections::BTreeMap;
use std::pin::Pin;
use tracing::{debug, error};

/// Default AWS region.
pub const DEFAULT_REGION: &str = "us-east-1";

/// Default model.
pub const DEFAULT_MODEL: &str = "anthropic.claude-3-5-sonnet-20241022-v2:0";

/// Bedrock provider implementation.
pub struct BedrockProvider {
    /// HTTP client.
    client: Client,
    /// AWS region.
    region: String,
    /// AWS credentials.
    credentials: AwsCredentials,
    /// Default model.
    default_model: String,
}

impl BedrockProvider {
    /// Create a new Bedrock provider with credentials from environment.
    pub fn from_env() -> drbot_core::Result<Self> {
        let credentials = AwsCredentials::from_env().ok_or_else(|| {
            drbot_core::Error::Auth(
                "AWS credentials not found. Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY"
                    .to_string(),
            )
        })?;

        let region = std::env::var("AWS_REGION").unwrap_or_else(|_| DEFAULT_REGION.to_string());

        Ok(Self {
            client: Client::new(),
            region,
            credentials,
            default_model: DEFAULT_MODEL.to_string(),
        })
    }

    /// Create with explicit credentials.
    pub fn new(credentials: AwsCredentials, region: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            region: region.into(),
            credentials,
            default_model: DEFAULT_MODEL.to_string(),
        }
    }

    /// Set the default model.
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    /// Get the Bedrock runtime endpoint URL.
    fn endpoint_url(&self) -> String {
        format!("https://bedrock-runtime.{}.amazonaws.com", self.region)
    }

    /// Convert messages to Bedrock format.
    fn convert_messages(messages: &[Message]) -> (Option<Vec<SystemContent>>, Vec<BedrockMessage>) {
        let mut system: Option<Vec<SystemContent>> = None;
        let mut bedrock_messages = Vec::new();

        for msg in messages {
            let content = msg.text_content();
            match msg.role {
                Role::System => {
                    let system_content = SystemContent { text: content };
                    match &mut system {
                        Some(s) => s.push(system_content),
                        None => system = Some(vec![system_content]),
                    }
                }
                Role::User => {
                    bedrock_messages.push(BedrockMessage::user(content));
                }
                Role::Assistant => {
                    bedrock_messages.push(BedrockMessage::assistant(content));
                }
            }
        }

        (system, bedrock_messages)
    }

    /// Build a converse request.
    fn build_request(&self, messages: &[Message], options: &ChatOptions) -> ConverseRequest {
        let (mut system, bedrock_messages) = Self::convert_messages(messages);

        // Handle system prompt from options
        if let Some(sys_prompt) = &options.system_prompt {
            let system_content = SystemContent {
                text: sys_prompt.clone(),
            };
            match &mut system {
                Some(s) => s.insert(0, system_content),
                None => system = Some(vec![system_content]),
            }
        }

        let inference_config = if options.temperature.is_some()
            || options.top_p.is_some()
            || options.max_tokens.is_some()
            || options.stop_sequences.is_some()
        {
            Some(InferenceConfig {
                temperature: options.temperature,
                top_p: options.top_p,
                max_tokens: options.max_tokens.map(|t| t as u32),
                stop_sequences: options.stop_sequences.clone(),
            })
        } else {
            None
        };

        ConverseRequest {
            model_id: options
                .model
                .clone()
                .unwrap_or_else(|| self.default_model.clone()),
            messages: bedrock_messages,
            system,
            inference_config,
        }
    }
}

impl Default for BedrockProvider {
    fn default() -> Self {
        Self::from_env().expect("AWS credentials required")
    }
}

#[async_trait]
impl Provider for BedrockProvider {
    fn name(&self) -> &str {
        "bedrock"
    }

    fn models(&self) -> Vec<ProviderModelInfo> {
        vec![
            ProviderModelInfo {
                id: "anthropic.claude-3-5-sonnet-20241022-v2:0".to_string(),
                name: "Claude 3.5 Sonnet v2".to_string(),
                provider: "bedrock".to_string(),
                context_window: 200000,
                max_output_tokens: Some(8192),
            },
            ProviderModelInfo {
                id: "anthropic.claude-3-5-haiku-20241022-v1:0".to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                provider: "bedrock".to_string(),
                context_window: 200000,
                max_output_tokens: Some(8192),
            },
            ProviderModelInfo {
                id: "anthropic.claude-3-opus-20240229-v1:0".to_string(),
                name: "Claude 3 Opus".to_string(),
                provider: "bedrock".to_string(),
                context_window: 200000,
                max_output_tokens: Some(4096),
            },
            ProviderModelInfo {
                id: "anthropic.claude-3-sonnet-20240229-v1:0".to_string(),
                name: "Claude 3 Sonnet".to_string(),
                provider: "bedrock".to_string(),
                context_window: 200000,
                max_output_tokens: Some(4096),
            },
            ProviderModelInfo {
                id: "anthropic.claude-3-haiku-20240307-v1:0".to_string(),
                name: "Claude 3 Haiku".to_string(),
                provider: "bedrock".to_string(),
                context_window: 200000,
                max_output_tokens: Some(4096),
            },
            ProviderModelInfo {
                id: "amazon.titan-text-premier-v1:0".to_string(),
                name: "Amazon Titan Text Premier".to_string(),
                provider: "bedrock".to_string(),
                context_window: 32000,
                max_output_tokens: Some(8192),
            },
            ProviderModelInfo {
                id: "meta.llama3-1-70b-instruct-v1:0".to_string(),
                name: "Llama 3.1 70B".to_string(),
                provider: "bedrock".to_string(),
                context_window: 128000,
                max_output_tokens: Some(2048),
            },
        ]
    }

    async fn chat(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<ProviderChatResponse> {
        let request = self.build_request(messages, &options);
        let model_id = request.model_id.clone();

        let url = format!("{}/model/{}/converse", self.endpoint_url(), model_id);

        let body = serde_json::to_vec(&request)?;

        debug!(model = %model_id, "Sending chat request to Bedrock");

        // Sign the request
        let signer = AwsSigner::new("bedrock", &self.region, self.credentials.clone());
        let headers: BTreeMap<String, String> = BTreeMap::new();
        let auth_headers = signer.sign("POST", &url, &headers, &body);

        // Build request
        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        for (k, v) in auth_headers {
            req = req.header(&k, &v);
        }

        let response = req
            .body(body)
            .send()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "Bedrock API error");
            if let Ok(err) = serde_json::from_str::<AwsError>(&body) {
                return Err(drbot_core::Error::Provider(
                    err.message
                        .unwrap_or_else(|| format!("Bedrock error: {}", status)),
                ));
            }
            return Err(drbot_core::Error::Provider(format!(
                "Bedrock API error ({}): {}",
                status, body
            )));
        }

        let converse_response: ConverseResponse = response
            .json()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        // Extract text content
        let content = converse_response
            .output
            .message
            .as_ref()
            .map(|m| {
                m.content
                    .iter()
                    .filter_map(|c| c.as_text())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        Ok(ProviderChatResponse {
            content,
            model: model_id,
            stop_reason: converse_response.stop_reason,
            usage: converse_response.usage.map(|u| Usage {
                input_tokens: u.input_tokens as usize,
                output_tokens: u.output_tokens as usize,
            }),
            tool_uses: Vec::new(),
        })
    }

    async fn stream(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>> {
        let request = self.build_request(messages, &options);
        let model_id = request.model_id.clone();

        let url = format!("{}/model/{}/converse-stream", self.endpoint_url(), model_id);

        let body = serde_json::to_vec(&request)?;

        debug!(model = %model_id, "Starting streaming request to Bedrock");

        // Sign the request
        let signer = AwsSigner::new("bedrock", &self.region, self.credentials.clone());
        let headers: BTreeMap<String, String> = BTreeMap::new();
        let auth_headers = signer.sign("POST", &url, &headers, &body);

        // Build request
        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        for (k, v) in auth_headers {
            req = req.header(&k, &v);
        }

        let response = req
            .body(body)
            .send()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if let Ok(err) = serde_json::from_str::<AwsError>(&body) {
                return Err(drbot_core::Error::Provider(
                    err.message
                        .unwrap_or_else(|| format!("Bedrock error: {}", status)),
                ));
            }
            return Err(drbot_core::Error::Provider(format!(
                "Bedrock API error ({}): {}",
                status, body
            )));
        }

        // AWS Bedrock uses event-stream format
        let stream = async_stream::stream! {
            use futures::StreamExt;

            let mut byte_stream = response.bytes_stream();
            let mut buffer = Vec::new();
            let mut final_usage: Option<BedrockUsage> = None;

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        error!(error = %e, "Stream error");
                        yield StreamEvent::Error { message: e.to_string() };
                        break;
                    }
                };

                buffer.extend_from_slice(&chunk);

                // Parse AWS event stream format
                // Events are delimited by newlines and contain JSON
                while let Some(event_end) = find_event_boundary(&buffer) {
                    let event_data = buffer[..event_end].to_vec();
                    buffer = buffer[event_end..].to_vec();

                    if let Some(json_str) = extract_json_from_event(&event_data) {
                        if let Ok(event) = serde_json::from_str::<ConverseStreamEvent>(&json_str) {
                            match event {
                                ConverseStreamEvent::ContentBlockDelta { delta, .. } => {
                                    if let Some(text) = delta.text {
                                        yield StreamEvent::Delta { content: text };
                                    }
                                }
                                ConverseStreamEvent::MessageStop { stop_reason } => {
                                    yield StreamEvent::Stop {
                                        reason: stop_reason,
                                        usage: final_usage.take().map(|u| Usage {
                                            input_tokens: u.input_tokens as usize,
                                            output_tokens: u.output_tokens as usize,
                                        }),
                                    };
                                }
                                ConverseStreamEvent::Metadata { usage } => {
                                    final_usage = usage;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

/// Find the boundary of an event in the buffer.
fn find_event_boundary(buffer: &[u8]) -> Option<usize> {
    // AWS event stream uses a binary format, but the payload is JSON
    // For simplicity, we look for complete JSON objects
    let s = String::from_utf8_lossy(buffer);
    if let Some(pos) = s.find("}\n") {
        return Some(pos + 2);
    }
    if let Some(pos) = s.find("}\r\n") {
        return Some(pos + 3);
    }
    None
}

/// Extract JSON from event data.
fn extract_json_from_event(data: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(data);
    // Find JSON object
    if let Some(start) = s.find('{') {
        if let Some(end) = s.rfind('}') {
            return Some(s[start..=end].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credentials_new() {
        let creds = AwsCredentials::new("test_key", "test_secret");
        assert_eq!(creds.access_key_id, "test_key");
        assert_eq!(creds.secret_access_key, "test_secret");
        assert!(creds.session_token.is_none());
    }

    #[test]
    fn test_credentials_with_token() {
        let creds = AwsCredentials::new("key", "secret").with_session_token("token");
        assert_eq!(creds.session_token, Some("token".to_string()));
    }

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there!"),
        ];

        let (system, bedrock_msgs) = BedrockProvider::convert_messages(&messages);

        assert!(system.is_some());
        assert_eq!(system.unwrap().len(), 1);
        assert_eq!(bedrock_msgs.len(), 2);
        assert_eq!(bedrock_msgs[0].role, "user");
        assert_eq!(bedrock_msgs[1].role, "assistant");
    }

    #[test]
    fn test_models() {
        // Skip if no credentials
        if std::env::var("AWS_ACCESS_KEY_ID").is_err() {
            return;
        }

        let provider = BedrockProvider::from_env().unwrap();
        let models = provider.models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id.contains("claude")));
    }
}
