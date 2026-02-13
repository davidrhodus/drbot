//! OpenClaw system-event + system-presence runtime (v3 compatibility).
//!
//! OpenClaw treats `system-event` as an ephemeral queue of human-readable lines that
//! should be prefixed to the next prompt (heartbeat or agent runs). It also uses
//! `system-event` to update an in-memory "system presence" table (e.g. nodes).
//!
//! drbot keeps these as in-memory stores so they work even when session transcripts
//! are excluded from heartbeats.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::Mutex;

const MAX_SYSTEM_EVENTS_PER_SESSION: usize = 20;
const PRESENCE_TTL_MS: u64 = 5 * 60 * 1000; // 5 minutes
const PRESENCE_MAX_ENTRIES: usize = 200;

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(0)
}

fn normalize_key(raw: Option<&str>) -> Option<String> {
    let trimmed = raw.unwrap_or("").trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_lowercase())
}

// ---------------------------------------------------------------------------
// System events queue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEventEntry {
    pub text: String,
    #[serde(rename = "ts", default)]
    pub ts_ms: u64,
}

#[derive(Debug, Default)]
struct SessionQueue {
    queue: Vec<SystemEventEntry>,
    last_text: Option<String>,
    last_context_key: Option<String>,
}

#[derive(Debug, Default)]
pub struct OpenclawSystemEvents {
    queues: Mutex<HashMap<String, SessionQueue>>,
}

impl OpenclawSystemEvents {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn is_context_changed(&self, session_key: &str, context_key: Option<&str>) -> bool {
        let key = session_key.trim();
        if key.is_empty() {
            return true;
        }
        let normalized = normalize_key(context_key);
        let queues = self.queues.lock().await;
        let existing = queues.get(key);
        normalized != existing.and_then(|q| q.last_context_key.clone())
    }

    pub async fn enqueue(&self, session_key: &str, text: &str, context_key: Option<&str>) {
        let session_key = session_key.trim();
        if session_key.is_empty() {
            return;
        }
        let cleaned = text.trim();
        if cleaned.is_empty() {
            return;
        }

        let mut queues = self.queues.lock().await;
        let entry = queues.entry(session_key.to_string()).or_default();
        let normalized_context = normalize_key(context_key);
        entry.last_context_key = normalized_context;

        if entry.last_text.as_deref() == Some(cleaned) {
            return;
        }
        entry.last_text = Some(cleaned.to_string());
        entry.queue.push(SystemEventEntry {
            text: cleaned.to_string(),
            ts_ms: now_ms(),
        });
        if entry.queue.len() > MAX_SYSTEM_EVENTS_PER_SESSION {
            let overflow = entry.queue.len() - MAX_SYSTEM_EVENTS_PER_SESSION;
            entry.queue.drain(0..overflow);
        }
    }

    pub async fn peek(&self, session_key: &str) -> Vec<SystemEventEntry> {
        let session_key = session_key.trim();
        if session_key.is_empty() {
            return Vec::new();
        }
        let queues = self.queues.lock().await;
        queues
            .get(session_key)
            .map(|q| q.queue.clone())
            .unwrap_or_default()
    }

    pub async fn has_events(&self, session_key: &str) -> bool {
        !self.peek(session_key).await.is_empty()
    }

    pub async fn drain(&self, session_key: &str) -> Vec<SystemEventEntry> {
        let session_key = session_key.trim();
        if session_key.is_empty() {
            return Vec::new();
        }
        let mut queues = self.queues.lock().await;
        let Some(mut q) = queues.remove(session_key) else {
            return Vec::new();
        };
        let out = std::mem::take(&mut q.queue);
        out
    }
}

// ---------------------------------------------------------------------------
// System presence
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPresence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_input_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    pub text: String,
    pub ts: u64,
}

