//! Anthropic API client implementation.

use crate::api::{
    ApiContent, ApiMessage, ContentBlock, ContentDelta, MessagesRequest, MessagesResponse,
    StreamEvent as ApiStreamEvent,
};
use crate::{API_VERSION, DEFAULT_BASE_URL};
use async_trait::async_trait;
use drbot_core::message::{Content, Message, Role};
use drbot_core::{Error, Result};
use drbot_providers::{
    ChatOptions, ChatResponse, ModelInfo, Provider, StreamEvent, ToolUse, Usage,
};
use futures::StreamExt;
use reqwest::Client;
use std::collections::HashMap;
use std::pin::Pin;
use tokio_stream::Stream;
use tracing::{debug, error, warn};

/// Anthropic Claude provider.
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    base_url: String,
    default_model: String,
    default_max_tokens: usize,
    extra_headers: HashMap<String, String>,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: "claude-sonnet-4-20250514".to_string(),
            default_max_tokens: 8192,
            extra_headers: HashMap::new(),
        }
    }

    /// Set a custom base URL (for proxies).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the default model.
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    /// Set the default max tokens.
    pub fn with_default_max_tokens(mut self, max_tokens: usize) -> Self {
        self.default_max_tokens = max_tokens;
        self
    }

    /// Set additional headers for requests.
    pub fn with_extra_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.extra_headers = headers;
        self
    }

    /// Convert drbot messages to Anthropic API format.
    fn convert_messages(&self, messages: &[Message]) -> (Option<String>, Vec<ApiMessage>) {
        let mut system_prompt = None;
        let mut api_messages = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    // Anthropic uses a separate system parameter
                    system_prompt = Some(msg.text_content());
                }
                Role::User | Role::Assistant => {
                    let role = match msg.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        _ => continue,
                    };

                    let content = self.convert_content(&msg.content);
                    api_messages.push(ApiMessage {
                        role: role.to_string(),
                        content,
                    });
                }
            }
        }

        (system_prompt, api_messages)
    }

    /// Convert content blocks to API format.
    fn convert_content(&self, content: &[Content]) -> ApiContent {
        if content.len() == 1 {
            if let Content::Text { text } = &content[0] {
                return ApiContent::Text(text.clone());
            }
        }

        let blocks: Vec<ContentBlock> = content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(ContentBlock::Text { text: text.clone() }),
                Content::Image { source, .. } => {
                    match source {
                        drbot_core::message::ImageSource::Base64 { media_type, data } => {
                            Some(ContentBlock::Image {
                                source: crate::api::ImageSource {
                                    source_type: "base64".to_string(),
                                    media_type: media_type.clone(),
                                    data: data.clone(),
                                },
                            })
                        }
                        _ => None, // URL images need to be fetched first
                    }
                }
                Content::ToolUse { id, name, input } => Some(ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                }),
                Content::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => Some(ContentBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: content.clone(),
                    is_error: if *is_error { Some(true) } else { None },
                }),
                _ => None,
            })
            .collect();

        ApiContent::Blocks(blocks)
    }

    /// Extract text from response content blocks.
    fn extract_text(content: &[ContentBlock]) -> String {
        content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    fn extract_tool_uses(content: &[ContentBlock]) -> Vec<ToolUse> {
        content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolUse { id, name, input } => Some(ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                }),
                _ => None,
            })
            .collect()
    }

    /// Make a request to the Messages API.
    async fn make_request(&self, request: &MessagesRequest) -> Result<MessagesResponse> {
        let url = format!("{}/v1/messages", self.base_url);

        debug!(model = %request.model, "Sending request to Anthropic");

        let mut builder = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json");
        for (key, value) in &self.extra_headers {
            builder = builder.header(key.as_str(), value.as_str());
        }
        let response = builder
            .json(request)
            .send()
            .await
            .map_err(|e| Error::Provider(format!("HTTP error: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(status = %status, error = %error_text, "Anthropic API error");
            return Err(Error::Provider(format!(
                "API error ({}): {}",
                status, error_text
            )));
        }

        let result: MessagesResponse = response
            .json()
            .await
            .map_err(|e| Error::Provider(format!("Failed to parse response: {}", e)))?;

        Ok(result)
    }

    /// Make a streaming request to the Messages API.
    async fn make_stream_request(
        &self,
        request: &MessagesRequest,
    ) -> Result<impl Stream<Item = Result<ApiStreamEvent>>> {
        let url = format!("{}/v1/messages", self.base_url);

        debug!(model = %request.model, "Sending streaming request to Anthropic");

        let mut builder = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json");
        for (key, value) in &self.extra_headers {
            builder = builder.header(key.as_str(), value.as_str());
        }
        let response = builder
            .json(request)
            .send()
            .await
            .map_err(|e| Error::Provider(format!("HTTP error: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::Provider(format!(
                "API error ({}): {}",
                status, error_text
            )));
        }

        // Parse SSE stream
        let stream = response.bytes_stream();
        let event_stream = async_stream::stream! {
            let mut buffer = String::new();
            let mut stream = std::pin::pin!(stream);

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&text);

                        // Process complete events
                        while let Some(pos) = buffer.find("\n\n") {
                            let event_text = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();

                            if let Some(event) = parse_sse_event(&event_text) {
                                yield event;
                            }
                        }
                    }
                    Err(e) => {
                        yield Err(Error::Provider(format!("Stream error: {}", e)));
                        break;
                    }
                }
            }
        };

        Ok(event_stream)
    }
}

