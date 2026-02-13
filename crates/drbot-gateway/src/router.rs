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
use drbot_providers::{ChatOptions, StreamEvent as ProviderStreamEvent};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Create the HTTP router.
pub fn create_router(state: GatewayState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let hooks_base = state.config().hooks.path.trim();
    let hooks_base = if hooks_base.is_empty() { "/hooks" } else { hooks_base };
    let hooks_base = hooks_base.trim_end_matches('/');
    let hooks_base = if hooks_base == "/" { "/hooks" } else { hooks_base };
    let hooks_base = hooks_base.to_string();
    let hooks_max_body = state
        .config()
        .hooks
        .max_body_bytes
        .unwrap_or(256_000)
        .clamp(1, 5_000_000) as usize;
    let hooks_router = Router::new()
        .route("/wake", post(crate::openclaw_webhooks::hooks_wake_handler))
        .route("/agent", post(crate::openclaw_webhooks::hooks_agent_handler))
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
        event_types, AuthLoginParams, AuthLoginResult, ChatSendParams, ErrorCode, Event,
        ProviderListResult, SessionCreateParams, SessionCreateResult, SessionListParams,
        SessionListResult, SystemInfoResult, SystemPingResult, WsMessage,
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

            // Check if provider is configured
            let provider = match state.provider() {
                Some(p) => p,
                None => {
                    return WsMessage::error(
                        request.id,
                        ErrorCode::ProviderError,
                        "No AI provider configured",
                    );
                }
            };

            // Get or create session
            let session_id = params.session_id.unwrap_or_else(uuid::Uuid::new_v4);
            let message_id = uuid::Uuid::new_v4();

            // Build messages for the provider
            let mut messages = Vec::new();

            // Build chat options
            let options = ChatOptions {
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

                messages.extend(session.messages.clone());
                // Keep the session for persistence updates below.
                persisted_session = Some({
                    // Ensure metadata stays consistent with prompt options.
                    if session.model.is_none() {
                        session.model = params.model.clone();
                    }
                    if session.system_prompt.is_none() {
                        session.system_prompt = options.system_prompt.clone();
                    }
                    session
                });
            }

            // Add the user's message (to provider only; persistence happens after a successful response).
            let user_msg = Message::user(&params.message);
            messages.push(user_msg.clone());

            let model_name = options
                .model
                .clone()
                .unwrap_or_else(|| "default".to_string());

            if params.stream {
                // Streaming response
                match provider.stream(&messages, options).await {
                    Ok(mut stream) => {
                        // Send start event
                        let start_event = Event::new(
                            event_types::CHAT_STREAM_START,
                            drbot_protocol::event::chat::StreamStartEvent {
                                request_id: request.id,
                                session_id,
                                message_id,
                                model: model_name.clone(),
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
                                    if let Ok(json) =
                                        serde_json::to_string(&WsMessage::Event(delta_event))
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
                                    if let Ok(json) =
                                        serde_json::to_string(&WsMessage::Event(error_event))
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
                                usage: final_usage.map(|u| drbot_protocol::response::TokenUsage {
                                    input_tokens: u.input_tokens,
                                    output_tokens: u.output_tokens,
                                }),
                            },
                        )
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to stream");
                        WsMessage::error(request.id, ErrorCode::ProviderError, e.to_string())
                    }
                }
            } else {
                // Non-streaming response
                match provider.chat(&messages, options).await {
                    Ok(response) => {
                        if let (Some(store), Some(session)) =
                            (state.session_store(), persisted_session.as_mut())
                        {
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
                                usage: response.usage.map(|u| {
                                    drbot_protocol::response::TokenUsage {
                                        input_tokens: u.input_tokens,
                                        output_tokens: u.output_tokens,
                                    }
                                }),
                            },
                        )
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to chat");
                        WsMessage::error(request.id, ErrorCode::ProviderError, e.to_string())
                    }
                }
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
            session.model = params.model.clone();
            session.system_prompt = params.system_prompt.clone();

            if let Err(e) = store.create(&session).await {
                error!(error = %e, session_id = %session_id, "Failed to create session");
                return WsMessage::error(request.id, ErrorCode::SessionError, e.to_string());
            }

            WsMessage::success(request.id, SessionCreateResult { session_id })
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
            let providers = if let Some(provider) = state.provider() {
                vec![drbot_protocol::ProviderInfo {
                    name: provider.name().to_string(),
                    status: "configured".to_string(),
                    models: provider.models().iter().map(|m| m.id.clone()).collect(),
                }]
            } else {
                vec![]
            };

            WsMessage::success(request.id, ProviderListResult { providers })
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
        let mut path = std::env::temp_dir();
        path.push(format!("drbot-gateway-{}-{}.db", test_name, Uuid::new_v4()));
        path
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
        assert_eq!(first_msgs.len(), 1);
        assert_eq!(first_msgs[0]["role"], "user");
        assert_eq!(first_msgs[0]["content"], "first");

        let second_msgs = recorded[1]["messages"].as_array().unwrap();
        assert_eq!(second_msgs.len(), 3);
        assert_eq!(second_msgs[0]["role"], "user");
        assert_eq!(second_msgs[0]["content"], "first");
        assert_eq!(second_msgs[1]["role"], "assistant");
        assert_eq!(second_msgs[1]["content"], "hello");
        assert_eq!(second_msgs[2]["role"], "user");
        assert_eq!(second_msgs[2]["content"], "second");

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
