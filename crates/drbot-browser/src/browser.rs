//! Browser instance management.

use crate::cdp::CdpConnection;
use crate::console::{ConsoleMessage, LogLevel};
use crate::page::Page;
use base64::Engine;
use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

const MAX_CONSOLE_MESSAGES: usize = 500;
const MAX_CONSOLE_CHARS: usize = 8_000;
const MAX_PAGE_ERRORS: usize = 200;
const MAX_NETWORK_REQUESTS: usize = 500;
const MAX_TRACE_EVENTS: usize = 200_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSetCookie {
    pub name: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<f64>,
    #[serde(rename = "httpOnly", skip_serializing_if = "Option::is_none")]
    pub http_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure: Option<bool>,
    #[serde(rename = "sameSite", skip_serializing_if = "Option::is_none")]
    pub same_site: Option<String>,
}

/// A download captured from CDP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserDownload {
    pub url: String,
    #[serde(rename = "suggestedFilename")]
    pub suggested_filename: String,
    pub path: String,
}

#[derive(Debug, Clone)]
struct DialogArm {
    id: u64,
    accept: bool,
    prompt_text: Option<String>,
}

#[derive(Debug, Clone)]
struct UploadArm {
    id: u64,
    paths: Vec<String>,
}

struct DownloadArm {
    id: u64,
    download_dir: PathBuf,
    tx: oneshot::Sender<Result<BrowserDownload, String>>,
    guid: Option<String>,
    url: Option<String>,
    suggested_filename: Option<String>,
}

struct TraceState {
    id: u64,
    events: Vec<Value>,
    done_tx: Option<oneshot::Sender<Result<Vec<Value>, String>>>,
}

/// A page error captured from CDP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserPageError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    pub timestamp: chrono::DateTime<Utc>,
}

/// A network request captured from CDP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserNetworkRequest {
    pub id: String,
    pub timestamp: chrono::DateTime<Utc>,
    pub method: String,
    pub url: String,
    #[serde(rename = "resourceType", skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(rename = "failureText", skip_serializing_if = "Option::is_none")]
    pub failure_text: Option<String>,
}

/// Browser launch options.
#[derive(Debug, Clone)]
pub struct BrowserOptions {
    /// Path to Chrome/Chromium executable.
    pub executable: Option<String>,
    /// Run headless.
    pub headless: bool,
    /// Additional arguments.
    pub args: Vec<String>,
    /// User data directory.
    pub user_data_dir: Option<String>,
    /// Remote debugging port.
    pub port: u16,
}

impl Default for BrowserOptions {
    fn default() -> Self {
        Self {
            executable: None,
            headless: true,
            args: vec![],
            user_data_dir: None,
            port: 0, // Random port
        }
    }
}

/// Chrome DevTools JSON endpoint response.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionInfo {
    /// Browser name.
    #[serde(rename = "Browser")]
    pub browser: String,
    /// Protocol version.
    #[serde(rename = "Protocol-Version")]
    pub protocol_version: String,
    /// V8 version.
    #[serde(rename = "V8-Version")]
    pub v8_version: Option<String>,
    /// WebKit version.
    #[serde(rename = "WebKit-Version")]
    pub webkit_version: Option<String>,
    /// WebSocket debugger URL.
    #[serde(rename = "webSocketDebuggerUrl")]
    pub websocket_debugger_url: Option<String>,
}

/// Target info from CDP.
#[derive(Debug, Clone, Deserialize)]
pub struct TargetInfo {
    /// Target ID.
    #[serde(rename = "targetId")]
    pub target_id: String,
    /// Target type.
    #[serde(rename = "type")]
    pub target_type: String,
    /// Title.
    pub title: String,
    /// URL.
    pub url: String,
    /// Whether attached.
    pub attached: Option<bool>,
}

/// Browser instance.
pub struct Browser {
    /// CDP connection.
    cdp: Arc<CdpConnection>,
    /// Debug state (console messages, etc).
    debug: Arc<BrowserDebugState>,
    /// Event processing task.
    event_task: Option<JoinHandle<()>>,
    /// Child process (if launched).
    process: Option<Child>,
    /// WebSocket URL.
    ws_url: String,
}

#[derive(Default)]
struct BrowserDebugState {
    arm_seq: AtomicU64,
    session_to_target: RwLock<HashMap<String, String>>,
    target_to_session: RwLock<HashMap<String, String>>,
    console_by_target: RwLock<HashMap<String, Vec<ConsoleMessage>>>,
    errors_by_target: RwLock<HashMap<String, Vec<BrowserPageError>>>,
    network_by_target: RwLock<HashMap<String, Vec<BrowserNetworkRequest>>>,
    network_pending_by_target: RwLock<HashMap<String, HashMap<String, BrowserNetworkRequest>>>,
    dialog_arms_by_target: RwLock<HashMap<String, DialogArm>>,
    upload_arms_by_target: RwLock<HashMap<String, UploadArm>>,
    download_arms_by_target: RwLock<HashMap<String, DownloadArm>>,
    http_credentials_by_target: RwLock<HashMap<String, (String, String)>>,
    http_credentials: RwLock<Option<(String, String)>>,
    trace_state: RwLock<Option<TraceState>>,
}

impl BrowserDebugState {
    fn next_arm_id(&self) -> u64 {
        self.arm_seq.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
    }

    async fn register_session(&self, target_id: &str, session_id: &str) {
        if target_id.trim().is_empty() || session_id.trim().is_empty() {
            return;
        }
        self.session_to_target
            .write()
            .await
            .insert(session_id.to_string(), target_id.to_string());
        self.target_to_session
            .write()
            .await
            .insert(target_id.to_string(), session_id.to_string());
        self.console_by_target
            .write()
            .await
            .entry(target_id.to_string())
            .or_insert_with(Vec::new);
        self.errors_by_target
            .write()
            .await
            .entry(target_id.to_string())
            .or_insert_with(Vec::new);
        self.network_by_target
            .write()
            .await
            .entry(target_id.to_string())
            .or_insert_with(Vec::new);
        self.network_pending_by_target
            .write()
            .await
            .entry(target_id.to_string())
            .or_insert_with(HashMap::new);
    }

    async fn session_for_target(&self, target_id: &str) -> Option<String> {
        self.target_to_session.read().await.get(target_id).cloned()
    }

    async fn target_for_session(&self, session_id: &str) -> Option<String> {
        self.session_to_target.read().await.get(session_id).cloned()
    }

    async fn forget_target(&self, target_id: &str) {
        let target_id = target_id.trim();
        if target_id.is_empty() {
            return;
        }

        self.console_by_target.write().await.remove(target_id);
        self.errors_by_target.write().await.remove(target_id);
        self.network_by_target.write().await.remove(target_id);
        self.network_pending_by_target
            .write()
            .await
            .remove(target_id);
        self.dialog_arms_by_target.write().await.remove(target_id);
        self.upload_arms_by_target.write().await.remove(target_id);
        self.download_arms_by_target.write().await.remove(target_id);
        self.http_credentials_by_target
            .write()
            .await
            .remove(target_id);
        self.target_to_session.write().await.remove(target_id);
        self.session_to_target
            .write()
            .await
            .retain(|_, v| v != target_id);
    }

    async fn set_http_credentials(&self, username: String, password: String) {
        *self.http_credentials.write().await = Some((username, password));
    }

    async fn clear_http_credentials(&self) {
        *self.http_credentials.write().await = None;
    }

    async fn get_http_credentials(&self) -> Option<(String, String)> {
        self.http_credentials.read().await.clone()
    }

    async fn set_dialog_arm(&self, target_id: &str, arm: DialogArm) {
        self.dialog_arms_by_target
            .write()
            .await
            .insert(target_id.to_string(), arm);
    }

    async fn take_dialog_arm(&self, target_id: &str) -> Option<DialogArm> {
        self.dialog_arms_by_target.write().await.remove(target_id)
    }

