//! OpenClaw exec approval support shared across the gateway.
//!
//! OpenClaw uses "exec approvals" as a generic interactive approval mechanism:
//! one client requests approval, the Control UI/operator resolves it.
//!
//! drbot reuses this mechanism to gate side-effectful tool calls (e.g. API writes)
//! without permanently enabling them via environment variables.

use crate::state::GatewayState;
use drbot_protocol::openclaw::{error_codes, ErrorShape};
use ring::digest;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

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

pub fn validate_exec_approval_decision(value: &str) -> bool {
    matches!(value, "allow-once" | "allow-always" | "deny")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecApprovalRequestPayload {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    #[serde(default, rename = "agentId", skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(
        default,
        rename = "resolvedPath",
        skip_serializing_if = "Option::is_none"
    )]
    pub resolved_path: Option<String>,
    #[serde(
        default,
        rename = "sessionKey",
        skip_serializing_if = "Option::is_none"
    )]
    pub session_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExecApprovalRecord {
    pub id: String,
    pub request: ExecApprovalRequestPayload,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
}

#[derive(Debug)]
struct PendingExecApproval {
    record: ExecApprovalRecord,
    tx: oneshot::Sender<Option<String>>,
}

static OPENCLAW_EXEC_APPROVALS: OnceLock<Mutex<HashMap<String, PendingExecApproval>>> =
    OnceLock::new();

fn openclaw_exec_approvals() -> &'static Mutex<HashMap<String, PendingExecApproval>> {
    OPENCLAW_EXEC_APPROVALS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub async fn exec_approval_snapshot(id: &str) -> Option<ExecApprovalRecord> {
    openclaw_exec_approvals()
        .lock()
        .await
        .get(id)
        .map(|p| p.record.clone())
}

pub async fn create_exec_approval(
    request: ExecApprovalRequestPayload,
    timeout_ms: u64,
    explicit_id: Option<String>,
) -> (ExecApprovalRecord, oneshot::Receiver<Option<String>>) {
    let now = now_ms();
    let id = explicit_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let record = ExecApprovalRecord {
        id: id.clone(),
        request,
        created_at_ms: now,
        expires_at_ms: now.saturating_add(timeout_ms),
    };
    let (tx, rx) = oneshot::channel::<Option<String>>();
    openclaw_exec_approvals().lock().await.insert(
        id,
        PendingExecApproval {
            record: record.clone(),
            tx,
        },
    );
    (record, rx)
}

pub async fn resolve_exec_approval(id: &str, decision: &str) -> Option<ExecApprovalRecord> {
    let mut approvals = openclaw_exec_approvals().lock().await;
    let pending = approvals.remove(id)?;
    let _ = pending.tx.send(Some(decision.to_string()));
    Some(pending.record)
}

pub async fn expire_exec_approval(id: &str) -> Option<ExecApprovalRecord> {
    let mut approvals = openclaw_exec_approvals().lock().await;
    let pending = approvals.remove(id)?;
    let _ = pending.tx.send(None);
    Some(pending.record)
}

pub fn resolve_exec_approvals_path() -> PathBuf {
    if let Some(dir) = drbot_core::Config::config_dir() {
        return dir.join("exec_approvals.json");
    }
    PathBuf::from("exec_approvals.json")
}

fn read_exec_approvals_raw() -> String {
    let path = resolve_exec_approvals_path();
    std::fs::read_to_string(&path).unwrap_or_else(|_| json!({ "version": 1 }).to_string())
}

fn load_exec_approvals_json() -> serde_json::Value {
    let raw = read_exec_approvals_raw();
    serde_json::from_str(&raw).unwrap_or_else(|_| json!({ "version": 1 }))
}