#[derive(Debug, Clone)]
pub struct SystemPresenceUpdate {
    pub key: String,
    pub previous: Option<SystemPresence>,
    pub next: SystemPresence,
    pub changed_keys: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SystemPresencePayload {
    pub text: String,
    pub device_id: Option<String>,
    pub instance_id: Option<String>,
    pub host: Option<String>,
    pub ip: Option<String>,
    pub version: Option<String>,
    pub platform: Option<String>,
    pub device_family: Option<String>,
    pub model_identifier: Option<String>,
    pub last_input_seconds: Option<u64>,
    pub mode: Option<String>,
    pub reason: Option<String>,
    pub roles: Option<Vec<String>>,
    pub scopes: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
}

fn merge_string_list(a: Option<Vec<String>>, b: Option<Vec<String>>) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();
    for list in [a, b] {
        let Some(list) = list else { continue };
        for item in list {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_string();
            if seen.insert(key.clone()) {
                out.push(key);
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[derive(Debug, Default)]
pub struct OpenclawSystemPresence {
    entries: Mutex<HashMap<String, SystemPresence>>,
}

impl OpenclawSystemPresence {
    pub fn new() -> Self {
        Self::default()
    }

    fn resolve_key(payload: &SystemPresencePayload) -> String {
        normalize_key(payload.device_id.as_deref())
            .or_else(|| normalize_key(payload.instance_id.as_deref()))
            .or_else(|| normalize_key(payload.host.as_deref()))
            .or_else(|| normalize_key(payload.ip.as_deref()))
            .unwrap_or_else(|| {
                let base = payload.text.trim();
                if base.is_empty() {
                    "unknown".to_string()
                } else {
                    base.chars().take(64).collect::<String>().to_lowercase()
                }
            })
    }

    pub async fn update(&self, payload: SystemPresencePayload) -> SystemPresenceUpdate {
        let now = now_ms();
        let key = Self::resolve_key(&payload);
        let mut entries = self.entries.lock().await;

        let previous = entries.get(&key).cloned();
        let existing = previous.clone().unwrap_or_default();
        let roles = merge_string_list(existing.roles.clone(), payload.roles.clone());
        let scopes = merge_string_list(existing.scopes.clone(), payload.scopes.clone());

        let next = SystemPresence {
            host: payload.host.or(existing.host),
            ip: payload.ip.or(existing.ip),
            version: payload.version.or(existing.version),
            platform: payload.platform.or(existing.platform),
            device_family: payload.device_family.or(existing.device_family),
            model_identifier: payload.model_identifier.or(existing.model_identifier),
            last_input_seconds: payload.last_input_seconds.or(existing.last_input_seconds),
            mode: payload.mode.or(existing.mode),
            reason: payload.reason.or(existing.reason),
            device_id: payload.device_id.or(existing.device_id),
            roles,
            scopes,
            instance_id: payload.instance_id.or(existing.instance_id),
            text: payload.text.trim().to_string(),
            ts: now,
        };

        entries.insert(key.clone(), next.clone());

        // Track changes for a few keys the UI cares about.
        let mut changed = Vec::new();
        for (name, prev, nextv) in [
            (
                "host",
                previous.as_ref().and_then(|p| p.host.as_deref()),
                next.host.as_deref(),
            ),
            (
                "ip",
                previous.as_ref().and_then(|p| p.ip.as_deref()),
                next.ip.as_deref(),
            ),
            (
                "version",
                previous.as_ref().and_then(|p| p.version.as_deref()),
                next.version.as_deref(),
            ),
            (
                "mode",
                previous.as_ref().and_then(|p| p.mode.as_deref()),
                next.mode.as_deref(),
            ),
            (
                "reason",
                previous.as_ref().and_then(|p| p.reason.as_deref()),
                next.reason.as_deref(),
            ),
        ] {
            if prev != nextv {
                changed.push(name.to_string());
            }
        }

        SystemPresenceUpdate {
            key,
            previous,
            next,
            changed_keys: changed,
        }
    }

    pub async fn list(&self) -> Vec<SystemPresence> {
        let now = now_ms();
        let mut entries = self.entries.lock().await;

        // prune expired
        entries.retain(|_, v| now.saturating_sub(v.ts) <= PRESENCE_TTL_MS);

        // enforce max size (drop oldest by ts)
        if entries.len() > PRESENCE_MAX_ENTRIES {
            let mut items: Vec<(String, u64)> =
                entries.iter().map(|(k, v)| (k.clone(), v.ts)).collect();
            items.sort_by_key(|(_, ts)| *ts);
            let drop = items.len().saturating_sub(PRESENCE_MAX_ENTRIES);
            for (k, _) in items.into_iter().take(drop) {
                entries.remove(&k);
            }
        }

        let mut out: Vec<SystemPresence> = entries.values().cloned().collect();
        out.sort_by(|a, b| b.ts.cmp(&a.ts));
        out
    }
}