    async fn clear_dialog_arm_if_matches(&self, target_id: &str, arm_id: u64) {
        let mut map = self.dialog_arms_by_target.write().await;
        let should_clear = map.get(target_id).map(|a| a.id) == Some(arm_id);
        if should_clear {
            map.remove(target_id);
        }
    }

    async fn set_upload_arm(&self, target_id: &str, arm: UploadArm) {
        self.upload_arms_by_target
            .write()
            .await
            .insert(target_id.to_string(), arm);
    }

    async fn take_upload_arm(&self, target_id: &str) -> Option<UploadArm> {
        self.upload_arms_by_target.write().await.remove(target_id)
    }

    async fn clear_upload_arm_if_matches(&self, target_id: &str, arm_id: u64) -> bool {
        let mut map = self.upload_arms_by_target.write().await;
        let should_clear = map.get(target_id).map(|a| a.id) == Some(arm_id);
        if should_clear {
            map.remove(target_id);
        }
        should_clear
    }

    async fn set_download_arm(&self, target_id: &str, arm: DownloadArm) {
        self.download_arms_by_target
            .write()
            .await
            .insert(target_id.to_string(), arm);
    }

    async fn clear_download_arm_if_matches(
        &self,
        target_id: &str,
        arm_id: u64,
    ) -> Option<DownloadArm> {
        let mut map = self.download_arms_by_target.write().await;
        let should_clear = map.get(target_id).map(|a| a.id) == Some(arm_id);
        if should_clear {
            map.remove(target_id)
        } else {
            None
        }
    }

    async fn download_will_begin(
        &self,
        target_id: &str,
        guid: &str,
        url: &str,
        suggested_filename: &str,
    ) {
        let mut map = self.download_arms_by_target.write().await;
        let Some(arm) = map.get_mut(target_id) else {
            return;
        };
        if arm.guid.is_some() {
            return;
        }
        arm.guid = Some(guid.to_string());
        arm.url = Some(url.to_string());
        arm.suggested_filename = Some(suggested_filename.to_string());
    }

    async fn take_download_arm_for_guid(&self, target_id: &str, guid: &str) -> Option<DownloadArm> {
        let mut map = self.download_arms_by_target.write().await;
        let matches = map.get(target_id).and_then(|a| a.guid.as_deref()) == Some(guid);
        if matches {
            map.remove(target_id)
        } else {
            None
        }
    }

    async fn push_console_for_session(&self, session_id: &str, message: ConsoleMessage) {
        let target_id = { self.session_to_target.read().await.get(session_id).cloned() };
        let Some(target_id) = target_id else {
            return;
        };

        let mut map = self.console_by_target.write().await;
        let entry = map.entry(target_id).or_insert_with(Vec::new);
        entry.push(message);
        if entry.len() > MAX_CONSOLE_MESSAGES {
            let excess = entry.len() - MAX_CONSOLE_MESSAGES;
            entry.drain(0..excess);
        }
    }

    async fn console_messages(&self, target_id: &str) -> Vec<ConsoleMessage> {
        self.console_by_target
            .read()
            .await
            .get(target_id)
            .cloned()
            .unwrap_or_default()
    }

    async fn push_page_error_for_session(&self, session_id: &str, error: BrowserPageError) {
        let target_id = { self.session_to_target.read().await.get(session_id).cloned() };
        let Some(target_id) = target_id else {
            return;
        };

        let mut map = self.errors_by_target.write().await;
        let entry = map.entry(target_id).or_insert_with(Vec::new);
        entry.push(error);
        if entry.len() > MAX_PAGE_ERRORS {
            let excess = entry.len() - MAX_PAGE_ERRORS;
            entry.drain(0..excess);
        }
    }

    async fn page_errors(&self, target_id: &str, clear: bool) -> Vec<BrowserPageError> {
        let mut map = self.errors_by_target.write().await;
        let errors = map.get(target_id).cloned().unwrap_or_default();
        if clear {
            map.insert(target_id.to_string(), Vec::new());
        }
        errors
    }

    async fn network_request_started_for_session(
        &self,
        session_id: &str,
        request: BrowserNetworkRequest,
    ) {
        let target_id = { self.session_to_target.read().await.get(session_id).cloned() };
        let Some(target_id) = target_id else {
            return;
        };

        let mut pending = self.network_pending_by_target.write().await;
        let entry = pending.entry(target_id).or_insert_with(HashMap::new);
        entry.insert(request.id.clone(), request);
    }

    async fn network_update_response_for_session(
        &self,
        session_id: &str,
        request_id: &str,
        status: u16,
    ) {
        let target_id = { self.session_to_target.read().await.get(session_id).cloned() };
        let Some(target_id) = target_id else {
            return;
        };

        let mut pending = self.network_pending_by_target.write().await;
        if let Some(map) = pending.get_mut(&target_id) {
            if let Some(entry) = map.get_mut(request_id) {
                entry.status = Some(status);
                entry.ok = Some((200..300).contains(&status));
            }
        }
    }

    async fn network_finalize_for_session(&self, session_id: &str, request_id: &str) {
        let target_id = { self.session_to_target.read().await.get(session_id).cloned() };
        let Some(target_id) = target_id else {
            return;
        };

        let request = {
            let mut pending = self.network_pending_by_target.write().await;
            pending
                .get_mut(&target_id)
                .and_then(|m| m.remove(request_id))
        };
        let Some(request) = request else {
            return;
        };

        let mut done = self.network_by_target.write().await;
        let entry = done.entry(target_id).or_insert_with(Vec::new);
        entry.push(request);
        if entry.len() > MAX_NETWORK_REQUESTS {
            let excess = entry.len() - MAX_NETWORK_REQUESTS;
            entry.drain(0..excess);
        }
    }

    async fn network_fail_for_session(
        &self,
        session_id: &str,
        request_id: &str,
        failure_text: &str,
    ) {
        let target_id = { self.session_to_target.read().await.get(session_id).cloned() };
        let Some(target_id) = target_id else {
            return;
        };

        let request = {
            let mut pending = self.network_pending_by_target.write().await;
            pending
                .get_mut(&target_id)
                .and_then(|m| m.remove(request_id))
        };
        let mut request = request.unwrap_or_else(|| BrowserNetworkRequest {
            id: request_id.to_string(),
            timestamp: Utc::now(),
            method: "GET".to_string(),
            url: "".to_string(),
            resource_type: None,
            status: None,
            ok: None,
            failure_text: None,
        });
        request.failure_text = Some(truncate_chars(failure_text, 1000));
        request.ok = Some(false);

        let mut done = self.network_by_target.write().await;
        let entry = done.entry(target_id).or_insert_with(Vec::new);
        entry.push(request);
        if entry.len() > MAX_NETWORK_REQUESTS {
            let excess = entry.len() - MAX_NETWORK_REQUESTS;
            entry.drain(0..excess);
        }
    }

