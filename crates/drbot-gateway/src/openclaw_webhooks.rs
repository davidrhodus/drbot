//! OpenClaw-style inbound webhook endpoints (/hooks/*).
//!
//! OpenClaw v2026.2.12 parity: adds /hooks/wake and /hooks/agent.

use crate::state::GatewayState;
use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use uuid::Uuid;

const DEFAULT_AGENT_TIMEOUT_MS: u64 = 120_000;
const AUTH_FAIL_WINDOW: Duration = Duration::from_secs(60);
const AUTH_FAIL_MAX_ATTEMPTS: usize = 10;

#[derive(Debug, Default)]
struct AuthFailureThrottle {
    inner: Mutex<HashMap<IpAddr, VecDeque<Instant>>>,
}

impl AuthFailureThrottle {
    async fn check_blocked(&self, ip: IpAddr) -> Option<u64> {
        let mut map = self.inner.lock().await;
        let attempts = map.get_mut(&ip)?;
        let now = Instant::now();
        prune_attempts(attempts, now);
        if attempts.len() < AUTH_FAIL_MAX_ATTEMPTS {
            if attempts.is_empty() {
                map.remove(&ip);
            }
            return None;
        }
        Some(retry_after_secs(attempts, now))
    }

    async fn record_failure(&self, ip: IpAddr) -> Option<u64> {
        let mut map = self.inner.lock().await;
        let attempts = map.entry(ip).or_insert_with(VecDeque::new);
        let now = Instant::now();
        prune_attempts(attempts, now);
        attempts.push_back(now);
        if attempts.len() < AUTH_FAIL_MAX_ATTEMPTS {
            return None;
        }
        Some(retry_after_secs(attempts, now))
    }

    async fn clear(&self, ip: IpAddr) {
        let mut map = self.inner.lock().await;
        map.remove(&ip);
    }
}

fn prune_attempts(attempts: &mut VecDeque<Instant>, now: Instant) {
    while let Some(front) = attempts.front().copied() {
        if now.duration_since(front) > AUTH_FAIL_WINDOW {
            attempts.pop_front();
        } else {
            break;
        }
    }
}

fn retry_after_secs(attempts: &VecDeque<Instant>, now: Instant) -> u64 {
    let Some(oldest) = attempts.front().copied() else {
        return 1;
    };
    let elapsed = now.duration_since(oldest);
    if elapsed >= AUTH_FAIL_WINDOW {
        1
    } else {
        (AUTH_FAIL_WINDOW - elapsed).as_secs().max(1)
    }
}

fn throttle() -> &'static AuthFailureThrottle {
    static THROTTLE: OnceLock<AuthFailureThrottle> = OnceLock::new();
    THROTTLE.get_or_init(AuthFailureThrottle::default)
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

fn extract_hook_token(headers: &HeaderMap) -> Option<String> {
    if let Some(token) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_bearer_token)
    {
        return Some(token.to_string());
    }
    headers
        .get("x-openclaw-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn json_error(status: StatusCode, message: &str) -> axum::response::Response {
    (
        status,
        Json(json!({
            "ok": false,
            "error": { "message": message }
        })),
    )
        .into_response()
}

fn json_error_with_retry_after(
    status: StatusCode,
    message: &str,
    retry_after_secs: u64,
) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        headers.insert("retry-after", v);
    }
    (
        status,
        headers,
        Json(json!({
            "ok": false,
            "error": { "message": message, "retryAfterSecs": retry_after_secs }
        })),
    )
        .into_response()
}

async fn authorize_hooks(
    state: &GatewayState,
    peer: SocketAddr,
    headers: &HeaderMap,
) -> Result<(), axum::response::Response> {
    if !state.config().hooks.enabled {
        return Err(json_error(StatusCode::NOT_FOUND, "hooks disabled"));
    }

    let expected = state
        .config()
        .hooks
        .token
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let Some(_expected) = expected else {
        return Err(json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "hooks enabled but hooks.token is not configured",
        ));
    };

    let ip = peer.ip();
    if let Some(retry_after) = throttle().check_blocked(ip).await {
        return Err(json_error_with_retry_after(
            StatusCode::TOO_MANY_REQUESTS,
            "too many auth failures",
            retry_after,
        ));
    }

    let provided = extract_hook_token(headers).unwrap_or_default();
    if !state.validate_hooks_token(provided.as_str()) {
        if let Some(retry_after) = throttle().record_failure(ip).await {
            return Err(json_error_with_retry_after(
                StatusCode::TOO_MANY_REQUESTS,
                "too many auth failures",
                retry_after,
            ));
        }
        return Err(json_error(StatusCode::UNAUTHORIZED, "unauthorized"));
    }

    throttle().clear(ip).await;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WakePayload {
    #[serde(default)]
    session_key: Option<String>,
    message: String,
}

