//! Gateway state management.

use crate::client::Client;
use crate::channel_manager::ChannelManager;
use drbot_anthropic::AnthropicProvider;
use drbot_core::Config;
use drbot_ollama::OllamaProvider;
use drbot_openai::OpenAIProvider;
use drbot_providers::Provider;
use drbot_sessions::{SessionStore, SqliteSessionStore};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// A connected OpenClaw gateway client (OpenClaw v3 compatibility endpoint).
#[derive(Clone)]
pub struct OpenclawClient {
    pub conn_id: String,
    pub peer: SocketAddr,
    pub client_id: String,
    pub client_mode: String,
    pub client_version: String,
    pub platform: String,
    pub display_name: Option<String>,
    pub instance_id: Option<String>,
    pub device_id: Option<String>,
    pub role: String,
    pub scopes: Vec<String>,
    pub caps: Vec<String>,
    pub commands: Vec<String>,
    pub permissions: HashMap<String, bool>,
    pub path_env: Option<String>,
    pub connected_at_ms: u64,
    pub tx: mpsc::Sender<String>,
    pub event_seq: Arc<AtomicU64>,
}

/// Shared gateway state.
#[derive(Clone)]
pub struct GatewayState {
    inner: Arc<GatewayStateInner>,
}

struct GatewayStateInner {
    /// Configuration.
    config: Config,
    /// Connected clients.
    clients: RwLock<HashMap<Uuid, Client>>,
    /// Connected OpenClaw v3 clients (tracked separately from legacy clients).
    openclaw_clients: RwLock<HashMap<String, OpenclawClient>>,
    /// State version counters for OpenClaw `presence` and `health`.
    ///
    /// OpenClaw uses these to detect changes without diffing full payloads.
    openclaw_presence_version: AtomicU64,
    openclaw_health_version: AtomicU64,
    /// Heartbeat controls for OpenClaw compatibility.
    openclaw_heartbeats_enabled: AtomicBool,
    openclaw_last_heartbeat: RwLock<Option<serde_json::Value>>,
    /// Tracks in-flight work on OpenClaw's "main lane" (used to gate heartbeats).
    openclaw_main_inflight: AtomicU64,
    /// Server start time.
    start_time: Instant,
    /// AI provider.
    provider: Option<Arc<dyn Provider>>,
    /// Session store.
    session_store: Option<Arc<dyn SessionStore>>,
    /// Outbound channels runtime (best-effort; used by OpenClaw send/poll).
    channel_manager: Arc<ChannelManager>,
}

impl GatewayState {
    /// Create a new gateway state.
    pub fn new(config: Config) -> Self {
        let provider = init_provider(&config);
        let channel_manager = Arc::new(ChannelManager::new(&config));

        // Initialize session store
        let session_store: Option<Arc<dyn SessionStore>> =
            SqliteSessionStore::new(&config.storage.database_path)
                .ok()
                .map(|store| Arc::new(store) as Arc<dyn SessionStore>);

        Self {
            inner: Arc::new(GatewayStateInner {
                config,
                clients: RwLock::new(HashMap::new()),
                openclaw_clients: RwLock::new(HashMap::new()),
                openclaw_presence_version: AtomicU64::new(0),
                openclaw_health_version: AtomicU64::new(0),
                openclaw_heartbeats_enabled: AtomicBool::new(false),
                openclaw_last_heartbeat: RwLock::new(None),
                openclaw_main_inflight: AtomicU64::new(0),
                start_time: Instant::now(),
                provider,
                session_store,
                channel_manager,
            }),
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    /// Get the AI provider.
    pub fn provider(&self) -> Option<&Arc<dyn Provider>> {
        self.inner.provider.as_ref()
    }

    /// Get the session store.
    pub fn session_store(&self) -> Option<&Arc<dyn SessionStore>> {
        self.inner.session_store.as_ref()
    }

    pub fn channel_manager(&self) -> &ChannelManager {
        self.inner.channel_manager.as_ref()
    }

    /// Register a new client.
    pub async fn register_client(&self, client: Client) {
        let mut clients = self.inner.clients.write().await;
        clients.insert(client.id(), client);
    }

    /// Unregister a client.
    pub async fn unregister_client(&self, client_id: Uuid) {
        let mut clients = self.inner.clients.write().await;
        clients.remove(&client_id);
    }

    /// Get a client by ID.
    pub async fn get_client(&self, client_id: Uuid) -> Option<Client> {
        let clients = self.inner.clients.read().await;
        clients.get(&client_id).cloned()
    }

    /// Get the number of connected clients.
    pub async fn client_count(&self) -> usize {
        let clients = self.inner.clients.read().await;
        clients.len()
    }

    /// Register a connected OpenClaw v3 client.
    pub async fn register_openclaw_client(&self, client: OpenclawClient) {
        let mut clients = self.inner.openclaw_clients.write().await;
        clients.insert(client.conn_id.clone(), client);
    }

    /// Unregister a connected OpenClaw v3 client.
    pub async fn unregister_openclaw_client(&self, conn_id: &str) {
        let mut clients = self.inner.openclaw_clients.write().await;
        clients.remove(conn_id);
    }

    /// List connected OpenClaw v3 clients.
    pub async fn list_openclaw_clients(&self) -> Vec<OpenclawClient> {
        let clients = self.inner.openclaw_clients.read().await;
        clients.values().cloned().collect()
    }

    /// Get the current OpenClaw presence state version.
    pub fn openclaw_presence_version(&self) -> u64 {
        self.inner.openclaw_presence_version.load(Ordering::Relaxed)
    }

    /// Increment and return the OpenClaw presence state version.
    pub fn increment_openclaw_presence_version(&self) -> u64 {
        self.inner
            .openclaw_presence_version
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1)
    }

    /// Get the current OpenClaw health state version.
    pub fn openclaw_health_version(&self) -> u64 {
        self.inner.openclaw_health_version.load(Ordering::Relaxed)
    }

    /// Increment and return the OpenClaw health state version.
    pub fn increment_openclaw_health_version(&self) -> u64 {
        self.inner
            .openclaw_health_version
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1)
    }

