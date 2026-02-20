//! HTTP router for the gateway.

use crate::state::GatewayState;
use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        ConnectInfo, DefaultBodyLimit, State,
    },
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use drbot_core::message::Message;
use drbot_core::session::Session;
use drbot_providers::{ChatOptions, Provider, StreamEvent as ProviderStreamEvent};
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Create the HTTP router.
pub fn create_router(state: GatewayState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let hooks_base = state.config().hooks.path.trim();
    let hooks_base = if hooks_base.is_empty() {
        "/hooks"
    } else {
        hooks_base
    };
    let hooks_base = hooks_base.trim_end_matches('/');
    let hooks_base = if hooks_base == "/" {
        "/hooks"
    } else {
        hooks_base
    };
    let hooks_base = hooks_base.to_string();
    let hooks_max_body = state
        .config()
        .hooks
        .max_body_bytes
        .unwrap_or(256_000)
        .clamp(1, 5_000_000) as usize;
    let hooks_router = Router::new()
        .route("/wake", post(crate::openclaw_webhooks::hooks_wake_handler))
        .route(
            "/agent",
            post(crate::openclaw_webhooks::hooks_agent_handler),
        )
        .layer(DefaultBodyLimit::max(hooks_max_body));

    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/ws", get(websocket_handler))
        .route("/openclaw/ws", get(openclaw_websocket_handler))
        .route("/tools/invoke", post(tools_invoke_handler))
        .nest(hooks_base.as_str(), hooks_router)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(
        r#"<!DOCTYPE html>
<html>
<head>
    <title>drbot Gateway</title>
    <style>
        body { font-family: system-ui, sans-serif; max-width: 800px; margin: 0 auto; padding: 2rem; }
        code { background: #f0f0f0; padding: 0.2rem 0.4rem; border-radius: 3px; }
    </style>
</head>
<body>
    <h1>drbot Gateway</h1>
    <p>WebSocket endpoint: <code>ws://localhost:18789/ws</code></p>
    <p>OpenClaw v3 endpoint: <code>ws://localhost:18789/openclaw/ws</code></p>
    <h2>Status</h2>
    <ul>
        <li>Health check: <code>GET /health</code></li>
        <li>WebSocket: <code>GET /ws</code></li>
        <li>OpenClaw WebSocket: <code>GET /openclaw/ws</code></li>
    </ul>
</body>
</html>"#,
    )
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

fn env_nonempty(var: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn normalize_http_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Ollama commonly uses OLLAMA_HOST like "127.0.0.1:11434".
    if trimmed.contains("://") {
        Some(trimmed.to_string())
    } else {
        Some(format!("http://{}", trimmed))
    }
}

fn ollama_base_url(config: &drbot_core::Config) -> String {
    if let Some(v) = env_nonempty("DRBOT_OLLAMA_URL").and_then(|v| normalize_http_url(&v)) {
        return v;
    }
    if let Some(v) = env_nonempty("OLLAMA_HOST").and_then(|v| normalize_http_url(&v)) {
        return v;
    }
    config
        .providers
        .ollama
        .as_ref()
        .map(|c| c.url.trim().to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| drbot_ollama::DEFAULT_BASE_URL.to_string())
}

async fn fetch_ollama_tags(
    base_url: &str,
    timeout: Duration,
) -> Option<(bool, Option<serde_json::Value>)> {
    let tags_url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    tokio::time::timeout(timeout, async {
        let resp = reqwest::Client::new().get(&tags_url).send().await.ok()?;
        let ok = resp.status().is_success();
        let json = if ok {
            resp.json::<serde_json::Value>().await.ok()
        } else {
            None
        };
        Some((ok, json))
    })
    .await
    .ok()
    .flatten()
}

async fn init_provider_auto_best_effort(
    state: &GatewayState,
) -> std::result::Result<std::sync::Arc<dyn Provider>, String> {
    // Auto selection prefers cost-savers first:
    // - CLI tools (claude-cli / codex-cli) if installed on PATH
    // - Ollama if running (even without config)
    // - API providers as fallback
    //
    // You can disable CLI auto-detect with: DRBOT_AUTO_DISABLE_CLI_PRESETS=1
    let cli_presets_disabled = env_flag_enabled("DRBOT_AUTO_DISABLE_CLI_PRESETS");

    if !cli_presets_disabled {
        if let Ok(p) = crate::state::try_init_provider_named(state.config(), "claude-cli") {
            return Ok(p);
        }
        if let Ok(p) = crate::state::try_init_provider_named(state.config(), "codex-cli") {
            return Ok(p);
        }
    }

    let base_url = ollama_base_url(state.config());
    if let Some((reachable, _json)) = fetch_ollama_tags(&base_url, Duration::from_millis(350)).await
    {
        if reachable {
            let mut provider = drbot_ollama::OllamaProvider::new().with_base_url(base_url);
            let model = state
                .config()
                .providers
                .ollama
                .as_ref()
                .and_then(|c| c.default_model.clone())
                .or_else(|| state.config().providers.default_model.clone());
            if let Some(model) = model {
                provider = provider.with_default_model(model);
            }
            return Ok(std::sync::Arc::new(provider) as std::sync::Arc<dyn Provider>);
        }
    }

    // Fall back to configured API/custom providers.
    if let Ok(p) = crate::state::try_init_provider_named(state.config(), "anthropic") {
        return Ok(p);
    }
    if let Ok(p) = crate::state::try_init_provider_named(state.config(), "openai") {
        return Ok(p);
    }
    for cfg in state.config().providers.openai_compatible.iter() {
        if let Ok(p) = crate::state::try_init_provider_named(state.config(), &cfg.name) {
            return Ok(p);
        }
    }
    for cfg in state.config().providers.cli.iter() {
        if let Ok(p) = crate::state::try_init_provider_named(state.config(), &cfg.name) {
            return Ok(p);
        }
    }

    Err("no providers available (try drbot wizard)".to_string())
}

async fn try_init_provider_named_best_effort(
    state: &GatewayState,
    name: &str,
) -> std::result::Result<std::sync::Arc<dyn Provider>, String> {
    let requested = name.trim();
    if requested.is_empty() {
        return Err("provider name is empty".to_string());
    }

    if requested.eq_ignore_ascii_case("auto") {
        return init_provider_auto_best_effort(state).await;
    }

    if requested.eq_ignore_ascii_case("ollama") || requested.eq_ignore_ascii_case("local") {
        if let Ok(p) = crate::state::try_init_provider_named(state.config(), "ollama") {
            return Ok(p);
        }

        // Allow configless Ollama if it's actually running.
        let base_url = ollama_base_url(state.config());
        let reachable = fetch_ollama_tags(&base_url, Duration::from_millis(350))
            .await
            .map(|(ok, _)| ok)
            .unwrap_or(false);
        if !reachable {
            return Err(format!(
                "ollama not reachable at {} (start Ollama or run drbot wizard)",
                base_url.trim()
            ));
        }

        let mut provider = drbot_ollama::OllamaProvider::new().with_base_url(base_url);
        let model = state
            .config()
            .providers
            .ollama
            .as_ref()
            .and_then(|c| c.default_model.clone())
            .or_else(|| state.config().providers.default_model.clone());
        if let Some(model) = model {
            provider = provider.with_default_model(model);
        }
        return Ok(std::sync::Arc::new(provider) as std::sync::Arc<dyn Provider>);
    }

    crate::state::try_init_provider_named(state.config(), requested)
}

async fn health(State(state): State<GatewayState>) -> impl IntoResponse {
    let clients = state.client_count().await;
    let uptime = state.uptime_secs();
    let provider = if state.has_provider() {
        "configured"
    } else {
        "not_configured"
    };
    let sessions = if state.has_session_store() {
        "configured"
    } else {
        "not_configured"
    };
    format!(
        r#"{{"status":"healthy","clients":{},"uptime_secs":{},"provider":"{}","sessions":"{}"}}"#,
        clients, uptime, provider, sessions
    )
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<GatewayState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, addr))
}

async fn openclaw_websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<GatewayState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| crate::openclaw::handle_socket(socket, state, addr))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolsInvokeBody {
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    args: Option<serde_json::Value>,
    #[serde(default)]
    session_key: Option<String>,
    #[serde(default)]
    dry_run: Option<bool>,
}

fn parse_bearer_token(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    let prefix = "bearer ";
    if raw.len() >= prefix.len() && raw[..prefix.len()].eq_ignore_ascii_case(prefix) {
        let token = raw[prefix.len()..].trim();
        if token.is_empty() {
            None
        } else {
            Some(token)
        }
    } else {
        None
    }
}