    async fn network_requests(
        &self,
        target_id: &str,
        filter: Option<&str>,
        clear: bool,
    ) -> Vec<BrowserNetworkRequest> {
        let filter = filter.unwrap_or("").trim();
        let raw = self
            .network_by_target
            .read()
            .await
            .get(target_id)
            .cloned()
            .unwrap_or_default();
        let out = if filter.is_empty() {
            raw
        } else {
            raw.into_iter().filter(|r| r.url.contains(filter)).collect()
        };
        if clear {
            {
                let mut pending = self.network_pending_by_target.write().await;
                pending.insert(target_id.to_string(), HashMap::new());
            }
            let mut done = self.network_by_target.write().await;
            done.insert(target_id.to_string(), Vec::new());
        }
        out
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let max_chars = max_chars.max(1);
    let mut chars = value.chars();
    let out: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_none() {
        out
    } else {
        format!("{}…", out)
    }
}

fn remote_object_to_text(obj: &Value) -> String {
    if let Some(v) = obj.get("value") {
        if let Some(s) = v.as_str() {
            return s.to_string();
        }
        if !v.is_null() {
            return v.to_string();
        }
    }
    if let Some(s) = obj.get("unserializableValue").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(s) = obj.get("description").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    if let Some(s) = obj.get("type").and_then(|v| v.as_str()) {
        return s.to_string();
    }
    "<value>".to_string()
}

fn parse_cdp_timestamp(raw: Option<&Value>) -> chrono::DateTime<Utc> {
    let ts = raw.and_then(|v| v.as_f64()).unwrap_or(0.0);
    if ts <= 0.0 {
        return Utc::now();
    }
    let secs = ts.trunc() as i64;
    let nanos = ((ts.fract() * 1_000_000_000.0).round() as i64).clamp(0, 999_999_999) as u32;
    Utc.timestamp_opt(secs, nanos)
        .single()
        .unwrap_or_else(Utc::now)
}

fn parse_console_api_called(params: &Value) -> Option<ConsoleMessage> {
    let obj = params.as_object()?;
    let typ = obj
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("log")
        .trim()
        .to_ascii_lowercase();
    let level = match typ.as_str() {
        "error" => LogLevel::Error,
        "warning" | "warn" => LogLevel::Warning,
        "debug" | "verbose" => LogLevel::Verbose,
        _ => LogLevel::Info,
    };

    let text = obj
        .get("args")
        .and_then(|v| v.as_array())
        .map(|args| {
            args.iter()
                .map(remote_object_to_text)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let text = if text.trim().is_empty() {
        typ.clone()
    } else {
        truncate_chars(&text, MAX_CONSOLE_CHARS)
    };

    let mut msg = ConsoleMessage::new(level, &text);
    msg.timestamp = parse_cdp_timestamp(obj.get("timestamp"));
    msg.context_id = obj.get("executionContextId").and_then(|v| v.as_i64());

    if let Some(stack) = obj.get("stackTrace") {
        if let Some(frame) = stack
            .get("callFrames")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_object())
        {
            if let Some(url) = frame
                .get("url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
            {
                msg.url = Some(url.to_string());
            }
            if let Some(line) = frame
                .get("lineNumber")
                .and_then(|v| v.as_i64())
                .filter(|v| *v >= 0)
            {
                msg.line = Some(line as u32);
            }
            if let Some(col) = frame
                .get("columnNumber")
                .and_then(|v| v.as_i64())
                .filter(|v| *v >= 0)
            {
                msg.column = Some(col as u32);
            }
        }
    }

    Some(msg)
}

fn parse_exception_thrown(params: &Value) -> Option<BrowserPageError> {
    let obj = params.as_object()?;
    let ts = parse_cdp_timestamp(obj.get("timestamp"));
    let details = obj.get("exceptionDetails").and_then(|v| v.as_object())?;

    let exception = details.get("exception").and_then(|v| v.as_object());
    let message_raw = exception
        .and_then(|e| e.get("description"))
        .and_then(|v| v.as_str())
        .or_else(|| details.get("text").and_then(|v| v.as_str()))
        .unwrap_or("Exception");
    let message = truncate_chars(message_raw, MAX_CONSOLE_CHARS);

    let name = exception
        .and_then(|e| e.get("className"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let stack = details
        .get("stackTrace")
        .and_then(|v| v.get("callFrames"))
        .and_then(|v| v.as_array())
        .map(|frames| {
            let mut out: Vec<String> = Vec::new();
            for frame in frames.iter().take(50) {
                let Some(frame) = frame.as_object() else {
                    continue;
                };
                let fn_name = frame
                    .get("functionName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let url = frame
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                let line = frame.get("lineNumber").and_then(|v| v.as_i64());
                let col = frame.get("columnNumber").and_then(|v| v.as_i64());

                let mut line_s = String::new();
                if !fn_name.is_empty() {
                    line_s.push_str(fn_name);
                } else {
                    line_s.push_str("<anonymous>");
                }
                if !url.is_empty() {
                    line_s.push_str(" @ ");
                    line_s.push_str(url);
                    if let Some(line) = line.filter(|v| *v >= 0) {
                        line_s.push(':');
                        line_s.push_str(&line.to_string());
                        if let Some(col) = col.filter(|v| *v >= 0) {
                            line_s.push(':');
                            line_s.push_str(&col.to_string());
                        }
                    }
                }
                out.push(line_s);
            }
            let joined = out.join("\n");
            let trimmed = joined.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(truncate_chars(trimmed, MAX_CONSOLE_CHARS))
            }
        })
        .unwrap_or(None);

    Some(BrowserPageError {
        message,
        name,
        stack,
        timestamp: ts,
    })
}

async fn browser_event_loop(
    mut events: mpsc::Receiver<crate::cdp::CdpEvent>,
    debug: Arc<BrowserDebugState>,
    cdp: Arc<CdpConnection>,
) {
    while let Some(event) = events.recv().await {
        match event.method.as_str() {
            "Fetch.requestPaused" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(params) = event.params.as_ref().and_then(|v| v.as_object()) else {
                    continue;
                };
                let request_id = params
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if request_id.is_empty() {
                    continue;
                }

                if params.get("authChallenge").is_some() {
                    let creds = debug.get_http_credentials().await;
                    let mut auth = serde_json::json!({
                        "response": "CancelAuth",
                    });
                    if let Some((username, password)) = creds {
                        if !username.trim().is_empty() {
                            auth["response"] = serde_json::json!("ProvideCredentials");
                            auth["username"] = serde_json::json!(username);
                            auth["password"] = serde_json::json!(password);
                        }
                    }
                    let _ = cdp
                        .send_with_session(
                            "Fetch.continueWithAuth",
                            Some(serde_json::json!({
                                "requestId": request_id,
                                "authChallengeResponse": auth,
                            })),
                            Some(session_id),
                        )
                        .await;
                    continue;
                }

                let _ = cdp
                    .send_with_session(
                        "Fetch.continueRequest",
                        Some(serde_json::json!({
                            "requestId": request_id,
                        })),
                        Some(session_id),
                    )
                    .await;
            }
            "Fetch.authRequired" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(params) = event.params.as_ref().and_then(|v| v.as_object()) else {
                    continue;
                };
                let request_id = params
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if request_id.is_empty() {
                    continue;
                }

                let creds = debug.get_http_credentials().await;
                let mut auth = serde_json::json!({
                    "response": "CancelAuth",
                });
                if let Some((username, password)) = creds {
                    if !username.trim().is_empty() {
                        auth["response"] = serde_json::json!("ProvideCredentials");
                        auth["username"] = serde_json::json!(username);
                        auth["password"] = serde_json::json!(password);
                    }
                }

                let _ = cdp
                    .send_with_session(
                        "Fetch.continueWithAuth",
                        Some(serde_json::json!({
                            "requestId": request_id,
                            "authChallengeResponse": auth,
                        })),
                        Some(session_id),
                    )
                    .await;
            }
            "Runtime.consoleAPICalled" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(params) = event.params.as_ref() else {
                    continue;
                };
                let Some(msg) = parse_console_api_called(params) else {
                    continue;
                };
                debug.push_console_for_session(session_id, msg).await;
            }
            "Runtime.exceptionThrown" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(params) = event.params.as_ref() else {
                    continue;
                };
                let Some(err) = parse_exception_thrown(params) else {
                    continue;
                };
                debug.push_page_error_for_session(session_id, err).await;
            }
            "Network.requestWillBeSent" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(params) = event.params.as_ref().and_then(|v| v.as_object()) else {
                    continue;
                };
                let request_id = params
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if request_id.is_empty() {
                    continue;
                }

                if let Some(redirect) = params.get("redirectResponse").and_then(|v| v.as_object()) {
                    if let Some(status) = redirect
                        .get("status")
                        .and_then(|v| v.as_f64())
                        .map(|v| v.clamp(0.0, 65535.0) as u16)
                    {
                        debug
                            .network_update_response_for_session(session_id, &request_id, status)
                            .await;
                    }
                    debug
                        .network_finalize_for_session(session_id, &request_id)
                        .await;
                }

                let request = params.get("request").and_then(|v| v.as_object());
                let url = request
                    .and_then(|r| r.get("url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let method = request
                    .and_then(|r| r.get("method"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("GET")
                    .to_string();
                let resource_type = params
                    .get("type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let ts = parse_cdp_timestamp(params.get("timestamp"));

                debug
                    .network_request_started_for_session(
                        session_id,
                        BrowserNetworkRequest {
                            id: request_id,
                            timestamp: ts,
                            method,
                            url,
                            resource_type,
                            status: None,
                            ok: None,
                            failure_text: None,
                        },
                    )
                    .await;
            }
            "Network.responseReceived" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(params) = event.params.as_ref().and_then(|v| v.as_object()) else {
                    continue;
                };
                let request_id = params
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if request_id.is_empty() {
                    continue;
                }
                let response = params.get("response").and_then(|v| v.as_object());
                let status = response
                    .and_then(|r| r.get("status"))
                    .and_then(|v| v.as_f64())
                    .map(|v| v.clamp(0.0, 65535.0) as u16);
                if let Some(status) = status {
                    debug
                        .network_update_response_for_session(session_id, &request_id, status)
                        .await;
                }
            }
            "Network.loadingFinished" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(params) = event.params.as_ref().and_then(|v| v.as_object()) else {
                    continue;
                };
                let request_id = params
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if request_id.is_empty() {
                    continue;
                }
                debug
                    .network_finalize_for_session(session_id, &request_id)
                    .await;
            }
            "Network.loadingFailed" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(params) = event.params.as_ref().and_then(|v| v.as_object()) else {
                    continue;
                };
                let request_id = params
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if request_id.is_empty() {
                    continue;
                }
                let error_text = params
                    .get("errorText")
                    .and_then(|v| v.as_str())
                    .unwrap_or("request failed");
                debug
                    .network_fail_for_session(session_id, &request_id, error_text)
                    .await;
            }
            "Page.javascriptDialogOpening" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(target_id) = debug.target_for_session(session_id).await else {
                    continue;
                };
                let Some(arm) = debug.take_dialog_arm(&target_id).await else {
                    continue;
                };
                let mut params = serde_json::json!({
                    "accept": arm.accept,
                });
                if let Some(prompt) = arm.prompt_text {
                    params["promptText"] = serde_json::json!(prompt);
                }
                let _ = cdp
                    .send_with_session(
                        "Page.handleJavaScriptDialog",
                        Some(params),
                        Some(session_id),
                    )
                    .await;
            }
            "Page.fileChooserOpened" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(params) = event.params.as_ref().and_then(|v| v.as_object()) else {
                    continue;
                };
                let backend_node_id = params
                    .get("backendNodeId")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if backend_node_id <= 0 {
                    continue;
                }
                let Some(target_id) = debug.target_for_session(session_id).await else {
                    continue;
                };
                let Some(arm) = debug.take_upload_arm(&target_id).await else {
                    continue;
                };

                if !arm.paths.is_empty() {
                    let set_params = serde_json::json!({
                        "backendNodeId": backend_node_id,
                        "files": arm.paths,
                    });
                    if let Err(err) = cdp
                        .send_with_session(
                            "DOM.setFileInputFiles",
                            Some(set_params),
                            Some(session_id),
                        )
                        .await
                    {
                        warn!("Failed to set file input files: {}", err);
                    }
                }

                let _ = cdp
                    .send_with_session(
                        "Page.setInterceptFileChooserDialog",
                        Some(serde_json::json!({"enabled": false})),
                        Some(session_id),
                    )
                    .await;
            }
            "Page.downloadWillBegin" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(target_id) = debug.target_for_session(session_id).await else {
                    continue;
                };
                let Some(params) = event.params.as_ref().and_then(|v| v.as_object()) else {
                    continue;
                };
                let guid = params
                    .get("guid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if guid.is_empty() {
                    continue;
                }
                let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let suggested = params
                    .get("suggestedFilename")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                debug
                    .download_will_begin(&target_id, guid, url, suggested)
                    .await;
            }
            "Page.downloadProgress" => {
                let Some(session_id) = event.session_id.as_deref() else {
                    continue;
                };
                let Some(target_id) = debug.target_for_session(session_id).await else {
                    continue;
                };
                let Some(params) = event.params.as_ref().and_then(|v| v.as_object()) else {
                    continue;
                };
                let guid = params
                    .get("guid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if guid.is_empty() {
                    continue;
                }
                let state = params
                    .get("state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase();
                if state.is_empty() || state == "inprogress" {
                    continue;
                }

                let Some(mut arm) = debug.take_download_arm_for_guid(&target_id, &guid).await
                else {
                    continue;
                };

                if state == "canceled" {
                    let _ = arm.tx.send(Err("Download canceled".to_string()));
                    continue;
                }

                let download_dir = arm.download_dir.clone();
                let url = arm.url.take().unwrap_or_default();
                let suggested = arm.suggested_filename.take().unwrap_or_default();
                tokio::spawn(async move {
                    let path =
                        discover_downloaded_file(&download_dir, &suggested, Duration::from_secs(5))
                            .await
                            .unwrap_or_else(|| download_dir.join(suggested.trim()));
                    let result = BrowserDownload {
                        url,
                        suggested_filename: suggested,
                        path: path.to_string_lossy().to_string(),
                    };
                    let _ = arm.tx.send(Ok(result));
                });
            }
            "Tracing.dataCollected" => {
                let Some(params) = event.params.as_ref().and_then(|v| v.as_object()) else {
                    continue;
                };
                let Some(values) = params.get("value").and_then(|v| v.as_array()) else {
                    continue;
                };

                let mut trace = debug.trace_state.write().await;
                let Some(state) = trace.as_mut() else {
                    continue;
                };
                for item in values {
                    if state.events.len() >= MAX_TRACE_EVENTS {
                        break;
                    }
                    state.events.push(item.clone());
                }
            }
            "Tracing.tracingComplete" => {
                let mut trace = debug.trace_state.write().await;
                let Some(state) = trace.take() else {
                    continue;
                };
                if let Some(tx) = state.done_tx {
                    let _ = tx.send(Ok(state.events));
                }
            }
            _ => {}
        }
    }
}

