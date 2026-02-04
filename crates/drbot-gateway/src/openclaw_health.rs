//! OpenClaw `health` snapshot + change events.
//!
//! OpenClaw clients use:
//! - `health` method for an on-demand snapshot
//! - `health` events + `stateVersion.health` to avoid polling

use crate::openclaw::broadcast_openclaw_event;
use crate::state::GatewayState;
use drbot_protocol::openclaw::StateVersion;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::debug;

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(0)
}

fn resolve_state_key(state: &GatewayState) -> PathBuf {
    crate::openclaw_paths::resolve_openclaw_state_dir(state.config())
        .unwrap_or_else(|| PathBuf::from(""))
        .join("openclaw-health")
}

#[derive(Debug)]
pub struct OpenclawHealthService {
    key: PathBuf,
    started: AtomicBool,
    last_snapshot: Mutex<Option<serde_json::Value>>,
}

static OPENCLAW_HEALTH_SERVICES: OnceLock<Mutex<HashMap<PathBuf, Arc<OpenclawHealthService>>>> =
    OnceLock::new();

fn openclaw_health_services() -> &'static Mutex<HashMap<PathBuf, Arc<OpenclawHealthService>>> {
    OPENCLAW_HEALTH_SERVICES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub async fn health_service_for_state(state: &GatewayState) -> Arc<OpenclawHealthService> {
    let key = resolve_state_key(state);
    let mut services = openclaw_health_services().lock().await;
    if let Some(svc) = services.get(&key) {
        svc.start_background(state.clone());
        return svc.clone();
    }

    let svc = Arc::new(OpenclawHealthService {
        key: key.clone(),
        started: AtomicBool::new(false),
        last_snapshot: Mutex::new(None),
    });
    svc.start_background(state.clone());
    services.insert(key, svc.clone());
    svc
}

impl OpenclawHealthService {
    fn start_background(self: &Arc<Self>, state: GatewayState) {
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

    async fn run_loop(self: Arc<Self>, state: GatewayState) {
        // Seed baseline snapshot without emitting an event or bumping stateVersion.
        {
            let snapshot = build_health_snapshot(&state).await;
            *self.last_snapshot.lock().await = Some(snapshot);
        }

        let poll_ms = std::env::var("DRBOT_OPENCLAW_HEALTH_POLL_INTERVAL_MS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v >= 250)
            .unwrap_or(5_000);

        let mut interval = tokio::time::interval(Duration::from_millis(poll_ms));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        // Push updates quickly for WhatsApp QR/login transitions.
        let mut whatsapp_rx = state.openclaw_web_login().subscribe_whatsapp();

        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = whatsapp_rx.changed() => {}
            }

            let next = build_health_snapshot(&state).await;
            let mut last = self.last_snapshot.lock().await;
            if last.as_ref() == Some(&next) {
                continue;
            }
            *last = Some(next.clone());

            let version = state.increment_openclaw_health_version();
            debug!(version, key = %self.key.to_string_lossy(), "OpenClaw health state changed");
            broadcast_openclaw_event(
                &state,
                "health",
                build_health_payload_from_snapshot(&state, next),
                Some(StateVersion {
                    presence: state.openclaw_presence_version(),
                    health: version,
                }),
            )
            .await;
        }
    }
}

pub async fn build_health_snapshot(state: &GatewayState) -> serde_json::Value {
    let provider_configured = state.provider().is_some();
    let sessions_enabled = state.session_store().is_some();

    // OpenClaw's `health` primarily reflects gateway availability; configuration issues
    // are reported in `issues` so clients don't treat an unconfigured provider as an
    // offline gateway.
    let status = "ok";
    let mut issues: Vec<String> = Vec::new();
    if !provider_configured {
        issues.push("provider-not-configured".to_string());
    }
    if !sessions_enabled {
        issues.push("session-store-unavailable".to_string());
    }
    issues.sort();

    let whatsapp = state.openclaw_web_login().snapshot_whatsapp();
    let runtime = state.channel_manager().runtime_snapshot().await;

    let mut keys = runtime.keys().cloned().collect::<Vec<_>>();
    keys.sort();

    let mut channels_obj = serde_json::Map::new();
    for key in keys {
        let snap = runtime.get(&key).expect("present");
        let connected = if key == "whatsapp" {
            whatsapp.connected
        } else {
            snap.connected
        };
        channels_obj.insert(
            key.clone(),
            json!({
                "enabled": snap.enabled,
                "configured": snap.configured,
                "running": snap.running,
                "connected": connected,
                "lastError": snap.last_error,
            }),
        );
    }

    json!({
        "status": status,
        "issues": issues,
        "provider": { "configured": provider_configured },
        "sessions": { "enabled": sessions_enabled },
        "heartbeats": { "enabled": state.openclaw_heartbeats_enabled() },
        "whatsapp": {
            "connected": whatsapp.connected,
            "status": whatsapp.status,
            "hasQr": whatsapp.qr_data_url.is_some(),
        },
        "channels": serde_json::Value::Object(channels_obj),
    })
}

pub async fn build_health_payload(state: &GatewayState) -> serde_json::Value {
    let snapshot = build_health_snapshot(state).await;
    build_health_payload_from_snapshot(state, snapshot)
}

fn build_health_payload_from_snapshot(state: &GatewayState, snapshot: serde_json::Value) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    out.insert("ts".to_string(), json!(now_ms()));
    out.insert(
        "uptimeMs".to_string(),
        json!(state.uptime_secs().saturating_mul(1000)),
    );

    match snapshot {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                out.insert(k, v);
            }
        }
        other => {
            out.insert("health".to_string(), other);
        }
    }

    serde_json::Value::Object(out)
}