pub(crate) async fn hooks_wake_handler(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(payload): Json<WakePayload>,
) -> impl IntoResponse {
    if let Err(resp) = authorize_hooks(&state, peer, &headers).await {
        return resp;
    }

    let message = payload.message.trim();
    if message.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "message required");
    }
    if let Some(max_chars) = state.config().hooks.max_message_chars {
        if message.chars().count() as u64 > max_chars {
            return json_error(StatusCode::BAD_REQUEST, "message too long");
        }
    }

    // drbot heartbeats currently operate on the main session key.
    let session_key = payload
        .session_key
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("main");
    let session_key = crate::openclaw::canonicalize_openclaw_session_key(
        crate::openclaw_paths::DEFAULT_AGENT_ID,
        session_key,
    );

    state
        .openclaw_enqueue_system_event(session_key.as_str(), message, None)
        .await;
    crate::openclaw_heartbeat::request_heartbeat_now(&state, Some("hooks.wake".to_string()))
        .await;

    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentPayload {
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    session_key: Option<String>,
    message: String,
    #[serde(default)]
    stream: Option<bool>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default, rename = "timeoutSeconds")]
    timeout_seconds: Option<u64>,
    #[serde(default, rename = "extraSystemPrompt")]
    extra_system_prompt: Option<String>,
}

fn normalize_message_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Keep ids short for easier log correlation.
    Some(trimmed.chars().take(64).collect())
}

fn derive_message_id() -> String {
    let id = Uuid::new_v4().to_string();
    id.chars().filter(|c| *c != '-').take(8).collect()
}

fn resolve_hook_session_key(
    agent_id: &str,
    requested: Option<&str>,
    cfg: &drbot_core::config::HooksConfig,
) -> Result<String, String> {
    let default_raw = cfg
        .default_session_key
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("main");
    let default = crate::openclaw::canonicalize_openclaw_session_key(agent_id, default_raw);

    let Some(requested) = requested.map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return Ok(default);
    };
    let requested = crate::openclaw::canonicalize_openclaw_session_key(agent_id, requested);

    if !cfg.allow_request_session_key {
        if requested == default {
            return Ok(default);
        }
        return Err("sessionKey overrides are disabled".to_string());
    }

    if !cfg.allowed_session_key_prefixes.is_empty()
        && !cfg
            .allowed_session_key_prefixes
            .iter()
            .filter_map(|p| {
                let trimmed = p.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            })
            .any(|p| requested.starts_with(p))
    {
        return Err("sessionKey not allowed".to_string());
    }

    Ok(requested)
}

pub(crate) async fn hooks_agent_handler(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(payload): Json<AgentPayload>,
) -> impl IntoResponse {
    if let Err(resp) = authorize_hooks(&state, peer, &headers).await {
        return resp;
    }

    if payload.stream.unwrap_or(false) {
        return json_error(
            StatusCode::BAD_REQUEST,
            "stream=true is not supported by this gateway",
        );
    }

    let message = payload.message.trim();
    if message.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "message required");
    }
    if let Some(max_chars) = state.config().hooks.max_message_chars {
        if message.chars().count() as u64 > max_chars {
            return json_error(StatusCode::BAD_REQUEST, "message too long");
        }
    }

    let agent_id = payload
        .agent_id
        .as_deref()
        .unwrap_or(crate::openclaw_paths::DEFAULT_AGENT_ID);
    let agent_id = crate::openclaw_paths::normalize_agent_id(agent_id);
    if !state.config().hooks.allowed_agent_ids.is_empty() {
        let allowed = state.config().hooks.allowed_agent_ids.iter().any(|entry| {
            crate::openclaw_paths::normalize_agent_id(entry) == agent_id
        });
        if !allowed {
            return json_error(StatusCode::FORBIDDEN, "agentId not allowed");
        }
    }

    let message_id = payload
        .id
        .as_deref()
        .and_then(normalize_message_id)
        .or_else(|| payload.idempotency_key.as_deref().and_then(normalize_message_id))
        .unwrap_or_else(derive_message_id);

    let timeout_ms = payload
        .timeout_ms
        .or_else(|| payload.timeout_seconds.map(|s| s.saturating_mul(1000)))
        .unwrap_or(DEFAULT_AGENT_TIMEOUT_MS)
        .clamp(1_000, 900_000);

    let session_key = match resolve_hook_session_key(
        agent_id.as_str(),
        payload.session_key.as_deref(),
        &state.config().hooks,
    ) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };

    let extra_system_prompt = payload
        .extra_system_prompt
        .as_deref()
        .map(|s| s.to_string());

    let req_id = format!("hooks-agent-{}", message_id);

    let result = crate::openclaw::openclaw_run_agent_for_webhook(
        state.clone(),
        peer,
        req_id,
        message_id.clone(),
        agent_id.clone(),
        session_key.clone(),
        message.to_string(),
        Some(timeout_ms),
        extra_system_prompt,
    )
    .await;

    match result {
        Ok(payload) => {
            let result_obj = payload.get("result").cloned().unwrap_or(serde_json::Value::Null);
            let usage = result_obj
                .get("usage")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            (
                StatusCode::OK,
                Json(json!({
                    "ok": true,
                    "agentId": agent_id,
                    "sessionKey": session_key,
                    "messageId": message_id,
                    "stream": false,
                    "result": result_obj,
                    "usage": usage,
                    "error": serde_json::Value::Null
                })),
            )
                .into_response()
        }
        Err(err) => (
            StatusCode::OK,
            Json(json!({
                "ok": false,
                "agentId": agent_id,
                "sessionKey": session_key,
                "messageId": message_id,
                "stream": false,
                "result": serde_json::Value::Null,
                "usage": serde_json::Value::Null,
                "error": { "code": err.code, "message": err.message, "details": err.details }
            })),
        )
            .into_response(),
    }
}
