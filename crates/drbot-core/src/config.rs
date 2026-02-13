//! Configuration system for drbot.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Main configuration for drbot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Gateway server configuration.
    #[serde(default)]
    pub gateway: GatewayConfig,

    /// Inbound webhook endpoints (OpenClaw parity: /hooks/*).
    #[serde(default)]
    pub hooks: HooksConfig,

    /// AI provider configurations.
    #[serde(default)]
    pub providers: ProvidersConfig,

    /// Channel configurations.
    #[serde(default)]
    pub channels: ChannelsConfig,

    /// Message formatting configuration.
    #[serde(default)]
    pub messages: MessagesConfig,

    /// Storage configuration.
    #[serde(default)]
    pub storage: StorageConfig,

    /// Logging configuration.
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl Config {
    /// Load configuration from a file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("failed to read config file: {}", e)))?;

        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("toml");

        match extension {
            "toml" => toml::from_str(&content)
                .map_err(|e| Error::Config(format!("failed to parse TOML config: {}", e))),
            "json" => serde_json::from_str(&content)
                .map_err(|e| Error::Config(format!("failed to parse JSON config: {}", e))),
            _ => Err(Error::Config(format!(
                "unsupported config file extension: {}",
                extension
            ))),
        }
    }

    /// Load configuration from environment and optional file.
    pub fn load() -> Result<Self> {
        if let Ok(path) = std::env::var("DRBOT_CONFIG_PATH") {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                let candidate = PathBuf::from(trimmed);
                if candidate.exists() {
                    return Self::from_file(candidate);
                }
            }
        }

        // Check for config file in standard locations
        //
        // Note: the setup wizard writes to `config.toml` in the config dir, so include it here.
        let mut config_paths = vec![
            PathBuf::from("drbot.toml"),
            PathBuf::from("config/drbot.toml"),
        ];
        if let Some(dir) = dirs::config_dir() {
            config_paths.push(dir.join("drbot").join("config.toml"));
            // Back-compat with older naming.
            config_paths.push(dir.join("drbot").join("drbot.toml"));
        }

        for path in config_paths {
            if path.exists() {
                return Self::from_file(path);
            }
        }

        // No config file found, use defaults
        Ok(Self::default())
    }

    /// Get the default configuration directory.
    pub fn config_dir() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("drbot"))
    }

    /// Get the default data directory.
    pub fn data_dir() -> Option<PathBuf> {
        dirs::data_dir().map(|p| p.join("drbot"))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gateway: GatewayConfig::default(),
            hooks: HooksConfig::default(),
            providers: ProvidersConfig::default(),
            channels: ChannelsConfig::default(),
            messages: MessagesConfig::default(),
            storage: StorageConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

/// Message formatting configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessagesConfig {
    /// Optional outbound response prefix to prepend to bot replies.
    ///
    /// OpenClaw parity: supports `responsePrefix = "auto"` which derives a prefix from the routed
    /// agent's identity name (e.g. "[drbot]").
    #[serde(
        default,
        rename = "responsePrefix",
        alias = "response_prefix",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_prefix: Option<String>,
}

/// Gateway server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Host to bind to.
    #[serde(default = "default_gateway_host")]
    pub host: String,
    /// Port to listen on.
    #[serde(default = "default_gateway_port")]
    pub port: u16,
    /// Authentication token (optional).
    pub auth_token: Option<String>,
    /// Enable TLS.
    #[serde(default)]
    pub tls_enabled: bool,
    /// TLS certificate path.
    pub tls_cert: Option<PathBuf>,
    /// TLS key path.
    pub tls_key: Option<PathBuf>,
}

fn default_gateway_host() -> String {
    "127.0.0.1".to_string()
}

fn default_gateway_port() -> u16 {
    18789
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: default_gateway_host(),
            port: default_gateway_port(),
            auth_token: None,
            tls_enabled: false,
            tls_cert: None,
            tls_key: None,
        }
    }
}

fn default_hooks_path() -> String {
    "/hooks".to_string()
}

