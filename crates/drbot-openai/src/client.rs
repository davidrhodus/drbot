//! OpenAI API client.

use crate::api::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ErrorResponse,
    StreamOptions, Tool as ApiTool, ToolFunction as ApiToolFunction,
};
use async_trait::async_trait;
use drbot_core::message::{Content, Message, Role};
use drbot_providers::{
    ChatOptions, ChatResponse, ModelInfo, Provider, StreamEvent, ToolUse, Usage,
};
use futures::stream::Stream;
use reqwest::Client;
use std::collections::HashMap;
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
    /// Provider name (for gateway/UI).
    provider_name: String,
    /// API key.
    api_key: String,
    /// Base URL for the API.
    base_url: String,
    /// Default model.
    default_model: String,
    /// Default max tokens.
    default_max_tokens: usize,
    /// Additional headers to send with each request (OpenRouter attribution, etc).
    extra_headers: HashMap<String, String>,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            provider_name: "openai".to_string(),
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
            default_max_tokens: 4096,
            extra_headers: HashMap::new(),
        }
    }

    /// Set the provider name (used for UI/usage attribution).
    pub fn with_provider_name(mut self, name: impl Into<String>) -> Self {
        self.provider_name = name.into();
        self
    }

    /// Set the base URL.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set additional headers for requests.
    pub fn with_extra_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.extra_headers = headers;
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
        let mut out: Vec<ChatMessage> = Vec::new();

        for msg in messages {
            // Tool results must be sent back as role=tool messages.
            for block in &msg.content {
                if let Content::ToolResult {
                    tool_use_id,
                    content,
                    ..
                } = block
                {
                    out.push(ChatMessage {
                        role: crate::api::Role::Tool,
                        content: Some(content.clone()),
                        tool_calls: None,
                        tool_call_id: Some(tool_use_id.clone()),
                    });
                }
            }

            // Build assistant tool calls (if any).
            let mut tool_calls: Vec<crate::api::ToolCall> = Vec::new();
            let mut text = String::new();
            for block in &msg.content {
                match block {
                    Content::Text { text: t } => text.push_str(t),
                    Content::ToolUse { id, name, input } => {
                        let args =
                            serde_json::to_string(input).unwrap_or_else(|_| "null".to_string());
                        tool_calls.push(crate::api::ToolCall {
                            id: id.clone(),
                            tool_type: "function".to_string(),
                            function: crate::api::FunctionCall {
                                name: name.clone(),
                                arguments: args,
                            },
                        });
                    }
                    _ => {}
                }
            }

            match msg.role {
                Role::System => {
                    if !text.trim().is_empty() {
                        out.push(ChatMessage::system(text));
                    }
                }
                Role::User => {
                    if !text.trim().is_empty() {
                        out.push(ChatMessage::user(text));
                    }
                }
                Role::Assistant => {
                    if tool_calls.is_empty() {
                        if !text.trim().is_empty() {
                            out.push(ChatMessage::assistant(text));
                        }
                    } else {
                        out.push(ChatMessage {
                            role: crate::api::Role::Assistant,
                            content: if text.trim().is_empty() {
                                None
                            } else {
                                Some(text)
                            },
                            tool_calls: Some(tool_calls),
                            tool_call_id: None,
                        });
                    }
                }
            }
        }

        out
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

        let tools = options.tools.as_ref().map(|defs| {
            defs.iter()
                .map(|d| ApiTool {
                    tool_type: "function".to_string(),
                    function: ApiToolFunction {
                        name: d.name.clone(),
                        description: Some(d.description.clone()),
                        parameters: d.parameters.clone(),
                    },
                })
                .collect::<Vec<_>>()
        });

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
            tools,
        }
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn name(&self) -> &str {
        self.provider_name.as_str()
    }

    fn models(&self) -> Vec<ModelInfo> {
        let provider = self.provider_name.clone();
        let mut models = vec![
            ModelInfo {
                id: "gpt-5.3".to_string(),
                name: "GPT-5.3".to_string(),
                provider: provider.clone(),
                context_window: 400000,
                max_output_tokens: Some(128000),
            },
            ModelInfo {
                id: "gpt-5.3-codex".to_string(),
                name: "GPT-5.3 Codex".to_string(),
                provider: provider.clone(),
                context_window: 400000,
                max_output_tokens: Some(128000),
            },
            ModelInfo {
                id: "gpt-5.2".to_string(),
                name: "GPT-5.2".to_string(),
                provider: provider.clone(),
                context_window: 400000,
                max_output_tokens: Some(128000),
            },
            ModelInfo {
                id: "gpt-5.2-codex".to_string(),
                name: "GPT-5.2 Codex".to_string(),
                provider: provider.clone(),
                context_window: 400000,
                max_output_tokens: Some(128000),
            },
            ModelInfo {
                id: "gpt-5.1".to_string(),
                name: "GPT-5.1".to_string(),
                provider: provider.clone(),
                context_window: 400000,
                max_output_tokens: Some(128000),
            },
            ModelInfo {
                id: "gpt-5.1-codex".to_string(),
                name: "GPT-5.1 Codex".to_string(),
                provider: provider.clone(),
                context_window: 400000,
                max_output_tokens: Some(128000),
            },
            ModelInfo {
                id: "gpt-5.1-codex-mini".to_string(),
                name: "GPT-5.1 Codex Mini".to_string(),
                provider: provider.clone(),
                context_window: 400000,
                max_output_tokens: Some(128000),
            },
            ModelInfo {
                id: "gpt-5.1-codex-max".to_string(),
                name: "GPT-5.1 Codex Max".to_string(),
                provider: provider.clone(),
                context_window: 400000,
                max_output_tokens: Some(128000),
            },
            ModelInfo {
                id: "gpt-5.0".to_string(),
                name: "GPT-5.0".to_string(),
                provider: provider.clone(),
                context_window: 400000,
                max_output_tokens: Some(128000),
            },
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                provider: provider.clone(),
                context_window: 128000,
                max_output_tokens: Some(16384),
            },
            ModelInfo {
                id: "gpt-4o-mini".to_string(),
                name: "GPT-4o Mini".to_string(),
                provider: provider.clone(),
                context_window: 128000,
                max_output_tokens: Some(16384),
            },
            ModelInfo {
                id: "gpt-4-turbo".to_string(),
                name: "GPT-4 Turbo".to_string(),
                provider: provider.clone(),
                context_window: 128000,
                max_output_tokens: Some(4096),
            },
            ModelInfo {
                id: "gpt-3.5-turbo".to_string(),
                name: "GPT-3.5 Turbo".to_string(),
                provider: provider.clone(),
                context_window: 16385,
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
                context_window: if self.default_model.starts_with("gpt-5") {
                    400000
                } else {
                    128000
                },
                max_output_tokens: None,
            });
        }

        models
    }

    async fn chat(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let request = self.build_request(messages, &options, false);

        debug!(
            provider = %self.provider_name,
            model = %request.model,
            "Sending chat request"
        );

        let mut builder = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");
        for (key, value) in &self.extra_headers {
            let key_trimmed = key.trim();
            if key_trimmed.is_empty() {
                continue;
            }
            if key_trimmed.eq_ignore_ascii_case("authorization")
                || key_trimmed.eq_ignore_ascii_case("content-type")
            {
                continue;
            }
            builder = builder.header(key_trimmed, value);
        }

        let response = builder
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

        let tool_uses: Vec<ToolUse> = completion
            .choices
            .first()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.tool_calls.clone())
            .unwrap_or_default()
            .into_iter()
            .map(|tc| {
                let input = serde_json::from_str::<serde_json::Value>(&tc.function.arguments)
                    .unwrap_or(serde_json::Value::Null);
                ToolUse {
                    id: tc.id,
                    name: tc.function.name,
                    input,
                }
            })
            .collect();

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
            tool_uses,
        })
    }

    async fn stream(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>> {
        let url = format!("{}/chat/completions", self.base_url);
        let request = self.build_request(messages, &options, true);

        debug!(
            provider = %self.provider_name,
            model = %request.model,
            "Starting streaming request"
        );

        let mut builder = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");
        for (key, value) in &self.extra_headers {
            let key_trimmed = key.trim();
            if key_trimmed.is_empty() {
                continue;
            }
            if key_trimmed.eq_ignore_ascii_case("authorization")
                || key_trimmed.eq_ignore_ascii_case("content-type")
            {
                continue;
            }
            builder = builder.header(key_trimmed, value);
        }

        let response = builder
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
        assert!(models.iter().any(|m| m.id == "gpt-5.3"));
        assert!(models.iter().any(|m| m.id == "gpt-5.3-codex"));
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
