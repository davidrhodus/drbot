//! TUI application state and logic.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use drbot_core::Result;
use drbot_protocol::{
    event::chat::{StreamCompleteEvent, StreamDeltaEvent, StreamErrorEvent, StreamStartEvent},
    event::provider::ChangedEvent,
    event_types, ChatCancelParams, ChatOptions as GatewayChatOptions, ChatSendParams,
    ProviderModelsParams, ProviderModelsResult, ProviderSelectParams, Request, Response,
    SessionUpdateParams,
};
use drbot_tool_mode::{
    bash_command_is_safe_for_auto_approve, build_agent_system_prompt_with_policy,
    execute_tool_call, extract_tool_calls, resolve_tool_root_with_allowlist,
    should_reprompt_for_tool_calls, BashAutoApprovePolicy, ToolCallSpec, ToolModeConfig,
};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::gateway_client::GatewayClient;
use crate::openclaw_client::OpenclawClient;

#[derive(Debug, Clone)]
pub enum Overlay {
    ProviderPicker(ProviderPicker),
    ModelPicker(ModelPicker),
    SessionPicker(SessionPicker),
    FirstRun(FirstRunOverlay),
    Openclaw(OpenclawOverlay),
    Skills(SkillsOverlay),
    SkillsAdd(SkillsAddOverlay),
    ToolApproval(ToolApprovalOverlay),
}

#[derive(Debug, Clone)]
pub struct ProviderPicker {
    pub providers: Vec<drbot_protocol::ProviderInfo>,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct ModelPicker {
    pub models: Vec<drbot_protocol::ModelInfo>,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct SessionPicker {
    pub sessions: Vec<drbot_protocol::SessionInfo>,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct FirstRunOverlay {
    pub providers: Vec<drbot_protocol::ProviderInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenclawTab {
    Overview,
    Logs,
    Events,
}

#[derive(Debug, Clone)]
pub struct OpenclawOverlay {
    pub tab: OpenclawTab,
    pub scroll: usize,
}

#[derive(Debug, Clone)]
pub struct SkillsOverlay {
    pub(crate) skills: Vec<OpenclawSkillStatusEntry>,
    pub selected: usize,
    pub workspace_dir: String,
    pub managed_dir: String,
    pub snapshot_version: u64,
}

#[derive(Debug, Clone)]
pub struct SkillsAddOverlay {
    pub input: String,
    pub cursor_pos: usize,
}

#[derive(Debug, Clone)]
pub struct ToolApprovalOverlay {
    pub call: ToolCallSpec,
    /// Why we're asking (e.g. "approval_required" or "unsafe_for_auto_approve").
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenclawSkillStatusReport {
    pub workspace_dir: String,
    pub managed_skills_dir: String,
    pub skills: Vec<OpenclawSkillStatusEntry>,
    pub snapshot_version: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenclawSkillStatusEntry {
    pub name: String,
    pub description: String,
    pub source: String,
    #[serde(rename = "skillKey")]
    pub skill_key: String,
    pub eligible: bool,
    pub disabled: bool,
    #[serde(rename = "blockedByAllowlist")]
    pub blocked_by_allowlist: bool,
    #[serde(default)]
    pub requirements: OpenclawSkillRequirements,
    #[serde(default)]
    pub missing: OpenclawSkillRequirements,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenclawSkillRequirements {
    #[serde(default)]
    pub bins: Vec<String>,
    #[serde(default)]
    pub any_bins: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub config: Vec<String>,
    #[serde(default)]
    pub os: Vec<String>,
}

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
    /// Gateway WebSocket URL for display (optional).
    pub gateway_url: Option<String>,
    /// Gateway auth token (optional; required when gateway auth is enabled).
    pub gateway_auth_token: Option<String>,
    /// Whether the gateway is running.
    pub gateway_running: bool,
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
            gateway_url: None,
            gateway_auth_token: None,
            gateway_running: false,
            max_history: 100,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TuiPrefs {
    agent_enabled: bool,
    auto_approve: bool,
    strict: bool,
}

impl Default for TuiPrefs {
    fn default() -> Self {
        Self {
            agent_enabled: false,
            auto_approve: false,
            strict: true,
        }
    }
}

impl TuiPrefs {
    fn path() -> Option<PathBuf> {
        drbot_core::Config::config_dir().map(|dir| dir.join("tui-prefs.json"))
    }

    fn write_atomic_best_effort(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let tmp = path.with_extension(format!(
            "tmp.{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")
        ));
        if std::fs::write(&tmp, content).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return;
        }

        if std::fs::rename(&tmp, path).is_err() {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::rename(&tmp, path);
        }
        let _ = std::fs::remove_file(&tmp);
    }

    fn load_best_effort() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    fn save_best_effort(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        let Ok(raw) = serde_json::to_string_pretty(self) else {
            return;
        };
        Self::write_atomic_best_effort(&path, &raw);
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct GatewayChatPrefs {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    last_session_by_project: BTreeMap<String, String>,
}

impl Default for GatewayChatPrefs {
    fn default() -> Self {
        Self {
            last_session_by_project: BTreeMap::new(),
        }
    }
}

impl GatewayChatPrefs {
    fn path() -> Option<PathBuf> {
        drbot_core::Config::config_dir().map(|dir| dir.join("gateway-chat-prefs.json"))
    }

    fn load_best_effort() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    fn save_best_effort(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        let Ok(raw) = serde_json::to_string_pretty(self) else {
            return;
        };
        TuiPrefs::write_atomic_best_effort(&path, &raw);
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

struct PendingToolJob {
    call: ToolCallSpec,
    handle: JoinHandle<(String, bool)>,
}

struct AgentRunState {
    user_text: String,
    next_message: String,
    rounds: usize,
    strict_remaining: usize,
    pending_calls: Vec<ToolCallSpec>,
    pending_call_index: usize,
    tool_updates: Vec<String>,
    pending_tool_job: Option<PendingToolJob>,
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
    /// Tool/agent mode config (local tools executed client-side).
    pub tool_cfg: ToolModeConfig,
    /// Strict tool mode: if the assistant responds with prose/commands but no tools, reprompt.
    pub tool_strict: bool,
    bash_policy: BashAutoApprovePolicy,
    trusted_bash_commands: BTreeSet<String>,
    agent_run: Option<AgentRunState>,
    /// Gateway client (WebSocket).
    gateway: Option<GatewayClient>,
    /// Gateway event receiver.
    gateway_rx: Option<mpsc::Receiver<drbot_protocol::Event>>,
    /// Gateway response receiver (unmatched responses, e.g. streaming chat.send completion/error).
    gateway_resp_rx: Option<mpsc::Receiver<Response>>,
    /// Current session ID.
    pub session_id: Option<uuid::Uuid>,
    /// Current model selection (sent to gateway).
    pub model: Option<String>,
    /// Last known active provider name (from provider.list/provider.select).
    pub active_provider: Option<String>,
    /// Last known active provider status string (e.g. "active", "active (unreachable)").
    pub active_provider_status: Option<String>,
    /// Active chat request id (for matching stream events).
    active_request_id: Option<uuid::Uuid>,
    /// Last gateway response (for debugging).
    last_response: Option<Response>,
    /// Modal overlay state.
    pub overlay: Option<Overlay>,
    /// Exit action when quitting the TUI.
    pub exit_action: crate::ExitAction,
    /// OpenClaw client (ws://.../openclaw/ws), connected on-demand.
    openclaw: Option<OpenclawClient>,
    /// OpenClaw event receiver.
    openclaw_rx: Option<mpsc::Receiver<drbot_protocol::openclaw::EventFrame>>,
    /// OpenClaw hello snapshot (from connect response).
    pub openclaw_hello: Option<drbot_protocol::openclaw::HelloOk>,
    /// Latest OpenClaw health payload (refreshed on-demand).
    pub openclaw_health: Option<serde_json::Value>,
    /// Latest OpenClaw logs.tail lines.
    pub openclaw_logs: Vec<String>,
    /// Cursor for logs.tail pagination.
    openclaw_logs_cursor: Option<u64>,
    /// Recent OpenClaw events (truncated).
    pub openclaw_events: VecDeque<String>,
}

impl App {
    /// Create a new app instance.
    pub async fn new(config: AppConfig) -> Result<Self> {
        let model = config.model.clone();
        let mut resumed_session = false;

        let assistant_cfg = drbot_core::Config::load().unwrap_or_default().assistant;
        let autonomy_mode = assistant_cfg.autonomy_mode;
        let tool_root = std::env::current_dir()
            .ok()
            .and_then(|d| {
                resolve_tool_root_with_allowlist(d, &assistant_cfg.workspace_allowlist)
                    .ok()
                    .map(|(root, used_default)| {
                        if used_default {
                            warn!(
                                root = %root.display(),
                                "tool root not in allowlist; using allowed workspace root"
                            );
                        }
                        root
                    })
            })
            .unwrap_or_else(|| PathBuf::from("."));

        let prefs = TuiPrefs::load_best_effort();

        let mut app = Self {
            config,
            messages: VecDeque::new(),
            input: String::new(),
            cursor_pos: 0,
            scroll_offset: 0,
            quit: false,
            is_loading: false,
            streaming_content: String::new(),
            status: "Starting...".to_string(),
            tool_cfg: ToolModeConfig {
                enabled: prefs.agent_enabled,
                auto_approve: prefs.auto_approve,
                root: tool_root,
                max_rounds: 10,
                autonomy_mode,
                tool_allowlist: assistant_cfg.tool_allowlist,
                tool_denylist: assistant_cfg.tool_denylist,
            },
            tool_strict: prefs.strict,
            bash_policy: BashAutoApprovePolicy::default(),
            trusted_bash_commands: BTreeSet::new(),
            agent_run: None,
            gateway: None,
            gateway_rx: None,
            gateway_resp_rx: None,
            session_id: None,
            model,
            active_provider: None,
            active_provider_status: None,
            active_request_id: None,
            last_response: None,
            overlay: None,
            exit_action: crate::ExitAction::Quit,
            openclaw: None,
            openclaw_rx: None,
            openclaw_hello: None,
            openclaw_health: None,
            openclaw_logs: Vec::new(),
            openclaw_logs_cursor: None,
            openclaw_events: VecDeque::new(),
        };

        // Auto-bootstrap a repo-scoped knowledge base (`.drbot/`) when launched inside a git repo.
        // This keeps project recall working out-of-the-box without requiring an explicit `drbot kb init`.
        let in_git_project =
            drbot_tool_mode::find_git_root_best_effort(&app.tool_cfg.root).is_some();
        if in_git_project && drbot_tool_mode::project_kb_auto_init_enabled() {
            let project_drbot_dir =
                drbot_tool_mode::resolve_project_drbot_dir_best_effort(&app.tool_cfg.root);
            drbot_tool_mode::ensure_project_kb_bootstrap_best_effort(&project_drbot_dir);
        }

        if let Some(url) = app.config.gateway_url.clone() {
            match GatewayClient::connect(&url).await {
                Ok((client, ev_rx, resp_rx)) => {
                    app.gateway = Some(client.clone());
                    app.gateway_rx = Some(ev_rx);
                    app.gateway_resp_rx = Some(resp_rx);
                    app.status = "Connected to gateway".to_string();

                    if let Some(token) = app
                        .config
                        .gateway_auth_token
                        .as_deref()
                        .map(|t| t.trim())
                        .filter(|t| !t.is_empty())
                    {
                        let resp = client
                            .request(
                                "auth.login",
                                drbot_protocol::AuthLoginParams {
                                    token: token.to_string(),
                                },
                            )
                            .await?;
                        app.last_response = Some(resp.clone());
                        if let Some(err) = resp.error {
                            app.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: format!("Gateway auth failed: {}", err.message),
                            });
                        }
                    }

                    // On startup, resume the most recently updated session (if any) so the TUI
                    // feels like a normal chat app.
                    let in_git_project =
                        drbot_tool_mode::find_git_root_best_effort(&app.tool_cfg.root).is_some();
                    resumed_session = app.try_resume_last_session_for_current_project().await;
                    if !resumed_session && !in_git_project {
                        resumed_session = app.try_resume_last_session().await;
                    }

                    // Prime provider state for header + pickers.
                    let _ = app.refresh_providers(false).await;
                    if app.active_provider.is_none() {
                        // If no provider is active yet, pick the best available provider (auto)
                        // so users can start chatting immediately.
                        let _ = app.try_auto_select_provider().await;
                        let _ = app.refresh_providers(false).await;
                    }
                    if app.active_provider.is_none() {
                        let _ = app.refresh_providers(true).await;
                    }
                }
                Err(e) => {
                    app.status = format!("Gateway connect failed: {}", e);
                    app.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: format!("Failed to connect to gateway: {}", e),
                    });
                }
            }
        } else {
            app.status = "No gateway URL configured".to_string();
        }

        // Add system message if configured (only when starting fresh; loaded sessions show their own).
        if !resumed_session {
            if let Some(prompt) = &app.config.system_prompt {
                app.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: prompt.clone(),
                });
            }
        }

        if app.tool_cfg.enabled {
            app.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "Agent/tools mode: ON (Ctrl+T or /agent off to disable).".to_string(),
            });
        }
        if app.tool_cfg.auto_approve {
            app.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "Auto-approve: ON (Ctrl+Y or /approve off to disable).".to_string(),
            });
        }

