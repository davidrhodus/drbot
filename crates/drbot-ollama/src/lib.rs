//! Local Ollama provider for drbot.
//!
//! This crate provides integration with locally running Ollama models.
//! Ollama allows running LLMs locally with a simple API.

mod api;

pub use api::*;

use async_trait::async_trait;
use drbot_core::message::{Message, Role};
use drbot_providers::{
    ChatOptions, ChatResponse as ProviderChatResponse, ModelInfo as ProviderModelInfo, Provider,
    StreamEvent, Usage,
};
use futures::stream::Stream;
use reqwest::Client;
use std::pin::Pin;
use tracing::{debug, error, trace};

/// Default Ollama API URL.
pub const DEFAULT_BASE_URL: &str = "http://localhost:11434";

/// Default model to use.
pub const DEFAULT_MODEL: &str = "llama3.2";

/// Ollama provider implementation.
pub struct OllamaProvider {
    /// HTTP client.
    client: Client,
    /// Base URL for Ollama API.
    base_url: String,
    /// Default model.
    default_model: String,
}

impl OllamaProvider {
    /// Create a new Ollama provider with default settings.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            base_url: DEFAULT_BASE_URL.to_string(),
            default_model: DEFAULT_MODEL.to_string(),
        }
    }

    /// Create with a custom base URL.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the default model.
    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    /// Convert our Message type to Ollama's format.
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

    /// Build a chat request.
    fn build_request(
        &self,
        messages: &[Message],
        options: &ChatOptions,
        stream: bool,
    ) -> ChatRequest {
        let mut api_messages = Self::convert_messages(messages);

        // Handle system prompt from options
        if let Some(system) = &options.system_prompt {
            if api_messages
                .first()
                .map(|m| m.role == "system")
                .unwrap_or(false)
            {
                api_messages[0] = ChatMessage::system(system);
            } else {
                api_messages.insert(0, ChatMessage::system(system));
            }
        }

        let gen_options = if options.temperature.is_some()
            || options.top_p.is_some()
            || options.max_tokens.is_some()
            || options.stop_sequences.is_some()
        {
            Some(GenerationOptions {
                temperature: options.temperature,
                top_p: options.top_p,
                num_predict: options.max_tokens,
                stop: options.stop_sequences.clone(),
            })
        } else {
            None
        };

        ChatRequest {
            model: options
                .model
                .clone()
                .unwrap_or_else(|| self.default_model.clone()),
            messages: api_messages,
            stream: Some(stream),
            options: gen_options,
        }
    }

    /// List available models from Ollama.
    pub async fn list_models(&self) -> drbot_core::Result<Vec<ModelInfo>> {
        let url = format!("{}/api/tags", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(drbot_core::Error::Provider(format!(
                "Ollama API error ({}): {}",
                status, body
            )));
        }

        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        Ok(tags.models)
    }

    /// Check if Ollama is running.
    pub async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        self.client.get(&url).send().await.is_ok()
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn models(&self) -> Vec<ProviderModelInfo> {
        // Return common Ollama models
        // In practice, you'd call list_models() but that's async
        vec![
            ProviderModelInfo {
                id: "llama3.2".to_string(),
                name: "Llama 3.2".to_string(),
                provider: "ollama".to_string(),
                context_window: 128000,
                max_output_tokens: Some(4096),
            },
            ProviderModelInfo {
                id: "llama3.1".to_string(),
                name: "Llama 3.1".to_string(),
                provider: "ollama".to_string(),
                context_window: 128000,
                max_output_tokens: Some(4096),
            },
            ProviderModelInfo {
                id: "mistral".to_string(),
                name: "Mistral".to_string(),
                provider: "ollama".to_string(),
                context_window: 32000,
                max_output_tokens: Some(4096),
            },
            ProviderModelInfo {
                id: "codellama".to_string(),
                name: "Code Llama".to_string(),
                provider: "ollama".to_string(),
                context_window: 16000,
                max_output_tokens: Some(4096),
            },
            ProviderModelInfo {
                id: "phi3".to_string(),
                name: "Phi-3".to_string(),
                provider: "ollama".to_string(),
                context_window: 4096,
                max_output_tokens: Some(2048),
            },
        ]
    }

    async fn chat(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<ProviderChatResponse> {
        let url = format!("{}/api/chat", self.base_url);
        let request = self.build_request(messages, &options, false);

        debug!(model = %request.model, "Sending chat request to Ollama");

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if let Ok(error) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(drbot_core::Error::Provider(error.error));
            }
            return Err(drbot_core::Error::Provider(format!(
                "Ollama API error ({}): {}",
                status, body
            )));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        Ok(ProviderChatResponse {
            content: chat_response.message.content,
            model: chat_response.model,
            stop_reason: Some("stop".to_string()),
            usage: chat_response.eval_count.map(|eval| Usage {
                input_tokens: chat_response.prompt_eval_count.unwrap_or(0) as usize,
                output_tokens: eval as usize,
            }),
            tool_uses: Vec::new(),
        })
    }

    async fn stream(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>> {
        let url = format!("{}/api/chat", self.base_url);
        let request = self.build_request(messages, &options, true);

        debug!(model = %request.model, "Starting streaming request to Ollama");

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| drbot_core::Error::Http(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if let Ok(error) = serde_json::from_str::<ErrorResponse>(&body) {
                return Err(drbot_core::Error::Provider(error.error));
            }
            return Err(drbot_core::Error::Provider(format!(
                "Ollama API error ({}): {}",
                status, body
            )));
        }

        let stream = async_stream::stream! {
            use futures::StreamExt;

            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();

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

                // Process complete JSON objects (each line is a JSON object)
                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<ChatStreamChunk>(&line) {
                        Ok(chunk) => {
                            // Emit content delta
                            if !chunk.message.content.is_empty() {
                                yield StreamEvent::Delta {
                                    content: chunk.message.content.clone(),
                                };
                            }

                            // Check if done
                            if chunk.done {
                                trace!("Stream complete");
                                let usage = chunk.eval_count.map(|eval| Usage {
                                    input_tokens: chunk.prompt_eval_count.unwrap_or(0) as usize,
                                    output_tokens: eval as usize,
                                });

                                yield StreamEvent::Stop {
                                    reason: "stop".to_string(),
                                    usage,
                                };
                                break;
                            }
                        }
                        Err(e) => {
                            trace!(error = %e, line = %line, "Failed to parse chunk");
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
        let provider = OllamaProvider::new();
        assert_eq!(provider.name(), "ollama");
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
        assert_eq!(provider.default_model, DEFAULT_MODEL);
    }

    #[test]
    fn test_builder() {
        let provider = OllamaProvider::new()
            .with_base_url("http://localhost:8080")
            .with_default_model("mistral");

        assert_eq!(provider.base_url, "http://localhost:8080");
        assert_eq!(provider.default_model, "mistral");
    }

    #[test]
    fn test_models() {
        let provider = OllamaProvider::new();
        let models = provider.models();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "llama3.2"));
    }

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi!"),
        ];

        let converted = OllamaProvider::convert_messages(&messages);
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].role, "user");
        assert_eq!(converted[2].role, "assistant");
    }
}