fn write_exec_approvals_json_atomic(value: &serde_json::Value) -> Result<(), String> {
    let path = resolve_exec_approvals_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    let tmp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
    std::fs::write(&tmp, raw).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Whether writes are permanently enabled for a given tool (via allow-always).
///
/// Stored in `exec_approvals.json` under:
/// `{ "drbotTools": { "<tool>": { "allowWrites": true } } }`
pub fn tool_writes_allowed(tool: &str) -> bool {
    let tool = tool.trim();
    if tool.is_empty() {
        return false;
    }
    let file = load_exec_approvals_json();
    let tools = file.get("drbotTools").and_then(|v| v.as_object());
    let Some(tools) = tools else {
        return false;
    };

    let mut keys: Vec<&str> = Vec::new();
    keys.push(tool);
    match tool {
        "bash" | "exec" => {
            keys.push("bash");
            keys.push("exec");
        }
        "write_file" | "write" => {
            keys.push("write_file");
            keys.push("write");
        }
        "http" | "web_fetch" => {
            keys.push("http");
            keys.push("web_fetch");
        }
        _ => {}
    }

    keys.into_iter().any(|key| {
        tools
            .get(key)
            .and_then(|v| v.get("allowWrites"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    })
}

pub fn set_tool_writes_allowed(tool: &str, allow: bool) -> Result<(), String> {
    let tool = tool.trim();
    if tool.is_empty() {
        return Ok(());
    }

    let mut file = load_exec_approvals_json();
    if !file.is_object() {
        file = json!({ "version": 1 });
    }

    let obj = file.as_object_mut().unwrap();
    let tools_val = obj
        .entry("drbotTools".to_string())
        .or_insert_with(|| json!({}));
    if !tools_val.is_object() {
        *tools_val = json!({});
    }

    let tools_obj = tools_val.as_object_mut().unwrap();

    let mut keys: Vec<&str> = Vec::new();
    keys.push(tool);
    match tool {
        "bash" | "exec" => {
            keys.push("bash");
            keys.push("exec");
        }
        "write_file" | "write" => {
            keys.push("write_file");
            keys.push("write");
        }
        "http" | "web_fetch" => {
            keys.push("http");
            keys.push("web_fetch");
        }
        _ => {}
    }

    for key in keys {
        let entry_val = tools_obj
            .entry(key.to_string())
            .or_insert_with(|| json!({}));
        if !entry_val.is_object() {
            *entry_val = json!({});
        }
        if let Some(map) = entry_val.as_object_mut() {
            map.insert("allowWrites".to_string(), json!(allow));
        }
    }

    write_exec_approvals_json_atomic(&serde_json::Value::Object(obj.clone()))
}

/// Best-effort OpenClaw parity: resolve whether exec approvals are configured
/// to auto-allow skill CLIs for a given agent.
///
/// OpenClaw stores this under `defaults.autoAllowSkills` and/or `agents.<id>.autoAllowSkills`.
pub fn exec_approvals_auto_allow_skills(agent_id: Option<&str>) -> bool {
    let file = load_exec_approvals_json();
    let defaults = file
        .get("defaults")
        .and_then(|v| v.get("autoAllowSkills"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let agent_id = agent_id.map(|s| s.trim()).filter(|s| !s.is_empty());
    let mut override_val = agent_id.and_then(|id| {
        file.get("agents")
            .and_then(|v| v.get(id))
            .and_then(|v| v.get("autoAllowSkills"))
            .and_then(|v| v.as_bool())
    });

    // Back-compat: OpenClaw's legacy default agent id is "main"; drbot uses "default".
    if override_val.is_none() {
        if let Some("default") = agent_id {
            override_val = file
                .get("agents")
                .and_then(|v| v.get("main"))
                .and_then(|v| v.get("autoAllowSkills"))
                .and_then(|v| v.as_bool());
        }
    }

    override_val.unwrap_or(defaults)
}

/// Ensure a side-effectful write is allowed for a tool.
///
/// - If the tool was previously approved with "allow-always", this returns Ok.
/// - Otherwise it emits `exec.approval.requested` and waits for a decision.
pub async fn ensure_tool_write_allowed(
    state: &GatewayState,
    tool: &str,
    request: ExecApprovalRequestPayload,
    timeout_ms: u64,
) -> Result<(), ErrorShape> {
    if tool_writes_allowed(tool) {
        return Ok(());
    }

    let timeout_ms = timeout_ms.max(1);
    let (record, rx) = create_exec_approval(request, timeout_ms, None).await;

    crate::openclaw::broadcast_openclaw_event(
        state,
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
            let _ = expire_exec_approval(&record.id).await;
            None
        }
    };

    match decision.as_deref() {
        Some("allow-once") => Ok(()),
        Some("allow-always") => {
            // Persist allow-always so future write calls do not block.
            let _ = set_tool_writes_allowed(tool, true);
            Ok(())
        }
        Some("deny") => Err(ErrorShape::new(error_codes::UNAVAILABLE, "request denied")
            .with_details(json!({ "tool": tool, "approvalId": record.id }))),
        _ => Err(
            ErrorShape::new(error_codes::UNAVAILABLE, "approval timed out")
                .with_details(json!({ "tool": tool, "approvalId": record.id })),
        ),
    }
}

/// Helper used by `exec.approvals.get` to compute the current file + hash.
pub fn exec_approvals_get_payload() -> serde_json::Value {
    let path = resolve_exec_approvals_path();
    let exists = path.exists();

    let raw = if exists {
        std::fs::read_to_string(&path).ok()
    } else {
        None
    };

    let file = raw
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .unwrap_or_else(|| json!({ "version": 1 }));

    let raw_for_hash = raw.clone().unwrap_or_else(|| file.to_string());
    let hash = sha256_hex(&raw_for_hash);

    json!({
        "path": path.to_string_lossy(),
        "exists": exists,
        "hash": hash,
        "file": file,
    })
}
