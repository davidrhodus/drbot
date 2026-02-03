//! OpenAI API client.

use crate::api::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ErrorResponse,
    StreamOptions,
};
use async_trait::async_trait;
use drbot_core::message::{Message, Role};
use drbot_providers::{ChatOptions, ChatResponse, ModelInfo, Provider, StreamEvent, Usage};
use futures::stream::Stream;
use reqwest::Client;
use std::pin::Pin;
use tracing::{debug, error, trace};

/// Default OpenAI API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

/// Default model to use.
pub const DEFAULT_MODEL: &str = "gpt-4o";

/// OpenAI provider implementation.
pub struct OpenAIProvider {
    /// HTTP client.
    client: Client,
    /// API key.
    api_key: String,
    /// Base URL for the API.
    base_url: String,
    /// Default model.
    default_model: String,
    /// Default max tokens.
    default_max_tokens: usize,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
            default_max_tokens: 4096,
        }
    }

    /// Set the base URL.
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

    /// Convert our Message type to OpenAI's format.
    fn convert_messages(messages: &[Message]) -> Vec<ChatMessage> {
        messages
            .iter()
            .map(|msg| {
                let content = msg.text_content();
                match msg.role {
                    Role::System => ChatMessage::system(content),
                    Role::User => ChatMessage::user(content),
                    Role::Assistant => ChatMessage::assistant(content),
                }
            })
            .collect()
    }

    /// Build the request.
    fn build_request(
        &self,
        messages: &[Message],
        options: &ChatOptions,
        stream: bool,
    ) -> ChatCompletionRequest {
        let mut api_messages = Self::convert_messages(messages);

        // Handle system prompt from options
        if let Some(system) = &options.system_prompt {
            // Check if first message is already system
            if api_messages
                .first()
                .map(|m| matches!(m.role, crate::api::Role::System))
                .unwrap_or(false)
            {
                // Replace the system message
                api_messages[0] = ChatMessage::system(system);
            } else {
                // Insert at beginning
                api_messages.insert(0, ChatMessage::system(system));
            }
        }

        let max_tokens = options
            .max_tokens
            .or(Some(self.default_max_tokens))
            .map(|t| t as u32);

        ChatCompletionRequest {
            model: options
                .model
                .clone()
                .unwrap_or_else(|| self.default_model.clone()),
            messages: api_messages,
            max_tokens,
            temperature: options.temperature,
            top_p: options.top_p,
            stop: options.stop_sequences.clone(),
            stream: if stream { Some(true) } else { None },
            stream_options: if stream {
                Some(StreamOptions {
                    include_usage: true,
                })
            } else {
                None
            },
        }
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                provider: "openai".to_string(),
                context_window: 128000,
                max_output_tokens: Some(16384),
            },
            ModelInfo {
                id: "gpt-4o-mini".to_string(),
                name: "GPT-4o Mini".to_string(),
                provider: "openai".to_string(),
                context_window: 128000,
                max_output_tokens: Some(16384),
            },
            ModelInfo {
                id: "gpt-4-turbo".to_string(),
                name: "GPT-4 Turbo".to_string(),
                provider: "openai".to_string(),
                context_window: 128000,
                max_output_tokens: Some(4096),
            },
            ModelInfo {
                id: "gpt-3.5-turbo".to_string(),
                name: "GPT-3.5 Turbo".to_string(),
                provider: "openai".to_string(),
                context_window: 16385,
                max_output_tokens: Some(4096),
            },
        ]
    }

    async fn chat(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let request = self.build_request(messages, &options, false);

        debug!(model = %request.model, "Sending chat request to OpenAI");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        if !status.is_success() {
            let error: ErrorResponse = serde_json::from_str(&body).map_err(|_| {
                drbot_core::Error::Provider(format!("OpenAI API error ({}): {}", status, body))
            })?;
            return Err(drbot_core::Error::Provider(error.error.message));
        }

        let completion: ChatCompletionResponse = serde_json::from_str(&body)?;

        let content = completion
            .choices
            .first()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        let stop_reason = completion
            .choices
            .first()
            .and_then(|c| c.finish_reason.clone());

        Ok(ChatResponse {
            content,
            model: completion.model,
            stop_reason,
            usage: completion.usage.map(|u| Usage {
                input_tokens: u.prompt_tokens as usize,
                output_tokens: u.completion_tokens as usize,
            }),
        })
    }

    async fn stream(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>> {
        let url = format!("{}/chat/completions", self.base_url);
        let request = self.build_request(messages, &options, true);

        debug!(model = %request.model, "Starting streaming request to OpenAI");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .map_err(|e| drbot_core::Error::Http(e.to_string()))?;
            let error: ErrorResponse = serde_json::from_str(&body).map_err(|_| {
                drbot_core::Error::Provider(format!("OpenAI API error ({}): {}", status, body))
            })?;
            return Err(drbot_core::Error::Provider(error.error.message));
        }

        let stream = async_stream::stream! {
            use futures::StreamExt;

            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut final_usage: Option<crate::api::Usage> = None;

            // Track active tool calls by index: (id, name, arguments)
            let mut active_tool_calls: std::collections::HashMap<usize, (String, String, String)> =
                std::collections::HashMap::new();

            while let Some(chunk_result) = byte_stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        error!(error = %e, "Stream error");
                        yield StreamEvent::Error { message: e.to_string() };
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Process complete lines
                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    if !line.starts_with("data: ") {
                        continue;
                    }

                    let data = &line[6..];

                    if data == "[DONE]" {
                        trace!("Stream complete");

                        // Emit any pending tool calls
                        for (_index, (id, name, arguments)) in active_tool_calls.drain() {
                            let input = serde_json::from_str(&arguments).unwrap_or(serde_json::Value::Null);
                            yield StreamEvent::ToolUse { id, name, input };
                        }

                        // Yield final stop event
                        yield StreamEvent::Stop {
                            reason: "stop".to_string(),
                            usage: final_usage.take().map(|u| Usage {
                                input_tokens: u.prompt_tokens as usize,
                                output_tokens: u.completion_tokens as usize,
                            }),
                        };
                        break;
                    }

                    match serde_json::from_str::<ChatCompletionChunk>(data) {
                        Ok(chunk) => {
                            // Store usage if present
                            if let Some(usage) = chunk.usage {
                                final_usage = Some(usage);
                            }

                            // Process choices
                            for choice in &chunk.choices {
                                if let Some(delta) = &choice.delta {
                                    // Handle text content
                                    if let Some(content) = &delta.content {
                                        if !content.is_empty() {
                                            yield StreamEvent::Delta { content: content.clone() };
                                        }
                                    }

                                    // Handle tool calls
                                    if let Some(tool_calls) = &delta.tool_calls {
                                        for tc in tool_calls {
                                            let entry = active_tool_calls
                                                .entry(tc.index)
                                                .or_insert_with(|| (String::new(), String::new(), String::new()));

                                            // Capture ID and name from first delta
                                            if let Some(id) = &tc.id {
                                                entry.0 = id.clone();
                                            }
                                            if let Some(func) = &tc.function {
                                                if let Some(name) = &func.name {
                                                    entry.1 = name.clone();
                                                }
                                                // Accumulate arguments
                                                if let Some(args) = &func.arguments {
                                                    entry.2.push_str(args);
                                                }
                                            }
                                        }
                                    }
                                }

                                // Check for finish reason - emit tool calls when tool_calls finish
                                if let Some(reason) = &choice.finish_reason {
                                    trace!(reason = %reason, "Received finish reason");
                                    if reason == "tool_calls" {
                                        // Emit all accumulated tool calls
                                        for (_index, (id, name, arguments)) in active_tool_calls.drain() {
                                            let input = serde_json::from_str(&arguments).unwrap_or(serde_json::Value::Null);
                                            yield StreamEvent::ToolUse { id, name, input };
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            trace!(error = %e, data = %data, "Failed to parse chunk");
                        }
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = OpenAIProvider::new("test-key");
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.default_model, DEFAULT_MODEL);
    }

    #[test]
    fn test_builder() {
        let provider = OpenAIProvider::new("test-key")
            .with_base_url("https://custom.api.com")
            .with_default_model("gpt-4")
            .with_default_max_tokens(2048);

        assert_eq!(provider.base_url, "https://custom.api.com");
        assert_eq!(provider.default_model, "gpt-4");
        assert_eq!(provider.default_max_tokens, 2048);
    }

    #[test]
    fn test_models() {
        let provider = OpenAIProvider::new("test-key");
        let models = provider.models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "gpt-4o"));
    }

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there!"),
        ];

        let converted = OpenAIProvider::convert_messages(&messages);
        assert_eq!(converted.len(), 3);
        assert!(matches!(converted[0].role, crate::api::Role::System));
        assert!(matches!(converted[1].role, crate::api::Role::User));
        assert!(matches!(converted[2].role, crate::api::Role::Assistant));
    }
}
