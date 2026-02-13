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
use std::time::Instant;
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
                build_health_payload_from_snapshot(&state, next).await,
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
    // Build a stable snapshot for change detection (avoid including timestamps/durations).
    //
    // The OpenClaw Control UI expects the `health` method payload to resemble the
    // `HealthSummary` shape (see upstream `commands/health.ts`), but for events we
    // only need to know when key health inputs have changed (channels/linking).

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
    ];

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

    let mut channel_labels = serde_json::Map::new();
    for key in &channel_order {
        channel_labels.insert((*key).to_string(), json!(label_for(key)));
    }

    let heartbeat_every_ms = std::env::var("DRBOT_OPENCLAW_HEARTBEAT_EVERY_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v >= 1_000)
        .unwrap_or(30 * 60 * 1000);
    let heartbeat_seconds = heartbeat_every_ms / 1000;

    let whatsapp = state.openclaw_web_login().snapshot_whatsapp();
    let runtime = state.channel_manager().runtime_snapshot().await;

    let mut channels_obj = serde_json::Map::new();
    for key in &channel_order {
        let rt = runtime.get(*key);
        let configured = rt.map(|r| r.configured).unwrap_or(false);

        let mut obj = serde_json::Map::new();
        obj.insert("configured".to_string(), json!(configured));

        // OpenClaw's health UI treats WhatsApp as the primary "linked" channel.
        if *key == "whatsapp" && configured {
            obj.insert("linked".to_string(), json!(whatsapp.connected));
        }

        channels_obj.insert((*key).to_string(), serde_json::Value::Object(obj));
    }

    json!({
        "channels": serde_json::Value::Object(channels_obj),
        "channelOrder": channel_order,
        "channelLabels": serde_json::Value::Object(channel_labels),
        "heartbeatSeconds": heartbeat_seconds,
    })
}

pub async fn build_health_payload(state: &GatewayState) -> serde_json::Value {
    let snapshot = build_health_snapshot(state).await;
    build_health_payload_from_snapshot(state, snapshot).await
}

async fn build_health_payload_from_snapshot(
    state: &GatewayState,
    snapshot: serde_json::Value,
) -> serde_json::Value {
    let started = Instant::now();
    let ts = now_ms();

    let sessions = build_sessions_summary(state, ts).await;

    let duration_ms: u64 = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);

    let mut out = serde_json::Map::new();
    out.insert("ok".to_string(), json!(true));
    out.insert("ts".to_string(), json!(ts));
    out.insert("durationMs".to_string(), json!(duration_ms));

    // Merge stable snapshot fields.
    if let serde_json::Value::Object(map) = snapshot {
        for (k, v) in map {
            out.insert(k, v);
        }
    }

    out.insert("sessions".to_string(), sessions);

    serde_json::Value::Object(out)
}

async fn build_sessions_summary(state: &GatewayState, now_ms: u64) -> serde_json::Value {
    let path = state
        .config()
        .storage
        .database_path
        .to_string_lossy()
        .to_string();
    let mut count: usize = 0;
    let mut recent: Vec<serde_json::Value> = Vec::new();

    if let Some(store) = state.session_store() {
        let mut list = store
            .list(drbot_sessions::ListOptions {
                include_archived: true,
                ..Default::default()
            })
            .await
            .unwrap_or_default();
        count = list.len();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        for s in list.into_iter().take(12) {
            let key = if s.channel_type == "openclaw" {
                s.channel_id.clone()
            } else {
                format!("{}:{}", s.channel_type, s.channel_id)
            };
            let updated_at_ms: u64 = s.updated_at.timestamp_millis().try_into().unwrap_or(0);
            let age_ms = now_ms.saturating_sub(updated_at_ms);
            recent.push(json!({
                "key": key,
                "updatedAt": updated_at_ms,
                "age": age_ms,
            }));
        }
    }

    json!({
        "path": path,
        "count": count,
        "recent": recent,
    })
}
