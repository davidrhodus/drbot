//! Gateway state management.

use crate::channel_manager::ChannelManager;
use crate::client::Client;
use crate::openclaw_logs::OpenclawLogBuffer;
use crate::openclaw_polls::OpenclawPollStore;
use crate::openclaw_system::{
    OpenclawSystemEvents, OpenclawSystemPresence, SystemEventEntry, SystemPresence,
    SystemPresencePayload, SystemPresenceUpdate,
};
use crate::openclaw_web_login::OpenclawWebLoginStore;
use drbot_anthropic::AnthropicProvider;
use drbot_core::Config;
use drbot_memory::MemoryStore;
use drbot_ollama::OllamaProvider;
use drbot_openai::OpenAIProvider;
use drbot_providers::{CliProvider, Provider};
use drbot_sessions::{SessionStore, SqliteSessionStore};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Outbound messages for the OpenClaw-compatible websocket connection.
///
/// We keep this enum independent of axum's websocket message type so the
/// gateway state doesn't depend on the HTTP stack.
#[derive(Debug, Clone)]
pub enum OpenclawOutbound {
    Text(String),
    Close { code: u16, reason: String },
}

/// A connected OpenClaw gateway client (OpenClaw v3 compatibility endpoint).
#[derive(Clone)]
pub struct OpenclawClient {
    pub conn_id: String,
    pub peer: SocketAddr,
    pub client_id: String,
    pub client_mode: String,
    pub client_version: String,
    pub platform: String,
    pub device_family: Option<String>,
    pub model_identifier: Option<String>,
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
    pub tx: mpsc::UnboundedSender<OpenclawOutbound>,
    /// Approximate per-connection send buffer accounting (OpenClaw parity with
    /// `ws.bufferedAmount` checks).
    pub queued_bytes: Arc<AtomicU64>,
    /// Set to true when the gateway has initiated a close for this connection.
    pub closing: Arc<AtomicBool>,
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
    provider: StdRwLock<Option<Arc<dyn Provider>>>,
    /// Session store.
    session_store: Option<Arc<dyn SessionStore>>,
    /// Outbound channels runtime (best-effort; used by OpenClaw send/poll).
    channel_manager: Arc<ChannelManager>,
    /// OpenClaw "system events" queue (ephemeral, session-scoped).
    openclaw_system_events: Arc<OpenclawSystemEvents>,
    /// OpenClaw "system presence" table (ephemeral, TTL-pruned).
    openclaw_system_presence: Arc<OpenclawSystemPresence>,
    /// OpenClaw poll tracking store (best-effort).
    openclaw_polls: Arc<OpenclawPollStore>,
    /// OpenClaw `web.login.*` runtime (QR codes + connection status).
    openclaw_web_login: Arc<OpenclawWebLoginStore>,
    /// OpenClaw `logs.tail` buffer (best-effort).
    openclaw_logs: Arc<OpenclawLogBuffer>,
    /// Tracks which inbound channel listeners have been started (OpenClaw interop).
    openclaw_inbound_started: Mutex<std::collections::HashSet<String>>,
    /// In-flight OpenClaw `chat.send` runs (global registry for cross-connection abort).
    openclaw_chat_runs: Mutex<HashMap<String, OpenclawChatRun>>,
    /// Semantic memory store for OpenClaw runs (best-effort).
    openclaw_memory_store: Option<Arc<MemoryStore>>,
}

#[derive(Clone)]
pub struct OpenclawChatRun {
    pub session_key: String,
    pub run_id: String,
    /// When set to `Some(reason)`, the run should abort and emit a `chat` event with
    /// `state=aborted` and `stopReason=reason`.
    pub cancel_tx: watch::Sender<Option<String>>,
    pub started_at_ms: u64,
}