/// Parse a Server-Sent Event.
fn parse_sse_event(text: &str) -> Option<Result<ApiStreamEvent>> {
    let mut event_type = None;
    let mut data = None;

    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event: ") {
            event_type = Some(value.trim());
        } else if let Some(value) = line.strip_prefix("data: ") {
            data = Some(value.trim());
        }
    }

    let data = data?;

    // Skip ping events with no data
    if data.is_empty() || event_type == Some("ping") {
        return Some(Ok(ApiStreamEvent::Ping));
    }

    match serde_json::from_str::<ApiStreamEvent>(data) {
        Ok(event) => Some(Ok(event)),
        Err(e) => {
            warn!(error = %e, data = %data, "Failed to parse SSE event");
            None
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    async fn chat(&self, messages: &[Message], options: ChatOptions) -> Result<ChatResponse> {
        let (system, api_messages) = self.convert_messages(messages);

        let tools = options.tools.clone().map(|defs| {
            defs.into_iter()
                .map(|d| crate::api::Tool {
                    name: d.name,
                    description: d.description,
                    input_schema: d.parameters,
                })
                .collect::<Vec<_>>()
        });

        let request = MessagesRequest {
            model: options.model.unwrap_or_else(|| self.default_model.clone()),
            messages: api_messages,
            max_tokens: options.max_tokens.unwrap_or(self.default_max_tokens),
            system: options.system_prompt.or(system),
            temperature: options.temperature,
            top_p: options.top_p,
            stop_sequences: options.stop_sequences,
            stream: None,
            tools,
        };

        let response = self.make_request(&request).await?;

        Ok(ChatResponse {
            content: Self::extract_text(&response.content),
            model: response.model,
            usage: Some(Usage {
                input_tokens: response.usage.input_tokens,
                output_tokens: response.usage.output_tokens,
            }),
            stop_reason: response.stop_reason,
            tool_uses: Self::extract_tool_uses(&response.content),
        })
    }

    async fn stream(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>> {
        let (system, api_messages) = self.convert_messages(messages);

        let tools = options.tools.clone().map(|defs| {
            defs.into_iter()
                .map(|d| crate::api::Tool {
                    name: d.name,
                    description: d.description,
                    input_schema: d.parameters,
                })
                .collect::<Vec<_>>()
        });

        let request = MessagesRequest {
            model: options
                .model
                .clone()
                .unwrap_or_else(|| self.default_model.clone()),
            messages: api_messages,
            max_tokens: options.max_tokens.unwrap_or(self.default_max_tokens),
            system: options.system_prompt.or(system),
            temperature: options.temperature,
            top_p: options.top_p,
            stop_sequences: options.stop_sequences,
            stream: Some(true),
            tools,
        };

        let model = request.model.clone();
        let api_stream = self.make_stream_request(&request).await?;

        let event_stream = async_stream::stream! {
            let mut total_output_tokens = 0usize;
            let mut input_tokens = 0usize;
            let mut stop_reason = None;
            let mut started = false;
            let mut stream = std::pin::pin!(api_stream);

            // Track active tool uses by index
            let mut active_tool_uses: std::collections::HashMap<usize, (String, String, String)> =
                std::collections::HashMap::new();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(api_event) => {
                        match api_event {
                            ApiStreamEvent::MessageStart { message } => {
                                input_tokens = message.usage.input_tokens;
                                if !started {
                                    started = true;
                                    yield StreamEvent::Start { model: model.clone() };
                                }
                            }
                            ApiStreamEvent::ContentBlockStart { index, content_block } => {
                                // Track tool use starts
                                if let ContentBlock::ToolUse { id, name, .. } = content_block {
                                    active_tool_uses.insert(index, (id, name, String::new()));
                                }
                            }
                            ApiStreamEvent::ContentBlockDelta { index, delta } => {
                                match delta {
                                    ContentDelta::TextDelta { text } => {
                                        yield StreamEvent::Delta { content: text };
                                    }
                                    ContentDelta::InputJsonDelta { partial_json } => {
                                        // Accumulate tool use JSON
                                        if let Some((_, _, json)) = active_tool_uses.get_mut(&index) {
                                            json.push_str(&partial_json);
                                        }
                                    }
                                }
                            }
                            ApiStreamEvent::ContentBlockStop { index } => {
                                // Emit completed tool use
                                if let Some((id, name, json)) = active_tool_uses.remove(&index) {
                                    let input = serde_json::from_str(&json).unwrap_or(serde_json::Value::Null);
                                    yield StreamEvent::ToolUse { id, name, input };
                                }
                            }
                            ApiStreamEvent::MessageDelta { delta, usage } => {
                                stop_reason = delta.stop_reason;
                                if let Some(u) = usage {
                                    total_output_tokens = u.output_tokens;
                                }
                            }
                            ApiStreamEvent::MessageStop => {
                                yield StreamEvent::Stop {
                                    reason: stop_reason.clone().unwrap_or_else(|| "end_turn".to_string()),
                                    usage: Some(Usage {
                                        input_tokens,
                                        output_tokens: total_output_tokens,
                                    }),
                                };
                            }
                            ApiStreamEvent::Error { error } => {
                                yield StreamEvent::Error {
                                    message: format!("{}: {}", error.error_type, error.message),
                                };
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        yield StreamEvent::Error { message: e.to_string() };
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(event_stream))
    }

    fn models(&self) -> Vec<ModelInfo> {
        let provider = "anthropic".to_string();
        let mut models = vec![
            ModelInfo {
                id: "claude-opus-4-6".to_string(),
                name: "Claude Opus 4.6".to_string(),
                provider: provider.clone(),
                context_window: 200000,
                max_output_tokens: Some(64000),
            },
            ModelInfo {
                id: "claude-opus-4-5".to_string(),
                name: "Claude Opus 4.5".to_string(),
                provider: provider.clone(),
                context_window: 200000,
                max_output_tokens: Some(64000),
            },
            ModelInfo {
                id: "claude-sonnet-4-5".to_string(),
                name: "Claude Sonnet 4.5".to_string(),
                provider: provider.clone(),
                context_window: 200000,
                max_output_tokens: Some(64000),
            },
            ModelInfo {
                id: "claude-sonnet-4-1".to_string(),
                name: "Claude Sonnet 4.1".to_string(),
                provider: provider.clone(),
                context_window: 200000,
                max_output_tokens: Some(64000),
            },
            ModelInfo {
                id: "claude-haiku-4-5".to_string(),
                name: "Claude Haiku 4.5".to_string(),
                provider: provider.clone(),
                context_window: 200000,
                max_output_tokens: Some(64000),
            },
            ModelInfo {
                id: "claude-sonnet-4-20250514".to_string(),
                name: "Claude Sonnet 4".to_string(),
                provider: provider.clone(),
                context_window: 200000,
                max_output_tokens: Some(64000),
            },
            ModelInfo {
                id: "claude-opus-4-20250514".to_string(),
                name: "Claude Opus 4".to_string(),
                provider: provider.clone(),
                context_window: 200000,
                max_output_tokens: Some(32000),
            },
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                provider: provider.clone(),
                context_window: 200000,
                max_output_tokens: Some(8192),
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                provider: provider.clone(),
                context_window: 200000,
                max_output_tokens: Some(8192),
            },
            ModelInfo {
                id: "claude-3-opus-20240229".to_string(),
                name: "Claude 3 Opus".to_string(),
                provider: provider.clone(),
                context_window: 200000,
                max_output_tokens: Some(4096),
            },
        ];

        // Ensure the configured default model appears in `models.list` even if it's not
        // in the small built-in catalog (OpenClaw Control UI convenience).
        if !models.iter().any(|m| m.id == self.default_model) {
            models.push(ModelInfo {
                id: self.default_model.clone(),
                name: self.default_model.clone(),
                provider,
                context_window: 200000,
                max_output_tokens: None,
            });
        }

        models
    }

    fn name(&self) -> &str {
        "anthropic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = AnthropicProvider::new("test-key");
        assert_eq!(provider.name(), "anthropic");
        let models = provider.models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "claude-opus-4-6"));
    }

    #[test]
    fn test_message_conversion() {
        let provider = AnthropicProvider::new("test-key");
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there!"),
        ];

        let (system, api_messages) = provider.convert_messages(&messages);

        assert_eq!(system, Some("You are helpful".to_string()));
        assert_eq!(api_messages.len(), 2);
        assert_eq!(api_messages[0].role, "user");
        assert_eq!(api_messages[1].role, "assistant");
    }
}
