//! TUI application state and logic.

use crossterm::event::{KeyCode, KeyEvent};
use drbot_core::message::Message;
use drbot_core::Result;
use drbot_providers::{ChatOptions, Provider, StreamEvent};
use futures::StreamExt;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error};

/// Provider type selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProviderType {
    #[default]
    Anthropic,
    OpenAI,
    Ollama,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::Anthropic => write!(f, "anthropic"),
            ProviderType::OpenAI => write!(f, "openai"),
            ProviderType::Ollama => write!(f, "ollama"),
        }
    }
}

impl ProviderType {
    /// Parse from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "anthropic" | "claude" => Some(ProviderType::Anthropic),
            "openai" | "gpt" => Some(ProviderType::OpenAI),
            "ollama" | "local" => Some(ProviderType::Ollama),
            _ => None,
        }
    }
}

/// Application configuration.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Provider type to use.
    pub provider_type: ProviderType,
    /// API key for the provider.
    pub api_key: Option<String>,
    /// Base URL for the provider (optional).
    pub base_url: Option<String>,
    /// Model to use.
    pub model: Option<String>,
    /// System prompt.
    pub system_prompt: Option<String>,
    /// Maximum history to keep.
    pub max_history: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider_type: ProviderType::default(),
            api_key: None,
            base_url: None,
            model: None,
            system_prompt: None,
            max_history: 100,
        }
    }
}

/// A chat message.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Message role.
    pub role: MessageRole,
    /// Message content.
    pub content: String,
}

/// Message role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

/// Streaming token from AI provider.
enum StreamToken {
    Delta(String),
    Done,
    Error(String),
}

/// Application state.
pub struct App {
    /// Configuration.
    pub config: AppConfig,
    /// Chat history.
    pub messages: VecDeque<ChatMessage>,
    /// Current input buffer.
    pub input: String,
    /// Cursor position in input.
    pub cursor_pos: usize,
    /// Scroll offset for messages.
    pub scroll_offset: usize,
    /// Whether the app should quit.
    quit: bool,
    /// Whether we're currently waiting for a response.
    pub is_loading: bool,
    /// Current streaming response buffer.
    pub streaming_content: String,
    /// Status message.
    pub status: String,
    /// AI provider (dynamic).
    provider: Option<Arc<dyn Provider>>,
    /// Provider name for display.
    pub provider_name: String,
    /// Channel receiver for streaming tokens.
    stream_rx: Option<mpsc::Receiver<StreamToken>>,
}