async fn tools_invoke_handler(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(body): Json<ToolsInvokeBody>,
) -> impl IntoResponse {
    // Auth matches the gateway token policy (if configured).
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_bearer_token)
        .unwrap_or("");
    if !state.validate_token(token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "ok": false,
                "error": { "type": "unauthorized", "message": "unauthorized" }
            })),
        )
            .into_response();
    }

    let tool_name = body.tool.unwrap_or_default();
    let tool_name = tool_name.trim().to_string();
    if tool_name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": { "type": "invalid_request", "message": "tools.invoke requires body.tool" }
            })),
        )
            .into_response();
    }

    let mut args = body.args.unwrap_or_else(|| serde_json::json!({}));
    if !args.is_object() {
        args = serde_json::json!({});
    }
    if let Some(action) = body
        .action
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        if let Some(map) = args.as_object_mut() {
            if !map.contains_key("action") {
                map.insert("action".to_string(), serde_json::json!(action));
            }
        }
    }

    // Minimal tool registry (OpenClaw parity enough for interop + hackathon skills).
    let mut tools: std::collections::HashMap<String, std::sync::Arc<dyn drbot_agents::AgentTool>> =
        std::collections::HashMap::new();
    let raw_session_key = body
        .session_key
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("main");
    let canonical_session_key = crate::openclaw::canonicalize_openclaw_session_key(
        crate::openclaw_paths::DEFAULT_AGENT_ID,
        raw_session_key,
    );
    let session_key = canonical_session_key.trim();
    let session_key_opt = if session_key.is_empty() {
        None
    } else {
        Some(session_key)
    };
    let agent_id = crate::openclaw::openclaw_session_key_agent_id(
        session_key,
        crate::openclaw_paths::DEFAULT_AGENT_ID,
    );
    let tool_filter = crate::openclaw::resolve_openclaw_effective_tool_filter(
        &state,
        session_key,
        None,
        &agent_id,
    );

    let workspace_root = crate::openclaw::resolve_agent_workspace_dir_for_state(&state, &agent_id);
    let mut builtin_options = drbot_agents::BuiltinToolsOptions::default();
    if crate::openclaw_exec_approvals::exec_approvals_auto_allow_skills(Some(&agent_id)) {
        let platform = crate::openclaw_skills::resolve_runtime_platform();
        let dirs = [workspace_root.clone()];
        let bins = crate::openclaw_skills::collect_required_skill_bins_for_platform(
            &dirs,
            state.config(),
            platform,
        );
        builtin_options.bash_extra_allowed_prefixes = bins;
    }
    let builtin = match drbot_agents::BuiltinTools::all_with_options(
        workspace_root.clone(),
        builtin_options,
    ) {
        Ok(v) => v,
        Err(err) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": { "type": "unavailable", "message": format!("failed to initialize tools: {}", err) }
                })),
            )
                .into_response();
        }
    };

    let exec_ask = crate::openclaw::resolve_openclaw_session_exec_ask_mode(&state, session_key);
    let tool_mode = |tool_name: &str, baseline: crate::openclaw::OpenclawExecAskMode| {
        let tool_policy = crate::openclaw::resolve_openclaw_session_tool_policy_mode(
            &state,
            session_key,
            tool_name,
        )
        .unwrap_or(crate::openclaw::OpenclawExecAskMode::Allow);
        crate::openclaw::merge_openclaw_exec_ask_modes(tool_policy, baseline)
    };
    for tool in builtin {
        if tool.name() == "http" || tool.name() == "exec" {
            continue;
        }
        if !tool_filter.is_allowed(tool.name()) {
            continue;
        }
        let baseline = if matches!(
            tool.name(),
            "bash" | "exec" | "write_file" | "write" | "edit" | "apply_patch" | "http"
        ) {
            exec_ask
        } else {
            crate::openclaw::OpenclawExecAskMode::Allow
        };
        let mode = tool_mode(tool.name(), baseline);
        let Some(tool) = crate::openclaw_agent_tools::apply_openclaw_tool_policy_to_tool(
            state.clone(),
            &agent_id,
            session_key_opt,
            mode,
            tool,
        ) else {
            continue;
        };
        tools.insert(tool.name().to_string(), tool);
    }
    for tool in [
        std::sync::Arc::new(crate::openclaw_agent_tools::AgentsListTool)
            as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::WebFetchTool::new())
            as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::WebSearchTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::McpTool::new(state.clone()))
            as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::BrowserTool::new(state.clone()))
            as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::CanvasTool::new(
            state.clone(),
            workspace_root.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::NodesTool::new_with_context(
            state.clone(),
            workspace_root.clone(),
            &agent_id,
            session_key_opt,
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::ImageTool::new_with_context(
            state.clone(),
            workspace_root.clone(),
            &agent_id,
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::ColosseumRequestTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MoltbookRequestTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MoltbookPostTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MoltbookFeedTool)
            as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MoltbookCommentTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MoltbookVoteTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MoltbookIdentityTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MoltbookSearchTool)
            as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MoltbookFollowTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MoltbookSubscribeTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MoltbookDmTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::SessionsListTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::SessionsHistoryTool::new(
            state.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(
            crate::openclaw_agent_tools::SessionsSendTool::new_with_context(
                state.clone(),
                &agent_id,
                session_key,
            ),
        ) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(
            crate::openclaw_agent_tools::SessionsSpawnTool::new_with_context(
                state.clone(),
                &agent_id,
                session_key,
            ),
        ) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(
            crate::openclaw_agent_tools::SessionStatusTool::new_with_context(
                state.clone(),
                &agent_id,
                session_key,
            ),
        ) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MemorySearchTool::new(
            state.clone(),
            &agent_id,
            workspace_root.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
        std::sync::Arc::new(crate::openclaw_agent_tools::MemoryGetTool::new(
            state.clone(),
            &agent_id,
            workspace_root.clone(),
        )) as std::sync::Arc<dyn drbot_agents::AgentTool>,
    ] {
        if !tool_filter.is_allowed(tool.name()) {
            continue;
        }
        let mode = tool_mode(tool.name(), crate::openclaw::OpenclawExecAskMode::Allow);
        let Some(tool) = crate::openclaw_agent_tools::apply_openclaw_tool_policy_to_tool(
            state.clone(),
            &agent_id,
            session_key_opt,
            mode,
            tool,
        ) else {
            continue;
        };
        tools.insert(tool.name().to_string(), tool);
    }

    if tool_filter.is_allowed("exec") {
        if let Some(tool) = crate::openclaw_agent_tools::apply_openclaw_tool_policy_to_tool(
            state.clone(),
            &agent_id,
            session_key_opt,
            tool_mode("exec", exec_ask),
            std::sync::Arc::new(crate::openclaw_agent_tools::ExecTool::new(
                state.clone(),
                Some(agent_id.clone()),
                workspace_root.clone(),
                Some(canonical_session_key.clone()),
            )),
        ) {
            tools.insert("exec".to_string(), tool);
        }
    }

    if tool_filter.is_allowed("process") {
        if let Some(tool) = crate::openclaw_agent_tools::apply_openclaw_tool_policy_to_tool(
            state.clone(),
            &agent_id,
            session_key_opt,
            tool_mode("process", exec_ask),
            std::sync::Arc::new(crate::openclaw_agent_tools::ProcessTool::new(
                workspace_root.clone(),
                Some(canonical_session_key.clone()),
            )),
        ) {
            tools.insert("process".to_string(), tool);
        }
    }

    let send_policy =
        crate::openclaw::resolve_openclaw_session_send_policy_mode(&state, session_key);

    let message_tool_mode = tool_mode("message", crate::openclaw::OpenclawExecAskMode::Allow);
    if tool_filter.is_allowed("message")
        && !matches!(send_policy, crate::openclaw::OpenclawSendPolicyMode::Deny)
        && !matches!(
            message_tool_mode,
            crate::openclaw::OpenclawExecAskMode::Deny
        )
    {
        let wrapper_mode = if matches!(send_policy, crate::openclaw::OpenclawSendPolicyMode::Ask) {
            crate::openclaw::OpenclawExecAskMode::Allow
        } else {
            message_tool_mode
        };
        if let Some(tool) = crate::openclaw_agent_tools::apply_openclaw_tool_policy_to_tool(
            state.clone(),
            &agent_id,
            session_key_opt,
            wrapper_mode,
            std::sync::Arc::new(
                crate::openclaw_agent_tools::MessageTool::new_with_session_key(
                    state.clone(),
                    Some(canonical_session_key.clone()),
                ),
            ),
        ) {
            tools.insert("message".to_string(), tool);
        }
    }

    let send_tool_mode = tool_mode("send", crate::openclaw::OpenclawExecAskMode::Allow);
    if tool_filter.is_allowed("send")
        && !matches!(send_policy, crate::openclaw::OpenclawSendPolicyMode::Deny)
        && !matches!(send_tool_mode, crate::openclaw::OpenclawExecAskMode::Deny)
    {
        let wrapper_mode = if matches!(send_policy, crate::openclaw::OpenclawSendPolicyMode::Ask) {
            crate::openclaw::OpenclawExecAskMode::Allow
        } else {
            send_tool_mode
        };
        if let Some(tool) = crate::openclaw_agent_tools::apply_openclaw_tool_policy_to_tool(
            state.clone(),
            &agent_id,
            session_key_opt,
            wrapper_mode,
            std::sync::Arc::new(crate::openclaw_agent_tools::SendTool::new_with_session_key(
                state.clone(),
                Some(canonical_session_key.clone()),
            )),
        ) {
            tools.insert("send".to_string(), tool);
        }
    }

    let poll_tool_mode = tool_mode("poll", crate::openclaw::OpenclawExecAskMode::Allow);
    if tool_filter.is_allowed("poll")
        && !matches!(send_policy, crate::openclaw::OpenclawSendPolicyMode::Deny)
        && !matches!(poll_tool_mode, crate::openclaw::OpenclawExecAskMode::Deny)
    {
        let wrapper_mode = if matches!(send_policy, crate::openclaw::OpenclawSendPolicyMode::Ask) {
            crate::openclaw::OpenclawExecAskMode::Allow
        } else {
            poll_tool_mode
        };
        if let Some(tool) = crate::openclaw_agent_tools::apply_openclaw_tool_policy_to_tool(
            state.clone(),
            &agent_id,
            session_key_opt,
            wrapper_mode,
            std::sync::Arc::new(crate::openclaw_agent_tools::PollTool::new_with_session_key(
                state.clone(),
                Some(canonical_session_key.clone()),
            )),
        ) {
            tools.insert("poll".to_string(), tool);
        }
    }
    if tool_filter.is_allowed("cron") {
        if let Some(tool) = crate::openclaw_agent_tools::apply_openclaw_tool_policy_to_tool(
            state.clone(),
            &agent_id,
            session_key_opt,
            tool_mode("cron", crate::openclaw::OpenclawExecAskMode::Allow),
            std::sync::Arc::new(crate::openclaw_agent_tools::CronTool::new_with_session_key(
                state.clone(),
                Some(canonical_session_key.clone()),
            )),
        ) {
            tools.insert("cron".to_string(), tool);
        }
    }
    if tool_filter.is_allowed("gateway") {
        if let Some(tool) = crate::openclaw_agent_tools::apply_openclaw_tool_policy_to_tool(
            state.clone(),
            &agent_id,
            session_key_opt,
            tool_mode("gateway", crate::openclaw::OpenclawExecAskMode::Allow),
            std::sync::Arc::new(
                crate::openclaw_agent_tools::GatewayTool::new_with_session_key(
                    state.clone(),
                    Some(canonical_session_key.clone()),
                ),
            ),
        ) {
            tools.insert("gateway".to_string(), tool);
        }
    }

    let Some(tool) = tools.get(&tool_name) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "ok": false,
                "error": { "type": "not_found", "message": format!("Tool not available: {}", tool_name) }
            })),
        )
            .into_response();
    };

    if body.dry_run.unwrap_or(false) {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "ok": true,
                "result": {
                    "content": [{ "type": "text", "text": "dryRun: true (no tool execution performed)" }],
                    "details": { "tool": tool_name, "args": args }
                }
            })),
        )
            .into_response();
    }

    match tool.execute(args).await {
        Ok(text) => {
            let details = serde_json::from_str::<serde_json::Value>(&text).ok();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "ok": true,
                    "result": {
                        "content": [{ "type": "text", "text": text }],
                        "details": details,
                        "meta": { "tool": tool_name, "sessionKey": body.session_key },
                    }
                })),
            )
                .into_response()
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": { "type": "tool_error", "message": err.to_string() }
            })),
        )
            .into_response(),
    }
}