/// Inbound webhook configuration.
///
/// OpenClaw parity: matches `hooks.*` documented in OpenClaw's gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HooksConfig {
    /// Enable /hooks endpoints.
    #[serde(default)]
    pub enabled: bool,

    /// Shared secret used to authorize webhook calls.
    pub token: Option<String>,

    /// Base path for hook endpoints (default: "/hooks").
    #[serde(default = "default_hooks_path")]
    pub path: String,

    /// Maximum webhook request body size in bytes.
    #[serde(
        default,
        rename = "maxBodyBytes",
        alias = "max_body_bytes",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_body_bytes: Option<u64>,

    /// Maximum accepted message length for hook payloads.
    #[serde(
        default,
        rename = "maxMessageChars",
        alias = "max_message_chars",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_message_chars: Option<u64>,

    /// Optional allowlist of agent ids that may be targeted via hooks.
    #[serde(
        default,
        rename = "allowedAgentIds",
        alias = "allowed_agent_ids",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub allowed_agent_ids: Vec<String>,

    /// If true, allow callers to set sessionKey in hook requests.
    #[serde(
        default,
        rename = "allowRequestSessionKey",
        alias = "allow_request_session_key"
    )]
    pub allow_request_session_key: bool,

    /// Default sessionKey for hooks when not provided (default: "main").
    #[serde(
        default,
        rename = "defaultSessionKey",
        alias = "default_session_key",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_session_key: Option<String>,

    /// Optional allowlist of accepted sessionKey prefixes.
    #[serde(
        default,
        rename = "allowedSessionKeyPrefixes",
        alias = "allowed_session_key_prefixes",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub allowed_session_key_prefixes: Vec<String>,
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: None,
            path: default_hooks_path(),
            max_body_bytes: None,
            max_message_chars: None,
            allowed_agent_ids: Vec::new(),
            allow_request_session_key: false,
            default_session_key: None,
            allowed_session_key_prefixes: Vec::new(),
        }
    }
}

/// AI providers configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvidersConfig {
    /// Default provider to use.
    pub default_provider: Option<String>,
    /// Default model to use.
    pub default_model: Option<String>,
    /// Anthropic configuration.
    pub anthropic: Option<AnthropicConfig>,
    /// OpenAI configuration.
    pub openai: Option<OpenAIConfig>,
    /// Ollama configuration.
    pub ollama: Option<OllamaConfig>,
    /// AWS Bedrock configuration.
    pub bedrock: Option<BedrockConfig>,
    /// Custom CLI provider configurations.
    #[serde(default)]
    pub cli: Vec<CliProviderConfig>,
    /// Additional OpenAI-compatible providers (OpenRouter, xAI, etc).
    #[serde(default)]
    pub openai_compatible: Vec<OpenAICompatibleConfig>,
}

/// Configuration for an OpenAI-compatible provider (e.g. OpenRouter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAICompatibleConfig {
    /// Provider name used in --provider and /model (e.g. "openrouter", "xai").
    pub name: String,
    /// API key.
    pub api_key: String,
    /// Base URL for the API (should include `/v1` for OpenAI-compatible endpoints).
    pub base_url: String,
    /// Default model name.
    pub default_model: Option<String>,
    /// Additional headers to send with each request.
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// Configuration for a custom CLI provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliProviderConfig {
    /// Provider name used in --provider and /model.
    pub name: String,
    /// CLI command to run (e.g. "claude", "codex", "my-local-llm").
    pub command: String,
    /// Base arguments (e.g. ["-p"] or ["exec", "--full-auto", "-"]).
    #[serde(default)]
    pub args: Vec<String>,
    /// Flag for specifying the model (e.g. "--model" or "-m").
    #[serde(default = "default_model_flag")]
    pub model_flag: String,
    /// Default model name.
    #[serde(default)]
    pub default_model: Option<String>,
    /// Flag for passing system prompt (e.g. "--system-prompt"). None if unsupported.
    pub system_flag: Option<String>,
    /// If true, send full conversation history via stdin instead of just last user message.
    #[serde(default)]
    pub send_history: bool,
    /// Timeout in seconds for CLI execution. Default: 120.
    pub timeout_secs: Option<u64>,
}

fn default_model_flag() -> String {
    "--model".to_string()
}

/// Anthropic provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// API key.
    pub api_key: String,
    /// Base URL (optional, for proxies).
    pub base_url: Option<String>,
    /// Additional headers to send with each request (for gateways/proxies like Cloudflare AI Gateway).
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Default model.
    pub default_model: Option<String>,
    /// Max tokens default.
    pub max_tokens: Option<usize>,
}