impl GatewayState {
    /// Create a new gateway state.
    pub fn new(config: Config) -> Self {
        let provider = init_provider(&config);
        let openclaw_web_login = Arc::new(OpenclawWebLoginStore::new());
        let channel_manager = Arc::new(ChannelManager::new(&config, openclaw_web_login.clone()));
        let openclaw_system_events = Arc::new(OpenclawSystemEvents::new());
        let openclaw_system_presence = Arc::new(OpenclawSystemPresence::new());
        let openclaw_polls = Arc::new(OpenclawPollStore::new());
        let openclaw_logs = Arc::new(OpenclawLogBuffer::new());
        let openclaw_memory_store = init_openclaw_memory_store(&config);

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
                provider: StdRwLock::new(provider),
                session_store,
                channel_manager,
                openclaw_system_events,
                openclaw_system_presence,
                openclaw_polls,
                openclaw_web_login,
                openclaw_logs,
                openclaw_inbound_started: Mutex::new(std::collections::HashSet::new()),
                openclaw_chat_runs: Mutex::new(HashMap::new()),
                openclaw_memory_store,
            }),
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    /// Get the AI provider.
    pub fn provider(&self) -> Option<Arc<dyn Provider>> {
        self.inner
            .provider
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_provider(&self, provider: Option<Arc<dyn Provider>>) {
        *self
            .inner
            .provider
            .write()
            .unwrap_or_else(|e| e.into_inner()) = provider;
    }

    /// Get the session store.
    pub fn session_store(&self) -> Option<&Arc<dyn SessionStore>> {
        self.inner.session_store.as_ref()
    }

    pub fn channel_manager(&self) -> &ChannelManager {
        self.inner.channel_manager.as_ref()
    }

    pub fn openclaw_polls(&self) -> &OpenclawPollStore {
        self.inner.openclaw_polls.as_ref()
    }

    pub fn openclaw_web_login(&self) -> &OpenclawWebLoginStore {
        self.inner.openclaw_web_login.as_ref()
    }

    pub fn openclaw_logs(&self) -> &OpenclawLogBuffer {
        self.inner.openclaw_logs.as_ref()
    }

    pub fn openclaw_memory_store(&self) -> Option<&Arc<MemoryStore>> {
        self.inner.openclaw_memory_store.as_ref()
    }

    /// Register a channel listener as started for the inbound bridge.
    ///
    /// Returns `true` if this call should start the listener (i.e. it wasn't already started).
    pub async fn openclaw_try_start_inbound_channel(&self, channel_type: &str) -> bool {
        let channel_type = channel_type.trim();
        if channel_type.is_empty() {
            return false;
        }
        let mut started = self.inner.openclaw_inbound_started.lock().await;
        if started.contains(channel_type) {
            false
        } else {
            started.insert(channel_type.to_string());
            true
        }
    }

    // ---------------------------------------------------------------------
    // OpenClaw chat run registry (for global idempotency + abort)
    // ---------------------------------------------------------------------

    pub async fn openclaw_register_chat_run(&self, key: &str, entry: OpenclawChatRun) {
        let key = key.trim();
        if key.is_empty() {
            return;
        }
        let mut runs = self.inner.openclaw_chat_runs.lock().await;
        // Preserve the first entry; concurrent leaders should not happen, but avoid
        // clobbering a live cancel handle if cleanup was delayed.
        runs.entry(key.to_string()).or_insert(entry);
    }

    /// Attempt to register a chat run if it does not already exist.
    ///
    /// Returns `true` if the run was inserted (caller is the leader) or `false` if a run
    /// with the same key is already active.
    pub async fn openclaw_try_register_chat_run(&self, key: &str, entry: OpenclawChatRun) -> bool {
        let key = key.trim();
        if key.is_empty() {
            return false;
        }
        let mut runs = self.inner.openclaw_chat_runs.lock().await;
        if runs.contains_key(key) {
            return false;
        }
        runs.insert(key.to_string(), entry);
        true
    }

    pub async fn openclaw_has_chat_run(&self, key: &str) -> bool {
        let key = key.trim();
        if key.is_empty() {
            return false;
        }
        let runs = self.inner.openclaw_chat_runs.lock().await;
        runs.contains_key(key)
    }

    pub async fn openclaw_unregister_chat_run(&self, key: &str) {
        let key = key.trim();
        if key.is_empty() {
            return;
        }
        let mut runs = self.inner.openclaw_chat_runs.lock().await;
        runs.remove(key);
    }

    pub async fn openclaw_abort_chat_runs(
        &self,
        session_key: &str,
        run_id: Option<&str>,
        stop_reason: &str,
    ) -> Vec<String> {
        let session_key = session_key.trim();
        if session_key.is_empty() {
            return Vec::new();
        }
        let run_id = run_id.and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        let stop_reason = stop_reason.trim();
        let stop_reason = if stop_reason.is_empty() {
            "rpc"
        } else {
            stop_reason
        };

        let mut aborted = Vec::new();
        let runs = self.inner.openclaw_chat_runs.lock().await;
        for entry in runs.values() {
            if entry.session_key != session_key {
                continue;
            }
            if let Some(target) = run_id.as_deref() {
                if entry.run_id != target {
                    continue;
                }
            }
            let _ = entry.cancel_tx.send(Some(stop_reason.to_string()));
            aborted.push(entry.run_id.clone());
        }
        aborted.sort();
        aborted.dedup();
        aborted
    }

    pub async fn openclaw_find_chat_run_session_key(&self, run_id: &str) -> Option<String> {
        let run_id = run_id.trim();
        if run_id.is_empty() {
            return None;
        }
        let runs = self.inner.openclaw_chat_runs.lock().await;
        runs.values()
            .find(|entry| entry.run_id == run_id)
            .map(|entry| entry.session_key.clone())
    }

    // ---------------------------------------------------------------------
    // OpenClaw system events + presence
    // ---------------------------------------------------------------------

    pub async fn openclaw_is_system_event_context_changed(
        &self,
        session_key: &str,
        context_key: Option<&str>,
    ) -> bool {
        self.inner
            .openclaw_system_events
            .is_context_changed(session_key, context_key)
            .await
    }

    pub async fn openclaw_enqueue_system_event(
        &self,
        session_key: &str,
        text: &str,
        context_key: Option<&str>,
    ) {
        self.inner
            .openclaw_system_events
            .enqueue(session_key, text, context_key)
            .await;
    }

    pub async fn openclaw_peek_system_events(&self, session_key: &str) -> Vec<SystemEventEntry> {
        self.inner.openclaw_system_events.peek(session_key).await
    }

    pub async fn openclaw_has_system_events(&self, session_key: &str) -> bool {
        self.inner
            .openclaw_system_events
            .has_events(session_key)
            .await
    }

    pub async fn openclaw_drain_system_event_entries(
        &self,
        session_key: &str,
    ) -> Vec<SystemEventEntry> {
        self.inner.openclaw_system_events.drain(session_key).await
    }

    pub async fn openclaw_update_system_presence(
        &self,
        payload: SystemPresencePayload,
    ) -> SystemPresenceUpdate {
        self.inner.openclaw_system_presence.update(payload).await
    }

    pub async fn openclaw_list_system_presence(&self) -> Vec<SystemPresence> {
        self.inner.openclaw_system_presence.list().await
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

    /// Get a connected OpenClaw v3 client by conn_id.
    pub async fn get_openclaw_client(&self, conn_id: &str) -> Option<OpenclawClient> {
        let conn_id = conn_id.trim();
        if conn_id.is_empty() {
            return None;
        }
        let clients = self.inner.openclaw_clients.read().await;
        clients.get(conn_id).cloned()
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
        self.inner
            .config
            .gateway
            .auth_token
            .as_deref()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .is_some()
    }

    /// Validate an authentication token.
    pub fn validate_token(&self, token: &str) -> bool {
        let expected = self
            .inner
            .config
            .gateway
            .auth_token
            .as_deref()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty());
        let Some(expected) = expected else {
            return true; // No auth required
        };

        let provided = token.trim();
        if provided.is_empty() || provided.len() != expected.len() {
            return false;
        }

        // Avoid `==` so we don't leak early-exit timing on mismatches.
        let mut diff: u8 = 0;
        for (a, b) in provided.as_bytes().iter().zip(expected.as_bytes().iter()) {
            diff |= a ^ b;
        }
        diff == 0
    }
    /// Validate an inbound webhook token (hooks.token).
    pub fn validate_hooks_token(&self, token: &str) -> bool {
        let expected = self
            .inner
            .config
            .hooks
            .token
            .as_deref()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty());
        let Some(expected) = expected else {
            return false;
        };

        let provided = token.trim();
        if provided.is_empty() || provided.len() != expected.len() {
            return false;
        }

        // Avoid `==` so we don't leak early-exit timing on mismatches.
        let mut diff: u8 = 0;
        for (a, b) in provided.as_bytes().iter().zip(expected.as_bytes().iter()) {
            diff |= a ^ b;
        }
        diff == 0
    }

    /// Check if the provider is configured.
    pub fn has_provider(&self) -> bool {
        self.inner
            .provider
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Check if the session store is configured.
    pub fn has_session_store(&self) -> bool {
        self.inner.session_store.is_some()
    }
}