        app.messages.push_back(ChatMessage {
            role: MessageRole::System,
            content: "Type a message and press Enter. Esc to stop, Ctrl+C to quit. Ctrl+P provider, Ctrl+M model, Ctrl+O sessions, Ctrl+D OpenClaw, Ctrl+K skills, Ctrl+T agent/tools, Ctrl+Y auto-approve. (/help for commands, /wizard for setup)".to_string(),
        });

        Ok(app)
    }

    fn save_prefs_best_effort(&self) {
        let mut prefs = TuiPrefs::load_best_effort();
        prefs.agent_enabled = self.tool_cfg.enabled;
        prefs.auto_approve = self.tool_cfg.auto_approve;
        prefs.strict = self.tool_strict;
        prefs.save_best_effort();
    }

    fn session_map_key_best_effort(&self) -> String {
        let base = drbot_tool_mode::find_git_root_best_effort(&self.tool_cfg.root)
            .unwrap_or_else(|| self.tool_cfg.root.clone());
        let mut key = base.to_string_lossy().to_string();
        if self.tool_cfg.enabled {
            key.push_str("#agent");
        } else {
            key.push_str("#chat");
        }
        key
    }

    fn remember_last_session_for_current_project_best_effort(&self, session_id: uuid::Uuid) {
        let project_key = self.session_map_key_best_effort();
        let mut prefs = GatewayChatPrefs::load_best_effort();
        prefs.last_session_by_project
            .insert(project_key, session_id.to_string());
        prefs.save_best_effort();
    }

    fn clear_last_session_for_current_project_best_effort(&self) {
        let project_key = self.session_map_key_best_effort();
        let mut prefs = GatewayChatPrefs::load_best_effort();
        prefs.last_session_by_project.remove(&project_key);
        prefs.save_best_effort();
    }

    async fn try_resume_last_session_for_current_project(&mut self) -> bool {
        if self.gateway.is_none() {
            return false;
        }

        let project_key = self.session_map_key_best_effort();
        let prefs = GatewayChatPrefs::load_best_effort();
        let Some(raw) = prefs.last_session_by_project.get(&project_key) else {
            return false;
        };
        let Ok(session_id) = uuid::Uuid::parse_str(raw) else {
            return false;
        };

        let _ = self.load_session(session_id).await;
        self.session_id == Some(session_id)
    }

    async fn try_resume_last_session(&mut self) -> bool {
        let Some(gateway) = self.gateway.clone() else {
            return false;
        };

        let resp = gateway
            .request(
                "session.list",
                drbot_protocol::SessionListParams {
                    limit: Some(1),
                    offset: None,
                    state: Some("active".to_string()),
                },
            )
            .await;

        let resp = match resp {
            Ok(r) => r,
            Err(_) => return false,
        };

        self.last_response = Some(resp.clone());
        if resp.error.is_some() {
            return false;
        }

        let sessions = resp
            .result
            .and_then(|v| serde_json::from_value::<drbot_protocol::SessionListResult>(v).ok())
            .map(|r| r.sessions)
            .unwrap_or_default();

        let Some(first) = sessions.first() else {
            return false;
        };

        let _ = self.load_session(first.id).await;
        self.session_id == Some(first.id)
    }

    async fn gateway_chat_send_non_streaming_best_effort(
        &mut self,
        message: &str,
    ) -> Option<drbot_protocol::ChatSendResult> {
        let Some(gateway) = self.gateway.clone() else {
            return None;
        };

        let resp = gateway
            .request(
                "chat.send",
                ChatSendParams {
                    session_id: self.session_id,
                    message: message.to_string(),
                    model: self.model.clone(),
                    stream: false,
                    options: None,
                },
            )
            .await;

        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: format!("chat.send error: {}", e),
                });
                return None;
            }
        };

        self.last_response = Some(resp.clone());
        if let Some(err) = resp.error {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("chat.send error: {}", err.message),
            });
            return None;
        }

        let Some(result) = resp.result else {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "chat.send returned no result".to_string(),
            });
            return None;
        };

        match serde_json::from_value::<drbot_protocol::ChatSendResult>(result) {
            Ok(parsed) => Some(parsed),
            Err(e) => {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: format!("Failed to parse chat.send result: {}", e),
                });
                None
            }
        }
    }

    async fn try_auto_select_provider(&mut self) -> bool {
        let Some(gateway) = self.gateway.clone() else {
            return false;
        };

        let resp = gateway
            .request(
                "provider.select",
                ProviderSelectParams {
                    provider: "auto".to_string(),
                },
            )
            .await;

        let resp = match resp {
            Ok(r) => r,
            Err(_) => return false,
        };

        self.last_response = Some(resp.clone());
        if let Some(err) = resp.error {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("Auto provider selection failed: {}", err.message),
            });
            return false;
        }

        let name = resp
            .result
            .clone()
            .and_then(|v| serde_json::from_value::<drbot_protocol::ProviderSelectResult>(v).ok())
            .map(|r| r.provider.name)
            .unwrap_or_else(|| "auto".to_string());

        self.active_provider = Some(name.clone());
        self.active_provider_status = Some("active".to_string());
        // Avoid cross-provider model mismatches.
        self.model = None;
        self.status = format!("Provider selected: {}", name);
        self.messages.push_back(ChatMessage {
            role: MessageRole::System,
            content: format!("Provider auto-selected: {} (Ctrl+P to change).", name),
        });

        true
    }

    /// Check if the app should quit.
    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Handle a key event.
    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        // Always allow quit keys, even while loading.
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return Ok(());
        }
        if key.code == KeyCode::Esc {
            if self.overlay.is_some() {
                self.overlay = None;
            } else {
                self.stop_current_agent("user requested stop").await?;
            }
            return Ok(());
        }

        // Overlay gets first crack at input.
        if self.overlay.is_some() {
            return self.handle_overlay_key(key).await;
        }

        if self.is_loading {
            // Allow typing/scrolling while streaming or running tools, but don't allow
            // starting a new request.
            match key.code {
                KeyCode::Enter => {
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Busy (wait for the current response/tool run to finish)."
                            .to_string(),
                    });
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
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
                    if self.scroll_offset > 0 {
                        self.scroll_offset -= 1;
                    }
                }
                KeyCode::Down => {
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
            return Ok(());
        }

        match key.code {
            KeyCode::Enter => {
                self.submit_message().await?;
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let _ = self.refresh_providers(true).await;
            }
            KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let _ = self.open_model_picker().await;
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let _ = self.open_session_picker().await;
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let _ = self.open_openclaw_overlay().await;
            }
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let _ = self.open_skills_overlay().await;
            }
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.tool_cfg.enabled = !self.tool_cfg.enabled;
                if self.tool_cfg.enabled {
                    self.status = "Agent mode: on".to_string();
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Agent mode enabled. Read-only tools and safe bash auto-run; write tools may ask approval. (Ctrl+Y or /approve on to auto-approve.)".to_string(),
                    });
                } else {
                    self.agent_run = None;
                    if matches!(self.overlay, Some(Overlay::ToolApproval(_))) {
                        self.overlay = None;
                    }
                    self.status = "Agent mode: off".to_string();
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Agent mode disabled.".to_string(),
                    });
                }
                self.save_prefs_best_effort();
            }
            KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.tool_cfg.auto_approve = !self.tool_cfg.auto_approve;
                let label = if self.tool_cfg.auto_approve {
                    "on"
                } else {
                    "off"
                };
                self.status = format!("Auto-approve: {}", label);
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: format!("Auto-approve is now {}.", label),
                });
                self.save_prefs_best_effort();
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
            return self.handle_command(&text).await;
        }
        let text_trimmed = text.trim_start();
        let text_lower = text_trimmed.to_ascii_lowercase();
        if text_lower.starts_with("kb:") {
            let query = text_trimmed[3..].trim();
            let cmd = if query.is_empty() {
                "/kb".to_string()
            } else {
                format!("/kb {}", query)
            };
            return self.handle_command(&cmd).await;
        }
        if text_lower.starts_with("notes:") {
            let query = text_trimmed[6..].trim();
            let cmd = if query.is_empty() {
                "/notes".to_string()
            } else {
                format!("/notes {}", query)
            };
            return self.handle_command(&cmd).await;
        }
        if drbot_tool_mode::is_project_remember_command(text_trimmed)
            || drbot_tool_mode::is_project_forget_command(text_trimmed)
        {
            let cmd = format!("/{}", text_trimmed);
            return self.handle_command(&cmd).await;
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

        // Best-effort: auto-capture a few high-confidence stable facts into the project KB
        // so project recall works with minimal manual setup.
        let in_git_project =
            drbot_tool_mode::find_git_root_best_effort(&self.tool_cfg.root).is_some();
        if in_git_project {
            let project_drbot_dir =
                drbot_tool_mode::resolve_project_drbot_dir_best_effort(&self.tool_cfg.root);
            drbot_tool_mode::autosave_project_kb_best_effort(&project_drbot_dir, &text);
        }

        if self.tool_cfg.enabled {
            if self.agent_run.is_some() || self.is_loading {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: "Agent is busy. Please wait for the current run to finish."
                        .to_string(),
                });
                return Ok(());
            }

            self.agent_run = Some(AgentRunState {
                user_text: text.clone(),
                next_message: text,
                rounds: 0,
                strict_remaining: if self.tool_strict { 2 } else { 0 },
                pending_calls: Vec::new(),
                pending_call_index: 0,
                tool_updates: Vec::new(),
                pending_tool_job: None,
            });
            return self.agent_send_next_message().await;
        }

        self.send_chat_request(text).await
    }

    async fn send_chat_request(&mut self, message: String) -> Result<()> {
        let Some(gateway) = self.gateway.clone() else {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "Gateway not connected.".to_string(),
            });
            return Ok(());
        };

        self.is_loading = true;
        self.status = "Thinking...".to_string();
        self.streaming_content.clear();
        self.active_request_id = None;

        let system_prompt = if self.tool_cfg.enabled {
            Some(build_agent_system_prompt_with_policy(
                self.config.system_prompt.clone(),
                &self.tool_cfg.root,
                &self.tool_cfg.tool_allowlist,
                &self.tool_cfg.tool_denylist,
            ))
        } else {
            self.config.system_prompt.clone()
        };

        // Project-local KB (optional): `.drbot/memory/*.md` under the nearest git root (or the tool root).
        //
        // We inject this client-side so the gateway doesn't need to know the user's cwd.
        let msg_trimmed = message.trim_start();
        let msg_lower = msg_trimmed.to_ascii_lowercase();
        let is_internal_tool_message = msg_trimmed.starts_with("[Tool Result]")
            || msg_trimmed.starts_with("[Tool Denied]")
            || msg_trimmed.starts_with("[Tool Mode Strict]");
        let is_project_mem_cmd = drbot_tool_mode::is_project_remember_command(msg_trimmed)
            || drbot_tool_mode::is_project_forget_command(msg_trimmed);
        let is_local_cmd = msg_lower.starts_with("/remember")
            || msg_lower.starts_with("remember:")
            || msg_lower.starts_with("/forget")
            || msg_lower.starts_with("forget:")
            || is_project_mem_cmd
            || msg_lower == "/memory"
            || msg_lower == "/mem"
            || msg_lower == "/profile"
            || msg_lower == "/kb"
            || msg_lower.starts_with("/kb ")
            || msg_lower.starts_with("/kb:")
            || msg_lower.starts_with("kb:")
            || msg_lower == "/notes"
            || msg_lower.starts_with("/notes ")
            || msg_lower.starts_with("/notes:")
            || msg_lower.starts_with("notes:");

        let project_notes = if is_internal_tool_message || is_local_cmd {
            None
        } else {
            let project_drbot_dir =
                drbot_tool_mode::resolve_project_drbot_dir_best_effort(&self.tool_cfg.root);
            drbot_core::workspace_notes_recall::recall_project_notes_prompt(
                &project_drbot_dir,
                &message,
            )
            .await
        };

        let system_prompt = match (system_prompt, project_notes) {
            (Some(core), Some(notes)) => {
                Some(format!("{}\n\n---\n\n{}", core.trim(), notes.trim()))
            }
            (None, Some(notes)) => Some(notes),
            (core, None) => core,
        };

        let opts = GatewayChatOptions {
            max_tokens: Some(4096),
            temperature: if self.tool_cfg.enabled {
                Some(0.2)
            } else {
                None
            },
            system_prompt,
            ..Default::default()
        };

        let params = ChatSendParams {
            session_id: self.session_id,
            message,
            model: self.model.clone(),
            stream: true,
            options: Some(opts),
        };

        let req = Request::create("chat.send", params);
        self.active_request_id = Some(req.id);
        debug!(request_id = %req.id, "Sending chat.send");
        gateway.send_request(req).await?;
        Ok(())
    }

    async fn stop_current_agent(&mut self, reason: &str) -> Result<()> {
        let mut stopped = false;

        if let Some(run) = self.agent_run.as_mut() {
            if let Some(job) = run.pending_tool_job.take() {
                job.handle.abort();
                stopped = true;
            }
        }

        if let Some(request_id) = self.active_request_id {
            if let Some(gateway) = self.gateway.clone() {
                let req = Request::create("chat.cancel", ChatCancelParams { request_id });
                let _ = gateway.send_request(req).await;
            }
            self.active_request_id = None;
            stopped = true;
        }

        if self.agent_run.is_some() {
            self.agent_run = None;
            stopped = true;
        }

        if self.is_loading {
            self.is_loading = false;
            self.streaming_content.clear();
            stopped = true;
        }

        if stopped {
            self.status = "Ready".to_string();
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("Stopped: {}", reason),
            });
        }

        Ok(())
    }

    async fn agent_send_next_message(&mut self) -> Result<()> {
        let Some(run) = self.agent_run.as_mut() else {
            return Ok(());
        };

        run.rounds = run.rounds.saturating_add(1);
        if run.rounds > self.tool_cfg.max_rounds.max(1) {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!(
                    "Agent stopped: max rounds exceeded ({})",
                    self.tool_cfg.max_rounds.max(1)
                ),
            });
            self.agent_run = None;
            self.is_loading = false;
            self.active_request_id = None;
            self.status = "Ready".to_string();
            return Ok(());
        }

        self.status = format!(
            "Agent thinking... ({}/{})",
            run.rounds,
            self.tool_cfg.max_rounds.max(1)
        );
        let msg = run.next_message.clone();
        self.send_chat_request(msg).await
    }

    async fn agent_handle_assistant_complete(&mut self, assistant_text: String) -> Result<()> {
        if self.agent_run.is_none() {
            // Tool mode toggled off while awaiting; just show output.
            self.messages.push_back(ChatMessage {
                role: MessageRole::Assistant,
                content: assistant_text,
            });
            return Ok(());
        }

        let calls = extract_tool_calls(&assistant_text);
        if calls.is_empty() {
            let mut reprompt = false;
            if let Some(run) = self.agent_run.as_mut() {
                if run.strict_remaining > 0
                    && should_reprompt_for_tool_calls(&run.user_text, &assistant_text)
                {
                    run.strict_remaining = run.strict_remaining.saturating_sub(1);
                    run.next_message = "[Tool Mode Strict] Convert the previous response into tool calls. Reply ONLY with a `drbot_tool` code block containing JSON tool calls (object or array). No prose.".to_string();
                    reprompt = true;
                }
            }

            if reprompt {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content:
                        "Tool mode strict: assistant responded without tool calls; reprompting."
                            .to_string(),
                });
                return self.agent_send_next_message().await;
            }

            self.messages.push_back(ChatMessage {
                role: MessageRole::Assistant,
                content: assistant_text,
            });
            self.agent_run = None;
            self.status = "Ready".to_string();
            return Ok(());
        }

        // Hide the raw tool JSON and run tools locally.
        let tool_count = calls.len();
        {
            let Some(run) = self.agent_run.as_mut() else {
                return Ok(());
            };
            run.pending_calls = calls;
            run.pending_call_index = 0;
            run.tool_updates.clear();
            run.pending_tool_job = None;
        }

        self.messages.push_back(ChatMessage {
            role: MessageRole::System,
            content: format!("Agent requested {} tool(s).", tool_count),
        });

        self.agent_start_next_tool().await
    }

    fn format_tool_summary(call: &ToolCallSpec) -> String {
        match call.tool.as_str() {
            "bash" => {
                let command = call
                    .args
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let cwd = call.args.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
                if cwd.trim().is_empty() {
                    format!("bash\ncommand: {}", command)
                } else {
                    format!("bash\ncwd: {}\ncommand: {}", cwd.trim(), command)
                }
            }
            "read_file" => {
                let path = call.args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                format!("read_file\npath: {}", path)
            }
            "write_file" => {
                let path = call.args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let bytes = call
                    .args
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                format!("write_file\npath: {}\nbytes: {}", path, bytes)
            }
            "list_dir" | "list_directory" => {
                let path = call
                    .args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                format!("list_dir\npath: {}", path)
            }
            "search" => {
                let pattern = call
                    .args
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let path = call
                    .args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                format!("search\npattern: {}\npath: {}", pattern, path)
            }
            "apply_patch" => {
                let bytes = call
                    .args
                    .get("patch")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                format!("apply_patch\nbytes: {}", bytes)
            }
            other => format!("{}", other),
        }
    }

    fn agent_call_auto_approved(&self, call: &ToolCallSpec) -> (bool, String) {
        match call.tool.as_str() {
            "read_file" | "list_dir" | "list_directory" | "search" => {
                (true, "auto: read-only".to_string())
            }
            "bash" => {
                let command = call
                    .args
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let normalized = command.trim();
                if bash_command_is_safe_for_auto_approve(command, &self.bash_policy) {
                    (true, "auto: safe bash".to_string())
                } else if !normalized.is_empty() && self.trusted_bash_commands.contains(normalized)
                {
                    (true, "trusted bash".to_string())
                } else {
                    (false, "bash not safe to auto-run".to_string())
                }
            }
            _ => {
                if self.tool_cfg.auto_approve {
                    (true, "auto: enabled".to_string())
                } else {
                    (false, "approval required".to_string())
                }
            }
        }
    }

    fn agent_spawn_tool_job(&mut self, call: ToolCallSpec) {
        let cfg = self.tool_cfg.clone();
        let call_for_task = call.clone();
        let handle = tokio::spawn(async move {
            match execute_tool_call(&cfg, &call_for_task).await {
                Ok((out, err)) => (out, err),
                Err(e) => (format!("Error: {}", e), true),
            }
        });

        self.is_loading = true;
        self.status = format!("Running tool: {}", call.tool);
        if let Some(run) = self.agent_run.as_mut() {
            run.pending_tool_job = Some(PendingToolJob { call, handle });
        }
    }

    async fn agent_start_next_tool(&mut self) -> Result<()> {
        let Some(run) = self.agent_run.as_mut() else {
            return Ok(());
        };

        if run.pending_tool_job.is_some() {
            return Ok(());
        }
        if matches!(self.overlay, Some(Overlay::ToolApproval(_))) {
            return Ok(());
        }

        if run.pending_call_index >= run.pending_calls.len() {
            let updates = run.tool_updates.join("\n\n");
            run.pending_calls.clear();
            run.pending_call_index = 0;
            run.tool_updates.clear();
            run.next_message = updates;
            return self.agent_send_next_message().await;
        }

        let call = run.pending_calls[run.pending_call_index].clone();
        let (approved, reason) = self.agent_call_auto_approved(&call);

        self.messages.push_back(ChatMessage {
            role: MessageRole::System,
            content: Self::format_tool_summary(&call),
        });

        if !approved {
            self.overlay = Some(Overlay::ToolApproval(ToolApprovalOverlay { call, reason }));
            self.status = "Approval required".to_string();
            return Ok(());
        }

        self.agent_spawn_tool_job(call);
        Ok(())
    }

    async fn agent_deny_call(&mut self, call: ToolCallSpec) -> Result<()> {
        if let Some(run) = self.agent_run.as_mut() {
            run.tool_updates.push(format!(
                "[Tool Denied] tool={} reason=user_denied",
                call.tool
            ));
            run.pending_call_index = run.pending_call_index.saturating_add(1);
        }

        self.messages.push_back(ChatMessage {
            role: MessageRole::System,
            content: format!("Tool denied: {}", call.tool),
        });

        self.agent_start_next_tool().await
    }

    async fn agent_approve_call(&mut self, call: ToolCallSpec) -> Result<()> {
        self.messages.push_back(ChatMessage {
            role: MessageRole::System,
            content: format!("Tool approved: {}", call.tool),
        });
        self.agent_spawn_tool_job(call);
        Ok(())
    }

    async fn tick_agent(&mut self) -> Result<()> {
        let Some(finished) = (self.agent_run.as_ref()).and_then(|run| {
            run.pending_tool_job
                .as_ref()
                .map(|j| j.handle.is_finished())
        }) else {
            return Ok(());
        };
        if !finished {
            return Ok(());
        }

        let job = {
            let Some(run) = self.agent_run.as_mut() else {
                return Ok(());
            };
            run.pending_tool_job.take()
        };
        let Some(job) = job else {
            return Ok(());
        };

        let (output, is_error) = match job.handle.await {
            Ok(pair) => pair,
            Err(e) => (format!("Tool task failed: {}", e), true),
        };

        self.is_loading = false;
        self.status = "Ready".to_string();

        self.messages.push_back(ChatMessage {
            role: MessageRole::System,
            content: format!(
                "[Tool Result] {}{}\n{}",
                job.call.tool,
                if is_error { " (error)" } else { "" },
                output
            ),
        });

        if let Some(run) = self.agent_run.as_mut() {
            run.tool_updates.push(format!(
                "[Tool Result] tool={}{}\n{}",
                job.call.tool,
                if is_error { " (error)" } else { "" },
                output
            ));
            run.pending_call_index = run.pending_call_index.saturating_add(1);
        }

        self.agent_start_next_tool().await
    }

    /// Handle a slash command.
    async fn handle_command(&mut self, command: &str) -> Result<()> {
        let cmd_trimmed = command.trim_start();
        let cmd_lower = cmd_trimmed.to_ascii_lowercase();

        if drbot_tool_mode::is_project_remember_command(cmd_trimmed)
            || drbot_tool_mode::is_project_forget_command(cmd_trimmed)
        {
            let project_drbot_dir =
                drbot_tool_mode::resolve_project_drbot_dir_best_effort(&self.tool_cfg.root);

            let reply = if drbot_tool_mode::is_project_remember_command(cmd_trimmed) {
                match drbot_tool_mode::parse_project_remember_note(cmd_trimmed) {
                    Some(note) => match drbot_tool_mode::remember_project_kb(&project_drbot_dir, &note)
                    {
                        Ok(updates) => {
                            if updates.rejected {
                                "Nothing saved (refused to store sensitive/invalid content)."
                                    .to_string()
                            } else if updates.applied {
                                if updates.updates.is_empty() {
                                    "Saved to project memory.".to_string()
                                } else {
                                    let mut out = String::new();
                                    out.push_str("Saved to project memory:\n");
                                    for u in updates.updates.iter().take(12) {
                                        out.push_str("- ");
                                        out.push_str(u);
                                        out.push('\n');
                                    }
                                    out.trim_end().to_string()
                                }
                            } else {
                                "Nothing saved.".to_string()
                            }
                        }
                        Err(e) => format!("Project remember error: {}", e),
                    },
                    None => "Usage: /remember project <note>".to_string(),
                }
            } else {
                match drbot_tool_mode::parse_project_forget_arg(cmd_trimmed) {
                    Some(arg) => match drbot_tool_mode::forget_project_kb(&project_drbot_dir, &arg) {
                        Ok(updates) => {
                            if updates.applied {
                                if updates.updates.is_empty() {
                                    "Forgot from project memory.".to_string()
                                } else {
                                    let mut out = String::new();
                                    out.push_str("Forgot from project memory:\n");
                                    for u in updates.updates.iter().take(12) {
                                        out.push_str("- ");
                                        out.push_str(u);
                                        out.push('\n');
                                    }
                                    out.trim_end().to_string()
                                }
                            } else {
                                "Nothing forgotten (no matching items).".to_string()
                            }
                        }
                        Err(e) => format!("Project forget error: {}", e),
                    },
                    None => "Usage: /forget project <all|pinned|conventions|runbooks|kb|text>"
                        .to_string(),
                }
            };

            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: reply,
            });
            return Ok(());
        }

        let mut mem_parts = cmd_trimmed.split_whitespace();
        let mem_cmd = mem_parts.next().unwrap_or("");
        let mem_arg = mem_parts.next().unwrap_or("");
        if (mem_cmd == "/memory" || mem_cmd == "/mem") && mem_arg.eq_ignore_ascii_case("project") {
            let project_drbot_dir =
                drbot_tool_mode::resolve_project_drbot_dir_best_effort(&self.tool_cfg.root);
            let reply = drbot_tool_mode::build_project_memory_overview(&project_drbot_dir);
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: reply,
            });
            return Ok(());
        }

        // Local workspace commands are routed through the gateway (no provider call).
        //
        // We also optionally merge project-local `.drbot/` knowledge base results for `/kb`.
        let is_remember_cmd = cmd_lower.starts_with("/remember");
        let is_forget_cmd = cmd_lower.starts_with("/forget");
        let is_memory_cmd = cmd_lower == "/memory" || cmd_lower == "/mem";
        let is_profile_cmd = cmd_lower == "/profile";
        let is_kb_cmd = cmd_lower == "/kb"
            || cmd_lower.starts_with("/kb ")
            || cmd_lower.starts_with("/kb:")
            || cmd_lower == "/notes"
            || cmd_lower.starts_with("/notes ")
            || cmd_lower.starts_with("/notes:");

        if is_remember_cmd || is_forget_cmd || is_memory_cmd || is_profile_cmd || is_kb_cmd {
            if is_kb_cmd {
                let query = if cmd_lower == "/kb"
                    || cmd_lower.starts_with("/kb ")
                    || cmd_lower.starts_with("/kb:")
                {
                    cmd_trimmed[3..].trim_start_matches(&[' ', ':'][..]).trim()
                } else if cmd_lower == "/notes"
                    || cmd_lower.starts_with("/notes ")
                    || cmd_lower.starts_with("/notes:")
                {
                    cmd_trimmed[6..].trim_start_matches(&[' ', ':'][..]).trim()
                } else {
                    ""
                };

                if query.trim().is_empty() {
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Usage: /kb <query>".to_string(),
                    });
                    return Ok(());
                }

                let workspace_reply = self
                    .gateway_chat_send_non_streaming_best_effort(cmd_trimmed)
                    .await
                    .and_then(|r| r.content);

                let project_drbot_dir =
                    drbot_tool_mode::resolve_project_drbot_dir_best_effort(&self.tool_cfg.root);
                let project_reply = drbot_core::workspace_notes_recall::recall_project_notes_prompt_explicit(
                    &project_drbot_dir,
                    query,
                )
                .await;

                let mut out = String::new();

                out.push_str("Workspace KB:\n");
                if let Some(reply) = workspace_reply.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    out.push_str(reply);
                } else if self.gateway.is_some() {
                    out.push_str("(unavailable)");
                } else {
                    out.push_str("(gateway not connected)");
                }

                out.push_str("\n\nProject KB (.drbot):\n");
                if let Some(reply) = project_reply.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    out.push_str(reply);
                } else {
                    out.push_str("No relevant notes found.");
                }

                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: out,
                });
                return Ok(());
            }

            if is_memory_cmd {
                let workspace_reply = self
                    .gateway_chat_send_non_streaming_best_effort(cmd_trimmed)
                    .await
                    .and_then(|r| r.content);

                let project_drbot_dir =
                    drbot_tool_mode::resolve_project_drbot_dir_best_effort(&self.tool_cfg.root);
                let in_git_project =
                    drbot_tool_mode::find_git_root_best_effort(&self.tool_cfg.root).is_some();
                let project_reply = if in_git_project || project_drbot_dir.is_dir() {
                    Some(drbot_tool_mode::build_project_memory_overview(&project_drbot_dir))
                } else {
                    None
                };

                let mut out = String::new();
                if let Some(reply) = workspace_reply
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                {
                    out.push_str(reply);
                } else if self.gateway.is_some() {
                    out.push_str("(workspace memory unavailable)");
                } else {
                    out.push_str("(gateway not connected)");
                }

                if let Some(project) = project_reply
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                {
                    out.push_str("\n\n");
                    out.push_str(project);
                }

                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: out,
                });
                return Ok(());
            }

            let Some(result) = self.gateway_chat_send_non_streaming_best_effort(cmd_trimmed).await
            else {
                if self.gateway.is_none() {
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Gateway not connected.".to_string(),
                    });
                }
                return Ok(());
            };

            let reply = result.content.unwrap_or_default();
            if is_remember_cmd || is_forget_cmd {
                self.session_id = Some(result.session_id);
                self.status = format!("Session: {}", result.session_id);
                self.remember_last_session_for_current_project_best_effort(result.session_id);
            }

            if !reply.trim().is_empty() {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: reply,
                });
            }
            return Ok(());
        }

        let parts: Vec<&str> = command.split_whitespace().collect();
        match parts.first().map(|s| *s) {
            Some("/quit") | Some("/exit") => {
                self.quit = true;
            }
            Some("/wizard") | Some("/setup") => {
                self.exit_action = crate::ExitAction::LaunchWizard;
                self.quit = true;
                self.status = "Launching wizard...".to_string();
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: "Exiting TUI to run `drbot wizard`...".to_string(),
                });
            }
            Some("/clear") => {
                self.messages.clear();
                self.scroll_offset = 0;
                self.status = "Cleared".to_string();
            }
            Some("/help") => {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: "Commands:\n  /quit, /exit - Exit the app\n  /clear - Clear chat history\n  /help - Show this help\n  /wizard, /setup - Run setup wizard\n  /openclaw, /oc - Open OpenClaw dashboard\n  /skills - Open skills overlay\n  /skills status - List skills\n  /skills add <skillKey> <url> - Add/enable remote skill\n  /skills enable <skillKey> [url] - Enable skill\n  /skills disable <skillKey> - Disable skill\n\n  /remember <note> - Save to memory (workspace)\n  /forget <name|timezone|style|all|text> - Forget from memory (workspace)\n  /remember project <note> - Save to project memory (.drbot)\n  /forget project <all|pinned|conventions|runbooks|kb|text> - Forget from project memory (.drbot)\n  /profile - Show USER.md profile (workspace)\n  /memory, /mem - Show memory overview (workspace + project)\n  /memory project - Show project memory only (.drbot)\n  /kb <query>, /notes <query> - Search notes (workspace + .drbot)\n\n  /provider - Pick provider (opens picker)\n  /provider list - List providers\n  /provider <name> - Select provider\n  /model - Pick model (opens picker)\n  /model list - List models\n  /model <id> - Set model override\n  /model clear - Use provider default\n\n  /agent on|off - Toggle agent/tools mode\n  /approve on|off - Toggle auto-approve for tools\n  /tools - Show tool settings/status\n  /trust list - List trusted bash commands\n  /trust clear - Clear trusted bash commands\n\n  /sessions - Pick a session (opens picker)\n  /sessions list - List sessions\n  /session - Show current session\n  /session new - Create a new session\n  /session clear - Clear current session history\n  /session delete - Delete current session\n  /session <uuid> - Open a session\n\nShortcuts:\n  Esc - Stop current agent/run\n  Ctrl+C - Quit\n  Ctrl+P - Provider picker\n  Ctrl+M - Model picker\n  Ctrl+O - Session picker\n  Ctrl+D - OpenClaw dashboard\n  Ctrl+K - Skills\n  Ctrl+T - Toggle agent/tools\n  Ctrl+Y - Toggle auto-approve"
                        .to_string(),
                });
            }
            Some("/tools") => {
                let agent = if self.tool_cfg.enabled { "on" } else { "off" };
                let approve = if self.tool_cfg.auto_approve {
                    "on"
                } else {
                    "off"
                };
                let strict = if self.tool_strict { "on" } else { "off" };
                let trusted = self.trusted_bash_commands.len();
                let allowlist = if self.tool_cfg.tool_allowlist.is_empty() {
                    "all".to_string()
                } else {
                    self.tool_cfg.tool_allowlist.join(", ")
                };
                let denylist = if self.tool_cfg.tool_denylist.is_empty() {
                    "(none)".to_string()
                } else {
                    self.tool_cfg.tool_denylist.join(", ")
                };
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: format!(
                        "Tools:\n  agent: {}\n  autonomy: {:?}\n  auto-approve: {}\n  strict: {}\n  root: {}\n  max_rounds: {}\n  tool allowlist: {}\n  tool denylist: {}\n  trusted bash: {}\n\nUsage:\n  /agent on|off\n  /approve on|off\n  /trust list\n  /trust clear",
                        agent,
                        self.tool_cfg.autonomy_mode,
                        approve,
                        strict,
                        self.tool_cfg.root.display(),
                        self.tool_cfg.max_rounds,
                        allowlist,
                        denylist,
                        trusted
                    ),
                });
            }
            Some("/trust") => {
                let usage = "Usage: /trust list|clear";
                let action = parts.get(1).copied().unwrap_or("list");
                match action {
                    "list" => {
                        if self.trusted_bash_commands.is_empty() {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: "No trusted bash commands.".to_string(),
                            });
                        } else {
                            let mut out = String::new();
                            out.push_str("Trusted bash commands:\n");
                            for cmd in self.trusted_bash_commands.iter().take(20) {
                                out.push_str(&format!("- {}\n", cmd));
                            }
                            if self.trusted_bash_commands.len() > 20 {
                                out.push_str(&format!(
                                    "... ({} more)\n",
                                    self.trusted_bash_commands.len() - 20
                                ));
                            }
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: out.trim_end().to_string(),
                            });
                        }
                    }
                    "clear" => {
                        self.trusted_bash_commands.clear();
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: "Cleared trusted bash commands.".to_string(),
                        });
                    }
                    _ => {
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: usage.to_string(),
                        });
                    }
                }
            }
            Some("/agent") => {
                let usage = "Usage: /agent on|off";
                if parts.len() == 1 {
                    let agent = if self.tool_cfg.enabled { "on" } else { "off" };
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: format!("Agent mode is {}. {}", agent, usage),
                    });
                    return Ok(());
                }

                match parts[1] {
                    "on" => {
                        self.tool_cfg.enabled = true;
                        self.status = "Agent mode: on".to_string();
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: "Agent mode enabled. Read-only tools and safe bash auto-run; write tools may ask approval. (Use /approve on to auto-approve.)".to_string(),
                        });
                        self.save_prefs_best_effort();
                    }
                    "off" => {
                        self.tool_cfg.enabled = false;
                        self.agent_run = None;
                        if matches!(self.overlay, Some(Overlay::ToolApproval(_))) {
                            self.overlay = None;
                        }
                        self.status = "Agent mode: off".to_string();
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: "Agent mode disabled.".to_string(),
                        });
                        self.save_prefs_best_effort();
                    }
                    _ => {
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: usage.to_string(),
                        });
                    }
                }
            }
            Some("/approve") => {
                let usage = "Usage: /approve on|off";
                if parts.len() == 1 {
                    let approve = if self.tool_cfg.auto_approve {
                        "on"
                    } else {
                        "off"
                    };
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: format!("Auto-approve is {}. {}", approve, usage),
                    });
                    return Ok(());
                }
                match parts[1] {
                    "on" => {
                        self.tool_cfg.auto_approve = true;
                        self.status = "Auto-approve: on".to_string();
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: "Auto-approve enabled.".to_string(),
                        });
                        self.save_prefs_best_effort();
                    }
                    "off" => {
                        self.tool_cfg.auto_approve = false;
                        self.status = "Auto-approve: off".to_string();
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: "Auto-approve disabled.".to_string(),
                        });
                        self.save_prefs_best_effort();
                    }
                    _ => {
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: usage.to_string(),
                        });
                    }
                }
            }
            Some("/openclaw") | Some("/oc") => {
                let _ = self.open_openclaw_overlay().await;
            }
            Some("/skills") => {
                let usage = "Usage: /skills [open|status|refresh|add|enable|disable]";
                if parts.len() == 1 || parts[1] == "open" {
                    let _ = self.open_skills_overlay().await;
                    return Ok(());
                }
                match parts[1] {
                    "status" => {
                        let _ = self.refresh_openclaw_skills(false, true).await;
                    }
                    "refresh" => {
                        let _ = self.refresh_openclaw_skills(true, false).await;
                    }
                    "add" => {
                        if parts.len() < 4 {
                            self.open_skills_add_overlay();
                            return Ok(());
                        }
                        let skill_key = parts[2];
                        let url = parts[3..].join(" ");
                        self.openclaw_skills_update(
                            skill_key,
                            Some(true),
                            Some(url),
                            Some(true),
                        )
                        .await?;
                        let _ = self.refresh_openclaw_skills(true, false).await;
                    }
                    "enable" => {
                        if parts.len() < 3 {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: "Usage: /skills enable <skillKey> [url]".to_string(),
                            });
                            return Ok(());
                        }
                        let skill_key = parts[2];
                        let url = parts.get(3).map(|s| s.to_string());
                        self.openclaw_skills_update(
                            skill_key,
                            Some(true),
                            url,
                            Some(true),
                        )
                        .await?;
                        let _ = self.refresh_openclaw_skills(true, false).await;
                    }
                    "disable" => {
                        if parts.len() < 3 {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: "Usage: /skills disable <skillKey>".to_string(),
                            });
                            return Ok(());
                        }
                        let skill_key = parts[2];
                        self.openclaw_skills_update(skill_key, Some(false), None, None)
                            .await?;
                        let _ = self.refresh_openclaw_skills(true, false).await;
                    }
                    _ => {
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: usage.to_string(),
                        });
                    }
                }
            }
            Some("/model") => {
                if parts.len() == 1 {
                    let _ = self.open_model_picker().await;
                    return Ok(());
                }

                let Some(gateway) = self.gateway.clone() else {
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Gateway not connected.".to_string(),
                    });
                    return Ok(());
                };

                if parts[1] == "list" {
                    let resp = gateway
                        .request("provider.models", ProviderModelsParams { provider: None })
                        .await?;
                    self.last_response = Some(resp.clone());
                    if let Some(err) = resp.error {
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!("provider.models error: {}", err.message),
                        });
                        return Ok(());
                    }

                    let models = resp
                        .result
                        .and_then(|v| serde_json::from_value::<ProviderModelsResult>(v).ok())
                        .map(|r| r.models)
                        .unwrap_or_default();

                    let cur = self.model.as_deref().unwrap_or("(default)");
                    let mut out = String::new();
                    out.push_str(&format!("Current model: {}\n", cur));
                    out.push_str("Available models:\n");
                    for m in models.iter() {
                        out.push_str(&format!("- {}  {}\n", m.id, m.name));
                    }
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: out.trim_end().to_string(),
                    });
                } else if parts[1] == "clear" || parts[1] == "default" {
                    self.model = None;
                    self.status = "Model: (default)".to_string();
                    if let Some(session_id) = self.session_id {
                        let resp = gateway
                            .request(
                                "session.update",
                                SessionUpdateParams {
                                    session_id,
                                    clear_model: true,
                                    ..Default::default()
                                },
                            )
                            .await?;
                        self.last_response = Some(resp.clone());
                        if let Some(err) = resp.error {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: format!("session.update error: {}", err.message),
                            });
                        }
                    }
                } else {
                    self.model = Some(parts[1].to_string());
                    self.status = format!("Model set: {}", parts[1]);
                    if let Some(session_id) = self.session_id {
                        let resp = gateway
                            .request(
                                "session.update",
                                SessionUpdateParams {
                                    session_id,
                                    model: Some(parts[1].to_string()),
                                    ..Default::default()
                                },
                            )
                            .await?;
                        self.last_response = Some(resp.clone());
                        if let Some(err) = resp.error {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: format!("session.update error: {}", err.message),
                            });
                        }
                    }
                }
            }
            Some("/session") => {
                let Some(gateway) = self.gateway.clone() else {
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Gateway not connected.".to_string(),
                    });
                    return Ok(());
                };

                if parts.len() == 1 || parts.get(1) == Some(&"show") {
                    let Some(id) = self.session_id else {
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: "Current session: (none)  (try: /session new)".to_string(),
                        });
                        return Ok(());
                    };

                    let resp = gateway
                        .request(
                            "session.get",
                            drbot_protocol::SessionGetParams { session_id: id },
                        )
                        .await?;
                    self.last_response = Some(resp.clone());
                    if let Some(err) = resp.error {
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!("session.get error: {}", err.message),
                        });
                        return Ok(());
                    }
                    if let Some(result) = resp.result {
                        if let Ok(parsed) =
                            serde_json::from_value::<drbot_protocol::SessionGetResult>(result)
                        {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: format!(
                                    "Session: {}\nState: {}\nTitle: {}\nProvider: {}\nModel: {}\nSystem prompt: {}\nMessages: {}",
                                    parsed.session.id,
                                    parsed.session.state,
                                    parsed
                                        .session
                                        .title
                                        .clone()
                                        .unwrap_or_else(|| "(untitled)".to_string()),
                                    parsed
                                        .session
                                        .provider
                                        .clone()
                                        .unwrap_or_else(|| "(default)".to_string()),
                                    parsed
                                        .session
                                        .model
                                        .clone()
                                        .unwrap_or_else(|| "(default)".to_string()),
                                    parsed
                                        .system_prompt
                                        .as_deref()
                                        .map(|s| s.trim())
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or("(none)"),
                                    parsed.messages.len()
                                ),
                            });
                        }
                    }
                    return Ok(());
                }

                match parts[1] {
                    "new" => {
                        let resp = gateway
                            .request(
                                "session.create",
                                drbot_protocol::SessionCreateParams {
                                    title: None,
                                    provider: self.active_provider.clone(),
                                    model: self.model.clone(),
                                    system_prompt: self.config.system_prompt.clone(),
                                },
                            )
                            .await?;
                        self.last_response = Some(resp.clone());
                        if let Some(err) = resp.error {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: format!("session.create error: {}", err.message),
                            });
                            return Ok(());
                        }

                        let session_id = resp
                            .result
                            .and_then(|v| {
                                serde_json::from_value::<drbot_protocol::SessionCreateResult>(v)
                                    .ok()
                            })
                            .map(|r| r.session_id);

                        let Some(session_id) = session_id else {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: "session.create returned no session_id".to_string(),
                            });
                            return Ok(());
                        };

                        self.session_id = Some(session_id);
                        self.remember_last_session_for_current_project_best_effort(session_id);
                        self.messages.clear();
                        self.scroll_offset = 0;
                        self.status = format!("Session created: {}", session_id);
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!("New session: {}", session_id),
                        });
                    }
                    "clear" => {
                        let Some(id) = self.session_id else {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: "No active session to clear.".to_string(),
                            });
                            return Ok(());
                        };

                        let resp = gateway
                            .request(
                                "session.clear",
                                drbot_protocol::SessionClearParams { session_id: id },
                            )
                            .await?;
                        self.last_response = Some(resp.clone());
                        if let Some(err) = resp.error {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: format!("session.clear error: {}", err.message),
                            });
                            return Ok(());
                        }

                        self.messages.clear();
                        self.scroll_offset = 0;
                        self.status = format!("Session cleared: {}", id);
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!("Session cleared: {}", id),
                        });
                    }
                    "delete" => {
                        let Some(id) = self.session_id else {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: "No active session to delete.".to_string(),
                            });
                            return Ok(());
                        };

                        let resp = gateway
                            .request(
                                "session.delete",
                                drbot_protocol::SessionDeleteParams { session_id: id },
                            )
                            .await?;
                        self.last_response = Some(resp.clone());
                        if let Some(err) = resp.error {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: format!("session.delete error: {}", err.message),
                            });
                            return Ok(());
                        }

                        self.session_id = None;
                        self.clear_last_session_for_current_project_best_effort();
                        self.messages.clear();
                        self.scroll_offset = 0;
                        self.status = format!("Session deleted: {}", id);
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!("Session deleted: {}", id),
                        });
                    }
                    "open" => {
                        let Some(id_str) = parts.get(2).copied() else {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: "Usage: /session open <uuid>".to_string(),
                            });
                            return Ok(());
                        };
                        let Ok(id) = id_str.parse::<uuid::Uuid>() else {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: format!("Invalid session id: {}", id_str),
                            });
                            return Ok(());
                        };
                        let _ = self.load_session(id).await;
                    }
                    other => {
                        if let Ok(id) = other.parse::<uuid::Uuid>() {
                            let _ = self.load_session(id).await;
                        } else {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: format!(
                                    "Unknown /session command: {} (try: /session new | /session clear | /session delete | /session <uuid>)",
                                    other
                                ),
                            });
                        }
                    }
                }
            }
            Some("/sessions") => {
                if parts.len() == 1 {
                    let _ = self.open_session_picker().await;
                    return Ok(());
                }

                let Some(gateway) = self.gateway.clone() else {
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Gateway not connected.".to_string(),
                    });
                    return Ok(());
                };

                if parts[1] != "list" {
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Usage: /sessions (picker) or /sessions list".to_string(),
                    });
                    return Ok(());
                }

                let resp = gateway
                    .request("session.list", drbot_protocol::SessionListParams::default())
                    .await?;
                self.last_response = Some(resp.clone());
                if let Some(err) = resp.error {
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: format!("session.list error: {}", err.message),
                    });
                } else if let Some(result) = resp.result {
                    let parsed: drbot_protocol::SessionListResult = serde_json::from_value(result)
                        .unwrap_or(drbot_protocol::SessionListResult {
                            sessions: vec![],
                            total: 0,
                        });
                    let mut out = String::new();
                    out.push_str(&format!("Sessions ({}):\n", parsed.total));
                    for s in parsed.sessions.iter().take(20) {
                        let provider = s
                            .provider
                            .clone()
                            .unwrap_or_else(|| "(default)".to_string());
                        let model = s.model.clone().unwrap_or_else(|| "(default)".to_string());
                        out.push_str(&format!(
                            "- {}  {}  provider:{}  model:{}  {}\n",
                            s.id,
                            s.state,
                            provider,
                            model,
                            s.title.clone().unwrap_or_else(|| "(untitled)".to_string())
                        ));
                    }
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: out.trim_end().to_string(),
                    });
                }
            }
            Some("/provider") => {
                if parts.len() == 1 {
                    let _ = self.refresh_providers(true).await;
                    return Ok(());
                }

                let Some(gateway) = self.gateway.clone() else {
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Gateway not connected.".to_string(),
                    });
                    return Ok(());
                };

                if parts[1] != "list" {
                    // provider.select is implemented in the gateway (legacy protocol extension).
                    let resp = gateway
                        .request(
                            "provider.select",
                            ProviderSelectParams {
                                provider: parts[1].to_string(),
                            },
                        )
                        .await?;
                    self.last_response = Some(resp.clone());
                    if let Some(err) = resp.error {
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!("provider.select error: {}", err.message),
                        });
                    } else {
                        let name = resp
                            .result
                            .clone()
                            .and_then(|v| {
                                serde_json::from_value::<drbot_protocol::ProviderSelectResult>(v)
                                    .ok()
                            })
                            .map(|r| r.provider.name)
                            .unwrap_or_else(|| parts[1].to_string());
                        self.active_provider = Some(name.clone());
                        self.active_provider_status = Some("active".to_string());
                        // Avoid cross-provider model mismatches.
                        self.model = None;
                        self.status = format!("Provider selected: {}", name);
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!(
                                "Provider set to {}. Model reset to provider default.",
                                name
                            ),
                        });

                        if let Some(session_id) = self.session_id {
                            let update_resp = gateway
                                .request(
                                    "session.update",
                                    SessionUpdateParams {
                                        session_id,
                                        provider: Some(name.clone()),
                                        clear_model: true,
                                        ..Default::default()
                                    },
                                )
                                .await?;
                            self.last_response = Some(update_resp.clone());
                            if let Some(err) = update_resp.error {
                                self.messages.push_back(ChatMessage {
                                    role: MessageRole::System,
                                    content: format!("session.update error: {}", err.message),
                                });
                            }
                        }
                    }
                    return Ok(());
                }

                let resp = gateway
                    .request(
                        "provider.list",
                        drbot_protocol::ProviderListParams::default(),
                    )
                    .await?;
                self.last_response = Some(resp.clone());
                if let Some(err) = resp.error {
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: format!("provider.list error: {}", err.message),
                    });
                } else if let Some(result) = resp.result {
                    let parsed: drbot_protocol::ProviderListResult = serde_json::from_value(result)
                        .unwrap_or(drbot_protocol::ProviderListResult { providers: vec![] });
                    let mut out = String::new();
                    out.push_str("Providers:\n");
                    for p in parsed.providers.iter() {
                        if p.status.starts_with("active") {
                            self.active_provider = Some(p.name.clone());
                            self.active_provider_status = Some(p.status.clone());
                        }
                        out.push_str(&format!(
                            "- {} ({})  models: {}\n",
                            p.name,
                            p.status,
                            p.models.len()
                        ));
                    }
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: out.trim_end().to_string(),
                    });
                }
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
        if let Some(rx) = &mut self.gateway_resp_rx {
            while let Ok(resp) = rx.try_recv() {
                self.last_response = Some(resp.clone());
                if self.active_request_id == Some(resp.id) {
                    if let Some(err) = resp.error {
                        self.is_loading = false;
                        self.active_request_id = None;
                        self.streaming_content.clear();
                        self.status = "Error".to_string();
                        // Abort any in-flight agent run.
                        self.agent_run = None;
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!("chat.send error: {}", err.message),
                        });
                    } else if let Some(result) = resp.result {
                        // If stream=false ever becomes the default, support showing the content.
                        if let Ok(parsed) =
                            serde_json::from_value::<drbot_protocol::ChatSendResult>(result)
                        {
                            if let Some(provider) = parsed.provider.as_deref() {
                                if self.active_provider.as_deref() != Some(provider) {
                                    self.active_provider = Some(provider.to_string());
                                    self.active_provider_status = Some("active".to_string());
                                    // Avoid cross-provider model mismatches.
                                    self.model = None;
                                }
                            }
                            if let Some(content) = parsed.content {
                                self.is_loading = false;
                                self.active_request_id = None;
                                self.status = "Ready".to_string();
                                self.messages.push_back(ChatMessage {
                                    role: MessageRole::Assistant,
                                    content,
                                });
                                self.session_id = Some(parsed.session_id);
                            }
                        }
                    }
                }
            }
        }

        // Drain gateway events first so we can `await` while processing without holding
        // a mutable borrow of the receiver.
        let mut gateway_events = Vec::new();
        if let Some(rx) = &mut self.gateway_rx {
            while let Ok(event) = rx.try_recv() {
                gateway_events.push(event);
            }
        }

        for event in gateway_events {
            match event.event_type.as_str() {
                "system.disconnected" => {
                    self.is_loading = false;
                    self.active_request_id = None;
                    self.agent_run = None;
                    self.status = "Gateway disconnected".to_string();
                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Gateway disconnected. (Restart or check gateway logs.)"
                            .to_string(),
                    });
                }
                event_types::CHAT_STREAM_START => {
                    if let Ok(start) =
                        serde_json::from_value::<StreamStartEvent>(event.data.clone())
                    {
                        if self.active_request_id == Some(start.request_id) {
                            self.session_id = Some(start.session_id);
                            self.remember_last_session_for_current_project_best_effort(
                                start.session_id,
                            );
                            if let Some(provider) = start.provider.as_deref() {
                                if self.active_provider.as_deref() != Some(provider) {
                                    self.active_provider = Some(provider.to_string());
                                    self.active_provider_status = Some("active".to_string());
                                    self.model = None;
                                }
                                self.status =
                                    format!("Streaming... ({}/{})", provider, start.model);
                            } else {
                                self.status = format!("Streaming... ({})", start.model);
                            }
                        }
                    }
                }
                event_types::CHAT_STREAM_DELTA => {
                    if let Ok(delta) =
                        serde_json::from_value::<StreamDeltaEvent>(event.data.clone())
                    {
                        if self.active_request_id == Some(delta.request_id) {
                            // In agent mode we hide the raw tool JSON streaming and instead
                            // display tool summaries/results.
                            if !(self.tool_cfg.enabled && self.agent_run.is_some()) {
                                self.streaming_content.push_str(&delta.delta);
                            }
                        }
                    }
                }
                event_types::CHAT_STREAM_COMPLETE => {
                    if let Ok(done) =
                        serde_json::from_value::<StreamCompleteEvent>(event.data.clone())
                    {
                        if self.active_request_id == Some(done.request_id) {
                            self.is_loading = false;
                            self.active_request_id = None;
                            self.status = "Ready".to_string();

                            let assistant_text = done.content;
                            if self.tool_cfg.enabled && self.agent_run.is_some() {
                                self.agent_handle_assistant_complete(assistant_text).await?;
                            } else {
                                self.messages.push_back(ChatMessage {
                                    role: MessageRole::Assistant,
                                    content: assistant_text,
                                });
                            }
                        }
                    }
                }
                event_types::CHAT_STREAM_ERROR => {
                    if let Ok(err) = serde_json::from_value::<StreamErrorEvent>(event.data.clone())
                    {
                        if self.active_request_id == Some(err.request_id) {
                            self.is_loading = false;
                            self.active_request_id = None;
                            self.streaming_content.clear();
                            self.status = "Error".to_string();
                            self.agent_run = None;
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: format!("Error: {}", err.error),
                            });
                        }
                    }
                }
                event_types::PROVIDER_CHANGED => {
                    if let Ok(changed) = serde_json::from_value::<ChangedEvent>(event.data.clone())
                    {
                        let previous = changed.previous_provider.clone();
                        self.active_provider = Some(changed.provider.clone());
                        self.active_provider_status = Some("active".to_string());
                        // Avoid cross-provider model mismatches.
                        self.model = None;

                        let mut msg = String::new();
                        if let Some(prev) = previous {
                            msg.push_str(&format!(
                                "Provider changed: {} -> {}",
                                prev, changed.provider
                            ));
                        } else {
                            msg.push_str(&format!("Provider changed: {}", changed.provider));
                        }
                        if let Some(reason) = changed.reason {
                            msg.push_str(&format!("\nReason: {}", reason));
                        }
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: msg,
                        });
                    }
                }
                other => {
                    debug!(event_type = %other, "Unhandled gateway event");
                }
            }
        }

        if let Some(rx) = &mut self.openclaw_rx {
            let mut disconnected = false;
            while let Ok(event) = rx.try_recv() {
                if event.event == "tick" {
                    continue;
                }
                if event.event == "system.disconnected" {
                    disconnected = true;
                    continue;
                }

                let payload = event
                    .payload
                    .as_ref()
                    .and_then(|v| serde_json::to_string(v).ok())
                    .unwrap_or_default();
                let preview = if payload.len() > 240 {
                    format!("{}...", &payload[..240])
                } else {
                    payload
                };
                let line = if preview.is_empty() {
                    event.event.clone()
                } else {
                    format!("{} {}", event.event, preview)
                };

                self.openclaw_events.push_back(line);
                while self.openclaw_events.len() > 200 {
                    self.openclaw_events.pop_front();
                }
            }

            if disconnected {
                self.openclaw = None;
                self.openclaw_rx = None;
                self.openclaw_hello = None;
                self.openclaw_health = None;
                self.openclaw_logs.clear();
                self.openclaw_logs_cursor = None;
            }
        }

        // Advance any in-flight agent tool loop.
        self.tick_agent().await?;

        Ok(())
    }

    async fn handle_overlay_key(&mut self, key: KeyEvent) -> Result<()> {
        if matches!(self.overlay, Some(Overlay::ToolApproval(_))) {
            match key.code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let call = match self.overlay.take() {
                        Some(Overlay::ToolApproval(t)) => t.call,
                        other => {
                            self.overlay = other;
                            return Ok(());
                        }
                    };
                    return self.agent_approve_call(call).await;
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    self.tool_cfg.auto_approve = true;
                    self.status = "Auto-approve: on".to_string();

                    let call = match self.overlay.take() {
                        Some(Overlay::ToolApproval(t)) => t.call,
                        other => {
                            self.overlay = other;
                            return Ok(());
                        }
                    };

                    // If the tool is bash, also trust this exact command so repeated calls
                    // don't keep prompting even when the command isn't "safe" per policy.
                    if call.tool == "bash" {
                        if let Some(cmd) = call
                            .args
                            .get("command")
                            .and_then(|v| v.as_str())
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                        {
                            self.trusted_bash_commands.insert(cmd.to_string());
                        }
                    }

                    self.messages.push_back(ChatMessage {
                        role: MessageRole::System,
                        content: "Auto-approve enabled.".to_string(),
                    });
                    self.save_prefs_best_effort();
                    return self.agent_approve_call(call).await;
                }
                KeyCode::Char('t') | KeyCode::Char('T') => {
                    let overlay = match self.overlay.take() {
                        Some(Overlay::ToolApproval(t)) => t,
                        other => {
                            self.overlay = other;
                            return Ok(());
                        }
                    };

                    if overlay.call.tool != "bash" {
                        self.overlay = Some(Overlay::ToolApproval(overlay));
                        return Ok(());
                    }

                    if let Some(cmd) = overlay
                        .call
                        .args
                        .get("command")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                    {
                        self.trusted_bash_commands.insert(cmd.to_string());
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!("Trusted bash command for this run: {}", cmd),
                        });
                    }

                    return self.agent_approve_call(overlay.call).await;
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    let call = match self.overlay.take() {
                        Some(Overlay::ToolApproval(t)) => t.call,
                        other => {
                            self.overlay = other;
                            return Ok(());
                        }
                    };
                    return self.agent_deny_call(call).await;
                }
                _ => {
                    return Ok(());
                }
            }
        }

        if matches!(self.overlay, Some(Overlay::SkillsAdd(_))) {
            match key.code {
                KeyCode::Esc => {
                    self.overlay = None;
                }
                KeyCode::Enter => {
                    let _ = self.submit_skills_add_overlay().await;
                }
                KeyCode::Left => {
                    if let Some(Overlay::SkillsAdd(s)) = self.overlay.as_mut() {
                        if s.cursor_pos > 0 {
                            s.cursor_pos = s.cursor_pos.saturating_sub(1);
                        }
                    }
                }
                KeyCode::Right => {
                    if let Some(Overlay::SkillsAdd(s)) = self.overlay.as_mut() {
                        if s.cursor_pos < s.input.len() {
                            s.cursor_pos = s.cursor_pos.saturating_add(1);
                        }
                    }
                }
                KeyCode::Home => {
                    if let Some(Overlay::SkillsAdd(s)) = self.overlay.as_mut() {
                        s.cursor_pos = 0;
                    }
                }
                KeyCode::End => {
                    if let Some(Overlay::SkillsAdd(s)) = self.overlay.as_mut() {
                        s.cursor_pos = s.input.len();
                    }
                }
                KeyCode::Backspace => {
                    if let Some(Overlay::SkillsAdd(s)) = self.overlay.as_mut() {
                        if s.cursor_pos > 0 {
                            s.cursor_pos -= 1;
                            s.input.remove(s.cursor_pos);
                        }
                    }
                }
                KeyCode::Delete => {
                    if let Some(Overlay::SkillsAdd(s)) = self.overlay.as_mut() {
                        if s.cursor_pos < s.input.len() {
                            s.input.remove(s.cursor_pos);
                        }
                    }
                }
                KeyCode::Char(c) => {
                    if !key.modifiers.contains(KeyModifiers::CONTROL) {
                        if let Some(Overlay::SkillsAdd(s)) = self.overlay.as_mut() {
                            s.input.insert(s.cursor_pos, c);
                            s.cursor_pos += 1;
                        }
                    }
                }
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.overlay = None;
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                if matches!(self.overlay, Some(Overlay::FirstRun(_))) {
                    self.exit_action = crate::ExitAction::LaunchWizard;
                    self.quit = true;
                    self.status = "Launching wizard...".to_string();
                    self.overlay = None;
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if matches!(self.overlay, Some(Overlay::FirstRun(_))) {
                    let _ = self.refresh_providers(true).await;
                } else if let Some(Overlay::Openclaw(oc)) = self.overlay.as_mut() {
                    match oc.tab {
                        OpenclawTab::Overview => {
                            let _ = self.refresh_openclaw_health().await;
                        }
                        OpenclawTab::Logs => {
                            let _ = self.refresh_openclaw_logs().await;
                        }
                        OpenclawTab::Events => {
                            // No request needed; events are streamed. Allow R to clear.
                            self.openclaw_events.clear();
                        }
                    }
                } else if matches!(self.overlay, Some(Overlay::Skills(_))) {
                    let _ = self.refresh_openclaw_skills(true, false).await;
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                if let Some(Overlay::FirstRun(fr)) = self.overlay.clone() {
                    let providers = fr.providers;
                    let selected = providers
                        .iter()
                        .position(|p| p.status == "available" || p.status.starts_with("active"))
                        .unwrap_or(0);
                    self.overlay = Some(Overlay::ProviderPicker(ProviderPicker {
                        providers,
                        selected,
                    }));
                }
            }
            KeyCode::Up => {
                if let Some(Overlay::ProviderPicker(p)) = self.overlay.as_mut() {
                    p.selected = p.selected.saturating_sub(1);
                } else if let Some(Overlay::ModelPicker(p)) = self.overlay.as_mut() {
                    p.selected = p.selected.saturating_sub(1);
                } else if let Some(Overlay::SessionPicker(p)) = self.overlay.as_mut() {
                    p.selected = p.selected.saturating_sub(1);
                } else if let Some(Overlay::Openclaw(oc)) = self.overlay.as_mut() {
                    oc.scroll = oc.scroll.saturating_add(1);
                } else if let Some(Overlay::Skills(s)) = self.overlay.as_mut() {
                    s.selected = s.selected.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(Overlay::ProviderPicker(p)) = self.overlay.as_mut() {
                    p.selected = (p.selected + 1).min(p.providers.len().saturating_sub(1));
                } else if let Some(Overlay::ModelPicker(p)) = self.overlay.as_mut() {
                    p.selected = (p.selected + 1).min(p.models.len().saturating_sub(1));
                } else if let Some(Overlay::SessionPicker(p)) = self.overlay.as_mut() {
                    p.selected = (p.selected + 1).min(p.sessions.len().saturating_sub(1));
                } else if let Some(Overlay::Openclaw(oc)) = self.overlay.as_mut() {
                    oc.scroll = oc.scroll.saturating_sub(1);
                } else if let Some(Overlay::Skills(s)) = self.overlay.as_mut() {
                    s.selected = (s.selected + 1).min(s.skills.len().saturating_sub(1));
                }
            }
            KeyCode::Left => {
                if let Some(Overlay::Openclaw(oc)) = self.overlay.as_mut() {
                    oc.tab = match oc.tab {
                        OpenclawTab::Overview => OpenclawTab::Overview,
                        OpenclawTab::Logs => OpenclawTab::Overview,
                        OpenclawTab::Events => OpenclawTab::Logs,
                    };
                    oc.scroll = 0;
                }
            }
            KeyCode::Right => {
                if let Some(Overlay::Openclaw(oc)) = self.overlay.as_mut() {
                    oc.tab = match oc.tab {
                        OpenclawTab::Overview => OpenclawTab::Logs,
                        OpenclawTab::Logs => OpenclawTab::Events,
                        OpenclawTab::Events => OpenclawTab::Events,
                    };
                    oc.scroll = 0;
                }
            }
            KeyCode::Char('1') | KeyCode::Char('o') | KeyCode::Char('O') => {
                if let Some(Overlay::Openclaw(oc)) = self.overlay.as_mut() {
                    oc.tab = OpenclawTab::Overview;
                    oc.scroll = 0;
                }
            }
            KeyCode::Char('2') | KeyCode::Char('l') | KeyCode::Char('L') => {
                if let Some(Overlay::Openclaw(oc)) = self.overlay.as_mut() {
                    oc.tab = OpenclawTab::Logs;
                    oc.scroll = 0;
                }
            }
            KeyCode::Char('3') | KeyCode::Char('e') | KeyCode::Char('E') => {
                if matches!(self.overlay, Some(Overlay::Skills(_))) {
                    let _ = self.set_selected_skill_enabled(true).await;
                } else if let Some(Overlay::Openclaw(oc)) = self.overlay.as_mut() {
                    oc.tab = OpenclawTab::Events;
                    oc.scroll = 0;
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if matches!(self.overlay, Some(Overlay::Skills(_))) {
                    let _ = self.set_selected_skill_enabled(false).await;
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                if matches!(self.overlay, Some(Overlay::Skills(_))) {
                    self.open_skills_add_overlay();
                }
            }
            KeyCode::Enter => match self.overlay.clone() {
                Some(Overlay::ProviderPicker(p)) => {
                    let Some(selected) = p.providers.get(p.selected) else {
                        self.overlay = None;
                        return Ok(());
                    };
                    let Some(gateway) = self.gateway.clone() else {
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: "Gateway not connected.".to_string(),
                        });
                        self.overlay = None;
                        return Ok(());
                    };

                    let resp = gateway
                        .request(
                            "provider.select",
                            ProviderSelectParams {
                                provider: selected.name.clone(),
                            },
                        )
                        .await?;
                    self.last_response = Some(resp.clone());
                    if let Some(err) = resp.error {
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!("provider.select error: {}", err.message),
                        });
                    } else if let Some(result) = resp.result {
                        let parsed: Option<drbot_protocol::ProviderSelectResult> =
                            serde_json::from_value(result).ok();
                        let name = parsed
                            .as_ref()
                            .map(|r| r.provider.name.clone())
                            .unwrap_or_else(|| selected.name.clone());
                        self.active_provider = Some(name.clone());
                        self.active_provider_status = Some("active".to_string());
                        // Avoid cross-provider model mismatches.
                        self.model = None;
                        self.status = format!("Provider selected: {}", name);
                        self.messages.push_back(ChatMessage {
                            role: MessageRole::System,
                            content: format!(
                                "Provider set to {}. Model reset to provider default.",
                                name
                            ),
                        });

                        if let Some(session_id) = self.session_id {
                            let update_resp = gateway
                                .request(
                                    "session.update",
                                    SessionUpdateParams {
                                        session_id,
                                        provider: Some(name.clone()),
                                        clear_model: true,
                                        ..Default::default()
                                    },
                                )
                                .await?;
                            self.last_response = Some(update_resp.clone());
                            if let Some(err) = update_resp.error {
                                self.messages.push_back(ChatMessage {
                                    role: MessageRole::System,
                                    content: format!("session.update error: {}", err.message),
                                });
                            }
                        }
                    }
                    self.overlay = None;
                }
                Some(Overlay::ModelPicker(p)) => {
                    let Some(selected) = p.models.get(p.selected) else {
                        self.overlay = None;
                        return Ok(());
                    };
                    if selected.id == "(default)" {
                        self.model = None;
                        self.status = "Model: (default)".to_string();
                    } else {
                        self.model = Some(selected.id.clone());
                        self.status = format!("Model set: {}", selected.id);
                    }

                    if let (Some(gateway), Some(session_id)) =
                        (self.gateway.clone(), self.session_id)
                    {
                        let params = if selected.id == "(default)" {
                            SessionUpdateParams {
                                session_id,
                                clear_model: true,
                                ..Default::default()
                            }
                        } else {
                            SessionUpdateParams {
                                session_id,
                                model: Some(selected.id.clone()),
                                ..Default::default()
                            }
                        };

                        let resp = gateway.request("session.update", params).await?;
                        self.last_response = Some(resp.clone());
                        if let Some(err) = resp.error {
                            self.messages.push_back(ChatMessage {
                                role: MessageRole::System,
                                content: format!("session.update error: {}", err.message),
                            });
                        }
                    }
                    self.overlay = None;
                }
                Some(Overlay::SessionPicker(p)) => {
                    let Some(selected) = p.sessions.get(p.selected) else {
                        self.overlay = None;
                        return Ok(());
                    };
                    let session_id = selected.id;
                    self.overlay = None;
                    let _ = self.load_session(session_id).await;
                }
                Some(Overlay::FirstRun(fr)) => {
                    let providers = fr.providers;
                    let selected = providers
                        .iter()
                        .position(|p| p.status == "available" || p.status.starts_with("active"))
                        .unwrap_or(0);
                    self.overlay = Some(Overlay::ProviderPicker(ProviderPicker {
                        providers,
                        selected,
                    }));
                }
                Some(Overlay::Openclaw(_)) => {}
                Some(Overlay::Skills(_)) => {
                    let _ = self.toggle_selected_skill().await;
                }
                Some(Overlay::SkillsAdd(_)) => {}
                Some(Overlay::ToolApproval(_)) => {
                    // Handled by the early-return ToolApproval block above.
                }
                None => {}
            },
            _ => {}
        }
        Ok(())
    }

    async fn refresh_providers(&mut self, open_picker: bool) -> Result<()> {
        let Some(gateway) = self.gateway.clone() else {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "Gateway not connected.".to_string(),
            });
            return Ok(());
        };

        let resp = gateway
            .request(
                "provider.list",
                drbot_protocol::ProviderListParams::default(),
            )
            .await?;
        self.last_response = Some(resp.clone());
        if let Some(err) = resp.error {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("provider.list error: {}", err.message),
            });
            return Ok(());
        }

        let mut providers = resp
            .result
            .and_then(|v| serde_json::from_value::<drbot_protocol::ProviderListResult>(v).ok())
            .map(|r| r.providers)
            .unwrap_or_default();

        let active = providers.iter().find(|p| p.status.starts_with("active"));
        self.active_provider = active.map(|p| p.name.clone());
        self.active_provider_status = active.map(|p| p.status.clone());

        if open_picker {
            let has_available = providers
                .iter()
                .any(|p| p.status == "available" || p.status.starts_with("active"));
            if !has_available {
                self.status = "No providers available".to_string();
                self.overlay = Some(Overlay::FirstRun(FirstRunOverlay { providers }));
                return Ok(());
            }

            // Offer `auto` as a convenience option in the picker so users can re-run the
            // "best provider" selection without typing `/provider auto`.
            if !providers.iter().any(|p| p.name == "auto") {
                providers.insert(
                    0,
                    drbot_protocol::ProviderInfo {
                        name: "auto".to_string(),
                        status: "available".to_string(),
                        models: vec![],
                    },
                );
            }

            let selected = if let Some(idx) = providers
                .iter()
                .position(|p| p.status.starts_with("active"))
            {
                idx
            } else {
                let preferred = ["auto", "claude-cli", "codex-cli", "codex-oss", "ollama"];
                preferred
                    .iter()
                    .find_map(|name| {
                        providers.iter().position(|p| {
                            p.name == *name
                                && (p.status == "available" || p.status.starts_with("active"))
                        })
                    })
                    .unwrap_or(0)
            };
            self.overlay = Some(Overlay::ProviderPicker(ProviderPicker {
                providers,
                selected,
            }));
        }

        Ok(())
    }

    fn openclaw_ws_url(&self) -> Option<String> {
        let url = self.config.gateway_url.as_deref()?.trim();
        if url.is_empty() {
            return None;
        }
        if url.ends_with("/ws") {
            let base = &url[..url.len().saturating_sub(3)];
            return Some(format!("{}{}", base, "/openclaw/ws"));
        }
        None
    }

    async fn ensure_openclaw_connected(&mut self) -> Result<()> {
        if self.openclaw.is_some() {
            return Ok(());
        }
        let Some(url) = self.openclaw_ws_url() else {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "OpenClaw URL not available (expected gateway URL ending with /ws)."
                    .to_string(),
            });
            return Ok(());
        };

        let auth = self.config.gateway_auth_token.as_deref();
        match OpenclawClient::connect(&url, auth).await {
            Ok((client, hello, ev_rx)) => {
                self.openclaw = Some(client);
                self.openclaw_rx = Some(ev_rx);
                self.openclaw_health = Some(hello.snapshot.health.clone());
                self.openclaw_hello = Some(hello);
            }
            Err(e) => {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: format!("OpenClaw connect failed: {}", e),
                });
            }
        }

        Ok(())
    }

    async fn refresh_openclaw_health(&mut self) -> Result<()> {
        self.ensure_openclaw_connected().await?;
        let Some(client) = self.openclaw.clone() else {
            return Ok(());
        };

        let resp = client
            .request("health", Option::<serde_json::Value>::None)
            .await?;
        if !resp.ok {
            let msg = resp
                .error
                .as_ref()
                .map(|e| e.message.as_str())
                .unwrap_or("health failed");
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("OpenClaw health error: {}", msg),
            });
            return Ok(());
        }

        if let Some(payload) = resp.payload {
            self.openclaw_health = Some(payload);
        }

        Ok(())
    }

    async fn refresh_openclaw_logs(&mut self) -> Result<()> {
        self.ensure_openclaw_connected().await?;
        let Some(client) = self.openclaw.clone() else {
            return Ok(());
        };

        let mut params = serde_json::json!({ "limit": 250u64, "maxBytes": 200_000u64 });
        if let Some(cursor) = self.openclaw_logs_cursor {
            params["cursor"] = serde_json::json!(cursor);
        }

        let resp = client.request("logs.tail", Some(params)).await?;
        if !resp.ok {
            let msg = resp
                .error
                .as_ref()
                .map(|e| e.message.as_str())
                .unwrap_or("logs.tail failed");
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("OpenClaw logs.tail error: {}", msg),
            });
            return Ok(());
        }

        let Some(payload) = resp.payload else {
            return Ok(());
        };

        let cursor = payload.get("cursor").and_then(|v| v.as_u64());
        if let Some(cursor) = cursor {
            self.openclaw_logs_cursor = Some(cursor);
        }

        self.openclaw_logs = payload
            .get("lines")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(())
    }

    async fn refresh_openclaw_skills(
        &mut self,
        open_overlay: bool,
        announce: bool,
    ) -> Result<()> {
        self.ensure_openclaw_connected().await?;
        let Some(client) = self.openclaw.clone() else {
            return Ok(());
        };

        let resp = client
            .request("skills.status", Option::<serde_json::Value>::None)
            .await?;
        if !resp.ok {
            let msg = resp
                .error
                .as_ref()
                .map(|e| e.message.as_str())
                .unwrap_or("skills.status failed");
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("OpenClaw skills.status error: {}", msg),
            });
            return Ok(());
        }

        let Some(payload) = resp.payload else {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "skills.status returned no payload".to_string(),
            });
            return Ok(());
        };

        let report: OpenclawSkillStatusReport = match serde_json::from_value(payload) {
            Ok(v) => v,
            Err(err) => {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: format!("skills.status parse error: {}", err),
                });
                return Ok(());
            }
        };

        if announce {
            let mut lines = Vec::new();
            lines.push(format!("Skills ({})", report.skills.len()));
            for skill in report.skills.iter() {
                let status = self.format_skill_status_flags(skill);
                lines.push(format!(
                    "- {} ({}) [{}]",
                    skill.name, skill.skill_key, status
                ));
            }
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: lines.join("\n"),
            });
        }

        if open_overlay {
            let selected = report
                .skills
                .iter()
                .position(|s| !s.disabled)
                .unwrap_or(0);
            self.overlay = Some(Overlay::Skills(SkillsOverlay {
                skills: report.skills,
                selected,
                workspace_dir: report.workspace_dir,
                managed_dir: report.managed_skills_dir,
                snapshot_version: report.snapshot_version,
            }));
        }

        Ok(())
    }

    async fn openclaw_skills_update(
        &mut self,
        skill_key: &str,
        enabled: Option<bool>,
        url: Option<String>,
        fetch_relative_docs: Option<bool>,
    ) -> Result<()> {
        self.ensure_openclaw_connected().await?;
        let Some(client) = self.openclaw.clone() else {
            return Ok(());
        };

        let mut params = serde_json::json!({ "skillKey": skill_key });
        if let Some(enabled) = enabled {
            params["enabled"] = serde_json::json!(enabled);
        }
        if let Some(url) = url {
            params["url"] = serde_json::json!(url);
        }
        if let Some(fetch_relative_docs) = fetch_relative_docs {
            params["fetchRelativeDocs"] = serde_json::json!(fetch_relative_docs);
        }

        let resp = client.request("skills.update", Some(params)).await?;
        if !resp.ok {
            let msg = resp
                .error
                .as_ref()
                .map(|e| e.message.as_str())
                .unwrap_or("skills.update failed");
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("OpenClaw skills.update error: {}", msg),
            });
            return Ok(());
        }

        self.messages.push_back(ChatMessage {
            role: MessageRole::System,
            content: format!("Skill updated: {}", skill_key),
        });
        Ok(())
    }

    async fn open_skills_overlay(&mut self) -> Result<()> {
        let _ = self.refresh_openclaw_skills(true, false).await;
        Ok(())
    }

    fn open_skills_add_overlay(&mut self) {
        self.overlay = Some(Overlay::SkillsAdd(SkillsAddOverlay {
            input: String::new(),
            cursor_pos: 0,
        }));
    }

    async fn submit_skills_add_overlay(&mut self) -> Result<()> {
        let raw = match self.overlay.as_ref() {
            Some(Overlay::SkillsAdd(s)) => s.input.trim().to_string(),
            _ => String::new(),
        };
        if raw.is_empty() {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "Enter: <skillKey> <url>".to_string(),
            });
            return Ok(());
        }

        let mut parts = raw.split_whitespace();
        let Some(skill_key) = parts.next() else {
            return Ok(());
        };
        let url = parts.collect::<Vec<&str>>().join(" ");
        if url.is_empty() {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "Missing url. Usage: <skillKey> <url>".to_string(),
            });
            return Ok(());
        }

        self.openclaw_skills_update(
            skill_key,
            Some(true),
            Some(url),
            Some(true),
        )
        .await?;
        let _ = self.refresh_openclaw_skills(true, false).await;
        Ok(())
    }

    async fn toggle_selected_skill(&mut self) -> Result<()> {
        let (skill_key, enable) = match self.overlay.as_ref() {
            Some(Overlay::Skills(s)) => s
                .skills
                .get(s.selected)
                .map(|entry| (entry.skill_key.clone(), entry.disabled))
                .unwrap_or_else(|| ("".to_string(), false)),
            _ => ("".to_string(), false),
        };
        if skill_key.is_empty() {
            return Ok(());
        }
        self.set_selected_skill_enabled(enable).await
    }

    async fn set_selected_skill_enabled(&mut self, enabled: bool) -> Result<()> {
        let skill_key = match self.overlay.as_ref() {
            Some(Overlay::Skills(s)) => s
                .skills
                .get(s.selected)
                .map(|entry| entry.skill_key.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        if skill_key.is_empty() {
            return Ok(());
        }

        self.openclaw_skills_update(&skill_key, Some(enabled), None, None)
            .await?;
        let _ = self.refresh_openclaw_skills(true, false).await;
        Ok(())
    }

    pub(crate) fn format_skill_status_flags(&self, skill: &OpenclawSkillStatusEntry) -> String {
        let mut flags: Vec<String> = Vec::new();
        if skill.disabled {
            flags.push("disabled".to_string());
        } else {
            flags.push("enabled".to_string());
        }
        if skill.blocked_by_allowlist {
            flags.push("blocked".to_string());
        }
        if skill.eligible {
            flags.push("eligible".to_string());
        } else {
            flags.push("ineligible".to_string());
        }
        if !skill.missing.bins.is_empty()
            || !skill.missing.any_bins.is_empty()
            || !skill.missing.env.is_empty()
            || !skill.missing.config.is_empty()
            || !skill.missing.os.is_empty()
        {
            let mut missing = Vec::new();
            if !skill.missing.bins.is_empty() || !skill.missing.any_bins.is_empty() {
                missing.push("bins");
            }
            if !skill.missing.env.is_empty() {
                missing.push("env");
            }
            if !skill.missing.config.is_empty() {
                missing.push("config");
            }
            if !skill.missing.os.is_empty() {
                missing.push("os");
            }
            flags.push(format!("missing:{}", missing.join(",")));
        }
        flags.join(", ")
    }

    async fn open_openclaw_overlay(&mut self) -> Result<()> {
        let _ = self.ensure_openclaw_connected().await;
        let _ = self.refresh_openclaw_logs().await;

        self.overlay = Some(Overlay::Openclaw(OpenclawOverlay {
            tab: OpenclawTab::Overview,
            scroll: 0,
        }));
        Ok(())
    }

    async fn open_model_picker(&mut self) -> Result<()> {
        let Some(gateway) = self.gateway.clone() else {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "Gateway not connected.".to_string(),
            });
            return Ok(());
        };

        let resp = gateway
            .request("provider.models", ProviderModelsParams { provider: None })
            .await?;
        self.last_response = Some(resp.clone());
        if let Some(err) = resp.error {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("provider.models error: {}", err.message),
            });
            return Ok(());
        }

        let mut models = resp
            .result
            .and_then(|v| serde_json::from_value::<ProviderModelsResult>(v).ok())
            .map(|r| r.models)
            .unwrap_or_default();

        models.insert(
            0,
            drbot_protocol::ModelInfo {
                id: "(default)".to_string(),
                name: "Use provider default".to_string(),
                provider: self.active_provider.clone().unwrap_or_default(),
                context_window: 0,
                max_output_tokens: None,
            },
        );

        let selected = self
            .model
            .as_deref()
            .and_then(|id| models.iter().position(|m| m.id == id))
            .unwrap_or(0);

        self.overlay = Some(Overlay::ModelPicker(ModelPicker { models, selected }));
        Ok(())
    }

    async fn open_session_picker(&mut self) -> Result<()> {
        let Some(gateway) = self.gateway.clone() else {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "Gateway not connected.".to_string(),
            });
            return Ok(());
        };

        let resp = gateway
            .request(
                "session.list",
                drbot_protocol::SessionListParams {
                    limit: Some(50),
                    offset: None,
                    state: Some("all".to_string()),
                },
            )
            .await?;
        self.last_response = Some(resp.clone());
        if let Some(err) = resp.error {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("session.list error: {}", err.message),
            });
            return Ok(());
        }

        let sessions = resp
            .result
            .and_then(|v| serde_json::from_value::<drbot_protocol::SessionListResult>(v).ok())
            .map(|r| r.sessions)
            .unwrap_or_default();

        let selected = self
            .session_id
            .and_then(|id| sessions.iter().position(|s| s.id == id))
            .unwrap_or(0);

        self.overlay = Some(Overlay::SessionPicker(SessionPicker { sessions, selected }));
        Ok(())
    }

    async fn load_session(&mut self, session_id: uuid::Uuid) -> Result<()> {
        let Some(gateway) = self.gateway.clone() else {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "Gateway not connected.".to_string(),
            });
            return Ok(());
        };

        let resp = gateway
            .request(
                "session.get",
                drbot_protocol::SessionGetParams { session_id },
            )
            .await?;
        self.last_response = Some(resp.clone());
        if let Some(err) = resp.error {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("session.get error: {}", err.message),
            });
            return Ok(());
        }

        let Some(result) = resp.result else {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: "session.get returned no result".to_string(),
            });
            return Ok(());
        };

        let parsed = match serde_json::from_value::<drbot_protocol::SessionGetResult>(result) {
            Ok(p) => p,
            Err(e) => {
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: format!("Failed to parse session.get result: {}", e),
                });
                return Ok(());
            }
        };

        self.session_id = Some(session_id);
        let mut provider_select_failed = false;
        if let Some(provider_name) = parsed
            .session
            .provider
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let resp = gateway
                .request(
                    "provider.select",
                    ProviderSelectParams {
                        provider: provider_name.to_string(),
                    },
                )
                .await?;
            self.last_response = Some(resp.clone());
            if let Some(err) = resp.error {
                provider_select_failed = true;
                self.messages.push_back(ChatMessage {
                    role: MessageRole::System,
                    content: format!(
                        "Failed to select session provider '{}': {}",
                        provider_name, err.message
                    ),
                });
            } else {
                self.active_provider = Some(provider_name.to_string());
                self.active_provider_status = Some("active".to_string());
            }
        }

        // Align local model override with the session model (if any). If the provider failed to
        // apply, drop the model override to avoid a mismatch.
        self.model = if provider_select_failed {
            None
        } else {
            parsed.session.model.clone()
        };

        self.messages.clear();
        self.scroll_offset = 0;

        let title = parsed
            .session
            .title
            .clone()
            .unwrap_or_else(|| "(untitled)".to_string());
        let provider = parsed
            .session
            .provider
            .clone()
            .unwrap_or_else(|| "(default)".to_string());
        let model = parsed
            .session
            .model
            .clone()
            .unwrap_or_else(|| "(default)".to_string());
        let total = parsed.messages.len();
        let keep = self.config.max_history.max(1);
        let start = total.saturating_sub(keep);

        if start > 0 {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!(
                    "Loaded session: {}  ({})\nProvider: {}  Model: {}\nShowing last {} of {} messages. Title: {}",
                    parsed.session.id, parsed.session.state, provider, model, keep, total, title
                ),
            });
        } else {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!(
                    "Loaded session: {}  ({})\nProvider: {}  Model: {}\nMessages: {}  Title: {}",
                    parsed.session.id, parsed.session.state, provider, model, total, title
                ),
            });
        }

        if let Some(prompt) = parsed
            .system_prompt
            .as_deref()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
        {
            self.messages.push_back(ChatMessage {
                role: MessageRole::System,
                content: format!("System prompt: {}", prompt),
            });
        }

        for m in parsed.messages.into_iter().skip(start) {
            let (role, content) = render_core_message(&m);
            if content.trim().is_empty() {
                continue;
            }
            self.messages.push_back(ChatMessage { role, content });
        }

        self.status = format!("Session: {}", session_id);
        self.remember_last_session_for_current_project_best_effort(session_id);
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

