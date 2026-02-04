//! OpenClaw Gateway protocol (v3) compatibility WebSocket handler.
//!
//! This implements a subset of OpenClaw's gateway protocol so the OpenClaw
//! Control UI (and other OpenClaw clients) can interoperate with drbot.
//!
//! Endpoint: `/openclaw/ws`

use crate::state::{GatewayState, OpenclawClient, OpenclawOutbound};
use crate::openclaw_exec_approvals::ExecApprovalRequestPayload;
use crate::openclaw_system::SystemPresencePayload;
use axum::extract::ws::WebSocket;
use chrono::{Datelike, Timelike};
use drbot_agents::{
    Agent as DrbotAgent, AgentConfig as DrbotAgentConfig, AgentEvent as DrbotAgentEvent,
    AgentMessage as DrbotAgentMessage, AgentRole as DrbotAgentRole, BuiltinTools,
};
use drbot_base64_util::Base64Config;
use drbot_browser::BrowserAutomation;
use drbot_core::message::{Content, ImageSource, Message, OutgoingMessage, Role};
use drbot_protocol::openclaw::{
    error_codes, ConnectParams, ErrorShape, EventFrame, GatewayFrame, HelloFeatures, HelloOk,
    HelloPolicy, HelloServer, PresenceEntry, RequestFrame, ResponseFrame, Snapshot, StateVersion,
    OPENCLAW_PROTOCOL_VERSION,
};
use drbot_providers::{ChatOptions, Provider, StreamEvent as ProviderStreamEvent, Usage};
use drbot_voice::{ElevenLabsTts, OpenAiTts, SystemTts, TextToSpeech};
use futures::StreamExt;
use ring::{digest, signature};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch, Mutex, Notify};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

const DEFAULT_TICK_INTERVAL_MS: u64 = 30_000;
const OPENCLAW_CHAT_DELTA_THROTTLE_MS: u64 = 150;
const DEFAULT_MAX_PAYLOAD_BYTES: u64 = 512 * 1024;
const DEFAULT_MAX_BUFFERED_BYTES: u64 = 1_572_864; // 1.5 MiB
const DEFAULT_HANDSHAKE_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_MAX_CHAT_HISTORY_BYTES: usize = 6 * 1024 * 1024;

fn resolve_openclaw_max_payload_bytes() -> u64 {
    static CACHED: OnceLock<u64> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("DRBOT_OPENCLAW_MAX_PAYLOAD_BYTES")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_MAX_PAYLOAD_BYTES)
    })
}

fn resolve_openclaw_max_buffered_bytes() -> u64 {
    static CACHED: OnceLock<u64> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("DRBOT_OPENCLAW_MAX_BUFFERED_BYTES")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_MAX_BUFFERED_BYTES)
    })
}

fn resolve_openclaw_handshake_timeout_ms() -> u64 {
    static CACHED: OnceLock<u64> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("DRBOT_OPENCLAW_HANDSHAKE_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_HANDSHAKE_TIMEOUT_MS)
    })
}

fn resolve_openclaw_max_chat_history_bytes() -> usize {
    static CACHED: OnceLock<usize> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("DRBOT_OPENCLAW_MAX_CHAT_HISTORY_BYTES")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_MAX_CHAT_HISTORY_BYTES)
    })
}

/// OpenClaw methods we currently advertise/support on `/openclaw/ws`.
///
/// Note: It's OK to expand this list over time as drbot gains parity.
const METHODS: &[&str] = &[
    // Core
    "health",
    "status",
    "logs.tail",
    "models.list",
    "last-heartbeat",
    "set-heartbeats",
    "system-presence",
    "system-event",
    "usage.status",
    "usage.cost",
    // Talk / voice
    "talk.mode",
    "voicewake.get",
    "voicewake.set",
    "tts.status",
    "tts.providers",
    "tts.enable",
    "tts.disable",
    "tts.convert",
    "tts.setProvider",
    // Exec approvals (schema-light; UI uses this on Nodes tab)
    "exec.approvals.get",
    "exec.approvals.set",
    "exec.approvals.node.get",
    "exec.approvals.node.set",
    "exec.approval.request",
    "exec.approval.resolve",
    // Wizard (stubbed)
    "wizard.start",
    "wizard.next",
    "wizard.cancel",
    "wizard.status",
    // Config
    "config.get",
    "config.schema",
    "config.set",
    "config.apply",
    "config.patch",
    // Sessions
    "sessions.list",
    "sessions.preview",
    "sessions.resolve",
    "sessions.patch",
    "sessions.reset",
    "sessions.delete",
    "sessions.compact",
    // Agents + Skills (schema-light; UI needs these on connect)
    "agent",
    "agent.wait",
    "agents.list",
    "agent.identity.get",
    "agents.files.list",
    "agents.files.get",
    "agents.files.set",
    "skills.status",
    "skills.bins",
    "skills.install",
    "skills.update",
    // Integrations (non-OpenClaw; for hackathon interop)
    "colosseum.request",
    "moltbook.request",
    // Browser (stubbed/minimal)
    "browser.request",
    // Nodes + Devices (schema-light)
    "node.pair.request",
    "node.pair.list",
    "node.pair.approve",
    "node.pair.reject",
    "node.pair.verify",
    "node.rename",
    "node.list",
    "node.describe",
    "node.invoke",
    "node.invoke.result",
    "node.event",
    "device.pair.list",
    "device.pair.approve",
    "device.pair.reject",
    "device.token.rotate",
    "device.token.revoke",
    // Channels (schema-light; return best-effort snapshots)
    "channels.status",
    "channels.logout",
    "web.login.start",
    "web.login.wait",
    // Send (stubbed)
    "send",
    "poll",
    "wake",
    // WebChat-native chat
    "chat.history",
    "chat.send",
    "chat.abort",
    "chat.inject",
    // Cron
    "cron.status",
    "cron.list",
    "cron.add",
    "cron.update",
    "cron.remove",
    "cron.run",
    "cron.runs",
    // Update (stubbed)
    "update.run",
];

/// OpenClaw events we currently emit/advertise.
const EVENTS: &[&str] = &[
    "connect.challenge",
    "agent",
    "chat",
    "presence",
    "tick",
    "talk.mode",
    "health",
    "heartbeat",
    "cron",
    "node.pair.requested",
    "node.pair.resolved",
    "node.invoke.request",
    "device.pair.requested",
    "device.pair.resolved",
    "voicewake.changed",
    "exec.approval.requested",
    "exec.approval.resolved",
    "shutdown",
];

#[derive(Clone)]
struct AgentDeliveryTarget {
    channel: String,
    to: String,
}

#[derive(Debug, Clone)]
struct WizardSessionState {
    step: u8,
}

#[derive(Clone)]
struct ConnCtx {
    state: GatewayState,
    tx: mpsc::UnboundedSender<OpenclawOutbound>,
    queued_bytes: Arc<AtomicU64>,
    closing: Arc<std::sync::atomic::AtomicBool>,
    event_seq: Arc<AtomicU64>,
    run_seq: Arc<Mutex<HashMap<String, u64>>>,
    wizard_sessions: Arc<Mutex<HashMap<String, WizardSessionState>>>,
    shutdown_tx: watch::Sender<bool>,
    conn_id: String,
    peer: SocketAddr,
}

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(0)
}

fn sha256_hex(raw: &str) -> String {
    let d = digest::digest(&digest::SHA256, raw.as_bytes());
    drbot_hex_util::encode(d.as_ref())
}

fn sha256_hex_bytes(raw: &[u8]) -> String {
    let d = digest::digest(&digest::SHA256, raw);
    drbot_hex_util::encode(d.as_ref())
}

fn base64_decode_url_safe_best_effort(input: &str) -> Option<Vec<u8>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    drbot_base64_util::decode_config(trimmed, Base64Config::URL_SAFE_NO_PAD)
        .or_else(|_| drbot_base64_util::decode_config(trimmed, Base64Config::STANDARD_NO_PAD))
        .or_else(|_| drbot_base64_util::decode_config(trimmed, Base64Config::STANDARD))
        .ok()
}

fn base64_encode_url_safe_no_pad(raw: &[u8]) -> String {
    drbot_base64_util::encode_config(raw, Base64Config::URL_SAFE_NO_PAD)
}

const DEVICE_SIGNATURE_SKEW_MS: u64 = 10 * 60 * 1000;

fn build_device_auth_payload(params: DeviceAuthPayloadParams<'_>) -> String {
    let version = if params.nonce.is_some() { "v2" } else { "v1" };
    let scopes = params.scopes.join(",");
    let token = params.token.unwrap_or("").trim();
    let mut parts = vec![
        version.to_string(),
        params.device_id.trim().to_string(),
        params.client_id.trim().to_string(),
        params.client_mode.trim().to_string(),
        params.role.trim().to_string(),
        scopes,
        params.signed_at_ms.to_string(),
        token.to_string(),
    ];
    if version == "v2" {
        parts.push(params.nonce.unwrap_or("").trim().to_string());
    }
    parts.join("|")
}

struct DeviceAuthPayloadParams<'a> {
    device_id: &'a str,
    client_id: &'a str,
    client_mode: &'a str,
    role: &'a str,
    scopes: &'a [String],
    signed_at_ms: u64,
    token: Option<&'a str>,
    nonce: Option<&'a str>,
}

fn verify_device_signature(public_key_raw: &[u8], payload: &str, signature_raw: &[u8]) -> bool {
    if public_key_raw.len() != 32 || signature_raw.is_empty() {
        return false;
    }
    signature::UnparsedPublicKey::new(&signature::ED25519, public_key_raw)
        .verify(payload.as_bytes(), signature_raw)
        .is_ok()
}

// Keep aligned with OpenClaw defaults (server-constants.ts).
const OPENCLAW_IDEMPOTENCY_TTL_MS: u64 = 5 * 60_000;
const OPENCLAW_IDEMPOTENCY_MAX_KEYS: usize = 1000;

#[derive(Debug, Clone)]
struct OpenclawDedupeEntry {
    ts: u64,
    ok: bool,
    payload: Option<serde_json::Value>,
    error: Option<ErrorShape>,
}

static OPENCLAW_DEDUPE: OnceLock<Mutex<HashMap<String, OpenclawDedupeEntry>>> = OnceLock::new();

fn openclaw_dedupe_store() -> &'static Mutex<HashMap<String, OpenclawDedupeEntry>> {
    OPENCLAW_DEDUPE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn openclaw_dedupe_get(key: &str) -> Option<OpenclawDedupeEntry> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let now = now_ms();
    let mut map = openclaw_dedupe_store().lock().await;
    let Some(entry) = map.get(key).cloned() else {
        return None;
    };
    if now.saturating_sub(entry.ts) < OPENCLAW_IDEMPOTENCY_TTL_MS {
        Some(entry)
    } else {
        map.remove(key);
        None
    }
}

async fn openclaw_dedupe_put(key: &str, entry: OpenclawDedupeEntry) {
    let key = key.trim();
    if key.is_empty() {
        return;
    }
    let now = now_ms();
    let mut map = openclaw_dedupe_store().lock().await;
    map.insert(key.to_string(), OpenclawDedupeEntry { ts: now, ..entry });

    if map.len() > OPENCLAW_IDEMPOTENCY_MAX_KEYS {
        let cutoff = now.saturating_sub(OPENCLAW_IDEMPOTENCY_TTL_MS);
        map.retain(|_, v| v.ts >= cutoff);

        // Hard-cap if still too large (drop oldest keys).
        if map.len() > OPENCLAW_IDEMPOTENCY_MAX_KEYS {
            let mut items: Vec<(String, u64)> =
                map.iter().map(|(k, v)| (k.clone(), v.ts)).collect();
            items.sort_by_key(|(_, ts)| *ts);
            let drop_count = items.len().saturating_sub(OPENCLAW_IDEMPOTENCY_MAX_KEYS);
            for (k, _) in items.into_iter().take(drop_count) {
                map.remove(&k);
            }
        }
    }
}

#[derive(Debug)]
struct OpenclawInflightEntry {
    done: Notify,
    result: Mutex<Option<OpenclawDedupeEntry>>,
}

impl OpenclawInflightEntry {
    fn new() -> Self {
        Self {
            done: Notify::new(),
            result: Mutex::new(None),
        }
    }
}

static OPENCLAW_INFLIGHT: OnceLock<Mutex<HashMap<String, Arc<OpenclawInflightEntry>>>> =
    OnceLock::new();

fn openclaw_inflight_store() -> &'static Mutex<HashMap<String, Arc<OpenclawInflightEntry>>> {
    OPENCLAW_INFLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn openclaw_inflight_get_or_insert(
    key: &str,
) -> Option<(Arc<OpenclawInflightEntry>, bool)> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    let mut map = openclaw_inflight_store().lock().await;
    if let Some(existing) = map.get(key) {
        return Some((existing.clone(), false));
    }
    let entry = Arc::new(OpenclawInflightEntry::new());
    map.insert(key.to_string(), entry.clone());
    Some((entry, true))
}

async fn openclaw_inflight_wait(entry: &Arc<OpenclawInflightEntry>) -> OpenclawDedupeEntry {
    loop {
        if let Some(result) = entry.result.lock().await.clone() {
            return result;
        }
        entry.done.notified().await;
    }
}

async fn openclaw_inflight_finish(key: &str, entry: &Arc<OpenclawInflightEntry>, result: OpenclawDedupeEntry) {
    {
        let mut st = entry.result.lock().await;
        *st = Some(result);
    }
    entry.done.notify_waiters();
    let mut map = openclaw_inflight_store().lock().await;
    if let Some(cur) = map.get(key) {
        if Arc::ptr_eq(cur, entry) {
            map.remove(key);
        }
    }
}

async fn openclaw_idempotent_run<F, Fut>(key: &str, op: F) -> OpenclawDedupeEntry
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = OpenclawDedupeEntry>,
{
    let key = key.trim();
    if key.is_empty() {
        return op().await;
    }

    if let Some(cached) = openclaw_dedupe_get(key).await {
        return cached;
    }

    let Some((inflight, is_leader)) = openclaw_inflight_get_or_insert(key).await else {
        return op().await;
    };
    if !is_leader {
        return openclaw_inflight_wait(&inflight).await;
    }

    let result = op().await;
    openclaw_dedupe_put(key, result.clone()).await;
    openclaw_inflight_finish(key, &inflight, result.clone()).await;
    result
}

fn chat_send_dedupe_key(_session_key: &str, run_id: &str) -> String {
    // OpenClaw dedupe uses the client-provided idempotency key (runId).
    format!("chat:{}", run_id.trim().replace('\n', " ").replace('\r', " "))
}

fn resolve_config_paths() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("drbot.toml"),
        PathBuf::from("config/drbot.toml"),
    ];
    if let Some(dir) = drbot_core::Config::config_dir() {
        paths.push(dir.join("config.toml"));
        // Back-compat.
        paths.push(dir.join("drbot.toml"));
    }
    paths
}

fn resolve_config_path_for_read() -> PathBuf {
    for path in resolve_config_paths() {
        if path.exists() {
            return path;
        }
    }
    resolve_config_path_for_write()
}

fn resolve_config_path_for_write() -> PathBuf {
    if let Some(dir) = drbot_core::Config::config_dir() {
        return dir.join("config.toml");
    }
    PathBuf::from("drbot.toml")
}

fn resolve_data_dir() -> Option<PathBuf> {
    drbot_core::Config::data_dir()
}

fn resolve_openclaw_state_dir(state: &GatewayState) -> Option<PathBuf> {
    crate::openclaw_paths::resolve_openclaw_state_dir(state.config())
}

fn resolve_agent_workspace_dir(agent_id: &str) -> PathBuf {
    let safe = agent_id.trim();
    let safe = if safe.is_empty() { "default" } else { safe };

    if let Some(dir) = drbot_core::Config::config_dir() {
        return dir.join("agents").join(safe);
    }
    PathBuf::from("agents").join(safe)
}

fn read_json_file<T: DeserializeOwned>(path: &PathBuf) -> Option<T> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_json_atomic<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), ErrorShape> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ErrorShape::new(
                error_codes::UNAVAILABLE,
                format!("failed to create dir {}: {}", parent.to_string_lossy(), e),
            )
        })?;
    }
    let raw = serde_json::to_string_pretty(value).map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("failed to serialize json: {}", e),
        )
    })?;
    let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    std::fs::write(&tmp, raw).map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("failed to write {}: {}", tmp.to_string_lossy(), e),
        )
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!(
                "failed to move {} -> {}: {}",
                tmp.to_string_lossy(),
                path.to_string_lossy(),
                e
            ),
        )
    })?;
    Ok(())
}

fn session_key_to_channel(key: &str) -> (String, String) {
    // OpenClaw uses a "key" string; drbot sessions are keyed by (channel_type, channel_id).
    // We map:
    // - keys without ":" => ("openclaw", key)
    // - keys with ":" => split at first ":" => (channel_type, channel_id)
    if let Some((left, right)) = key.split_once(':') {
        (left.to_string(), right.to_string())
    } else {
        ("openclaw".to_string(), key.to_string())
    }
}

const WS_CLOSE_CODE_POLICY_VIOLATION: u16 = 1008;
const WS_CLOSE_CODE_MESSAGE_TOO_BIG: u16 = 1009;

fn truncate_ws_close_reason(reason: &str) -> String {
    // RFC 6455 close reason max is 123 bytes. Keep it short and ASCII.
    const MAX_BYTES: usize = 120;
    let trimmed = reason.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for ch in trimmed.chars() {
        if !ch.is_ascii() {
            continue;
        }
        if out.len() + ch.len_utf8() > MAX_BYTES {
            break;
        }
        out.push(ch);
    }
    out
}

fn openclaw_request_close(
    tx: &mpsc::UnboundedSender<OpenclawOutbound>,
    closing: &Arc<std::sync::atomic::AtomicBool>,
    code: u16,
    reason: &str,
) {
    if closing.swap(true, Ordering::Relaxed) {
        return;
    }
    let reason = truncate_ws_close_reason(reason);
    let _ = tx.send(OpenclawOutbound::Close { code, reason });
}

fn openclaw_send_text(
    tx: &mpsc::UnboundedSender<OpenclawOutbound>,
    queued_bytes: &Arc<AtomicU64>,
    closing: &Arc<std::sync::atomic::AtomicBool>,
    text: String,
    drop_if_slow: bool,
) -> bool {
    if closing.load(Ordering::Relaxed) {
        return false;
    }

    let len = text.len() as u64;
    let max_buffered_bytes = resolve_openclaw_max_buffered_bytes();
    let prev = queued_bytes.fetch_add(len, Ordering::Relaxed);
    let next = prev.saturating_add(len);

    if next > max_buffered_bytes {
        let _ = queued_bytes.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
            Some(cur.saturating_sub(len))
        });
        if drop_if_slow {
            return false;
        }
        openclaw_request_close(tx, closing, WS_CLOSE_CODE_POLICY_VIOLATION, "slow consumer");
        return false;
    }

    if tx.send(OpenclawOutbound::Text(text)).is_err() {
        let _ = queued_bytes.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
            Some(cur.saturating_sub(len))
        });
        return false;
    }
    true
}

async fn send_frame(
    tx: &mpsc::UnboundedSender<OpenclawOutbound>,
    queued_bytes: &Arc<AtomicU64>,
    closing: &Arc<std::sync::atomic::AtomicBool>,
    frame: &GatewayFrame,
) {
    match serde_json::to_string(frame) {
        Ok(json) => {
            let _ = openclaw_send_text(tx, queued_bytes, closing, json, false);
        }
        Err(e) => {
            warn!(error = %e, "Failed to serialize OpenClaw frame");
        }
    }
}

async fn send_event(
    tx: &mpsc::UnboundedSender<OpenclawOutbound>,
    queued_bytes: &Arc<AtomicU64>,
    closing: &Arc<std::sync::atomic::AtomicBool>,
    event_seq: &AtomicU64,
    event: &str,
    payload: serde_json::Value,
    state_version: Option<StateVersion>,
    drop_if_slow: bool,
) {
    let seq = event_seq.fetch_add(1, Ordering::Relaxed) + 1;
    let frame = GatewayFrame::Event(EventFrame {
        event: event.to_string(),
        payload: Some(payload),
        seq: Some(seq),
        state_version,
    });
    match serde_json::to_string(&frame) {
        Ok(json) => {
            let _ = openclaw_send_text(tx, queued_bytes, closing, json, drop_if_slow);
        }
        Err(e) => warn!(error = %e, "Failed to serialize OpenClaw event frame"),
    }
}

fn next_run_seq(run_seq: &mut HashMap<String, u64>, run_id: &str) -> u64 {
    let next = run_seq.get(run_id).copied().unwrap_or(0) + 1;
    run_seq.insert(run_id.to_string(), next);
    next
}

fn build_self_presence(now: u64, conn_id: &str) -> serde_json::Value {
    // Mirror OpenClaw's "self" presence concept so the Control UI always has at least one entry.
    let host = std::env::var("HOSTNAME")
        .ok()
        .unwrap_or_else(|| "drbot".to_string());
    let text = format!(
        "Gateway: {} · app {} · mode gateway · reason self",
        host,
        env!("CARGO_PKG_VERSION")
    );
    json!({
        "host": host,
        "version": env!("CARGO_PKG_VERSION"),
        "platform": std::env::consts::OS,
        "mode": "gateway",
        "reason": "self",
        "instanceId": conn_id,
        "text": text,
        "ts": now,
    })
}

fn openclaw_client_presence(client: &OpenclawClient, now: u64) -> serde_json::Value {
    let host = client
        .display_name
        .as_deref()
        .unwrap_or_else(|| client.client_id.as_str());
    let ip = client.peer.ip().to_string();
    let text = format!(
        "Client: {} ({}) · mode {} · role {}",
        host,
        ip,
        client.client_mode.as_str(),
        client.role.as_str()
    );
    // Keep optional fields omitted (instead of null) to stay closer to OpenClaw schemas.
    let mut obj = serde_json::Map::new();
    obj.insert("host".to_string(), json!(host));
    obj.insert("ip".to_string(), json!(ip));
    obj.insert("version".to_string(), json!(&client.client_version));
    obj.insert("platform".to_string(), json!(&client.platform));
    obj.insert("mode".to_string(), json!(&client.client_mode));
    obj.insert("reason".to_string(), json!("connect"));
    obj.insert("roles".to_string(), json!([&client.role]));
    obj.insert("scopes".to_string(), json!(&client.scopes));
    obj.insert(
        "instanceId".to_string(),
        json!(client.instance_id.as_deref().unwrap_or(&client.conn_id)),
    );
    if let Some(device_id) = client.device_id.as_deref() {
        obj.insert("deviceId".to_string(), json!(device_id));
    }
    obj.insert("text".to_string(), json!(text));
    obj.insert("ts".to_string(), json!(now));
    serde_json::Value::Object(obj)
}

async fn list_system_presence(state: &GatewayState, conn_id: &str) -> Vec<serde_json::Value> {
    let now = now_ms();
    let mut out = Vec::new();
    out.push(build_self_presence(now, conn_id));

    // Extra system presence entries from `system-event` updates (e.g. nodes).
    // These are ephemeral and TTL-pruned (OpenClaw parity).
    for entry in state.openclaw_list_system_presence().await {
        if let Ok(v) = serde_json::to_value(entry) {
            out.push(v);
        }
    }

    let mut clients = state.list_openclaw_clients().await;
    // Deterministic ordering helps UI diffs.
    clients.sort_by(|a, b| a.conn_id.cmp(&b.conn_id));
    for c in clients {
        out.push(openclaw_client_presence(&c, now));
    }

    out
}

fn openclaw_event_required_scopes(event: &str) -> Option<&'static [&'static str]> {
    match event {
        "exec.approval.requested" | "exec.approval.resolved" => Some(&["operator.approvals"]),
        "device.pair.requested"
        | "device.pair.resolved"
        | "node.pair.requested"
        | "node.pair.resolved" => Some(&["operator.pairing"]),
        _ => None,
    }
}

fn openclaw_client_has_event_scope(client: &OpenclawClient, event: &str) -> bool {
    let Some(required) = openclaw_event_required_scopes(event) else {
        return true;
    };
    if client.role != "operator" {
        return false;
    }
    if client
        .scopes
        .iter()
        .any(|s| s == "operator.admin" || s == "global")
    {
        return true;
    }
    required
        .iter()
        .any(|scope| client.scopes.iter().any(|s| s == *scope))
}

pub(crate) async fn broadcast_openclaw_event(
    state: &GatewayState,
    event: &str,
    payload: serde_json::Value,
    state_version: Option<StateVersion>,
) {
    broadcast_openclaw_event_opts(state, event, payload, state_version, false).await;
}

pub(crate) async fn broadcast_openclaw_event_opts(
    state: &GatewayState,
    event: &str,
    payload: serde_json::Value,
    state_version: Option<StateVersion>,
    drop_if_slow: bool,
) {
    for client in state.list_openclaw_clients().await {
        if !openclaw_client_has_event_scope(&client, event) {
            continue;
        }
        send_event(
            &client.tx,
            &client.queued_bytes,
            &client.closing,
            client.event_seq.as_ref(),
            event,
            payload.clone(),
            state_version.clone(),
            drop_if_slow,
        )
        .await;
    }
}

async fn broadcast_presence(state: &GatewayState, presence_version: Option<u64>) {
    let version = presence_version.unwrap_or_else(|| state.increment_openclaw_presence_version());
    let payload = json!({
        "presence": list_system_presence(state, "gateway").await,
    });
    broadcast_openclaw_event_opts(
        state,
        "presence",
        payload,
        Some(StateVersion {
            presence: version,
            health: state.openclaw_health_version(),
        }),
        true,
    )
    .await;
}

// ---------------------------------------------------------------------------
// Node command allowlist (OpenClaw parity)
// ---------------------------------------------------------------------------

const NODE_CANVAS_COMMANDS: &[&str] = &[
    "canvas.present",
    "canvas.hide",
    "canvas.navigate",
    "canvas.eval",
    "canvas.snapshot",
    "canvas.a2ui.push",
    "canvas.a2ui.pushJSONL",
    "canvas.a2ui.reset",
];

const NODE_CAMERA_COMMANDS: &[&str] = &["camera.list", "camera.snap", "camera.clip"];
const NODE_SCREEN_COMMANDS: &[&str] = &["screen.record"];
const NODE_LOCATION_COMMANDS: &[&str] = &["location.get"];
const NODE_SMS_COMMANDS: &[&str] = &["sms.send"];

const NODE_SYSTEM_COMMANDS: &[&str] = &[
    "system.run",
    "system.which",
    "system.notify",
    "system.execApprovals.get",
    "system.execApprovals.set",
    "browser.proxy",
];

fn normalize_node_platform_id(platform: &str, device_family: Option<&str>) -> &'static str {
    let raw = platform.trim().to_lowercase();
    if raw.starts_with("ios") {
        return "ios";
    }
    if raw.starts_with("android") {
        return "android";
    }
    if raw.starts_with("mac") || raw.starts_with("darwin") {
        return "macos";
    }
    if raw.starts_with("win") {
        return "windows";
    }
    if raw.starts_with("linux") {
        return "linux";
    }

    let family = device_family.unwrap_or("").trim().to_lowercase();
    if family.contains("iphone") || family.contains("ipad") || family.contains("ios") {
        return "ios";
    }
    if family.contains("android") {
        return "android";
    }
    if family.contains("mac") {
        return "macos";
    }
    if family.contains("windows") {
        return "windows";
    }
    if family.contains("linux") {
        return "linux";
    }
    "unknown"
}

fn parse_env_command_list(var: &str) -> Vec<String> {
    let raw = std::env::var(var).unwrap_or_default();
    raw.split(|c: char| c == ',' || c == '\n' || c == '\r' || c == '\t' || c == ' ' || c == ';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn resolve_node_command_allowlist(platform: &str, device_family: Option<&str>) -> HashSet<String> {
    let platform_id = normalize_node_platform_id(platform, device_family);

    let mut allow: HashSet<String> = HashSet::new();
    let mut insert_all = |items: &[&str]| {
        for cmd in items {
            allow.insert(cmd.to_string());
        }
    };

    match platform_id {
        "ios" => {
            insert_all(NODE_CANVAS_COMMANDS);
            insert_all(NODE_CAMERA_COMMANDS);
            insert_all(NODE_SCREEN_COMMANDS);
            insert_all(NODE_LOCATION_COMMANDS);
        }
        "android" => {
            insert_all(NODE_CANVAS_COMMANDS);
            insert_all(NODE_CAMERA_COMMANDS);
            insert_all(NODE_SCREEN_COMMANDS);
            insert_all(NODE_LOCATION_COMMANDS);
            insert_all(NODE_SMS_COMMANDS);
        }
        "macos" => {
            insert_all(NODE_CANVAS_COMMANDS);
            insert_all(NODE_CAMERA_COMMANDS);
            insert_all(NODE_SCREEN_COMMANDS);
            insert_all(NODE_LOCATION_COMMANDS);
            insert_all(NODE_SYSTEM_COMMANDS);
        }
        "linux" | "windows" => {
            insert_all(NODE_SYSTEM_COMMANDS);
        }
        _ => {
            insert_all(NODE_CANVAS_COMMANDS);
            insert_all(NODE_CAMERA_COMMANDS);
            insert_all(NODE_SCREEN_COMMANDS);
            insert_all(NODE_LOCATION_COMMANDS);
            insert_all(NODE_SMS_COMMANDS);
            insert_all(NODE_SYSTEM_COMMANDS);
        }
    }

    // Env overrides (drbot-only): match OpenClaw's `gateway.nodes.allowCommands/denyCommands`.
    for cmd in parse_env_command_list("DRBOT_OPENCLAW_NODE_ALLOW_COMMANDS") {
        allow.insert(cmd);
    }
    for cmd in parse_env_command_list("DRBOT_OPENCLAW_NODE_DENY_COMMANDS") {
        allow.remove(cmd.trim());
    }

    allow
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodeInvokeError {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Debug)]
struct NodeInvokeResult {
    ok: bool,
    payload: Option<serde_json::Value>,
    payload_json: Option<String>,
    error: Option<NodeInvokeError>,
}

#[derive(Debug)]
struct PendingNodeInvoke {
    node_id: String,
    command: String,
    tx: oneshot::Sender<NodeInvokeResult>,
}

static OPENCLAW_NODE_INVOKES: OnceLock<Mutex<HashMap<String, PendingNodeInvoke>>> = OnceLock::new();

fn openclaw_node_invokes() -> &'static Mutex<HashMap<String, PendingNodeInvoke>> {
    OPENCLAW_NODE_INVOKES.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn register_node_invoke(
    id: String,
    node_id: String,
    command: String,
) -> oneshot::Receiver<NodeInvokeResult> {
    let (tx, rx) = oneshot::channel();
    openclaw_node_invokes().lock().await.insert(
        id,
        PendingNodeInvoke {
            node_id,
            command,
            tx,
        },
    );
    rx
}

async fn resolve_node_invoke(id: &str, node_id: &str, result: NodeInvokeResult) -> bool {
    let pending = openclaw_node_invokes().lock().await.remove(id);
    let Some(pending) = pending else {
        return false;
    };
    if pending.node_id != node_id {
        // Put it back so the correct node can resolve it.
        openclaw_node_invokes().lock().await.insert(
            id.to_string(),
            PendingNodeInvoke {
                node_id: pending.node_id,
                command: pending.command,
                tx: pending.tx,
            },
        );
        return false;
    }
    let _ = pending.tx.send(result);
    true
}

async fn cancel_node_invokes_for_node(node_id: &str, reason: &str) {
    let mut invokes = openclaw_node_invokes().lock().await;
    let keys: Vec<String> = invokes
        .iter()
        .filter_map(|(id, p)| {
            if p.node_id == node_id {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect();
    for id in keys {
        if let Some(p) = invokes.remove(&id) {
            let _ = p.tx.send(NodeInvokeResult {
                ok: false,
                payload: None,
                payload_json: None,
                error: Some(NodeInvokeError {
                    code: Some("NOT_CONNECTED".to_string()),
                    message: Some(reason.to_string()),
                }),
            });
        }
    }
}

async fn find_node_client(state: &GatewayState, node_id: &str) -> Option<OpenclawClient> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return None;
    }
    state.list_openclaw_clients().await.into_iter().find(|c| {
        if c.role != "node" {
            return false;
        }
        let id = c
            .device_id
            .clone()
            .or(c.instance_id.clone())
            .unwrap_or_else(|| c.conn_id.clone());
        id == node_id
    })
}

async fn invoke_node_command(
    state: &GatewayState,
    node_id: &str,
    command: &str,
    params: serde_json::Value,
    timeout_ms: u64,
) -> Result<serde_json::Value, ErrorShape> {
    let Some(node_client) = find_node_client(state, node_id).await else {
        return Err(
            ErrorShape::new(error_codes::UNAVAILABLE, "node not connected")
                .with_details(json!({ "code": "NOT_CONNECTED" })),
        );
    };

    let allowlist =
        resolve_node_command_allowlist(&node_client.platform, node_client.device_family.as_deref());
    let allowed_reason = if !allowlist.contains(command) {
        Some("command not allowlisted")
    } else if node_client.commands.is_empty() {
        Some("node did not declare commands")
    } else if !node_client.commands.iter().any(|c| c == command) {
        Some("command not declared by node")
    } else {
        None
    };
    if let Some(reason) = allowed_reason {
        return Err(
            ErrorShape::new(error_codes::INVALID_REQUEST, "node command not allowed").with_details(
                json!({
                    "reason": reason,
                    "command": command,
                }),
            ),
        );
    }

    let invoke_id = Uuid::new_v4().to_string();
    let idempotency_key = Uuid::new_v4().to_string();
    let params_json = params.to_string();
    let payload = json!({
        "id": invoke_id,
        "nodeId": node_id,
        "command": command,
        "paramsJSON": params_json,
        "timeoutMs": timeout_ms,
        "idempotencyKey": idempotency_key,
    });

    let rx =
        register_node_invoke(invoke_id.clone(), node_id.to_string(), command.to_string()).await;
    send_event(
        &node_client.tx,
        &node_client.queued_bytes,
        &node_client.closing,
        node_client.event_seq.as_ref(),
        "node.invoke.request",
        payload,
        None,
        false,
    )
    .await;

    let wait = tokio::time::timeout(Duration::from_millis(timeout_ms.max(1)), rx).await;
    let result = match wait {
        Ok(Ok(r)) => r,
        Ok(Err(_)) => NodeInvokeResult {
            ok: false,
            payload: None,
            payload_json: None,
            error: Some(NodeInvokeError {
                code: Some("UNAVAILABLE".to_string()),
                message: Some("node invoke dropped".to_string()),
            }),
        },
        Err(_) => {
            openclaw_node_invokes().lock().await.remove(&invoke_id);
            NodeInvokeResult {
                ok: false,
                payload: None,
                payload_json: None,
                error: Some(NodeInvokeError {
                    code: Some("TIMEOUT".to_string()),
                    message: Some("node invoke timed out".to_string()),
                }),
            }
        }
    };

    if !result.ok {
        return Err(ErrorShape::new(
            error_codes::UNAVAILABLE,
            result
                .error
                .as_ref()
                .and_then(|e| e.message.as_deref())
                .unwrap_or("node invoke failed"),
        )
        .with_details(json!({ "nodeError": result.error })));
    }

    let payload_value = if let Some(s) = &result.payload_json {
        serde_json::from_str::<serde_json::Value>(s)
            .ok()
            .or_else(|| result.payload.clone())
            .unwrap_or(serde_json::Value::Null)
    } else {
        result.payload.clone().unwrap_or(serde_json::Value::Null)
    };

    Ok(payload_value)
}

// ---------------------------------------------------------------------------
// Remote skills (OpenClaw parity)
// ---------------------------------------------------------------------------

fn is_mac_platform(platform: &str, device_family: Option<&str>) -> bool {
    let platform_norm = platform.trim().to_lowercase();
    if platform_norm.contains("mac") || platform_norm.contains("darwin") {
        return true;
    }
    let family_norm = device_family.unwrap_or("").trim().to_lowercase();
    family_norm == "mac"
}

fn supports_node_command(commands: &[String], target: &str) -> bool {
    commands.iter().any(|cmd| cmd == target)
}

fn build_bin_probe_script(bins: &[String]) -> String {
    let mut escaped: Vec<String> = Vec::new();
    for bin in bins {
        // Escape single quotes for sh -lc.
        let cleaned = bin.trim().replace('\'', "'\\''");
        if cleaned.is_empty() {
            continue;
        }
        escaped.push(format!("'{}'", cleaned));
    }
    if escaped.is_empty() {
        return String::new();
    }
    format!(
        "for b in {}; do if command -v \"$b\" >/dev/null 2>&1; then echo \"$b\"; fi; done",
        escaped.join(" ")
    )
}

fn parse_bin_probe_payload(payload: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    if let Some(arr) = payload.get("bins").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(s) = item.as_str() {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
            }
        }
    } else if let Some(stdout) = payload.get("stdout").and_then(|v| v.as_str()) {
        for line in stdout.split('\n') {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
    } else if let Some(arr) = payload.as_array() {
        for item in arr {
            if let Some(s) = item.as_str() {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
            }
        }
    }

    out.sort();
    out.dedup();
    out
}

const DEFAULT_REMOTE_BIN_PROBE_MIN_INTERVAL_MS: u64 = 5 * 60_000;

fn resolve_openclaw_remote_bin_probe_min_interval_ms() -> u64 {
    static CACHED: OnceLock<u64> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::env::var("DRBOT_OPENCLAW_REMOTE_BIN_PROBE_MIN_INTERVAL_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEFAULT_REMOTE_BIN_PROBE_MIN_INTERVAL_MS)
    })
}

#[derive(Debug, Default)]
struct RemoteBinProbeState {
    inflight: HashSet<String>,
    last_probe_at_ms: HashMap<String, u64>,
}

static REMOTE_BIN_PROBE_STATE: OnceLock<StdMutex<RemoteBinProbeState>> = OnceLock::new();

fn remote_bin_probe_state() -> &'static StdMutex<RemoteBinProbeState> {
    REMOTE_BIN_PROBE_STATE.get_or_init(|| StdMutex::new(RemoteBinProbeState::default()))
}

struct RemoteBinProbeGuard {
    node_id: String,
}

impl Drop for RemoteBinProbeGuard {
    fn drop(&mut self) {
        let mut st = remote_bin_probe_state()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        st.inflight.remove(&self.node_id);
    }
}

fn try_begin_remote_bin_probe(node_id: &str, force: bool) -> Option<RemoteBinProbeGuard> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return None;
    }

    let now = now_ms();
    let min_interval_ms = resolve_openclaw_remote_bin_probe_min_interval_ms();

    let mut st = remote_bin_probe_state()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if st.inflight.contains(node_id) {
        return None;
    }
    if !force {
        if let Some(last) = st.last_probe_at_ms.get(node_id).copied() {
            if now.saturating_sub(last) < min_interval_ms {
                return None;
            }
        }
    }

    st.inflight.insert(node_id.to_string());
    // Update last-seen immediately to prevent rapid retries if the probe fails quickly.
    st.last_probe_at_ms.insert(node_id.to_string(), now);
    Some(RemoteBinProbeGuard {
        node_id: node_id.to_string(),
    })
}

pub(crate) async fn refresh_remote_bins_for_connected_nodes_best_effort(
    state: GatewayState,
    force: bool,
) {
    let nodes: Vec<String> = state
        .list_openclaw_clients()
        .await
        .into_iter()
        .filter(|c| c.role == "node")
        .map(|c| {
            c.device_id
                .clone()
                .or(c.instance_id.clone())
                .unwrap_or_else(|| c.conn_id.clone())
        })
        .collect();

    for node_id in nodes {
        let st = state.clone();
        tokio::spawn(async move {
            refresh_remote_node_bins_best_effort(st, node_id, force).await;
        });
    }
}

async fn refresh_remote_node_bins_best_effort(state: GatewayState, node_id: String, force: bool) {
    let Some(_guard) = try_begin_remote_bin_probe(&node_id, force) else {
        return;
    };
    let Some(node_client) = find_node_client(&state, &node_id).await else {
        return;
    };
    if !is_mac_platform(&node_client.platform, node_client.device_family.as_deref()) {
        return;
    }

    let can_which = supports_node_command(&node_client.commands, "system.which");
    let can_run = supports_node_command(&node_client.commands, "system.run");
    if !can_which && !can_run {
        return;
    }

    let workspace_dirs = vec![resolve_agent_workspace_dir("default")];
    let required_bins = crate::openclaw_skills::collect_required_skill_bins_for_platform(
        &workspace_dirs,
        state.config(),
        "darwin",
    );
    if required_bins.is_empty() {
        return;
    }

    let timeout_ms = 15_000;
    let res = if can_which {
        invoke_node_command(
            &state,
            &node_id,
            "system.which",
            json!({ "bins": required_bins }),
            timeout_ms,
        )
        .await
    } else {
        let script = build_bin_probe_script(&required_bins);
        if script.trim().is_empty() {
            return;
        }
        invoke_node_command(
            &state,
            &node_id,
            "system.run",
            json!({ "command": ["/bin/sh", "-lc", script] }),
            timeout_ms,
        )
        .await
    };

    let payload = match res {
        Ok(p) => p,
        Err(err) => {
            let msg = err.message.to_lowercase();
            if msg.contains("node not connected") || msg.contains("not connected") {
                info!(node_id = %node_id, "remote bin probe skipped: node unavailable");
            } else if msg.contains("timed out") || msg.contains("timeout") {
                warn!(node_id = %node_id, "remote bin probe timed out");
            } else {
                warn!(node_id = %node_id, error = %err.message, "remote bin probe failed");
            }
            return;
        }
    };

    let bins = parse_bin_probe_payload(&payload);
    match update_paired_node_bins(&state, &node_id, bins) {
        Ok(true) => {
            // No-op: OpenClaw bumps a skills snapshot version here; drbot recomputes on demand.
        }
        Ok(false) => {}
        Err(e) => warn!(node_id = %node_id, error = %e.message, "failed to persist node bins"),
    }
}

fn normalize_openclaw_platform_id(platform: &str, device_family: Option<&str>) -> String {
    let raw = platform.trim().to_lowercase();
    if raw.contains("mac") || raw.contains("darwin") {
        return "darwin".to_string();
    }
    if raw.contains("win") {
        return "win32".to_string();
    }
    if raw.contains("linux") {
        return "linux".to_string();
    }
    let family = device_family.unwrap_or("").trim().to_lowercase();
    if family == "mac" {
        return "darwin".to_string();
    }
    if family == "windows" {
        return "win32".to_string();
    }
    if family == "linux" {
        return "linux".to_string();
    }
    raw
}

fn resolve_gateway_platform_id() -> String {
    match std::env::consts::OS {
        "macos" => "darwin".to_string(),
        "windows" => "win32".to_string(),
        other => other.to_string(),
    }
}

fn summarize_install_output(text: &str) -> Option<String> {
    let raw = text.trim();
    if raw.is_empty() {
        return None;
    }
    let lines = raw
        .split('\n')
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }

    let preferred = lines
        .iter()
        .copied()
        .find(|line| line.to_ascii_lowercase().starts_with("error"))
        .or_else(|| {
            lines.iter().copied().find(|line| {
                let lc = line.to_ascii_lowercase();
                lc.contains("err!") || lc.contains("error:") || lc.contains("failed")
            })
        })
        .or_else(|| lines.last().copied());
    let preferred = preferred?;
    let normalized = preferred.split_whitespace().collect::<Vec<_>>().join(" ");
    let max_len = 200usize;
    if normalized.chars().count() > max_len {
        Some(normalized.chars().take(max_len - 3).collect::<String>() + "...")
    } else {
        Some(normalized)
    }
}

fn format_install_failure_message(code: Option<i64>, stdout: &str, stderr: &str) -> String {
    let code_str = code
        .map(|c| format!("exit {}", c))
        .unwrap_or_else(|| "unknown exit".to_string());
    let summary = summarize_install_output(stderr).or_else(|| summarize_install_output(stdout));
    match summary {
        Some(s) => format!("Install failed ({}): {}", code_str, s),
        None => format!("Install failed ({})", code_str),
    }
}

fn parse_node_run_output(payload: &serde_json::Value) -> (Option<i64>, bool, String, String) {
    let mut code = payload
        .get("code")
        .and_then(|v| v.as_i64())
        .or_else(|| payload.get("exitCode").and_then(|v| v.as_i64()))
        .or_else(|| payload.get("exit_code").and_then(|v| v.as_i64()));
    let mut stdout = payload
        .get("stdout")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut stderr = payload
        .get("stderr")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // OpenClaw node hosts return `{ exitCode, success, timedOut, stdout, stderr, error }`.
    let success = payload.get("success").and_then(|v| v.as_bool());
    let timed_out = payload.get("timedOut").and_then(|v| v.as_bool()).unwrap_or(false);
    let error_text = payload.get("error").and_then(|v| v.as_str()).unwrap_or("").trim();
    if !error_text.is_empty() {
        if !stderr.trim().is_empty() {
            stderr.push('\n');
        }
        stderr.push_str(error_text);
    }
    if timed_out && stderr.trim().is_empty() {
        stderr = "timed out".to_string();
    }

    let ok = match success {
        Some(b) => b,
        None => code == Some(0),
    };
    if ok && code.is_none() {
        code = Some(0);
    }

    // Keep outputs small-ish; nodes may return very large buffers.
    let cap = 200_000usize;
    if stdout.len() > cap {
        stdout.truncate(cap);
    }
    if stderr.len() > cap {
        stderr.truncate(cap);
    }
    (code, ok, stdout, stderr)
}

fn resolve_node_manager() -> &'static str {
    match std::env::var("DRBOT_OPENCLAW_SKILLS_INSTALL_NODE_MANAGER")
        .ok()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "pnpm" => "pnpm",
        "yarn" => "yarn",
        "bun" => "bun",
        _ => "npm",
    }
}

fn build_node_install_argv(package: &str) -> Vec<String> {
    match resolve_node_manager() {
        "pnpm" => vec![
            "pnpm".to_string(),
            "add".to_string(),
            "-g".to_string(),
            package.to_string(),
        ],
        "yarn" => vec![
            "yarn".to_string(),
            "global".to_string(),
            "add".to_string(),
            package.to_string(),
        ],
        "bun" => vec![
            "bun".to_string(),
            "add".to_string(),
            "-g".to_string(),
            package.to_string(),
        ],
        _ => vec![
            "npm".to_string(),
            "install".to_string(),
            "-g".to_string(),
            package.to_string(),
        ],
    }
}

fn shell_escape_one(input: &str) -> String {
    let cleaned = input.trim().replace('\'', "'\\''");
    format!("'{}'", cleaned)
}

fn build_brew_resolver_sh() -> &'static str {
    r#"BREW="$(command -v brew 2>/dev/null || true)"
if [ -z "$BREW" ]; then
  if [ -x /opt/homebrew/bin/brew ]; then BREW="/opt/homebrew/bin/brew"; fi
fi
if [ -z "$BREW" ]; then
  if [ -x /usr/local/bin/brew ]; then BREW="/usr/local/bin/brew"; fi
fi
if [ -z "$BREW" ]; then
  echo "brew not installed" >&2
  exit 1
fi
"#
}

async fn run_skill_install_on_node(
    state: &GatewayState,
    node_id: &str,
    plan: &crate::openclaw_skills::SkillInstallPlan,
    timeout_ms: u64,
) -> crate::openclaw_skills::SkillInstallResult {
    let kind = plan.kind.as_str();

    let script = match kind {
        "brew" => {
            let Some(formula) = plan.formula.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
                return crate::openclaw_skills::SkillInstallResult {
                    ok: false,
                    message: "missing brew formula".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    code: None,
                };
            };
            format!(
                "{}\"$BREW\" install {}",
                build_brew_resolver_sh(),
                shell_escape_one(formula)
            )
        }
        "node" => {
            let Some(package) = plan.package.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
                return crate::openclaw_skills::SkillInstallResult {
                    ok: false,
                    message: "missing node package".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    code: None,
                };
            };
            let argv = build_node_install_argv(package);
            // Use direct argv so package managers see signals/exit codes correctly.
            let res = invoke_node_command(
                state,
                node_id,
                "system.run",
                json!({ "command": argv }),
                timeout_ms,
            )
            .await;
            return match res {
                Ok(payload) => {
                    let (code, ok, stdout, stderr) = parse_node_run_output(&payload);
                    crate::openclaw_skills::SkillInstallResult {
                        ok,
                        message: if ok {
                            "Installed".to_string()
                        } else {
                            format_install_failure_message(code, &stdout, &stderr)
                        },
                        stdout: stdout.trim().to_string(),
                        stderr: stderr.trim().to_string(),
                        code,
                    }
                }
                Err(err) => crate::openclaw_skills::SkillInstallResult {
                    ok: false,
                    message: err.message.clone(),
                    stdout: String::new(),
                    stderr: err.message,
                    code: None,
                },
            };
        }
        "go" => {
            let Some(module) = plan.module.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
                return crate::openclaw_skills::SkillInstallResult {
                    ok: false,
                    message: "missing go module".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    code: None,
                };
            };
            format!(
                r#"{brew}
if ! command -v go >/dev/null 2>&1; then
  "$BREW" install go
fi
GOBIN="$("$BREW" --prefix 2>/dev/null | tr -d '\n')/bin"
if [ -n "$GOBIN" ] && [ "$GOBIN" != "/bin" ]; then export GOBIN; fi
go install {module}"#,
                brew = build_brew_resolver_sh(),
                module = shell_escape_one(module),
            )
        }
        "uv" => {
            let Some(package) = plan.package.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
                return crate::openclaw_skills::SkillInstallResult {
                    ok: false,
                    message: "missing uv package".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    code: None,
                };
            };
            format!(
                r#"{brew}
if ! command -v uv >/dev/null 2>&1; then
  "$BREW" install uv
fi
uv tool install {package}"#,
                brew = build_brew_resolver_sh(),
                package = shell_escape_one(package),
            )
        }
        "download" => {
            let Some(url) = plan.url.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
                return crate::openclaw_skills::SkillInstallResult {
                    ok: false,
                    message: "missing download url".to_string(),
                    stdout: String::new(),
                    stderr: String::new(),
                    code: None,
                };
            };
            let target_dir = plan
                .target_dir
                .as_deref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("~/.openclaw/tools/{}", plan.skill_key));
            let extract = plan.extract.unwrap_or(true);
            let archive = plan
                .archive
                .as_deref()
                .map(|s| s.trim().to_ascii_lowercase())
                .unwrap_or_default();
            let strip = plan.strip_components.unwrap_or(0);

            // Minimal download/extract helper for nodes (best-effort; assumes curl + tar/unzip).
            format!(
                r#"set -e
URL={url}
TARGET_DIR={target}
TARGET_DIR="${{TARGET_DIR/#\~/$HOME}}"
mkdir -p "$TARGET_DIR"
FILENAME="$(basename "$URL")"
ARCHIVE_PATH="$TARGET_DIR/$FILENAME"
if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$URL" -o "$ARCHIVE_PATH"
elif command -v wget >/dev/null 2>&1; then
  wget -O "$ARCHIVE_PATH" "$URL"
else
  echo "curl or wget required" >&2
  exit 1
fi
if [ "{extract_flag}" != "1" ]; then
  echo "Downloaded to $ARCHIVE_PATH"
  exit 0
fi
TYPE="{archive}"
if [ -z "$TYPE" ]; then
  case "$FILENAME" in
    *.tar.gz|*.tgz) TYPE="tar.gz" ;;
    *.tar.bz2|*.tbz2) TYPE="tar.bz2" ;;
    *.zip) TYPE="zip" ;;
    *) TYPE="" ;;
  esac
fi
if [ -z "$TYPE" ]; then
  echo "extract requested but archive type could not be detected" >&2
  exit 1
fi
if [ "$TYPE" = "zip" ]; then
  command -v unzip >/dev/null 2>&1 || (echo "unzip not found on PATH" >&2; exit 1)
  unzip -q "$ARCHIVE_PATH" -d "$TARGET_DIR"
else
  command -v tar >/dev/null 2>&1 || (echo "tar not found on PATH" >&2; exit 1)
  if [ "{strip}" != "0" ]; then
    tar xf "$ARCHIVE_PATH" -C "$TARGET_DIR" --strip-components "{strip}"
  else
    tar xf "$ARCHIVE_PATH" -C "$TARGET_DIR"
  fi
fi
echo "Downloaded and extracted to $TARGET_DIR""#,
                url = shell_escape_one(url),
                target = shell_escape_one(&target_dir),
                extract_flag = if extract { "1" } else { "0" },
                archive = archive,
                strip = strip,
            )
        }
        _ => {
            return crate::openclaw_skills::SkillInstallResult {
                ok: false,
                message: format!("unsupported installer kind: {}", kind),
                stdout: String::new(),
                stderr: String::new(),
                code: None,
            };
        }
    };

    let payload = match invoke_node_command(
        state,
        node_id,
        "system.run",
        json!({ "command": ["/bin/sh", "-lc", script] }),
        timeout_ms,
    )
    .await
    {
        Ok(p) => p,
        Err(err) => {
            return crate::openclaw_skills::SkillInstallResult {
                ok: false,
                message: err.message.clone(),
                stdout: String::new(),
                stderr: err.message,
                code: None,
            };
        }
    };

    let (code, ok, stdout, stderr) = parse_node_run_output(&payload);
    crate::openclaw_skills::SkillInstallResult {
        ok,
        message: if ok {
            "Installed".to_string()
        } else {
            format_install_failure_message(code, &stdout, &stderr)
        },
        stdout: stdout.trim().to_string(),
        stderr: stderr.trim().to_string(),
        code,
    }
}

async fn select_install_node_for_plan(
    state: &GatewayState,
    plan: &crate::openclaw_skills::SkillInstallPlan,
    requested_node: Option<&str>,
) -> Result<String, ErrorShape> {
    let desired = if plan.os.is_empty() {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "remote install requires installer os restrictions",
        ));
    } else {
        plan.os
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    };
    if desired.is_empty() {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "remote install requires installer os restrictions",
        ));
    }

    let nodes = state.list_openclaw_clients().await;
    let mut candidates: Vec<(String, OpenclawClient)> = Vec::new();
    for c in nodes {
        if c.role != "node" {
            continue;
        }
        if !supports_node_command(&c.commands, "system.run") {
            continue;
        }
        let id = c
            .device_id
            .clone()
            .or(c.instance_id.clone())
            .unwrap_or_else(|| c.conn_id.clone());
        let platform_id = normalize_openclaw_platform_id(&c.platform, c.device_family.as_deref());
        if !desired.iter().any(|os| os == &platform_id) {
            continue;
        }
        candidates.push((id, c));
    }

    if let Some(requested) = requested_node {
        let q = requested.trim();
        if q.is_empty() {
            return Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                "invalid nodeId",
            ));
        }
        if let Some((id, _)) = candidates.into_iter().find(|(id, c)| {
            id == q
                || c.display_name.as_deref() == Some(q)
                || c.peer.ip().to_string() == q
                || (q.len() >= 6 && id.starts_with(q))
        }) {
            return Ok(id);
        }
        return Err(ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("requested node not connected or not eligible: {}", q),
        ));
    }

    if candidates.len() == 1 {
        return Ok(candidates.remove(0).0);
    }
    if candidates.is_empty() {
        return Err(ErrorShape::new(
            error_codes::UNAVAILABLE,
            "no eligible nodes connected for remote install",
        ));
    }
    Err(ErrorShape::new(
        error_codes::INVALID_REQUEST,
        "multiple eligible nodes connected; specify nodeId",
    ))
}

pub(crate) async fn resolve_remote_skill_eligibility(
    state: &GatewayState,
) -> Option<crate::openclaw_skills::RemoteSkillEligibility> {
    let paired = load_node_pairing_state(state).1;
    let live_nodes = state.list_openclaw_clients().await;

    // Start with paired nodes and then layer in live metadata if available.
    #[derive(Default)]
    struct NodeMeta {
        platform: Option<String>,
        device_family: Option<String>,
        commands: Vec<String>,
        bins: Vec<String>,
    }

    let mut nodes: HashMap<String, NodeMeta> = HashMap::new();
    for (node_id, node) in paired {
        nodes.insert(
            node_id,
            NodeMeta {
                platform: node.platform,
                device_family: node.device_family,
                commands: node.commands.unwrap_or_default(),
                bins: node.bins.unwrap_or_default(),
            },
        );
    }
    for live in live_nodes {
        if live.role != "node" {
            continue;
        }
        let node_id = live
            .device_id
            .clone()
            .or(live.instance_id.clone())
            .unwrap_or_else(|| live.conn_id.clone());
        let entry = nodes.entry(node_id).or_default();
        entry.platform = Some(live.platform);
        entry.device_family = live.device_family;
        entry.commands = live.commands;
    }

    let mut remote = crate::openclaw_skills::RemoteSkillEligibility::default();
    for meta in nodes.into_values() {
        let platform = meta.platform.unwrap_or_default();
        let device_family = meta.device_family.as_deref();
        if !is_mac_platform(&platform, device_family) {
            continue;
        }
        if !supports_node_command(&meta.commands, "system.run") {
            continue;
        }
        remote.platforms.insert("darwin".to_string());
        for bin in meta.bins {
            let trimmed = bin.trim();
            if !trimmed.is_empty() {
                remote.bins.insert(trimmed.to_string());
            }
        }
    }

    if remote.platforms.is_empty() {
        None
    } else {
        Some(remote)
    }
}

// ---------------------------------------------------------------------------
// Agent job tracking (OpenClaw agent.wait support)
// ---------------------------------------------------------------------------

const AGENT_RUN_CACHE_TTL_MS: u64 = 10 * 60_000;

#[derive(Debug, Clone)]
struct AgentRunSnapshot {
    status: String, // "running" | "ok" | "error"
    started_at: u64,
    ended_at: Option<u64>,
    error: Option<String>,
    ts: u64,
}

#[derive(Debug)]
struct AgentRunEntry {
    tx: watch::Sender<AgentRunSnapshot>,
    snapshot: AgentRunSnapshot,
}

static OPENCLAW_AGENT_RUNS: OnceLock<Mutex<HashMap<String, AgentRunEntry>>> = OnceLock::new();

fn openclaw_agent_runs() -> &'static Mutex<HashMap<String, AgentRunEntry>> {
    OPENCLAW_AGENT_RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prune_agent_runs(map: &mut HashMap<String, AgentRunEntry>, now: u64) {
    map.retain(|_, entry| now.saturating_sub(entry.snapshot.ts) <= AGENT_RUN_CACHE_TTL_MS);
}

async fn register_agent_run(run_id: &str) -> (watch::Receiver<AgentRunSnapshot>, bool) {
    let now = now_ms();
    let mut runs = openclaw_agent_runs().lock().await;
    prune_agent_runs(&mut runs, now);
    if let Some(entry) = runs.get(run_id) {
        return (entry.tx.subscribe(), false);
    }

    let snapshot = AgentRunSnapshot {
        status: "running".to_string(),
        started_at: now,
        ended_at: None,
        error: None,
        ts: now,
    };
    let (tx, rx) = watch::channel(snapshot.clone());
    runs.insert(run_id.to_string(), AgentRunEntry { tx, snapshot });
    (rx, true)
}

async fn finish_agent_run(
    run_id: &str,
    status: &str,
    error: Option<String>,
) -> Option<AgentRunSnapshot> {
    let now = now_ms();
    let mut runs = openclaw_agent_runs().lock().await;
    prune_agent_runs(&mut runs, now);
    let entry = runs.get_mut(run_id)?;
    let mut next = entry.snapshot.clone();
    next.status = status.to_string();
    next.ended_at = Some(now);
    next.error = error;
    next.ts = now;
    entry.snapshot = next.clone();
    let _ = entry.tx.send(next.clone());
    Some(next)
}

async fn wait_for_agent_run(run_id: &str, timeout_ms: u64) -> Option<AgentRunSnapshot> {
    let (mut rx, current) = {
        let now = now_ms();
        let mut runs = openclaw_agent_runs().lock().await;
        prune_agent_runs(&mut runs, now);
        let entry = runs.get(run_id)?;
        (entry.tx.subscribe(), entry.snapshot.clone())
    };

    if current.status != "running" {
        return Some(current);
    }
    if timeout_ms == 0 {
        return None;
    }

    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let snap = rx.borrow().clone();
        if snap.status != "running" {
            return Some(snap);
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return None;
        }
        let remain = deadline.saturating_duration_since(now);
        if tokio::time::timeout(remain, rx.changed()).await.is_err() {
            return None;
        }
    }
}

fn drbot_message_to_openclaw(msg: &Message) -> serde_json::Value {
    let role = match msg.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    };

    let mut blocks: Vec<serde_json::Value> = Vec::new();
    for c in &msg.content {
        match c {
            Content::Text { text } => blocks.push(json!({ "type": "text", "text": text })),
            Content::Image { source, alt_text } => {
                // OpenClaw UI understands a "base64" image source with either raw base64 or a
                // full data URL string in `data`.
                match source {
                    ImageSource::Base64 { media_type, data } => blocks.push(json!({
                        "type": "image",
                        "source": { "type": "base64", "media_type": media_type, "data": data },
                        "alt": alt_text,
                    })),
                    ImageSource::Url { url } => blocks.push(json!({
                        "type": "image",
                        "url": url,
                        "alt": alt_text,
                    })),
                }
            }
            Content::File {
                name, mime_type, ..
            } => {
                blocks.push(json!({
                    "type": "text",
                    "text": format!("[file: {} ({})]", name, mime_type)
                }));
            }
            Content::Audio { .. } => blocks.push(json!({ "type": "text", "text": "[audio]" })),
            Content::ToolUse { id, name, input } => blocks.push(json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "arguments": input,
            })),
            Content::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => blocks.push(json!({
                "type": "tool_result",
                "toolCallId": tool_use_id,
                "name": "tool",
                "text": content,
                "isError": is_error,
            })),
        }
    }

    json!({
        "id": msg.id.to_string(),
        "role": role,
        "content": blocks,
        "timestamp": msg.created_at.timestamp_millis(),
    })
}

fn openclaw_user_message_to_drbot(message: &str, attachments: &[serde_json::Value]) -> Message {
    let mut content: Vec<Content> = Vec::new();
    let trimmed = message.trim();
    if !trimmed.is_empty() {
        content.push(Content::Text {
            text: trimmed.to_string(),
        });
    }

    // Best-effort: accept {type:"image", mimeType, content: base64}.
    for att in attachments {
        let t = att.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if t != "image" {
            continue;
        }
        let mime = att
            .get("mimeType")
            .and_then(|v| v.as_str())
            .unwrap_or("image/png");
        let data = att.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if data.is_empty() {
            continue;
        }
        content.push(Content::Image {
            source: ImageSource::Base64 {
                media_type: mime.to_string(),
                data: data.to_string(),
            },
            alt_text: None,
        });
    }

    Message {
        id: Uuid::new_v4(),
        role: Role::User,
        content,
        created_at: chrono::Utc::now(),
        metadata: serde_json::Map::new(),
    }
}

fn openclaw_timestamp_injection_enabled() -> bool {
    match std::env::var("DRBOT_OPENCLAW_TIMESTAMP_INJECT")
        .ok()
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .as_deref()
    {
        Some("0") | Some("false") | Some("off") => false,
        _ => true,
    }
}

fn message_contains_ymd_hm_pattern(message: &str) -> bool {
    let bytes = message.as_bytes();
    let need = 16usize;
    if bytes.len() < need {
        return false;
    }
    let is_d = |b: u8| b.is_ascii_digit();
    for i in 0..=bytes.len().saturating_sub(need) {
        let s = &bytes[i..i + need];
        if is_d(s[0])
            && is_d(s[1])
            && is_d(s[2])
            && is_d(s[3])
            && s[4] == b'-'
            && is_d(s[5])
            && is_d(s[6])
            && s[7] == b'-'
            && is_d(s[8])
            && is_d(s[9])
            && s[10] == b' '
            && is_d(s[11])
            && is_d(s[12])
            && s[13] == b':'
            && is_d(s[14])
            && is_d(s[15])
        {
            return true;
        }
    }
    false
}

fn openclaw_inject_timestamp_prefix(message: &str) -> String {
    if !openclaw_timestamp_injection_enabled() {
        return message.to_string();
    }
    if message.trim().is_empty() {
        return message.to_string();
    }
    // Already has a channel envelope or previously injected timestamp.
    if message.trim_start().starts_with('[') && message_contains_ymd_hm_pattern(message) {
        return message.to_string();
    }
    // Cron jobs inject "Current time: ..." into messages; avoid double-stamping.
    if message.contains("Current time: ") {
        return message.to_string();
    }

    let now = chrono::Utc::now();
    let dow = match now.weekday() {
        chrono::Weekday::Mon => "Mon",
        chrono::Weekday::Tue => "Tue",
        chrono::Weekday::Wed => "Wed",
        chrono::Weekday::Thu => "Thu",
        chrono::Weekday::Fri => "Fri",
        chrono::Weekday::Sat => "Sat",
        chrono::Weekday::Sun => "Sun",
    };
    let prefix = format!(
        "[{} {:04}-{:02}-{:02} {:02}:{:02} UTC] ",
        dow,
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
    );
    format!("{}{}", prefix, message)
}

fn stamp_user_message_for_agent(msg: &Message) -> Message {
    if !openclaw_timestamp_injection_enabled() {
        return msg.clone();
    }
    if msg.role != Role::User {
        return msg.clone();
    }
    let mut cloned = msg.clone();
    for content in cloned.content.iter_mut() {
        if let Content::Text { text } = content {
            if text.trim().is_empty() {
                continue;
            }
            *text = openclaw_inject_timestamp_prefix(text);
            break;
        }
    }
    cloned
}

fn is_chat_stop_command_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let normalized = trimmed.to_lowercase();
    if normalized == "/stop" {
        return true;
    }
    matches!(
        normalized.as_str(),
        "stop" | "esc" | "abort" | "wait" | "exit" | "interrupt"
    )
}

fn assistant_message_from_text(text: &str) -> Message {
    Message::assistant(text)
}

fn error_response(
    id: &str,
    code: &str,
    message: &str,
    details: Option<serde_json::Value>,
) -> GatewayFrame {
    let mut err = ErrorShape::new(code, message);
    if let Some(d) = details {
        err = err.with_details(d);
    }
    GatewayFrame::Res(ResponseFrame {
        id: id.to_string(),
        ok: false,
        payload: None,
        error: Some(err),
    })
}

fn ok_response(id: &str, payload: serde_json::Value) -> GatewayFrame {
    GatewayFrame::Res(ResponseFrame {
        id: id.to_string(),
        ok: true,
        payload: Some(payload),
        error: None,
    })
}

async fn build_channels_snapshot(now: u64, state: &GatewayState) -> serde_json::Value {
    // Align ordering with OpenClaw's channel registry, but keep drbot-only channels too.
    let channel_order = vec![
        "telegram",
        "whatsapp",
        "discord",
        "googlechat",
        "slack",
        "signal",
        "imessage",
        "matrix",
        "webchat",
        // Future/placeholder channels (not wired in drbot yet).
        "nostr",
    ];

    let config = state.config();
    let runtime = state.channel_manager().runtime_snapshot().await;
    let whatsapp_login = state.openclaw_web_login().snapshot_whatsapp();

    let mut channel_labels = serde_json::Map::new();
    let label_for = |key: &str| -> String {
        match key {
            "telegram" => "Telegram".to_string(),
            "whatsapp" => "WhatsApp".to_string(),
            "discord" => "Discord".to_string(),
            "googlechat" => "Google Chat".to_string(),
            "slack" => "Slack".to_string(),
            "signal" => "Signal".to_string(),
            "imessage" => "iMessage".to_string(),
            "matrix" => "Matrix".to_string(),
            "webchat" => "WebChat".to_string(),
            other => other.to_string(),
        }
    };
    for key in &channel_order {
        channel_labels.insert((*key).to_string(), json!(label_for(key)));
    }

    let is_enabled = |name: &str| config.channels.enabled.iter().any(|c| c == name);
    let is_configured = |name: &str| match name {
        "whatsapp" => config.channels.whatsapp.is_some(),
        "telegram" => config.channels.telegram.is_some(),
        "discord" => config.channels.discord.is_some(),
        "slack" => config.channels.slack.is_some(),
        "signal" => config.channels.signal.is_some(),
        "imessage" => config.channels.imessage.is_some(),
        "matrix" => config.channels.matrix.is_some(),
        "webchat" => config.channels.webchat.is_some(),
        _ => false,
    };

    let mut channels_obj = serde_json::Map::new();
    let mut channel_accounts = serde_json::Map::new();
    let mut channel_default_account_id = serde_json::Map::new();

    for key in &channel_order {
        let configured = runtime.get(*key).map(|r| r.configured).unwrap_or_else(|| is_configured(key));
        let enabled = runtime.get(*key).map(|r| r.enabled).unwrap_or_else(|| is_enabled(key));
        let linked = if *key == "whatsapp" {
            configured && whatsapp_login.connected
        } else {
            configured
        };

        let rt = runtime.get(*key);
        let running = rt.map(|r| r.running).unwrap_or(false);
        let connected = if *key == "whatsapp" {
            whatsapp_login.connected
        } else {
            rt.map(|r| r.connected).unwrap_or(false)
        };
        let reconnect_attempts = rt.map(|r| r.reconnect_attempts).unwrap_or(0);
        let last_connected_at = rt.and_then(|r| r.last_connected_at_ms);
        let last_error = rt.and_then(|r| r.last_error.clone());
        let last_start_at = rt.and_then(|r| r.last_start_at_ms);
        let last_stop_at = rt.and_then(|r| r.last_stop_at_ms);
        let last_inbound_at = rt.and_then(|r| r.last_inbound_at_ms);
        let last_outbound_at = rt.and_then(|r| r.last_outbound_at_ms);

        channels_obj.insert(
            (*key).to_string(),
            json!({
                "configured": configured,
                "enabled": enabled,
                "linked": linked,
                "running": running,
                "connected": connected,
                "reconnectAttempts": reconnect_attempts,
                "lastError": last_error,
                "lastInboundAt": last_inbound_at,
                "lastOutboundAt": last_outbound_at,
            }),
        );

        channel_accounts.insert(
            (*key).to_string(),
            json!([{
                "accountId": "default",
                "enabled": enabled,
                "configured": configured,
                "linked": linked,
                "running": running,
                "connected": connected,
                "reconnectAttempts": reconnect_attempts,
                "lastConnectedAt": last_connected_at,
                "lastError": last_error,
                "lastStartAt": last_start_at,
                "lastStopAt": last_stop_at,
                "lastInboundAt": last_inbound_at,
                "lastOutboundAt": last_outbound_at,
            }]),
        );
        channel_default_account_id.insert((*key).to_string(), json!("default"));
    }

    json!({
        "ts": now,
        "channelOrder": channel_order,
        "channelLabels": channel_labels,
        "channels": serde_json::Value::Object(channels_obj),
        "channelAccounts": channel_accounts,
        "channelDefaultAccountId": channel_default_account_id,
    })
}

fn build_config_schema() -> serde_json::Value {
    // A small JSON Schema that keeps the OpenClaw Control UI usable.
    // It doesn't need to match OpenClaw's config; it just needs to be valid schema.
    json!({
        "type": "object",
        "properties": {
            "gateway": {
                "type": "object",
                "properties": {
                    "host": { "type": "string" },
                    "port": { "type": "integer" },
                    "auth_token": { "type": ["string", "null"] },
                    "tls_enabled": { "type": "boolean" },
                    "tls_cert": { "type": ["string", "null"] },
                    "tls_key": { "type": ["string", "null"] }
                }
            },
            "providers": { "type": "object" },
            "channels": { "type": "object" },
            "storage": { "type": "object" },
            "logging": { "type": "object" }
        }
    })
}

async fn handle_config_get() -> serde_json::Value {
    let path = resolve_config_path_for_read();
    let exists = path.exists();

    let raw = if exists {
        std::fs::read_to_string(&path).ok()
    } else {
        // Provide a reasonable starting point for the UI even if no config exists yet.
        toml::to_string_pretty(&drbot_core::Config::default()).ok()
    };

    let raw_str = raw.clone().unwrap_or_default();
    let hash = sha256_hex(&raw_str);
    let (parsed, valid, issues) = match toml::from_str::<serde_json::Value>(&raw_str) {
        Ok(v) => (v, true, Vec::<String>::new()),
        Err(e) => (json!({}), false, vec![e.to_string()]),
    };

    json!({
        "path": path.to_string_lossy(),
        "exists": exists,
        "raw": raw,
        "hash": hash,
        "parsed": parsed,
        "valid": valid,
        "config": parsed,
        "issues": issues,
    })
}

async fn handle_config_schema() -> serde_json::Value {
    json!({
        "schema": build_config_schema(),
        "uiHints": {},
        "version": format!("drbot-{}-schema-v1", env!("CARGO_PKG_VERSION")),
        "generatedAt": chrono::Utc::now().to_rfc3339(),
    })
}

async fn handle_config_set(
    raw: &str,
    base_hash: Option<&str>,
) -> Result<serde_json::Value, ErrorShape> {
    let write_path = resolve_config_path_for_write();
    let current_path = resolve_config_path_for_read();
    let current_exists = current_path.exists();
    let current_raw = std::fs::read_to_string(&current_path).unwrap_or_default();
    let current_hash = sha256_hex(&current_raw);

    if current_exists {
        let expected_trimmed = base_hash.unwrap_or("").trim();
        if expected_trimmed.is_empty() {
            return Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                "config base hash required; re-run config.get and retry",
            ));
        }
        if expected_trimmed != current_hash {
            return Err(
                ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    "config changed since last load; re-run config.get and retry",
                )
                .with_details(json!({"expected": expected_trimmed, "actual": current_hash})),
            );
        }
    }

    // Validate TOML parses into drbot config before writing.
    let cfg: drbot_core::Config = toml::from_str(raw).map_err(|e| {
        ErrorShape::new(error_codes::INVALID_REQUEST, format!("invalid config: {}", e))
    })?;

    if let Some(parent) = write_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Err(ErrorShape::new(
                error_codes::UNAVAILABLE,
                format!("failed to create config dir: {}", e),
            ));
        }
    }

    if let Err(e) = std::fs::write(&write_path, raw) {
        return Err(ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("failed to write config: {}", e),
        ));
    }

    Ok(json!({
        "ok": true,
        "path": write_path.to_string_lossy(),
        "config": serde_json::to_value(&cfg).unwrap_or_else(|_| json!({})),
    }))
}

fn apply_merge_patch(target: &mut serde_json::Value, patch: &serde_json::Value) {
    use serde_json::Value;
    match patch {
        Value::Object(patch_obj) => {
            if !target.is_object() {
                *target = json!({});
            }
            let Some(target_obj) = target.as_object_mut() else {
                return;
            };
            for (k, v) in patch_obj {
                if v.is_null() {
                    target_obj.remove(k);
                    continue;
                }
                match target_obj.get_mut(k) {
                    Some(existing) => apply_merge_patch(existing, v),
                    None => {
                        target_obj.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        _ => {
            *target = patch.clone();
        }
    }
}

async fn handle_config_patch(
    raw: &str,
    base_hash: Option<&str>,
) -> Result<serde_json::Value, ErrorShape> {
    let write_path = resolve_config_path_for_write();
    let current_path = resolve_config_path_for_read();
    let current_exists = current_path.exists();
    let current_raw = std::fs::read_to_string(&current_path).unwrap_or_default();
    let current_hash = sha256_hex(&current_raw);

    if current_exists {
        let expected_trimmed = base_hash.unwrap_or("").trim();
        if expected_trimmed.is_empty() {
            return Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                "config base hash required; re-run config.get and retry",
            ));
        }
        if expected_trimmed != current_hash {
            return Err(
                ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    "config changed since last load; re-run config.get and retry",
                )
                .with_details(json!({"expected": expected_trimmed, "actual": current_hash})),
            );
        }
    }

    // Load the current config as the merge base. If no config exists yet, patch the defaults.
    let base_cfg: drbot_core::Config = if current_exists {
        toml::from_str(&current_raw).map_err(|_| {
            ErrorShape::new(
                error_codes::INVALID_REQUEST,
                "invalid config; fix before patching",
            )
        })?
    } else {
        drbot_core::Config::default()
    };

    let patch_value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
        ErrorShape::new(
            error_codes::INVALID_REQUEST,
            format!("invalid config.patch params: {}", e),
        )
    })?;
    if !patch_value.is_object() {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "config.patch raw must be an object",
        ));
    }

    let mut merged = serde_json::to_value(&base_cfg).unwrap_or_else(|_| json!({}));
    apply_merge_patch(&mut merged, &patch_value);
    let merged_cfg: drbot_core::Config = serde_json::from_value(merged).map_err(|e| {
        ErrorShape::new(error_codes::INVALID_REQUEST, format!("invalid config: {}", e))
    })?;
    let raw_out = toml::to_string_pretty(&merged_cfg).map_err(|e| {
        ErrorShape::new(error_codes::UNAVAILABLE, format!("failed to serialize config: {}", e))
    })?;

    if let Some(parent) = write_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ErrorShape::new(error_codes::UNAVAILABLE, format!("failed to create config dir: {}", e))
        })?;
    }
    std::fs::write(&write_path, raw_out).map_err(|e| {
        ErrorShape::new(error_codes::UNAVAILABLE, format!("failed to write config: {}", e))
    })?;

    Ok(json!({
        "ok": true,
        "path": write_path.to_string_lossy(),
        "config": serde_json::to_value(&merged_cfg).unwrap_or_else(|_| json!({})),
    }))
}

// ---------------------------------------------------------------------------
// Sessions sidecar (OpenClaw parity)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenclawSessionsSidecarFile {
    version: u32,
    #[serde(default)]
    entries: HashMap<String, serde_json::Value>,
}

impl Default for OpenclawSessionsSidecarFile {
    fn default() -> Self {
        Self {
            version: 1,
            entries: HashMap::new(),
        }
    }
}

fn resolve_openclaw_sessions_sidecar_path(state: &GatewayState, agent_id: &str) -> PathBuf {
    let agent_id = agent_id.trim();
    let agent_id = if agent_id.is_empty() { "default" } else { agent_id };
    if let Some(dir) = resolve_openclaw_state_dir(state) {
        return dir
            .join("agents")
            .join(agent_id)
            .join("sessions")
            .join("sessions.json");
    }
    PathBuf::from("openclaw_sessions.json")
}

fn load_openclaw_sessions_sidecar(state: &GatewayState, agent_id: &str) -> OpenclawSessionsSidecarFile {
    let path = resolve_openclaw_sessions_sidecar_path(state, agent_id);
    read_json_file::<OpenclawSessionsSidecarFile>(&path).unwrap_or_default()
}

fn save_openclaw_sessions_sidecar(
    state: &GatewayState,
    agent_id: &str,
    file: &OpenclawSessionsSidecarFile,
) -> Result<(), ErrorShape> {
    let path = resolve_openclaw_sessions_sidecar_path(state, agent_id);
    write_json_atomic(&path, file)
}

fn extract_sidecar_string(entry: Option<&serde_json::Value>, field: &str) -> Option<String> {
    entry?
        .get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn extract_sidecar_literal(entry: Option<&serde_json::Value>, field: &str) -> Option<serde_json::Value> {
    entry?.get(field).cloned().filter(|v| !v.is_null())
}

fn apply_sidecar_patch_string(
    entry: &mut serde_json::Map<String, serde_json::Value>,
    patch: &serde_json::Value,
    field: &str,
) {
    if !patch.get(field).is_some() {
        return;
    }
    match patch.get(field) {
        Some(serde_json::Value::Null) => {
            entry.remove(field);
        }
        Some(serde_json::Value::String(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                entry.remove(field);
            } else {
                entry.insert(field.to_string(), json!(trimmed));
            }
        }
        Some(other) => {
            // Keep schema-light: accept non-string values as-is for forward compatibility.
            entry.insert(field.to_string(), other.clone());
        }
        None => {}
    }
}

fn apply_sidecar_patch_literal(
    entry: &mut serde_json::Map<String, serde_json::Value>,
    patch: &serde_json::Value,
    field: &str,
) {
    if !patch.get(field).is_some() {
        return;
    }
    match patch.get(field) {
        Some(serde_json::Value::Null) => {
            entry.remove(field);
        }
        Some(other) => {
            entry.insert(field.to_string(), other.clone());
        }
        None => {}
    }
}

pub(crate) fn resolve_openclaw_session_model_override(
    state: &GatewayState,
    session_key: &str,
) -> Option<String> {
    let key = session_key.trim();
    if key.is_empty() {
        return None;
    }
    let file = load_openclaw_sessions_sidecar(state, "default");
    extract_sidecar_string(file.entries.get(key), "model")
}

async fn handle_sessions_list(state: &GatewayState) -> serde_json::Value {
    let now = now_ms();
    let sidecar_path = resolve_openclaw_sessions_sidecar_path(state, "default");
    let sidecar = load_openclaw_sessions_sidecar(state, "default");

    let mut sessions_out: Vec<serde_json::Value> = Vec::new();
    let mut count = 0usize;

    if let Some(store) = state.session_store() {
        let list = store
            .list(drbot_sessions::ListOptions::default())
            .await
            .unwrap_or_default();
        count = list.len();

        for s in list {
            // For interop, treat OpenClaw sessions as those stored under channel_type "openclaw".
            let key = if s.channel_type == "openclaw" {
                s.channel_id.clone()
            } else {
                format!("{}:{}", s.channel_type, s.channel_id)
            };
            let kind = if key == "main" { "global" } else { "unknown" };
            let sidecar_entry = sidecar.entries.get(&key);
            sessions_out.push(json!({
                "key": key,
                "kind": kind,
                "label": s.title,
                "updatedAt": s.updated_at.timestamp_millis(),
                "sessionId": s.id.to_string(),
                "thinkingLevel": extract_sidecar_string(sidecar_entry, "thinkingLevel"),
                "verboseLevel": extract_sidecar_string(sidecar_entry, "verboseLevel"),
                "reasoningLevel": extract_sidecar_string(sidecar_entry, "reasoningLevel"),
                "responseUsage": extract_sidecar_literal(sidecar_entry, "responseUsage"),
                "elevatedLevel": extract_sidecar_string(sidecar_entry, "elevatedLevel"),
                "execHost": extract_sidecar_string(sidecar_entry, "execHost"),
                "execSecurity": extract_sidecar_string(sidecar_entry, "execSecurity"),
                "execAsk": extract_sidecar_string(sidecar_entry, "execAsk"),
                "execNode": extract_sidecar_string(sidecar_entry, "execNode"),
                "sendPolicy": extract_sidecar_literal(sidecar_entry, "sendPolicy"),
                "groupActivation": extract_sidecar_literal(sidecar_entry, "groupActivation"),
                "spawnedBy": extract_sidecar_string(sidecar_entry, "spawnedBy"),
                "inputTokens": s.metadata.total_input_tokens,
                "outputTokens": s.metadata.total_output_tokens,
                "totalTokens": s.metadata.total_input_tokens + s.metadata.total_output_tokens,
                "model": extract_sidecar_string(sidecar_entry, "model").or(s.model),
            }));
        }
    }

    json!({
        "ts": now,
        "path": sidecar_path,
        "count": count,
        "defaults": {
            "model": state.config().providers.default_model,
            "contextTokens": null
        },
        "sessions": sessions_out,
    })
}

async fn handle_sessions_patch(
    state: &GatewayState,
    key: &str,
    patch: &serde_json::Value,
) -> Result<serde_json::Value, ErrorShape> {
    let store = state
        .session_store()
        .ok_or_else(|| ErrorShape::new(error_codes::UNAVAILABLE, "session store not configured"))?;

    // Stable operator user id (single-user gateway).
    let user_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"openclaw-operator");

    // Try "openclaw" first, then fall back to split keys. If missing, create.
    let mut session = store
        .get_by_channel("openclaw", key)
        .await
        .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
    if session.is_none() {
        let (channel_type, channel_id) = session_key_to_channel(key);
        session = store
            .get_by_channel(&channel_type, &channel_id)
            .await
            .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
        if session.is_none() {
            session = store
                .get_or_create(user_id, &channel_type, &channel_id)
                .await
                .map(Some)
                .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
        }
    }
    let mut session =
        session.ok_or_else(|| ErrorShape::new(error_codes::UNAVAILABLE, "session unavailable"))?;

    // Update label (stored in sqlite).
    let mut touched = false;
    if patch.get("label").is_some() {
        match patch.get("label") {
            Some(serde_json::Value::Null) => {
                if session.title.is_some() {
                    session.title = None;
                    touched = true;
                }
            }
            Some(serde_json::Value::String(s)) => {
                let trimmed = s.trim();
                let next = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.chars().take(200).collect::<String>())
                };
                if session.title != next {
                    session.title = next;
                    touched = true;
                }
            }
            _ => {}
        }
    }

    // Update sidecar metadata (OpenClaw parity).
    let mut sidecar = load_openclaw_sessions_sidecar(state, "default");
    let mut sidecar_entry = sidecar
        .entries
        .get(key)
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_else(serde_json::Map::new);

    for field in [
        "thinkingLevel",
        "verboseLevel",
        "reasoningLevel",
        "elevatedLevel",
        "execHost",
        "execSecurity",
        "execAsk",
        "execNode",
        "model",
        "spawnedBy",
    ] {
        if patch.get(field).is_some() {
            touched = true;
        }
        apply_sidecar_patch_string(&mut sidecar_entry, patch, field);
    }
    for field in ["responseUsage", "sendPolicy", "groupActivation"] {
        if patch.get(field).is_some() {
            touched = true;
        }
        apply_sidecar_patch_literal(&mut sidecar_entry, patch, field);
    }

    if sidecar_entry.is_empty() {
        sidecar.entries.remove(key);
    } else {
        sidecar
            .entries
            .insert(key.to_string(), serde_json::Value::Object(sidecar_entry));
    }
    if touched {
        session.update_timestamp();
        store
            .update(&session)
            .await
            .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
        // Keep this best-effort: session data is still in sqlite even if the sidecar fails.
        if let Err(e) = save_openclaw_sessions_sidecar(state, "default", &sidecar) {
            warn!(error = %e.message, "failed to persist OpenClaw sessions sidecar");
        }
    }

    let sidecar_entry = sidecar.entries.get(key);

    Ok(json!({
        "ok": true,
        "path": resolve_openclaw_sessions_sidecar_path(state, "default"),
        "key": key,
        "entry": {
            "sessionId": session.id.to_string(),
            "updatedAt": session.updated_at.timestamp_millis(),
            "label": session.title,
            "thinkingLevel": extract_sidecar_string(sidecar_entry, "thinkingLevel"),
            "verboseLevel": extract_sidecar_string(sidecar_entry, "verboseLevel"),
            "reasoningLevel": extract_sidecar_string(sidecar_entry, "reasoningLevel"),
            "responseUsage": extract_sidecar_literal(sidecar_entry, "responseUsage"),
            "elevatedLevel": extract_sidecar_string(sidecar_entry, "elevatedLevel"),
            "execHost": extract_sidecar_string(sidecar_entry, "execHost"),
            "execSecurity": extract_sidecar_string(sidecar_entry, "execSecurity"),
            "execAsk": extract_sidecar_string(sidecar_entry, "execAsk"),
            "execNode": extract_sidecar_string(sidecar_entry, "execNode"),
            "sendPolicy": extract_sidecar_literal(sidecar_entry, "sendPolicy"),
            "groupActivation": extract_sidecar_literal(sidecar_entry, "groupActivation"),
            "spawnedBy": extract_sidecar_string(sidecar_entry, "spawnedBy"),
            "model": extract_sidecar_string(sidecar_entry, "model").or(session.model),
        }
    }))
}

async fn handle_sessions_delete(
    state: &GatewayState,
    key: &str,
) -> Result<serde_json::Value, ErrorShape> {
    let store = state
        .session_store()
        .ok_or_else(|| ErrorShape::new(error_codes::UNAVAILABLE, "session store not configured"))?;

    // Try "openclaw" first, then fall back to split keys.
    let mut session = store
        .get_by_channel("openclaw", key)
        .await
        .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
    if session.is_none() {
        let (channel_type, channel_id) = session_key_to_channel(key);
        session = store
            .get_by_channel(&channel_type, &channel_id)
            .await
            .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
    }
    let session =
        session.ok_or_else(|| ErrorShape::new(error_codes::INVALID_REQUEST, "unknown session"))?;

    store
        .delete(session.id)
        .await
        .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;

    // Best-effort: keep sessions.json sidecar in sync with sqlite deletes.
    let mut sidecar = load_openclaw_sessions_sidecar(state, "default");
    if sidecar.entries.remove(key).is_some() {
        if let Err(e) = save_openclaw_sessions_sidecar(state, "default", &sidecar) {
            warn!(error = %e.message, "failed to update OpenClaw sessions sidecar");
        }
    }

    Ok(json!({ "ok": true }))
}

fn resolve_voicewake_path() -> PathBuf {
    // Match OpenClaw's convention: stateDir/settings/voicewake.json
    if let Some(dir) = resolve_data_dir() {
        return dir.join("settings").join("voicewake.json");
    }
    PathBuf::from("voicewake.json")
}

fn default_voicewake_triggers() -> Vec<String> {
    vec![
        "openclaw".to_string(),
        "claude".to_string(),
        "computer".to_string(),
    ]
}

fn normalize_voicewake_triggers(input: &[serde_json::Value]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for v in input.iter().take(32) {
        let s = v.as_str().unwrap_or("").trim();
        if s.is_empty() {
            continue;
        }
        let truncated: String = s.chars().take(64).collect();
        out.push(truncated);
    }
    if out.is_empty() {
        default_voicewake_triggers()
    } else {
        out
    }
}

async fn handle_voicewake_get() -> Result<serde_json::Value, ErrorShape> {
    let path = resolve_voicewake_path();
    if !path.exists() {
        return Ok(json!({ "triggers": default_voicewake_triggers() }));
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("failed to read voicewake config: {}", e),
        )
    })?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!({}));
    let triggers = parsed
        .get("triggers")
        .and_then(|v| v.as_array())
        .map(|arr| normalize_voicewake_triggers(arr))
        .unwrap_or_else(default_voicewake_triggers);
    Ok(json!({ "triggers": triggers }))
}

async fn handle_voicewake_set(
    triggers: &[serde_json::Value],
) -> Result<serde_json::Value, ErrorShape> {
    let path = resolve_voicewake_path();
    let triggers = normalize_voicewake_triggers(triggers);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ErrorShape::new(
                error_codes::UNAVAILABLE,
                format!("failed to create voicewake dir: {}", e),
            )
        })?;
    }
    let file = json!({ "triggers": triggers, "updatedAtMs": now_ms() });
    let raw = serde_json::to_string_pretty(&file).unwrap_or_else(|_| file.to_string());
    std::fs::write(&path, raw).map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("failed to write voicewake config: {}", e),
        )
    })?;
    Ok(json!({ "triggers": file.get("triggers").cloned().unwrap_or_else(|| json!([])) }))
}

// ---------------------------------------------------------------------------
// TTS (OpenClaw compatibility)
// ---------------------------------------------------------------------------

const OPENAI_TTS_MODELS: &[&str] = &["gpt-4o-mini-tts", "tts-1", "tts-1-hd"];
const OPENAI_TTS_VOICES: &[&str] = &[
    "alloy", "ash", "coral", "echo", "fable", "onyx", "nova", "sage", "shimmer",
];
const ELEVENLABS_TTS_MODELS: &[&str] = &[
    "eleven_multilingual_v2",
    "eleven_turbo_v2_5",
    "eleven_monolingual_v1",
];
const DEFAULT_ELEVENLABS_VOICE_ID: &str = "pMsXgVXv3BLzUgSXRplE";
const TTS_PROVIDERS: &[&str] = &["openai", "elevenlabs", "edge"];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TtsPrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auto: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TtsPrefsFile {
    #[serde(default)]
    tts: TtsPrefs,
}

fn resolve_tts_prefs_path() -> PathBuf {
    // Match OpenClaw's convention: stateDir/settings/tts.json
    if let Some(dir) = resolve_data_dir() {
        return dir.join("settings").join("tts.json");
    }
    PathBuf::from("tts.json")
}

fn load_tts_prefs(path: &PathBuf) -> TtsPrefsFile {
    read_json_file(path).unwrap_or_default()
}

fn store_tts_prefs(path: &PathBuf, prefs: &TtsPrefsFile) -> Result<(), ErrorShape> {
    write_json_atomic(path, prefs)
}

fn normalize_tts_provider(value: &str) -> &'static str {
    match value.trim().to_lowercase().as_str() {
        "openai" => "openai",
        "elevenlabs" => "elevenlabs",
        "edge" => "edge",
        _ => "edge",
    }
}

fn resolve_openai_tts_key(state: &GatewayState) -> Option<String> {
    if let Some(cfg) = state.config().providers.openai.as_ref() {
        let key = cfg.api_key.trim();
        if !key.is_empty() {
            return Some(key.to_string());
        }
    }
    std::env::var("OPENAI_API_KEY")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn resolve_elevenlabs_tts_key() -> Option<String> {
    std::env::var("ELEVENLABS_API_KEY")
        .or_else(|_| std::env::var("XI_API_KEY"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn is_edge_tts_enabled() -> bool {
    // Best-effort: map OpenClaw's "edge" provider to OS-native TTS.
    cfg!(target_os = "macos")
}

fn is_tts_provider_configured(state: &GatewayState, provider: &str) -> bool {
    match provider {
        "openai" => resolve_openai_tts_key(state).is_some(),
        "elevenlabs" => resolve_elevenlabs_tts_key().is_some(),
        "edge" => is_edge_tts_enabled(),
        _ => false,
    }
}

fn resolve_tts_provider_order(primary: &str) -> Vec<&'static str> {
    let primary = normalize_tts_provider(primary);
    let mut out = Vec::with_capacity(TTS_PROVIDERS.len());
    out.push(primary);
    for p in TTS_PROVIDERS {
        if *p != primary {
            out.push(*p);
        }
    }
    out
}

fn resolve_active_tts_provider(state: &GatewayState, prefs: &TtsPrefsFile) -> String {
    let preferred = prefs
        .tts
        .provider
        .as_deref()
        .map(normalize_tts_provider)
        .unwrap_or("edge");

    for p in resolve_tts_provider_order(preferred) {
        if is_tts_provider_configured(state, p) {
            return p.to_string();
        }
    }
    preferred.to_string()
}

fn resolve_tts_enabled(prefs: &TtsPrefsFile) -> bool {
    prefs.tts.enabled.unwrap_or(false)
}

fn resolve_tts_auto(prefs: &TtsPrefsFile) -> String {
    if let Some(raw) = prefs.tts.auto.as_deref() {
        let val = raw.trim().to_lowercase();
        match val.as_str() {
            "off" | "always" | "inbound" | "tagged" => return val,
            _ => {}
        }
    }
    if resolve_tts_enabled(prefs) {
        "always".to_string()
    } else {
        "off".to_string()
    }
}

async fn handle_tts_status(state: &GatewayState) -> Result<serde_json::Value, ErrorShape> {
    let prefs_path = resolve_tts_prefs_path();
    let prefs = load_tts_prefs(&prefs_path);
    let provider = resolve_active_tts_provider(state, &prefs);
    let auto = resolve_tts_auto(&prefs);
    let enabled = resolve_tts_enabled(&prefs);

    let has_openai_key = resolve_openai_tts_key(state).is_some();
    let has_eleven_key = resolve_elevenlabs_tts_key().is_some();
    let edge_enabled = is_edge_tts_enabled();

    let fallback_providers = resolve_tts_provider_order(&provider)
        .into_iter()
        .skip(1)
        .filter(|p| is_tts_provider_configured(state, p))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let fallback_provider = fallback_providers.first().cloned().unwrap_or_default();

    Ok(json!({
        "enabled": enabled,
        "auto": auto,
        "provider": provider,
        "fallbackProvider": if fallback_provider.is_empty() { serde_json::Value::Null } else { json!(fallback_provider) },
        "fallbackProviders": fallback_providers,
        "prefsPath": prefs_path.to_string_lossy(),
        "hasOpenAIKey": has_openai_key,
        "hasElevenLabsKey": has_eleven_key,
        "edgeEnabled": edge_enabled,
    }))
}

async fn handle_tts_enable(
    _state: &GatewayState,
    enabled: bool,
) -> Result<serde_json::Value, ErrorShape> {
    let path = resolve_tts_prefs_path();
    let mut prefs = load_tts_prefs(&path);
    prefs.tts.enabled = Some(enabled);
    store_tts_prefs(&path, &prefs)?;
    Ok(json!({ "enabled": enabled }))
}

async fn handle_tts_set_provider(provider: &str) -> Result<serde_json::Value, ErrorShape> {
    let provider = normalize_tts_provider(provider);
    if provider != "openai" && provider != "elevenlabs" && provider != "edge" {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "Invalid provider. Use openai, elevenlabs, or edge.",
        ));
    }
    let path = resolve_tts_prefs_path();
    let mut prefs = load_tts_prefs(&path);
    prefs.tts.provider = Some(provider.to_string());
    store_tts_prefs(&path, &prefs)?;
    Ok(json!({ "provider": provider }))
}

async fn handle_tts_providers(state: &GatewayState) -> Result<serde_json::Value, ErrorShape> {
    let prefs_path = resolve_tts_prefs_path();
    let prefs = load_tts_prefs(&prefs_path);
    let active = resolve_active_tts_provider(state, &prefs);

    Ok(json!({
        "providers": [
            {
                "id": "openai",
                "name": "OpenAI",
                "configured": resolve_openai_tts_key(state).is_some(),
                "models": OPENAI_TTS_MODELS,
                "voices": OPENAI_TTS_VOICES,
            },
            {
                "id": "elevenlabs",
                "name": "ElevenLabs",
                "configured": resolve_elevenlabs_tts_key().is_some(),
                "models": ELEVENLABS_TTS_MODELS,
            },
            {
                "id": "edge",
                "name": "Edge TTS",
                "configured": is_edge_tts_enabled(),
                "models": [],
            }
        ],
        "active": active,
    }))
}

fn resolve_media_dir_fallback() -> PathBuf {
    resolve_data_dir()
        .map(|dir| dir.join("media"))
        .unwrap_or_else(|| std::env::temp_dir().join("drbot-media"))
}

async fn handle_tts_convert(
    state: &GatewayState,
    text: &str,
    channel: Option<&str>,
) -> Result<serde_json::Value, ErrorShape> {
    let _ = channel; // channel-specific formats not implemented yet
    let prefs_path = resolve_tts_prefs_path();
    let prefs = load_tts_prefs(&prefs_path);
    let provider = resolve_active_tts_provider(state, &prefs);

    let tts: TextToSpeech = match provider.as_str() {
        "openai" => {
            let key = resolve_openai_tts_key(state).ok_or_else(|| {
                ErrorShape::new(error_codes::UNAVAILABLE, "OpenAI TTS key not configured")
            })?;
            TextToSpeech::new(Box::new(OpenAiTts::new(&key))).with_default_voice("alloy")
        }
        "elevenlabs" => {
            let key = resolve_elevenlabs_tts_key().ok_or_else(|| {
                ErrorShape::new(
                    error_codes::UNAVAILABLE,
                    "ElevenLabs TTS key not configured",
                )
            })?;
            TextToSpeech::new(Box::new(ElevenLabsTts::new(&key).multilingual()))
                .with_default_voice(DEFAULT_ELEVENLABS_VOICE_ID)
        }
        "edge" => TextToSpeech::new(Box::new(SystemTts)),
        other => {
            return Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                format!("unknown tts provider: {}", other),
            ));
        }
    };

    let result = tts.synthesize(text, None).await.map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("TTS conversion failed: {}", e),
        )
    })?;

    let base = resolve_media_dir_fallback().join("tts");
    std::fs::create_dir_all(&base).map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!(
                "failed to create media dir {}: {}",
                base.to_string_lossy(),
                e
            ),
        )
    })?;
    let ext = match result.format.as_str() {
        "mp3" => "mp3",
        "aiff" => "aiff",
        other => other,
    };
    let filename = format!("{}.{}", Uuid::new_v4(), ext);
    let path = base.join(filename);
    let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    std::fs::write(&tmp, &result.audio).map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("failed to write tts audio {}: {}", tmp.to_string_lossy(), e),
        )
    })?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!(
                "failed to move {} -> {}: {}",
                tmp.to_string_lossy(),
                path.to_string_lossy(),
                e
            ),
        )
    })?;

    Ok(json!({
        "audioPath": path.to_string_lossy(),
        "provider": provider,
        "outputFormat": result.format,
        "voiceCompatible": false,
    }))
}

fn normalize_browser_node_key(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

fn is_browser_node(client: &OpenclawClient) -> bool {
    client.caps.iter().any(|c| c == "browser")
        || client.commands.iter().any(|c| c == "browser.proxy")
}

async fn handle_browser_request(
    state: &GatewayState,
    method: &str,
    path: &str,
    query: Option<&serde_json::Value>,
    body: Option<&serde_json::Value>,
    timeout_ms: Option<u64>,
) -> Result<serde_json::Value, ErrorShape> {
    let method = method.trim().to_uppercase();
    let mut path = path.trim().to_string();
    if path.is_empty() {
        path = "/".to_string();
    }
    if !path.starts_with('/') {
        path = format!("/{}", path);
    }

    // OpenClaw parity: prefer proxying browser routes to a connected browser-capable node
    // (companion app or node host) when one is available.
    let timeout_ms = timeout_ms.unwrap_or(20_000).clamp(1, 900_000);
    let requested_node = std::env::var("DRBOT_OPENCLAW_BROWSER_NODE")
        .ok()
        .unwrap_or_default()
        .trim()
        .to_string();
    let connected_nodes = state
        .list_openclaw_clients()
        .await
        .into_iter()
        .filter(|c| c.role == "node")
        .filter(|c| is_browser_node(c))
        .filter(|c| supports_node_command(&c.commands, "browser.proxy"))
        .collect::<Vec<_>>();

    let select_proxy_node = || -> Result<Option<String>, ErrorShape> {
        if connected_nodes.is_empty() {
            return Ok(None);
        }
        if requested_node.is_empty() {
            if connected_nodes.len() == 1 {
                let c = connected_nodes
                    .first()
                    .expect("len checked");
                let id = c
                    .device_id
                    .clone()
                    .or(c.instance_id.clone())
                    .unwrap_or_else(|| c.conn_id.clone());
                return Ok(Some(id));
            }
            return Ok(None);
        }

        let q = requested_node.trim();
        let q_norm = normalize_browser_node_key(q);
        let matches = connected_nodes
            .iter()
            .filter_map(|c| {
                let id = c
                    .device_id
                    .clone()
                    .or(c.instance_id.clone())
                    .unwrap_or_else(|| c.conn_id.clone());
                if id == q {
                    return Some(id);
                }
                if c.peer.ip().to_string() == q {
                    return Some(id);
                }
                if let Some(name) = c.display_name.as_deref() {
                    if !name.trim().is_empty() && normalize_browser_node_key(name) == q_norm {
                        return Some(id);
                    }
                }
                if q.len() >= 6 && id.starts_with(q) {
                    return Some(id);
                }
                None
            })
            .collect::<Vec<_>>();

        if matches.len() == 1 {
            return Ok(Some(matches[0].clone()));
        }
        if matches.is_empty() {
            return Err(ErrorShape::new(
                error_codes::UNAVAILABLE,
                format!("Configured browser node not connected: {}", q),
            ));
        }
        Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            format!("ambiguous browser node: {}", q),
        ))
    };

    if let Some(node_id) = select_proxy_node()? {
        let profile = query
            .and_then(|q| q.get("profile"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let proxy_params = json!({
            "method": method,
            "path": path,
            "query": query.cloned().unwrap_or_else(|| json!({})),
            "body": body.cloned().unwrap_or(serde_json::Value::Null),
            "timeoutMs": timeout_ms,
            "profile": profile,
        });
        let payload =
            invoke_node_command(state, &node_id, "browser.proxy", proxy_params, timeout_ms).await?;
        let Some(obj) = payload.as_object() else {
            return Err(ErrorShape::new(
                error_codes::UNAVAILABLE,
                "browser proxy failed",
            ));
        };
        let Some(result) = obj.get("result").cloned() else {
            return Err(ErrorShape::new(
                error_codes::UNAVAILABLE,
                "browser proxy failed",
            ));
        };

        let mut mapping: HashMap<String, String> = HashMap::new();
        if let Some(files) = obj.get("files").and_then(|v| v.as_array()) {
            let base = resolve_media_dir_fallback().join("browser");
            std::fs::create_dir_all(&base).map_err(|e| {
                ErrorShape::new(
                    error_codes::UNAVAILABLE,
                    format!(
                        "failed to create media dir {}: {}",
                        base.to_string_lossy(),
                        e
                    ),
                )
            })?;

            for file in files {
                let Some(path) = file.get("path").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(raw_b64) = file.get("base64").and_then(|v| v.as_str()) else {
                    continue;
                };
                let mime = file
                    .get("mimeType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let bytes = base64_decode_url_safe_best_effort(raw_b64).unwrap_or_default();
                if bytes.is_empty() {
                    continue;
                }

                let ext = match mime {
                    "image/png" => "png",
                    "image/jpeg" => "jpg",
                    "application/pdf" => "pdf",
                    _ => "bin",
                };
                let out_path = base.join(format!("{}.{}", Uuid::new_v4(), ext));
                let tmp = out_path.with_extension(format!("{}.tmp", Uuid::new_v4()));
                std::fs::write(&tmp, &bytes).map_err(|e| {
                    ErrorShape::new(
                        error_codes::UNAVAILABLE,
                        format!("failed to write {}: {}", tmp.to_string_lossy(), e),
                    )
                })?;
                std::fs::rename(&tmp, &out_path).map_err(|e| {
                    ErrorShape::new(
                        error_codes::UNAVAILABLE,
                        format!(
                            "failed to move {} -> {}: {}",
                            tmp.to_string_lossy(),
                            out_path.to_string_lossy(),
                            e
                        ),
                    )
                })?;
                mapping.insert(path.to_string(), out_path.to_string_lossy().to_string());
            }
        }

        let mut out = result;
        if !mapping.is_empty() {
            let apply_path = |obj: &mut serde_json::Map<String, serde_json::Value>, key: &str| {
                let Some(v) = obj.get_mut(key) else { return; };
                let Some(raw) = v.as_str() else { return; };
                if let Some(next) = mapping.get(raw) {
                    *v = json!(next);
                }
            };

            if let Some(obj) = out.as_object_mut() {
                apply_path(obj, "path");
                apply_path(obj, "imagePath");
                if let Some(download) = obj.get_mut("download").and_then(|v| v.as_object_mut()) {
                    apply_path(download, "path");
                }
            }
        }

        return Ok(out);
    }

    match (method.as_str(), path.as_str()) {
        ("GET", "/") | ("GET", "/status") => Ok(json!({
            "enabled": true,
            "running": false,
            "ts": now_ms(),
        })),
        ("POST", "/screenshot") => {
            let url = query
                .and_then(|q| q.get("url"))
                .and_then(|v| v.as_str())
                .or_else(|| body.and_then(|b| b.get("url")).and_then(|v| v.as_str()))
                .unwrap_or("")
                .trim()
                .to_string();
            if url.is_empty() {
                return Err(ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    "browser screenshot requires url",
                ));
            }
            let full_page = body
                .and_then(|b| b.get("fullPage"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let fut = async {
                let automation = BrowserAutomation::new().await.map_err(|e| {
                    ErrorShape::new(
                        error_codes::UNAVAILABLE,
                        format!("browser init failed: {}", e),
                    )
                })?;
                let bytes = if full_page {
                    automation.screenshot_full_page(&url).await
                } else {
                    automation.screenshot_url(&url).await
                };
                // Close best-effort (ignore errors).
                let _ = automation.close().await;
                bytes.map_err(|e| {
                    ErrorShape::new(
                        error_codes::UNAVAILABLE,
                        format!("screenshot failed: {}", e),
                    )
                })
            };

            let bytes = tokio::time::timeout(Duration::from_millis(timeout_ms), fut)
                .await
                .map_err(|_| ErrorShape::new(error_codes::UNAVAILABLE, "browser request timed out"))??;

            let base = resolve_media_dir_fallback().join("browser");
            std::fs::create_dir_all(&base).map_err(|e| {
                ErrorShape::new(
                    error_codes::UNAVAILABLE,
                    format!(
                        "failed to create media dir {}: {}",
                        base.to_string_lossy(),
                        e
                    ),
                )
            })?;
            let path = base.join(format!("{}.png", Uuid::new_v4()));
            let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
            std::fs::write(&tmp, &bytes).map_err(|e| {
                ErrorShape::new(
                    error_codes::UNAVAILABLE,
                    format!(
                        "failed to write screenshot {}: {}",
                        tmp.to_string_lossy(),
                        e
                    ),
                )
            })?;
            std::fs::rename(&tmp, &path).map_err(|e| {
                ErrorShape::new(
                    error_codes::UNAVAILABLE,
                    format!(
                        "failed to move {} -> {}: {}",
                        tmp.to_string_lossy(),
                        path.to_string_lossy(),
                        e
                    ),
                )
            })?;

            Ok(json!({
                "ok": true,
                "path": path.to_string_lossy(),
                "url": url,
                "targetId": serde_json::Value::Null,
            }))
        }
        _ => Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            format!("unsupported browser route: {} {}", method, path),
        )),
    }
}

async fn handle_sessions_preview(
    state: &GatewayState,
    keys: &[String],
    limit: usize,
    max_chars: usize,
) -> serde_json::Value {
    let ts = now_ms();
    let mut previews: Vec<serde_json::Value> = Vec::new();

    let store = match state.session_store() {
        Some(s) => s.clone(),
        None => {
            return json!({ "ts": ts, "previews": [] });
        }
    };

    for key in keys.iter().take(64) {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        // Try "openclaw" first, then fall back to split keys.
        let mut session = store.get_by_channel("openclaw", key).await.ok().flatten();
        if session.is_none() {
            let (channel_type, channel_id) = session_key_to_channel(key);
            session = store
                .get_by_channel(&channel_type, &channel_id)
                .await
                .ok()
                .flatten();
        }

        let Some(session) = session else {
            previews.push(json!({ "key": key, "status": "missing", "items": [] }));
            continue;
        };

        let slice = if session.messages.len() > limit {
            &session.messages[session.messages.len() - limit..]
        } else {
            &session.messages[..]
        };

        let mut items: Vec<serde_json::Value> = Vec::new();
        for msg in slice {
            let role = match msg.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            let mut text = msg.text_content();
            if text.trim().is_empty() {
                // Best-effort placeholders for non-text content.
                text = msg
                    .content
                    .iter()
                    .find_map(|c| match c {
                        Content::Image { .. } => Some("[image]".to_string()),
                        Content::Audio { .. } => Some("[audio]".to_string()),
                        Content::File { name, .. } => Some(format!("[file: {}]", name)),
                        Content::ToolUse { name, .. } => Some(format!("[tool_use: {}]", name)),
                        Content::ToolResult { .. } => Some("[tool_result]".to_string()),
                        Content::Text { .. } => None,
                    })
                    .unwrap_or_default();
            }
            if text.chars().count() > max_chars {
                text = text.chars().take(max_chars).collect();
            }
            if text.trim().is_empty() {
                continue;
            }
            items.push(json!({ "role": role, "text": text }));
        }

        let status = if items.is_empty() { "empty" } else { "ok" };
        previews.push(json!({ "key": key, "status": status, "items": items }));
    }

    json!({ "ts": ts, "previews": previews })
}

async fn handle_sessions_reset(
    state: &GatewayState,
    key: &str,
) -> Result<serde_json::Value, ErrorShape> {
    let store = state
        .session_store()
        .ok_or_else(|| ErrorShape::new(error_codes::UNAVAILABLE, "session store not configured"))?;

    let mut session = store
        .get_by_channel("openclaw", key)
        .await
        .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
    if session.is_none() {
        let (channel_type, channel_id) = session_key_to_channel(key);
        session = store
            .get_by_channel(&channel_type, &channel_id)
            .await
            .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
    }
    let mut session =
        session.ok_or_else(|| ErrorShape::new(error_codes::INVALID_REQUEST, "unknown session"))?;

    session.clear_messages();
    session.metadata.total_input_tokens = 0;
    session.metadata.total_output_tokens = 0;
    session.update_timestamp();

    store
        .update(&session)
        .await
        .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;

    Ok(json!({
        "ok": true,
        "key": key,
        "entry": {
            "sessionId": session.id.to_string(),
            "updatedAt": session.updated_at.timestamp_millis(),
            "inputTokens": 0,
            "outputTokens": 0,
            "totalTokens": 0,
        }
    }))
}

async fn handle_sessions_compact(
    state: &GatewayState,
    key: &str,
    max_lines: usize,
) -> Result<serde_json::Value, ErrorShape> {
    let store = state
        .session_store()
        .ok_or_else(|| ErrorShape::new(error_codes::UNAVAILABLE, "session store not configured"))?;

    let mut session = store
        .get_by_channel("openclaw", key)
        .await
        .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
    if session.is_none() {
        let (channel_type, channel_id) = session_key_to_channel(key);
        session = store
            .get_by_channel(&channel_type, &channel_id)
            .await
            .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;
    }
    let mut session =
        session.ok_or_else(|| ErrorShape::new(error_codes::INVALID_REQUEST, "unknown session"))?;

    if session.messages.len() <= max_lines {
        return Ok(json!({
            "ok": true,
            "key": key,
            "compacted": false,
            "kept": session.messages.len(),
        }));
    }

    let start = session.messages.len().saturating_sub(max_lines);
    session.messages = session.messages[start..].to_vec();
    session.metadata.message_count = session.messages.len();
    session.update_timestamp();

    store
        .update(&session)
        .await
        .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()))?;

    Ok(json!({
        "ok": true,
        "key": key,
        "compacted": true,
        "archived": null,
        "kept": session.messages.len(),
    }))
}

async fn handle_usage_status(state: &GatewayState) -> serde_json::Value {
    let (provider, display_name) = match state
        .config()
        .providers
        .default_provider
        .as_deref()
        .unwrap_or("auto")
    {
        "anthropic" | "claude" => ("anthropic", "Anthropic"),
        "openai" | "gpt" => ("openai-codex", "OpenAI"),
        "ollama" | "local" => ("openai-codex", "Local"),
        _ => ("openai-codex", "Provider"),
    };

    json!({
        "updatedAt": now_ms(),
        "providers": [{
            "provider": provider,
            "displayName": display_name,
            "windows": [{
                "label": "quota",
                "usedPercent": 0,
                "resetAt": null
            }]
        }]
    })
}

async fn handle_usage_cost(state: &GatewayState, days: usize) -> serde_json::Value {
    use chrono::{Duration as ChronoDuration, Local};

    let days = days.max(1).min(365);
    let today = Local::now().date_naive();
    let start = today - ChronoDuration::days((days - 1) as i64);

    let mut by_date: HashMap<String, (u64, u64)> = HashMap::new();
    if let Some(store) = state.session_store() {
        let list = store
            .list(drbot_sessions::ListOptions {
                include_archived: true,
                ..Default::default()
            })
            .await
            .unwrap_or_default();

        for s in list {
            let d = s.updated_at.with_timezone(&Local).date_naive();
            if d < start || d > today {
                continue;
            }
            let key = d.format("%Y-%m-%d").to_string();
            let entry = by_date.entry(key).or_insert((0, 0));
            entry.0 = entry.0.saturating_add(s.metadata.total_input_tokens as u64);
            entry.1 = entry
                .1
                .saturating_add(s.metadata.total_output_tokens as u64);
        }
    }

    let mut daily: Vec<serde_json::Value> = Vec::new();
    let mut totals_input: u64 = 0;
    let mut totals_output: u64 = 0;
    for i in 0..days {
        let d = start + ChronoDuration::days(i as i64);
        let date = d.format("%Y-%m-%d").to_string();
        let (input, output) = by_date.get(&date).copied().unwrap_or((0, 0));
        totals_input = totals_input.saturating_add(input);
        totals_output = totals_output.saturating_add(output);
        daily.push(json!({
            "date": date,
            "input": input,
            "output": output,
            "cacheRead": 0,
            "cacheWrite": 0,
            "totalTokens": input + output,
            "totalCost": 0,
            "missingCostEntries": 0,
        }));
    }

    json!({
        "updatedAt": now_ms(),
        "days": days,
        "daily": daily,
        "totals": {
            "input": totals_input,
            "output": totals_output,
            "cacheRead": 0,
            "cacheWrite": 0,
            "totalTokens": totals_input + totals_output,
            "totalCost": 0,
            "missingCostEntries": 0,
        }
    })
}

const PAIRING_PENDING_TTL_MS: u64 = 5 * 60 * 1000;

fn new_pairing_token() -> String {
    Uuid::new_v4().to_string().replace('-', "")
}

fn resolve_node_pairing_paths(state: &GatewayState) -> (PathBuf, PathBuf) {
    if let Some(dir) = resolve_openclaw_state_dir(state) {
        return (
            dir.join("nodes").join("pending.json"),
            dir.join("nodes").join("paired.json"),
        );
    }
    (
        PathBuf::from("nodes_pending.json"),
        PathBuf::from("nodes_paired.json"),
    )
}

fn resolve_device_pairing_paths(state: &GatewayState) -> (PathBuf, PathBuf) {
    if let Some(dir) = resolve_openclaw_state_dir(state) {
        return (
            dir.join("devices").join("pending.json"),
            dir.join("devices").join("paired.json"),
        );
    }
    (
        PathBuf::from("devices_pending.json"),
        PathBuf::from("devices_paired.json"),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodePairingPendingRequest {
    #[serde(rename = "requestId")]
    request_id: String,
    #[serde(rename = "nodeId")]
    node_id: String,
    #[serde(
        default,
        rename = "displayName",
        skip_serializing_if = "Option::is_none"
    )]
    display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(
        default,
        rename = "coreVersion",
        skip_serializing_if = "Option::is_none"
    )]
    core_version: Option<String>,
    #[serde(default, rename = "uiVersion", skip_serializing_if = "Option::is_none")]
    ui_version: Option<String>,
    #[serde(
        default,
        rename = "deviceFamily",
        skip_serializing_if = "Option::is_none"
    )]
    device_family: Option<String>,
    #[serde(
        default,
        rename = "modelIdentifier",
        skip_serializing_if = "Option::is_none"
    )]
    model_identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caps: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commands: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    permissions: Option<HashMap<String, bool>>,
    #[serde(default, rename = "remoteIp", skip_serializing_if = "Option::is_none")]
    remote_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    silent: Option<bool>,
    #[serde(default, rename = "isRepair", skip_serializing_if = "Option::is_none")]
    is_repair: Option<bool>,
    ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NodePairingPairedNode {
    #[serde(rename = "nodeId")]
    node_id: String,
    token: String,
    #[serde(
        default,
        rename = "displayName",
        skip_serializing_if = "Option::is_none"
    )]
    display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(
        default,
        rename = "coreVersion",
        skip_serializing_if = "Option::is_none"
    )]
    core_version: Option<String>,
    #[serde(default, rename = "uiVersion", skip_serializing_if = "Option::is_none")]
    ui_version: Option<String>,
    #[serde(
        default,
        rename = "deviceFamily",
        skip_serializing_if = "Option::is_none"
    )]
    device_family: Option<String>,
    #[serde(
        default,
        rename = "modelIdentifier",
        skip_serializing_if = "Option::is_none"
    )]
    model_identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    caps: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commands: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bins: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    permissions: Option<HashMap<String, bool>>,
    #[serde(default, rename = "remoteIp", skip_serializing_if = "Option::is_none")]
    remote_ip: Option<String>,
    #[serde(rename = "createdAtMs")]
    created_at_ms: u64,
    #[serde(rename = "approvedAtMs")]
    approved_at_ms: u64,
    #[serde(
        default,
        rename = "lastConnectedAtMs",
        skip_serializing_if = "Option::is_none"
    )]
    last_connected_at_ms: Option<u64>,
}

fn load_node_pairing_state(
    state: &GatewayState,
) -> (
    HashMap<String, NodePairingPendingRequest>,
    HashMap<String, NodePairingPairedNode>,
) {
    let (pending_path, paired_path) = resolve_node_pairing_paths(state);
    let mut pending: HashMap<String, NodePairingPendingRequest> =
        read_json_file(&pending_path).unwrap_or_default();
    let paired: HashMap<String, NodePairingPairedNode> =
        read_json_file(&paired_path).unwrap_or_default();

    let now = now_ms();
    pending.retain(|_, v| now.saturating_sub(v.ts) <= PAIRING_PENDING_TTL_MS);
    (pending, paired)
}

fn persist_node_pairing_state(
    state: &GatewayState,
    pending: &HashMap<String, NodePairingPendingRequest>,
    paired: &HashMap<String, NodePairingPairedNode>,
) -> Result<(), ErrorShape> {
    let (pending_path, paired_path) = resolve_node_pairing_paths(state);
    write_json_atomic(&pending_path, pending)?;
    write_json_atomic(&paired_path, paired)?;
    Ok(())
}

fn list_node_pairing(state: &GatewayState) -> Result<serde_json::Value, ErrorShape> {
    let (pending, paired) = load_node_pairing_state(state);
    let mut pending_list: Vec<NodePairingPendingRequest> = pending.into_values().collect();
    pending_list.sort_by(|a, b| b.ts.cmp(&a.ts));
    let mut paired_list: Vec<NodePairingPairedNode> = paired.into_values().collect();
    paired_list.sort_by(|a, b| b.approved_at_ms.cmp(&a.approved_at_ms));
    Ok(json!({ "pending": pending_list, "paired": paired_list }))
}

fn request_node_pairing(
    state: &GatewayState,
    node_id: &str,
    meta: &serde_json::Value,
) -> Result<(serde_json::Value, Option<NodePairingPendingRequest>), ErrorShape> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "nodeId required",
        ));
    }

    let (mut pending, paired) = load_node_pairing_state(state);

    if let Some(existing) = pending.values().find(|p| p.node_id == node_id).cloned() {
        return Ok((
            json!({ "status": "pending", "request": existing, "created": false }),
            None,
        ));
    }

    let is_repair = paired.contains_key(node_id);
    let req = NodePairingPendingRequest {
        request_id: Uuid::new_v4().to_string(),
        node_id: node_id.to_string(),
        display_name: meta
            .get("displayName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        platform: meta
            .get("platform")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        version: meta
            .get("version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        core_version: meta
            .get("coreVersion")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        ui_version: meta
            .get("uiVersion")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        device_family: meta
            .get("deviceFamily")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        model_identifier: meta
            .get("modelIdentifier")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        caps: meta.get("caps").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        }),
        commands: meta.get("commands").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        }),
        permissions: None,
        remote_ip: meta
            .get("remoteIp")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        silent: meta.get("silent").and_then(|v| v.as_bool()),
        is_repair: Some(is_repair),
        ts: now_ms(),
    };
    pending.insert(req.request_id.clone(), req.clone());
    persist_node_pairing_state(state, &pending, &paired)?;
    Ok((
        json!({ "status": "pending", "request": req, "created": true }),
        Some(req),
    ))
}

fn approve_node_pairing(
    state: &GatewayState,
    request_id: &str,
) -> Result<Option<(String, NodePairingPairedNode)>, ErrorShape> {
    let request_id = request_id.trim();
    let (mut pending, mut paired) = load_node_pairing_state(state);
    let Some(req) = pending.remove(request_id) else {
        return Ok(None);
    };

    let now = now_ms();
    let existing = paired.get(&req.node_id).cloned();
    let node = NodePairingPairedNode {
        node_id: req.node_id.clone(),
        token: new_pairing_token(),
        display_name: req.display_name.clone(),
        platform: req.platform.clone(),
        version: req.version.clone(),
        core_version: req.core_version.clone(),
        ui_version: req.ui_version.clone(),
        device_family: req.device_family.clone(),
        model_identifier: req.model_identifier.clone(),
        caps: req.caps.clone(),
        commands: req.commands.clone(),
        bins: existing.as_ref().and_then(|e| e.bins.clone()),
        permissions: req.permissions.clone(),
        remote_ip: req.remote_ip.clone(),
        created_at_ms: existing.as_ref().map(|e| e.created_at_ms).unwrap_or(now),
        approved_at_ms: now,
        last_connected_at_ms: existing.and_then(|e| e.last_connected_at_ms),
    };
    paired.insert(node.node_id.clone(), node.clone());
    persist_node_pairing_state(state, &pending, &paired)?;
    Ok(Some((request_id.to_string(), node)))
}

fn reject_node_pairing(
    state: &GatewayState,
    request_id: &str,
) -> Result<Option<(String, String)>, ErrorShape> {
    let request_id = request_id.trim();
    let (mut pending, paired) = load_node_pairing_state(state);
    let Some(req) = pending.remove(request_id) else {
        return Ok(None);
    };
    persist_node_pairing_state(state, &pending, &paired)?;
    Ok(Some((request_id.to_string(), req.node_id)))
}

fn verify_node_token(
    state: &GatewayState,
    node_id: &str,
    token: &str,
) -> Result<serde_json::Value, ErrorShape> {
    let node_id = node_id.trim();
    let token = token.trim();
    let (_pending, paired) = load_node_pairing_state(state);
    let Some(node) = paired.get(node_id) else {
        return Ok(json!({ "ok": false }));
    };
    if node.token == token {
        Ok(json!({ "ok": true, "node": node }))
    } else {
        Ok(json!({ "ok": false }))
    }
}

fn rename_paired_node(
    state: &GatewayState,
    node_id: &str,
    display_name: &str,
) -> Result<Option<NodePairingPairedNode>, ErrorShape> {
    let node_id = node_id.trim();
    let display_name = display_name.trim();
    if node_id.is_empty() || display_name.is_empty() {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "nodeId and displayName required",
        ));
    }
    let (pending, mut paired) = load_node_pairing_state(state);
    let Some(existing) = paired.get(node_id).cloned() else {
        return Ok(None);
    };
    let next = NodePairingPairedNode {
        display_name: Some(display_name.to_string()),
        ..existing
    };
    paired.insert(node_id.to_string(), next.clone());
    persist_node_pairing_state(state, &pending, &paired)?;
    Ok(Some(next))
}

fn update_paired_node_last_connected(
    state: &GatewayState,
    node_id: &str,
    connected_at_ms: u64,
) -> Result<(), ErrorShape> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Ok(());
    }
    let (pending, mut paired) = load_node_pairing_state(state);
    let Some(existing) = paired.get(node_id).cloned() else {
        return Ok(());
    };
    let next = NodePairingPairedNode {
        last_connected_at_ms: Some(connected_at_ms),
        ..existing
    };
    paired.insert(node_id.to_string(), next);
    persist_node_pairing_state(state, &pending, &paired)?;
    Ok(())
}

fn update_paired_node_metadata(state: &GatewayState, node_id: &str, client: &OpenclawClient) -> Result<(), ErrorShape> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Ok(());
    }

    let (pending, mut paired) = load_node_pairing_state(state);
    let Some(existing) = paired.get(node_id).cloned() else {
        return Ok(());
    };

    let next = NodePairingPairedNode {
        display_name: client.display_name.clone().or(existing.display_name),
        platform: Some(client.platform.clone()),
        version: Some(client.client_version.clone()),
        device_family: client.device_family.clone().or(existing.device_family),
        model_identifier: client.model_identifier.clone().or(existing.model_identifier),
        caps: if client.caps.is_empty() {
            existing.caps
        } else {
            Some(client.caps.clone())
        },
        commands: if client.commands.is_empty() {
            existing.commands
        } else {
            Some(client.commands.clone())
        },
        permissions: if client.permissions.is_empty() {
            existing.permissions
        } else {
            Some(client.permissions.clone())
        },
        remote_ip: Some(client.peer.ip().to_string()),
        last_connected_at_ms: Some(client.connected_at_ms),
        ..existing
    };

    paired.insert(node_id.to_string(), next);
    persist_node_pairing_state(state, &pending, &paired)?;
    Ok(())
}

fn update_paired_node_bins(state: &GatewayState, node_id: &str, bins: Vec<String>) -> Result<bool, ErrorShape> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        return Ok(false);
    }

    let (pending, mut paired) = load_node_pairing_state(state);
    let Some(existing) = paired.get(node_id).cloned() else {
        return Ok(false);
    };

    let mut normalized: Vec<String> = bins
        .into_iter()
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
        .collect();
    normalized.sort();
    normalized.dedup();

    let mut existing_norm = existing.bins.clone().unwrap_or_default();
    existing_norm.sort();
    existing_norm.dedup();

    if existing_norm == normalized {
        return Ok(false);
    }

    let next = NodePairingPairedNode {
        bins: Some(normalized),
        ..existing
    };
    paired.insert(node_id.to_string(), next);
    persist_node_pairing_state(state, &pending, &paired)?;
    crate::openclaw_skills::bump_skills_snapshot_version();
    Ok(true)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DevicePairingPendingRequest {
    #[serde(rename = "requestId")]
    request_id: String,
    #[serde(rename = "deviceId")]
    device_id: String,
    #[serde(rename = "publicKey")]
    public_key: String,
    #[serde(
        default,
        rename = "displayName",
        skip_serializing_if = "Option::is_none"
    )]
    display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    platform: Option<String>,
    #[serde(default, rename = "clientId", skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(
        default,
        rename = "clientMode",
        skip_serializing_if = "Option::is_none"
    )]
    client_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    roles: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scopes: Option<Vec<String>>,
    #[serde(default, rename = "remoteIp", skip_serializing_if = "Option::is_none")]
    remote_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    silent: Option<bool>,
    #[serde(default, rename = "isRepair", skip_serializing_if = "Option::is_none")]
    is_repair: Option<bool>,
    ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceAuthToken {
    token: String,
    role: String,
    scopes: Vec<String>,
    #[serde(rename = "createdAtMs")]
    created_at_ms: u64,
    #[serde(
        default,
        rename = "rotatedAtMs",
        skip_serializing_if = "Option::is_none"
    )]
    rotated_at_ms: Option<u64>,
    #[serde(
        default,
        rename = "revokedAtMs",
        skip_serializing_if = "Option::is_none"
    )]
    revoked_at_ms: Option<u64>,
    #[serde(
        default,
        rename = "lastUsedAtMs",
        skip_serializing_if = "Option::is_none"
    )]
    last_used_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceAuthTokenSummary {
    role: String,
    scopes: Vec<String>,
    #[serde(rename = "createdAtMs")]
    created_at_ms: u64,
    #[serde(
        default,
        rename = "rotatedAtMs",
        skip_serializing_if = "Option::is_none"
    )]
    rotated_at_ms: Option<u64>,
    #[serde(
        default,
        rename = "revokedAtMs",
        skip_serializing_if = "Option::is_none"
    )]
    revoked_at_ms: Option<u64>,
    #[serde(
        default,
        rename = "lastUsedAtMs",
        skip_serializing_if = "Option::is_none"
    )]
    last_used_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PairedDevice {
    #[serde(rename = "deviceId")]
    device_id: String,
    #[serde(rename = "publicKey")]
    public_key: String,
    #[serde(
        default,
        rename = "displayName",
        skip_serializing_if = "Option::is_none"
    )]
    display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    platform: Option<String>,
    #[serde(default, rename = "clientId", skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(
        default,
        rename = "clientMode",
        skip_serializing_if = "Option::is_none"
    )]
    client_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    roles: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scopes: Option<Vec<String>>,
    #[serde(default, rename = "remoteIp", skip_serializing_if = "Option::is_none")]
    remote_ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tokens: Option<HashMap<String, DeviceAuthToken>>,
    #[serde(rename = "createdAtMs")]
    created_at_ms: u64,
    #[serde(rename = "approvedAtMs")]
    approved_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct PairedDeviceRedacted {
    #[serde(rename = "deviceId")]
    device_id: String,
    #[serde(rename = "publicKey")]
    public_key: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "displayName")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "clientId")]
    client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "clientMode")]
    client_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    roles: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "remoteIp")]
    remote_ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tokens: Option<HashMap<String, DeviceAuthTokenSummary>>,
    #[serde(rename = "createdAtMs")]
    created_at_ms: u64,
    #[serde(rename = "approvedAtMs")]
    approved_at_ms: u64,
}

fn summarize_device_tokens(
    tokens: Option<&HashMap<String, DeviceAuthToken>>,
) -> Option<HashMap<String, DeviceAuthTokenSummary>> {
    let tokens = tokens?;
    if tokens.is_empty() {
        return None;
    }
    let mut out = HashMap::new();
    for (role, entry) in tokens {
        out.insert(
            role.clone(),
            DeviceAuthTokenSummary {
                role: entry.role.clone(),
                scopes: entry.scopes.clone(),
                created_at_ms: entry.created_at_ms,
                rotated_at_ms: entry.rotated_at_ms,
                revoked_at_ms: entry.revoked_at_ms,
                last_used_at_ms: entry.last_used_at_ms,
            },
        );
    }
    Some(out)
}

fn load_device_pairing_state(
    state: &GatewayState,
) -> (
    HashMap<String, DevicePairingPendingRequest>,
    HashMap<String, PairedDevice>,
) {
    let (pending_path, paired_path) = resolve_device_pairing_paths(state);
    let mut pending: HashMap<String, DevicePairingPendingRequest> =
        read_json_file(&pending_path).unwrap_or_default();
    let paired: HashMap<String, PairedDevice> = read_json_file(&paired_path).unwrap_or_default();

    let now = now_ms();
    pending.retain(|_, v| now.saturating_sub(v.ts) <= PAIRING_PENDING_TTL_MS);
    (pending, paired)
}

fn persist_device_pairing_state(
    state: &GatewayState,
    pending: &HashMap<String, DevicePairingPendingRequest>,
    paired: &HashMap<String, PairedDevice>,
) -> Result<(), ErrorShape> {
    let (pending_path, paired_path) = resolve_device_pairing_paths(state);
    write_json_atomic(&pending_path, pending)?;
    write_json_atomic(&paired_path, paired)?;
    Ok(())
}

fn list_device_pairing(state: &GatewayState) -> Result<serde_json::Value, ErrorShape> {
    let (pending, paired) = load_device_pairing_state(state);
    let mut pending_list: Vec<DevicePairingPendingRequest> = pending.into_values().collect();
    pending_list.sort_by(|a, b| b.ts.cmp(&a.ts));
    let mut paired_list: Vec<PairedDeviceRedacted> = paired
        .into_values()
        .map(|d| PairedDeviceRedacted {
            device_id: d.device_id,
            public_key: d.public_key,
            display_name: d.display_name,
            platform: d.platform,
            client_id: d.client_id,
            client_mode: d.client_mode,
            role: d.role,
            roles: d.roles,
            scopes: d.scopes,
            remote_ip: d.remote_ip,
            tokens: summarize_device_tokens(d.tokens.as_ref()),
            created_at_ms: d.created_at_ms,
            approved_at_ms: d.approved_at_ms,
        })
        .collect();
    paired_list.sort_by(|a, b| b.approved_at_ms.cmp(&a.approved_at_ms));
    Ok(json!({ "pending": pending_list, "paired": paired_list }))
}

fn normalize_role(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_scopes(raw: &[String]) -> Vec<String> {
    let mut out = std::collections::BTreeSet::<String>::new();
    for item in raw {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.insert(trimmed.to_string());
    }
    out.into_iter().collect()
}

fn merge_roles(items: &[Option<Vec<String>>], singles: &[Option<String>]) -> Option<Vec<String>> {
    let mut out = std::collections::BTreeSet::<String>::new();
    for list in items {
        let Some(list) = list else { continue };
        for item in list {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                continue;
            }
            out.insert(trimmed.to_string());
        }
    }
    for item in singles {
        let Some(item) = item else { continue };
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.insert(trimmed.to_string());
    }
    if out.is_empty() {
        None
    } else {
        Some(out.into_iter().collect())
    }
}

fn merge_scopes(items: &[Option<Vec<String>>]) -> Option<Vec<String>> {
    let mut out = std::collections::BTreeSet::<String>::new();
    for list in items {
        let Some(list) = list else { continue };
        for item in list {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                continue;
            }
            out.insert(trimmed.to_string());
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out.into_iter().collect())
    }
}

fn scopes_allow(requested: &[String], allowed: &[String]) -> bool {
    if requested.is_empty() {
        return true;
    }
    if allowed.is_empty() {
        return false;
    }
    let allowed: std::collections::HashSet<&str> = allowed.iter().map(|s| s.as_str()).collect();
    requested.iter().all(|s| allowed.contains(s.as_str()))
}

fn get_paired_device(state: &GatewayState, device_id: &str) -> Result<Option<PairedDevice>, ErrorShape> {
    let device_id = device_id.trim();
    if device_id.is_empty() {
        return Ok(None);
    }
    let (_pending, paired) = load_device_pairing_state(state);
    Ok(paired.get(device_id).cloned())
}

fn update_paired_device_metadata(state: &GatewayState, meta: &OpenclawClient) -> Result<(), ErrorShape> {
    let Some(device_id) = meta.device_id.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let (pending, mut paired) = load_device_pairing_state(state);
    let Some(existing) = paired.get(device_id).cloned() else {
        return Ok(());
    };

    let merged_roles = merge_roles(
        &[existing.roles.clone()],
        &[existing.role.clone(), Some(meta.role.clone())],
    );
    let merged_scopes = merge_scopes(&[existing.scopes.clone(), Some(meta.scopes.clone())]);

    let next = PairedDevice {
        display_name: meta.display_name.clone().or(existing.display_name.clone()),
        platform: Some(meta.platform.clone()).or(existing.platform.clone()),
        client_id: Some(meta.client_id.clone()).or(existing.client_id.clone()),
        client_mode: Some(meta.client_mode.clone()).or(existing.client_mode.clone()),
        role: Some(meta.role.clone()),
        roles: merged_roles,
        scopes: merged_scopes,
        remote_ip: Some(meta.peer.ip().to_string()).or(existing.remote_ip.clone()),
        ..existing
    };
    paired.insert(device_id.to_string(), next);
    persist_device_pairing_state(state, &pending, &paired)?;
    Ok(())
}

fn verify_device_token(
    state: &GatewayState,
    device_id: &str,
    token: &str,
    role: &str,
    scopes: &[String],
) -> Result<bool, ErrorShape> {
    let device_id = device_id.trim();
    let token = token.trim();
    let Some(role) = normalize_role(role) else {
        return Ok(false);
    };
    if device_id.is_empty() || token.is_empty() {
        return Ok(false);
    }

    let (pending, mut paired) = load_device_pairing_state(state);
    let Some(mut device) = paired.get(device_id).cloned() else {
        return Ok(false);
    };
    let mut tokens = device.tokens.clone().unwrap_or_default();
    let Some(mut entry) = tokens.get(&role).cloned() else {
        return Ok(false);
    };
    if entry.revoked_at_ms.is_some() {
        return Ok(false);
    }
    if entry.token != token {
        return Ok(false);
    }
    let requested_scopes = normalize_scopes(scopes);
    if !scopes_allow(&requested_scopes, &entry.scopes) {
        return Ok(false);
    }
    entry.last_used_at_ms = Some(now_ms());
    tokens.insert(role, entry);
    device.tokens = Some(tokens);
    paired.insert(device_id.to_string(), device);
    persist_device_pairing_state(state, &pending, &paired)?;
    Ok(true)
}

fn ensure_device_token(
    state: &GatewayState,
    device_id: &str,
    role: &str,
    scopes: &[String],
) -> Result<Option<DeviceAuthToken>, ErrorShape> {
    let device_id = device_id.trim();
    let Some(role) = normalize_role(role) else {
        return Ok(None);
    };
    if device_id.is_empty() {
        return Ok(None);
    }

    let (pending, mut paired) = load_device_pairing_state(state);
    let Some(mut device) = paired.get(device_id).cloned() else {
        return Ok(None);
    };
    let requested_scopes = normalize_scopes(scopes);
    let mut tokens = device.tokens.clone().unwrap_or_default();
    let existing = tokens.get(&role).cloned();
    if let Some(existing) = existing.clone() {
        if existing.revoked_at_ms.is_none() && scopes_allow(&requested_scopes, &existing.scopes) {
            return Ok(Some(existing));
        }
    }

    let now = now_ms();
    let next = DeviceAuthToken {
        token: new_pairing_token(),
        role: role.clone(),
        scopes: requested_scopes,
        created_at_ms: existing.as_ref().map(|t| t.created_at_ms).unwrap_or(now),
        rotated_at_ms: existing.as_ref().map(|_| now),
        revoked_at_ms: None,
        last_used_at_ms: existing.and_then(|t| t.last_used_at_ms),
    };
    tokens.insert(role, next.clone());
    device.tokens = Some(tokens);
    paired.insert(device_id.to_string(), device);
    persist_device_pairing_state(state, &pending, &paired)?;
    Ok(Some(next))
}

fn request_device_pairing(
    state: &GatewayState,
    device_id: &str,
    public_key: &str,
    meta: &OpenclawClient,
    silent: Option<bool>,
) -> Result<(DevicePairingPendingRequest, bool), ErrorShape> {
    let device_id = device_id.trim();
    let public_key = public_key.trim();
    if device_id.is_empty() || public_key.is_empty() {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "deviceId and publicKey required",
        ));
    }
    let (mut pending, paired) = load_device_pairing_state(state);
    if let Some(existing) = pending.values().find(|p| p.device_id == device_id).cloned() {
        return Ok((existing, false));
    }
    let is_repair = paired.contains_key(device_id);
    let req = DevicePairingPendingRequest {
        request_id: Uuid::new_v4().to_string(),
        device_id: device_id.to_string(),
        public_key: public_key.to_string(),
        display_name: meta.display_name.clone(),
        platform: Some(meta.platform.clone()),
        client_id: Some(meta.client_id.clone()),
        client_mode: Some(meta.client_mode.clone()),
        role: Some(meta.role.clone()),
        roles: Some(vec![meta.role.clone()]),
        scopes: Some(meta.scopes.clone()),
        remote_ip: Some(meta.peer.ip().to_string()),
        silent,
        is_repair: Some(is_repair),
        ts: now_ms(),
    };
    pending.insert(req.request_id.clone(), req.clone());
    persist_device_pairing_state(state, &pending, &paired)?;
    Ok((req, true))
}

fn approve_device_pairing(
    state: &GatewayState,
    request_id: &str,
) -> Result<Option<(String, PairedDeviceRedacted)>, ErrorShape> {
    let request_id = request_id.trim();
    let (mut pending, mut paired) = load_device_pairing_state(state);
    let Some(req) = pending.remove(request_id) else {
        return Ok(None);
    };
    let now = now_ms();
    let existing = paired.get(&req.device_id).cloned();
    let normalized_scopes = req
        .scopes
        .clone()
        .map(|v| normalize_scopes(&v))
        .filter(|v| !v.is_empty());
    let mut tokens = existing
        .as_ref()
        .and_then(|d| d.tokens.clone())
        .unwrap_or_default();
    if let Some(role) = req
        .role
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let scopes = normalized_scopes.clone().unwrap_or_default();
        let existing_token = tokens.get(role).cloned();
        tokens.insert(
            role.to_string(),
            DeviceAuthToken {
                token: new_pairing_token(),
                role: role.to_string(),
                scopes,
                created_at_ms: existing_token
                    .as_ref()
                    .map(|t| t.created_at_ms)
                    .unwrap_or(now),
                rotated_at_ms: Some(now),
                revoked_at_ms: None,
                last_used_at_ms: existing_token.and_then(|t| t.last_used_at_ms),
            },
        );
    }
    let device = PairedDevice {
        device_id: req.device_id.clone(),
        public_key: req.public_key.clone(),
        display_name: req.display_name.clone(),
        platform: req.platform.clone(),
        client_id: req.client_id.clone(),
        client_mode: req.client_mode.clone(),
        role: req.role.clone(),
        roles: req.roles.clone(),
        scopes: normalized_scopes,
        remote_ip: req.remote_ip.clone(),
        tokens: if tokens.is_empty() {
            None
        } else {
            Some(tokens)
        },
        created_at_ms: existing.as_ref().map(|d| d.created_at_ms).unwrap_or(now),
        approved_at_ms: now,
    };
    paired.insert(device.device_id.clone(), device.clone());
    persist_device_pairing_state(state, &pending, &paired)?;
    let redacted = PairedDeviceRedacted {
        device_id: device.device_id,
        public_key: device.public_key,
        display_name: device.display_name,
        platform: device.platform,
        client_id: device.client_id,
        client_mode: device.client_mode,
        role: device.role,
        roles: device.roles,
        scopes: device.scopes,
        remote_ip: device.remote_ip,
        tokens: summarize_device_tokens(device.tokens.as_ref()),
        created_at_ms: device.created_at_ms,
        approved_at_ms: device.approved_at_ms,
    };
    Ok(Some((request_id.to_string(), redacted)))
}

fn reject_device_pairing(
    state: &GatewayState,
    request_id: &str,
) -> Result<Option<(String, String)>, ErrorShape> {
    let request_id = request_id.trim();
    let (mut pending, paired) = load_device_pairing_state(state);
    let Some(req) = pending.remove(request_id) else {
        return Ok(None);
    };
    persist_device_pairing_state(state, &pending, &paired)?;
    Ok(Some((request_id.to_string(), req.device_id)))
}

fn rotate_device_token(
    state: &GatewayState,
    device_id: &str,
    role: &str,
    scopes: Option<Vec<String>>,
) -> Result<Option<DeviceAuthToken>, ErrorShape> {
    let device_id = device_id.trim();
    let Some(role) = normalize_role(role) else {
        return Ok(None);
    };
    if device_id.is_empty() {
        return Ok(None);
    }
    let (pending, mut paired) = load_device_pairing_state(state);
    let Some(mut device) = paired.get(device_id).cloned() else {
        return Ok(None);
    };
    let now = now_ms();
    let mut tokens = device.tokens.clone().unwrap_or_default();
    let existing = tokens.get(&role).cloned();
    let scopes_provided = scopes.is_some();
    let scopes = scopes.unwrap_or_else(|| {
        existing
            .as_ref()
            .map(|t| t.scopes.clone())
            .unwrap_or_default()
    });
    let scopes = normalize_scopes(&scopes);
    if scopes_provided {
        device.scopes = if scopes.is_empty() { None } else { Some(scopes.clone()) };
    }
    let next = DeviceAuthToken {
        token: new_pairing_token(),
        role: role.clone(),
        scopes,
        created_at_ms: existing.as_ref().map(|t| t.created_at_ms).unwrap_or(now),
        rotated_at_ms: Some(now),
        revoked_at_ms: None,
        last_used_at_ms: existing.and_then(|t| t.last_used_at_ms),
    };
    tokens.insert(role.clone(), next.clone());
    device.tokens = Some(tokens);
    paired.insert(device_id.to_string(), device);
    persist_device_pairing_state(state, &pending, &paired)?;
    Ok(Some(next))
}

fn revoke_device_token(
    state: &GatewayState,
    device_id: &str,
    role: &str,
) -> Result<Option<DeviceAuthToken>, ErrorShape> {
    let device_id = device_id.trim();
    let role = role.trim();
    if device_id.is_empty() || role.is_empty() {
        return Ok(None);
    }
    let (pending, mut paired) = load_device_pairing_state(state);
    let Some(mut device) = paired.get(device_id).cloned() else {
        return Ok(None);
    };
    let mut tokens = device.tokens.clone().unwrap_or_default();
    let Some(existing) = tokens.get(role).cloned() else {
        return Ok(None);
    };
    let next = DeviceAuthToken {
        revoked_at_ms: Some(now_ms()),
        ..existing
    };
    tokens.insert(role.to_string(), next.clone());
    device.tokens = Some(tokens);
    paired.insert(device_id.to_string(), device);
    persist_device_pairing_state(state, &pending, &paired)?;
    Ok(Some(next))
}

async fn handle_exec_approvals_get() -> serde_json::Value {
    crate::openclaw_exec_approvals::exec_approvals_get_payload()
}

async fn handle_exec_approvals_set(
    file: &serde_json::Value,
    base_hash: Option<&str>,
) -> Result<serde_json::Value, ErrorShape> {
    let path = crate::openclaw_exec_approvals::resolve_exec_approvals_path();
    let current_raw =
        std::fs::read_to_string(&path).unwrap_or_else(|_| json!({ "version": 1 }).to_string());
    let current_hash = sha256_hex(&current_raw);

    if let Some(expected) = base_hash {
        let expected_trimmed = expected.trim();
        if !expected_trimmed.is_empty() && expected_trimmed != current_hash {
            return Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                "exec approvals baseHash mismatch",
            )
            .with_details(json!({"expected": expected_trimmed, "actual": current_hash})));
        }
    }

    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Err(ErrorShape::new(
                error_codes::UNAVAILABLE,
                format!("failed to create exec approvals dir: {}", e),
            ));
        }
    }

    // Write pretty JSON so humans can edit it.
    let raw = serde_json::to_string_pretty(file).unwrap_or_else(|_| file.to_string());
    if let Err(e) = std::fs::write(&path, raw) {
        return Err(ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("failed to write exec approvals: {}", e),
        ));
    }
    Ok(json!({ "ok": true }))
}

fn is_safe_agent_filename(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.contains('/') || name.contains('\\') {
        return false;
    }
    if name == "." || name == ".." || name.contains("..") {
        return false;
    }
    true
}

async fn handle_agents_files_list(agent_id: &str) -> serde_json::Value {
    let workspace = resolve_agent_workspace_dir(agent_id);
    let mut files: Vec<serde_json::Value> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&workspace) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if !path.is_file() {
                continue;
            }
            let meta = entry.metadata().ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let updated_at_ms = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64);
            files.push(json!({
                "name": name,
                "path": path.to_string_lossy(),
                "missing": false,
                "size": size,
                "updatedAtMs": updated_at_ms,
            }));
        }
    }

    json!({
        "agentId": agent_id,
        "workspace": workspace.to_string_lossy(),
        "files": files,
    })
}

async fn handle_agents_files_get(
    agent_id: &str,
    name: &str,
) -> Result<serde_json::Value, ErrorShape> {
    if !is_safe_agent_filename(name) {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "invalid file name",
        ));
    }
    let workspace = resolve_agent_workspace_dir(agent_id);
    let path = workspace.join(name);
    let missing = !path.exists();
    let content = if missing {
        None
    } else {
        std::fs::read_to_string(&path).ok()
    };
    let meta = std::fs::metadata(&path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let updated_at_ms = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);

    Ok(json!({
        "agentId": agent_id,
        "workspace": workspace.to_string_lossy(),
        "file": {
            "name": name,
            "path": path.to_string_lossy(),
            "missing": missing,
            "size": size,
            "updatedAtMs": updated_at_ms,
            "content": content,
        }
    }))
}

async fn handle_agents_files_set(
    agent_id: &str,
    name: &str,
    content: &str,
) -> Result<serde_json::Value, ErrorShape> {
    if !is_safe_agent_filename(name) {
        return Err(ErrorShape::new(
            error_codes::INVALID_REQUEST,
            "invalid file name",
        ));
    }
    let workspace = resolve_agent_workspace_dir(agent_id);
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        return Err(ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("failed to create agent workspace: {}", e),
        ));
    }
    let path = workspace.join(name);
    if let Err(e) = std::fs::write(&path, content) {
        return Err(ErrorShape::new(
            error_codes::UNAVAILABLE,
            format!("failed to write agent file: {}", e),
        ));
    }
    let meta = std::fs::metadata(&path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let updated_at_ms = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);

    Ok(json!({
        "ok": true,
        "agentId": agent_id,
        "workspace": workspace.to_string_lossy(),
        "file": {
            "name": name,
            "path": path.to_string_lossy(),
            "missing": false,
            "size": size,
            "updatedAtMs": updated_at_ms,
            "content": content,
        }
    }))
}

fn cap_array_by_json_bytes(items: Vec<serde_json::Value>, max_bytes: usize) -> Vec<serde_json::Value> {
    if items.is_empty() {
        return items;
    }
    let max_bytes = max_bytes.max(1);
    let mut kept_rev: Vec<serde_json::Value> = Vec::new();
    let mut total: usize = 2; // "[]"

    // Keep most-recent messages (OpenClaw behavior).
    for item in items.iter().rev() {
        let serialized_len = serde_json::to_string(item).map(|s| s.len()).unwrap_or(0);
        if serialized_len == 0 {
            continue;
        }
        // Rough accounting for commas/whitespace.
        let next = total.saturating_add(serialized_len).saturating_add(1);
        if next > max_bytes {
            break;
        }
        total = next;
        kept_rev.push(item.clone());
    }

    kept_rev.reverse();
    kept_rev
}

async fn handle_chat_history(
    state: &GatewayState,
    session_key: &str,
    limit: Option<u64>,
) -> serde_json::Value {
    let limit = limit.unwrap_or(200).min(1000) as usize;
    if let Some(store) = state.session_store() {
        let mut session = store.get_by_channel("openclaw", session_key).await.ok().flatten();
        if session.is_none() {
            let (channel_type, channel_id) = session_key_to_channel(session_key);
            session = store.get_by_channel(&channel_type, &channel_id).await.ok().flatten();
        }
        if let Some(session) = session {
            let total = session.messages.len();
            let slice = if total > limit {
                &session.messages[total - limit..]
            } else {
                &session.messages[..]
            };
            let messages: Vec<serde_json::Value> =
                slice.iter().map(drbot_message_to_openclaw).collect();
            let messages = cap_array_by_json_bytes(messages, resolve_openclaw_max_chat_history_bytes());
            return json!({
                "sessionKey": session_key,
                "sessionId": session.id.to_string(),
                "messages": messages,
                "thinkingLevel": null
            });
        }
    }
    json!({
        "sessionKey": session_key,
        "sessionId": null,
        "messages": [],
        "thinkingLevel": null
    })
}

struct OpenclawMainLaneGuard {
    state: GatewayState,
}

impl OpenclawMainLaneGuard {
    fn new(state: GatewayState) -> Self {
        state.openclaw_main_lane_enter();
        Self { state }
    }
}

impl Drop for OpenclawMainLaneGuard {
    fn drop(&mut self) {
        self.state.openclaw_main_lane_exit();
    }
}

async fn spawn_chat_run(
    ctx: ConnCtx,
    provider: Arc<dyn Provider>,
    run_id: String,
    session_key: String,
    user_msg: Message,
    mut cancel_rx: watch::Receiver<Option<String>>,
) {
    let dedupe_key = chat_send_dedupe_key(&session_key, &run_id);
    let _lane = OpenclawMainLaneGuard::new(ctx.state.clone());

    // If an abort request raced with task startup, honor it before doing any work.
    let initial_abort_reason = cancel_rx.borrow().clone();
    if let Some(stop_reason) = initial_abort_reason {
        let seq = {
            let mut run_seq = ctx.run_seq.lock().await;
            next_run_seq(&mut run_seq, &run_id)
        };
        let payload = json!({
            "runId": run_id,
            "sessionKey": session_key,
            "seq": seq,
            "state": "aborted",
            "stopReason": stop_reason,
        });
        broadcast_openclaw_event(&ctx.state, "chat", payload, None).await;
        openclaw_dedupe_put(
            &dedupe_key,
            OpenclawDedupeEntry {
                ts: now_ms(),
                ok: true,
                payload: Some(json!({"runId": run_id, "status": "aborted" })),
                error: None,
            },
        )
        .await;
        ctx.state.openclaw_unregister_chat_run(&run_id).await;
        return;
    }

    // Load session messages from store (if configured).
    let mut messages: Vec<Message> = Vec::new();
    let mut persisted_session = None;
    if let Some(store) = ctx.state.session_store() {
        // Stable operator user id.
        let user_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"openclaw-operator");
        // Prefer a legacy "openclaw" session if it exists, otherwise fall back to
        // OpenClaw's channel-style `(channel_type, channel_id)` mapping.
        let mut session = store
            .get_by_channel("openclaw", &session_key)
            .await
            .ok()
            .flatten();
        if session.is_none() {
            let (channel_type, channel_id) = session_key_to_channel(&session_key);
            session = store
                .get_or_create(user_id, &channel_type, &channel_id)
                .await
                .ok();
        }
        if let Some(s) = session {
            messages.extend(s.messages.clone());
            persisted_session = Some(s);
        }
    }

    // Inject a compact timestamp into the *agent* copy only (OpenClaw parity).
    messages.push(stamp_user_message_for_agent(&user_msg));

    // Prefix any queued system events for this session (ephemeral queue).
    let queued_system_events = ctx.state.openclaw_peek_system_events(&session_key).await;
    let has_system_events = !queued_system_events.is_empty();
    if has_system_events {
        let mut block = String::new();
        for evt in &queued_system_events {
            let ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(evt.ts_ms as i64)
                .unwrap_or_else(chrono::Utc::now)
                .to_rfc3339();
            let text = evt.text.trim();
            if text.is_empty() {
                continue;
            }
            block.push_str(&format!("System: [{}] {}\n", ts, text));
        }
        if !block.trim().is_empty() {
            // Place the system-event block directly before the new user message.
            let idx = messages.len().saturating_sub(1);
            messages.insert(idx, Message::system(block.trim_end()));
        }
    }

    // Best-effort: keep remote skills' docs up to date (no-op unless configured).
    tokio::join!(
        crate::colosseum::sync_colosseum_docs_best_effort(ctx.state.config()),
        crate::moltbook::sync_moltbook_docs_best_effort(ctx.state.config()),
        crate::agentwallet::sync_agentwallet_docs_best_effort(ctx.state.config()),
    );

    let remote = resolve_remote_skill_eligibility(&ctx.state).await;
    let skills_prompt = crate::openclaw_skills::build_workspace_skills_prompt_with_remote(
        &resolve_agent_workspace_dir("default"),
        ctx.state.config(),
        remote.as_ref(),
    );
    let system_prompt = {
        let trimmed = skills_prompt.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    };

    let model_override = resolve_openclaw_session_model_override(&ctx.state, &session_key);
    let options = ChatOptions {
        model: model_override,
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop_sequences: None,
        system_prompt,
    };

    let mut full = String::new();
    let mut final_usage: Option<Usage> = None;
    let mut stop_reason: Option<String> = None;
    let mut last_delta_sent_at_ms: u64 = 0;

    let stream_res = provider.stream(&messages, options).await;
    let mut stream = match stream_res {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "chat stream failed to start");
            let error_message = e.to_string();
            let seq = {
                let mut run_seq = ctx.run_seq.lock().await;
                next_run_seq(&mut run_seq, &run_id)
            };
            let payload = json!({
                "runId": run_id,
                "sessionKey": session_key,
                "seq": seq,
                "state": "error",
                "errorMessage": error_message,
            });
            broadcast_openclaw_event(&ctx.state, "chat", payload, None).await;
            let dedupe_payload = json!({"runId": run_id, "status": "error", "summary": error_message.clone() });
            let err_shape = ErrorShape::new(error_codes::UNAVAILABLE, error_message);
            openclaw_dedupe_put(
                &dedupe_key,
                OpenclawDedupeEntry {
                    ts: now_ms(),
                    ok: false,
                    payload: Some(dedupe_payload),
                    error: Some(err_shape),
                },
            )
            .await;
            ctx.state.openclaw_unregister_chat_run(&run_id).await;
            return;
        }
    };

    loop {
        tokio::select! {
            _ = cancel_rx.changed() => {
                let stop_reason = cancel_rx.borrow().clone();
                if let Some(stop_reason) = stop_reason {
                    let seq = {
                        let mut run_seq = ctx.run_seq.lock().await;
                        next_run_seq(&mut run_seq, &run_id)
                    };
                    let payload = json!({
                        "runId": run_id,
                        "sessionKey": session_key,
                        "seq": seq,
                        "state": "aborted",
                        "stopReason": stop_reason,
                    });
                    broadcast_openclaw_event(&ctx.state, "chat", payload, None).await;
                    openclaw_dedupe_put(
                        &dedupe_key,
                        OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: true,
                            payload: Some(json!({"runId": run_id, "status": "aborted" })),
                            error: None,
                        },
                    )
                    .await;
                    ctx.state.openclaw_unregister_chat_run(&run_id).await;
                    return;
                }
            }
            maybe_evt = stream.next() => {
                let evt = match maybe_evt {
                    Some(e) => e,
                    None => break,
                };

                match evt {
                    ProviderStreamEvent::Delta { content } => {
                        full.push_str(&content);
                        let now = now_ms();
                        if now.saturating_sub(last_delta_sent_at_ms) < OPENCLAW_CHAT_DELTA_THROTTLE_MS
                        {
                            continue;
                        }
                        last_delta_sent_at_ms = now;
                        let seq = {
                            let mut run_seq = ctx.run_seq.lock().await;
                            next_run_seq(&mut run_seq, &run_id)
                        };
                        let payload = json!({
                            "runId": run_id,
                            "sessionKey": session_key,
                            "seq": seq,
                            "state": "delta",
                            "message": {
                                "role": "assistant",
                                "content": [{ "type": "text", "text": full }],
                                "timestamp": now
                            }
                        });
                        broadcast_openclaw_event_opts(&ctx.state, "chat", payload, None, true).await;
                    }
                    ProviderStreamEvent::Stop { reason, usage } => {
                        stop_reason = Some(reason);
                        final_usage = usage;
                    }
                    ProviderStreamEvent::Error { message } => {
                        error!(error = %message, "provider stream error");
                        let seq = {
                            let mut run_seq = ctx.run_seq.lock().await;
                            next_run_seq(&mut run_seq, &run_id)
                        };
                        let payload = json!({
                            "runId": run_id,
                            "sessionKey": session_key,
                            "seq": seq,
                            "state": "error",
                            "errorMessage": message,
                        });
                        broadcast_openclaw_event(&ctx.state, "chat", payload, None).await;
                        let dedupe_payload = json!({"runId": run_id, "status": "error", "summary": message.clone() });
                        let err_shape = ErrorShape::new(error_codes::UNAVAILABLE, message);
                        openclaw_dedupe_put(
                            &dedupe_key,
                            OpenclawDedupeEntry {
                                ts: now_ms(),
                                ok: false,
                                payload: Some(dedupe_payload),
                                error: Some(err_shape),
                            },
                        )
                        .await;
                        ctx.state.openclaw_unregister_chat_run(&run_id).await;
                        return;
                    }
                    _ => {}
                }
            }
        }
    }

    let seq = {
        let mut run_seq = ctx.run_seq.lock().await;
        next_run_seq(&mut run_seq, &run_id)
    };
    let payload = json!({
        "runId": run_id,
        "sessionKey": session_key,
        "seq": seq,
        "state": "final",
        "message": {
            "role": "assistant",
            "content": [{ "type": "text", "text": full }],
            "timestamp": now_ms()
        },
        "stopReason": stop_reason,
        "usage": final_usage.as_ref().map(|u| json!({"input": u.input_tokens, "output": u.output_tokens})),
    });
    broadcast_openclaw_event(&ctx.state, "chat", payload, None).await;
    openclaw_dedupe_put(
        &dedupe_key,
        OpenclawDedupeEntry {
            ts: now_ms(),
            ok: true,
            payload: Some(json!({"runId": run_id, "status": "ok" })),
            error: None,
        },
    )
    .await;
    ctx.state.openclaw_unregister_chat_run(&run_id).await;

    // Clear system events only after we successfully produced a final response.
    if has_system_events {
        let _ = ctx
            .state
            .openclaw_drain_system_event_entries(&session_key)
            .await;
    }

    if let (Some(store), Some(mut session)) = (ctx.state.session_store(), persisted_session) {
        session.add_message(user_msg);
        session.add_message(assistant_message_from_text(&full));
        if let Some(usage) = &final_usage {
            session.add_token_usage(usage.input_tokens, usage.output_tokens);
        }
        session.update_timestamp();
        if let Err(e) = store.update(&session).await {
            warn!(error = %e, "failed to persist openclaw chat session");
        }
    }

    // Run completed; any in-flight abort handle is removed in the global registry above.
}

async fn emit_agent_event(ctx: &ConnCtx, run_id: &str, stream: &str, data: serde_json::Value) {
    let seq = {
        let mut run_seq = ctx.run_seq.lock().await;
        next_run_seq(&mut run_seq, run_id)
    };
    let data = if data.is_object() {
        data
    } else {
        json!({ "value": data })
    };
    let payload = json!({
        "runId": run_id,
        "seq": seq,
        "stream": stream,
        "ts": now_ms(),
        "data": data,
    });
    broadcast_openclaw_event(&ctx.state, "agent", payload, None).await;
}

async fn spawn_agent_run(
    ctx: ConnCtx,
    provider: Arc<dyn Provider>,
    req_id: String,
    run_id: String,
    session_key: String,
    user_msg: Message,
    timeout_ms: Option<u64>,
    extra_system_prompt: Option<String>,
    delivery_target: Option<AgentDeliveryTarget>,
) {
    let started_at = now_ms();
    let _lane = OpenclawMainLaneGuard::new(ctx.state.clone());
    emit_agent_event(
        &ctx,
        &run_id,
        "lifecycle",
        json!({ "phase": "start", "startedAt": started_at }),
    )
    .await;

    // Load session messages from store (if configured).
    let mut messages: Vec<Message> = Vec::new();
    let mut persisted_session = None;
    if let Some(store) = ctx.state.session_store() {
        // Stable operator user id.
        let user_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"openclaw-operator");
        let mut session = store
            .get_by_channel("openclaw", &session_key)
            .await
            .ok()
            .flatten();
        if session.is_none() {
            let (channel_type, channel_id) = session_key_to_channel(&session_key);
            session = store
                .get_or_create(user_id, &channel_type, &channel_id)
                .await
                .ok();
        }
        if let Some(s) = session {
            messages.extend(s.messages.clone());
            persisted_session = Some(s);
        }
    }

    let user_msg = user_msg.clone();

    // Best-effort: keep remote skills' docs up to date (no-op unless configured).
    tokio::join!(
        crate::colosseum::sync_colosseum_docs_best_effort(ctx.state.config()),
        crate::moltbook::sync_moltbook_docs_best_effort(ctx.state.config()),
        crate::agentwallet::sync_agentwallet_docs_best_effort(ctx.state.config()),
    );

    let remote = resolve_remote_skill_eligibility(&ctx.state).await;
    let skills_prompt = crate::openclaw_skills::build_workspace_skills_prompt_with_remote(
        &resolve_agent_workspace_dir("default"),
        ctx.state.config(),
        remote.as_ref(),
    );
    let mut base_system_prompt = skills_prompt.trim().to_string();
    if let Some(extra) = extra_system_prompt.as_deref() {
        let trimmed = extra.trim();
        if !trimmed.is_empty() {
            if !base_system_prompt.is_empty() {
                base_system_prompt.push_str("\n\n---\n");
            }
            base_system_prompt.push_str("Extra system prompt:\n");
            base_system_prompt.push_str(trimmed);
        }
    }

    let agent_cfg = DrbotAgentConfig {
        max_iterations: std::env::var("DRBOT_OPENCLAW_AGENT_MAX_ITERATIONS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|v| *v >= 1)
            .unwrap_or(10),
        model: resolve_openclaw_session_model_override(&ctx.state, &session_key),
        system_prompt: base_system_prompt,
        use_planning: false,
        iteration_timeout_secs: std::env::var("DRBOT_OPENCLAW_AGENT_ITERATION_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v >= 1)
            .unwrap_or(60),
    };

    let mut agent = DrbotAgent::new(provider.clone(), agent_cfg);

    // Register a conservative baseline toolset (avoid generic HTTP; prefer allowlisted API tools).
    for tool in BuiltinTools::all() {
        if tool.name() == "http" {
            continue;
        }
        agent.register_tool(tool);
    }
    agent.register_tool(Arc::new(
        crate::openclaw_agent_tools::ColosseumRequestTool::new(ctx.state.clone()),
    ));
    agent.register_tool(Arc::new(
        crate::openclaw_agent_tools::MoltbookRequestTool::new(ctx.state.clone()),
    ));
    agent.register_tool(Arc::new(crate::openclaw_agent_tools::SendTool::new(
        ctx.state.clone(),
    )));
    agent.register_tool(Arc::new(crate::openclaw_agent_tools::PollTool::new(
        ctx.state.clone(),
    )));

    // Seed prior transcript (best-effort) so the agent can keep continuity.
    for msg in &messages {
        let text = msg.text_content();
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let role = match msg.role {
            Role::System => continue,
            Role::User => DrbotAgentRole::User,
            Role::Assistant => DrbotAgentRole::Assistant,
        };
        agent.push_message(DrbotAgentMessage {
            role,
            content: text.to_string(),
            tool_calls: None,
            tool_result: None,
        });
    }

    let (agent_events_tx, mut agent_events_rx) = mpsc::channel::<DrbotAgentEvent>(128);
    let forward_ctx = ctx.clone();
    let forward_run_id = run_id.clone();
    let forwarder = tokio::spawn(async move {
        let mut iteration = 0u64;
        while let Some(evt) = agent_events_rx.recv().await {
            match evt {
                DrbotAgentEvent::ThinkingStart => {
                    iteration = iteration.saturating_add(1);
                    emit_agent_event(
                        &forward_ctx,
                        &forward_run_id,
                        "thinking",
                        json!({ "iteration": iteration }),
                    )
                    .await;
                }
                DrbotAgentEvent::ToolCall { tool, args } => {
                    emit_agent_event(
                        &forward_ctx,
                        &forward_run_id,
                        "tool",
                        json!({ "phase": "call", "tool": tool, "args": args }),
                    )
                    .await;
                }
                DrbotAgentEvent::ToolResult {
                    tool,
                    result,
                    is_error,
                } => {
                    let trimmed = result.trim();
                    let clipped = if trimmed.len() > 20_000 {
                        format!("{}…", &trimmed[..20_000])
                    } else {
                        trimmed.to_string()
                    };
                    emit_agent_event(
                        &forward_ctx,
                        &forward_run_id,
                        "tool",
                        json!({ "phase": "result", "tool": tool, "isError": is_error, "result": clipped }),
                    )
                    .await;
                }
                DrbotAgentEvent::Output { content } => {
                    let trimmed = content.trim();
                    if !trimmed.is_empty() {
                        emit_agent_event(
                            &forward_ctx,
                            &forward_run_id,
                            "assistant",
                            json!({ "text": trimmed }),
                        )
                        .await;
                    }
                }
                DrbotAgentEvent::Complete { iterations } => {
                    emit_agent_event(
                        &forward_ctx,
                        &forward_run_id,
                        "lifecycle",
                        json!({ "phase": "complete", "iterations": iterations }),
                    )
                    .await;
                }
                DrbotAgentEvent::Error { message } => {
                    emit_agent_event(
                        &forward_ctx,
                        &forward_run_id,
                        "lifecycle",
                        json!({ "phase": "error", "error": message }),
                    )
                    .await;
                }
                DrbotAgentEvent::Thought { .. } => {
                    // Intentionally suppressed to avoid leaking chain-of-thought style content.
                }
            }
        }
    });

    // Inject timestamp for the agent runtime only (do not mutate/persist the transcript message).
    let user_text_raw = user_msg.text_content();
    let user_text = openclaw_inject_timestamp_prefix(&user_text_raw);
    let queued_system_events = ctx.state.openclaw_peek_system_events(&session_key).await;
    let has_system_events = !queued_system_events.is_empty();
    let user_input = if has_system_events {
        let mut block = String::new();
        for evt in &queued_system_events {
            let ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(evt.ts_ms as i64)
                .unwrap_or_else(chrono::Utc::now)
                .to_rfc3339();
            let text = evt.text.trim();
            if text.is_empty() {
                continue;
            }
            block.push_str(&format!("System: [{}] {}\n", ts, text));
        }
        format!("{}\n{}", block.trim_end(), user_text.trim())
    } else {
        user_text
    };
    let run_fut = agent.run_with_events(user_input.as_str(), agent_events_tx);
    let res = if let Some(ms) = timeout_ms.filter(|v| *v > 0) {
        tokio::time::timeout(Duration::from_millis(ms), run_fut)
            .await
            .map_err(|_| "timeout".to_string())
            .and_then(|r| r.map_err(|e| e.to_string()))
    } else {
        run_fut.await.map_err(|e| e.to_string())
    };

    // Ensure forwarder drains any final events.
    let _ = forwarder.await;

    let full = match res {
        Ok(v) => v,
        Err(e) => {
            let ended_at = now_ms();
            let _ = finish_agent_run(&run_id, "error", Some(e.clone())).await;
            emit_agent_event(
                &ctx,
                &run_id,
                "lifecycle",
                json!({ "phase": "error", "startedAt": started_at, "endedAt": ended_at, "error": e }),
            )
            .await;
            let payload = json!({
                "runId": run_id,
                "status": "error",
                "summary": e,
            });
            let frame = GatewayFrame::Res(ResponseFrame {
                id: req_id,
                ok: false,
                payload: Some(payload),
                error: Some(ErrorShape::new(error_codes::UNAVAILABLE, "agent failed")),
            });
            send_frame(&ctx.tx, &ctx.queued_bytes, &ctx.closing, &frame).await;
            return;
        }
    };

    // Clear system events only after a successful agent run.
    if has_system_events {
        let _ = ctx
            .state
            .openclaw_drain_system_event_entries(&session_key)
            .await;
    }

    let final_usage: Option<Usage> = None;
    let stop_reason: Option<String> = None;

    let ended_at = now_ms();
    let _ = finish_agent_run(&run_id, "ok", None).await;
    emit_agent_event(
        &ctx,
        &run_id,
        "lifecycle",
        json!({ "phase": "end", "startedAt": started_at, "endedAt": ended_at }),
    )
    .await;

    if let (Some(store), Some(mut session)) = (ctx.state.session_store(), persisted_session) {
        session.add_message(user_msg);
        session.add_message(assistant_message_from_text(&full));
        if let Some(usage) = &final_usage {
            session.add_token_usage(usage.input_tokens, usage.output_tokens);
        }
        session.update_timestamp();
        if let Err(e) = store.update(&session).await {
            warn!(error = %e, "failed to persist openclaw agent session");
        }
    }

    // Optional outbound delivery of the agent output (OpenClaw `deliver`).
    let mut delivery_report: Option<serde_json::Value> = None;
    if let Some(target) = delivery_target.as_ref() {
        let mut ok = false;
        let mut err_msg: Option<String> = None;
        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_SEND_WRITE")
            .ok()
            .as_deref()
            == Some("1");
        if !allow_write_by_env {
            let approval = ExecApprovalRequestPayload {
                command: format!("send {} {}", target.channel, target.to),
                cwd: None,
                host: Some("channels".to_string()),
                security: Some("channel-send".to_string()),
                ask: Some(format!(
                    "Allow sending an outbound message via {} to {}?",
                    target.channel, target.to
                )),
                agent_id: Some("default".to_string()),
                resolved_path: None,
                session_key: Some(session_key.clone()),
            };
            if let Err(e) = crate::openclaw_exec_approvals::ensure_tool_write_allowed(
                &ctx.state,
                "send",
                approval,
                120_000,
            )
            .await
            {
                err_msg = Some(format!("{}: {}", e.code, e.message));
            }
        }
        if err_msg.is_none() {
            let outgoing = OutgoingMessage::text(full.clone());
            match ctx
                .state
                .channel_manager()
                .send(&target.channel, &target.to, outgoing)
                .await
            {
                Ok(()) => ok = true,
                Err(e) => err_msg = Some(e.to_string()),
            }
        }

        delivery_report = Some(json!({
            "requested": true,
            "ok": ok,
            "channel": target.channel.as_str(),
            "to": target.to.as_str(),
            "error": err_msg,
        }));
    }

    let payload = json!({
        "runId": run_id,
        "status": "ok",
        "summary": "completed",
        "delivery": delivery_report,
        "result": {
            "text": full,
            "stopReason": stop_reason,
            "usage": final_usage.as_ref().map(|u| json!({"input": u.input_tokens, "output": u.output_tokens})),
        }
    });
    let frame = GatewayFrame::Res(ResponseFrame {
        id: req_id,
        ok: true,
        payload: Some(payload),
        error: None,
    });
    send_frame(&ctx.tx, &ctx.queued_bytes, &ctx.closing, &frame).await;
}

// ---------------------------------------------------------------------------
// Cron (OpenClaw v3)
// ---------------------------------------------------------------------------

const CRON_STUCK_RUN_MS: u64 = 2 * 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum CronSchedule {
    At {
        #[serde(rename = "atMs")]
        at_ms: u64,
    },
    Every {
        #[serde(rename = "everyMs")]
        every_ms: u64,
        #[serde(rename = "anchorMs", default, skip_serializing_if = "Option::is_none")]
        anchor_ms: Option<u64>,
    },
    Cron {
        expr: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tz: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum CronPayload {
    SystemEvent {
        text: String,
    },
    AgentTurn {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thinking: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(rename = "timeoutSeconds")]
        timeout_seconds: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deliver: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(rename = "bestEffortDeliver")]
        best_effort_deliver: Option<bool>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum CronPayloadPatch {
    SystemEvent {
        #[serde(default)]
        text: Option<String>,
    },
    AgentTurn {
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        thinking: Option<String>,
        #[serde(rename = "timeoutSeconds", default)]
        timeout_seconds: Option<u64>,
        #[serde(default)]
        deliver: Option<bool>,
        #[serde(default)]
        channel: Option<String>,
        #[serde(default)]
        to: Option<String>,
        #[serde(rename = "bestEffortDeliver", default)]
        best_effort_deliver: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CronSessionTarget {
    Main,
    Isolated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CronWakeMode {
    #[serde(rename = "next-heartbeat")]
    NextHeartbeat,
    #[serde(rename = "now")]
    Now,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum CronPostToMainMode {
    Summary,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum CronRunStatus {
    Ok,
    Error,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CronJobState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    next_run_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    running_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_run_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_status: Option<CronRunStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CronIsolation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    post_to_main_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    post_to_main_mode: Option<CronPostToMainMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    post_to_main_max_chars: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CronJob {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    agent_id: Option<String>,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    delete_after_run: Option<bool>,
    created_at_ms: u64,
    updated_at_ms: u64,
    schedule: CronSchedule,
    session_target: CronSessionTarget,
    wake_mode: CronWakeMode,
    payload: CronPayload,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    isolation: Option<CronIsolation>,
    #[serde(default)]
    state: CronJobState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CronStoreFile {
    version: u32,
    #[serde(default)]
    jobs: Vec<CronJob>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CronListParams {
    #[serde(default)]
    include_disabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CronAddParams {
    name: String,
    #[serde(default)]
    agent_id: Option<Option<String>>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    delete_after_run: Option<bool>,
    schedule: CronSchedule,
    session_target: CronSessionTarget,
    wake_mode: CronWakeMode,
    payload: CronPayload,
    #[serde(default)]
    isolation: Option<CronIsolation>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CronJobPatch {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    agent_id: Option<Option<String>>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    delete_after_run: Option<bool>,
    #[serde(default)]
    schedule: Option<CronSchedule>,
    #[serde(default)]
    session_target: Option<CronSessionTarget>,
    #[serde(default)]
    wake_mode: Option<CronWakeMode>,
    #[serde(default)]
    payload: Option<CronPayloadPatch>,
    #[serde(default)]
    isolation: Option<CronIsolation>,
    #[serde(default)]
    state: Option<CronJobState>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CronUpdateParams {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    job_id: Option<String>,
    patch: CronJobPatch,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CronRemoveParams {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    job_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CronRunParams {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    job_id: Option<String>,
    #[serde(default)]
    mode: Option<String>, // "due" | "force"
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CronRunsParams {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    job_id: Option<String>,
    #[serde(default)]
    limit: Option<u64>,
}

fn resolve_openclaw_cron_store_path(state: &GatewayState) -> PathBuf {
    resolve_openclaw_state_dir(state)
        .map(|dir| dir.join("cron").join("jobs.json"))
        .unwrap_or_else(|| PathBuf::from("cron").join("jobs.json"))
}

fn load_cron_store(store_path: &PathBuf) -> CronStoreFile {
    read_json_file::<CronStoreFile>(store_path).unwrap_or(CronStoreFile {
        version: 1,
        jobs: Vec::new(),
    })
}

fn resolve_cron_run_log_path(store_path: &PathBuf, job_id: &str) -> PathBuf {
    let dir = store_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    dir.join("runs").join(format!("{}.jsonl", job_id))
}

fn append_cron_run_log(entry_path: &PathBuf, entry: &serde_json::Value) -> std::io::Result<()> {
    use std::io::Write;

    const MAX_BYTES: u64 = 2_000_000;
    const KEEP_LINES: usize = 2_000;

    if let Some(parent) = entry_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Append JSONL line.
    let mut line = serde_json::to_string(entry).unwrap_or_else(|_| "{}".to_string());
    line.push('\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(entry_path)?
        .write_all(line.as_bytes())?;

    // Prune if needed.
    let size = std::fs::metadata(entry_path).map(|m| m.len()).unwrap_or(0);
    if size <= MAX_BYTES {
        return Ok(());
    }

    let raw = std::fs::read_to_string(entry_path).unwrap_or_default();
    let mut lines: Vec<&str> = raw
        .split('\n')
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() > KEEP_LINES {
        lines = lines.split_off(lines.len().saturating_sub(KEEP_LINES));
    }
    let tmp = entry_path.with_extension(format!(
        "{}.tmp",
        Uuid::new_v4().to_string().replace('-', "")
    ));
    std::fs::write(&tmp, format!("{}\n", lines.join("\n")))?;
    std::fs::rename(&tmp, entry_path)?;
    Ok(())
}

fn read_cron_run_log_entries(
    entry_path: &PathBuf,
    job_id: &str,
    limit: usize,
) -> Vec<serde_json::Value> {
    let limit = limit.clamp(1, 5000);
    let raw = std::fs::read_to_string(entry_path).unwrap_or_default();
    if raw.trim().is_empty() {
        return Vec::new();
    }

    let mut parsed: Vec<serde_json::Value> = Vec::new();
    let lines: Vec<&str> = raw.split('\n').collect();
    for i in (0..lines.len()).rev() {
        if parsed.len() >= limit {
            break;
        }
        let line = lines[i].trim();
        if line.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(o) = obj.as_object() else {
            continue;
        };
        if o.get("action").and_then(|v| v.as_str()) != Some("finished") {
            continue;
        }
        if o.get("jobId").and_then(|v| v.as_str()) != Some(job_id) {
            continue;
        }
        if o.get("ts").and_then(|v| v.as_u64()).is_none() {
            continue;
        }
        parsed.push(obj);
    }
    parsed.reverse();
    parsed
}

fn assert_supported_job_spec(job: &CronJob) -> Result<(), ErrorShape> {
    match job.session_target {
        CronSessionTarget::Main => match job.payload {
            CronPayload::SystemEvent { .. } => Ok(()),
            _ => Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                r#"main cron jobs require payload.kind="systemEvent""#,
            )),
        },
        CronSessionTarget::Isolated => match job.payload {
            CronPayload::AgentTurn { .. } => Ok(()),
            _ => Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                r#"isolated cron jobs require payload.kind="agentTurn""#,
            )),
        },
    }
}

fn compute_next_run_at_ms(schedule: &CronSchedule, now_ms: u64) -> Option<u64> {
    match schedule {
        CronSchedule::At { at_ms } => {
            if *at_ms > now_ms {
                Some(*at_ms)
            } else {
                None
            }
        }
        CronSchedule::Every {
            every_ms,
            anchor_ms,
        } => {
            let every = (*every_ms).max(1);
            let anchor = anchor_ms.unwrap_or(now_ms);
            if now_ms < anchor {
                return Some(anchor);
            }
            let elapsed = now_ms.saturating_sub(anchor);
            let steps = ((elapsed.saturating_add(every.saturating_sub(1))) / every).max(1);
            Some(anchor.saturating_add(steps.saturating_mul(every)))
        }
        CronSchedule::Cron { expr, tz } => {
            let expr = expr.trim();
            if expr.is_empty() {
                return None;
            }
            let parsed = drbot_cron::CronExpression::parse(expr).ok()?;

            let tz_parsed = tz
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .and_then(|s| s.parse::<chrono_tz::Tz>().ok());

            let start_utc = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now_ms as i64)?
                + chrono::Duration::minutes(1);
            let mut candidate_utc = start_utc
                .with_second(0)
                .unwrap_or(start_utc)
                .with_nanosecond(0)
                .unwrap_or(start_utc);

            // Search for up to 4 years, minute-by-minute.
            let max_iterations = 365 * 4 * 24 * 60;
            for _ in 0..max_iterations {
                let matches = match tz_parsed {
                    Some(tz) => {
                        let dt = candidate_utc.with_timezone(&tz);
                        let minute = dt.minute();
                        let hour = dt.hour();
                        let dom = dt.day();
                        let month = dt.month();
                        let dow = dt.weekday().num_days_from_sunday();
                        parsed.minutes.contains(&minute)
                            && parsed.hours.contains(&hour)
                            && parsed.days_of_month.contains(&dom)
                            && parsed.months.contains(&month)
                            && parsed.days_of_week.contains(&dow)
                    }
                    None => {
                        let dt = candidate_utc.with_timezone(&chrono::Local);
                        let minute = dt.minute();
                        let hour = dt.hour();
                        let dom = dt.day();
                        let month = dt.month();
                        let dow = dt.weekday().num_days_from_sunday();
                        parsed.minutes.contains(&minute)
                            && parsed.hours.contains(&hour)
                            && parsed.days_of_month.contains(&dom)
                            && parsed.months.contains(&month)
                            && parsed.days_of_week.contains(&dow)
                    }
                };

                if matches {
                    return Some(candidate_utc.timestamp_millis().max(0) as u64);
                }
                candidate_utc = candidate_utc + chrono::Duration::minutes(1);
            }
            None
        }
    }
}

fn compute_job_next_run_at_ms(job: &CronJob, now_ms: u64) -> Option<u64> {
    if !job.enabled {
        return None;
    }

    match &job.schedule {
        CronSchedule::At { at_ms } => {
            // One-shot jobs stay due until they successfully finish.
            if job.state.last_status == Some(CronRunStatus::Ok)
                && job.state.last_run_at_ms.is_some()
            {
                None
            } else {
                Some(*at_ms)
            }
        }
        _ => compute_next_run_at_ms(&job.schedule, now_ms),
    }
}

fn next_wake_at_ms(jobs: &[CronJob]) -> Option<u64> {
    let mut next: Option<u64> = None;
    for job in jobs {
        if !job.enabled {
            continue;
        }
        let Some(ts) = job.state.next_run_at_ms else {
            continue;
        };
        next = Some(match next {
            Some(min) => min.min(ts),
            None => ts,
        });
    }
    next
}

fn merge_cron_payload(
    existing: &CronPayload,
    patch: CronPayloadPatch,
) -> Result<CronPayload, ErrorShape> {
    match (existing, patch) {
        (CronPayload::SystemEvent { text }, CronPayloadPatch::SystemEvent { text: patch_text }) => {
            Ok(CronPayload::SystemEvent {
                text: patch_text.unwrap_or_else(|| text.clone()),
            })
        }
        (
            CronPayload::AgentTurn {
                message,
                model,
                thinking,
                timeout_seconds,
                deliver,
                channel,
                to,
                best_effort_deliver,
            },
            CronPayloadPatch::AgentTurn {
                message: patch_message,
                model: patch_model,
                thinking: patch_thinking,
                timeout_seconds: patch_timeout_seconds,
                deliver: patch_deliver,
                channel: patch_channel,
                to: patch_to,
                best_effort_deliver: patch_best_effort_deliver,
            },
        ) => Ok(CronPayload::AgentTurn {
            message: patch_message.unwrap_or_else(|| message.clone()),
            model: patch_model.or_else(|| model.clone()),
            thinking: patch_thinking.or_else(|| thinking.clone()),
            timeout_seconds: patch_timeout_seconds.or(*timeout_seconds),
            deliver: patch_deliver.or(*deliver),
            channel: patch_channel.or_else(|| channel.clone()),
            to: patch_to.or_else(|| to.clone()),
            best_effort_deliver: patch_best_effort_deliver.or(*best_effort_deliver),
        }),
        // Kind changed; require required field to be present and non-empty.
        (_, CronPayloadPatch::SystemEvent { text }) => {
            let Some(text) = text else {
                return Err(ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    r#"cron.update payload.kind="systemEvent" requires text"#,
                ));
            };
            if text.is_empty() {
                return Err(ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    r#"cron.update payload.kind="systemEvent" requires non-empty text"#,
                ));
            }
            Ok(CronPayload::SystemEvent { text })
        }
        (
            _,
            CronPayloadPatch::AgentTurn {
                message,
                model,
                thinking,
                timeout_seconds,
                deliver,
                channel,
                to,
                best_effort_deliver,
            },
        ) => {
            let Some(message) = message else {
                return Err(ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    r#"cron.update payload.kind="agentTurn" requires message"#,
                ));
            };
            if message.is_empty() {
                return Err(ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    r#"cron.update payload.kind="agentTurn" requires non-empty message"#,
                ));
            }
            Ok(CronPayload::AgentTurn {
                message,
                model,
                thinking,
                timeout_seconds,
                deliver,
                channel,
                to,
                best_effort_deliver,
            })
        }
    }
}

#[derive(Debug)]
struct OpenclawCronState {
    loaded: bool,
    store: CronStoreFile,
}

#[derive(Debug)]
struct OpenclawCronService {
    store_path: PathBuf,
    enabled: bool,
    started: std::sync::atomic::AtomicBool,
    notify: Notify,
    state: Mutex<OpenclawCronState>,
}

static OPENCLAW_CRON_SERVICES: OnceLock<Mutex<HashMap<PathBuf, Arc<OpenclawCronService>>>> =
    OnceLock::new();

fn openclaw_cron_services() -> &'static Mutex<HashMap<PathBuf, Arc<OpenclawCronService>>> {
    OPENCLAW_CRON_SERVICES.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn cron_service_for_state(state: &GatewayState) -> Arc<OpenclawCronService> {
    let store_path = resolve_openclaw_cron_store_path(state);
    let mut services = openclaw_cron_services().lock().await;
    if let Some(svc) = services.get(&store_path) {
        // Ensure background loop started (idempotent).
        svc.start_background(state.clone());
        return svc.clone();
    }

    let enabled = std::env::var("OPENCLAW_SKIP_CRON").ok().as_deref() != Some("1")
        && std::env::var("DRBOT_SKIP_CRON").ok().as_deref() != Some("1");
    let svc = Arc::new(OpenclawCronService {
        store_path: store_path.clone(),
        enabled,
        started: std::sync::atomic::AtomicBool::new(false),
        notify: Notify::new(),
        state: Mutex::new(OpenclawCronState {
            loaded: false,
            store: CronStoreFile {
                version: 1,
                jobs: Vec::new(),
            },
        }),
    });
    svc.start_background(state.clone());
    services.insert(store_path, svc.clone());
    svc
}

impl OpenclawCronService {
    fn start_background(self: &Arc<Self>, state: GatewayState) {
        if !self.enabled {
            return;
        }
        if self
            .started
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        let svc = self.clone();
        tokio::spawn(async move {
            svc.run_loop(state).await;
        });
    }

    async fn ensure_loaded_locked(&self, st: &mut OpenclawCronState) {
        if st.loaded {
            return;
        }
        st.store = load_cron_store(&self.store_path);
        st.loaded = true;

        let now = now_ms();
        recompute_next_runs(&mut st.store.jobs, now);
        let _ = write_json_atomic(&self.store_path, &st.store);
    }

    async fn persist_locked(&self, st: &OpenclawCronState) -> Result<(), ErrorShape> {
        write_json_atomic(&self.store_path, &st.store)
    }

    async fn run_loop(self: Arc<Self>, state: GatewayState) {
        loop {
            let next_at = {
                let mut st = self.state.lock().await;
                self.ensure_loaded_locked(&mut st).await;
                next_wake_at_ms(&st.store.jobs)
            };

            let Some(next_at) = next_at else {
                self.notify.notified().await;
                continue;
            };

            let delay_ms = next_at.saturating_sub(now_ms());
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {
                    let _ = self.run_due_jobs(&state).await;
                }
                _ = self.notify.notified() => {
                    continue;
                }
            }
        }
    }

    async fn status(&self) -> serde_json::Value {
        let mut st = self.state.lock().await;
        self.ensure_loaded_locked(&mut st).await;
        let next = if self.enabled {
            next_wake_at_ms(&st.store.jobs)
        } else {
            None
        };
        json!({
            "enabled": self.enabled,
            "storePath": self.store_path.to_string_lossy(),
            "jobs": st.store.jobs.len(),
            "nextWakeAtMs": next,
        })
    }

    async fn list(&self, include_disabled: bool) -> Vec<CronJob> {
        let mut st = self.state.lock().await;
        self.ensure_loaded_locked(&mut st).await;
        let now = now_ms();
        recompute_next_runs(&mut st.store.jobs, now);
        let mut jobs = st
            .store
            .jobs
            .iter()
            .cloned()
            .filter(|j| include_disabled || j.enabled)
            .collect::<Vec<_>>();
        jobs.sort_by_key(|j| j.state.next_run_at_ms.unwrap_or(0));
        jobs
    }

    async fn add(&self, state: &GatewayState, input: CronAddParams) -> Result<CronJob, ErrorShape> {
        if !self.enabled {
            warn!("cron: disabled; add called");
        }

        let now = now_ms();
        let name = input.name.trim();
        if name.is_empty() {
            return Err(ErrorShape::new(
                error_codes::INVALID_REQUEST,
                "cron job name is required",
            ));
        }
        let agent_id = input.agent_id.unwrap_or(None).and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        let description = input.description.and_then(|s| {
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        let enabled = input.enabled.unwrap_or(true);
        let job = CronJob {
            id: Uuid::new_v4().to_string(),
            agent_id,
            name: name.to_string(),
            description,
            enabled,
            delete_after_run: input.delete_after_run,
            created_at_ms: now,
            updated_at_ms: now,
            schedule: input.schedule,
            session_target: input.session_target,
            wake_mode: input.wake_mode,
            payload: input.payload,
            isolation: input.isolation,
            state: CronJobState::default(),
        };

        assert_supported_job_spec(&job)?;

        let mut st = self.state.lock().await;
        self.ensure_loaded_locked(&mut st).await;
        let mut job = job;
        job.state.next_run_at_ms = compute_job_next_run_at_ms(&job, now);
        st.store.jobs.push(job.clone());
        self.persist_locked(&st).await?;
        self.notify.notify_one();

        broadcast_openclaw_event(
            state,
            "cron",
            json!({ "jobId": job.id, "action": "added", "nextRunAtMs": job.state.next_run_at_ms }),
            None,
        )
        .await;

        Ok(job)
    }

    async fn update(
        &self,
        state: &GatewayState,
        job_id: &str,
        patch: CronJobPatch,
    ) -> Result<CronJob, ErrorShape> {
        if job_id.trim().is_empty() {
            return Err(ErrorShape::new(error_codes::INVALID_REQUEST, "missing id"));
        }
        let now = now_ms();

        let mut st = self.state.lock().await;
        self.ensure_loaded_locked(&mut st).await;
        let job = st
            .store
            .jobs
            .iter_mut()
            .find(|j| j.id == job_id)
            .ok_or_else(|| {
                ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    format!("unknown cron job id: {}", job_id),
                )
            })?;

        if let Some(name) = patch.name.as_deref() {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Err(ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    "cron job name is required",
                ));
            }
            job.name = trimmed.to_string();
        }
        if let Some(desc) = patch.description {
            let trimmed = desc.trim().to_string();
            job.description = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            };
        }
        if let Some(enabled) = patch.enabled {
            job.enabled = enabled;
        }
        if let Some(delete_after_run) = patch.delete_after_run {
            job.delete_after_run = Some(delete_after_run);
        }
        if let Some(schedule) = patch.schedule {
            job.schedule = schedule;
        }
        if let Some(session_target) = patch.session_target {
            job.session_target = session_target;
        }
        if let Some(wake_mode) = patch.wake_mode {
            job.wake_mode = wake_mode;
        }
        if let Some(payload_patch) = patch.payload {
            job.payload = merge_cron_payload(&job.payload, payload_patch)?;
        }
        if let Some(isolation) = patch.isolation {
            job.isolation = Some(isolation);
        }
        if let Some(state_patch) = patch.state {
            if let Some(v) = state_patch.next_run_at_ms {
                job.state.next_run_at_ms = Some(v);
            }
            if let Some(v) = state_patch.running_at_ms {
                job.state.running_at_ms = Some(v);
            }
            if let Some(v) = state_patch.last_run_at_ms {
                job.state.last_run_at_ms = Some(v);
            }
            if let Some(v) = state_patch.last_status {
                job.state.last_status = Some(v);
            }
            if let Some(v) = state_patch.last_error {
                job.state.last_error = Some(v);
            }
            if let Some(v) = state_patch.last_duration_ms {
                job.state.last_duration_ms = Some(v);
            }
        }
        if let Some(agent_field) = patch.agent_id {
            job.agent_id = agent_field.and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });
        }

        assert_supported_job_spec(job)?;
        job.updated_at_ms = now;

        if job.enabled {
            job.state.next_run_at_ms = compute_job_next_run_at_ms(job, now);
        } else {
            job.state.next_run_at_ms = None;
            job.state.running_at_ms = None;
        }

        let updated = job.clone();
        self.persist_locked(&st).await?;
        self.notify.notify_one();

        broadcast_openclaw_event(
            state,
            "cron",
            json!({ "jobId": job_id, "action": "updated", "nextRunAtMs": updated.state.next_run_at_ms }),
            None,
        )
        .await;

        Ok(updated)
    }

    async fn remove(
        &self,
        state: &GatewayState,
        job_id: &str,
    ) -> Result<serde_json::Value, ErrorShape> {
        if job_id.trim().is_empty() {
            return Err(ErrorShape::new(error_codes::INVALID_REQUEST, "missing id"));
        }
        let mut st = self.state.lock().await;
        self.ensure_loaded_locked(&mut st).await;
        let before = st.store.jobs.len();
        st.store.jobs.retain(|j| j.id != job_id);
        let removed = st.store.jobs.len() != before;
        self.persist_locked(&st).await?;
        self.notify.notify_one();
        if removed {
            broadcast_openclaw_event(
                state,
                "cron",
                json!({ "jobId": job_id, "action": "removed" }),
                None,
            )
            .await;
        }
        Ok(json!({ "ok": true, "removed": removed }))
    }

    async fn run(
        &self,
        state: &GatewayState,
        job_id: &str,
        mode: Option<&str>,
    ) -> Result<serde_json::Value, ErrorShape> {
        if job_id.trim().is_empty() {
            return Err(ErrorShape::new(error_codes::INVALID_REQUEST, "missing id"));
        }
        let forced = mode == Some("force");
        let now = now_ms();

        let mut st = self.state.lock().await;
        self.ensure_loaded_locked(&mut st).await;
        let idx = st
            .store
            .jobs
            .iter()
            .position(|j| j.id == job_id)
            .ok_or_else(|| {
                ErrorShape::new(
                    error_codes::INVALID_REQUEST,
                    format!("unknown cron job id: {}", job_id),
                )
            })?;

        let due = {
            let job = &st.store.jobs[idx];
            if forced {
                true
            } else {
                job.enabled
                    && job.state.next_run_at_ms.is_some()
                    && now >= job.state.next_run_at_ms.unwrap_or(u64::MAX)
            }
        };
        if !due {
            return Ok(json!({ "ok": true, "ran": false, "reason": "not-due" }));
        }

        // Execute inside lock to keep store consistent (matches OpenClaw semantics).
        let job_id = st.store.jobs[idx].id.clone();
        execute_cron_job(state, &self.store_path, &mut st.store, &job_id, forced).await;
        // Persist store changes.
        self.persist_locked(&st).await?;
        self.notify.notify_one();
        Ok(json!({ "ok": true, "ran": true }))
    }

    async fn run_due_jobs(&self, state: &GatewayState) -> Result<(), ErrorShape> {
        let now = now_ms();
        let mut st = self.state.lock().await;
        self.ensure_loaded_locked(&mut st).await;
        recompute_next_runs(&mut st.store.jobs, now);

        // Collect due job IDs first (so we can mutate safely during execution).
        let due_ids: Vec<String> = st
            .store
            .jobs
            .iter()
            .filter(|j| {
                j.enabled
                    && j.state.running_at_ms.is_none()
                    && j.state.next_run_at_ms.is_some()
                    && now >= j.state.next_run_at_ms.unwrap_or(u64::MAX)
            })
            .map(|j| j.id.clone())
            .collect();

        for id in due_ids {
            execute_cron_job(state, &self.store_path, &mut st.store, &id, false).await;
        }

        self.persist_locked(&st).await?;
        Ok(())
    }
}

fn recompute_next_runs(jobs: &mut [CronJob], now: u64) {
    for job in jobs {
        if !job.enabled {
            job.state.next_run_at_ms = None;
            job.state.running_at_ms = None;
            continue;
        }
        if let Some(running_at) = job.state.running_at_ms {
            if now.saturating_sub(running_at) > CRON_STUCK_RUN_MS {
                warn!(job_id = %job.id, running_at_ms = running_at, "cron: clearing stuck running marker");
                job.state.running_at_ms = None;
            }
        }
        job.state.next_run_at_ms = compute_job_next_run_at_ms(job, now);
    }
}

async fn enqueue_openclaw_system_event(state: &GatewayState, text: &str) {
    state
        .openclaw_enqueue_system_event("main", text, None)
        .await;
}

async fn execute_cron_job(
    state: &GatewayState,
    store_path: &PathBuf,
    store: &mut CronStoreFile,
    job_id: &str,
    forced: bool,
) {
    let started_at = now_ms();

    // Mark running + emit started.
    let Some(snapshot) = store.jobs.iter_mut().find(|j| j.id == job_id).map(|j| {
        j.state.running_at_ms = Some(started_at);
        j.state.last_error = None;
        j.clone()
    }) else {
        return;
    };
    broadcast_openclaw_event(
        state,
        "cron",
        json!({ "jobId": job_id, "action": "started", "runAtMs": started_at }),
        None,
    )
    .await;

    // Execute (without holding a mutable borrow into the store across awaits).
    let mut status = CronRunStatus::Ok;
    let mut err: Option<String> = None;
    let mut summary: Option<String> = None;
    let mut output_text: Option<String> = None;

    match snapshot.session_target {
        CronSessionTarget::Main => {
            let text = match snapshot.payload {
                CronPayload::SystemEvent { text } => text.trim().to_string(),
                _ => "".to_string(),
            };
            if text.trim().is_empty() {
                status = CronRunStatus::Skipped;
                err = Some("main job requires non-empty systemEvent text".to_string());
            } else {
                enqueue_openclaw_system_event(state, &text).await;
                summary = Some(text.clone());

                if matches!(snapshot.wake_mode, CronWakeMode::Now) {
                    let reason = format!("cron:{}", job_id);
                    let max_wait_ms = 2 * 60_000;
                    let wait_started_at = now_ms();
                    let mut hb_res;
                    loop {
                        hb_res = crate::openclaw_heartbeat::run_heartbeat_once(
                            state,
                            Some(reason.clone()),
                        )
                        .await;
                        let requests_in_flight = matches!(
                            &hb_res,
                            crate::openclaw_heartbeat::HeartbeatRunResult::Skipped { reason }
                                if reason == "requests-in-flight"
                        );
                        if requests_in_flight {
                            // Keep waiting for the main lane to become idle.
                            if now_ms().saturating_sub(wait_started_at) > max_wait_ms {
                                hb_res = crate::openclaw_heartbeat::HeartbeatRunResult::Skipped {
                                    reason: "timeout waiting for main lane to become idle"
                                        .to_string(),
                                };
                                break;
                            }
                            tokio::time::sleep(Duration::from_millis(250)).await;
                            continue;
                        }
                        break;
                    }

                    match hb_res {
                        crate::openclaw_heartbeat::HeartbeatRunResult::Ran { .. } => {}
                        crate::openclaw_heartbeat::HeartbeatRunResult::Skipped { reason } => {
                            status = CronRunStatus::Skipped;
                            err = Some(reason);
                        }
                        crate::openclaw_heartbeat::HeartbeatRunResult::Failed { reason } => {
                            status = CronRunStatus::Error;
                            err = Some(reason);
                        }
                    }
                } else {
                    crate::openclaw_heartbeat::request_heartbeat_now(
                        state,
                        Some(format!("cron:{}", job_id)),
                    )
                    .await;
                }
            }
        }
        CronSessionTarget::Isolated => {
            let (message, model, timeout_seconds) = match snapshot.payload {
                CronPayload::AgentTurn {
                    message,
                    model,
                    timeout_seconds,
                    ..
                } => (message, model, timeout_seconds),
                _ => {
                    status = CronRunStatus::Skipped;
                    err = Some("isolated job requires payload.kind=agentTurn".to_string());
                    (String::new(), None, None)
                }
            };

            if status != CronRunStatus::Skipped {
                let provider = match state.provider().cloned() {
                    Some(p) => Some(p),
                    None => {
                        status = CronRunStatus::Error;
                        let msg = "provider not configured".to_string();
                        err = Some(msg.clone());
                        summary = Some(msg);
                        None
                    }
                };

                if let Some(provider) = provider {
                    let run_fut = async {
                        let mut messages: Vec<Message> = Vec::new();
                        let mut persisted_session = None;
                        if let Some(store) = state.session_store() {
                            let user_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"openclaw-operator");
                            let session_key = format!("cron:{}", job_id);
                            if let Ok(s) =
                                store.get_or_create(user_id, "openclaw", &session_key).await
                            {
                                messages.extend(s.messages.clone());
                                persisted_session = Some(s);
                            }
                        }

                        let user_msg = Message::user(message.clone());
                        messages.push(user_msg.clone());
                        let options = ChatOptions {
                            model,
                            max_tokens: None,
                            temperature: None,
                            top_p: None,
                            stop_sequences: None,
                            system_prompt: None,
                        };

                        let mut full = String::new();
                        let mut final_usage: Option<Usage> = None;
                        let mut stop_reason: Option<String> = None;

                        let mut stream = provider
                            .stream(&messages, options)
                            .await
                            .map_err(|e| e.to_string())?;
                        while let Some(evt) = stream.next().await {
                            match evt {
                                ProviderStreamEvent::Delta { content } => full.push_str(&content),
                                ProviderStreamEvent::Stop { reason, usage } => {
                                    stop_reason = Some(reason);
                                    final_usage = usage;
                                }
                                ProviderStreamEvent::Error { message } => return Err(message),
                                _ => {}
                            }
                        }

                        if let (Some(store), Some(mut session)) =
                            (state.session_store(), persisted_session)
                        {
                            session.add_message(user_msg);
                            session.add_message(assistant_message_from_text(&full));
                            if let Some(usage) = &final_usage {
                                session.add_token_usage(usage.input_tokens, usage.output_tokens);
                            }
                            session.update_timestamp();
                            let _ = store.update(&session).await;
                        }

                        Ok::<_, String>((full, stop_reason, final_usage))
                    };

                    let res = if let Some(secs) = timeout_seconds {
                        tokio::time::timeout(Duration::from_secs(secs), run_fut)
                            .await
                            .map_err(|_| "timeout".to_string())
                            .and_then(|r| r)
                    } else {
                        run_fut.await
                    };

                    match res {
                        Ok((full, _stop_reason, _usage)) => {
                            output_text = Some(full.clone());
                            let picked = full
                                .lines()
                                .map(|l| l.trim())
                                .find(|l| !l.is_empty())
                                .unwrap_or("completed")
                                .to_string();
                            summary = Some(picked);
                        }
                        Err(e) => {
                            status = CronRunStatus::Error;
                            err = Some(e.clone());
                            summary = Some(e);
                        }
                    }
                }
            }
        }
    }

    // Finish.
    let ended_at = now_ms();
    let duration_ms = ended_at.saturating_sub(started_at);
    let status_str = match status {
        CronRunStatus::Ok => "ok",
        CronRunStatus::Error => "error",
        CronRunStatus::Skipped => "skipped",
    };

    let mut next_run_at_ms: Option<u64> = None;
    let mut should_delete = false;

    if let Some(j) = store.jobs.iter_mut().find(|j| j.id == job_id) {
        j.state.running_at_ms = None;
        j.state.last_run_at_ms = Some(started_at);
        j.state.last_status = Some(status.clone());
        j.state.last_duration_ms = Some(duration_ms);
        j.state.last_error = err.clone();
        j.updated_at_ms = ended_at;

        should_delete = matches!(j.schedule, CronSchedule::At { .. })
            && status == CronRunStatus::Ok
            && j.delete_after_run == Some(true);

        if !should_delete {
            if matches!(j.schedule, CronSchedule::At { .. }) && status == CronRunStatus::Ok {
                // One-shot job completed successfully; disable it.
                j.enabled = false;
                j.state.next_run_at_ms = None;
            } else if j.enabled {
                next_run_at_ms = compute_job_next_run_at_ms(j, ended_at);
                j.state.next_run_at_ms = next_run_at_ms;
            } else {
                j.state.next_run_at_ms = None;
            }
        }
    }

    // Emit finished event.
    {
        let mut obj = serde_json::Map::new();
        obj.insert("jobId".to_string(), json!(job_id));
        obj.insert("action".to_string(), json!("finished"));
        obj.insert("status".to_string(), json!(status_str));
        obj.insert("runAtMs".to_string(), json!(started_at));
        obj.insert("durationMs".to_string(), json!(duration_ms));
        if let Some(v) = next_run_at_ms {
            obj.insert("nextRunAtMs".to_string(), json!(v));
        }
        if let Some(e) = err.as_deref() {
            obj.insert("error".to_string(), json!(e));
        }
        if let Some(s) = summary.as_deref() {
            obj.insert("summary".to_string(), json!(s));
        }
        broadcast_openclaw_event(state, "cron", serde_json::Value::Object(obj), None).await;
    }

    // Append run log entry (JSONL).
    {
        let mut obj = serde_json::Map::new();
        obj.insert("ts".to_string(), json!(ended_at));
        obj.insert("jobId".to_string(), json!(job_id));
        obj.insert("action".to_string(), json!("finished"));
        obj.insert("status".to_string(), json!(status_str));
        obj.insert("runAtMs".to_string(), json!(started_at));
        obj.insert("durationMs".to_string(), json!(duration_ms));
        if let Some(v) = next_run_at_ms {
            obj.insert("nextRunAtMs".to_string(), json!(v));
        }
        if let Some(e) = err.as_deref() {
            obj.insert("error".to_string(), json!(e));
        }
        if let Some(s) = summary.as_deref() {
            obj.insert("summary".to_string(), json!(s));
        }
        let log_path = resolve_cron_run_log_path(store_path, job_id);
        let _ = append_cron_run_log(&log_path, &serde_json::Value::Object(obj));
    }

    if should_delete {
        store.jobs.retain(|j| j.id != job_id);
        broadcast_openclaw_event(
            state,
            "cron",
            json!({ "jobId": job_id, "action": "removed" }),
            None,
        )
        .await;
    }

    if let CronSessionTarget::Isolated = snapshot.session_target {
        let prefix = snapshot
            .isolation
            .as_ref()
            .and_then(|i| i.post_to_main_prefix.as_deref())
            .unwrap_or("Cron")
            .trim();
        let mode = snapshot
            .isolation
            .as_ref()
            .and_then(|i| i.post_to_main_mode.clone())
            .unwrap_or(CronPostToMainMode::Summary);

        let mut body = summary
            .clone()
            .or_else(|| err.clone())
            .unwrap_or_else(|| status_str.to_string());
        if let CronPostToMainMode::Full = mode {
            let max_chars = snapshot
                .isolation
                .as_ref()
                .and_then(|i| i.post_to_main_max_chars)
                .unwrap_or(8000) as usize;
            let full = output_text.clone().unwrap_or_default().trim().to_string();
            if !full.is_empty() {
                body = if full.chars().count() > max_chars {
                    full.chars().take(max_chars).collect::<String>() + "..."
                } else {
                    full
                };
            }
        }

        let status_prefix = if status == CronRunStatus::Ok {
            prefix.to_string()
        } else {
            format!("{} ({})", prefix, status_str)
        };
        enqueue_openclaw_system_event(state, &format!("{}: {}", status_prefix, body)).await;

        if matches!(snapshot.wake_mode, CronWakeMode::Now) {
            crate::openclaw_heartbeat::request_heartbeat_now(
                state,
                Some(format!("cron:{}:post", job_id)),
            )
            .await;
        }
    }

    // Keep nextRunAtMs in sync in case schedule advanced during a long run.
    if !forced && !should_delete {
        if let Some(j) = store.jobs.iter_mut().find(|j| j.id == job_id) {
            if j.enabled {
                j.state.next_run_at_ms = compute_job_next_run_at_ms(j, now_ms());
            }
        }
    }
}

async fn handle_request_after_connect(ctx: &ConnCtx, req: RequestFrame) -> GatewayFrame {
    if let Some(frame) = authorize_openclaw_request(ctx, &req).await {
        return frame;
    }
    match req.method.as_str() {
        "health" => ok_response(
            &req.id,
            crate::openclaw_health::build_health_payload(&ctx.state).await,
        ),
        "status" => ok_response(
            &req.id,
            json!({
                "ts": now_ms(),
                "version": env!("CARGO_PKG_VERSION"),
                "peer": ctx.peer.to_string(),
            }),
        ),
        "logs.tail" => ok_response(
            &req.id,
            {
                let params = req.params.clone().unwrap_or_else(|| json!({}));
                let cursor = params
                    .get("cursor")
                    .and_then(|v| v.as_u64())
                    .or_else(|| {
                        params
                            .get("cursor")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.trim().parse::<u64>().ok())
                    });
                let limit = params
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .or_else(|| {
                        params
                            .get("limit")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.trim().parse::<u64>().ok())
                    })
                    .unwrap_or(400)
                    .max(1)
                    .min(5000) as usize;
                let max_bytes = params
                    .get("maxBytes")
                    .and_then(|v| v.as_u64())
                    .or_else(|| {
                        params
                            .get("maxBytes")
                            .and_then(|v| v.as_str())
                            .and_then(|s| s.trim().parse::<u64>().ok())
                    })
                    .unwrap_or(200_000)
                    .max(1)
                    .min(1_000_000) as usize;

                let result = ctx.state.openclaw_logs().tail(cursor, limit, max_bytes).await;
                json!({
                    "file": result.file,
                    "cursor": result.cursor,
                    "size": result.size,
                    "lines": result.lines,
                    "truncated": result.truncated,
                    "reset": result.reset,
                })
            },
        ),
        "models.list" => {
            let models = ctx
                .state
                .provider()
                .map(|p| {
                    p.models()
                        .into_iter()
                        .map(|m| {
                            json!({
                                "id": m.id,
                                "name": m.name,
                                "provider": m.provider,
                                "contextWindow": m.context_window,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            ok_response(&req.id, json!({ "models": models }))
        }
        "usage.status" => ok_response(&req.id, handle_usage_status(&ctx.state).await),
        "usage.cost" => {
            let raw = req.params.as_ref().and_then(|v| v.get("days"));
            let mut days = raw
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(30);
            if days == 0 {
                days = 30;
            }
            // Accept a string value too (OpenClaw UI sometimes sends text inputs).
            if let Some(s) = raw.and_then(|v| v.as_str()) {
                if let Ok(parsed) = s.trim().parse::<usize>() {
                    days = parsed.max(1);
                }
            }
            ok_response(&req.id, handle_usage_cost(&ctx.state, days).await)
        }
        "exec.approvals.get" => ok_response(&req.id, handle_exec_approvals_get().await),
        "exec.approvals.set" => {
            let file = req
                .params
                .as_ref()
                .and_then(|v| v.get("file"))
                .cloned()
                .unwrap_or_else(|| json!({ "version": 1 }));
            let base_hash = req
                .params
                .as_ref()
                .and_then(|v| v.get("baseHash"))
                .and_then(|v| v.as_str());
            match handle_exec_approvals_set(&file, base_hash).await {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "exec.approvals.node.get" => {
            let node_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("nodeId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if node_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "nodeId required",
                    None,
                );
            }
            match invoke_node_command(
                &ctx.state,
                &node_id,
                "system.execApprovals.get",
                json!({}),
                30_000,
            )
            .await
            {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "exec.approvals.node.set" => {
            let node_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("nodeId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if node_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "nodeId required",
                    None,
                );
            }
            let file = req
                .params
                .as_ref()
                .and_then(|v| v.get("file"))
                .cloned()
                .unwrap_or_else(|| json!({ "version": 1 }));
            let base_hash = req
                .params
                .as_ref()
                .and_then(|v| v.get("baseHash"))
                .and_then(|v| v.as_str());
            let params = if let Some(base_hash) = base_hash {
                json!({ "file": file, "baseHash": base_hash })
            } else {
                json!({ "file": file })
            };
            match invoke_node_command(
                &ctx.state,
                &node_id,
                "system.execApprovals.set",
                params,
                30_000,
            )
            .await
            {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "exec.approval.request" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let command = params
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if command.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "command required",
                    None,
                );
            }
            let timeout_ms = params
                .get("timeoutMs")
                .and_then(|v| v.as_u64())
                .unwrap_or(120_000)
                .max(1);
            let explicit_id = params
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if let Some(explicit_id) = explicit_id.as_deref() {
                if crate::openclaw_exec_approvals::exec_approval_snapshot(explicit_id)
                    .await
                    .is_some()
                {
                    return error_response(
                        &req.id,
                        error_codes::INVALID_REQUEST,
                        "approval id already pending",
                        None,
                    );
                }
            }

            let request = ExecApprovalRequestPayload {
                command,
                cwd: params
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                host: params
                    .get("host")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                security: params
                    .get("security")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                ask: params
                    .get("ask")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                agent_id: params
                    .get("agentId")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                resolved_path: params
                    .get("resolvedPath")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                session_key: params
                    .get("sessionKey")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            };

            let (record, rx) = crate::openclaw_exec_approvals::create_exec_approval(
                request,
                timeout_ms,
                explicit_id,
            )
            .await;

            broadcast_openclaw_event(
                &ctx.state,
                "exec.approval.requested",
                json!({
                    "id": record.id,
                    "request": record.request,
                    "createdAtMs": record.created_at_ms,
                    "expiresAtMs": record.expires_at_ms,
                }),
                None,
            )
            .await;

            let decision = match tokio::time::timeout(Duration::from_millis(timeout_ms), rx).await {
                Ok(Ok(v)) => v,
                Ok(Err(_)) => None,
                Err(_) => {
                    let _ = crate::openclaw_exec_approvals::expire_exec_approval(&record.id).await;
                    None
                }
            };

            ok_response(
                &req.id,
                json!({
                    "id": record.id,
                    "decision": decision,
                    "createdAtMs": record.created_at_ms,
                    "expiresAtMs": record.expires_at_ms,
                }),
            )
        }
        "exec.approval.resolve" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let decision = params
                .get("decision")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if id.is_empty() || decision.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "id and decision required",
                    None,
                );
            }
            if !crate::openclaw_exec_approvals::validate_exec_approval_decision(&decision) {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid decision",
                    None,
                );
            }

            let resolved =
                crate::openclaw_exec_approvals::resolve_exec_approval(&id, &decision).await;
            if resolved.is_none() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "unknown approval id",
                    None,
                );
            }

            let resolved_by = ctx
                .state
                .list_openclaw_clients()
                .await
                .into_iter()
                .find(|c| c.conn_id == ctx.conn_id)
                .map(|c| c.display_name.unwrap_or_else(|| c.client_id));

            broadcast_openclaw_event(
                &ctx.state,
                "exec.approval.resolved",
                json!({ "id": id, "decision": decision, "resolvedBy": resolved_by, "ts": now_ms() }),
                None,
            )
            .await;
            ok_response(&req.id, json!({ "ok": true }))
        }
        "last-heartbeat" => {
            let hb = ctx
                .state
                .openclaw_last_heartbeat()
                .await
                .unwrap_or(serde_json::Value::Null);
            ok_response(&req.id, hb)
        }
        "set-heartbeats" => {
            let enabled = req
                .params
                .as_ref()
                .and_then(|v| v.get("enabled"))
                .and_then(|v| v.as_bool());
            let Some(enabled) = enabled else {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid set-heartbeats params: enabled (boolean) required",
                    None,
                );
            };
            let previously = ctx.state.openclaw_heartbeats_enabled();
            ctx.state.set_openclaw_heartbeats_enabled(enabled);
            // Ensure the heartbeat runner exists; request an immediate run when
            // transitioning from disabled -> enabled for fast UI feedback.
            let _ = crate::openclaw_heartbeat::heartbeat_service_for_state(&ctx.state).await;
            if enabled && !previously {
                crate::openclaw_heartbeat::request_heartbeat_now(
                    &ctx.state,
                    Some("enabled".to_string()),
                )
                .await;
            }
            ok_response(&req.id, json!({ "ok": true, "enabled": enabled }))
        }
        "agent" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let message = params
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if message.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid agent params: message required",
                    None,
                );
            }
            let attachments = params
                .get("attachments")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let run_id = params
                .get("idempotencyKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if run_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid agent params: idempotencyKey required",
                    None,
                );
            }
            let mut session_key = params
                .get("sessionKey")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if session_key.is_none() {
                let session_id = params
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                if let (Some(session_id), Some(store)) = (session_id, ctx.state.session_store()) {
                    if let Ok(uuid) = Uuid::parse_str(&session_id) {
                        if let Ok(Some(session)) = store.get(uuid).await {
                            session_key = Some(session.channel_id);
                        }
                    }
                }
            }
            let session_key = session_key.unwrap_or_else(|| "main".to_string());

            let timeout_ms = params.get("timeout").and_then(|v| v.as_u64());
            let extra_system_prompt = params
                .get("extraSystemPrompt")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Best-effort delivery: match OpenClaw's `deliver` shape (subset).
            let wants_delivery = params.get("deliver").and_then(|v| v.as_bool()).unwrap_or(false);
            let explicit_to = params
                .get("replyTo")
                .and_then(|v| v.as_str())
                .or_else(|| params.get("to").and_then(|v| v.as_str()))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let explicit_channel = params
                .get("replyChannel")
                .and_then(|v| v.as_str())
                .or_else(|| params.get("channel").and_then(|v| v.as_str()))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            let delivery_target = if wants_delivery {
                let normalize_channel = |raw: &str| -> Option<String> {
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    let lowered = trimmed.to_lowercase();
                    let mapped = match lowered.as_str() {
                        "imsg" => "imessage".to_string(),
                        "google-chat" | "gchat" => "googlechat".to_string(),
                        other => other.to_string(),
                    };
                    Some(mapped)
                };

                let mut channel = explicit_channel
                    .as_deref()
                    .and_then(|c| normalize_channel(c))
                    .unwrap_or_default();
                let mut to = explicit_to.clone().unwrap_or_default();

                // If no explicit target, fall back to sessionKey mapping when it matches a channel.
                if to.trim().is_empty() {
                    let (ct, cid) = session_key_to_channel(&session_key);
                    if ctx.state.channel_manager().has_channel(&ct) {
                        channel = ct;
                        to = cid;
                    }
                }

                // Allow `<channel>:<to>` shorthand if channel isn't set.
                if channel.trim().is_empty() {
                    if let Some((left, right)) = to.split_once(':') {
                        if ctx.state.channel_manager().has_channel(left) {
                            channel = normalize_channel(left)
                                .unwrap_or_else(|| left.trim().to_string());
                            to = right.trim().to_string();
                        }
                    }
                } else if let Some((left, right)) = to.split_once(':') {
                    // If the caller passed `to` with a prefix that matches the channel, strip it.
                    if left.eq_ignore_ascii_case(channel.as_str()) {
                        to = right.trim().to_string();
                    }
                }

                if channel.trim().is_empty() {
                    channel = ctx
                        .state
                        .channel_manager()
                        .default_channel()
                        .unwrap_or("whatsapp")
                        .to_string();
                }

                if to.trim().is_empty() {
                    return error_response(
                        &req.id,
                        error_codes::INVALID_REQUEST,
                        "invalid agent params: deliver requested but no target (to/replyTo or sessionKey channel required)",
                        None,
                    );
                }
                if !ctx.state.channel_manager().has_channel(&channel) {
                    return error_response(
                        &req.id,
                        error_codes::INVALID_REQUEST,
                        &format!("unsupported channel: {}", channel),
                        None,
                    );
                }

                Some(AgentDeliveryTarget { channel, to })
            } else {
                None
            };

            let provider = ctx.state.provider().cloned().ok_or_else(|| {
                ErrorShape::new(error_codes::UNAVAILABLE, "provider not configured")
            });
            let provider = match provider {
                Ok(p) => p,
                Err(err) => {
                    return GatewayFrame::Res(ResponseFrame {
                        id: req.id,
                        ok: false,
                        payload: None,
                        error: Some(err),
                    });
                }
            };

            let (_rx, created) = register_agent_run(&run_id).await;
            if created {
                let user_msg = openclaw_user_message_to_drbot(message, &attachments);
                tokio::spawn(spawn_agent_run(
                    ctx.clone(),
                    provider,
                    req.id.clone(),
                    run_id.clone(),
                    session_key,
                    user_msg,
                    timeout_ms,
                    extra_system_prompt,
                    delivery_target,
                ));
            }

            ok_response(
                &req.id,
                json!({
                    "runId": run_id,
                    "status": "accepted",
                    "acceptedAt": now_ms(),
                }),
            )
        }
        "agent.wait" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let run_id = params
                .get("runId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if run_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid agent.wait params: runId required",
                    None,
                );
            }
            let timeout_ms = params
                .get("timeoutMs")
                .and_then(|v| v.as_u64())
                .unwrap_or(30_000);

            match wait_for_agent_run(&run_id, timeout_ms).await {
                None => ok_response(
                    &req.id,
                    json!({
                        "runId": run_id,
                        "status": "timeout",
                    }),
                ),
                Some(snapshot) => ok_response(
                    &req.id,
                    json!({
                        "runId": run_id,
                        "status": snapshot.status,
                        "startedAt": snapshot.started_at,
                        "endedAt": snapshot.ended_at,
                        "error": snapshot.error,
                    }),
                ),
            }
        }
        "agents.list" => ok_response(
            &req.id,
            json!({
                "defaultId": "default",
                "mainKey": "main",
                "scope": "global",
                "agents": [{
                    "id": "default",
                    "name": "drbot",
                    "identity": { "name": "drbot" }
                }]
            }),
        ),
        "agent.identity.get" => ok_response(&req.id, {
            let agent_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("agentId"))
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .trim();
            json!({
                "agentId": if agent_id.is_empty() { "default" } else { agent_id },
                "name": "drbot",
                "avatar": "D"
            })
        }),
        "agents.files.list" => {
            let agent_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("agentId"))
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .trim();
            ok_response(
                &req.id,
                handle_agents_files_list(if agent_id.is_empty() {
                    "default"
                } else {
                    agent_id
                })
                .await,
            )
        }
        "agents.files.get" => {
            let agent_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("agentId"))
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .trim()
                .to_string();
            let name = req
                .params
                .as_ref()
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "name required",
                    None,
                );
            }
            match handle_agents_files_get(
                if agent_id.is_empty() {
                    "default"
                } else {
                    &agent_id
                },
                &name,
            )
            .await
            {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "agents.files.set" => {
            let agent_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("agentId"))
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .trim()
                .to_string();
            let name = req
                .params
                .as_ref()
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let content = req
                .params
                .as_ref()
                .and_then(|v| v.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if name.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "name required",
                    None,
                );
            }
            match handle_agents_files_set(
                if agent_id.is_empty() {
                    "default"
                } else {
                    &agent_id
                },
                &name,
                content,
            )
            .await
            {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "skills.status" => {
            let agent_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("agentId"))
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .trim();
            let agent_id = if agent_id.is_empty() {
                "default"
            } else {
                agent_id
            };
            let workspace = resolve_agent_workspace_dir(agent_id);

            // Best-effort: if remote skills were enabled via skills.update/env, fetch them so
            // they appear in the Skills UI without waiting for a heartbeat.
            tokio::join!(
                crate::colosseum::sync_colosseum_docs_best_effort(ctx.state.config()),
                crate::moltbook::sync_moltbook_docs_best_effort(ctx.state.config()),
                crate::agentwallet::sync_agentwallet_docs_best_effort(ctx.state.config()),
                crate::openclaw_skills::sync_configured_remote_skills_best_effort(
                    ctx.state.config()
                ),
            );

            let remote = resolve_remote_skill_eligibility(&ctx.state).await;
            let report = crate::openclaw_skills::build_skills_status_report_with_remote(
                &workspace,
                ctx.state.config(),
                remote.as_ref(),
            );
            let payload = serde_json::to_value(report).unwrap_or_else(|_| {
                json!({ "workspaceDir": workspace.to_string_lossy(), "managedSkillsDir": "", "skills": [] })
            });
            ok_response(&req.id, payload)
        }
        "skills.bins" => {
            let workspace_dirs = vec![resolve_agent_workspace_dir("default")];
            let bins = crate::openclaw_skills::collect_skill_bins(&workspace_dirs, ctx.state.config());
            ok_response(&req.id, json!({ "bins": bins }))
        }
        "skills.install" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let install_id = params
                .get("installId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if name.is_empty() || install_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid skills.install params: name and installId are required",
                    None,
                );
            }
            let timeout_ms = params
                .get("timeoutMs")
                .and_then(|v| v.as_u64())
                .map(|v| v.max(1000));
            let node_id = params
                .get("nodeId")
                .and_then(|v| v.as_str())
                .or_else(|| params.get("nodeID").and_then(|v| v.as_str()))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            let workspace = resolve_agent_workspace_dir("default");

            let plan = match crate::openclaw_skills::resolve_skill_install_plan(
                ctx.state.config(),
                &workspace,
                &name,
                &install_id,
            ) {
                Ok(p) => p,
                Err(result) => {
                    let payload = serde_json::to_value(&result).unwrap_or_else(|_| {
                        json!({ "ok": result.ok, "message": &result.message })
                    });
                    return GatewayFrame::Res(ResponseFrame {
                        id: req.id,
                        ok: false,
                        payload: Some(payload),
                        error: Some(ErrorShape::new(error_codes::UNAVAILABLE, result.message)),
                    });
                }
            };

            let gateway_platform = resolve_gateway_platform_id();
            let installer_os: Vec<String> = plan
                .os
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let needs_remote = !installer_os.is_empty()
                && !installer_os.iter().any(|os| os == &gateway_platform);

            let result = if needs_remote {
                let node_id = match select_install_node_for_plan(
                    &ctx.state,
                    &plan,
                    node_id.as_deref(),
                )
                .await
                {
                    Ok(id) => id,
                    Err(err) => {
                        return GatewayFrame::Res(ResponseFrame {
                            id: req.id,
                            ok: false,
                            payload: None,
                            error: Some(err),
                        });
                    }
                };

                let timeout_ms = timeout_ms.unwrap_or(300_000).clamp(1_000, 900_000);
                let result =
                    run_skill_install_on_node(&ctx.state, &node_id, &plan, timeout_ms).await;
                if result.ok {
                    let st = ctx.state.clone();
                    let node_id_clone = node_id.clone();
                    tokio::spawn(async move {
                        refresh_remote_node_bins_best_effort(st.clone(), node_id_clone, true)
                            .await;
                        refresh_remote_bins_for_connected_nodes_best_effort(st, true).await;
                    });
                }
                result
            } else {
                crate::openclaw_skills::run_skill_install(
                    ctx.state.config(),
                    &workspace,
                    &name,
                    &install_id,
                    timeout_ms,
                )
                .await
            };
            let payload = serde_json::to_value(&result)
                .unwrap_or_else(|_| json!({ "ok": result.ok, "message": &result.message }));
            if result.ok {
                crate::openclaw_skills::bump_skills_snapshot_version();
                // Installing a skill can add/update darwin requirements; refresh remote bin probes
                // so node eligibility stays accurate.
                let st = ctx.state.clone();
                tokio::spawn(async move {
                    refresh_remote_bins_for_connected_nodes_best_effort(st, true).await;
                });
                ok_response(&req.id, payload)
            } else {
                GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: Some(payload),
                    error: Some(ErrorShape::new(error_codes::UNAVAILABLE, result.message)),
                })
            }
        }
        "skills.update" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let skill_key = params
                .get("skillKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if skill_key.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid skills.update params: skillKey required",
                    None,
                );
            }
            let enabled = params.get("enabled").and_then(|v| v.as_bool());
            let api_key = params
                .get("apiKey")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let url = params
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let fetch_relative_docs = params
                .get("fetchRelativeDocs")
                .and_then(|v| v.as_bool())
                .or_else(|| params.get("fetch_relative_docs").and_then(|v| v.as_bool()));
            let extra_docs = params.get("extraDocs").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            });
            let heartbeat_url = params
                .get("heartbeatUrl")
                .and_then(|v| v.as_str())
                .or_else(|| params.get("heartbeat_url").and_then(|v| v.as_str()))
                .map(|s| s.to_string());
            let env = params.get("env").and_then(|v| v.as_object()).map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|val| (k.to_string(), val.to_string())))
                    .collect::<HashMap<String, String>>()
            });

            match crate::openclaw_skills::update_skill_config(
                crate::openclaw_skills::SkillsUpdateRequest {
                    skill_key: skill_key.clone(),
                    enabled,
                    api_key,
                    env,
                    url,
                    fetch_relative_docs,
                    extra_docs,
                    heartbeat_url,
                },
            ) {
                Ok(config) => {
                    // If the operator enabled a remote skill, fetch its docs immediately so it can
                    // be used in prompts (and show up in skills.status) without waiting for a heartbeat.
                    if skill_key == crate::colosseum::COLOSSEUM_SKILL_KEY
                        || skill_key == crate::moltbook::MOLTBOOK_SKILL_KEY
                        || skill_key == crate::agentwallet::AGENTWALLET_SKILL_KEY
                    {
                        let st = ctx.state.clone();
                        let key = skill_key.clone();
                        tokio::spawn(async move {
                            if key == crate::colosseum::COLOSSEUM_SKILL_KEY {
                                crate::colosseum::sync_colosseum_docs_best_effort(st.config())
                                    .await;
                            } else if key == crate::moltbook::MOLTBOOK_SKILL_KEY {
                                crate::moltbook::sync_moltbook_docs_best_effort(st.config()).await;
                            } else if key == crate::agentwallet::AGENTWALLET_SKILL_KEY {
                                crate::agentwallet::sync_agentwallet_docs_best_effort(st.config())
                                    .await;
                            }

                            // Sync any configured remote SKILL.md URLs as well.
                            crate::openclaw_skills::sync_configured_remote_skills_best_effort(
                                st.config(),
                            )
                            .await;

                            // After any remote docs update, refresh node bin probes so skills.status
                            // can report accurate remote eligibility without waiting for reconnect.
                            refresh_remote_bins_for_connected_nodes_best_effort(st, true).await;
                        });
                    } else {
                        // Local skill configs can still affect required bins (e.g. enabling a skill
                        // with darwin-only requirements). Refresh probes best-effort.
                        let st = ctx.state.clone();
                        tokio::spawn(async move {
                            crate::openclaw_skills::sync_configured_remote_skills_best_effort(
                                st.config(),
                            )
                            .await;
                            refresh_remote_bins_for_connected_nodes_best_effort(st, true).await;
                        });
                    }

                    ok_response(
                        &req.id,
                        json!({ "ok": true, "skillKey": skill_key, "config": config }),
                    )
                }
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "colosseum.request" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let method = params
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("GET")
                .trim()
                .to_string();
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let query = params.get("query").filter(|v| v.is_object());
            let body = params.get("body");
            let timeout_ms = params.get("timeoutMs").and_then(|v| v.as_u64());
            let dry_run = params
                .get("dryRun")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let method_upper = method.trim().to_uppercase();
            let is_write =
                matches!(method_upper.as_str(), "POST" | "PUT" | "PATCH" | "DELETE");
            let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_COLOSSEUM_WRITE")
                .ok()
                .as_deref()
                == Some("1");
            let mut allow_write = allow_write_by_env;
            if is_write && !dry_run && !allow_write_by_env {
                let mut url = "https://agents.colosseum.com/api".to_string();
                if path.starts_with('/') {
                    url.push_str(&path);
                } else {
                    url.push('/');
                    url.push_str(&path);
                }

                let approval = ExecApprovalRequestPayload {
                    command: format!("colosseum.request {} {}", method_upper, path),
                    cwd: None,
                    host: Some("colosseum".to_string()),
                    security: Some("integration-http-write".to_string()),
                    ask: Some(format!(
                        "Allow Colosseum API write request? {} {}",
                        method_upper, path
                    )),
                    agent_id: Some("default".to_string()),
                    resolved_path: Some(url),
                    session_key: None,
                };
                if let Err(err) = crate::openclaw_exec_approvals::ensure_tool_write_allowed(
                    &ctx.state,
                    "colosseum.request",
                    approval,
                    120_000,
                )
                .await
                {
                    return GatewayFrame::Res(ResponseFrame {
                        id: req.id,
                        ok: false,
                        payload: None,
                        error: Some(err),
                    });
                }
                allow_write = true;
            }

            match crate::colosseum::colosseum_request(
                &method_upper,
                &path,
                query,
                body,
                timeout_ms,
                dry_run,
                allow_write,
            )
            .await
            {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "moltbook.request" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let method = params
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("GET")
                .trim()
                .to_string();
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let query = params.get("query").filter(|v| v.is_object());
            let body = params.get("body");
            let timeout_ms = params.get("timeoutMs").and_then(|v| v.as_u64());
            let dry_run = params
                .get("dryRun")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let method_upper = method.trim().to_uppercase();
            let is_write =
                matches!(method_upper.as_str(), "POST" | "PUT" | "PATCH" | "DELETE");
            let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_MOLTBOOK_WRITE")
                .ok()
                .as_deref()
                == Some("1");
            let mut allow_write = allow_write_by_env;
            if is_write && !dry_run && !allow_write_by_env {
                let mut url = "https://www.moltbook.com/api/v1".to_string();
                if path.starts_with('/') {
                    url.push_str(&path);
                } else {
                    url.push('/');
                    url.push_str(&path);
                }

                let approval = ExecApprovalRequestPayload {
                    command: format!("moltbook.request {} {}", method_upper, path),
                    cwd: None,
                    host: Some("moltbook".to_string()),
                    security: Some("integration-http-write".to_string()),
                    ask: Some(format!(
                        "Allow Moltbook API write request? {} {}",
                        method_upper, path
                    )),
                    agent_id: Some("default".to_string()),
                    resolved_path: Some(url),
                    session_key: None,
                };
                if let Err(err) = crate::openclaw_exec_approvals::ensure_tool_write_allowed(
                    &ctx.state,
                    "moltbook.request",
                    approval,
                    120_000,
                )
                .await
                {
                    return GatewayFrame::Res(ResponseFrame {
                        id: req.id,
                        ok: false,
                        payload: None,
                        error: Some(err),
                    });
                }
                allow_write = true;
            }

            match crate::moltbook::moltbook_request(
                &method_upper,
                &path,
                query,
                body,
                timeout_ms,
                dry_run,
                allow_write,
            )
            .await
            {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "browser.request" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let method = params
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if method.is_empty() || path.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "method and path are required",
                    None,
                );
            }
            let method_upper = method.to_uppercase();
            if method_upper != "GET" && method_upper != "POST" && method_upper != "DELETE" {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "method must be GET, POST, or DELETE",
                    None,
                );
            }
            let query = params.get("query").filter(|v| v.is_object());
            let body = params.get("body");
            let timeout_ms = params.get("timeoutMs").and_then(|v| v.as_u64());
            match handle_browser_request(&ctx.state, &method_upper, path, query, body, timeout_ms)
                .await
            {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "send" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let mut channel = params
                .get("channel")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let mut to = params
                .get("to")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let mut message = params
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let media_url = params
                .get("mediaUrl")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let media_urls = params
                .get("mediaUrls")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let session_key = params
                .get("sessionKey")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let dry_run = params.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);
            let idem = params
                .get("idempotencyKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            if to.is_empty() || message.is_empty() || idem.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "send requires to, message, and idempotencyKey",
                    None,
                );
            }

            let dedupe_key = format!("send:{}", idem);
            let state = ctx.state.clone();
            let entry = openclaw_idempotent_run(&dedupe_key, || async move {
                let normalize_channel = |raw: &str| -> Option<String> {
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    let lowered = trimmed.to_lowercase();
                    let mapped = match lowered.as_str() {
                        "imsg" => "imessage".to_string(),
                        "google-chat" | "gchat" => "googlechat".to_string(),
                        other => other.to_string(),
                    };
                    Some(mapped)
                };

                if !channel.is_empty() {
                    let normalized = normalize_channel(&channel).unwrap_or_default();
                    if !state.channel_manager().has_channel(&normalized) {
                        let err = ErrorShape::new(
                            error_codes::INVALID_REQUEST,
                            format!("unsupported channel: {}", channel),
                        );
                        return OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: false,
                            payload: None,
                            error: Some(err),
                        };
                    }
                    channel = normalized;
                }

                if channel.is_empty() {
                    // drbot compatibility: `<channel>:<to>` shorthand when channel is omitted.
                    if let Some((left, right)) = to.split_once(':') {
                        if state.channel_manager().has_channel(left) {
                            channel =
                                normalize_channel(left).unwrap_or_else(|| left.trim().to_string());
                            to = right.trim().to_string();
                        }
                    }
                }

                if channel.is_empty() {
                    if let Some(default) = state.channel_manager().default_channel() {
                        channel = default.to_string();
                    } else {
                        // OpenClaw default chat channel.
                        channel = "whatsapp".to_string();
                    }
                }

                if !state.channel_manager().has_channel(&channel) {
                    let err = ErrorShape::new(
                        error_codes::INVALID_REQUEST,
                        format!("unsupported channel: {}", channel),
                    );
                    return OpenclawDedupeEntry {
                        ts: now_ms(),
                        ok: false,
                        payload: None,
                        error: Some(err),
                    };
                }

                // Avoid blocking on exec approvals when the channel is not usable anyway.
                if !dry_run {
                    if !state.channel_manager().is_enabled(&channel) {
                        let err = ErrorShape::new(
                            error_codes::UNAVAILABLE,
                            format!("channel '{}' is disabled", channel),
                        );
                        return OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: false,
                            payload: None,
                            error: Some(err),
                        };
                    }
                    if !state.channel_manager().is_configured(&channel) {
                        let err = ErrorShape::new(
                            error_codes::UNAVAILABLE,
                            format!("channel '{}' is not configured", channel),
                        );
                        return OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: false,
                            payload: None,
                            error: Some(err),
                        };
                    }
                }

                // Inline media URLs into the message (best-effort).
                if let Some(url) = media_url.as_deref() {
                    if !message.is_empty() {
                        message.push_str("\n\n");
                    }
                    message.push_str(url);
                }
                if !media_urls.is_empty() {
                    if !message.is_empty() {
                        message.push_str("\n\n");
                    }
                    for (idx, url) in media_urls.iter().enumerate() {
                        if idx > 0 {
                            message.push('\n');
                        }
                        message.push_str(url);
                    }
                }
                message = message.trim().to_string();

                let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_SEND_WRITE")
                    .ok()
                    .as_deref()
                    == Some("1");
                if !dry_run && !allow_write_by_env {
                    let approval = ExecApprovalRequestPayload {
                        command: format!("send {} {}", channel, to),
                        cwd: None,
                        host: Some("channels".to_string()),
                        security: Some("channel-send".to_string()),
                        ask: Some(format!(
                            "Allow sending an outbound message via {} to {}?",
                            channel, to
                        )),
                        agent_id: Some("default".to_string()),
                        resolved_path: None,
                        session_key: session_key.clone(),
                    };
                    if let Err(err) = crate::openclaw_exec_approvals::ensure_tool_write_allowed(
                        &state,
                        "send",
                        approval,
                        120_000,
                    )
                    .await
                    {
                        return OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: false,
                            payload: None,
                            error: Some(err),
                        };
                    }
                }

                let outgoing = OutgoingMessage::text(message.clone());
                let send_res = if dry_run {
                    Ok(())
                } else {
                    state.channel_manager().send(&channel, &to, outgoing).await
                };

                match send_res {
                    Ok(()) => {
                        let message_id = Uuid::new_v4().to_string();
                        let payload = json!({
                            "runId": idem,
                            "messageId": message_id,
                            "channel": channel.clone(),
                            "channelId": to.clone(),
                        });

                        // Best-effort: mirror to session transcript for inspection via sessions.preview.
                        if let Some(store) = state.session_store() {
                            let user_id =
                                Uuid::new_v5(&Uuid::NAMESPACE_URL, b"openclaw-operator");
                            let mirror_key = session_key
                                .clone()
                                .unwrap_or_else(|| format!("{}:{}", channel, to));
                            let mut session = store
                                .get_by_channel("openclaw", &mirror_key)
                                .await
                                .ok()
                                .flatten();
                            if session.is_none() {
                                let (ct, cid) = session_key_to_channel(&mirror_key);
                                session = store.get_or_create(user_id, &ct, &cid).await.ok();
                            }
                            if let Some(mut s) = session {
                                s.add_message(Message::assistant(&message));
                                s.update_timestamp();
                                let _ = store.update(&s).await;
                            }
                        }

                        OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: true,
                            payload: Some(payload),
                            error: None,
                        }
                    }
                    Err(e) => {
                        let code = match e {
                            drbot_core::Error::InvalidInput(_)
                            | drbot_core::Error::NotFound(_)
                            | drbot_core::Error::Config(_) => error_codes::UNAVAILABLE,
                            _ => error_codes::UNAVAILABLE,
                        };
                        let err = ErrorShape::new(code, format!("send failed: {}", e));
                        OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: false,
                            payload: None,
                            error: Some(err),
                        }
                    }
                }
            })
            .await;

            GatewayFrame::Res(ResponseFrame {
                id: req.id,
                ok: entry.ok,
                payload: entry.payload,
                error: entry.error,
            })
        }
        "poll" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let mut channel = params
                .get("channel")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let mut to = params
                .get("to")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let question = params
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let options = params
                .get("options")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let max_selections = params.get("maxSelections").and_then(|v| v.as_u64());
            let duration_hours = params.get("durationHours").and_then(|v| v.as_u64());
            let dry_run = params.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);
            let idem = params
                .get("idempotencyKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            if to.is_empty() || question.is_empty() || options.len() < 2 || idem.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "poll requires to, question, options (>=2), and idempotencyKey",
                    None,
                );
            }

            let dedupe_key = format!("poll:{}", idem);
            let state = ctx.state.clone();
            let entry = openclaw_idempotent_run(&dedupe_key, || async move {
                let normalize_channel = |raw: &str| -> Option<String> {
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    let lowered = trimmed.to_lowercase();
                    let mapped = match lowered.as_str() {
                        "imsg" => "imessage".to_string(),
                        "google-chat" | "gchat" => "googlechat".to_string(),
                        other => other.to_string(),
                    };
                    Some(mapped)
                };

                if !channel.is_empty() {
                    let normalized = normalize_channel(&channel).unwrap_or_default();
                    if !state.channel_manager().has_channel(&normalized) {
                        let err = ErrorShape::new(
                            error_codes::INVALID_REQUEST,
                            format!("unsupported poll channel: {}", channel),
                        );
                        return OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: false,
                            payload: None,
                            error: Some(err),
                        };
                    }
                    channel = normalized;
                }

                if channel.is_empty() {
                    if let Some((left, right)) = to.split_once(':') {
                        if state.channel_manager().has_channel(left) {
                            channel =
                                normalize_channel(left).unwrap_or_else(|| left.trim().to_string());
                            to = right.trim().to_string();
                        }
                    }
                }

                if channel.is_empty() {
                    if let Some(default) = state.channel_manager().default_channel() {
                        channel = default.to_string();
                    } else {
                        channel = "whatsapp".to_string();
                    }
                }

                if !state.channel_manager().has_channel(&channel) {
                    let err = ErrorShape::new(
                        error_codes::INVALID_REQUEST,
                        format!("unsupported poll channel: {}", channel),
                    );
                    return OpenclawDedupeEntry {
                        ts: now_ms(),
                        ok: false,
                        payload: None,
                        error: Some(err),
                    };
                }

                if !dry_run {
                    if !state.channel_manager().is_enabled(&channel) {
                        let err = ErrorShape::new(
                            error_codes::UNAVAILABLE,
                            format!("poll channel '{}' is disabled", channel),
                        );
                        return OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: false,
                            payload: None,
                            error: Some(err),
                        };
                    }
                    if !state.channel_manager().is_configured(&channel) {
                        let err = ErrorShape::new(
                            error_codes::UNAVAILABLE,
                            format!("poll channel '{}' is not configured", channel),
                        );
                        return OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: false,
                            payload: None,
                            error: Some(err),
                        };
                    }
                }

                let mut text = String::new();
                text.push_str(question.trim());
                text.push_str("\n\n");
                for (idx, opt) in options.iter().enumerate().take(12) {
                    let label = opt.as_str().unwrap_or("").trim();
                    if label.is_empty() {
                        continue;
                    }
                    text.push_str(&format!("{}. {}\n", idx + 1, label));
                }
                let reply_hint = if max_selections.unwrap_or(1) > 1 {
                    "Reply with the numbers of your choices."
                } else {
                    "Reply with the number of your choice."
                };
                text.push_str("\n");
                text.push_str(reply_hint);

                let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_SEND_WRITE")
                    .ok()
                    .as_deref()
                    == Some("1");
                if !dry_run && !allow_write_by_env {
                    let approval = ExecApprovalRequestPayload {
                        command: format!("poll {} {}", channel, to),
                        cwd: None,
                        host: Some("channels".to_string()),
                        security: Some("channel-send".to_string()),
                        ask: Some(format!(
                            "Allow sending an outbound poll via {} to {}?",
                            channel, to
                        )),
                        agent_id: Some("default".to_string()),
                        resolved_path: None,
                        session_key: Some(format!("{}:{}", channel, to)),
                    };
                    if let Err(err) = crate::openclaw_exec_approvals::ensure_tool_write_allowed(
                        &state,
                        "poll",
                        approval,
                        120_000,
                    )
                    .await
                    {
                        return OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: false,
                            payload: None,
                            error: Some(err),
                        };
                    }
                }

                let outgoing = OutgoingMessage::text(text);
                let send_res = if dry_run {
                    Ok(())
                } else {
                    state.channel_manager().send(&channel, &to, outgoing).await
                };

                match send_res {
                    Ok(()) => {
                        let message_id = Uuid::new_v4().to_string();
                        let poll_id = state
                            .openclaw_polls()
                            .register_text_poll(
                                &idem,
                                &channel,
                                &to,
                                &question,
                                options
                                    .iter()
                                    .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                                    .filter(|s| !s.is_empty())
                                    .collect::<Vec<_>>(),
                                max_selections,
                                duration_hours,
                            )
                            .await;

                        let payload = json!({
                            "runId": idem,
                            "messageId": message_id,
                            "channel": channel,
                            "channelId": to,
                            "pollId": poll_id,
                        });
                        OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: true,
                            payload: Some(payload),
                            error: None,
                        }
                    }
                    Err(e) => {
                        let code = match e {
                            drbot_core::Error::InvalidInput(_)
                            | drbot_core::Error::NotFound(_)
                            | drbot_core::Error::Config(_) => error_codes::UNAVAILABLE,
                            _ => error_codes::UNAVAILABLE,
                        };
                        let err = ErrorShape::new(code, format!("poll failed: {}", e));
                        OpenclawDedupeEntry {
                            ts: now_ms(),
                            ok: false,
                            payload: None,
                            error: Some(err),
                        }
                    }
                }
            })
            .await;

            GatewayFrame::Res(ResponseFrame {
                id: req.id,
                ok: entry.ok,
                payload: entry.payload,
                error: entry.error,
            })
        }
        "node.pair.request" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let node_id = params
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if node_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "nodeId required",
                    None,
                );
            }
            match request_node_pairing(&ctx.state, node_id, &params) {
                Ok((payload, created)) => {
                    if let Some(request) = created {
                        broadcast_openclaw_event(
                            &ctx.state,
                            "node.pair.requested",
                            serde_json::to_value(&request).unwrap_or_else(|_| json!({})),
                            None,
                        )
                        .await;
                    }
                    ok_response(&req.id, payload)
                }
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "node.pair.list" => match list_node_pairing(&ctx.state) {
            Ok(payload) => ok_response(&req.id, payload),
            Err(err) => GatewayFrame::Res(ResponseFrame {
                id: req.id,
                ok: false,
                payload: None,
                error: Some(err),
            }),
        },
        "node.pair.approve" => {
            let request_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("requestId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if request_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "requestId required",
                    None,
                );
            }
            match approve_node_pairing(&ctx.state, request_id) {
                Ok(Some((request_id, node))) => {
                    broadcast_openclaw_event(
                        &ctx.state,
                        "node.pair.resolved",
                        json!({
                            "requestId": request_id,
                            "nodeId": node.node_id,
                            "decision": "approved",
                            "ts": now_ms(),
                        }),
                        None,
                    )
                    .await;
                    ok_response(&req.id, json!({ "requestId": request_id, "node": node }))
                }
                Ok(None) => error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "unknown requestId",
                    None,
                ),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "node.pair.reject" => {
            let request_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("requestId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if request_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "requestId required",
                    None,
                );
            }
            match reject_node_pairing(&ctx.state, request_id) {
                Ok(Some((request_id, node_id))) => {
                    broadcast_openclaw_event(
                        &ctx.state,
                        "node.pair.resolved",
                        json!({
                            "requestId": request_id,
                            "nodeId": node_id,
                            "decision": "rejected",
                            "ts": now_ms(),
                        }),
                        None,
                    )
                    .await;
                    ok_response(
                        &req.id,
                        json!({ "requestId": request_id, "nodeId": node_id }),
                    )
                }
                Ok(None) => error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "unknown requestId",
                    None,
                ),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "node.pair.verify" => {
            let node_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("nodeId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let token = req
                .params
                .as_ref()
                .and_then(|v| v.get("token"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if node_id.is_empty() || token.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "nodeId and token required",
                    None,
                );
            }
            match verify_node_token(&ctx.state, node_id, token) {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "node.rename" => {
            let node_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("nodeId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let display_name = req
                .params
                .as_ref()
                .and_then(|v| v.get("displayName"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if node_id.is_empty() || display_name.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "nodeId and displayName required",
                    None,
                );
            }
            match rename_paired_node(&ctx.state, node_id, display_name) {
                Ok(Some(node)) => ok_response(
                    &req.id,
                    json!({ "nodeId": node.node_id, "displayName": node.display_name }),
                ),
                Ok(None) => error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "unknown nodeId",
                    None,
                ),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "node.list" => {
            let ts = now_ms();
            let paired = load_node_pairing_state(&ctx.state).1;

            let mut connected_by_id: HashMap<String, OpenclawClient> = HashMap::new();
            for c in ctx.state.list_openclaw_clients().await {
                if c.role != "node" {
                    continue;
                }
                let node_id = c
                    .device_id
                    .clone()
                    .or(c.instance_id.clone())
                    .unwrap_or_else(|| c.conn_id.clone());
                connected_by_id.insert(node_id, c);
            }

            let mut node_ids: Vec<String> = Vec::new();
            for k in paired.keys() {
                node_ids.push(k.clone());
            }
            for k in connected_by_id.keys() {
                if !paired.contains_key(k) {
                    node_ids.push(k.clone());
                }
            }

            let mut nodes: Vec<serde_json::Value> = Vec::new();
            for node_id in node_ids {
                let paired_node = paired.get(&node_id);
                let live = connected_by_id.get(&node_id);

                let caps = {
                    let mut set = std::collections::BTreeSet::new();
                    if let Some(l) = live {
                        for c in &l.caps {
                            set.insert(c.clone());
                        }
                    }
                    if let Some(p) = paired_node.and_then(|p| p.caps.as_ref()) {
                        for c in p {
                            set.insert(c.clone());
                        }
                    }
                    set.into_iter().collect::<Vec<_>>()
                };
                let commands = {
                    let mut set = std::collections::BTreeSet::new();
                    if let Some(l) = live {
                        for c in &l.commands {
                            set.insert(c.clone());
                        }
                    }
                    if let Some(p) = paired_node.and_then(|p| p.commands.as_ref()) {
                        for c in p {
                            set.insert(c.clone());
                        }
                    }
                    set.into_iter().collect::<Vec<_>>()
                };

                nodes.push(json!({
                    "nodeId": node_id,
                    "displayName": live.and_then(|l| l.display_name.clone()).or_else(|| paired_node.and_then(|p| p.display_name.clone())),
                    "platform": live.map(|l| l.platform.clone()).or_else(|| paired_node.and_then(|p| p.platform.clone())),
                    "version": live.map(|l| l.client_version.clone()).or_else(|| paired_node.and_then(|p| p.version.clone())),
                    "coreVersion": paired_node.and_then(|p| p.core_version.clone()),
                    "uiVersion": paired_node.and_then(|p| p.ui_version.clone()),
                    "deviceFamily": live.and_then(|l| l.device_family.clone()).or_else(|| paired_node.and_then(|p| p.device_family.clone())),
                    "modelIdentifier": live.and_then(|l| l.model_identifier.clone()).or_else(|| paired_node.and_then(|p| p.model_identifier.clone())),
                    "remoteIp": live.map(|l| l.peer.ip().to_string()).or_else(|| paired_node.and_then(|p| p.remote_ip.clone())),
                    "caps": caps,
                    "commands": commands,
                    "pathEnv": live.and_then(|l| l.path_env.clone()),
                    "permissions": live.map(|l| l.permissions.clone()).or_else(|| paired_node.and_then(|p| p.permissions.clone())),
                    "connectedAtMs": live.map(|l| l.connected_at_ms),
                    "paired": paired_node.is_some(),
                    "connected": live.is_some(),
                }));
            }

            nodes.sort_by(|a, b| {
                let ac = a
                    .get("connected")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let bc = b
                    .get("connected")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if ac != bc {
                    return if ac {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    };
                }
                let an = a
                    .get("displayName")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| a.get("nodeId").and_then(|v| v.as_str()).unwrap_or(""))
                    .to_lowercase();
                let bn = b
                    .get("displayName")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| b.get("nodeId").and_then(|v| v.as_str()).unwrap_or(""))
                    .to_lowercase();
                an.cmp(&bn)
            });

            ok_response(&req.id, json!({ "ts": ts, "nodes": nodes }))
        }
        "node.describe" => {
            let node_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("nodeId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if node_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "nodeId required",
                    None,
                );
            }
            let paired = load_node_pairing_state(&ctx.state).1;
            let live = ctx
                .state
                .list_openclaw_clients()
                .await
                .into_iter()
                .find(|c| {
                    c.role == "node"
                        && c.device_id
                            .clone()
                            .or(c.instance_id.clone())
                            .unwrap_or_else(|| c.conn_id.clone())
                            == node_id
                });
            let paired_node = paired.get(&node_id);
            if live.is_none() && paired_node.is_none() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "unknown nodeId",
                    None,
                );
            }
            ok_response(
                &req.id,
                json!({
                    "ts": now_ms(),
                    "nodeId": node_id,
                    "displayName": live.as_ref().and_then(|l| l.display_name.clone()).or_else(|| paired_node.and_then(|p| p.display_name.clone())),
                    "platform": live.as_ref().map(|l| l.platform.clone()).or_else(|| paired_node.and_then(|p| p.platform.clone())),
                    "version": live.as_ref().map(|l| l.client_version.clone()).or_else(|| paired_node.and_then(|p| p.version.clone())),
                    "coreVersion": paired_node.and_then(|p| p.core_version.clone()),
                    "uiVersion": paired_node.and_then(|p| p.ui_version.clone()),
                    "deviceFamily": live.as_ref().and_then(|l| l.device_family.clone()).or_else(|| paired_node.and_then(|p| p.device_family.clone())),
                    "modelIdentifier": live.as_ref().and_then(|l| l.model_identifier.clone()).or_else(|| paired_node.and_then(|p| p.model_identifier.clone())),
                    "remoteIp": live.as_ref().map(|l| l.peer.ip().to_string()).or_else(|| paired_node.and_then(|p| p.remote_ip.clone())),
                    "caps": live.as_ref().map(|l| l.caps.clone()).unwrap_or_default(),
                    "commands": live.as_ref().map(|l| l.commands.clone()).unwrap_or_default(),
                    "pathEnv": live.as_ref().and_then(|l| l.path_env.clone()),
                    "permissions": live.as_ref().map(|l| l.permissions.clone()).or_else(|| paired_node.and_then(|p| p.permissions.clone())),
                    "connectedAtMs": live.as_ref().map(|l| l.connected_at_ms),
                    "paired": paired_node.is_some(),
                    "connected": live.is_some(),
                }),
            )
        }
        "node.invoke" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let node_id = params
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let command = params
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let timeout_ms = params
                .get("timeoutMs")
                .and_then(|v| v.as_u64())
                .unwrap_or(30_000);
            let idempotency_key = params
                .get("idempotencyKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if node_id.is_empty() || command.is_empty() || idempotency_key.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "nodeId, command, and idempotencyKey required",
                    None,
                );
            }

            let state = ctx.state.clone();
            let dedupe_key = format!("node.invoke:{}:{}", node_id, idempotency_key);
            let params_json = params.get("params").map(|v| v.to_string());
            let entry = openclaw_idempotent_run(&dedupe_key, || async move {
                let node_client = state
                    .list_openclaw_clients()
                    .await
                    .into_iter()
                    .find(|c| {
                        if c.role != "node" {
                            return false;
                        }
                        let id = c
                            .device_id
                            .clone()
                            .or(c.instance_id.clone())
                            .unwrap_or_else(|| c.conn_id.clone());
                        id == node_id
                    });
                let Some(node_client) = node_client else {
                    let err = ErrorShape::new(error_codes::UNAVAILABLE, "node not connected")
                        .with_details(json!({ "code": "NOT_CONNECTED" }));
                    return OpenclawDedupeEntry {
                        ts: now_ms(),
                        ok: false,
                        payload: None,
                        error: Some(err),
                    };
                };

                let allowlist = resolve_node_command_allowlist(
                    &node_client.platform,
                    node_client.device_family.as_deref(),
                );
                let allowed_reason = if !allowlist.contains(&command) {
                    Some("command not allowlisted")
                } else if node_client.commands.is_empty() {
                    Some("node did not declare commands")
                } else if !node_client.commands.iter().any(|c| c == &command) {
                    Some("command not declared by node")
                } else {
                    None
                };

                if let Some(reason) = allowed_reason {
                    let err = ErrorShape::new(error_codes::INVALID_REQUEST, "node command not allowed")
                        .with_details(json!({ "reason": reason, "command": command }));
                    return OpenclawDedupeEntry {
                        ts: now_ms(),
                        ok: false,
                        payload: None,
                        error: Some(err),
                    };
                }

                let invoke_id = Uuid::new_v4().to_string();
                let payload = json!({
                    "id": invoke_id,
                    "nodeId": node_id.clone(),
                    "command": command.clone(),
                    "paramsJSON": params_json,
                    "timeoutMs": timeout_ms,
                    "idempotencyKey": idempotency_key.clone(),
                });

                let rx =
                    register_node_invoke(invoke_id.clone(), node_id.clone(), command.clone()).await;
                send_event(
                    &node_client.tx,
                    &node_client.queued_bytes,
                    &node_client.closing,
                    node_client.event_seq.as_ref(),
                    "node.invoke.request",
                    payload,
                    None,
                    false,
                )
                .await;

                let wait =
                    tokio::time::timeout(Duration::from_millis(timeout_ms.max(1)), rx).await;
                let result = match wait {
                    Ok(Ok(r)) => r,
                    Ok(Err(_)) => NodeInvokeResult {
                        ok: false,
                        payload: None,
                        payload_json: None,
                        error: Some(NodeInvokeError {
                            code: Some("UNAVAILABLE".to_string()),
                            message: Some("node invoke dropped".to_string()),
                        }),
                    },
                    Err(_) => {
                        openclaw_node_invokes().lock().await.remove(&invoke_id);
                        NodeInvokeResult {
                            ok: false,
                            payload: None,
                            payload_json: None,
                            error: Some(NodeInvokeError {
                                code: Some("TIMEOUT".to_string()),
                                message: Some("node invoke timed out".to_string()),
                            }),
                        }
                    }
                };

                if !result.ok {
                    let msg = result
                        .error
                        .as_ref()
                        .and_then(|e| e.message.as_deref())
                        .unwrap_or("node invoke failed")
                        .to_string();
                    let err = ErrorShape::new(error_codes::UNAVAILABLE, msg)
                        .with_details(json!({ "nodeError": result.error }));
                    return OpenclawDedupeEntry {
                        ts: now_ms(),
                        ok: false,
                        payload: None,
                        error: Some(err),
                    };
                }

                let payload_value = if let Some(s) = &result.payload_json {
                    serde_json::from_str::<serde_json::Value>(s)
                        .ok()
                        .or_else(|| result.payload.clone())
                } else {
                    result.payload.clone()
                };
                OpenclawDedupeEntry {
                    ts: now_ms(),
                    ok: true,
                    payload: Some(json!({
                        "ok": true,
                        "nodeId": node_id,
                        "command": command,
                        "payload": payload_value,
                        "payloadJSON": result.payload_json,
                    })),
                    error: None,
                }
            })
            .await;

            GatewayFrame::Res(ResponseFrame {
                id: req.id,
                ok: entry.ok,
                payload: entry.payload,
                error: entry.error,
            })
        }
        "node.invoke.result" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let id = params
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let node_id = params
                .get("nodeId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let ok = params.get("ok").and_then(|v| v.as_bool());
            if id.is_empty() || node_id.is_empty() || ok.is_none() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "id, nodeId, ok required",
                    None,
                );
            }

            // OpenClaw parity: when a node submits an invoke result, it must match its own nodeId.
            if let Some(caller) = ctx.state.get_openclaw_client(&ctx.conn_id).await {
                if caller.role == "node" {
                    let caller_node_id = caller
                        .device_id
                        .clone()
                        .or(caller.instance_id.clone())
                        .unwrap_or_else(|| caller.conn_id.clone());
                    if caller_node_id != node_id {
                        return error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            "nodeId mismatch",
                            None,
                        );
                    }
                }
            }
            let payload = params.get("payload").cloned();
            let payload_json = params
                .get("payloadJSON")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let error =
                params
                    .get("error")
                    .and_then(|v| v.as_object())
                    .map(|obj| NodeInvokeError {
                        code: obj
                            .get("code")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        message: obj
                            .get("message")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                    });

            let resolved = resolve_node_invoke(
                &id,
                &node_id,
                NodeInvokeResult {
                    ok: ok.unwrap_or(false),
                    payload,
                    payload_json,
                    error,
                },
            )
            .await;
            if !resolved {
                ok_response(&req.id, json!({ "ok": true, "ignored": true }))
            } else {
                ok_response(&req.id, json!({ "ok": true }))
            }
        }
        "node.event" => {
            // Best-effort: allow nodes to forward events; only handle a small subset for interop.
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let event = params
                .get("event")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if event.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "event required",
                    None,
                );
            }
            if event == "voicewake.changed" {
                if let Some(triggers) = params
                    .get("payload")
                    .and_then(|v| v.get("triggers"))
                    .and_then(|v| v.as_array())
                {
                    if let Ok(payload) = handle_voicewake_set(triggers).await {
                        broadcast_openclaw_event(&ctx.state, "voicewake.changed", payload, None)
                            .await;
                    }
                }
            }
            ok_response(&req.id, json!({ "ok": true }))
        }
        "device.pair.list" => match list_device_pairing(&ctx.state) {
            Ok(payload) => ok_response(&req.id, payload),
            Err(err) => GatewayFrame::Res(ResponseFrame {
                id: req.id,
                ok: false,
                payload: None,
                error: Some(err),
            }),
        },
        "device.pair.approve" => {
            let request_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("requestId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if request_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "requestId required",
                    None,
                );
            }
            match approve_device_pairing(&ctx.state, request_id) {
                Ok(Some((request_id, device))) => {
                    broadcast_openclaw_event(
                        &ctx.state,
                        "device.pair.resolved",
                        json!({
                            "requestId": request_id,
                            "deviceId": device.device_id,
                            "decision": "approved",
                            "ts": now_ms(),
                        }),
                        None,
                    )
                    .await;
                    ok_response(
                        &req.id,
                        json!({ "requestId": request_id, "device": device }),
                    )
                }
                Ok(None) => error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "unknown requestId",
                    None,
                ),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "device.pair.reject" => {
            let request_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("requestId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if request_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "requestId required",
                    None,
                );
            }
            match reject_device_pairing(&ctx.state, request_id) {
                Ok(Some((request_id, device_id))) => {
                    broadcast_openclaw_event(
                        &ctx.state,
                        "device.pair.resolved",
                        json!({
                            "requestId": request_id,
                            "deviceId": device_id,
                            "decision": "rejected",
                            "ts": now_ms(),
                        }),
                        None,
                    )
                    .await;
                    ok_response(
                        &req.id,
                        json!({ "requestId": request_id, "deviceId": device_id }),
                    )
                }
                Ok(None) => error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "unknown requestId",
                    None,
                ),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "device.token.rotate" => {
            let device_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("deviceId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let role = req
                .params
                .as_ref()
                .and_then(|v| v.get("role"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let scopes = req
                .params
                .as_ref()
                .and_then(|v| v.get("scopes"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                });
            match rotate_device_token(&ctx.state, device_id, role, scopes) {
                Ok(Some(entry)) => ok_response(
                    &req.id,
                    json!({
                        "deviceId": device_id,
                        "role": entry.role,
                        "token": entry.token,
                        "scopes": entry.scopes,
                        "rotatedAtMs": entry.rotated_at_ms.unwrap_or(entry.created_at_ms),
                    }),
                ),
                Ok(None) => error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "unknown deviceId/role",
                    None,
                ),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "device.token.revoke" => {
            let device_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("deviceId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let role = req
                .params
                .as_ref()
                .and_then(|v| v.get("role"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            match revoke_device_token(&ctx.state, device_id, role) {
                Ok(Some(entry)) => ok_response(
                    &req.id,
                    json!({
                        "deviceId": device_id,
                        "role": entry.role,
                        "revokedAtMs": entry.revoked_at_ms.unwrap_or(now_ms()),
                    }),
                ),
                Ok(None) => error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "unknown deviceId/role",
                    None,
                ),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "system-event" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let text = params
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if text.is_empty() {
                return error_response(&req.id, error_codes::INVALID_REQUEST, "text required", None);
            }

            let session_key = "main";

            let roles = params.get("roles").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            });
            let scopes = params.get("scopes").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            });
            let tags = params.get("tags").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            });

            // Update system presence table (OpenClaw parity).
            let presence_update = ctx
                .state
                .openclaw_update_system_presence(SystemPresencePayload {
                    text: text.clone(),
                    device_id: params
                        .get("deviceId")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    instance_id: params
                        .get("instanceId")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    host: params
                        .get("host")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    ip: params
                        .get("ip")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    version: params
                        .get("version")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    platform: params
                        .get("platform")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    device_family: params
                        .get("deviceFamily")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    model_identifier: params
                        .get("modelIdentifier")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    last_input_seconds: params
                        .get("lastInputSeconds")
                        .and_then(|v| v.as_u64()),
                    mode: params
                        .get("mode")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    reason: params
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    roles,
                    scopes,
                    tags,
                })
                .await;

            let is_node_presence_line = text.starts_with("Node:");
            if is_node_presence_line {
                let changed = std::collections::HashSet::<String>::from_iter(
                    presence_update.changed_keys.iter().cloned(),
                );
                let reason_value = presence_update.next.reason.clone().unwrap_or_default();
                let normalized_reason = reason_value.trim().to_lowercase();
                let ignore_reason =
                    normalized_reason.starts_with("periodic") || normalized_reason == "heartbeat";

                let host_changed = changed.contains("host");
                let ip_changed = changed.contains("ip");
                let version_changed = changed.contains("version");
                let mode_changed = changed.contains("mode");
                let reason_changed = changed.contains("reason") && !ignore_reason;
                let has_changes = host_changed
                    || ip_changed
                    || version_changed
                    || mode_changed
                    || reason_changed;

                if has_changes {
                    let context_changed = ctx
                        .state
                        .openclaw_is_system_event_context_changed(
                            session_key,
                            Some(&presence_update.key),
                        )
                        .await;

                    let mut parts: Vec<String> = Vec::new();
                    if context_changed || host_changed || ip_changed {
                        let host_label =
                            presence_update.next.host.clone().unwrap_or_else(|| "Unknown".to_string());
                        let ip_label = presence_update.next.ip.clone();
                        let node = if let Some(ip) = ip_label.as_deref() {
                            format!("Node: {} ({})", host_label.trim(), ip.trim())
                        } else {
                            format!("Node: {}", host_label.trim())
                        };
                        parts.push(node);
                    }
                    if version_changed {
                        let v = presence_update
                            .next
                            .version
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string());
                        parts.push(format!("app {}", v.trim()));
                    }
                    if mode_changed {
                        let m = presence_update
                            .next
                            .mode
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string());
                        parts.push(format!("mode {}", m.trim()));
                    }
                    if reason_changed {
                        let r = if reason_value.trim().is_empty() {
                            "event".to_string()
                        } else {
                            reason_value
                        };
                        parts.push(format!("reason {}", r.trim()));
                    }

                    let delta_text = parts.join(" · ");
                    if !delta_text.trim().is_empty() {
                        ctx.state
                            .openclaw_enqueue_system_event(
                                session_key,
                                &delta_text,
                                Some(&presence_update.key),
                            )
                            .await;
                    }
                }
            } else {
                ctx.state
                    .openclaw_enqueue_system_event(session_key, &text, None)
                    .await;
            }

            // system-event always triggers a presence broadcast in OpenClaw.
            broadcast_presence(&ctx.state, None).await;

            ok_response(&req.id, json!({ "ok": true }))
        }
        "talk.mode" => {
            let enabled = req
                .params
                .as_ref()
                .and_then(|v| v.get("enabled"))
                .and_then(|v| v.as_bool());
            let Some(enabled) = enabled else {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "enabled required",
                    None,
                );
            };
            let phase = req
                .params
                .as_ref()
                .and_then(|v| v.get("phase"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let payload = json!({
                "enabled": enabled,
                "phase": phase,
                "ts": now_ms(),
            });
            broadcast_openclaw_event(&ctx.state, "talk.mode", payload.clone(), None).await;
            ok_response(&req.id, payload)
        }
        "voicewake.get" => match handle_voicewake_get().await {
            Ok(payload) => ok_response(&req.id, payload),
            Err(err) => GatewayFrame::Res(ResponseFrame {
                id: req.id,
                ok: false,
                payload: None,
                error: Some(err),
            }),
        },
        "voicewake.set" => {
            let triggers = req
                .params
                .as_ref()
                .and_then(|v| v.get("triggers"))
                .and_then(|v| v.as_array());
            let Some(triggers) = triggers else {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "voicewake.set requires triggers: string[]",
                    None,
                );
            };

            match handle_voicewake_set(triggers).await {
                Ok(payload) => {
                    broadcast_openclaw_event(
                        &ctx.state,
                        "voicewake.changed",
                        payload.clone(),
                        None,
                    )
                    .await;
                    ok_response(&req.id, payload)
                }
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "tts.status" => match handle_tts_status(&ctx.state).await {
            Ok(payload) => ok_response(&req.id, payload),
            Err(err) => GatewayFrame::Res(ResponseFrame {
                id: req.id,
                ok: false,
                payload: None,
                error: Some(err),
            }),
        },
        "tts.providers" => match handle_tts_providers(&ctx.state).await {
            Ok(payload) => ok_response(&req.id, payload),
            Err(err) => GatewayFrame::Res(ResponseFrame {
                id: req.id,
                ok: false,
                payload: None,
                error: Some(err),
            }),
        },
        "tts.enable" => match handle_tts_enable(&ctx.state, true).await {
            Ok(payload) => ok_response(&req.id, payload),
            Err(err) => GatewayFrame::Res(ResponseFrame {
                id: req.id,
                ok: false,
                payload: None,
                error: Some(err),
            }),
        },
        "tts.disable" => match handle_tts_enable(&ctx.state, false).await {
            Ok(payload) => ok_response(&req.id, payload),
            Err(err) => GatewayFrame::Res(ResponseFrame {
                id: req.id,
                ok: false,
                payload: None,
                error: Some(err),
            }),
        },
        "tts.setProvider" => {
            let provider = req
                .params
                .as_ref()
                .and_then(|v| v.get("provider"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if provider.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "tts.setProvider requires provider",
                    None,
                );
            }
            match handle_tts_set_provider(provider).await {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "tts.convert" => {
            let text = req
                .params
                .as_ref()
                .and_then(|v| v.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if text.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "tts.convert requires text",
                    None,
                );
            }
            let channel = req
                .params
                .as_ref()
                .and_then(|v| v.get("channel"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            match handle_tts_convert(&ctx.state, text, channel).await {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "wizard.start" => {
            let session_id = Uuid::new_v4().to_string();
            let step = json!({
                "id": "drbot-wizard-info",
                "type": "note",
                "title": "drbot",
                "message": "Wizard is not implemented in drbot's OpenClaw gateway yet. Use Config tab (config.get/config.set).",
                "executor": "gateway"
            });
            ctx.wizard_sessions
                .lock()
                .await
                .insert(session_id.clone(), WizardSessionState { step: 0 });
            ok_response(
                &req.id,
                json!({ "sessionId": session_id, "done": false, "step": step, "status": "running" }),
            )
        }
        "wizard.next" => {
            let session_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("sessionId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if session_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "sessionId required",
                    None,
                );
            }
            let mut sessions = ctx.wizard_sessions.lock().await;
            if sessions.remove(&session_id).is_none() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "wizard not found",
                    None,
                );
            }
            ok_response(&req.id, json!({ "done": true, "status": "done" }))
        }
        "wizard.cancel" => {
            let session_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("sessionId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if session_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "sessionId required",
                    None,
                );
            }
            let mut sessions = ctx.wizard_sessions.lock().await;
            if sessions.remove(&session_id).is_none() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "wizard not found",
                    None,
                );
            }
            ok_response(&req.id, json!({ "status": "cancelled" }))
        }
        "wizard.status" => {
            let session_id = req
                .params
                .as_ref()
                .and_then(|v| v.get("sessionId"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if session_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "sessionId required",
                    None,
                );
            }
            let sessions = ctx.wizard_sessions.lock().await;
            if !sessions.contains_key(&session_id) {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "wizard not found",
                    None,
                );
            }
            ok_response(&req.id, json!({ "status": "running" }))
        }
        "system-presence" => ok_response(
            &req.id,
            json!(list_system_presence(&ctx.state, "gateway").await),
        ),
        "channels.status" => ok_response(
            &req.id,
            build_channels_snapshot(now_ms(), &ctx.state).await,
        ),
        "channels.logout" => {
            let channel = req
                .params
                .as_ref()
                .and_then(|v| v.get("channel"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if channel.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid channels.logout params: channel required",
                    None,
                );
            }
            let normalized = channel.to_lowercase();
            if normalized == "whatsapp" {
                // WhatsApp bridge disconnect performs an actual logout.
                let _ = ctx.state.channel_manager().stop_channel("whatsapp").await;
                ctx.state.openclaw_web_login().reset_whatsapp();
                ok_response(&req.id, json!({ "cleared": true, "loggedOut": true }))
            } else {
                error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    &format!("channel {} does not support logout", channel),
                    None,
                )
            }
        }
        "web.login.start" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let _force = params.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
            let timeout_ms = params
                .get("timeoutMs")
                .and_then(|v| v.as_u64())
                .unwrap_or(15_000)
                .min(120_000)
                .max(500);

            if !ctx.state.channel_manager().has_channel("whatsapp")
                || !ctx.state.channel_manager().is_enabled("whatsapp")
                || !ctx.state.channel_manager().is_configured("whatsapp")
            {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "web login provider is not available",
                    None,
                );
            }

            // Ensure a clean session before starting QR login.
            let _ = ctx.state.channel_manager().stop_channel("whatsapp").await;
            ctx.state.openclaw_web_login().reset_whatsapp();

            if let Err(e) = ctx.state.channel_manager().start_channel("whatsapp").await {
                return error_response(
                    &req.id,
                    error_codes::UNAVAILABLE,
                    &format!("failed to start whatsapp: {}", e),
                    None,
                );
            }

            let mut rx = ctx.state.openclaw_web_login().subscribe_whatsapp();
            let wait_fut = async {
                loop {
                    let cur = rx.borrow().clone();
                    if cur.connected || cur.qr_data_url.is_some() {
                        return cur;
                    }
                    if rx.changed().await.is_err() {
                        return rx.borrow().clone();
                    }
                }
            };
            let snapshot = tokio::time::timeout(Duration::from_millis(timeout_ms), wait_fut)
                .await
                .unwrap_or_else(|_| rx.borrow().clone());

            let mut obj = serde_json::Map::new();
            if snapshot.connected {
                obj.insert("message".to_string(), json!("Connected"));
            } else if let Some(url) = snapshot.qr_data_url.clone() {
                obj.insert("qrDataUrl".to_string(), json!(url));
                obj.insert(
                    "message".to_string(),
                    json!("Scan the QR code with WhatsApp to connect."),
                );
            } else {
                obj.insert("message".to_string(), json!("Waiting for QR code..."));
            }
            ok_response(&req.id, serde_json::Value::Object(obj))
        }
        "web.login.wait" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let timeout_ms = params
                .get("timeoutMs")
                .and_then(|v| v.as_u64())
                .unwrap_or(60_000)
                .min(300_000);

            if !ctx.state.channel_manager().has_channel("whatsapp")
                || !ctx.state.channel_manager().is_enabled("whatsapp")
                || !ctx.state.channel_manager().is_configured("whatsapp")
            {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "web login provider is not available",
                    None,
                );
            }

            let mut rx = ctx.state.openclaw_web_login().subscribe_whatsapp();
            if rx.borrow().connected {
                let _ = ctx.state.channel_manager().start_channel("whatsapp").await;
                return ok_response(
                    &req.id,
                    json!({ "connected": true, "message": "Connected" }),
                );
            }

            let wait_fut = async {
                loop {
                    let cur = rx.borrow().clone();
                    if cur.connected {
                        return true;
                    }
                    if rx.changed().await.is_err() {
                        return rx.borrow().connected;
                    }
                }
            };
            let connected = tokio::time::timeout(Duration::from_millis(timeout_ms), wait_fut)
                .await
                .unwrap_or(false);
            if connected {
                let _ = ctx.state.channel_manager().start_channel("whatsapp").await;
                // Best-effort: ensure inbound bridge is active.
                crate::openclaw_inbound::start_inbound_bridge(ctx.state.clone()).await;
            }
            ok_response(
                &req.id,
                json!({
                    "connected": connected,
                    "message": if connected { "Connected" } else { "Waiting for connection" }
                }),
            )
        }
        "sessions.list" => ok_response(&req.id, handle_sessions_list(&ctx.state).await),
        "sessions.preview" => {
            let keys = req
                .params
                .as_ref()
                .and_then(|v| v.get("keys"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            let limit = req
                .params
                .as_ref()
                .and_then(|v| v.get("limit"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(12)
                .max(1);
            let max_chars = req
                .params
                .as_ref()
                .and_then(|v| v.get("maxChars"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(240)
                .max(20);
            ok_response(
                &req.id,
                handle_sessions_preview(&ctx.state, &keys, limit, max_chars).await,
            )
        }
        "sessions.resolve" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let key = params
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let session_id = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let label = params
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            let has_key = !key.is_empty();
            let has_session_id = !session_id.is_empty();
            let has_label = !label.is_empty();
            let selection_count = [has_key, has_session_id, has_label]
                .into_iter()
                .filter(|v| *v)
                .count();
            if selection_count > 1 {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "Provide either key, sessionId, or label (not multiple)",
                    None,
                );
            }
            if selection_count == 0 {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "Either key, sessionId, or label is required",
                    None,
                );
            }

            let Some(store) = ctx.state.session_store() else {
                return error_response(
                    &req.id,
                    error_codes::UNAVAILABLE,
                    "session store not configured",
                    None,
                );
            };

            if has_key {
                let (channel_type, channel_id) = session_key_to_channel(&key);
                let existing = store
                    .get_by_channel(&channel_type, &channel_id)
                    .await
                    .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()));
                match existing {
                    Ok(Some(_)) => return ok_response(&req.id, json!({ "ok": true, "key": key })),
                    Ok(None) => {
                        return error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            &format!("No session found: {}", key),
                            None,
                        );
                    }
                    Err(err) => {
                        return GatewayFrame::Res(ResponseFrame {
                            id: req.id,
                            ok: false,
                            payload: None,
                            error: Some(err),
                        });
                    }
                }
            }

            if has_session_id {
                // OpenClaw parity: allow resolving by UUID sessionId or by key string.
                if let Ok(session_uuid) = Uuid::parse_str(&session_id) {
                    match store.get(session_uuid).await {
                        Ok(Some(session)) => {
                            let key = if session.channel_type == "openclaw" {
                                session.channel_id
                            } else {
                                format!("{}:{}", session.channel_type, session.channel_id)
                            };
                            return ok_response(&req.id, json!({ "ok": true, "key": key }));
                        }
                        Ok(None) => {
                            return error_response(
                                &req.id,
                                error_codes::INVALID_REQUEST,
                                &format!("No session found: {}", session_id),
                                None,
                            );
                        }
                        Err(e) => {
                            return error_response(
                                &req.id,
                                error_codes::UNAVAILABLE,
                                "session store unavailable",
                                Some(json!({ "error": e.to_string() })),
                            );
                        }
                    }
                }

                let (channel_type, channel_id) = session_key_to_channel(&session_id);
                match store.get_by_channel(&channel_type, &channel_id).await {
                    Ok(Some(_)) => {
                        return ok_response(&req.id, json!({ "ok": true, "key": session_id }))
                    }
                    Ok(None) => {
                        return error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            &format!("No session found: {}", session_id),
                            None,
                        );
                    }
                    Err(e) => {
                        return error_response(
                            &req.id,
                            error_codes::UNAVAILABLE,
                            "session store unavailable",
                            Some(json!({ "error": e.to_string() })),
                        );
                    }
                }
            }

            // label lookup (OpenClaw parity).
            let list = store
                .list(drbot_sessions::ListOptions::default())
                .await
                .map_err(|e| ErrorShape::new(error_codes::UNAVAILABLE, e.to_string()));
            let list = match list {
                Ok(v) => v,
                Err(err) => {
                    return GatewayFrame::Res(ResponseFrame {
                        id: req.id,
                        ok: false,
                        payload: None,
                        error: Some(err),
                    });
                }
            };

            let matches: Vec<String> = list
                .into_iter()
                .filter(|s| s.title.as_deref() == Some(label.as_str()))
                .map(|s| {
                    if s.channel_type == "openclaw" {
                        s.channel_id
                    } else {
                        format!("{}:{}", s.channel_type, s.channel_id)
                    }
                })
                .collect();

            if matches.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    &format!("No session found with label: {}", label),
                    None,
                );
            }
            if matches.len() > 1 {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    &format!(
                        "Multiple sessions found with label: {} ({})",
                        label,
                        matches.join(", ")
                    ),
                    None,
                );
            }
            ok_response(
                &req.id,
                json!({ "ok": true, "key": matches[0].clone() }),
            )
        }
        "sessions.patch" => {
            let patch = req.params.clone().unwrap_or_else(|| json!({}));
            let key = patch
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if key.is_empty() {
                return error_response(&req.id, error_codes::INVALID_REQUEST, "key required", None);
            }
            match handle_sessions_patch(&ctx.state, &key, &patch).await {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "sessions.reset" => {
            let key = req
                .params
                .as_ref()
                .and_then(|v| v.get("key"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if key.is_empty() {
                return error_response(&req.id, error_codes::INVALID_REQUEST, "key required", None);
            }
            match handle_sessions_reset(&ctx.state, &key).await {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "sessions.delete" => {
            let key = req
                .params
                .as_ref()
                .and_then(|v| v.get("key"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if key.is_empty() {
                return error_response(&req.id, error_codes::INVALID_REQUEST, "key required", None);
            }
            match handle_sessions_delete(&ctx.state, &key).await {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "sessions.compact" => {
            let key = req
                .params
                .as_ref()
                .and_then(|v| v.get("key"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if key.is_empty() {
                return error_response(&req.id, error_codes::INVALID_REQUEST, "key required", None);
            }
            let max_lines = req
                .params
                .as_ref()
                .and_then(|v| v.get("maxLines"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(400)
                .max(1);
            match handle_sessions_compact(&ctx.state, &key, max_lines).await {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "config.get" => ok_response(&req.id, handle_config_get().await),
        "config.schema" => ok_response(&req.id, handle_config_schema().await),
        "config.set" => {
            let raw = req
                .params
                .as_ref()
                .and_then(|v| v.get("raw"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if raw.trim().is_empty() {
                return error_response(&req.id, error_codes::INVALID_REQUEST, "raw required", None);
            }
            let base_hash = req
                .params
                .as_ref()
                .and_then(|v| v.get("baseHash"))
                .and_then(|v| v.as_str());
            match handle_config_set(raw, base_hash).await {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "config.apply" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let raw = params.get("raw").and_then(|v| v.as_str()).unwrap_or("");
            if raw.trim().is_empty() {
                return error_response(&req.id, error_codes::INVALID_REQUEST, "raw required", None);
            }
            let base_hash = params.get("baseHash").and_then(|v| v.as_str());
            let session_key = params
                .get("sessionKey")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let note = params
                .get("note")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let restart_delay_ms = params
                .get("restartDelayMs")
                .and_then(|v| v.as_u64())
                .map(|v| v.min(60_000))
                .unwrap_or(2_000);

            match handle_config_set(raw, base_hash).await {
                Ok(payload) => {
                    let path = payload
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let config = payload.get("config").cloned().unwrap_or_else(|| json!({}));
                    let sentinel_payload = json!({
                        "kind": "config-apply",
                        "status": "ok",
                        "ts": now_ms(),
                        "sessionKey": session_key,
                        "message": note,
                        "doctorHint": "Restart drbot to apply config changes.",
                        "stats": {
                            "mode": "config.apply",
                            "root": path,
                        },
                    });

                    ok_response(
                        &req.id,
                        json!({
                            "ok": true,
                            "path": path,
                            "config": config,
                            "restart": {
                                "ok": false,
                                "pid": std::process::id(),
                                "signal": "SIGUSR1",
                                "delayMs": restart_delay_ms,
                                "reason": "config.apply",
                                "mode": "signal",
                            },
                            "sentinel": {
                                "path": serde_json::Value::Null,
                                "payload": sentinel_payload,
                            }
                        }),
                    )
                }
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "config.patch" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let raw = params.get("raw").and_then(|v| v.as_str()).unwrap_or("");
            if raw.trim().is_empty() {
                return error_response(&req.id, error_codes::INVALID_REQUEST, "raw required", None);
            }
            let base_hash = params.get("baseHash").and_then(|v| v.as_str());
            let session_key = params
                .get("sessionKey")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let note = params
                .get("note")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let restart_delay_ms = params
                .get("restartDelayMs")
                .and_then(|v| v.as_u64())
                .map(|v| v.min(60_000))
                .unwrap_or(2_000);

            match handle_config_patch(raw, base_hash).await {
                Ok(payload) => {
                    let path = payload
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let config = payload.get("config").cloned().unwrap_or_else(|| json!({}));
                    let sentinel_payload = json!({
                        "kind": "config-apply",
                        "status": "ok",
                        "ts": now_ms(),
                        "sessionKey": session_key,
                        "message": note,
                        "doctorHint": "Restart drbot to apply config changes.",
                        "stats": {
                            "mode": "config.patch",
                            "root": path,
                        },
                    });

                    ok_response(
                        &req.id,
                        json!({
                            "ok": true,
                            "path": path,
                            "config": config,
                            "restart": {
                                "ok": false,
                                "pid": std::process::id(),
                                "signal": "SIGUSR1",
                                "delayMs": restart_delay_ms,
                                "reason": "config.patch",
                                "mode": "signal",
                            },
                            "sentinel": {
                                "path": serde_json::Value::Null,
                                "payload": sentinel_payload,
                            }
                        }),
                    )
                }
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "wake" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let mode = params
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let text = params
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if text.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid wake params: text required",
                    None,
                );
            }
            if mode != "now" && mode != "next-heartbeat" {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid wake params: mode must be now or next-heartbeat",
                    None,
                );
            }

            enqueue_openclaw_system_event(&ctx.state, text).await;
            if mode == "now" {
                crate::openclaw_heartbeat::request_heartbeat_now(
                    &ctx.state,
                    Some("wake".to_string()),
                )
                .await;
            }

            ok_response(&req.id, json!({ "ok": true }))
        }
        "cron.status" => {
            let svc = cron_service_for_state(&ctx.state).await;
            ok_response(&req.id, svc.status().await)
        }
        "cron.list" => {
            let raw = req.params.clone().unwrap_or_else(|| json!({}));
            let params: CronListParams = match serde_json::from_value(raw) {
                Ok(p) => p,
                Err(e) => {
                    return error_response(
                        &req.id,
                        error_codes::INVALID_REQUEST,
                        &format!("invalid cron.list params: {}", e),
                        None,
                    );
                }
            };
            let include_disabled = params.include_disabled.unwrap_or(false);
            let svc = cron_service_for_state(&ctx.state).await;
            let jobs = svc.list(include_disabled).await;
            ok_response(&req.id, json!({ "jobs": jobs }))
        }
        "cron.add" => {
            let raw = req.params.clone().unwrap_or_else(|| json!({}));
            let params: CronAddParams = match serde_json::from_value(raw) {
                Ok(p) => p,
                Err(e) => {
                    return error_response(
                        &req.id,
                        error_codes::INVALID_REQUEST,
                        &format!("invalid cron.add params: {}", e),
                        None,
                    );
                }
            };
            let svc = cron_service_for_state(&ctx.state).await;
            match svc.add(&ctx.state, params).await {
                Ok(job) => ok_response(&req.id, json!(job)),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "cron.update" => {
            let raw = req.params.clone().unwrap_or_else(|| json!({}));
            let params: CronUpdateParams = match serde_json::from_value(raw) {
                Ok(p) => p,
                Err(e) => {
                    return error_response(
                        &req.id,
                        error_codes::INVALID_REQUEST,
                        &format!("invalid cron.update params: {}", e),
                        None,
                    );
                }
            };
            let job_id = params
                .id
                .or(params.job_id)
                .unwrap_or_default()
                .trim()
                .to_string();
            if job_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid cron.update params: missing id",
                    None,
                );
            }
            let svc = cron_service_for_state(&ctx.state).await;
            match svc.update(&ctx.state, &job_id, params.patch).await {
                Ok(job) => ok_response(&req.id, json!(job)),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "cron.remove" => {
            let raw = req.params.clone().unwrap_or_else(|| json!({}));
            let params: CronRemoveParams = match serde_json::from_value(raw) {
                Ok(p) => p,
                Err(e) => {
                    return error_response(
                        &req.id,
                        error_codes::INVALID_REQUEST,
                        &format!("invalid cron.remove params: {}", e),
                        None,
                    );
                }
            };
            let job_id = params
                .id
                .or(params.job_id)
                .unwrap_or_default()
                .trim()
                .to_string();
            if job_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid cron.remove params: missing id",
                    None,
                );
            }
            let svc = cron_service_for_state(&ctx.state).await;
            match svc.remove(&ctx.state, &job_id).await {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "cron.run" => {
            let raw = req.params.clone().unwrap_or_else(|| json!({}));
            let params: CronRunParams = match serde_json::from_value(raw) {
                Ok(p) => p,
                Err(e) => {
                    return error_response(
                        &req.id,
                        error_codes::INVALID_REQUEST,
                        &format!("invalid cron.run params: {}", e),
                        None,
                    );
                }
            };
            let job_id = params
                .id
                .or(params.job_id)
                .unwrap_or_default()
                .trim()
                .to_string();
            if job_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid cron.run params: missing id",
                    None,
                );
            }
            let mode = params
                .mode
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            if let Some(mode) = mode {
                if mode != "due" && mode != "force" {
                    return error_response(
                        &req.id,
                        error_codes::INVALID_REQUEST,
                        "invalid cron.run params: mode must be due or force",
                        None,
                    );
                }
            }
            let svc = cron_service_for_state(&ctx.state).await;
            match svc.run(&ctx.state, &job_id, mode).await {
                Ok(payload) => ok_response(&req.id, payload),
                Err(err) => GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: false,
                    payload: None,
                    error: Some(err),
                }),
            }
        }
        "cron.runs" => {
            let raw = req.params.clone().unwrap_or_else(|| json!({}));
            let params: CronRunsParams = match serde_json::from_value(raw) {
                Ok(p) => p,
                Err(e) => {
                    return error_response(
                        &req.id,
                        error_codes::INVALID_REQUEST,
                        &format!("invalid cron.runs params: {}", e),
                        None,
                    );
                }
            };
            let job_id = params
                .id
                .or(params.job_id)
                .unwrap_or_default()
                .trim()
                .to_string();
            if job_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid cron.runs params: missing id",
                    None,
                );
            }
            let limit = params.limit.unwrap_or(200) as usize;
            let svc = cron_service_for_state(&ctx.state).await;
            let log_path = resolve_cron_run_log_path(&svc.store_path, &job_id);
            let entries = read_cron_run_log_entries(&log_path, &job_id, limit);
            ok_response(&req.id, json!({ "entries": entries }))
        }
        "update.run" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let session_key = params
                .get("sessionKey")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let note = params
                .get("note")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let restart_delay_ms = params
                .get("restartDelayMs")
                .and_then(|v| v.as_u64())
                .map(|v| v.min(60_000))
                .unwrap_or(2_000);

            let result = json!({
                "status": "error",
                "mode": "unknown",
                "reason": "update.run is not supported by drbot (OpenClaw compatibility endpoint only)",
                "steps": [],
                "durationMs": 0,
            });

            let sentinel_payload = json!({
                "kind": "update",
                "status": "error",
                "ts": now_ms(),
                "sessionKey": session_key,
                "message": note,
                "doctorHint": "Update is not implemented for drbot. Update via your package manager / git checkout.",
                "stats": {
                    "mode": "unknown",
                    "root": serde_json::Value::Null,
                    "before": serde_json::Value::Null,
                    "after": serde_json::Value::Null,
                    "steps": [],
                    "reason": "unsupported",
                    "durationMs": 0
                }
            });

            ok_response(
                &req.id,
                json!({
                    "ok": true,
                    "result": result,
                    "restart": {
                        "ok": false,
                        "pid": std::process::id(),
                        "signal": "SIGUSR1",
                        "delayMs": restart_delay_ms,
                        "reason": "update.run",
                        "mode": "signal",
                    },
                    "sentinel": {
                        "path": serde_json::Value::Null,
                        "payload": sentinel_payload,
                    }
                }),
            )
        }
        "chat.history" => {
            let session_key = req
                .params
                .as_ref()
                .and_then(|v| v.get("sessionKey"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if session_key.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "sessionKey required",
                    None,
                );
            }
            let limit = req
                .params
                .as_ref()
                .and_then(|v| v.get("limit"))
                .and_then(|v| v.as_u64());
            ok_response(
                &req.id,
                handle_chat_history(&ctx.state, session_key, limit).await,
            )
        }
        "chat.send" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let session_key = params
                .get("sessionKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let message = params.get("message").and_then(|v| v.as_str()).unwrap_or("");
            let run_id = params
                .get("idempotencyKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let attachments = params
                .get("attachments")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            if session_key.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "sessionKey required",
                    None,
                );
            }
            if run_id.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "idempotencyKey required",
                    None,
                );
            }
            if message.trim().is_empty() && attachments.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "message or attachment required",
                    None,
                );
            }

            if is_chat_stop_command_text(message) {
                let run_ids = ctx
                    .state
                    .openclaw_abort_chat_runs(&session_key, None, "stop")
                    .await;
                return ok_response(
                    &req.id,
                    json!({
                        "ok": true,
                        "aborted": !run_ids.is_empty(),
                        "runIds": run_ids,
                    }),
                );
            }

            let dedupe_key = chat_send_dedupe_key(&session_key, &run_id);
            if let Some(entry) = openclaw_dedupe_get(&dedupe_key).await {
                return GatewayFrame::Res(ResponseFrame {
                    id: req.id,
                    ok: entry.ok,
                    payload: entry.payload,
                    error: entry.error,
                });
            }

            if ctx.state.openclaw_has_chat_run(&run_id).await {
                return ok_response(&req.id, json!({ "runId": run_id, "status": "in_flight" }));
            }

            let provider = match ctx.state.provider() {
                Some(p) => p.clone(),
                None => {
                    return error_response(
                        &req.id,
                        error_codes::UNAVAILABLE,
                        "No AI provider configured",
                        None,
                    );
                }
            };

            let user_msg = openclaw_user_message_to_drbot(message, &attachments);
            let (cancel_tx, cancel_rx) = watch::channel::<Option<String>>(None);
            let inserted = ctx
                .state
                .openclaw_try_register_chat_run(
                    &run_id,
                    crate::state::OpenclawChatRun {
                        session_key: session_key.clone(),
                        run_id: run_id.clone(),
                        cancel_tx: cancel_tx.clone(),
                        started_at_ms: now_ms(),
                    },
                )
                .await;
            if !inserted {
                return ok_response(&req.id, json!({ "runId": run_id, "status": "in_flight" }));
            }

            tokio::spawn(spawn_chat_run(ctx.clone(), provider, run_id.clone(), session_key, user_msg, cancel_rx));
            ok_response(&req.id, json!({ "runId": run_id, "status": "started" }))
        }
        "chat.abort" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let session_key = params
                .get("sessionKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let run_id = params
                .get("runId")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string());

            if session_key.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "sessionKey required",
                    None,
                );
            }

            // OpenClaw parity: if runId is present and active but doesn't match the provided
            // sessionKey, return an INVALID_REQUEST error instead of silently ignoring it.
            if let Some(run_id) = run_id.as_deref() {
                if let Some(found_session) = ctx.state.openclaw_find_chat_run_session_key(run_id).await
                {
                    if found_session != session_key {
                        return error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            "runId does not match sessionKey",
                            None,
                        );
                    }
                }
            }

            let run_ids = ctx
                .state
                .openclaw_abort_chat_runs(&session_key, run_id.as_deref(), "rpc")
                .await;

            if let Some(run_id) = run_id {
                let aborted = run_ids.iter().any(|id| id == &run_id);
                ok_response(
                    &req.id,
                    json!({ "ok": true, "aborted": aborted, "runIds": if aborted { vec![run_id] } else { Vec::<String>::new() } }),
                )
            } else {
                ok_response(
                    &req.id,
                    json!({ "ok": true, "aborted": !run_ids.is_empty(), "runIds": run_ids }),
                )
            }
        }
        "chat.inject" => {
            let params = req.params.clone().unwrap_or_else(|| json!({}));
            let session_key = params
                .get("sessionKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let message = params
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let label = params
                .get("label")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if session_key.is_empty() || message.is_empty() {
                return error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "chat.inject requires sessionKey and message",
                    None,
                );
            }

            let Some(store) = ctx.state.session_store() else {
                return error_response(
                    &req.id,
                    error_codes::UNAVAILABLE,
                    "session store not configured",
                    None,
                );
            };

            let message_id = Uuid::new_v4().to_string();
            let message_id: String = message_id.chars().take(8).collect();
            let label_prefix = label
                .as_deref()
                .map(|l| format!("[{}]\n\n", l.chars().take(100).collect::<String>()))
                .unwrap_or_default();
            let combined = format!("{}{}", label_prefix, message);

            // Persist to the session transcript so chat.history shows it later.
            // Stable operator user id.
            let user_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"openclaw-operator");
            let mut session = store
                .get_by_channel("openclaw", &session_key)
                .await
                .ok()
                .flatten();
            if session.is_none() {
                let (channel_type, channel_id) = session_key_to_channel(&session_key);
                session = store.get_or_create(user_id, &channel_type, &channel_id).await.ok();
            }
            if let Some(mut s) = session {
                s.add_message(Message::assistant(&combined));
                s.update_timestamp();
                let _ = store.update(&s).await;
            }

            // Broadcast to websocket clients for immediate UI update (OpenClaw parity).
            let now = now_ms();
            let transcript_message = json!({
                "role": "assistant",
                "content": [{ "type": "text", "text": combined }],
                "timestamp": now,
                "stopReason": "injected",
                "usage": { "input": 0, "output": 0, "totalTokens": 0 },
            });
            let chat_payload = json!({
                "runId": format!("inject-{}", message_id),
                "sessionKey": session_key,
                "seq": 0,
                "state": "final",
                "message": transcript_message,
            });
            broadcast_openclaw_event(&ctx.state, "chat", chat_payload, None).await;

            ok_response(&req.id, json!({ "ok": true, "messageId": message_id }))
        }
        "connect" => error_response(
            &req.id,
            error_codes::INVALID_REQUEST,
            "connect is only valid as the first request",
            None,
        ),
        other => error_response(
            &req.id,
            error_codes::INVALID_REQUEST,
            &format!("unknown method: {}", other),
            None,
        ),
    }
}

fn method_requires_global_scope(method: &str) -> bool {
    match method {
        // Always available.
        "health" | "status" | "logs.tail" | "models.list" => false,
        // Read-only / discovery.
        "last-heartbeat"
        | "system-presence"
        | "usage.status"
        | "usage.cost"
        | "voicewake.get"
        | "tts.status"
        | "tts.providers"
        | "config.get"
        | "config.schema"
        | "sessions.list"
        | "sessions.preview"
        | "sessions.resolve"
        | "agents.list"
        | "agent.identity.get"
        | "agents.files.list"
        | "agents.files.get"
        | "skills.status"
        | "skills.bins"
        | "channels.status"
        | "web.login.wait"
        | "chat.history"
        | "cron.status"
        | "cron.list"
        | "cron.runs"
        | "node.list"
        | "node.describe"
        | "device.pair.list"
        | "exec.approvals.get"
        | "exec.approvals.node.get"
        | "wizard.status" => false,
        _ => true,
    }
}

fn node_method_allowed(method: &str) -> bool {
    matches!(
        method,
        "health"
            | "status"
            | "logs.tail"
            | "system-event"
            | "system-presence"
            | "node.pair.request"
            | "node.pair.verify"
            | "node.invoke.result"
            | "node.event"
            | "exec.approval.request"
    )
}

async fn authorize_openclaw_request(ctx: &ConnCtx, req: &RequestFrame) -> Option<GatewayFrame> {
    let Some(client) = ctx.state.get_openclaw_client(&ctx.conn_id).await else {
        return Some(error_response(
            &req.id,
            error_codes::FORBIDDEN,
            "client not connected",
            None,
        ));
    };

    if client.role == "node" {
        if node_method_allowed(req.method.as_str()) {
            return None;
        }
        return Some(error_response(
            &req.id,
            error_codes::FORBIDDEN,
            "method not allowed for role node",
            Some(json!({ "method": req.method, "role": client.role })),
        ));
    }

    if method_requires_global_scope(req.method.as_str()) {
        let has_global = client.scopes.iter().any(|s| s == "global");
        if !has_global {
            return Some(error_response(
                &req.id,
                error_codes::FORBIDDEN,
                "scope 'global' required",
                Some(json!({ "requiredScopes": ["global"], "scopes": client.scopes })),
            ));
        }
    }

    None
}

/// Handle a new OpenClaw-compatible WebSocket connection.
pub async fn handle_socket(socket: WebSocket, state: GatewayState, peer: SocketAddr) {
    info!(%peer, "OpenClaw WS connected");
    let conn_id = Uuid::new_v4().to_string();
    state
        .openclaw_logs()
        .push_line(&format!(
            "openclaw: ws connected peer={} conn_id={}",
            peer, conn_id
        ))
        .await;

    let (ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<OpenclawOutbound>();
    let queued_bytes = Arc::new(AtomicU64::new(0));
    let closing = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let state_for_sender = state.clone();
    let conn_id_for_sender = conn_id.clone();
    let queued_bytes_for_sender = queued_bytes.clone();
    let closing_for_sender = closing.clone();
    let sender_task = tokio::spawn(async move {
        use futures::SinkExt;
        let mut ws_sender = ws_sender;
        while let Some(out) = rx.recv().await {
            match out {
                OpenclawOutbound::Text(msg) => {
                    let msg_len = msg.len() as u64;

                    // Best-effort: keep `logs.tail` useful without leaking full payload bodies.
                    let line = match serde_json::from_str::<GatewayFrame>(&msg) {
                        Ok(GatewayFrame::Event(evt)) => format!(
                            "openclaw: ws send event={} seq={}",
                            evt.event,
                            evt.seq.unwrap_or(0)
                        ),
                        Ok(GatewayFrame::Res(res)) => {
                            format!("openclaw: ws send res id={} ok={}", res.id, res.ok)
                        }
                        Ok(GatewayFrame::Req(req)) => {
                            format!("openclaw: ws send req id={} method={}", req.id, req.method)
                        }
                        Err(_) => format!("openclaw: ws send text len={}", msg.len()),
                    };
                    state_for_sender.openclaw_logs().push_line(&line).await;

                    if ws_sender
                        .send(axum::extract::ws::Message::Text(msg.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }

                    let _ = queued_bytes_for_sender.fetch_update(
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                        |cur| Some(cur.saturating_sub(msg_len)),
                    );
                }
                OpenclawOutbound::Close { code, reason } => {
                    closing_for_sender.store(true, Ordering::Relaxed);
                    queued_bytes_for_sender.store(0, Ordering::Relaxed);
                    let frame = axum::extract::ws::CloseFrame {
                        code,
                        reason: axum::extract::ws::Utf8Bytes::from(reason),
                    };
                    let _ = ws_sender
                        .send(axum::extract::ws::Message::Close(Some(frame)))
                        .await;
                    break;
                }
            }
        }
    });

    let event_seq = Arc::new(AtomicU64::new(0));
    let run_seq = Arc::new(Mutex::new(HashMap::<String, u64>::new()));
    let wizard_sessions = Arc::new(Mutex::new(HashMap::<String, WizardSessionState>::new()));
    let (shutdown_tx, _shutdown_rx) = watch::channel(false);

    // Send connect.challenge immediately (OpenClaw convention).
    let connect_nonce = Uuid::new_v4().to_string();
    send_event(
        &tx,
        &queued_bytes,
        &closing,
        &event_seq,
        "connect.challenge",
        json!({ "nonce": connect_nonce, "ts": now_ms() }),
        None,
        false,
    )
    .await;

    let ctx = ConnCtx {
        state: state.clone(),
        tx: tx.clone(),
        queued_bytes: queued_bytes.clone(),
        closing: closing.clone(),
        event_seq: event_seq.clone(),
        run_seq,
        wizard_sessions,
        shutdown_tx: shutdown_tx.clone(),
        conn_id: conn_id.clone(),
        peer,
    };

    let mut connected = false;
    let mut tick_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut registered = false;
    let mut connected_role: Option<String> = None;
    let mut connected_node_id: Option<String> = None;
    let mut device_pair_request: Option<DevicePairingPendingRequest> = None;

    let idle_timeout_ms = std::env::var("DRBOT_OPENCLAW_IDLE_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v >= 1000);
    let handshake_timeout_ms = resolve_openclaw_handshake_timeout_ms();
    let handshake_deadline =
        std::time::Instant::now() + Duration::from_millis(handshake_timeout_ms.max(1));
    let max_payload_bytes = resolve_openclaw_max_payload_bytes();

    loop {
        let maybe_msg = if !connected {
            let now = std::time::Instant::now();
            if now >= handshake_deadline {
                warn!(%peer, handshake_timeout_ms, "OpenClaw WS handshake timeout");
                state
                    .openclaw_logs()
                    .push_line(&format!(
                        "openclaw: ws handshake timeout peer={} conn_id={} timeout_ms={}",
                        peer, conn_id, handshake_timeout_ms
                    ))
                    .await;
                openclaw_request_close(&tx, &closing, 1000, "handshake timeout");
                break;
            }
            let remain = handshake_deadline.duration_since(now);
            match tokio::time::timeout(remain, ws_receiver.next()).await {
                Ok(v) => v,
                Err(_) => {
                    warn!(%peer, handshake_timeout_ms, "OpenClaw WS handshake timeout");
                    state
                        .openclaw_logs()
                        .push_line(&format!(
                            "openclaw: ws handshake timeout peer={} conn_id={} timeout_ms={}",
                            peer, conn_id, handshake_timeout_ms
                        ))
                        .await;
                    openclaw_request_close(&tx, &closing, 1000, "handshake timeout");
                    break;
                }
            }
        } else if let Some(timeout_ms) = idle_timeout_ms {
            match tokio::time::timeout(Duration::from_millis(timeout_ms), ws_receiver.next())
                .await
            {
                Ok(v) => v,
                Err(_) => {
                    warn!(%peer, timeout_ms, "OpenClaw WS idle timeout");
                    state
                        .openclaw_logs()
                        .push_line(&format!(
                            "openclaw: ws idle timeout peer={} conn_id={}",
                            peer, conn_id
                        ))
                        .await;
                    break;
                }
            }
        } else {
            ws_receiver.next().await
        };

        let Some(msg) = maybe_msg else {
            break;
        };
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                warn!(error = %e, "OpenClaw WS error");
                break;
            }
        };

        let text = match msg {
            axum::extract::ws::Message::Text(t) => {
                if t.len() as u64 > max_payload_bytes {
                    warn!(%peer, len = t.len(), "OpenClaw WS payload too large");
                    state
                        .openclaw_logs()
                        .push_line(&format!(
                            "openclaw: ws recv too large peer={} conn_id={} len={}",
                            peer,
                            conn_id,
                            t.len()
                        ))
                        .await;
                    openclaw_request_close(
                        &tx,
                        &closing,
                        WS_CLOSE_CODE_MESSAGE_TOO_BIG,
                        "payload too large",
                    );
                    break;
                }
                t.to_string()
            }
            axum::extract::ws::Message::Binary(b) => {
                if b.len() as u64 > max_payload_bytes {
                    warn!(%peer, len = b.len(), "OpenClaw WS payload too large (binary)");
                    state
                        .openclaw_logs()
                        .push_line(&format!(
                            "openclaw: ws recv too large peer={} conn_id={} len={}",
                            peer,
                            conn_id,
                            b.len()
                        ))
                        .await;
                    openclaw_request_close(
                        &tx,
                        &closing,
                        WS_CLOSE_CODE_MESSAGE_TOO_BIG,
                        "payload too large",
                    );
                    break;
                }
                match String::from_utf8(b.to_vec()) {
                    Ok(s) => s,
                    Err(_) => continue,
                }
            }
            axum::extract::ws::Message::Ping(_) | axum::extract::ws::Message::Pong(_) => continue,
            axum::extract::ws::Message::Close(_) => break,
        };

        debug!(%peer, len = text.len(), "OpenClaw WS recv");

        let parsed: Result<GatewayFrame, _> = serde_json::from_str(&text);
        let frame = match parsed {
            Ok(f) => f,
            Err(e) => {
                warn!(error = %e, "Invalid OpenClaw frame JSON");
                state
                    .openclaw_logs()
                    .push_line(&format!(
                        "openclaw: ws recv invalid_json peer={} conn_id={} err={}",
                        peer, conn_id, e
                    ))
                    .await;
                let err = error_response(
                    "invalid",
                    error_codes::INVALID_REQUEST,
                    "invalid frame",
                    None,
                );
                send_frame(&tx, &queued_bytes, &closing, &err).await;
                continue;
            }
        };

        let req = match frame {
            GatewayFrame::Req(r) => {
                state
                    .openclaw_logs()
                    .push_line(&format!(
                        "openclaw: ws recv req id={} method={} peer={} conn_id={}",
                        r.id, r.method, peer, conn_id
                    ))
                    .await;
                r
            }
            _ => {
                // OpenClaw server expects only req frames from clients.
                continue;
            }
        };

        if !connected {
            if req.method != "connect" {
                let err = error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "invalid handshake: first request must be connect",
                    None,
                );
                send_frame(&tx, &queued_bytes, &closing, &err).await;
                break;
            }

            let params_value = req.params.clone().unwrap_or_else(|| json!({}));
            let params: ConnectParams = match serde_json::from_value(params_value) {
                Ok(p) => p,
                Err(e) => {
                    let err = error_response(
                        &req.id,
                        error_codes::INVALID_REQUEST,
                        &format!("invalid connect params: {}", e),
                        None,
                    );
                    send_frame(&tx, &queued_bytes, &closing, &err).await;
                    break;
                }
            };

            if params.max_protocol < OPENCLAW_PROTOCOL_VERSION
                || params.min_protocol > OPENCLAW_PROTOCOL_VERSION
            {
                let err = error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    "protocol mismatch",
                    Some(json!({"expectedProtocol": OPENCLAW_PROTOCOL_VERSION})),
                );
                send_frame(&tx, &queued_bytes, &closing, &err).await;
                break;
            }

            let auth_token = params
                .auth
                .as_ref()
                .and_then(|a| a.token.as_ref())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let auth_password = params
                .auth
                .as_ref()
                .and_then(|a| a.password.as_ref())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let shared_provided = auth_token
                .as_deref()
                .or(auth_password.as_deref())
                .unwrap_or("");
            let shared_auth_ok = state.validate_token(shared_provided);
            let mut auth_ok = shared_auth_ok;

            let role_raw = params.role.clone().unwrap_or_else(|| "operator".to_string());
            let role_trimmed = role_raw.trim();
            let role = if role_trimmed.is_empty() || role_trimmed == "operator" {
                "operator".to_string()
            } else if role_trimmed == "node" {
                "node".to_string()
            } else {
                let err = error_response(
                    &req.id,
                    error_codes::INVALID_REQUEST,
                    &format!("invalid role: {}", role_trimmed),
                    None,
                );
                send_frame(&tx, &queued_bytes, &closing, &err).await;
                break;
            };
            let scopes_raw = params.scopes.clone().unwrap_or_default();
            let requested_scopes = normalize_scopes(&scopes_raw);

            // Device identity verification (OpenClaw parity).
            let is_local_client = peer.ip().is_loopback();
            let mut device_id: Option<String> = None;
            let mut device_public_key: Option<String> = None;
            if let Some(device) = params.device.as_ref().and_then(|v| v.as_object()) {
                let has_any = ["id", "publicKey", "signature"]
                    .into_iter()
                    .any(|k| {
                        device
                            .get(k)
                            .and_then(|v| v.as_str())
                            .map(|s| !s.trim().is_empty())
                            .unwrap_or(false)
                    });
                if has_any {
                    let raw_id = device
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let raw_public = device
                        .get("publicKey")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let raw_signature = device
                        .get("signature")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let signed_at_ms = device
                        .get("signedAt")
                        .and_then(|v| v.as_u64())
                        .or_else(|| {
                            device
                                .get("signedAt")
                                .and_then(|v| v.as_i64())
                                .and_then(|v| u64::try_from(v).ok())
                        })
                        .unwrap_or(0);

                    if raw_id.is_empty() || raw_public.is_empty() || raw_signature.is_empty() {
                        let err = error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            "device identity required",
                            None,
                        );
                        send_frame(&tx, &queued_bytes, &closing, &err).await;
                        break;
                    }

                    let public_key_raw = base64_decode_url_safe_best_effort(&raw_public);
                    let Some(public_key_raw) = public_key_raw.filter(|v| v.len() == 32) else {
                        let err = error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            "device public key invalid",
                            None,
                        );
                        send_frame(&tx, &queued_bytes, &closing, &err).await;
                        break;
                    };

                    let derived_id = sha256_hex_bytes(&public_key_raw);
                    if derived_id != raw_id {
                        let err = error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            "device identity mismatch",
                            None,
                        );
                        send_frame(&tx, &queued_bytes, &closing, &err).await;
                        break;
                    }

                    let now = now_ms();
                    let skew = if now > signed_at_ms {
                        now.saturating_sub(signed_at_ms)
                    } else {
                        signed_at_ms.saturating_sub(now)
                    };
                    if signed_at_ms == 0 || skew > DEVICE_SIGNATURE_SKEW_MS {
                        let err = error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            "device signature expired",
                            None,
                        );
                        send_frame(&tx, &queued_bytes, &closing, &err).await;
                        break;
                    }

                    let provided_nonce = device
                        .get("nonce")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let nonce_required = !is_local_client;
                    if nonce_required && provided_nonce.is_empty() {
                        let err = error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            "device nonce required",
                            None,
                        );
                        send_frame(&tx, &queued_bytes, &closing, &err).await;
                        break;
                    }
                    if !provided_nonce.is_empty() && provided_nonce != connect_nonce {
                        let err = error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            "device nonce mismatch",
                            None,
                        );
                        send_frame(&tx, &queued_bytes, &closing, &err).await;
                        break;
                    }
                    let nonce_opt = if provided_nonce.is_empty() {
                        None
                    } else {
                        Some(provided_nonce.as_str())
                    };

                    let payload = build_device_auth_payload(DeviceAuthPayloadParams {
                        device_id: &raw_id,
                        client_id: &params.client.id,
                        client_mode: &params.client.mode,
                        role: &role,
                        scopes: &requested_scopes,
                        signed_at_ms,
                        token: auth_token.as_deref(),
                        nonce: nonce_opt,
                    });

                    let sig_raw = base64_decode_url_safe_best_effort(&raw_signature);
                    let Some(sig_raw) = sig_raw.filter(|v| !v.is_empty()) else {
                        let err = error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            "device signature invalid",
                            None,
                        );
                        send_frame(&tx, &queued_bytes, &closing, &err).await;
                        break;
                    };

                    if !verify_device_signature(&public_key_raw, &payload, &sig_raw) {
                        let err = error_response(
                            &req.id,
                            error_codes::INVALID_REQUEST,
                            "device signature invalid",
                            None,
                        );
                        send_frame(&tx, &queued_bytes, &closing, &err).await;
                        break;
                    }

                    device_id = Some(raw_id);
                    device_public_key = Some(base64_encode_url_safe_no_pad(&public_key_raw));
                }
            }
            let client_id = params.client.id.clone();
            let client_mode = params.client.mode.clone();
            let client_version = params.client.version.clone();
            let platform = params.client.platform.clone();
            let device_family = params.client.device_family.clone();
            let model_identifier = params.client.model_identifier.clone();
            let display_name = params.client.display_name.clone();
            let instance_id = params.client.instance_id.clone();
            let caps = params.caps.clone().unwrap_or_default();
            let mut commands = params.commands.clone().unwrap_or_default();
            let permissions = params.permissions.clone().unwrap_or_default();
            let path_env = params.path_env.clone();
            let connected_at_ms = now_ms();
            // Back-compat: treat missing scopes as "global" for operator clients that are
            // not using signed device identity. For signed connections, scopes are part of
            // the signature payload; do not silently widen them.
            let scopes = if role == "operator" && requested_scopes.is_empty() && device_id.is_none() {
                vec!["global".to_string()]
            } else {
                requested_scopes
            };

            // OpenClaw parity: nodes must only declare allowlisted commands for their platform.
            if role == "node" {
                let allowlist = resolve_node_command_allowlist(&platform, device_family.as_deref());
                commands = commands
                    .into_iter()
                    .map(|c| c.trim().to_string())
                    .filter(|c| !c.is_empty() && allowlist.contains(c))
                    .collect();
            }
            let openclaw_client = OpenclawClient {
                conn_id: conn_id.clone(),
                peer,
                client_id,
                client_mode,
                client_version,
                platform,
                device_family,
                model_identifier,
                display_name,
                instance_id,
                device_id,
                role,
                scopes,
                caps,
                commands,
                permissions,
                path_env,
                connected_at_ms: connected_at_ms,
                tx: tx.clone(),
                queued_bytes: queued_bytes.clone(),
                closing: closing.clone(),
                event_seq: event_seq.clone(),
            };

            // Device-token auth fallback (OpenClaw parity): if the gateway shared token fails,
            // allow a previously issued device token for a paired device.
            if !auth_ok {
                if let (Some(token), Some(device_id)) =
                    (auth_token.as_deref(), openclaw_client.device_id.as_deref())
                {
                    match verify_device_token(&state, device_id, token, &openclaw_client.role, &openclaw_client.scopes) {
                        Ok(true) => {
                            auth_ok = true;
                        }
                        Ok(false) => {}
                        Err(e) => warn!(error = %e.message, "device token verification failed"),
                    }
                }
            }

            if !auth_ok {
                let err = error_response(&req.id, error_codes::INVALID_REQUEST, "unauthorized", None);
                send_frame(&tx, &queued_bytes, &closing, &err).await;
                break;
            }

            // Pairing enforcement (OpenClaw parity) when a device identity is presented.
            let mut hello_auth: Option<serde_json::Value> = None;
            if let (Some(device_id), Some(public_key)) =
                (openclaw_client.device_id.clone(), device_public_key.as_deref())
            {
                let paired = match get_paired_device(&state, &device_id) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(error = %e.message, "failed to load paired device");
                        None
                    }
                };

                let mut require_pairing: Option<&'static str> = None;
                if let Some(paired) = paired.as_ref() {
                    if paired.public_key != public_key {
                        require_pairing = Some("not-paired");
                    } else {
                        let allowed_roles = merge_roles(
                            &[paired.roles.clone()],
                            &[paired.role.clone()],
                        )
                        .unwrap_or_default();
                        if allowed_roles.is_empty()
                            || !allowed_roles.iter().any(|r| r == &openclaw_client.role)
                        {
                            require_pairing = Some("role-upgrade");
                        } else if !openclaw_client.scopes.is_empty() {
                            let allowed_scopes = paired.scopes.clone().unwrap_or_default();
                            if allowed_scopes.is_empty()
                                || !scopes_allow(&openclaw_client.scopes, &allowed_scopes)
                            {
                                require_pairing = Some("scope-upgrade");
                            }
                        }
                    }
                } else {
                    require_pairing = Some("not-paired");
                }

                if let Some(reason) = require_pairing {
                    let silent = if is_local_client { Some(true) } else { None };
                    match request_device_pairing(&state, &device_id, public_key, &openclaw_client, silent) {
                        Ok((pair_req, created)) => {
                            if pair_req.silent == Some(true) {
                                if let Ok(Some((request_id, device))) = approve_device_pairing(&state, &pair_req.request_id) {
                                    broadcast_openclaw_event(
                                        &state,
                                        "device.pair.resolved",
                                        json!({
                                            "requestId": request_id,
                                            "deviceId": device.device_id,
                                            "decision": "approved",
                                            "ts": now_ms(),
                                        }),
                                        None,
                                    )
                                    .await;
                                }
                            } else {
                                if created {
                                    broadcast_openclaw_event(
                                        &state,
                                        "device.pair.requested",
                                        serde_json::to_value(&pair_req).unwrap_or_else(|_| json!({})),
                                        None,
                                    )
                                    .await;
                                }
                                let err = error_response(
                                    &req.id,
                                    error_codes::NOT_PAIRED,
                                    "pairing required",
                                    Some(json!({"requestId": pair_req.request_id, "reason": reason })),
                                );
                                send_frame(&tx, &queued_bytes, &closing, &err).await;
                                break;
                            }
                        }
                        Err(e) => {
                            warn!(error = %e.message, "device pairing request failed");
                            let err = error_response(&req.id, error_codes::UNAVAILABLE, "pairing request failed", None);
                            send_frame(&tx, &queued_bytes, &closing, &err).await;
                            break;
                        }
                    }
                }

                // Best-effort: keep paired device metadata current.
                if let Err(e) = update_paired_device_metadata(&state, &openclaw_client) {
                    warn!(error = %e.message, "failed to update paired device metadata");
                }

                if let Ok(Some(token)) = ensure_device_token(
                    &state,
                    &device_id,
                    &openclaw_client.role,
                    &openclaw_client.scopes,
                ) {
                    hello_auth = Some(json!({
                        "deviceToken": token.token,
                        "role": token.role,
                        "scopes": token.scopes,
                        "issuedAtMs": token.rotated_at_ms.unwrap_or(token.created_at_ms),
                    }));
                }
            }

            if openclaw_client.role == "node" {
                let node_id = openclaw_client
                    .device_id
                    .clone()
                    .or(openclaw_client.instance_id.clone())
                    .unwrap_or_else(|| openclaw_client.conn_id.clone());
                connected_node_id = Some(node_id.clone());
                connected_role = Some("node".to_string());
                if let Err(e) = update_paired_node_metadata(&state, &node_id, &openclaw_client) {
                    warn!(error = %e.message, "failed to update paired node metadata");
                }
            } else {
                connected_role = Some(openclaw_client.role.clone());
            }

            state.register_openclaw_client(openclaw_client).await;
            registered = true;
            let presence_version = state.increment_openclaw_presence_version();

            // Build hello-ok payload.
            let host = std::env::var("HOSTNAME").ok();
            let system_presence = list_system_presence(&state, "gateway").await;
            let snapshot = Snapshot {
                presence: system_presence
                    .into_iter()
                    .filter_map(|v| serde_json::from_value::<PresenceEntry>(v).ok())
                    .collect(),
                health: crate::openclaw_health::build_health_snapshot(&state).await,
                state_version: StateVersion {
                    presence: presence_version,
                    health: state.openclaw_health_version(),
                },
                uptime_ms: state.uptime_secs() * 1000,
                config_path: Some(resolve_config_path_for_read().to_string_lossy().to_string()),
                state_dir: resolve_openclaw_state_dir(&state)
                    .map(|p| p.to_string_lossy().to_string()),
                session_defaults: Some(json!({
                    "defaultAgentId": "default",
                    "mainKey": "main",
                    "mainSessionKey": "main",
                    "scope": "global"
                })),
            };

            let hello = HelloOk {
                kind: "hello-ok".to_string(),
                protocol: OPENCLAW_PROTOCOL_VERSION,
                server: HelloServer {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    commit: option_env!("GIT_COMMIT").map(|s| s.to_string()),
                    host,
                    conn_id: conn_id.clone(),
                },
                features: HelloFeatures {
                    methods: METHODS.iter().map(|s| s.to_string()).collect(),
                    events: EVENTS.iter().map(|s| s.to_string()).collect(),
                },
                snapshot,
                canvas_host_url: None,
                auth: hello_auth,
                policy: HelloPolicy {
                    max_payload: max_payload_bytes,
                    max_buffered_bytes: resolve_openclaw_max_buffered_bytes(),
                    tick_interval_ms: DEFAULT_TICK_INTERVAL_MS,
                },
            };

            let payload = serde_json::to_value(&hello).unwrap_or_else(|_| json!({}));
            let res = ok_response(&req.id, payload);
            send_frame(&tx, &queued_bytes, &closing, &res).await;

            // Start cron scheduler on connect so persisted jobs run even if no one
            // explicitly calls cron.* methods (matches OpenClaw behavior).
            let _ = cron_service_for_state(&state).await;
            // Start health monitor so clients can rely on `health` events/stateVersion.
            let _ = crate::openclaw_health::health_service_for_state(&state).await;

            // Start tick loop.
            let tick_tx = tx.clone();
            let tick_bytes = queued_bytes.clone();
            let tick_closing = closing.clone();
            let tick_seq = event_seq.clone();
            let mut shutdown_rx = shutdown_tx.subscribe();
            tick_task = Some(tokio::spawn(async move {
                let mut interval =
                    tokio::time::interval(Duration::from_millis(DEFAULT_TICK_INTERVAL_MS));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            send_event(&tick_tx, &tick_bytes, &tick_closing, &tick_seq, "tick", json!({"ts": now_ms()}), None, true).await;
                        }
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() {
                                break;
                            }
                        }
                    }
                }
            }));

            connected = true;
            info!(%peer, "OpenClaw handshake complete");
            state
                .openclaw_logs()
                .push_line(&format!(
                    "openclaw: handshake complete peer={} conn_id={} role={}",
                    peer,
                    conn_id,
                    connected_role.as_deref().unwrap_or("unknown")
                ))
                .await;

            // Best-effort presence broadcast after handshake completes.
            broadcast_presence(&state, Some(presence_version)).await;

            // If we staged a device pairing request during connect, broadcast it after hello-ok.
            if let Some(req) = device_pair_request.take() {
                broadcast_openclaw_event(
                    &state,
                    "device.pair.requested",
                    serde_json::to_value(&req).unwrap_or_else(|_| json!({})),
                    None,
                )
                .await;
            }

            // Nodes expect a voicewake snapshot on connect.
            if connected_role.as_deref() == Some("node") {
                if let Ok(cfg) = handle_voicewake_get().await {
                    send_event(
                        &tx,
                        &queued_bytes,
                        &closing,
                        &event_seq,
                        "voicewake.changed",
                        cfg,
                        None,
                        false,
                    )
                    .await;
                }

                // Best-effort: probe for remote bin availability so skills.status can report
                // remote eligibility (OpenClaw parity).
                if let Some(node_id) = connected_node_id.clone() {
                    let st = state.clone();
                    tokio::spawn(async move {
                        refresh_remote_node_bins_best_effort(st, node_id, true).await;
                    });
                }
            }
            continue;
        }

        // Handle requests concurrently so long-running methods (browser, approvals, etc.)
        // don't block the websocket receive loop (important for node interop).
        let ctx_task = ctx.clone();
        tokio::spawn(async move {
            let response = handle_request_after_connect(&ctx_task, req).await;
            send_frame(
                &ctx_task.tx,
                &ctx_task.queued_bytes,
                &ctx_task.closing,
                &response,
            )
            .await;
        });
    }

    // Shutdown background tasks.
    let _ = shutdown_tx.send(true);
    if let Some(task) = tick_task {
        task.abort();
    }

    if registered {
        state.unregister_openclaw_client(&conn_id).await;
        broadcast_presence(&state, None).await;
    }

    if connected_role.as_deref() == Some("node") {
        if let Some(node_id) = connected_node_id.as_deref() {
            cancel_node_invokes_for_node(node_id, "node disconnected").await;
        }
    }

    drop(tx);
    let _ = sender_task.await;
    info!(%peer, "OpenClaw WS disconnected");
    state
        .openclaw_logs()
        .push_line(&format!(
            "openclaw: ws disconnected peer={} conn_id={}",
            peer, conn_id_for_sender
        ))
        .await;
}
