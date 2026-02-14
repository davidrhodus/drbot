//! OpenClaw-compatible restart scheduling + sentinel helpers.
//!
//! OpenClaw uses SIGUSR1 as a "restart requested" signal and writes a
//! `restart-sentinel.json` file into the OpenClaw state dir so UIs / operators
//! can understand what triggered the restart.

use crate::state::GatewayState;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;

const SIGUSR1_AUTH_GRACE_MS: u64 = 5_000;

static SIGUSR1_SELF_RESTART_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enable_sigusr1_self_restart() {
    SIGUSR1_SELF_RESTART_ENABLED.store(true, Ordering::Relaxed);
}

fn sigusr1_self_restart_enabled() -> bool {
    SIGUSR1_SELF_RESTART_ENABLED.load(Ordering::Relaxed)
}

const SENTINEL_FILENAME: &str = "restart-sentinel.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledRestart {
    pub ok: bool,
    pub pid: u32,
    pub signal: String,
    #[serde(rename = "delayMs")]
    pub delay_ms: u64,
    pub reason: Option<String>,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RestartSentinelFile {
    version: u32,
    payload: serde_json::Value,
}

#[derive(Debug, Default)]
struct Sigusr1AuthorizationState {
    count: u32,
    expires_at_ms: u64,
}

static SIGUSR1_AUTH: OnceLock<Mutex<Sigusr1AuthorizationState>> = OnceLock::new();

fn sigusr1_auth() -> &'static Mutex<Sigusr1AuthorizationState> {
    SIGUSR1_AUTH.get_or_init(|| Mutex::new(Sigusr1AuthorizationState::default()))
}

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(0)
}

fn normalize_delay_ms(value: Option<u64>) -> u64 {
    let raw = value.unwrap_or(2_000);
    raw.clamp(0, 60_000)
}

pub fn authorize_sigusr1_restart(delay_ms: u64) {
    let delay = delay_ms.min(60_000);
    let expires_at = now_ms()
        .saturating_add(delay)
        .saturating_add(SIGUSR1_AUTH_GRACE_MS);

    let mut st = sigusr1_auth().lock().expect("sigusr1 auth lock poisoned");
    st.count = st.count.saturating_add(1);
    if expires_at > st.expires_at_ms {
        st.expires_at_ms = expires_at;
    }
}

pub fn consume_sigusr1_restart_authorization() -> bool {
    let now = now_ms();
    let mut st = sigusr1_auth().lock().expect("sigusr1 auth lock poisoned");
    if st.count == 0 {
        return false;
    }
    if st.expires_at_ms > 0 && now > st.expires_at_ms {
        st.count = 0;
        st.expires_at_ms = 0;
        return false;
    }
    st.count = st.count.saturating_sub(1);
    if st.count == 0 {
        st.expires_at_ms = 0;
    }
    true
}

pub fn is_sigusr1_restart_externally_allowed() -> bool {
    matches!(
        std::env::var("DRBOT_OPENCLAW_ALLOW_EXTERNAL_RESTART")
            .ok()
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

#[cfg(unix)]
fn send_sigusr1(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGUSR1);
    }
}

#[cfg(not(unix))]
fn send_sigusr1(_pid: u32) {}

pub fn schedule_sigusr1_restart(delay_ms: Option<u64>, reason: Option<&str>) -> ScheduledRestart {
    let delay_ms = normalize_delay_ms(delay_ms);
    let pid = std::process::id();
    let reason = reason
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.len() > 200 {
                s.chars().take(200).collect()
            } else {
                s
            }
        });

    #[cfg(not(unix))]
    {
        return ScheduledRestart {
            ok: false,
            pid,
            signal: "SIGUSR1".to_string(),
            delay_ms,
            reason,
            mode: "unsupported".to_string(),
        };
    }

    #[cfg(unix)]
    authorize_sigusr1_restart(delay_ms);

    #[cfg(unix)]
    if sigusr1_self_restart_enabled() {
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            send_sigusr1(pid);
        });

        return ScheduledRestart {
            ok: true,
            pid,
            signal: "SIGUSR1".to_string(),
            delay_ms,
            reason,
            mode: "signal".to_string(),
        };
    }

    ScheduledRestart {
        ok: true,
        pid,
        signal: "SIGUSR1".to_string(),
        delay_ms,
        reason,
        mode: "authorized".to_string(),
    }
}

fn resolve_restart_sentinel_path(state: &GatewayState) -> Option<PathBuf> {
    crate::openclaw_paths::resolve_openclaw_state_dir(state.config())
        .map(|dir| dir.join(SENTINEL_FILENAME))
}

pub fn write_restart_sentinel_best_effort(
    state: &GatewayState,
    payload: serde_json::Value,
) -> Option<String> {
    let path = resolve_restart_sentinel_path(state)?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = RestartSentinelFile {
        version: 1,
        payload,
    };
    let raw = serde_json::to_string_pretty(&file).ok()?;
    let _ = std::fs::write(&path, format!("{}\n", raw));
    Some(path.to_string_lossy().to_string())
}

pub fn build_restart_sentinel_payload(params: RestartSentinelPayloadParams) -> serde_json::Value {
    json!({
        "kind": params.kind,
        "status": params.status,
        "ts": params.ts_ms,
        "sessionKey": params.session_key,
        "message": params.message,
        "doctorHint": params.doctor_hint,
        "stats": params.stats,
    })
}

pub struct RestartSentinelPayloadParams {
    pub kind: &'static str,
    pub status: &'static str,
    pub ts_ms: u64,
    pub session_key: Option<String>,
    pub message: Option<String>,
    pub doctor_hint: Option<String>,
    pub stats: serde_json::Value,
}