async fn discover_downloaded_file(
    download_dir: &Path,
    suggested_filename: &str,
    timeout: Duration,
) -> Option<PathBuf> {
    let deadline = std::time::Instant::now() + timeout;
    let suggested = suggested_filename.trim();
    let suggested_path = if suggested.is_empty() {
        None
    } else {
        Some(download_dir.join(suggested))
    };

    loop {
        if let Some(path) = suggested_path.as_deref() {
            if path.exists() && !path.to_string_lossy().ends_with(".crdownload") {
                return Some(path.to_path_buf());
            }
        }

        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(download_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if name.ends_with(".crdownload") {
                        continue;
                    }
                    candidates.push(path);
                }
            }
        }

        if candidates.len() == 1 {
            return candidates.pop();
        }
        if candidates.len() > 1 {
            candidates.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
            return candidates.last().cloned();
        }

        if std::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

impl Browser {
    /// Connect to an existing browser instance.
    pub async fn connect(ws_url: &str) -> drbot_core::Result<Self> {
        info!("Connecting to browser at: {}", ws_url);

        let (cdp, events) = CdpConnection::connect(ws_url).await?;
        let cdp = Arc::new(cdp);
        let debug = Arc::new(BrowserDebugState::default());
        let event_task = Some(tokio::spawn(browser_event_loop(
            events,
            debug.clone(),
            cdp.clone(),
        )));

        // Enable necessary domains
        cdp.send(
            "Target.setDiscoverTargets",
            Some(serde_json::json!({"discover": true})),
        )
        .await?;

        Ok(Self {
            cdp,
            debug,
            event_task,
            process: None,
            ws_url: ws_url.to_string(),
        })
    }