fn render_core_message(msg: &drbot_core::message::Message) -> (MessageRole, String) {
    use drbot_core::message::{Content, Role};

    let role = match msg.role {
        Role::User => MessageRole::User,
        Role::Assistant => MessageRole::Assistant,
        Role::System => MessageRole::System,
    };

    let mut out = String::new();
    for block in &msg.content {
        let line = match block {
            Content::Text { text } => text.clone(),
            Content::Image { .. } => "[image]".to_string(),
            Content::File {
                name, mime_type, ..
            } => format!("[file: {}  {}]", name, mime_type),
            Content::Audio { .. } => "[audio]".to_string(),
            Content::ToolUse { name, .. } => format!("[tool use: {}]", name),
            Content::ToolResult {
                content, is_error, ..
            } => {
                if *is_error {
                    format!("[tool error] {}", content)
                } else {
                    format!("[tool result] {}", content)
                }
            }
        };

        if line.trim().is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&line);
    }

    if out.is_empty() {
        out = "[empty message]".to_string();
    }

    (role, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_app_creation() {
        let config = AppConfig::default();
        let app = App::new(config).await.unwrap();
        assert!(!app.should_quit());
        assert!(!app.messages.is_empty());
    }

    #[tokio::test]
    async fn test_command_handling() {
        let config = AppConfig::default();
        let mut app = App::new(config).await.unwrap();

        app.input = "/help".to_string();
        app.cursor_pos = 5;
        app.submit_message().await.unwrap();

        assert!(app.messages.len() >= 1);
        assert!(app.messages.back().unwrap().content.contains("Commands:"));
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