/// OpenAI provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// API key.
    pub api_key: String,
    /// Base URL (optional, for proxies or Azure).
    pub base_url: Option<String>,
    /// Additional headers to send with each request (for gateways/proxies like Cloudflare AI Gateway).
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Organization ID (optional).
    pub organization: Option<String>,
    /// Default model.
    pub default_model: Option<String>,
}

/// Ollama provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Ollama server URL.
    #[serde(default = "default_ollama_url")]
    pub url: String,
    /// Default model.
    pub default_model: Option<String>,
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            url: default_ollama_url(),
            default_model: None,
        }
    }
}

/// AWS Bedrock provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedrockConfig {
    /// AWS region.
    pub region: String,
    /// AWS profile (optional).
    pub profile: Option<String>,
    /// Default model.
    pub default_model: Option<String>,
}

/// Channels configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelsConfig {
    /// Enabled channels.
    #[serde(default)]
    pub enabled: Vec<String>,
    /// Telegram configuration.
    pub telegram: Option<TelegramConfig>,
    /// Discord configuration.
    pub discord: Option<DiscordConfig>,
    /// Slack configuration.
    pub slack: Option<SlackConfig>,
    /// WhatsApp configuration.
    pub whatsapp: Option<WhatsAppConfig>,
    /// Signal configuration.
    pub signal: Option<SignalConfig>,
    /// iMessage configuration.
    pub imessage: Option<IMessageConfig>,
    /// Matrix configuration.
    pub matrix: Option<MatrixConfig>,
    /// WebChat configuration.
    pub webchat: Option<WebChatConfig>,
}

/// Per-account channel overrides (OpenClaw parity).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelAccountConfig {
    /// Optional outbound response prefix override for this account.
    #[serde(
        default,
        rename = "responsePrefix",
        alias = "response_prefix",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_prefix: Option<String>,
}

/// Telegram channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Bot token.
    pub bot_token: String,
    /// Optional outbound response prefix override for this channel.
    #[serde(
        default,
        rename = "responsePrefix",
        alias = "response_prefix",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_prefix: Option<String>,
    /// Optional per-account outbound response prefix overrides (OpenClaw parity).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub accounts: HashMap<String, ChannelAccountConfig>,
    /// Allowed user IDs (empty = all).
    #[serde(default)]
    pub allowed_users: Vec<i64>,
    /// Allowed chat IDs (empty = all).
    #[serde(default)]
    pub allowed_chats: Vec<i64>,
}

/// Discord channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// Bot token.
    pub bot_token: String,
    /// Application ID.
    pub application_id: String,
    /// Optional outbound response prefix override for this channel.
    #[serde(
        default,
        rename = "responsePrefix",
        alias = "response_prefix",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_prefix: Option<String>,
    /// Optional per-account outbound response prefix overrides (OpenClaw parity).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub accounts: HashMap<String, ChannelAccountConfig>,
    /// Allowed guild IDs (empty = all).
    #[serde(default)]
    pub allowed_guilds: Vec<String>,
}

/// Slack channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackConfig {
    /// Bot token.
    pub bot_token: String,
    /// App token (for socket mode).
    pub app_token: String,
    /// Signing secret.
    pub signing_secret: String,
    /// Optional outbound response prefix override for this channel.
    #[serde(
        default,
        rename = "responsePrefix",
        alias = "response_prefix",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_prefix: Option<String>,
    /// Optional per-account outbound response prefix overrides (OpenClaw parity).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub accounts: HashMap<String, ChannelAccountConfig>,
}

/// WhatsApp channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    /// Session data path.
    pub session_path: PathBuf,
    /// Bridge WebSocket URL.
    #[serde(default = "default_whatsapp_bridge_url")]
    pub bridge_url: String,
    /// Optional outbound response prefix override for this channel.
    #[serde(
        default,
        rename = "responsePrefix",
        alias = "response_prefix",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_prefix: Option<String>,
    /// Optional per-account outbound response prefix overrides (OpenClaw parity).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub accounts: HashMap<String, ChannelAccountConfig>,
    /// Allowed phone numbers (empty = all).
    #[serde(default)]
    pub allowed_numbers: Vec<String>,
}