    /// Launch a new browser instance.
    pub async fn launch(options: BrowserOptions) -> drbot_core::Result<Self> {
        let executable = options
            .executable
            .or_else(find_chrome_executable)
            .ok_or_else(|| {
                drbot_core::Error::NotFound("Chrome executable not found".to_string())
            })?;

        let port = if options.port == 0 {
            // Find an available port
            let listener = std::net::TcpListener::bind("127.0.0.1:0")
                .map_err(|e| drbot_core::Error::Internal(format!("Failed to bind port: {}", e)))?;
            listener.local_addr().unwrap().port()
        } else {
            options.port
        };

        let mut args = vec![
            format!("--remote-debugging-port={}", port),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
        ];

        if options.headless {
            args.push("--headless=new".to_string());
        }

        if let Some(ref user_data_dir) = options.user_data_dir {
            args.push(format!("--user-data-dir={}", user_data_dir));
        }

        args.extend(options.args);

        info!("Launching browser: {} {:?}", executable, args);

        let process = Command::new(&executable)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| drbot_core::Error::Internal(format!("Failed to launch browser: {}", e)))?;

        // Wait for browser to start and get WebSocket URL
        let ws_url = wait_for_browser(port).await?;

        let (cdp, events) = CdpConnection::connect(&ws_url).await?;
        let cdp = Arc::new(cdp);
        let debug = Arc::new(BrowserDebugState::default());
        let event_task = Some(tokio::spawn(browser_event_loop(
            events,
            debug.clone(),
            cdp.clone(),
        )));

        cdp.send(
            "Target.setDiscoverTargets",
            Some(serde_json::json!({"discover": true})),
        )
        .await?;