fn init_openclaw_memory_store(config: &Config) -> Option<Arc<MemoryStore>> {
    fn truthy(raw: &str) -> bool {
        matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    }

    let enabled = std::env::var("DRBOT_OPENCLAW_MEMORY_ENABLED")
        .ok()
        .as_deref()
        .map(truthy)
        .unwrap_or(true);
    if !enabled {
        return None;
    }

    let Some(dir) = crate::openclaw_paths::resolve_openclaw_state_dir(config) else {
        return None;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return None;
    }

    let path: PathBuf = dir.join("openclaw-memory.sqlite");
    match MemoryStore::new(&path) {
        Ok(store) => Some(Arc::new(store)),
        Err(err) => {
            tracing::warn!(error = %err, path = %path.to_string_lossy(), "failed to open OpenClaw memory store");
            None
        }
    }
}

pub(crate) fn init_provider(config: &Config) -> Option<Arc<dyn Provider>> {
    let selected = config
        .providers
        .default_provider
        .clone()
        .unwrap_or_else(|| "auto".to_string());

    try_init_provider_named(config, &selected)
        .ok()
        .or_else(|| init_provider_auto(config))
}

fn env_nonempty(var: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn env_flag_enabled(var: &str) -> bool {
    match std::env::var(var) {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

fn parse_env_placeholder(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let inner = trimmed.strip_prefix("${")?.strip_suffix("}")?;
    let name = inner.trim();
    if name.is_empty() {
        return None;
    }
    Some(name)
}

fn resolve_api_key(raw: &str, primary_env: Option<&str>) -> Option<String> {
    if let Some(primary_env) = primary_env {
        if let Some(v) = env_nonempty(primary_env) {
            return Some(v);
        }
    }

    if let Some(env_name) = parse_env_placeholder(raw) {
        return env_nonempty(env_name);
    }

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn try_init_provider_named(
    config: &Config,
    provider_name: &str,
) -> std::result::Result<Arc<dyn Provider>, String> {
    let name = provider_name.trim();
    if name.is_empty() {
        return Err("provider name is empty".to_string());
    }

    let provider = match name {
        "auto" => init_provider_auto(config)
            .ok_or_else(|| "no providers available (try drbot wizard)".to_string())?,
        "anthropic" | "claude" => init_provider_anthropic(config)
            .ok_or_else(|| "anthropic not configured (missing API key)".to_string())?,
        "openai" | "gpt" => init_provider_openai(config)
            .ok_or_else(|| "openai not configured (missing API key)".to_string())?,
        "ollama" | "local" => {
            init_provider_ollama(config).ok_or_else(|| "ollama not configured".to_string())?
        }
        "claude-cli" | "claude-code" => init_provider_cli_preset("claude-cli")
            .ok_or_else(|| CliProvider::claude_cli().not_found_hint())?,
        "codex-cli" | "codex" => init_provider_cli_preset("codex-cli")
            .ok_or_else(|| CliProvider::codex_cli().not_found_hint())?,
        "codex-oss" | "codex-local" => init_provider_cli_preset("codex-oss")
            .ok_or_else(|| CliProvider::codex_oss_ollama().not_found_hint())?,
        other => {
            if let Some(provider) = init_provider_openai_compatible(config, other) {
                provider
            } else if let Some(provider) = init_provider_cli_custom(config, other) {
                provider
            } else {
                return Err(format!("unknown provider: {}", other));
            }
        }
    };

    Ok(provider)
}

fn init_provider_auto(config: &Config) -> Option<Arc<dyn Provider>> {
    // Auto selection prefers cost-savers first:
    // - CLI tools (claude-cli / codex-cli) if installed on PATH
    // - Ollama if configured
    // - API providers as fallback
    //
    // You can disable CLI auto-detect with: DRBOT_AUTO_DISABLE_CLI_PRESETS=1
    let cli_presets_disabled = env_flag_enabled("DRBOT_AUTO_DISABLE_CLI_PRESETS");

    let cli = if cli_presets_disabled {
        None
    } else {
        init_provider_cli_preset("claude-cli").or_else(|| init_provider_cli_preset("codex-cli"))
    };

    cli.or_else(|| init_provider_ollama(config))
        .or_else(|| init_provider_anthropic(config))
        .or_else(|| init_provider_openai(config))
        .or_else(|| {
            config
                .providers
                .openai_compatible
                .iter()
                .find_map(|c| init_provider_openai_compatible(config, &c.name))
        })
        .or_else(|| {
            config
                .providers
                .cli
                .iter()
                .find_map(|c| init_provider_cli_custom(config, &c.name))
        })
}

fn init_provider_anthropic(config: &Config) -> Option<Arc<dyn Provider>> {
    config.providers.anthropic.as_ref().and_then(|cfg| {
        let api_key = resolve_api_key(&cfg.api_key, Some("ANTHROPIC_API_KEY"))?;
        let mut provider = AnthropicProvider::new(&api_key);
        if let Some(base_url) = &cfg.base_url {
            provider = provider.with_base_url(base_url);
        }
        if !cfg.headers.is_empty() {
            provider = provider.with_extra_headers(cfg.headers.clone());
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
        Some(Arc::new(provider) as Arc<dyn Provider>)
    })
}

fn init_provider_openai(config: &Config) -> Option<Arc<dyn Provider>> {
    config.providers.openai.as_ref().and_then(|cfg| {
        let api_key = resolve_api_key(&cfg.api_key, Some("OPENAI_API_KEY"))?;
        let mut provider = OpenAIProvider::new(&api_key);
        if let Some(base_url) = &cfg.base_url {
            provider = provider.with_base_url(base_url);
        }
        if !cfg.headers.is_empty() {
            provider = provider.with_extra_headers(cfg.headers.clone());
        }
        let model = cfg
            .default_model
            .clone()
            .or_else(|| config.providers.default_model.clone());
        if let Some(model) = model {
            provider = provider.with_default_model(model);
        }
        tracing::info!(provider = "openai", "Initialized provider");
        Some(Arc::new(provider) as Arc<dyn Provider>)
    })
}

fn init_provider_openai_compatible(config: &Config, name: &str) -> Option<Arc<dyn Provider>> {
    config
        .providers
        .openai_compatible
        .iter()
        .find(|c| c.name == name)
        .and_then(|cfg| {
            let api_key = resolve_api_key(&cfg.api_key, None)?;
            let mut provider = OpenAIProvider::new(&api_key)
                .with_provider_name(name.to_string())
                .with_base_url(cfg.base_url.clone());
            let model = cfg
                .default_model
                .clone()
                .or_else(|| config.providers.default_model.clone());
            if let Some(model) = model {
                provider = provider.with_default_model(model);
            }
            if !cfg.headers.is_empty() {
                provider = provider.with_extra_headers(cfg.headers.clone());
            }
            tracing::info!(
                provider = %name,
                base_url = %cfg.base_url,
                "Initialized OpenAI-compatible provider"
            );
            Some(Arc::new(provider) as Arc<dyn Provider>)
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

fn init_provider_cli_preset(name: &str) -> Option<Arc<dyn Provider>> {
    let provider = match name {
        "claude-cli" | "claude-code" => CliProvider::claude_cli(),
        "codex-cli" | "codex" => CliProvider::codex_cli(),
        "codex-oss" | "codex-local" => CliProvider::codex_oss_ollama(),
        _ => return None,
    };
    match provider.check_command_exists() {
        Ok(()) => {
            tracing::info!(provider = %name, "Initialized CLI provider");
            Some(Arc::new(provider))
        }
        Err(e) => {
            tracing::warn!(provider = %name, error = %e, "CLI provider not available");
            None
        }
    }
}

fn init_provider_cli_custom(config: &Config, name: &str) -> Option<Arc<dyn Provider>> {
    let cfg = config.providers.cli.iter().find(|c| c.name == name)?;
    let provider = CliProvider::from_config(cfg);
    if let Err(e) = provider.check_command_exists() {
        tracing::warn!(
            provider = %name,
            command = %cfg.command,
            error = %e,
            "Custom CLI provider not available"
        );
        return None;
    }
    tracing::info!(provider = %name, command = %cfg.command, "Initialized custom CLI provider");
    Some(Arc::new(provider) as Arc<dyn Provider>)
}