fn default_whatsapp_bridge_url() -> String {
    "ws://localhost:3001".to_string()
}

/// Signal channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    /// Signal CLI socket path or URL.
    pub socket_path: String,
    /// Phone number to use.
    pub phone_number: String,
    /// Optional outbound response prefix override for this channel.
    #[serde(
        default,
        rename = "responsePrefix",
        alias = "response_prefix",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_prefix: Option<String>,
    /// Optional per-account outbound response prefix overrides (OpenClaw parity).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub accounts: HashMap<String, ChannelAccountConfig>,
    /// Allowed numbers (empty = all).
    #[serde(default)]
    pub allowed_numbers: Vec<String>,
}

/// iMessage channel configuration (macOS only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IMessageConfig {
    /// Messages database path (defaults to ~/Library/Messages/chat.db).
    pub database_path: Option<PathBuf>,
    /// Optional outbound response prefix override for this channel.
    #[serde(
        default,
        rename = "responsePrefix",
        alias = "response_prefix",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_prefix: Option<String>,
    /// Optional per-account outbound response prefix overrides (OpenClaw parity).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub accounts: HashMap<String, ChannelAccountConfig>,
    /// Allowed contacts (empty = all).
    #[serde(default)]
    pub allowed_contacts: Vec<String>,
}

/// Matrix channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixConfig {
    /// Homeserver URL.
    pub homeserver_url: String,
    /// User ID (@user:server.com).
    pub user_id: String,
    /// Access token.
    pub access_token: String,
    /// Optional outbound response prefix override for this channel.
    #[serde(
        default,
        rename = "responsePrefix",
        alias = "response_prefix",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_prefix: Option<String>,
    /// Optional per-account outbound response prefix overrides (OpenClaw parity).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub accounts: HashMap<String, ChannelAccountConfig>,
    /// Allowed rooms (empty = all).
    #[serde(default)]
    pub allowed_rooms: Vec<String>,
}

/// WebChat channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebChatConfig {
    /// Port to serve on.
    #[serde(default = "default_webchat_port")]
    pub port: u16,
    /// Host to bind to.
    #[serde(default = "default_webchat_host")]
    pub host: String,
    /// Enable authentication.
    #[serde(default)]
    pub require_auth: bool,
    /// Authentication token (optional). If unset, callers may fall back to `gateway.auth_token`.
    pub auth_token: Option<String>,
    /// Optional outbound response prefix override for this channel.
    #[serde(
        default,
        rename = "responsePrefix",
        alias = "response_prefix",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_prefix: Option<String>,
    /// Optional per-account outbound response prefix overrides (OpenClaw parity).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub accounts: HashMap<String, ChannelAccountConfig>,
}

fn default_webchat_port() -> u16 {
    8080
}

fn default_webchat_host() -> String {
    "127.0.0.1".to_string()
}

impl Default for WebChatConfig {
    fn default() -> Self {
        Self {
            port: default_webchat_port(),
            host: default_webchat_host(),
            require_auth: false,
            auth_token: None,
            response_prefix: None,
            accounts: HashMap::new(),
        }
    }
}

impl WebChatConfig {
    /// Create a new WebChat configuration.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            require_auth: false,
            auth_token: None,
            response_prefix: None,
            accounts: HashMap::new(),
        }
    }
}

/// Storage configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Database path.
    #[serde(default = "default_db_path")]
    pub database_path: PathBuf,
    /// Media storage path.
    #[serde(default = "default_media_path")]
    pub media_path: PathBuf,
}

fn default_db_path() -> PathBuf {
    Config::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("drbot.db")
}

fn default_media_path() -> PathBuf {
    Config::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("media")
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            database_path: default_db_path(),
            media_path: default_media_path(),
        }
    }
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error).
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Log format (pretty, json).
    #[serde(default = "default_log_format")]
    pub format: String,
    /// Log file path (optional).
    pub file: Option<PathBuf>,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "pretty".to_string()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            file: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.gateway.host, "127.0.0.1");
        assert_eq!(config.gateway.port, 18789);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml).unwrap();
        assert_eq!(parsed.gateway.port, config.gateway.port);
    }
}