impl App {
    /// Create a new app instance.
    pub async fn new(config: AppConfig) -> Result<Self> {
        use drbot_anthropic::AnthropicProvider;
        use drbot_ollama::OllamaProvider;
        use drbot_openai::OpenAIProvider;

        // Create provider based on type
        let (provider, provider_name): (Option<Arc<dyn Provider>>, String) =
            match config.provider_type {
                ProviderType::Anthropic => {
                    if let Some(key) = &config.api_key {
                        let mut p = AnthropicProvider::new(key);
                        if let Some(model) = &config.model {
                            p = p.with_default_model(model);
                        }
                        if let Some(base_url) = &config.base_url {
                            p = p.with_base_url(base_url);
                        }
                        (Some(Arc::new(p)), "Anthropic".to_string())
                    } else {
                        (None, "Anthropic (no key)".to_string())
                    }
                }
                ProviderType::OpenAI => {
                    if let Some(key) = &config.api_key {
                        let mut p = OpenAIProvider::new(key);
                        if let Some(model) = &config.model {
                            p = p.with_default_model(model);
                        }
                        if let Some(base_url) = &config.base_url {
                            p = p.with_base_url(base_url);
                        }
                        (Some(Arc::new(p)), "OpenAI".to_string())
                    } else {
                        (None, "OpenAI (no key)".to_string())
                    }
                }
                ProviderType::Ollama => {
                    let base_url = config
                        .base_url
                        .as_deref()
                        .unwrap_or("http://localhost:11434");
                    let mut p = OllamaProvider::new().with_base_url(base_url);
                    if let Some(model) = &config.model {
                        p = p.with_default_model(model);
                    }
                    (Some(Arc::new(p)), "Ollama".to_string())
                }
            };

        let has_provider = provider.is_some();

        let mut app = Self {
            config,
            messages: VecDeque::new(),
            input: String::new(),
            cursor_pos: 0,
            scroll_offset: 0,
            quit: false,
            is_loading: false,
            streaming_content: String::new(),
            status: if has_provider {
                "Ready".to_string()
            } else {
                "No API key - responses disabled".to_string()
            },
            provider,
            provider_name,
            stream_rx: None,
        };

        // Add system message if configured
        if let Some(prompt) = &app.config.system_prompt {
            app.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: prompt.clone(),
            });
        }

        Ok(app)
    }

    /// Check if the app should quit.
    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Handle a key event.
    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.is_loading {
            // Ignore most keys while loading
            return Ok(());
        }

        match key.code {
            KeyCode::Enter => {
                self.submit_message().await?;
            }
            KeyCode::Char(c) => {
                self.input.insert(self.cursor_pos, c);
                self.cursor_pos += 1;
            }
            KeyCode::Backspace => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    self.input.remove(self.cursor_pos);
                }
            }
            KeyCode::Delete => {
                if self.cursor_pos < self.input.len() {
                    self.input.remove(self.cursor_pos);
                }
            }
            KeyCode::Left => {
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
            }
            KeyCode::Right => {
                if self.cursor_pos < self.input.len() {
                    self.cursor_pos += 1;
                }
            }
            KeyCode::Home => {
                self.cursor_pos = 0;
            }
            KeyCode::End => {
                self.cursor_pos = self.input.len();
            }
            KeyCode::Up => {
                // Scroll up
                if self.scroll_offset > 0 {
                    self.scroll_offset -= 1;
                }
            }
            KeyCode::Down => {
                // Scroll down
                self.scroll_offset += 1;
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.scroll_offset += 10;
            }
            _ => {}
        }

        Ok(())
    }

    /// Submit the current input as a message.
    async fn submit_message(&mut self) -> Result<()> {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return Ok(());
        }

        // Clear input
        self.input.clear();
        self.cursor_pos = 0;

        // Handle commands
        if text.starts_with('/') {
            return self.handle_command(&text);
        }

        // Add user message
        self.messages.push_back(ChatMessage {
            role: MessageRole::User,
            content: text.clone(),
        });

        // Trim history if needed
        while self.messages.len() > self.config.max_history {
            self.messages.pop_front();
        }

        // Check if we have a provider
        let Some(provider) = self.provider.clone() else {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "No provider configured. Set the appropriate API key or configure Ollama."
                    .to_string(),
            });
            return Ok(());
        };

        // Set loading state
        self.is_loading = true;
        self.status = "Thinking...".to_string();
        self.streaming_content.clear();

        // Build messages for the provider
        let mut provider_messages = Vec::new();
        for msg in &self.messages {
            match msg.role {
                MessageRole::User => provider_messages.push(Message::user(&msg.content)),
                MessageRole::Assistant => provider_messages.push(Message::assistant(&msg.content)),
                MessageRole::System => {} // System handled separately
            }
        }

        // Build options
        let options = ChatOptions {
            model: self.config.model.clone(),
            max_tokens: Some(4096),
            temperature: None,
            top_p: None,
            stop_sequences: None,
            system_prompt: self.config.system_prompt.clone(),
            tools: None,
        };

        // Create channel for streaming
        let (tx, rx) = mpsc::channel::<StreamToken>(100);
        self.stream_rx = Some(rx);

        // Spawn task to stream response
        tokio::spawn(async move {
            debug!("Starting AI stream");
            match provider.stream(&provider_messages, options).await {
                Ok(mut stream) => {
                    while let Some(event) = stream.next().await {
                        let token = match event {
                            StreamEvent::Delta { content } => StreamToken::Delta(content),
                            StreamEvent::Stop { .. } => StreamToken::Done,
                            StreamEvent::Error { message } => StreamToken::Error(message),
                            _ => continue,
                        };
                        if tx.send(token).await.is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Stream error");
                    let _ = tx.send(StreamToken::Error(e.to_string())).await;
                }
            }
        });

        Ok(())
    }

    /// Handle a slash command.
    fn handle_command(&mut self, command: &str) -> Result<()> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        match parts.first().map(|s| *s) {
            Some("/quit") | Some("/exit") => {
                self.quit = true;
            }
            Some("/clear") => {
                self.messages.clear();
                self.scroll_offset = 0;
                self.status = "Cleared".to_string();
            }
            Some("/help") => {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: "Commands:\n  /quit, /exit - Exit the app\n  /clear - Clear chat history\n  /help - Show this help".to_string(),
                });
            }
            _ => {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: format!("Unknown command: {}", command),
                });
            }
        }
        Ok(())
    }

    /// Process async operations (called every tick).
    pub async fn tick(&mut self) -> Result<()> {
        // Process streaming tokens
        if let Some(rx) = &mut self.stream_rx {
            // Try to receive without blocking
            while let Ok(token) = rx.try_recv() {
                match token {
                    StreamToken::Delta(content) => {
                        self.streaming_content.push_str(&content);
                        self.status =
                            format!("Streaming... ({} chars)", self.streaming_content.len());
                    }
                    StreamToken::Done => {
                        // Add completed response to messages
                        if !self.streaming_content.is_empty() {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::Assistant,
                                content: std::mem::take(&mut self.streaming_content),
                            });
                        }
                        self.is_loading = false;
                        self.status = "Ready".to_string();
                        self.stream_rx = None;
                        break;
                    }
                    StreamToken::Error(msg) => {
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!("Error: {}", msg),
                        });
                        self.streaming_content.clear();
                        self.is_loading = false;
                        self.status = "Error".to_string();
                        self.stream_rx = None;
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Get visible messages based on scroll offset.
    pub fn visible_messages(&self, max_lines: usize) -> impl Iterator<Item = &ChatMessage> {
        let total = self.messages.len();
        let start = if total > max_lines {
            (total - max_lines).saturating_sub(self.scroll_offset)
        } else {
            0
        };
        self.messages.iter().skip(start).take(max_lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_app_creation() {
        let config = AppConfig::default();
        let app = App::new(config).await.unwrap();
        assert!(!app.should_quit());
        assert!(app.messages.is_empty());
    }

    #[tokio::test]
    async fn test_command_handling() {
        let config = AppConfig::default();
        let mut app = App::new(config).await.unwrap();

        app.input = "/help".to_string();
        app.cursor_pos = 5;
        app.submit_message().await.unwrap();

        assert_eq!(app.messages.len(), 1);
        assert!(app.messages[0].content.contains("Commands:"));
    }

    #[tokio::test]
    async fn test_quit_command() {
        let config = AppConfig::default();
        let mut app = App::new(config).await.unwrap();

        app.input = "/quit".to_string();
        app.cursor_pos = 5;
        app.submit_message().await.unwrap();

        assert!(app.should_quit());
    }
}