/// Client context for tracking connection state.
struct ClientContext {
    /// Client connection ID.
    client_id: uuid::Uuid,
    /// Authenticated user ID (set after auth.login).
    user_id: Option<uuid::Uuid>,
    /// Whether the client is authenticated.
    authenticated: bool,
}

impl ClientContext {
    fn new(client_id: uuid::Uuid, auth_required: bool) -> Self {
        Self {
            client_id,
            user_id: None,
            // If no auth required, client is implicitly authenticated
            authenticated: !auth_required,
        }
    }

    /// Get the effective user ID (client_id if not authenticated with user).
    fn effective_user_id(&self) -> uuid::Uuid {
        self.user_id.unwrap_or(self.client_id)
    }
}

async fn handle_socket(socket: WebSocket, state: GatewayState, addr: SocketAddr) {
    use drbot_protocol::{event_types, Event, WsMessage};
    use futures::stream::StreamExt as _;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tracing::{debug, info, warn};

    info!(%addr, "New WebSocket connection");

    let (ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(32);

    let sender_task = tokio::spawn(async move {
        use futures::SinkExt;
        let mut ws_sender = ws_sender;
        while let Some(msg) = rx.recv().await {
            if ws_sender
                .send(axum::extract::ws::Message::Text(msg.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let client_id = uuid::Uuid::new_v4();
    let client_ctx = Arc::new(RwLock::new(ClientContext::new(
        client_id,
        state.auth_required(),
    )));
    let tx_clone = tx.clone();

    let connected_event = Event::new(
        event_types::SYSTEM_CONNECTED,
        drbot_protocol::event::system::ConnectedEvent {
            client_id,
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: drbot_protocol::PROTOCOL_VERSION.to_string(),
        },
    );

    if let Ok(json) = serde_json::to_string(&WsMessage::Event(connected_event)) {
        let _ = tx_clone.send(json).await;
    }

    while let Some(msg) = ws_receiver.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                warn!(%addr, error = %e, "WebSocket error");
                break;
            }
        };

        let text = match msg {
            axum::extract::ws::Message::Text(t) => t.to_string(),
            axum::extract::ws::Message::Binary(b) => match String::from_utf8(b.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            },
            axum::extract::ws::Message::Ping(_) | axum::extract::ws::Message::Pong(_) => continue,
            axum::extract::ws::Message::Close(_) => {
                info!(%addr, "Client disconnected");
                break;
            }
        };

        debug!(%addr, "Received message: {}", text);

        let ws_msg: WsMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                warn!(%addr, error = %e, "Failed to parse message");
                let error_response = WsMessage::error(
                    uuid::Uuid::nil(),
                    drbot_protocol::ErrorCode::ParseError,
                    format!("Failed to parse message: {}", e),
                );
                if let Ok(json) = serde_json::to_string(&error_response) {
                    let _ = tx.send(json).await;
                }
                continue;
            }
        };

        match ws_msg {
            WsMessage::Request(request) => {
                let response = handle_request(&state, &tx, request, &client_ctx).await;
                if let Ok(json) = serde_json::to_string(&response) {
                    let _ = tx.send(json).await;
                }
            }
            WsMessage::Response(_) | WsMessage::Event(_) => {
                warn!(%addr, "Client sent unexpected message type");
            }
        }
    }

    drop(tx);
    let _ = sender_task.await;
    info!(%addr, "Connection closed");
}

async fn handle_request(
    state: &GatewayState,
    tx: &tokio::sync::mpsc::Sender<String>,
    request: drbot_protocol::Request,
    client_ctx: &std::sync::Arc<tokio::sync::RwLock<ClientContext>>,
) -> drbot_protocol::WsMessage {
    use drbot_protocol::{
        event_types, AuthLoginParams, AuthLoginResult, ChatSendParams, ErrorCode, Event, ModelInfo,
        ProviderListResult, ProviderModelsParams, ProviderModelsResult, ProviderSelectParams,
        ProviderSelectResult, SessionClearParams, SessionClearResult, SessionCreateParams,
        SessionCreateResult, SessionDeleteParams, SessionDeleteResult, SessionGetParams,
        SessionGetResult, SessionListParams, SessionListResult, SessionUpdateParams,
        SessionUpdateResult, SystemInfoResult, SystemPingResult, WsMessage,
    };
    use futures::StreamExt;
    use tracing::{debug, error, warn};

    debug!(method = %request.method, "Handling request");

    let ctx = client_ctx.read().await;
    let authenticated = ctx.authenticated;
    drop(ctx);

    if request.method != "auth.login" && state.auth_required() && !authenticated {
        return WsMessage::error(
            request.id,
            ErrorCode::AuthRequired,
            "Authentication required",
        );
    }

    match request.method.as_str() {
        "auth.login" => {
            let params: AuthLoginParams = match serde_json::from_value(request.params.clone()) {
                Ok(p) => p,
                Err(e) => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::InvalidParams,
                        format!("Invalid params: {}", e),
                    );
                }
            };

            if state.validate_token(&params.token) {
                // Derive a stable user ID from the token so sessions persist across reconnects.
                let user_id =
                    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, params.token.as_bytes());
                {
                    let mut ctx = client_ctx.write().await;
                    ctx.authenticated = true;
                    ctx.user_id = Some(user_id);
                }
                WsMessage::success(
                    request.id,
                    AuthLoginResult {
                        success: true,
                        user_id: Some(user_id),
                    },
                )
            } else {
                WsMessage::error(request.id, ErrorCode::PermissionDenied, "Invalid token")
            }
        }

        "chat.send" => {
            let params: ChatSendParams = match serde_json::from_value(request.params.clone()) {
                Ok(p) => p,
                Err(e) => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::InvalidParams,
                        format!("Invalid params: {}", e),
                    );
                }
            };

            // Establish a session id upfront (even if we short-circuit provider calls).
            let session_id = params.session_id.unwrap_or_else(uuid::Uuid::new_v4);
            let message_id = uuid::Uuid::new_v4();

            // Local workspace commands (e.g. `/remember ...`) are handled without calling a provider.
            //
            // This keeps "memory updates" fast, reliable, and available even when no provider is
            // configured.
            let msg_trimmed = params.message.trim_start();
            let msg_lower = msg_trimmed.to_ascii_lowercase();
            let is_remember_cmd =
                msg_lower.starts_with("/remember") || msg_lower.starts_with("remember:");
            let is_forget_cmd =
                msg_lower.starts_with("/forget") || msg_lower.starts_with("forget:");
            let is_memory_cmd = msg_lower == "/memory" || msg_lower == "/mem";
            let is_profile_cmd = msg_lower == "/profile";
            let is_kb_cmd = msg_lower == "/kb"
                || msg_lower.starts_with("/kb ")
                || msg_lower.starts_with("/kb:")
                || msg_lower.starts_with("kb:")
                || msg_lower == "/notes"
                || msg_lower.starts_with("/notes ")
                || msg_lower.starts_with("/notes:")
                || msg_lower.starts_with("notes:");
            let is_local_cmd =
                is_remember_cmd || is_forget_cmd || is_memory_cmd || is_profile_cmd || is_kb_cmd;

            if is_local_cmd {
                let workspace_dir = crate::openclaw::ensure_agent_workspace_bootstrap_best_effort(
                    state,
                    crate::openclaw_paths::DEFAULT_AGENT_ID,
                );
                let should_persist = is_remember_cmd || is_forget_cmd;

                let reply = if is_remember_cmd || is_forget_cmd {
                    let updates = if is_remember_cmd {
                        crate::workspace_autosave::autosave_workspace_best_effort(
                            &workspace_dir,
                            &params.message,
                        )
                    } else {
                        crate::workspace_autosave::forget_workspace_best_effort(
                            &workspace_dir,
                            &params.message,
                        )
                    };

                    if updates.applied {
                        if updates.updates.is_empty() {
                            if is_remember_cmd {
                                "Saved to memory.".to_string()
                            } else {
                                "Forgot.".to_string()
                            }
                        } else {
                            let mut out = String::new();
                            out.push_str(if is_remember_cmd {
                                "Saved to memory:\n"
                            } else {
                                "Forgot:\n"
                            });
                            for u in updates.updates.iter().take(12) {
                                out.push_str("- ");
                                out.push_str(u);
                                out.push('\n');
                            }
                            out.trim_end().to_string()
                        }
                    } else if is_remember_cmd {
                        if crate::workspace_autosave::parse_remember_command(&params.message)
                            .is_some()
                        {
                            "Nothing saved (refused to store sensitive/invalid content)."
                                .to_string()
                        } else {
                            "Usage: /remember <note>".to_string()
                        }
                    } else if crate::workspace_autosave::parse_forget_command(&params.message)
                        .is_some()
                    {
                        "Nothing forgotten (no matching items).".to_string()
                    } else {
                        "Usage: /forget <name|timezone|style|all|text>".to_string()
                    }
                } else if is_profile_cmd {
                    crate::workspace_memory_view::build_workspace_profile_overview(&workspace_dir)
                } else if is_memory_cmd {
                    crate::workspace_memory_view::build_workspace_memory_overview(&workspace_dir)
                } else if is_kb_cmd {
                    let query = if msg_lower == "/kb"
                        || msg_lower.starts_with("/kb ")
                        || msg_lower.starts_with("/kb:")
                    {
                        msg_trimmed[3..].trim_start_matches(&[' ', ':'][..]).trim()
                    } else if msg_lower.starts_with("kb:") {
                        msg_trimmed[3..].trim()
                    } else if msg_lower == "/notes"
                        || msg_lower.starts_with("/notes ")
                        || msg_lower.starts_with("/notes:")
                    {
                        msg_trimmed[6..].trim_start_matches(&[' ', ':'][..]).trim()
                    } else if msg_lower.starts_with("notes:") {
                        msg_trimmed[6..].trim()
                    } else {
                        ""
                    };

                    if query.trim().is_empty() {
                        "Usage: /kb <query>".to_string()
                    } else {
                        crate::workspace_notes_recall::recall_workspace_notes_prompt_explicit(
                            &workspace_dir,
                            query,
                        )
                        .await
                        .unwrap_or_else(|| "No relevant notes found.".to_string())
                    }
                } else {
                    "Unknown local command.".to_string()
                };

                // Persist only mutating commands; view/search commands would bloat chat history.
                if should_persist {
                    if let Some(store) = state.session_store() {
                        let user_id = client_ctx.read().await.effective_user_id();
                        match store.get(session_id).await {
                            Ok(Some(mut s)) => {
                                if s.user_id != user_id {
                                    warn!(
                                        session_id = %session_id,
                                        session_user_id = %s.user_id,
                                        request_user_id = %user_id,
                                        "Session access denied"
                                    );
                                    return WsMessage::error(
                                        request.id,
                                        ErrorCode::PermissionDenied,
                                        "Permission denied",
                                    );
                                }
                                s.add_message(Message::user(&params.message));
                                s.add_message(Message::assistant(&reply));
                                s.update_timestamp();
                                let _ = store.update(&s).await;
                            }
                            Ok(None) => {
                                let mut s =
                                    Session::new(user_id, "gateway", session_id.to_string());
                                s.id = session_id;
                                s.title = Some("Gateway Chat".to_string());
                                // Don't force a provider selection for local commands.
                                s.provider = state.provider().map(|p| p.name().to_string());
                                s.model = params.model.clone();
                                s.system_prompt = params
                                    .options
                                    .as_ref()
                                    .and_then(|o| o.system_prompt.clone());
                                s.add_message(Message::user(&params.message));
                                s.add_message(Message::assistant(&reply));
                                let _ = store.create(&s).await;
                            }
                            Err(_) => {}
                        }
                    }
                }

                let model_name = params.model.clone().unwrap_or_else(|| "local".to_string());

                if params.stream {
                    let start_event = Event::new(
                        event_types::CHAT_STREAM_START,
                        drbot_protocol::event::chat::StreamStartEvent {
                            request_id: request.id,
                            session_id,
                            message_id,
                            model: model_name.clone(),
                            provider: None,
                        },
                    );
                    if let Ok(json) = serde_json::to_string(&WsMessage::Event(start_event)) {
                        let _ = tx.send(json).await;
                    }

                    let delta_event = Event::new(
                        event_types::CHAT_STREAM_DELTA,
                        drbot_protocol::event::chat::StreamDeltaEvent {
                            request_id: request.id,
                            delta: reply.clone(),
                        },
                    );
                    if let Ok(json) = serde_json::to_string(&WsMessage::Event(delta_event)) {
                        let _ = tx.send(json).await;
                    }

                    let complete_event = Event::new(
                        event_types::CHAT_STREAM_COMPLETE,
                        drbot_protocol::event::chat::StreamCompleteEvent {
                            request_id: request.id,
                            content: reply.clone(),
                            stop_reason: Some("local".to_string()),
                            usage: None,
                        },
                    );
                    if let Ok(json) = serde_json::to_string(&WsMessage::Event(complete_event)) {
                        let _ = tx.send(json).await;
                    }

                    return WsMessage::success(
                        request.id,
                        drbot_protocol::ChatSendResult {
                            session_id,
                            message_id,
                            content: None,
                            model: model_name,
                            provider: None,
                            usage: None,
                        },
                    );
                }

                return WsMessage::success(
                    request.id,
                    drbot_protocol::ChatSendResult {
                        session_id,
                        message_id,
                        content: Some(reply),
                        model: model_name,
                        provider: None,
                        usage: None,
                    },
                );
            }

            // Get provider; if none is active, try auto-init (best effort).
            let mut provider = match state.provider() {
                Some(p) => p,
                None => match init_provider_auto_best_effort(state).await {
                    Ok(p) => {
                        state.set_provider(Some(p.clone()));
                        p
                    }
                    Err(reason) => {
                        return WsMessage::error(request.id, ErrorCode::ProviderError, reason);
                    }
                },
            };
            let mut active_provider_name = provider.name().to_string();
            let mut initial_provider_name = active_provider_name.clone();

            // Get or create session
            // Build messages for the provider
            let mut messages = Vec::new();

            // Build chat options
            let mut options = ChatOptions {
                model: params.model.clone(),
                max_tokens: params.options.as_ref().and_then(|o| o.max_tokens),
                temperature: params.options.as_ref().and_then(|o| o.temperature),
                top_p: params.options.as_ref().and_then(|o| o.top_p),
                stop_sequences: params
                    .options
                    .as_ref()
                    .and_then(|o| o.stop_sequences.clone()),
                system_prompt: params
                    .options
                    .as_ref()
                    .and_then(|o| o.system_prompt.clone()),
                tools: None,
            };

            // Load existing session messages if we have a session store.
            // If the session doesn't exist yet, create it with the requested ID so
            // subsequent calls can resume by `session_id`.
            let mut persisted_session: Option<Session> = None;
            if let Some(store) = state.session_store() {
                let user_id = client_ctx.read().await.effective_user_id();
                let mut session = match store.get(session_id).await {
                    Ok(Some(s)) => {
                        if s.user_id != user_id {
                            warn!(
                                session_id = %session_id,
                                session_user_id = %s.user_id,
                                request_user_id = %user_id,
                                "Session access denied"
                            );
                            return WsMessage::error(
                                request.id,
                                ErrorCode::PermissionDenied,
                                "Permission denied",
                            );
                        }
                        s
                    }
                    Ok(None) => {
                        let mut s = Session::new(user_id, "gateway", session_id.to_string());
                        s.id = session_id;
                        s.provider = Some(active_provider_name.clone());
                        s.model = params.model.clone();
                        s.system_prompt = options.system_prompt.clone();
                        s.title = Some("Gateway Chat".to_string());
                        if let Err(e) = store.create(&s).await {
                            error!(error = %e, session_id = %session_id, "Failed to create session");
                        }
                        s
                    }
                    Err(e) => {
                        error!(error = %e, session_id = %session_id, "Failed to load session");
                        let mut s = Session::new(user_id, "gateway", session_id.to_string());
                        s.id = session_id;
                        s
                    }
                };

                // Prefer the session's persisted provider when resuming. This makes gateway chat
                // robust even if the client doesn't explicitly select a provider.
                if let Some(session_provider) = session
                    .provider
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                {
                    if session_provider != active_provider_name {
                        match crate::state::try_init_provider_named(
                            state.config(),
                            session_provider,
                        ) {
                            Ok(p) => {
                                provider = p;
                                active_provider_name = provider.name().to_string();
                                initial_provider_name = active_provider_name.clone();
                                state.set_provider(Some(provider.clone()));
                            }
                            Err(reason) => {
                                warn!(
                                    session_id = %session_id,
                                    provider = %session_provider,
                                    reason = %reason,
                                    "Session provider unavailable; continuing with active provider"
                                );
                            }
                        }
                    }
                }

                // Treat request model/system prompt as session-level settings.
                // If omitted, fall back to the session's persisted values.
                if params.model.is_some() {
                    session.model = params.model.clone();
                }
                if options.system_prompt.is_some() {
                    session.system_prompt = options.system_prompt.clone();
                }
                if options.model.is_none() {
                    options.model = session.model.clone();
                }
                if options.system_prompt.is_none() {
                    options.system_prompt = session.system_prompt.clone();
                }

                messages.extend(session.messages.clone());
                // Keep the session for persistence updates below.
                persisted_session = Some(session);
            }

            // Workspace-backed personalization + knowledge base (best effort).
            //
            // We use the default OpenClaw agent workspace so the same USER.md/MEMORY.md and
            // memory/*.md notes apply across the gateway and OpenClaw surfaces.
            let workspace_dir = crate::openclaw::ensure_agent_workspace_bootstrap_best_effort(
                state,
                crate::openclaw_paths::DEFAULT_AGENT_ID,
            );
            // Avoid polluting workspace memory/recall with internal tool-loop messages.
            let msg_trimmed = params.message.trim_start();
            let is_internal_tool_message = msg_trimmed.starts_with("[Tool Result]")
                || msg_trimmed.starts_with("[Tool Denied]")
                || msg_trimmed.starts_with("[Tool Mode Strict]");
            if !is_internal_tool_message {
                crate::workspace_autosave::autosave_workspace_best_effort(
                    &workspace_dir,
                    &params.message,
                );
            }
            let workspace_context =
                crate::workspace_chat_context::build_chat_workspace_context_prompt(&workspace_dir);
            let workspace_notes = if is_internal_tool_message {
                None
            } else {
                crate::workspace_notes_recall::recall_workspace_notes_prompt(
                    &workspace_dir,
                    &params.message,
                )
                .await
            };

            let mut system_sections: Vec<String> = Vec::new();
            if let Some(existing) = options.system_prompt.take() {
                let trimmed = existing.trim();
                if !trimmed.is_empty() {
                    system_sections.push(trimmed.to_string());
                }
            }
            let ctx_trimmed = workspace_context.trim();
            if !ctx_trimmed.is_empty() {
                system_sections.push(ctx_trimmed.to_string());
            }
            if let Some(notes) = workspace_notes.as_deref() {
                let trimmed = notes.trim();
                if !trimmed.is_empty() {
                    system_sections.push(trimmed.to_string());
                }
            }
            options.system_prompt = if system_sections.is_empty() {
                None
            } else {
                Some(system_sections.join("\n\n---\n\n"))
            };

            // Add the user's message (to provider only; persistence happens after a successful response).
            let user_msg = Message::user(&params.message);
            messages.push(user_msg.clone());

            let mut fallback_providers: Vec<String> = vec![
                "claude-cli".to_string(),
                "codex-cli".to_string(),
                "codex-oss".to_string(),
                "ollama".to_string(),
                "anthropic".to_string(),
                "openai".to_string(),
            ];
            fallback_providers.extend(
                state
                    .config()
                    .providers
                    .openai_compatible
                    .iter()
                    .map(|c| c.name.clone()),
            );
            fallback_providers.extend(state.config().providers.cli.iter().map(|c| c.name.clone()));
            let mut seen = std::collections::HashSet::new();
            fallback_providers.retain(|n| seen.insert(n.clone()));

            if params.stream {
                // Streaming response
                let mut stream = match provider.stream(&messages, options.clone()).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        let first_error = e.to_string();
                        warn!(
                            provider = %active_provider_name,
                            error = %first_error,
                            "Provider stream failed; attempting fallback"
                        );

                        let mut last_error = first_error.clone();
                        let mut selected_provider = None;
                        let mut selected_stream = None;

                        for name in fallback_providers.iter() {
                            if name == &active_provider_name {
                                continue;
                            }

                            let p = match try_init_provider_named_best_effort(state, name).await {
                                Ok(p) => p,
                                Err(reason) => {
                                    debug!(
                                        provider = %name,
                                        reason = %reason,
                                        "Fallback provider not available"
                                    );
                                    continue;
                                }
                            };

                            let mut attempt_options = options.clone();
                            // Model ids rarely port across providers; use provider default on fallback.
                            attempt_options.model = None;

                            match p.stream(&messages, attempt_options).await {
                                Ok(s) => {
                                    selected_provider = Some(p);
                                    selected_stream = Some(s);
                                    break;
                                }
                                Err(err) => {
                                    last_error = err.to_string();
                                    debug!(
                                        provider = %name,
                                        error = %last_error,
                                        "Fallback provider stream failed"
                                    );
                                    continue;
                                }
                            }
                        }

                        let Some(p) = selected_provider else {
                            error!(
                                provider = %active_provider_name,
                                error = %first_error,
                                last_error = %last_error,
                                "All provider fallbacks failed (stream)"
                            );
                            return WsMessage::error(
                                request.id,
                                ErrorCode::ProviderError,
                                format!("{} (fallback failed: {})", first_error, last_error),
                            );
                        };

                        let Some(s) = selected_stream else {
                            return WsMessage::error(
                                request.id,
                                ErrorCode::ProviderError,
                                format!("{} (fallback failed: {})", first_error, last_error),
                            );
                        };

                        let previous_provider = active_provider_name.clone();
                        provider = p;
                        active_provider_name = provider.name().to_string();
                        state.set_provider(Some(provider.clone()));

                        // Drop model selection when switching providers to avoid mismatch.
                        options.model = None;

                        let changed_event = Event::new(
                            event_types::PROVIDER_CHANGED,
                            drbot_protocol::event::provider::ChangedEvent {
                                provider: active_provider_name.clone(),
                                previous_provider: Some(previous_provider.clone()),
                                reason: Some(format!(
                                    "{} failed: {}",
                                    previous_provider, first_error
                                )),
                            },
                        );
                        if let Ok(json) = serde_json::to_string(&WsMessage::Event(changed_event)) {
                            let _ = tx.send(json).await;
                        }

                        s
                    }
                };

                let model_name = options
                    .model
                    .clone()
                    .unwrap_or_else(|| "default".to_string());

                // Send start event
                let start_event = Event::new(
                    event_types::CHAT_STREAM_START,
                    drbot_protocol::event::chat::StreamStartEvent {
                        request_id: request.id,
                        session_id,
                        message_id,
                        model: model_name.clone(),
                        provider: Some(active_provider_name.clone()),
                    },
                );
                if let Ok(json) = serde_json::to_string(&WsMessage::Event(start_event)) {
                    let _ = tx.send(json).await;
                }

                let mut full_content = String::new();
                let mut final_usage = None;

                while let Some(event) = stream.next().await {
                    match event {
                        ProviderStreamEvent::Delta { content } => {
                            full_content.push_str(&content);
                            let delta_event = Event::new(
                                event_types::CHAT_STREAM_DELTA,
                                drbot_protocol::event::chat::StreamDeltaEvent {
                                    request_id: request.id,
                                    delta: content,
                                },
                            );
                            if let Ok(json) = serde_json::to_string(&WsMessage::Event(delta_event))
                            {
                                let _ = tx.send(json).await;
                            }
                        }
                        ProviderStreamEvent::Stop { reason, usage } => {
                            final_usage = usage;
                            let complete_event = Event::new(
                                event_types::CHAT_STREAM_COMPLETE,
                                drbot_protocol::event::chat::StreamCompleteEvent {
                                    request_id: request.id,
                                    content: full_content.clone(),
                                    stop_reason: Some(reason),
                                    usage: final_usage.as_ref().map(|u| {
                                        drbot_protocol::response::TokenUsage {
                                            input_tokens: u.input_tokens,
                                            output_tokens: u.output_tokens,
                                        }
                                    }),
                                },
                            );
                            if let Ok(json) =
                                serde_json::to_string(&WsMessage::Event(complete_event))
                            {
                                let _ = tx.send(json).await;
                            }
                        }
                        ProviderStreamEvent::Error { message } => {
                            error!(error = %message, "Stream error");
                            let error_event = Event::new(
                                event_types::CHAT_STREAM_ERROR,
                                drbot_protocol::event::chat::StreamErrorEvent {
                                    request_id: request.id,
                                    error: message,
                                },
                            );
                            if let Ok(json) = serde_json::to_string(&WsMessage::Event(error_event))
                            {
                                let _ = tx.send(json).await;
                            }
                        }
                        _ => {}
                    }
                }

                if let (Some(store), Some(session)) =
                    (state.session_store(), persisted_session.as_mut())
                {
                    if active_provider_name != initial_provider_name {
                        // Prevent a persisted model id from the previous provider from breaking
                        // future calls after fallback.
                        session.model = None;
                    }
                    session.provider = Some(active_provider_name.clone());
                    session.add_message(user_msg);
                    session.add_message(Message::assistant(&full_content));
                    if let Some(usage) = &final_usage {
                        session.add_token_usage(usage.input_tokens, usage.output_tokens);
                    }
                    session.update_timestamp();
                    if let Err(e) = store.update(session).await {
                        error!(error = %e, session_id = %session_id, "Failed to update session");
                    }
                }

                WsMessage::success(
                    request.id,
                    drbot_protocol::ChatSendResult {
                        session_id,
                        message_id,
                        content: None,
                        model: model_name,
                        provider: Some(active_provider_name.clone()),
                        usage: final_usage.map(|u| drbot_protocol::response::TokenUsage {
                            input_tokens: u.input_tokens,
                            output_tokens: u.output_tokens,
                        }),
                    },
                )
            } else {
                // Non-streaming response
                let response = match provider.chat(&messages, options.clone()).await {
                    Ok(response) => response,
                    Err(e) => {
                        let first_error = e.to_string();
                        warn!(
                            provider = %active_provider_name,
                            error = %first_error,
                            "Provider chat failed; attempting fallback"
                        );

                        let mut last_error = first_error.clone();
                        let mut selected_provider = None;
                        let mut selected_response = None;

                        for name in fallback_providers.iter() {
                            if name == &active_provider_name {
                                continue;
                            }

                            let p = match try_init_provider_named_best_effort(state, name).await {
                                Ok(p) => p,
                                Err(reason) => {
                                    debug!(
                                        provider = %name,
                                        reason = %reason,
                                        "Fallback provider not available"
                                    );
                                    continue;
                                }
                            };

                            let mut attempt_options = options.clone();
                            // Model ids rarely port across providers; use provider default on fallback.
                            attempt_options.model = None;

                            match p.chat(&messages, attempt_options).await {
                                Ok(r) => {
                                    selected_provider = Some(p);
                                    selected_response = Some(r);
                                    break;
                                }
                                Err(err) => {
                                    last_error = err.to_string();
                                    debug!(
                                        provider = %name,
                                        error = %last_error,
                                        "Fallback provider chat failed"
                                    );
                                    continue;
                                }
                            }
                        }

                        let Some(p) = selected_provider else {
                            error!(
                                provider = %active_provider_name,
                                error = %first_error,
                                last_error = %last_error,
                                "All provider fallbacks failed (chat)"
                            );
                            return WsMessage::error(
                                request.id,
                                ErrorCode::ProviderError,
                                format!("{} (fallback failed: {})", first_error, last_error),
                            );
                        };

                        let Some(r) = selected_response else {
                            return WsMessage::error(
                                request.id,
                                ErrorCode::ProviderError,
                                format!("{} (fallback failed: {})", first_error, last_error),
                            );
                        };

                        let previous_provider = active_provider_name.clone();
                        provider = p;
                        active_provider_name = provider.name().to_string();
                        state.set_provider(Some(provider.clone()));

                        // Drop model selection when switching providers to avoid mismatch.
                        options.model = None;

                        let changed_event = Event::new(
                            event_types::PROVIDER_CHANGED,
                            drbot_protocol::event::provider::ChangedEvent {
                                provider: active_provider_name.clone(),
                                previous_provider: Some(previous_provider.clone()),
                                reason: Some(format!(
                                    "{} failed: {}",
                                    previous_provider, first_error
                                )),
                            },
                        );
                        if let Ok(json) = serde_json::to_string(&WsMessage::Event(changed_event)) {
                            let _ = tx.send(json).await;
                        }

                        r
                    }
                };

                if let (Some(store), Some(session)) =
                    (state.session_store(), persisted_session.as_mut())
                {
                    if active_provider_name != initial_provider_name {
                        // Prevent a persisted model id from the previous provider from breaking
                        // future calls after fallback.
                        session.model = None;
                    }
                    session.provider = Some(active_provider_name.clone());
                    session.add_message(user_msg);
                    session.add_message(Message::assistant(&response.content));
                    if let Some(usage) = &response.usage {
                        session.add_token_usage(usage.input_tokens, usage.output_tokens);
                    }
                    session.update_timestamp();
                    if let Err(e) = store.update(session).await {
                        error!(error = %e, session_id = %session_id, "Failed to update session");
                    }
                }

                WsMessage::success(
                    request.id,
                    drbot_protocol::ChatSendResult {
                        session_id,
                        message_id,
                        content: Some(response.content),
                        model: response.model,
                        provider: Some(active_provider_name.clone()),
                        usage: response
                            .usage
                            .map(|u| drbot_protocol::response::TokenUsage {
                                input_tokens: u.input_tokens,
                                output_tokens: u.output_tokens,
                            }),
                    },
                )
            }
        }

        "session.create" => {
            let params: SessionCreateParams = match serde_json::from_value(request.params.clone()) {
                Ok(p) => p,
                Err(e) => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::InvalidParams,
                        format!("Invalid params: {}", e),
                    );
                }
            };

            let store = match state.session_store() {
                Some(s) => s,
                None => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::SessionError,
                        "Session store not configured",
                    );
                }
            };

            let user_id = client_ctx.read().await.effective_user_id();
            let session_id = uuid::Uuid::new_v4();
            let mut session = Session::new(user_id, "gateway", session_id.to_string());
            session.id = session_id;
            session.title = params.title.clone();
            session.provider = params
                .provider
                .clone()
                .or_else(|| state.provider().map(|p| p.name().to_string()));
            session.model = params.model.clone();
            session.system_prompt = params.system_prompt.clone();

            if let Err(e) = store.create(&session).await {
                error!(error = %e, session_id = %session_id, "Failed to create session");
                return WsMessage::error(request.id, ErrorCode::SessionError, e.to_string());
            }

            WsMessage::success(request.id, SessionCreateResult { session_id })
        }

        "session.get" => {
            let params: SessionGetParams = match serde_json::from_value(request.params.clone()) {
                Ok(p) => p,
                Err(e) => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::InvalidParams,
                        format!("Invalid params: {}", e),
                    );
                }
            };

            let store = match state.session_store() {
                Some(s) => s,
                None => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::SessionError,
                        "Session store not configured",
                    );
                }
            };

            let user_id = client_ctx.read().await.effective_user_id();
            let session = match store.get(params.session_id).await {
                Ok(Some(s)) => s,
                Ok(None) => {
                    return WsMessage::error(request.id, ErrorCode::NotFound, "Session not found");
                }
                Err(e) => {
                    error!(error = %e, session_id = %params.session_id, "Failed to load session");
                    return WsMessage::error(request.id, ErrorCode::SessionError, e.to_string());
                }
            };

            if session.user_id != user_id {
                warn!(
                    session_id = %params.session_id,
                    session_user_id = %session.user_id,
                    request_user_id = %user_id,
                    "Session access denied"
                );
                return WsMessage::error(
                    request.id,
                    ErrorCode::PermissionDenied,
                    "Permission denied",
                );
            }

            let info = drbot_protocol::SessionInfo {
                id: session.id,
                title: session.title.clone(),
                provider: session.provider.clone(),
                model: session.model.clone(),
                message_count: session.metadata.message_count,
                created_at: session.created_at,
                updated_at: session.updated_at,
                state: match session.state {
                    drbot_core::session::SessionState::Active => "active".to_string(),
                    drbot_core::session::SessionState::Archived => "archived".to_string(),
                    drbot_core::session::SessionState::Deleted => "deleted".to_string(),
                },
            };

            WsMessage::success(
                request.id,
                SessionGetResult {
                    session: info,
                    messages: session.messages,
                    system_prompt: session.system_prompt,
                },
            )
        }

        "session.update" => {
            let params: SessionUpdateParams = match serde_json::from_value(request.params.clone()) {
                Ok(p) => p,
                Err(e) => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::InvalidParams,
                        format!("Invalid params: {}", e),
                    );
                }
            };

            let store = match state.session_store() {
                Some(s) => s,
                None => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::SessionError,
                        "Session store not configured",
                    );
                }
            };

            let user_id = client_ctx.read().await.effective_user_id();
            let mut session = match store.get(params.session_id).await {
                Ok(Some(s)) => s,
                Ok(None) => {
                    return WsMessage::error(request.id, ErrorCode::NotFound, "Session not found");
                }
                Err(e) => {
                    error!(error = %e, session_id = %params.session_id, "Failed to load session");
                    return WsMessage::error(request.id, ErrorCode::SessionError, e.to_string());
                }
            };

            if session.user_id != user_id {
                warn!(
                    session_id = %params.session_id,
                    session_user_id = %session.user_id,
                    request_user_id = %user_id,
                    "Session access denied"
                );
                return WsMessage::error(
                    request.id,
                    ErrorCode::PermissionDenied,
                    "Permission denied",
                );
            }

            if params.clear_provider {
                session.provider = None;
            }
            if params.clear_model {
                session.model = None;
            }
            if params.clear_system_prompt {
                session.system_prompt = None;
            }

            if let Some(title) = params.title.clone() {
                session.title = Some(title);
            }
            if let Some(provider) = params.provider.clone() {
                session.provider = Some(provider);
            }
            if let Some(model) = params.model.clone() {
                session.model = Some(model);
            }
            if let Some(system_prompt) = params.system_prompt.clone() {
                session.system_prompt = Some(system_prompt);
            }

            session.update_timestamp();
            if let Err(e) = store.update(&session).await {
                error!(error = %e, session_id = %params.session_id, "Failed to update session");
                return WsMessage::error(request.id, ErrorCode::SessionError, e.to_string());
            }

            WsMessage::success(
                request.id,
                SessionUpdateResult {
                    session_id: params.session_id,
                    updated: true,
                },
            )
        }

        "session.clear" => {
            let params: SessionClearParams = match serde_json::from_value(request.params.clone()) {
                Ok(p) => p,
                Err(e) => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::InvalidParams,
                        format!("Invalid params: {}", e),
                    );
                }
            };

            let store = match state.session_store() {
                Some(s) => s,
                None => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::SessionError,
                        "Session store not configured",
                    );
                }
            };

            let user_id = client_ctx.read().await.effective_user_id();
            let mut session = match store.get(params.session_id).await {
                Ok(Some(s)) => s,
                Ok(None) => {
                    return WsMessage::error(request.id, ErrorCode::NotFound, "Session not found");
                }
                Err(e) => {
                    error!(error = %e, session_id = %params.session_id, "Failed to load session");
                    return WsMessage::error(request.id, ErrorCode::SessionError, e.to_string());
                }
            };

            if session.user_id != user_id {
                warn!(
                    session_id = %params.session_id,
                    session_user_id = %session.user_id,
                    request_user_id = %user_id,
                    "Session access denied"
                );
                return WsMessage::error(
                    request.id,
                    ErrorCode::PermissionDenied,
                    "Permission denied",
                );
            }

            session.clear_messages();
            if let Err(e) = store.update(&session).await {
                error!(error = %e, session_id = %params.session_id, "Failed to update session");
                return WsMessage::error(request.id, ErrorCode::SessionError, e.to_string());
            }

            WsMessage::success(
                request.id,
                SessionClearResult {
                    session_id: params.session_id,
                    cleared: true,
                },
            )
        }

        "session.delete" => {
            let params: SessionDeleteParams = match serde_json::from_value(request.params.clone()) {
                Ok(p) => p,
                Err(e) => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::InvalidParams,
                        format!("Invalid params: {}", e),
                    );
                }
            };

            let store = match state.session_store() {
                Some(s) => s,
                None => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::SessionError,
                        "Session store not configured",
                    );
                }
            };

            let user_id = client_ctx.read().await.effective_user_id();
            let session = match store.get(params.session_id).await {
                Ok(Some(s)) => s,
                Ok(None) => {
                    return WsMessage::error(request.id, ErrorCode::NotFound, "Session not found");
                }
                Err(e) => {
                    error!(error = %e, session_id = %params.session_id, "Failed to load session");
                    return WsMessage::error(request.id, ErrorCode::SessionError, e.to_string());
                }
            };

            if session.user_id != user_id {
                warn!(
                    session_id = %params.session_id,
                    session_user_id = %session.user_id,
                    request_user_id = %user_id,
                    "Session access denied"
                );
                return WsMessage::error(
                    request.id,
                    ErrorCode::PermissionDenied,
                    "Permission denied",
                );
            }

            if let Err(e) = store.delete(params.session_id).await {
                error!(error = %e, session_id = %params.session_id, "Failed to delete session");
                return WsMessage::error(request.id, ErrorCode::SessionError, e.to_string());
            }

            WsMessage::success(
                request.id,
                SessionDeleteResult {
                    session_id: params.session_id,
                    deleted: true,
                },
            )
        }

        "session.list" => {
            let params: SessionListParams = serde_json::from_value(request.params.clone())
                .unwrap_or_else(|_| SessionListParams::default());

            let store = match state.session_store() {
                Some(s) => s,
                None => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::SessionError,
                        "Session store not configured",
                    );
                }
            };

            let user_id = client_ctx.read().await.effective_user_id();

            let include_archived = match params.state.as_deref() {
                None | Some("active") => false,
                Some("archived") | Some("all") => true,
                Some(other) => {
                    warn!(state = %other, "Unknown session state filter; defaulting to active");
                    false
                }
            };

            let list_opts = drbot_sessions::ListOptions {
                user_id: Some(user_id),
                channel_type: Some("gateway".to_string()),
                limit: params.limit,
                offset: params.offset,
                include_archived,
            };

            let sessions = match store.list(list_opts.clone()).await {
                Ok(s) => s,
                Err(e) => {
                    error!(error = %e, "Failed to list sessions");
                    return WsMessage::error(request.id, ErrorCode::SessionError, e.to_string());
                }
            };

            // Count total (cheap: sessions list doesn't load messages).
            let total = match store
                .list(drbot_sessions::ListOptions {
                    limit: None,
                    offset: None,
                    ..list_opts
                })
                .await
            {
                Ok(all) => all.len(),
                Err(_) => sessions.len(),
            };

            let state_filter = params.state.as_deref();
            let sessions: Vec<_> = sessions
                .into_iter()
                .filter(|s| match state_filter {
                    Some("archived") => {
                        matches!(s.state, drbot_core::session::SessionState::Archived)
                    }
                    Some("active") | None => {
                        matches!(s.state, drbot_core::session::SessionState::Active)
                    }
                    Some("all") => matches!(
                        s.state,
                        drbot_core::session::SessionState::Active
                            | drbot_core::session::SessionState::Archived
                    ),
                    _ => true,
                })
                .map(|s| drbot_protocol::SessionInfo {
                    id: s.id,
                    title: s.title,
                    provider: s.provider,
                    model: s.model,
                    message_count: s.metadata.message_count,
                    created_at: s.created_at,
                    updated_at: s.updated_at,
                    state: match s.state {
                        drbot_core::session::SessionState::Active => "active".to_string(),
                        drbot_core::session::SessionState::Archived => "archived".to_string(),
                        drbot_core::session::SessionState::Deleted => "deleted".to_string(),
                    },
                })
                .collect();

            WsMessage::success(request.id, SessionListResult { sessions, total })
        }

        "provider.list" => {
            let active = state.provider().map(|p| p.name().to_string());

            let mut candidates: Vec<String> = vec![
                "auto".to_string(),
                "claude-cli".to_string(),
                "codex-cli".to_string(),
                "codex-oss".to_string(),
                "ollama".to_string(),
                "anthropic".to_string(),
                "openai".to_string(),
            ];
            candidates.extend(
                state
                    .config()
                    .providers
                    .openai_compatible
                    .iter()
                    .map(|c| c.name.clone()),
            );
            candidates.extend(state.config().providers.cli.iter().map(|c| c.name.clone()));
            candidates.sort();
            candidates.dedup();

            // Preserve a friendly order: cost-savers first.
            let mut ordered: Vec<String> = Vec::new();
            for name in [
                "auto",
                "claude-cli",
                "codex-cli",
                "codex-oss",
                "ollama",
                "anthropic",
                "openai",
            ] {
                if candidates.iter().any(|c| c == name) {
                    ordered.push(name.to_string());
                }
            }
            for c in candidates {
                if !ordered.contains(&c) {
                    ordered.push(c);
                }
            }

            let mut providers: Vec<drbot_protocol::ProviderInfo> = Vec::new();
            let status_is_selectable = |status: &str| {
                status == "available"
                    || (status.starts_with("active") && !status.contains("unreachable"))
            };
            for name in ordered.into_iter().filter(|n| n != "auto") {
                // Special-case Ollama so we can report reachability (configured != running).
                if name == "ollama" {
                    let base_url = ollama_base_url(state.config());
                    let fetched = fetch_ollama_tags(&base_url, Duration::from_millis(350)).await;
                    let mut reachable = false;
                    let mut models: Vec<String> = drbot_ollama::OllamaProvider::new()
                        .models()
                        .iter()
                        .map(|m| m.id.clone())
                        .collect();

                    if let Some((ok, json)) = fetched {
                        reachable = ok;
                        if let Some(json) = json {
                            let mut names: Vec<String> = json
                                .get("models")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|m| {
                                            m.get("name")
                                                .and_then(|n| n.as_str())
                                                .map(|s| s.to_string())
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            names.sort();
                            names.dedup();
                            if !names.is_empty() {
                                models = names;
                            }
                        }
                    }

                    let is_active = active.as_deref() == Some("ollama");
                    let status = if is_active && !reachable {
                        "active (unreachable)".to_string()
                    } else if is_active {
                        "active".to_string()
                    } else if reachable {
                        "available".to_string()
                    } else {
                        format!("unavailable: ollama not reachable at {}", base_url.trim())
                    };
                    providers.push(drbot_protocol::ProviderInfo {
                        name,
                        status,
                        models,
                    });
                    continue;
                }

                // Special-case codex-oss so we can report local Ollama reachability.
                if name == "codex-oss" {
                    match crate::state::try_init_provider_named(state.config(), &name) {
                        Ok(p) => {
                            let mut models: Vec<String> =
                                p.models().iter().map(|m| m.id.clone()).collect();
                            let mut reachable = false;

                            let base_url = ollama_base_url(state.config());
                            let fetched =
                                fetch_ollama_tags(&base_url, Duration::from_millis(350)).await;

                            if let Some((ok, json)) = fetched {
                                reachable = ok;
                                if let Some(json) = json {
                                    let mut names: Vec<String> = json
                                        .get("models")
                                        .and_then(|v| v.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|m| {
                                                    m.get("name")
                                                        .and_then(|n| n.as_str())
                                                        .map(|s| s.to_string())
                                                })
                                                .collect::<Vec<_>>()
                                        })
                                        .unwrap_or_default();
                                    names.sort();
                                    names.dedup();
                                    if !names.is_empty() {
                                        models = names;
                                    }
                                }
                            }

                            let is_active = active.as_deref() == Some("codex-oss");
                            let status = if is_active && !reachable {
                                "active (unreachable)".to_string()
                            } else if is_active {
                                "active".to_string()
                            } else if reachable {
                                "available".to_string()
                            } else {
                                format!("unavailable: ollama not reachable at {}", base_url.trim())
                            };

                            providers.push(drbot_protocol::ProviderInfo {
                                name,
                                status,
                                models,
                            });
                        }
                        Err(reason) => providers.push(drbot_protocol::ProviderInfo {
                            name,
                            status: format!("unavailable: {}", reason),
                            models: vec![],
                        }),
                    }
                    continue;
                }

                match crate::state::try_init_provider_named(state.config(), &name) {
                    Ok(p) => providers.push(drbot_protocol::ProviderInfo {
                        name: name.clone(),
                        status: if active.as_deref() == Some(name.as_str()) {
                            "active".to_string()
                        } else {
                            "available".to_string()
                        },
                        models: p.models().iter().map(|m| m.id.clone()).collect(),
                    }),
                    Err(reason) => providers.push(drbot_protocol::ProviderInfo {
                        name,
                        status: format!("unavailable: {}", reason),
                        models: vec![],
                    }),
                }
            }

            let any_available = providers.iter().any(|p| status_is_selectable(&p.status));
            providers.insert(
                0,
                drbot_protocol::ProviderInfo {
                    name: "auto".to_string(),
                    status: if any_available {
                        "available".to_string()
                    } else {
                        "unavailable: no providers available (try drbot wizard)".to_string()
                    },
                    models: vec![],
                },
            );

            WsMessage::success(request.id, ProviderListResult { providers })
        }

        "provider.models" => {
            let params: ProviderModelsParams = match serde_json::from_value(request.params.clone())
            {
                Ok(p) => p,
                Err(e) => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::InvalidParams,
                        format!("Invalid params: {}", e),
                    );
                }
            };

            let provider = match params
                .provider
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                Some(name) => match crate::state::try_init_provider_named(state.config(), name) {
                    Ok(p) => p,
                    Err(e) => return WsMessage::error(request.id, ErrorCode::ProviderError, e),
                },
                None => match state.provider() {
                    Some(p) => p,
                    None => {
                        return WsMessage::error(
                            request.id,
                            ErrorCode::ProviderError,
                            "No provider configured",
                        );
                    }
                },
            };

            let models = if provider.name() == "ollama" || provider.name() == "codex-oss" {
                let mut out: Vec<ModelInfo> = Vec::new();
                let mut tags_models: Vec<String> = Vec::new();

                let base_url = ollama_base_url(state.config());
                if let Some((reachable, json)) =
                    fetch_ollama_tags(&base_url, Duration::from_millis(600)).await
                {
                    if reachable {
                        if let Some(json) = json {
                            tags_models = json
                                .get("models")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|m| {
                                            m.get("name")
                                                .and_then(|n| n.as_str())
                                                .map(|s| s.to_string())
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            tags_models.sort();
                            tags_models.dedup();
                        }
                    }
                };

                if !tags_models.is_empty() {
                    let provider_name = provider.name().to_string();
                    for name in tags_models {
                        out.push(ModelInfo {
                            id: name.clone(),
                            name,
                            provider: provider_name.clone(),
                            context_window: 0,
                            max_output_tokens: None,
                        });
                    }
                    out
                } else {
                    provider
                        .models()
                        .into_iter()
                        .map(|m| ModelInfo {
                            id: m.id,
                            name: m.name,
                            provider: m.provider,
                            context_window: m.context_window,
                            max_output_tokens: m.max_output_tokens,
                        })
                        .collect()
                }
            } else {
                provider
                    .models()
                    .into_iter()
                    .map(|m| ModelInfo {
                        id: m.id,
                        name: m.name,
                        provider: m.provider,
                        context_window: m.context_window,
                        max_output_tokens: m.max_output_tokens,
                    })
                    .collect()
            };

            WsMessage::success(request.id, ProviderModelsResult { models })
        }

        "provider.select" => {
            let params: ProviderSelectParams = match serde_json::from_value(request.params.clone())
            {
                Ok(p) => p,
                Err(e) => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::InvalidParams,
                        format!("Invalid params: {}", e),
                    );
                }
            };

            let requested = params.provider.trim();
            let provider = if requested.eq_ignore_ascii_case("auto") {
                match init_provider_auto_best_effort(state).await {
                    Ok(p) => p,
                    Err(e) => return WsMessage::error(request.id, ErrorCode::ProviderError, e),
                }
            } else if (requested.eq_ignore_ascii_case("ollama")
                || requested.eq_ignore_ascii_case("local"))
                && state.config().providers.ollama.is_none()
            {
                // Allow selecting Ollama without config when it's actually running.
                let base_url = ollama_base_url(state.config());
                let reachable = fetch_ollama_tags(&base_url, Duration::from_millis(350))
                    .await
                    .map(|(ok, _)| ok)
                    .unwrap_or(false);
                if !reachable {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::ProviderError,
                        format!(
                            "ollama not reachable at {} (start Ollama or run drbot wizard)",
                            base_url.trim()
                        ),
                    );
                }
                let mut provider = drbot_ollama::OllamaProvider::new().with_base_url(base_url);
                if let Some(model) = state.config().providers.default_model.clone() {
                    provider = provider.with_default_model(model);
                }
                std::sync::Arc::new(provider) as std::sync::Arc<dyn Provider>
            } else {
                match crate::state::try_init_provider_named(state.config(), requested) {
                    Ok(p) => p,
                    Err(e) => return WsMessage::error(request.id, ErrorCode::ProviderError, e),
                }
            };

            let info = drbot_protocol::ProviderInfo {
                name: provider.name().to_string(),
                status: "active".to_string(),
                models: provider.models().iter().map(|m| m.id.clone()).collect(),
            };
            state.set_provider(Some(provider));

            WsMessage::success(request.id, ProviderSelectResult { provider: info })
        }

        "system.ping" => WsMessage::success(
            request.id,
            SystemPingResult {
                pong: true,
                timestamp: chrono::Utc::now().timestamp_millis(),
            },
        ),

        "system.info" => WsMessage::success(
            request.id,
            SystemInfoResult {
                version: env!("CARGO_PKG_VERSION").to_string(),
                protocol_version: drbot_protocol::PROTOCOL_VERSION.to_string(),
                uptime_secs: state.uptime_secs(),
                connected_clients: state.client_count().await,
                active_sessions: 0,
            },
        ),

        _ => {
            warn!(method = %request.method, "Unknown method");
            WsMessage::error(
                request.id,
                ErrorCode::MethodNotFound,
                format!("Unknown method: {}", request.method),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State, routing::post, Json, Router};
    use drbot_core::config::{OpenAIConfig, ProvidersConfig};
    use drbot_core::Config;
    use drbot_protocol::{ChatSendParams, SessionCreateParams, SessionListParams, WsMessage};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::{mpsc, Mutex};
    use uuid::Uuid;

    fn temp_db_path(test_name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("drbot-gateway-{}-{}", test_name, Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.join("drbot.db")
    }

    async fn parse_success<T: serde::de::DeserializeOwned>(msg: WsMessage) -> T {
        match msg {
            WsMessage::Response(resp) => {
                assert!(
                    resp.error.is_none(),
                    "expected success, got error: {:?}",
                    resp.error
                );
                serde_json::from_value(resp.result.expect("missing result")).expect("bad result")
            }
            other => panic!("expected response, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn session_create_and_list_use_store() {
        let mut config = Config::default();
        config.storage.database_path = temp_db_path("session_create_and_list");

        let state = GatewayState::new(config);

        let (tx, _rx) = mpsc::channel::<String>(8);
        let client_id = Uuid::new_v4();
        let client_ctx = Arc::new(tokio::sync::RwLock::new(ClientContext::new(
            client_id,
            state.auth_required(),
        )));

        // Create a session.
        let req_id = Uuid::new_v4();
        let create = handle_request(
            &state,
            &tx,
            drbot_protocol::Request::new(
                req_id,
                "session.create",
                SessionCreateParams {
                    title: Some("Test Session".to_string()),
                    provider: None,
                    model: Some("gpt-4o".to_string()),
                    system_prompt: Some("You are helpful.".to_string()),
                },
            ),
            &client_ctx,
        )
        .await;

        let create_result: drbot_protocol::SessionCreateResult = parse_success(create).await;

        // List sessions.
        let list = handle_request(
            &state,
            &tx,
            drbot_protocol::Request::new(req_id, "session.list", SessionListParams::default()),
            &client_ctx,
        )
        .await;

        let list_result: drbot_protocol::SessionListResult = parse_success(list).await;
        assert_eq!(list_result.total, 1);
        assert_eq!(list_result.sessions.len(), 1);
        assert_eq!(list_result.sessions[0].id, create_result.session_id);
        assert_eq!(
            list_result.sessions[0].title.as_deref(),
            Some("Test Session")
        );
    }

    async fn start_mock_openai(
        record: Arc<Mutex<Vec<serde_json::Value>>>,
    ) -> (String, tokio::sync::oneshot::Sender<()>) {
        async fn handler(
            State(record): State<Arc<Mutex<Vec<serde_json::Value>>>>,
            Json(payload): Json<serde_json::Value>,
        ) -> Json<serde_json::Value> {
            record.lock().await.push(payload);
            Json(serde_json::json!({
                "id": "chatcmpl_test",
                "object": "chat.completion",
                "created": 0,
                "model": "gpt-4o",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "hello" },
                    "finish_reason": "stop"
                }],
                "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
            }))
        }

        let app = Router::new()
            .route("/v1/chat/completions", post(handler))
            .with_state(record);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        (format!("http://{}", addr), shutdown_tx)
    }

    #[tokio::test]
    async fn chat_send_persists_and_resumes_by_session_id() {
        let record = Arc::new(Mutex::new(Vec::new()));
        let (base_url, shutdown) = start_mock_openai(record.clone()).await;

        let mut config = Config::default();
        config.storage.database_path = temp_db_path("chat_send_persists");
        config.providers = ProvidersConfig {
            default_provider: Some("openai".to_string()),
            default_model: None,
            anthropic: None,
            openai: Some(OpenAIConfig {
                api_key: "test-key".to_string(),
                base_url: Some(format!("{}/v1", base_url)),
                headers: Default::default(),
                organization: None,
                default_model: Some("gpt-4o".to_string()),
            }),
            ollama: None,
            bedrock: None,
            cli: vec![],
            openai_compatible: vec![],
        };

        let state = GatewayState::new(config);

        let (tx, _rx) = mpsc::channel::<String>(8);
        let client_id = Uuid::new_v4();
        let client_ctx = Arc::new(tokio::sync::RwLock::new(ClientContext::new(
            client_id,
            state.auth_required(),
        )));

        // First message creates a session.
        let req_id = Uuid::new_v4();
        let resp1 = handle_request(
            &state,
            &tx,
            drbot_protocol::Request::new(
                req_id,
                "chat.send",
                ChatSendParams {
                    session_id: None,
                    message: "first".to_string(),
                    model: None,
                    stream: false,
                    options: None,
                },
            ),
            &client_ctx,
        )
        .await;
        let result1: drbot_protocol::ChatSendResult = parse_success(resp1).await;

        // Second message should include prior history.
        let resp2 = handle_request(
            &state,
            &tx,
            drbot_protocol::Request::new(
                req_id,
                "chat.send",
                ChatSendParams {
                    session_id: Some(result1.session_id),
                    message: "second".to_string(),
                    model: None,
                    stream: false,
                    options: None,
                },
            ),
            &client_ctx,
        )
        .await;
        let _result2: drbot_protocol::ChatSendResult = parse_success(resp2).await;

        // Verify OpenAI received 2 requests and the second includes history.
        let recorded = record.lock().await.clone();
        assert_eq!(recorded.len(), 2);
        let first_msgs = recorded[0]["messages"].as_array().unwrap();
        assert_eq!(first_msgs.len(), 2);
        assert_eq!(first_msgs[0]["role"], "system");
        assert!(
            first_msgs[0]["content"]
                .as_str()
                .unwrap_or("")
                .contains("Workspace context:"),
            "expected workspace context in system prompt"
        );
        assert_eq!(first_msgs[1]["role"], "user");
        assert_eq!(first_msgs[1]["content"], "first");

        let second_msgs = recorded[1]["messages"].as_array().unwrap();
        assert_eq!(second_msgs.len(), 4);
        assert_eq!(second_msgs[0]["role"], "system");
        assert_eq!(second_msgs[1]["role"], "user");
        assert_eq!(second_msgs[1]["content"], "first");
        assert_eq!(second_msgs[2]["role"], "assistant");
        assert_eq!(second_msgs[2]["content"], "hello");
        assert_eq!(second_msgs[3]["role"], "user");
        assert_eq!(second_msgs[3]["content"], "second");

        // Verify session messages were persisted.
        let store = state.session_store().expect("store").clone();
        let session = store
            .get(result1.session_id)
            .await
            .expect("get")
            .expect("missing session");
        assert_eq!(session.messages.len(), 4);
        assert_eq!(session.metadata.message_count, 4);

        let _ = shutdown.send(());
    }

    #[tokio::test]
    async fn chat_send_injects_workspace_notes_recall() {
        let record = Arc::new(Mutex::new(Vec::new()));
        let (base_url, shutdown) = start_mock_openai(record.clone()).await;

        let mut config = Config::default();
        config.storage.database_path = temp_db_path("chat_send_workspace_notes_recall");
        config.providers = ProvidersConfig {
            default_provider: Some("openai".to_string()),
            default_model: None,
            anthropic: None,
            openai: Some(OpenAIConfig {
                api_key: "test-key".to_string(),
                base_url: Some(format!("{}/v1", base_url)),
                headers: Default::default(),
                organization: None,
                default_model: Some("gpt-4o".to_string()),
            }),
            ollama: None,
            bedrock: None,
            cli: vec![],
            openai_compatible: vec![],
        };

        let state = GatewayState::new(config);

        // Seed a knowledge-base note in the default workspace.
        let workspace = crate::openclaw::ensure_agent_workspace_bootstrap_best_effort(
            &state,
            crate::openclaw_paths::DEFAULT_AGENT_ID,
        );
        let memory_dir = workspace.join("memory");
        std::fs::create_dir_all(&memory_dir).expect("create memory dir");
        let token = format!("ZEBRA_{}", Uuid::new_v4());
        let note_path = memory_dir.join("test.md");
        std::fs::write(&note_path, format!("# Test\n\n{}\n", token)).expect("write note");

        let (tx, _rx) = mpsc::channel::<String>(8);
        let client_id = Uuid::new_v4();
        let client_ctx = Arc::new(tokio::sync::RwLock::new(ClientContext::new(
            client_id,
            state.auth_required(),
        )));

        let req_id = Uuid::new_v4();
        let resp = handle_request(
            &state,
            &tx,
            drbot_protocol::Request::new(
                req_id,
                "chat.send",
                ChatSendParams {
                    session_id: None,
                    message: token.clone(),
                    model: None,
                    stream: false,
                    options: None,
                },
            ),
            &client_ctx,
        )
        .await;
        let _result: drbot_protocol::ChatSendResult = parse_success(resp).await;

        let recorded = record.lock().await.clone();
        assert_eq!(recorded.len(), 1);
        let msgs = recorded[0]["messages"].as_array().unwrap();
        assert!(!msgs.is_empty());
        assert_eq!(msgs[0]["role"], "system");
        let sys = msgs[0]["content"].as_str().unwrap_or("");
        assert!(
            sys.contains("Relevant notes (workspace knowledge base):"),
            "expected notes recall section"
        );
        assert!(sys.contains("memory/test.md"), "expected note citation");
        assert!(sys.contains(&token), "expected note content to be injected");

        let _ = shutdown.send(());
    }

    #[tokio::test]
    async fn chat_send_denies_cross_user_session_access() {
        let record = Arc::new(Mutex::new(Vec::new()));
        let (base_url, shutdown) = start_mock_openai(record.clone()).await;

        let mut config = Config::default();
        config.storage.database_path = temp_db_path("chat_send_denies_cross_user");
        config.providers = ProvidersConfig {
            default_provider: Some("openai".to_string()),
            default_model: None,
            anthropic: None,
            openai: Some(OpenAIConfig {
                api_key: "test-key".to_string(),
                base_url: Some(format!("{}/v1", base_url)),
                headers: Default::default(),
                organization: None,
                default_model: Some("gpt-4o".to_string()),
            }),
            ollama: None,
            bedrock: None,
            cli: vec![],
            openai_compatible: vec![],
        };

        let state = GatewayState::new(config);

        let (tx, _rx) = mpsc::channel::<String>(8);

        // Client A creates a session via chat.send.
        let client_ctx_a = Arc::new(tokio::sync::RwLock::new(ClientContext::new(
            Uuid::new_v4(),
            state.auth_required(),
        )));
        let req_id = Uuid::new_v4();
        let resp1 = handle_request(
            &state,
            &tx,
            drbot_protocol::Request::new(
                req_id,
                "chat.send",
                ChatSendParams {
                    session_id: None,
                    message: "first".to_string(),
                    model: None,
                    stream: false,
                    options: None,
                },
            ),
            &client_ctx_a,
        )
        .await;
        let result1: drbot_protocol::ChatSendResult = parse_success(resp1).await;

        // Client B tries to use Client A's session_id: should be denied.
        let client_ctx_b = Arc::new(tokio::sync::RwLock::new(ClientContext::new(
            Uuid::new_v4(),
            state.auth_required(),
        )));
        let resp2 = handle_request(
            &state,
            &tx,
            drbot_protocol::Request::new(
                req_id,
                "chat.send",
                ChatSendParams {
                    session_id: Some(result1.session_id),
                    message: "second".to_string(),
                    model: None,
                    stream: false,
                    options: None,
                },
            ),
            &client_ctx_b,
        )
        .await;

        match resp2 {
            WsMessage::Response(resp) => {
                let err = resp.error.expect("expected error");
                assert_eq!(
                    i32::from(err.code),
                    i32::from(drbot_protocol::ErrorCode::PermissionDenied)
                );
            }
            other => panic!("expected response, got: {:?}", other),
        }

        // Only the first request should reach OpenAI.
        let recorded = record.lock().await.clone();
        assert_eq!(recorded.len(), 1);

        let _ = shutdown.send(());
    }
}