        Ok(Self {
            cdp,
            debug,
            event_task,
            process: Some(process),
            ws_url,
        })
    }

    /// Create a new page (tab).
    pub async fn new_page(&self) -> drbot_core::Result<Page> {
        let result = self
            .cdp
            .send(
                "Target.createTarget",
                Some(serde_json::json!({"url": "about:blank"})),
            )
            .await?;

        let target_id = result
            .get("targetId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| drbot_core::Error::Internal("No targetId in response".to_string()))?
            .to_string();

        self.attach_page(&target_id).await
    }

    /// Attach to an existing page (tab) by target ID.
    pub async fn attach_page(&self, target_id: &str) -> drbot_core::Result<Page> {
        // Attach to target (flattened sessions require `sessionId` per call).
        let attach_result = self
            .cdp
            .send(
                "Target.attachToTarget",
                Some(serde_json::json!({
                    "targetId": target_id,
                    "flatten": true,
                })),
            )
            .await?;

        let session_id = attach_result
            .get("sessionId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(session_id) = session_id.as_deref() {
            self.debug.register_session(target_id, session_id).await;
        }

        if let Some(session_id) = session_id.as_deref() {
            // Enable domains in the attached session.
            let _ = self
                .cdp
                .send_with_session("Page.enable", None, Some(session_id))
                .await;
            let _ = self
                .cdp
                .send_with_session("Runtime.enable", None, Some(session_id))
                .await;
            let _ = self
                .cdp
                .send_with_session("Network.enable", None, Some(session_id))
                .await;
            let _ = self
                .cdp
                .send_with_session("DOM.enable", None, Some(session_id))
                .await;
        }

        Ok(Page::new(
            self.cdp.clone(),
            target_id.to_string(),
            session_id,
        ))
    }

    /// Get all pages (tabs).
    pub async fn pages(&self) -> drbot_core::Result<Vec<TargetInfo>> {
        let result = self.cdp.send("Target.getTargets", None).await?;

        let targets: Vec<TargetInfo> = result
            .get("targetInfos")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        Ok(targets
            .into_iter()
            .filter(|t| t.target_type == "page")
            .collect())
    }

    /// Close a page by target ID.
    pub async fn close_page(&self, target_id: &str) -> drbot_core::Result<()> {
        self.cdp
            .send(
                "Target.closeTarget",
                Some(serde_json::json!({"targetId": target_id})),
            )
            .await?;
        self.debug.forget_target(target_id).await;
        Ok(())
    }

    /// Get buffered console messages for a target.
    pub async fn console_messages(&self, target_id: &str) -> Vec<ConsoleMessage> {
        self.debug.console_messages(target_id).await
    }

    /// Get buffered page errors for a target.
    pub async fn page_errors(&self, target_id: &str, clear: bool) -> Vec<BrowserPageError> {
        self.debug.page_errors(target_id, clear).await
    }

    /// Get buffered network requests for a target.
    pub async fn network_requests(
        &self,
        target_id: &str,
        filter: Option<&str>,
        clear: bool,
    ) -> Vec<BrowserNetworkRequest> {
        self.debug.network_requests(target_id, filter, clear).await
    }

    async fn session_id_for_target(&self, target_id: &str) -> drbot_core::Result<String> {
        if let Some(session_id) = self.debug.session_for_target(target_id).await {
            return Ok(session_id);
        }

        // Best-effort: attach so we can run target-scoped commands.
        let _ = self.attach_page(target_id).await?;
        self.debug
            .session_for_target(target_id)
            .await
            .ok_or_else(|| {
                drbot_core::Error::Internal(format!("No session id for target {}", target_id))
            })
    }

    /// Get all cookies for the browser context (best-effort).
    pub async fn cookies_get_all(&self, target_id: &str) -> drbot_core::Result<Vec<Value>> {
        let session_id = self.session_id_for_target(target_id).await?;
        let result = self
            .cdp
            .send_with_session("Network.getAllCookies", None, Some(&session_id))
            .await?;
        let cookies = result
            .get("cookies")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(cookies)
    }

    /// Set a cookie (best-effort).
    pub async fn cookies_set(
        &self,
        target_id: &str,
        cookie: BrowserSetCookie,
    ) -> drbot_core::Result<()> {
        let session_id = self.session_id_for_target(target_id).await?;
        let params =
            serde_json::to_value(cookie).map_err(|e| drbot_core::Error::Internal(e.to_string()))?;
        let result = self
            .cdp
            .send_with_session("Network.setCookie", Some(params), Some(&session_id))
            .await?;
        if result.get("success").and_then(|v| v.as_bool()) == Some(false) {
            return Err(drbot_core::Error::Internal(
                "Failed to set cookie".to_string(),
            ));
        }
        Ok(())
    }

    /// Clear all cookies (best-effort).
    pub async fn cookies_clear(&self, target_id: &str) -> drbot_core::Result<()> {
        let session_id = self.session_id_for_target(target_id).await?;
        self.cdp
            .send_with_session("Network.clearBrowserCookies", None, Some(&session_id))
            .await?;
        Ok(())
    }

    /// Set extra HTTP headers for future requests in a tab/session (best-effort).
    pub async fn set_extra_http_headers(
        &self,
        target_id: &str,
        headers: HashMap<String, String>,
    ) -> drbot_core::Result<()> {
        let session_id = self.session_id_for_target(target_id).await?;
        self.cdp
            .send_with_session(
                "Network.setExtraHTTPHeaders",
                Some(serde_json::json!({ "headers": headers })),
                Some(&session_id),
            )
            .await?;
        Ok(())
    }

    /// Emulate offline/online mode (best-effort).
    pub async fn set_offline(&self, target_id: &str, offline: bool) -> drbot_core::Result<()> {
        let session_id = self.session_id_for_target(target_id).await?;
        let (download, upload) = if offline { (0, 0) } else { (-1, -1) };
        self.cdp
            .send_with_session(
                "Network.emulateNetworkConditions",
                Some(serde_json::json!({
                    "offline": offline,
                    "latency": 0,
                    "downloadThroughput": download,
                    "uploadThroughput": upload,
                })),
                Some(&session_id),
            )
            .await?;
        Ok(())
    }

    /// Set HTTP authentication credentials (best-effort).
    pub async fn set_http_credentials(
        &self,
        target_id: &str,
        username: Option<String>,
        password: Option<String>,
        clear: bool,
    ) -> drbot_core::Result<()> {
        let session_id = self.session_id_for_target(target_id).await?;
        if clear {
            self.debug.clear_http_credentials().await;
            let _ = self
                .cdp
                .send_with_session("Fetch.disable", None, Some(&session_id))
                .await;
            return Ok(());
        }

        let username = username.unwrap_or_default();
        if username.trim().is_empty() {
            return Err(drbot_core::Error::Internal(
                "username is required (or set clear=true)".to_string(),
            ));
        }
        let password = password.unwrap_or_default();
        self.debug
            .set_http_credentials(username.clone(), password.clone())
            .await;

        // Note: enabling Fetch may pause requests; we continue them in the event loop and only
        // provide credentials when the request is challenged.
        self.cdp
            .send_with_session(
                "Fetch.enable",
                Some(serde_json::json!({
                    "handleAuthRequests": true,
                })),
                Some(&session_id),
            )
            .await?;
        Ok(())
    }

    /// Set geolocation override and grant geolocation permission for `origin` (best-effort).
    pub async fn set_geolocation(
        &self,
        target_id: &str,
        latitude: Option<f64>,
        longitude: Option<f64>,
        accuracy: Option<f64>,
        origin: Option<String>,
        clear: bool,
    ) -> drbot_core::Result<()> {
        let session_id = self.session_id_for_target(target_id).await?;
        if clear {
            let _ = self
                .cdp
                .send_with_session(
                    "Emulation.clearGeolocationOverride",
                    None,
                    Some(&session_id),
                )
                .await;
            let _ = self.cdp.send("Browser.resetPermissions", None).await;
            return Ok(());
        }

        let Some(latitude) = latitude else {
            return Err(drbot_core::Error::Internal(
                "latitude is required (or set clear=true)".to_string(),
            ));
        };
        let Some(longitude) = longitude else {
            return Err(drbot_core::Error::Internal(
                "longitude is required (or set clear=true)".to_string(),
            ));
        };

        self.cdp
            .send_with_session(
                "Emulation.setGeolocationOverride",
                Some(serde_json::json!({
                    "latitude": latitude,
                    "longitude": longitude,
                    "accuracy": accuracy,
                })),
                Some(&session_id),
            )
            .await?;

        if let Some(origin) = origin
            .as_deref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            let _ = self
                .cdp
                .send(
                    "Browser.grantPermissions",
                    Some(serde_json::json!({
                        "origin": origin,
                        "permissions": ["geolocation"],
                    })),
                )
                .await;
        }

        Ok(())
    }

    /// Emulate media features (best-effort).
    pub async fn emulate_media(
        &self,
        target_id: &str,
        color_scheme: Option<String>,
    ) -> drbot_core::Result<()> {
        let session_id = self.session_id_for_target(target_id).await?;
        let features = if let Some(scheme) = color_scheme.as_deref().map(|s| s.trim()) {
            if scheme.is_empty() {
                Vec::new()
            } else {
                vec![serde_json::json!({
                    "name": "prefers-color-scheme",
                    "value": scheme,
                })]
            }
        } else {
            Vec::new()
        };
        self.cdp
            .send_with_session(
                "Emulation.setEmulatedMedia",
                Some(serde_json::json!({
                    "features": features,
                })),
                Some(&session_id),
            )
            .await?;
        Ok(())
    }

    /// Set the timezone override (best-effort).
    pub async fn set_timezone(&self, target_id: &str, timezone_id: &str) -> drbot_core::Result<()> {
        let session_id = self.session_id_for_target(target_id).await?;
        let timezone_id = timezone_id.trim();
        if timezone_id.is_empty() {
            return Err(drbot_core::Error::Internal(
                "timezoneId is required".to_string(),
            ));
        }
        match self
            .cdp
            .send_with_session(
                "Emulation.setTimezoneOverride",
                Some(serde_json::json!({ "timezoneId": timezone_id })),
                Some(&session_id),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("Timezone override is already in effect") {
                    return Ok(());
                }
                Err(err)
            }
        }
    }

    /// Set the locale override (best-effort).
    pub async fn set_locale(&self, target_id: &str, locale: &str) -> drbot_core::Result<()> {
        let session_id = self.session_id_for_target(target_id).await?;
        let locale = locale.trim();
        if locale.is_empty() {
            return Err(drbot_core::Error::Internal(
                "locale is required".to_string(),
            ));
        }
        match self
            .cdp
            .send_with_session(
                "Emulation.setLocaleOverride",
                Some(serde_json::json!({ "locale": locale })),
                Some(&session_id),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("Another locale override is already in effect") {
                    return Ok(());
                }
                Err(err)
            }
        }
    }

    /// Set device emulation preset (best-effort; limited presets).
    pub async fn set_device(&self, target_id: &str, name: &str) -> drbot_core::Result<()> {
        let session_id = self.session_id_for_target(target_id).await?;
        let name = name.trim();
        if name.is_empty() {
            return Err(drbot_core::Error::Internal(
                "device name is required".to_string(),
            ));
        }

        #[derive(Clone, Copy)]
        struct DevicePreset {
            user_agent: &'static str,
            width: u32,
            height: u32,
            device_scale_factor: f64,
            mobile: bool,
            has_touch: bool,
            locale: Option<&'static str>,
        }

        let preset = match name.to_ascii_lowercase().as_str() {
            "iphone 13" | "iphone13" | "iphone" => DevicePreset {
                user_agent: "Mozilla/5.0 (iPhone; CPU iPhone OS 15_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/15.0 Mobile/15E148 Safari/604.1",
                width: 390,
                height: 844,
                device_scale_factor: 3.0,
                mobile: true,
                has_touch: true,
                locale: Some("en-US"),
            },
            "pixel 5" | "pixel5" | "android" => DevicePreset {
                user_agent: "Mozilla/5.0 (Linux; Android 12; Pixel 5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36",
                width: 393,
                height: 851,
                device_scale_factor: 2.75,
                mobile: true,
                has_touch: true,
                locale: Some("en-US"),
            },
            "desktop chrome" | "desktop" => DevicePreset {
                user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
                width: 1280,
                height: 720,
                device_scale_factor: 1.0,
                mobile: false,
                has_touch: false,
                locale: Some("en-US"),
            },
            _ => {
                return Err(drbot_core::Error::Internal(format!(
                    "Unknown device \"{}\". Supported: iPhone 13, Pixel 5, Desktop Chrome.",
                    name
                )));
            }
        };

        if !preset.user_agent.trim().is_empty() {
            let mut ua_params = serde_json::json!({
                "userAgent": preset.user_agent,
            });
            if let Some(locale) = preset.locale {
                ua_params["acceptLanguage"] = serde_json::json!(locale);
            }
            self.cdp
                .send_with_session(
                    "Emulation.setUserAgentOverride",
                    Some(ua_params),
                    Some(&session_id),
                )
                .await?;
        }

        self.cdp
            .send_with_session(
                "Emulation.setDeviceMetricsOverride",
                Some(serde_json::json!({
                    "mobile": preset.mobile,
                    "width": preset.width,
                    "height": preset.height,
                    "deviceScaleFactor": preset.device_scale_factor,
                    "screenWidth": preset.width,
                    "screenHeight": preset.height,
                })),
                Some(&session_id),
            )
            .await?;

        if preset.has_touch {
            let _ = self
                .cdp
                .send_with_session(
                    "Emulation.setTouchEmulationEnabled",
                    Some(serde_json::json!({ "enabled": true })),
                    Some(&session_id),
                )
                .await;
        }

        Ok(())
    }

    /// Start a CDP trace (best-effort).
    pub async fn trace_start(&self) -> drbot_core::Result<()> {
        let id = self.debug.next_arm_id();
        {
            let mut trace = self.debug.trace_state.write().await;
            if trace.is_some() {
                return Err(drbot_core::Error::Internal(
                    "trace is already active".to_string(),
                ));
            }
            *trace = Some(TraceState {
                id,
                events: Vec::new(),
                done_tx: None,
            });
        }

        if let Err(err) = self
            .cdp
            .send(
                "Tracing.start",
                Some(serde_json::json!({
                    "transferMode": "ReportEvents",
                })),
            )
            .await
        {
            let mut trace = self.debug.trace_state.write().await;
            if trace.as_ref().map(|t| t.id) == Some(id) {
                *trace = None;
            }
            return Err(err);
        }

        Ok(())
    }

    /// Stop the current CDP trace and return a trace JSON payload (best-effort).
    pub async fn trace_stop(&self, timeout: Duration) -> drbot_core::Result<Value> {
        let (trace_id, rx) = {
            let mut trace = self.debug.trace_state.write().await;
            let Some(state) = trace.as_mut() else {
                return Err(drbot_core::Error::Internal(
                    "trace is not active".to_string(),
                ));
            };
            if state.done_tx.is_some() {
                return Err(drbot_core::Error::Internal(
                    "trace stop already requested".to_string(),
                ));
            }
            let (tx, rx) = oneshot::channel::<Result<Vec<Value>, String>>();
            state.done_tx = Some(tx);
            (state.id, rx)
        };

        let _ = self.cdp.send("Tracing.end", None).await;

        let events = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(events))) => events,
            Ok(Ok(Err(msg))) => {
                let mut trace = self.debug.trace_state.write().await;
                if trace.as_ref().map(|t| t.id) == Some(trace_id) {
                    *trace = None;
                }
                return Err(drbot_core::Error::Internal(msg));
            }
            Ok(Err(_)) => {
                let mut trace = self.debug.trace_state.write().await;
                if trace.as_ref().map(|t| t.id) == Some(trace_id) {
                    *trace = None;
                }
                return Err(drbot_core::Error::Internal(
                    "trace receiver dropped".to_string(),
                ));
            }
            Err(_) => {
                let mut trace = self.debug.trace_state.write().await;
                if trace.as_ref().map(|t| t.id) == Some(trace_id) {
                    *trace = None;
                }
                return Err(drbot_core::Error::Internal(
                    "timeout waiting for trace".to_string(),
                ));
            }
        };

        Ok(serde_json::json!({ "traceEvents": events }))
    }

    /// Arm a JavaScript dialog handler (best-effort).
    pub async fn arm_dialog(
        &self,
        target_id: &str,
        accept: bool,
        prompt_text: Option<String>,
        timeout: Duration,
    ) -> drbot_core::Result<()> {
        let _session_id = self
            .debug
            .session_for_target(target_id)
            .await
            .ok_or_else(|| {
                drbot_core::Error::Internal(format!(
                    "no session for target {}; is it attached?",
                    target_id
                ))
            })?;

        let arm_id = self.debug.next_arm_id();
        self.debug
            .set_dialog_arm(
                target_id,
                DialogArm {
                    id: arm_id,
                    accept,
                    prompt_text,
                },
            )
            .await;

        let debug = self.debug.clone();
        let target = target_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            debug.clear_dialog_arm_if_matches(&target, arm_id).await;
        });

        Ok(())
    }

    /// Arm a file upload handler by intercepting the next file chooser (best-effort).
    pub async fn arm_file_upload(
        &self,
        target_id: &str,
        paths: Vec<String>,
        timeout: Duration,
    ) -> drbot_core::Result<()> {
        let session_id = self
            .debug
            .session_for_target(target_id)
            .await
            .ok_or_else(|| {
                drbot_core::Error::Internal(format!(
                    "no session for target {}; is it attached?",
                    target_id
                ))
            })?;

        let _ = self
            .cdp
            .send_with_session(
                "Page.setInterceptFileChooserDialog",
                Some(serde_json::json!({"enabled": true})),
                Some(&session_id),
            )
            .await;

        let arm_id = self.debug.next_arm_id();
        self.debug
            .set_upload_arm(target_id, UploadArm { id: arm_id, paths })
            .await;

        let debug = self.debug.clone();
        let cdp = self.cdp.clone();
        let target = target_id.to_string();
        let session = session_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            if debug.clear_upload_arm_if_matches(&target, arm_id).await {
                let _ = cdp
                    .send_with_session(
                        "Page.setInterceptFileChooserDialog",
                        Some(serde_json::json!({"enabled": false})),
                        Some(&session),
                    )
                    .await;
            }
        });

        Ok(())
    }

    /// Set files on an `<input type="file">` via CDP (best-effort).
    pub async fn set_input_files(
        &self,
        target_id: &str,
        selector: &str,
        paths: Vec<String>,
    ) -> drbot_core::Result<()> {
        let session_id = self
            .debug
            .session_for_target(target_id)
            .await
            .ok_or_else(|| {
                drbot_core::Error::Internal(format!(
                    "no session for target {}; is it attached?",
                    target_id
                ))
            })?;

        let _ = self
            .cdp
            .send_with_session("DOM.enable", None, Some(&session_id))
            .await;

        let doc = self
            .cdp
            .send_with_session("DOM.getDocument", None, Some(&session_id))
            .await?;
        let root_id = doc
            .get("root")
            .and_then(|v| v.get("nodeId"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        if root_id <= 0 {
            return Err(drbot_core::Error::Internal(
                "DOM.getDocument returned no root nodeId".to_string(),
            ));
        }

        let found = self
            .cdp
            .send_with_session(
                "DOM.querySelector",
                Some(serde_json::json!({
                    "nodeId": root_id,
                    "selector": selector,
                })),
                Some(&session_id),
            )
            .await?;
        let node_id = found.get("nodeId").and_then(|v| v.as_i64()).unwrap_or(0);
        if node_id <= 0 {
            return Err(drbot_core::Error::Internal(format!(
                "file input not found for selector: {}",
                selector
            )));
        }

        self.cdp
            .send_with_session(
                "DOM.setFileInputFiles",
                Some(serde_json::json!({
                    "nodeId": node_id,
                    "files": paths,
                })),
                Some(&session_id),
            )
            .await?;

        Ok(())
    }

    /// Fetch a captured response body by CDP request id (best-effort).
    pub async fn response_body(
        &self,
        target_id: &str,
        request_id: &str,
    ) -> drbot_core::Result<String> {
        let session_id = self
            .debug
            .session_for_target(target_id)
            .await
            .ok_or_else(|| {
                drbot_core::Error::Internal(format!(
                    "no session for target {}; is it attached?",
                    target_id
                ))
            })?;

        let result = self
            .cdp
            .send_with_session(
                "Network.getResponseBody",
                Some(serde_json::json!({
                    "requestId": request_id,
                })),
                Some(&session_id),
            )
            .await?;

        let body_raw = result.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let base64_encoded = result
            .get("base64Encoded")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !base64_encoded {
            return Ok(body_raw.to_string());
        }

        match base64::engine::general_purpose::STANDARD.decode(body_raw) {
            Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).to_string()),
            Err(_) => Ok(body_raw.to_string()),
        }
    }

    /// Wait for the next download from a target (best-effort).
    pub async fn wait_for_download(
        &self,
        target_id: &str,
        download_dir: impl Into<PathBuf>,
        timeout: Duration,
    ) -> drbot_core::Result<BrowserDownload> {
        let session_id = self
            .debug
            .session_for_target(target_id)
            .await
            .ok_or_else(|| {
                drbot_core::Error::Internal(format!(
                    "no session for target {}; is it attached?",
                    target_id
                ))
            })?;

        let download_dir: PathBuf = download_dir.into();
        std::fs::create_dir_all(&download_dir).map_err(|e| {
            drbot_core::Error::Internal(format!(
                "failed to create download dir {}: {}",
                download_dir.to_string_lossy(),
                e
            ))
        })?;

        // Best-effort: configure downloads for the page session.
        let _ = self
            .cdp
            .send_with_session(
                "Page.setDownloadBehavior",
                Some(serde_json::json!({
                    "behavior": "allow",
                    "downloadPath": download_dir.to_string_lossy(),
                })),
                Some(&session_id),
            )
            .await;

        let arm_id = self.debug.next_arm_id();
        let (tx, rx) = oneshot::channel::<Result<BrowserDownload, String>>();
        self.debug
            .set_download_arm(
                target_id,
                DownloadArm {
                    id: arm_id,
                    download_dir: download_dir.clone(),
                    tx,
                    guid: None,
                    url: None,
                    suggested_filename: None,
                },
            )
            .await;

        let debug = self.debug.clone();
        let target = target_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            if let Some(arm) = debug.clear_download_arm_if_matches(&target, arm_id).await {
                let _ = arm.tx.send(Err("Timeout waiting for download".to_string()));
            }
        });

        let result = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(download))) => Ok(download),
            Ok(Ok(Err(msg))) => Err(drbot_core::Error::Internal(msg)),
            Ok(Err(_)) => Err(drbot_core::Error::Internal(
                "download waiter dropped".to_string(),
            )),
            Err(_) => Err(drbot_core::Error::Internal(
                "timeout waiting for download".to_string(),
            )),
        };

        // Best-effort: reset download behavior.
        let _ = self
            .cdp
            .send_with_session(
                "Page.setDownloadBehavior",
                Some(serde_json::json!({ "behavior": "default" })),
                Some(&session_id),
            )
            .await;

        result
    }

    /// Click an element and wait for the resulting download (best-effort).
    pub async fn download_via_click(
        &self,
        page: &Page,
        selector: &str,
        download_dir: impl Into<PathBuf>,
        timeout: Duration,
    ) -> drbot_core::Result<BrowserDownload> {
        let target_id = page.target_id();
        let session_id = self
            .debug
            .session_for_target(target_id)
            .await
            .ok_or_else(|| {
                drbot_core::Error::Internal(format!(
                    "no session for target {}; is it attached?",
                    target_id
                ))
            })?;

        let download_dir: PathBuf = download_dir.into();
        std::fs::create_dir_all(&download_dir).map_err(|e| {
            drbot_core::Error::Internal(format!(
                "failed to create download dir {}: {}",
                download_dir.to_string_lossy(),
                e
            ))
        })?;

        let _ = self
            .cdp
            .send_with_session(
                "Page.setDownloadBehavior",
                Some(serde_json::json!({
                    "behavior": "allow",
                    "downloadPath": download_dir.to_string_lossy(),
                })),
                Some(&session_id),
            )
            .await;

        let arm_id = self.debug.next_arm_id();
        let (tx, rx) = oneshot::channel::<Result<BrowserDownload, String>>();
        self.debug
            .set_download_arm(
                target_id,
                DownloadArm {
                    id: arm_id,
                    download_dir: download_dir.clone(),
                    tx,
                    guid: None,
                    url: None,
                    suggested_filename: None,
                },
            )
            .await;

        let debug = self.debug.clone();
        let target = target_id.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            if let Some(arm) = debug.clear_download_arm_if_matches(&target, arm_id).await {
                let _ = arm.tx.send(Err("Timeout waiting for download".to_string()));
            }
        });

        let click_result = page.click(selector).await;
        if let Err(err) = click_result {
            let _ = self
                .debug
                .clear_download_arm_if_matches(target_id, arm_id)
                .await;
            let _ = self
                .cdp
                .send_with_session(
                    "Page.setDownloadBehavior",
                    Some(serde_json::json!({ "behavior": "default" })),
                    Some(&session_id),
                )
                .await;
            return Err(err);
        }

        let result = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(download))) => Ok(download),
            Ok(Ok(Err(msg))) => Err(drbot_core::Error::Internal(msg)),
            Ok(Err(_)) => Err(drbot_core::Error::Internal(
                "download waiter dropped".to_string(),
            )),
            Err(_) => Err(drbot_core::Error::Internal(
                "timeout waiting for download".to_string(),
            )),
        };

        let _ = self
            .cdp
            .send_with_session(
                "Page.setDownloadBehavior",
                Some(serde_json::json!({ "behavior": "default" })),
                Some(&session_id),
            )
            .await;

        result
    }

    /// Get browser version info.
    pub async fn version(&self) -> drbot_core::Result<serde_json::Value> {
        self.cdp.send("Browser.getVersion", None).await
    }

    /// Close the browser.
    pub async fn close(mut self) -> drbot_core::Result<()> {
        info!("Closing browser");

        if let Some(handle) = self.event_task.take() {
            handle.abort();
        }

        // Try to close gracefully via CDP
        let _ = self.cdp.send("Browser.close", None).await;

        // Kill process if we launched it
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
        }

        Ok(())
    }

    /// Get WebSocket URL.
    pub fn ws_url(&self) -> &str {
        &self.ws_url
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        if let Some(handle) = self.event_task.take() {
            handle.abort();
        }
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
        }
    }
}