    /// Whether OpenClaw heartbeats are enabled.
    pub fn openclaw_heartbeats_enabled(&self) -> bool {
        self.inner
            .openclaw_heartbeats_enabled
            .load(Ordering::Relaxed)
    }

    /// Enable or disable OpenClaw heartbeats.
    pub fn set_openclaw_heartbeats_enabled(&self, enabled: bool) {
        self.inner
            .openclaw_heartbeats_enabled
            .store(enabled, Ordering::Relaxed);
    }

    /// Get the last emitted OpenClaw heartbeat payload (if any).
    pub async fn openclaw_last_heartbeat(&self) -> Option<serde_json::Value> {
        self.inner.openclaw_last_heartbeat.read().await.clone()
    }

    /// Record the last OpenClaw heartbeat payload (and return it).
    pub async fn openclaw_set_last_heartbeat(
        &self,
        payload: serde_json::Value,
    ) -> serde_json::Value {
        *self.inner.openclaw_last_heartbeat.write().await = Some(payload.clone());
        payload
    }

    /// Count of in-flight operations on OpenClaw's main lane (best-effort).
    pub fn openclaw_main_inflight(&self) -> u64 {
        self.inner.openclaw_main_inflight.load(Ordering::Relaxed)
    }

    pub fn openclaw_main_lane_enter(&self) {
        self.inner
            .openclaw_main_inflight
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn openclaw_main_lane_exit(&self) {
        let mut cur = self.inner.openclaw_main_inflight.load(Ordering::Relaxed);
        loop {
            if cur == 0 {
                break;
            }
            match self.inner.openclaw_main_inflight.compare_exchange(
                cur,
                cur.saturating_sub(1),
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(next) => cur = next,
            }
        }
    }

    /// Get server uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.inner.start_time.elapsed().as_secs()
    }

    /// Check if authentication is required.
    pub fn auth_required(&self) -> bool {
        self.inner.config.gateway.auth_token.is_some()
    }

    /// Validate an authentication token.
    pub fn validate_token(&self, token: &str) -> bool {
        match &self.inner.config.gateway.auth_token {
            Some(expected) => token == expected,
            None => true, // No auth required
        }
    }

    /// Check if the provider is configured.
    pub fn has_provider(&self) -> bool {
        self.inner.provider.is_some()
    }

    /// Check if the session store is configured.
    pub fn has_session_store(&self) -> bool {
        self.inner.session_store.is_some()
    }
}

fn init_provider(config: &Config) -> Option<Arc<dyn Provider>> {
    let selected = config
        .providers
        .default_provider
        .clone()
        .unwrap_or_else(|| "auto".to_string());

    match selected.as_str() {
        "auto" => init_provider_auto(config),
        "anthropic" | "claude" => init_provider_anthropic(config),
        "openai" | "gpt" => init_provider_openai(config),
        "ollama" | "local" => init_provider_ollama(config),
        other => {
            tracing::warn!(provider = %other, "Unknown provider; falling back to auto");
            init_provider_auto(config)
        }
    }
}

fn init_provider_auto(config: &Config) -> Option<Arc<dyn Provider>> {
    init_provider_anthropic(config)
        .or_else(|| init_provider_openai(config))
        .or_else(|| init_provider_ollama(config))
}

fn init_provider_anthropic(config: &Config) -> Option<Arc<dyn Provider>> {
    config.providers.anthropic.as_ref().map(|cfg| {
        let mut provider = AnthropicProvider::new(&cfg.api_key);
        if let Some(base_url) = &cfg.base_url {
            provider = provider.with_base_url(base_url);
        }
        let model = cfg
            .default_model
            .clone()
            .or_else(|| config.providers.default_model.clone());
        if let Some(model) = model {
            provider = provider.with_default_model(model);
        }
        if let Some(max_tokens) = cfg.max_tokens {
            provider = provider.with_default_max_tokens(max_tokens);
        }
        tracing::info!(provider = "anthropic", "Initialized provider");
        Arc::new(provider) as Arc<dyn Provider>
    })
}

fn init_provider_openai(config: &Config) -> Option<Arc<dyn Provider>> {
    config.providers.openai.as_ref().map(|cfg| {
        let mut provider = OpenAIProvider::new(&cfg.api_key);
        if let Some(base_url) = &cfg.base_url {
            provider = provider.with_base_url(base_url);
        }
        let model = cfg
            .default_model
            .clone()
            .or_else(|| config.providers.default_model.clone());
        if let Some(model) = model {
            provider = provider.with_default_model(model);
        }
        tracing::info!(provider = "openai", "Initialized provider");
        Arc::new(provider) as Arc<dyn Provider>
    })
}

fn init_provider_ollama(config: &Config) -> Option<Arc<dyn Provider>> {
    config.providers.ollama.as_ref().map(|cfg| {
        let mut provider = OllamaProvider::new().with_base_url(&cfg.url);
        let model = cfg
            .default_model
            .clone()
            .or_else(|| config.providers.default_model.clone());
        if let Some(model) = model {
            provider = provider.with_default_model(model);
        }
        tracing::info!(provider = "ollama", base_url = %cfg.url, "Initialized provider");
        Arc::new(provider) as Arc<dyn Provider>
    })
}
