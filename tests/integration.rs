//! Integration tests for drbot.
//!
//! These tests verify the integration between different components.

use drbot_core::message::Message;
use drbot_core::Config;
use drbot_providers::{ChatOptions, Provider};

/// Test that providers can be created from config.
#[test]
fn test_provider_creation_from_config() {
    // Test that config loads with defaults
    let config = Config::default();

    // Verify default values
    assert_eq!(config.gateway.host, "127.0.0.1");
    assert_eq!(config.gateway.port, 18789);
    assert!(config.providers.anthropic.is_none());
    assert!(config.providers.openai.is_none());
}

/// Test message creation and serialization.
#[test]
fn test_message_serialization() {
    let msg = Message::user("Hello, world!");
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.text_content(), "Hello, world!");
}

/// Test provider trait is object-safe.
#[test]
fn test_provider_trait_object_safe() {
    // This test verifies that Provider can be used as a trait object
    fn _accepts_provider(_: &dyn Provider) {}

    // The fact that this compiles proves Provider is object-safe
}

/// Test chat options defaults.
#[test]
fn test_chat_options_defaults() {
    let options = ChatOptions::default();
    assert!(options.model.is_none());
    assert!(options.max_tokens.is_none());
    assert!(options.temperature.is_none());
}

/// Test config serialization round-trip.
#[test]
fn test_config_serialization_roundtrip() {
    use drbot_core::config::{AnthropicConfig, OllamaConfig};

    let mut config = Config::default();
    config.providers.anthropic = Some(AnthropicConfig {
        api_key: "test-key".to_string(),
        base_url: None,
        headers: Default::default(),
        default_model: Some("claude-sonnet-4-20250514".to_string()),
        max_tokens: Some(4096),
    });
    config.providers.ollama = Some(OllamaConfig {
        url: "http://localhost:11434".to_string(),
        default_model: Some("llama3.2".to_string()),
    });

    let toml_str = toml::to_string_pretty(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();

    assert!(parsed.providers.anthropic.is_some());
    assert_eq!(
        parsed.providers.anthropic.unwrap().default_model,
        Some("claude-sonnet-4-20250514".to_string())
    );
    assert!(parsed.providers.ollama.is_some());
}

/// Test protocol message types.
#[test]
fn test_protocol_message_types() {
    use drbot_protocol::WsMessage;
    use uuid::Uuid;

    // Test request message creation
    let id = Uuid::new_v4();
    let request = WsMessage::request(
        id,
        "chat.send",
        serde_json::json!({
            "message": "Hello"
        }),
    );

    let json = request.to_json().unwrap();
    assert!(json.contains("chat.send"));

    // Test response message creation
    let response = WsMessage::success(
        id,
        serde_json::json!({
            "status": "ok",
            "session_id": "session-456"
        }),
    );

    let json = response.to_json().unwrap();
    assert!(json.contains("session-456"));

    // Test event message creation
    let event = WsMessage::event(
        "message.received",
        serde_json::json!({
            "content": "Hello world"
        }),
    );

    let json = event.to_json().unwrap();
    assert!(json.contains("message.received"));
}

#[cfg(test)]
mod anthropic_tests {
    use drbot_anthropic::AnthropicProvider;
    use drbot_providers::Provider;

    #[test]
    fn test_anthropic_provider_creation() {
        let provider = AnthropicProvider::new("test-key");
        assert_eq!(provider.name(), "anthropic");
    }

    #[test]
    fn test_anthropic_with_options() {
        let provider = AnthropicProvider::new("test-key")
            .with_default_model("claude-sonnet-4-20250514")
            .with_default_max_tokens(8192);

        // Verify it has the right name
        assert_eq!(provider.name(), "anthropic");
    }
}

#[cfg(test)]
mod openai_tests {
    use drbot_openai::OpenAIProvider;
    use drbot_providers::Provider;

    #[test]
    fn test_openai_provider_creation() {
        let provider = OpenAIProvider::new("test-key");
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_openai_with_base_url() {
        let provider = OpenAIProvider::new("test-key")
            .with_base_url("https://api.openai.com/v1")
            .with_default_model("gpt-4o");

        assert_eq!(provider.name(), "openai");
    }
}

#[cfg(test)]
mod ollama_tests {
    use drbot_ollama::OllamaProvider;
    use drbot_providers::Provider;

    #[test]
    fn test_ollama_provider_creation() {
        let provider = OllamaProvider::new();
        assert_eq!(provider.name(), "ollama");
    }

    #[test]
    fn test_ollama_with_options() {
        let provider = OllamaProvider::new()
            .with_base_url("http://localhost:11434")
            .with_default_model("llama3.2");

        assert_eq!(provider.name(), "ollama");
    }
}

#[cfg(test)]
mod gateway_tests {
    use drbot_core::Config;
    use drbot_gateway::Gateway;

    #[test]
    fn test_gateway_creation() {
        let config = Config::default();
        let gateway = Gateway::new(config);
        // Gateway created successfully
        drop(gateway);
    }
}

#[cfg(test)]
mod tui_tests {
    use drbot_tui::{AppConfig, ProviderType};

    #[test]
    fn test_provider_type_parsing() {
        assert_eq!(
            ProviderType::from_str("anthropic"),
            Some(ProviderType::Anthropic)
        );
        assert_eq!(
            ProviderType::from_str("claude"),
            Some(ProviderType::Anthropic)
        );
        assert_eq!(ProviderType::from_str("openai"), Some(ProviderType::OpenAI));
        assert_eq!(ProviderType::from_str("gpt"), Some(ProviderType::OpenAI));
        assert_eq!(ProviderType::from_str("ollama"), Some(ProviderType::Ollama));
        assert_eq!(ProviderType::from_str("local"), Some(ProviderType::Ollama));
        assert_eq!(ProviderType::from_str("unknown"), None);
    }

    #[test]
    fn test_app_config_defaults() {
        let config = AppConfig::default();
        assert_eq!(config.provider_type, ProviderType::Anthropic);
        assert!(config.api_key.is_none());
        assert!(config.model.is_none());
        assert_eq!(config.max_history, 100);
    }
}
