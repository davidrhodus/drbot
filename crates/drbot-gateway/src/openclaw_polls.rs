//! OpenClaw poll tracking (best-effort).
//!
//! OpenClaw's gateway can send polls via `poll`. drbot currently implements
//! polls as text-based messages, but we still track the lifecycle so inbound
//! replies can be matched back to the originating poll.

use crate::state::GatewayState;
use drbot_core::message::{Content, IncomingMessage, Message};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::Mutex;
use uuid::Uuid;

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(0)
}

const POLL_DEFAULT_DURATION_HOURS: u64 = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PollStatus {
    Pending,
    Resolved,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PollRecord {
    pub poll_id: String,
    pub run_id: String,
    pub channel_type: String,
    pub channel_id: String,
    pub question: String,
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_selections: Option<u64>,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub status: PollStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selections: Option<Vec<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_reply: Option<String>,
}

#[derive(Default)]
struct PollStoreState {
    by_id: HashMap<String, PollRecord>,
    // `channel_type:channel_id` -> poll_id
    pending_by_session: HashMap<String, String>,
    // runId/idempotencyKey -> poll_id (idempotency)
    by_run: HashMap<String, String>,
}

#[derive(Default)]
pub struct OpenclawPollStore {
    inner: Mutex<PollStoreState>,
}

impl OpenclawPollStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn cleanup_locked(st: &mut PollStoreState) {
        let now = now_ms();
        let mut expired_ids: Vec<String> = Vec::new();
        for (id, rec) in &st.by_id {
            if matches!(rec.status, PollStatus::Pending) && now > rec.expires_at_ms {
                expired_ids.push(id.clone());
            }
        }
        for id in expired_ids {
            if let Some(rec) = st.by_id.get_mut(&id) {
                rec.status = PollStatus::Expired;
                rec.resolved_at_ms = Some(now_ms());
            }
            st.pending_by_session.retain(|_, v| v != &id);
        }
    }

    pub async fn register_text_poll(
        &self,
        run_id: &str,
        channel_type: &str,
        channel_id: &str,
        question: &str,
        options: Vec<String>,
        max_selections: Option<u64>,
        duration_hours: Option<u64>,
    ) -> String {
        let run_id = run_id.trim();
        let channel_type = channel_type.trim();
        let channel_id = channel_id.trim();

        let mut st = self.inner.lock().await;
        Self::cleanup_locked(&mut st);

        if !run_id.is_empty() {
            if let Some(existing) = st.by_run.get(run_id).cloned() {
                return existing;
            }
        }

        let created_at_ms = now_ms();
        let duration_hours = duration_hours.unwrap_or(POLL_DEFAULT_DURATION_HOURS).max(1);
        let expires_at_ms = created_at_ms.saturating_add(duration_hours * 60 * 60 * 1000);

        let poll_id = Uuid::new_v4().to_string();
        let record = PollRecord {
            poll_id: poll_id.clone(),
            run_id: run_id.to_string(),
            channel_type: channel_type.to_string(),
            channel_id: channel_id.to_string(),
            question: question.trim().to_string(),
            options,
            max_selections,
            created_at_ms,
            expires_at_ms,
            status: PollStatus::Pending,
            resolved_at_ms: None,
            selections: None,
            raw_reply: None,
        };
        st.by_id.insert(poll_id.clone(), record);
        if !channel_type.is_empty() && !channel_id.is_empty() {
            let session_key = format!("{}:{}", channel_type, channel_id);
            st.pending_by_session.insert(session_key, poll_id.clone());
        }
        if !run_id.is_empty() {
            st.by_run.insert(run_id.to_string(), poll_id.clone());
        }
        poll_id
    }

    pub async fn maybe_resolve_from_incoming(
        &self,
        state: &GatewayState,
        incoming: &IncomingMessage,
    ) {
        let channel_type = incoming.channel_type.trim();
        let channel_id = incoming.channel_id.trim();
        if channel_type.is_empty() || channel_id.is_empty() {
            return;
        }
        let session_key = format!("{}:{}", channel_type, channel_id);

        let text = incoming_text(incoming).unwrap_or_default();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }

        let (poll_id, mut record) = {
            let mut st = self.inner.lock().await;
            Self::cleanup_locked(&mut st);
            let Some(poll_id) = st.pending_by_session.get(&session_key).cloned() else {
                return;
            };
            let Some(record) = st.by_id.get(&poll_id).cloned() else {
                st.pending_by_session.remove(&session_key);
                return;
            };
            if !matches!(record.status, PollStatus::Pending) {
                st.pending_by_session.remove(&session_key);
                return;
            }
            (poll_id, record)
        };

        let max = record.max_selections.unwrap_or(1).max(1);
        let selections = parse_selections(trimmed, record.options.len(), max);
        let Some(selections) = selections else {
            // Not a poll answer; ignore.
            return;
        };

        let resolved_at_ms = now_ms();
        record.status = PollStatus::Resolved;
        record.resolved_at_ms = Some(resolved_at_ms);
        record.selections = Some(selections.clone());
        record.raw_reply = Some(trimmed.to_string());

        {
            let mut st = self.inner.lock().await;
            st.by_id.insert(poll_id.clone(), record.clone());
            st.pending_by_session.remove(&session_key);
        }

        let chosen = selections
            .iter()
            .filter_map(|idx| {
                let i = (*idx as usize).saturating_sub(1);
                record
                    .options
                    .get(i)
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            })
            .collect::<Vec<_>>()
            .join(", ");
        let note = if chosen.is_empty() {
            format!("Poll resolved: {}", trimmed)
        } else {
            format!("Poll resolved: {}", chosen)
        };

        // Persist a small system note into the session transcript so the Control UI
        // can surface the outcome via sessions.preview.
        if let Some(store) = state.session_store() {
            let user_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"openclaw-operator");
            if let Ok(mut session) = store.get_or_create(user_id, channel_type, channel_id).await {
                session.add_message(Message::system(note.clone()));
                session.update_timestamp();
                let _ = store.update(&session).await;
            }
        }

        // Also enqueue an ephemeral system event for that session so the next agent
        // run can react without requiring transcripts.
        state.openclaw_enqueue_system_event(&session_key, &note, None).await;
    }
}

fn incoming_text(incoming: &IncomingMessage) -> Option<String> {
    let mut out = String::new();
    for c in &incoming.content {
        match c {
            Content::Text { text } => out.push_str(text),
            _ => {}
        }
    }
    if out.trim().is_empty() {
        None
    } else {
        Some(out)
    }
}

fn parse_selections(input: &str, options_len: usize, max: u64) -> Option<Vec<u64>> {
    if options_len == 0 {
        return None;
    }
    let mut nums: Vec<u64> = Vec::new();
    let mut cur = String::new();
    for ch in input.chars() {
        if ch.is_ascii_digit() {
            cur.push(ch);
            continue;
        }
        if !cur.is_empty() {
            if let Ok(v) = cur.parse::<u64>() {
                nums.push(v);
            }
            cur.clear();
        }
    }
    if !cur.is_empty() {
        if let Ok(v) = cur.parse::<u64>() {
            nums.push(v);
        }
    }

    nums.retain(|n| *n >= 1 && (*n as usize) <= options_len);
    nums.dedup();
    if nums.is_empty() {
        return None;
    }
    let max = max.max(1) as usize;
    nums.truncate(max);
    Some(nums)
}

pub async fn maybe_resolve_from_incoming(state: &GatewayState, incoming: &IncomingMessage) {
    state
        .openclaw_polls()
        .maybe_resolve_from_incoming(state, incoming)
        .await;
}