/// Find Chrome executable on the system.
fn find_chrome_executable() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let paths = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ];
        for path in paths {
            if std::path::Path::new(path).exists() {
                return Some(path.to_string());
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let paths = [
            "/usr/bin/google-chrome",
            "/usr/bin/chromium-browser",
            "/usr/bin/chromium",
            "/usr/bin/microsoft-edge",
        ];
        for path in paths {
            if std::path::Path::new(path).exists() {
                return Some(path.to_string());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let paths = [
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        ];
        for path in paths {
            if std::path::Path::new(path).exists() {
                return Some(path.to_string());
            }
        }
    }

    None
}

/// Wait for browser to be ready and return WebSocket URL.
async fn wait_for_browser(port: u16) -> drbot_core::Result<String> {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/json/version", port);

    for attempt in 0..50 {
        match client.get(&url).send().await {
            Ok(resp) => {
                if let Ok(info) = resp.json::<VersionInfo>().await {
                    if let Some(ws_url) = info.websocket_debugger_url {
                        debug!("Browser ready after {} attempts", attempt + 1);
                        return Ok(ws_url);
                    }
                }
            }
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }

    Err(drbot_core::Error::Timeout(
        "Browser failed to start".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_options_default() {
        let opts = BrowserOptions::default();
        assert!(opts.headless);
        assert!(opts.executable.is_none());
        assert_eq!(opts.port, 0);
    }

    #[test]
    fn test_version_info_deserialize() {
        let json = r#"{
            "Browser": "Chrome/120.0.0.0",
            "Protocol-Version": "1.3",
            "webSocketDebuggerUrl": "ws://127.0.0.1:9222/devtools/browser/abc123"
        }"#;
        let info: VersionInfo = serde_json::from_str(json).unwrap();
        assert!(info.browser.contains("Chrome"));
        assert!(info.websocket_debugger_url.is_some());
    }

    #[test]
    fn test_target_info_deserialize() {
        let json = r#"{
            "targetId": "ABC123",
            "type": "page",
            "title": "Test Page",
            "url": "https://example.com",
            "attached": false
        }"#;
        let info: TargetInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.target_id, "ABC123");
        assert_eq!(info.target_type, "page");
    }

    #[test]
    fn test_browser_set_cookie_serialize() {
        let cookie = BrowserSetCookie {
            name: "session".to_string(),
            value: "abc123".to_string(),
            url: Some("https://example.com".to_string()),
            domain: None,
            path: None,
            expires: Some(123.0),
            http_only: Some(true),
            secure: Some(true),
            same_site: Some("Lax".to_string()),
        };
        let v = serde_json::to_value(cookie).unwrap();
        assert_eq!(v.get("name").and_then(|v| v.as_str()), Some("session"));
        assert_eq!(v.get("httpOnly").and_then(|v| v.as_bool()), Some(true));
        assert!(v.get("http_only").is_none());
    }

    #[test]
    fn test_find_chrome_executable() {
        // This test just ensures the function doesn't panic
        let _ = find_chrome_executable();
    }
}
