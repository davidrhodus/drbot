//! OpenClaw "heartbeat" runner (v3 compatibility).
//!
//! OpenClaw's gateway exposes:
//! - `set-heartbeats` to enable/disable heartbeat runs
//! - `last-heartbeat` to retrieve the last heartbeat event payload
//! - `heartbeat` events emitted when a heartbeat run completes
//!
//! OpenClaw heartbeats are *not* websocket keepalives. They are periodic (or
//! explicitly requested) "wake" runs that can process queued system events and
//! consult a workspace `HEARTBEAT.md` file. The model should reply with
//! `HEARTBEAT_OK` when nothing needs attention.

use crate::state::GatewayState;
use drbot_agents::{
    Agent as DrbotAgent, AgentConfig as DrbotAgentConfig, AgentMessage as DrbotAgentMessage,
    AgentRole as DrbotAgentRole, BuiltinTools,
};
use drbot_core::config::AutonomyMode;
use drbot_core::message::Message;
use drbot_core::message::Role;
use drbot_providers::{ChatOptions, Provider, StreamEvent as ProviderStreamEvent, Usage};
use futures::StreamExt;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::{Mutex, Notify};
use tracing::{debug, warn};
use uuid::Uuid;

pub const HEARTBEAT_TOKEN: &str = "HEARTBEAT_OK";

// Keep aligned with OpenClaw defaults (auto-reply/heartbeat.ts).
const DEFAULT_HEARTBEAT_EVERY_MS: u64 = 30 * 60 * 1000;
const DEFAULT_HEARTBEAT_ACK_MAX_CHARS: usize = 300;
const DEFAULT_HEARTBEAT_PROMPT: &str =
    "Read HEARTBEAT.md if it exists (workspace context). Follow it strictly. Do not infer or repeat old tasks from prior chats. If nothing needs attention, reply HEARTBEAT_OK.";

const DEFAULT_HEARTBEAT_FILENAME: &str = "HEARTBEAT.md";

const RETRY_REQUESTS_IN_FLIGHT_MS: u64 = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeartbeatRunResult {
    Ran { duration_ms: u64 },
    Skipped { reason: String },
    Failed { reason: String },
}

impl HeartbeatRunResult {
    fn is_requests_in_flight(&self) -> bool {
        matches!(
            self,
            HeartbeatRunResult::Skipped { reason } if reason == "requests-in-flight"
        )
    }

    fn is_disabled(&self) -> bool {
        matches!(self, HeartbeatRunResult::Skipped { reason } if reason == "disabled")
    }
}

#[derive(Debug, Default)]
struct HeartbeatState {
    next_due_ms: Option<u64>,
    retry_due_ms: Option<u64>,
    pending_reason: Option<String>,
}

#[derive(Debug)]
pub struct OpenclawHeartbeatService {
    key: PathBuf,
    interval_ms: u64,
    started: AtomicBool,
    notify: Notify,
    run_lock: Mutex<()>,
    state: Mutex<HeartbeatState>,
}

static OPENCLAW_HEARTBEAT_SERVICES: OnceLock<
    Mutex<HashMap<PathBuf, Arc<OpenclawHeartbeatService>>>,
> = OnceLock::new();

fn openclaw_heartbeat_services() -> &'static Mutex<HashMap<PathBuf, Arc<OpenclawHeartbeatService>>>
{
    OPENCLAW_HEARTBEAT_SERVICES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(0)
}

fn format_system_event_ts(ts_ms: u64) -> String {
    // OpenClaw formats these in a human-friendly timezone; for drbot interop a UTC
    // RFC3339 timestamp is sufficient (clients treat this as prompt context only).
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ts_ms as i64)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339()
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

fn resolve_state_key(state: &GatewayState) -> PathBuf {
    crate::openclaw_paths::resolve_openclaw_state_dir(state.config())
        .unwrap_or_else(|| PathBuf::from(""))
}

fn resolve_indicator_type(status: &str) -> Option<&'static str> {
    match status {
        "ok-empty" | "ok-token" => Some("ok"),
        "sent" => Some("alert"),
        "failed" => Some("error"),
        _ => None,
    }
}

fn is_heartbeat_content_effectively_empty(content: &str) -> bool {
    for raw in content.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Markdown headers: "# " / "## " / etc. (also allow lone "#")
        if trimmed.starts_with('#') {
            let hashes = trimmed.chars().take_while(|c| *c == '#').count();
            if hashes > 0 {
                let rest = trimmed[hashes..].trim_start();
                if rest.is_empty() || trimmed.chars().nth(hashes) == Some(' ') {
                    continue;
                }
            }
        }
        // Empty markdown list items: "- [ ]" / "* [x]" / "- " / "+ [ ]"
        if let Some(first) = trimmed.chars().next() {
            if first == '-' || first == '*' || first == '+' {
                let after = trimmed[1..].trim_start();
                if after.is_empty() {
                    continue;
                }
                if after.starts_with('[') {
                    let after_bracket = after.trim_start_matches('[');
                    let after_mark = after_bracket
                        .trim_start_matches(|c: char| c == ' ' || c == 'X' || c == 'x');
                    if after_mark.starts_with(']') {
                        let remaining = after_mark[1..].trim_start();
                        if remaining.is_empty() {
                            continue;
                        }
                    }
                }
            }
        }
        // Found actionable content.
        return false;
    }
    true
}

#[derive(Debug, Clone)]
struct StripResult {
    should_skip: bool,
    text: String,
}

fn strip_heartbeat_token(raw: &str, max_ack_chars: usize) -> StripResult {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return StripResult {
            should_skip: true,
            text: String::new(),
        };
    }

    // Lightweight markup normalization (OpenClaw parity): drop tags and wrapper chars.
    let strip_markup = |text: &str| -> String {
        let mut out = String::with_capacity(text.len());
        let mut in_tag = false;
        for ch in text.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ => {
                    if !in_tag {
                        out.push(ch);
                    } else {
                        out.push(' ');
                    }
                }
            }
        }
        let out = out.replace("&nbsp;", " ").replace("&NBSP;", " ");
        out.trim_matches(|c: char| c == '*' || c == '`' || c == '~' || c == '_')
            .to_string()
    };

    let normalized = strip_markup(trimmed);
    let has_token = trimmed.contains(HEARTBEAT_TOKEN) || normalized.contains(HEARTBEAT_TOKEN);
    if !has_token {
        return StripResult {
            should_skip: false,
            text: trimmed.to_string(),
        };
    }

    fn strip_token_at_edges(text: &str) -> (String, bool) {
        let mut cur = text.trim().to_string();
        let mut did_strip = false;
        let token = HEARTBEAT_TOKEN;
        if !cur.contains(token) {
            return (cur, false);
        }
        loop {
            let next = cur.trim();
            let mut changed = false;
            if next.starts_with(token) {
                let after = next[token.len()..].trim_start().to_string();
                cur = after;
                did_strip = true;
                changed = true;
            }
            let next = cur.trim();
            if next.ends_with(token) {
                let before = next[..next.len().saturating_sub(token.len())]
                    .trim_end()
                    .to_string();
                cur = before;
                did_strip = true;
                changed = true;
            }
            if !changed {
                break;
            }
        }
        let collapsed = cur.split_whitespace().collect::<Vec<_>>().join(" ");
        (collapsed.trim().to_string(), did_strip)
    }

    let (stripped_original, did_strip_original) = strip_token_at_edges(trimmed);
    let (stripped_normalized, did_strip_normalized) = strip_token_at_edges(&normalized);
    let picked = if did_strip_original && !stripped_original.is_empty() {
        (stripped_original, true)
    } else {
        (stripped_normalized, did_strip_normalized)
    };
    if !picked.1 {
        return StripResult {
            should_skip: false,
            text: trimmed.to_string(),
        };
    }
    if picked.0.is_empty() {
        return StripResult {
            should_skip: true,
            text: String::new(),
        };
    }
    if picked.0.len() <= max_ack_chars {
        return StripResult {
            should_skip: true,
            text: String::new(),
        };
    }
    StripResult {
        should_skip: false,
        text: picked.0,
    }
}

pub async fn heartbeat_service_for_state(state: &GatewayState) -> Arc<OpenclawHeartbeatService> {
    let key = resolve_state_key(state).join("openclaw-heartbeat");
    let mut services = openclaw_heartbeat_services().lock().await;
    if let Some(svc) = services.get(&key) {
        svc.start_background(state.clone());
        return svc.clone();
    }

    let interval_ms = std::env::var("DRBOT_OPENCLAW_HEARTBEAT_EVERY_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v >= 1_000)
        .unwrap_or(DEFAULT_HEARTBEAT_EVERY_MS);

    let svc = Arc::new(OpenclawHeartbeatService {
        key: key.clone(),
        interval_ms,
        started: AtomicBool::new(false),
        notify: Notify::new(),
        run_lock: Mutex::new(()),
        state: Mutex::new(HeartbeatState::default()),
    });
    svc.start_background(state.clone());
    services.insert(key, svc.clone());
    svc
}

pub async fn request_heartbeat_now(state: &GatewayState, reason: Option<String>) {
    let svc = heartbeat_service_for_state(state).await;
    svc.request_now(reason).await;
}

pub async fn run_heartbeat_once(
    state: &GatewayState,
    reason: Option<String>,
) -> HeartbeatRunResult {
    let svc = heartbeat_service_for_state(state).await;
    svc.run_once(state, reason).await
}

impl OpenclawHeartbeatService {
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

    pub async fn request_now(&self, reason: Option<String>) {
        let mut st = self.state.lock().await;
        if let Some(reason) = reason {
            let trimmed = reason.trim();
            if !trimmed.is_empty() {
                st.pending_reason = Some(trimmed.to_string());
            }
        } else if st.pending_reason.is_none() {
            st.pending_reason = Some("requested".to_string());
        }
        self.notify.notify_one();
    }

    pub async fn run_once(
        &self,
        state: &GatewayState,
        reason: Option<String>,
    ) -> HeartbeatRunResult {
        let _guard = self.run_lock.lock().await;
        let started_at = now_ms();

        let res = run_heartbeat_once_impl(state, reason.clone()).await;

        // Update scheduling state unless this was a disabled/no-op or a queue-busy skip.
        let ended_at = now_ms();
        let mut st = self.state.lock().await;
        if res.is_requests_in_flight() {
            // Keep pending reason and retry soon.
            if st.pending_reason.is_none() {
                st.pending_reason = reason.or_else(|| Some("retry".to_string()));
            }
            st.retry_due_ms = Some(ended_at.saturating_add(RETRY_REQUESTS_IN_FLIGHT_MS));
        } else if res.is_disabled() {
            // Don't advance the schedule when disabled.
        } else {
            st.retry_due_ms = None;
            st.next_due_ms = Some(ended_at.saturating_add(self.interval_ms));
            st.pending_reason = None;
        }

        // Ensure duration is present when we report "ran"/"failed"/some skips.
        match &res {
            HeartbeatRunResult::Ran { .. } => res,
            HeartbeatRunResult::Failed { .. } => res,
            HeartbeatRunResult::Skipped { .. } => {
                // For disabled/requests-in-flight we didn't emit events; leave as-is.
                // Other skips (e.g. empty-heartbeat-file) already emitted with duration.
                let _ = started_at;
                res
            }
        }
    }

    async fn run_loop(self: Arc<Self>, state: GatewayState) {
        loop {
            if !state.openclaw_heartbeats_enabled() {
                self.notify.notified().await;
                continue;
            }

            let delay_ms = {
                let mut st = self.state.lock().await;
                let now = now_ms();
                if st.next_due_ms.is_none() {
                    st.next_due_ms = Some(now.saturating_add(self.interval_ms));
                }
                let next_due = st
                    .next_due_ms
                    .unwrap_or(now.saturating_add(self.interval_ms));
                let retry_due = st.retry_due_ms;
                let next_wake = match retry_due {
                    Some(r) if r < next_due => r,
                    _ => next_due,
                };
                next_wake.saturating_sub(now)
            };

            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {
                    let now = now_ms();
                    let (reason, is_retry) = {
                        let mut st = self.state.lock().await;
                        let retry_due = st.retry_due_ms;
                        let is_retry = retry_due.is_some_and(|r| now >= r);
                        if is_retry {
                            st.retry_due_ms = None;
                            let r = st.pending_reason.take().unwrap_or_else(|| "retry".to_string());
                            (Some(r), true)
                        } else {
                            (Some("interval".to_string()), false)
                        }
                    };
                    let res = self.run_once(&state, reason).await;
                    if is_retry && res.is_requests_in_flight() {
                        warn!(key = %self.key.to_string_lossy(), "heartbeat: retry still blocked by requests-in-flight");
                    }
                }
                _ = self.notify.notified() => {
                    // Coalesce requests by draining pending_reason once per wake.
                    let reason = {
                        let mut st = self.state.lock().await;
                        st.pending_reason.take().or_else(|| Some("requested".to_string()))
                    };
                    let res = self.run_once(&state, reason).await;
                    if res.is_requests_in_flight() {
                        debug!(key = %self.key.to_string_lossy(), "heartbeat: skipped due to requests-in-flight (will retry)");
                    }
                }
            }
        }
    }
}

async fn run_heartbeat_once_impl(
    state: &GatewayState,
    reason: Option<String>,
) -> HeartbeatRunResult {
    if !state.openclaw_heartbeats_enabled() {
        return HeartbeatRunResult::Skipped {
            reason: "disabled".to_string(),
        };
    }
    if state.openclaw_main_inflight() > 0 {
        return HeartbeatRunResult::Skipped {
            reason: "requests-in-flight".to_string(),
        };
    }

    // Best-effort: keep remote skills' docs up to date (no-op unless configured).
    tokio::join!(
        crate::colosseum::sync_colosseum_docs_best_effort(state.config()),
        crate::moltbook::sync_moltbook_docs_best_effort(state.config()),
        crate::agentwallet::sync_agentwallet_docs_best_effort(state.config()),
        crate::openclaw_skills::sync_configured_remote_skills_best_effort(state.config()),
    );

    // Best-effort: if remote skills updated their requirements, refresh remote node bin probes.
    let st = state.clone();
    tokio::spawn(async move {
        crate::openclaw::refresh_remote_bins_for_connected_nodes_best_effort(st, false).await;
    });

    let provider: Arc<dyn Provider> = match state.provider() {
        Some(p) => p,
        None => {
            emit_heartbeat_event(
                state,
                HeartbeatEventBuild {
                    status: "failed",
                    reason: Some("provider not configured".to_string()),
                    preview: None,
                    duration_ms: Some(0),
                    silent: None,
                },
            )
            .await;
            return HeartbeatRunResult::Failed {
                reason: "provider not configured".to_string(),
            };
        }
    };

    let started_at = now_ms();
    let _lane = OpenclawMainLaneGuard::new(state.clone());

    let workspace_dir = crate::openclaw::resolve_agent_workspace_dir_for_state(&state, "default");
    let mut heartbeat_sections: Vec<(String, String)> = Vec::new();
    let remote = crate::openclaw::resolve_remote_skill_eligibility(state).await;

    let main_session_key = crate::openclaw::canonicalize_openclaw_session_key(
        crate::openclaw_paths::DEFAULT_AGENT_ID,
        "main",
    );
    let legacy_main_key = "main";

    // Workspace heartbeat file (optional).
    let heartbeat_path = workspace_dir.join(DEFAULT_HEARTBEAT_FILENAME);
    let heartbeat_file_content = std::fs::read_to_string(&heartbeat_path).ok();
    if let Some(content) = heartbeat_file_content.as_deref() {
        heartbeat_sections.push(("workspace HEARTBEAT.md".to_string(), content.to_string()));
    }

    // Skill-provided heartbeat files (optional).
    //
    // Many OpenClaw skills ship a HEARTBEAT.md alongside SKILL.md (e.g. Colosseum).
    for (skill_name, base_dir) in crate::openclaw_skills::list_eligible_skill_dirs_with_remote(
        &workspace_dir,
        state.config(),
        remote.as_ref(),
    ) {
        let path = base_dir.join(DEFAULT_HEARTBEAT_FILENAME);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let trimmed = content.trim();
        if trimmed.is_empty() {
            continue;
        }
        heartbeat_sections.push((
            format!("skill {} HEARTBEAT.md", skill_name),
            trimmed.to_string(),
        ));
    }

    // Pending system events (ephemeral queue). These should still be visible to
    // heartbeats even when transcripts are excluded.
    let mut queued_system_events = state.openclaw_peek_system_events(&main_session_key).await;
    if legacy_main_key != main_session_key {
        queued_system_events.extend(state.openclaw_peek_system_events(legacy_main_key).await);
    }
    let has_system_events = !queued_system_events.is_empty();

    // Skip heartbeat only if we have at least one heartbeat file and all of them
    // are effectively empty (OpenClaw uses this to avoid unnecessary API calls).
    if !has_system_events
        && !heartbeat_sections.is_empty()
        && heartbeat_sections
            .iter()
            .all(|(_, content)| is_heartbeat_content_effectively_empty(content))
    {
        let duration_ms = now_ms().saturating_sub(started_at);
        emit_heartbeat_event(
            state,
            HeartbeatEventBuild {
                status: "skipped",
                reason: Some("empty-heartbeat-file".to_string()),
                preview: None,
                duration_ms: Some(duration_ms),
                silent: None,
            },
        )
        .await;
        return HeartbeatRunResult::Skipped {
            reason: "empty-heartbeat-file".to_string(),
        };
    }

    let include_transcript = std::env::var("DRBOT_OPENCLAW_HEARTBEAT_INCLUDE_TRANSCRIPT")
        .ok()
        .as_deref()
        == Some("1");

    // Load main session (if configured). We keep this best-effort even when
    // we don't include the transcript in the heartbeat prompt, so output can
    // still be persisted for inspection via chat.history.
    let mut messages: Vec<Message> = Vec::new();
    let mut persisted_session = None;
    if let Some(store) = state.session_store() {
        let user_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"openclaw-operator");
        let mut session = store
            .get_by_channel("openclaw", &main_session_key)
            .await
            .ok()
            .flatten();
        if session.is_none() && legacy_main_key != main_session_key {
            session = store
                .get_by_channel("openclaw", legacy_main_key)
                .await
                .ok()
                .flatten();
            if let Some(mut s) = session.take() {
                if store
                    .get_by_channel("openclaw", &main_session_key)
                    .await
                    .ok()
                    .flatten()
                    .is_none()
                {
                    s.channel_id = main_session_key.clone();
                    let _ = store.update(&s).await;
                }
                session = Some(s);
            }
        }
        if session.is_none() {
            session = store
                .get_or_create(user_id, "openclaw", &main_session_key)
                .await
                .ok();
        }
        if let Some(s) = session {
            if include_transcript {
                messages.extend(s.messages.clone());
            }
            persisted_session = Some(s);
        }
    }

    let mut prompt = String::new();
    if let Some(reason) = reason.as_deref() {
        let trimmed = reason.trim();
        if !trimmed.is_empty() {
            prompt.push_str(&format!("Reason: {}\n\n", trimmed));
        }
    }
    if has_system_events {
        for evt in &queued_system_events {
            let ts = format_system_event_ts(evt.ts_ms);
            let text = evt.text.trim();
            if text.is_empty() {
                continue;
            }
            prompt.push_str(&format!("System: [{}] {}\n", ts, text));
        }
        prompt.push('\n');
    }
    prompt.push_str(DEFAULT_HEARTBEAT_PROMPT);

    // Colosseum-native prefetch: provide /agents/status etc as a pull signal.
    if let Some(ctx) = crate::colosseum::fetch_colosseum_heartbeat_context().await {
        if let Ok(pretty) = serde_json::to_string_pretty(&ctx) {
            prompt.push_str("\n\n---\nColosseum context (prefetched):\n");
            prompt.push_str(&pretty);
        }
    }

    // Moltbook-native prefetch: agent status, DMs, and feed preview.
    if let Some(ctx) = crate::moltbook::fetch_moltbook_heartbeat_context().await {
        if let Ok(pretty) = serde_json::to_string_pretty(&ctx) {
            prompt.push_str("\n\n---\nMoltbook context (prefetched):\n");
            prompt.push_str(&pretty);
        }
    }

    // AgentWallet-native prefetch: public network pulse.
    if let Some(ctx) = crate::agentwallet::fetch_agentwallet_heartbeat_context(state.config()).await
    {
        if let Ok(pretty) = serde_json::to_string_pretty(&ctx) {
            prompt.push_str("\n\n---\nAgentWallet context (prefetched):\n");
            prompt.push_str(&pretty);
        }
    }

    if heartbeat_sections.is_empty() {
        prompt.push_str("\n\n---\nHEARTBEAT.md: (missing)\n");
    } else {
        for (label, content) in &heartbeat_sections {
            prompt.push_str("\n\n---\n");
            prompt.push_str(label);
            prompt.push_str(":\n");
            prompt.push_str(content.trim());
        }
    }

    let skills_filter = crate::openclaw::resolve_openclaw_agent_skills_filter(
        state,
        crate::openclaw_paths::DEFAULT_AGENT_ID,
    );
    let skills_prompt = crate::openclaw_skills::build_workspace_skills_prompt_with_remote_filtered(
        &workspace_dir,
        state.config(),
        remote.as_ref(),
        skills_filter.as_deref(),
    );
    let system_prompt_text = skills_prompt.trim().to_string();
    let system_prompt_opt = if system_prompt_text.is_empty() {
        None
    } else {
        Some(system_prompt_text.clone())
    };

    let use_tools = std::env::var("DRBOT_OPENCLAW_HEARTBEAT_TOOLS")
        .ok()
        .as_deref()
        == Some("1");

    let full = if use_tools {
        let autonomy_mode = state.config().assistant.autonomy_mode;
        let readonly = matches!(autonomy_mode, AutonomyMode::ReadOnly);
        let agent_cfg = DrbotAgentConfig {
            max_iterations: std::env::var("DRBOT_OPENCLAW_HEARTBEAT_MAX_ITERATIONS")
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|v| *v >= 1)
                .unwrap_or(6),
            model: crate::openclaw::resolve_openclaw_session_model_override(
                state,
                &main_session_key,
            )
            .or_else(|| crate::openclaw::resolve_openclaw_agent_default_model(state, "default")),
            system_prompt: system_prompt_text,
            use_planning: false,
            iteration_timeout_secs: std::env::var(
                "DRBOT_OPENCLAW_HEARTBEAT_ITERATION_TIMEOUT_SECS",
            )
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|v| *v >= 1)
            .unwrap_or(60),
        };

        let provider = Arc::new(crate::openclaw_usage::UsageLoggingProvider::new(
            state.clone(),
            provider.clone(),
            Some(main_session_key.clone()),
            Some(format!("heartbeat-agent:{}", started_at)),
        )) as Arc<dyn Provider>;
        let mut agent = DrbotAgent::new(provider, agent_cfg);

        // Conservative baseline: do not expose generic HTTP, shell, or write tools in heartbeats.
        let builtin = match BuiltinTools::all(workspace_dir.clone()) {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, "heartbeat: failed to initialize builtin tools");
                Vec::new()
            }
        };
        for tool in builtin {
            if matches!(tool.name(), "http" | "bash" | "write_file" | "apply_patch") {
                continue;
            }
            if readonly && !crate::openclaw::is_autonomy_readonly_tool(tool.name()) {
                continue;
            }
            agent.register_tool(tool);
        }
        if !readonly {
            agent.register_tool(Arc::new(
                crate::openclaw_agent_tools::ColosseumRequestTool::new(state.clone()),
            ));
            agent.register_tool(Arc::new(
                crate::openclaw_agent_tools::MoltbookRequestTool::new(state.clone()),
            ));
            agent.register_tool(Arc::new(
                crate::openclaw_agent_tools::SendTool::new_with_context(
                    state.clone(),
                    "default",
                    &main_session_key,
                ),
            ));
            agent.register_tool(Arc::new(
                crate::openclaw_agent_tools::PollTool::new_with_context(
                    state.clone(),
                    "default",
                    &main_session_key,
                ),
            ));
        }

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

        match agent.run(prompt.as_str()).await {
            Ok(v) => v,
            Err(e) => {
                let duration_ms = now_ms().saturating_sub(started_at);
                let reason = e.to_string();
                emit_heartbeat_event(
                    state,
                    HeartbeatEventBuild {
                        status: "failed",
                        reason: Some(reason.clone()),
                        preview: None,
                        duration_ms: Some(duration_ms),
                        silent: None,
                    },
                )
                .await;
                return HeartbeatRunResult::Failed { reason };
            }
        }
    } else {
        // Ensure the model has a fresh "tick" prompt even if the session has no new user input.
        messages.push(Message::user(prompt));

        let model_override =
            crate::openclaw::resolve_openclaw_session_model_override(state, &main_session_key);
        let model_override = model_override
            .clone()
            .or_else(|| crate::openclaw::resolve_openclaw_agent_default_model(state, "default"));
        let model_override_for_record = model_override.clone();
        let options = ChatOptions {
            model: model_override,
            max_tokens: None,
            temperature: None,
            top_p: None,
            stop_sequences: None,
            system_prompt: system_prompt_opt,
            tools: None,
        };

        let mut full = String::new();
        let mut final_usage: Option<Usage> = None;
        let mut stream_model: Option<String> = None;
        let stream_res = provider.stream(&messages, options).await;
        let mut stream = match stream_res {
            Ok(s) => s,
            Err(e) => {
                let duration_ms = now_ms().saturating_sub(started_at);
                let reason = e.to_string();
                emit_heartbeat_event(
                    state,
                    HeartbeatEventBuild {
                        status: "failed",
                        reason: Some(reason.clone()),
                        preview: None,
                        duration_ms: Some(duration_ms),
                        silent: None,
                    },
                )
                .await;
                return HeartbeatRunResult::Failed { reason };
            }
        };

        while let Some(evt) = stream.next().await {
            match evt {
                ProviderStreamEvent::Start { model } => stream_model = Some(model),
                ProviderStreamEvent::Delta { content } => full.push_str(&content),
                ProviderStreamEvent::Stop { reason: _, usage } => {
                    final_usage = usage;
                }
                ProviderStreamEvent::Error { message } => {
                    let duration_ms = now_ms().saturating_sub(started_at);
                    emit_heartbeat_event(
                        state,
                        HeartbeatEventBuild {
                            status: "failed",
                            reason: Some(message.clone()),
                            preview: None,
                            duration_ms: Some(duration_ms),
                            silent: None,
                        },
                    )
                    .await;
                    return HeartbeatRunResult::Failed { reason: message };
                }
                _ => {}
            }
        }

        if let Some(usage) = final_usage.as_ref() {
            let model_for_record = stream_model.clone().or(model_override_for_record.clone());
            let record = crate::openclaw_usage::record_from_stream(
                state,
                provider.name(),
                model_for_record,
                Some(main_session_key.clone()),
                Some(format!("heartbeat-stream:{}", started_at)),
                usage,
            );
            crate::openclaw_usage::append_usage_record_best_effort(state, record).await;
        }

        full
    };

    let duration_ms = now_ms().saturating_sub(started_at);

    // We've run a heartbeat (success path); clear queued system events so they
    // don't leak into subsequent prompts.
    if has_system_events {
        let _ = state
            .openclaw_drain_system_event_entries(&main_session_key)
            .await;
        if legacy_main_key != main_session_key {
            let _ = state
                .openclaw_drain_system_event_entries(legacy_main_key)
                .await;
        }
    }

    let trimmed = full.trim();
    if trimmed.is_empty() {
        emit_heartbeat_event(
            state,
            HeartbeatEventBuild {
                status: "ok-empty",
                reason,
                preview: None,
                duration_ms: Some(duration_ms),
                silent: Some(true),
            },
        )
        .await;
        return HeartbeatRunResult::Ran { duration_ms };
    }

    let stripped = strip_heartbeat_token(trimmed, DEFAULT_HEARTBEAT_ACK_MAX_CHARS);
    if stripped.should_skip {
        emit_heartbeat_event(
            state,
            HeartbeatEventBuild {
                status: "ok-token",
                reason,
                preview: None,
                duration_ms: Some(duration_ms),
                silent: Some(true),
            },
        )
        .await;
        return HeartbeatRunResult::Ran { duration_ms };
    }

    let final_text = stripped.text.trim().to_string();
    if final_text.is_empty() {
        emit_heartbeat_event(
            state,
            HeartbeatEventBuild {
                status: "ok-token",
                reason,
                preview: None,
                duration_ms: Some(duration_ms),
                silent: Some(true),
            },
        )
        .await;
        return HeartbeatRunResult::Ran { duration_ms };
    }

    // Best-effort: persist the heartbeat output into the main session transcript so
    // the Control UI can inspect it via chat.history (drbot has no outbound channel delivery yet).
    if let (Some(store), Some(mut session)) = (state.session_store(), persisted_session) {
        session.add_message(Message::assistant(&final_text));
        session.update_timestamp();
        let _ = store.update(&session).await;
    }

    let preview = final_text
        .chars()
        .take(200)
        .collect::<String>()
        .trim()
        .to_string();
    emit_heartbeat_event(
        state,
        HeartbeatEventBuild {
            status: "sent",
            reason,
            preview: Some(preview),
            duration_ms: Some(duration_ms),
            silent: None,
        },
    )
    .await;
    HeartbeatRunResult::Ran { duration_ms }
}

struct HeartbeatEventBuild {
    status: &'static str,
    reason: Option<String>,
    preview: Option<String>,
    duration_ms: Option<u64>,
    silent: Option<bool>,
}

async fn emit_heartbeat_event(state: &GatewayState, build: HeartbeatEventBuild) {
    // Build payload in OpenClaw's HeartbeatEventPayload shape.
    let mut obj = serde_json::Map::new();
    obj.insert("ts".to_string(), json!(now_ms()));
    obj.insert("status".to_string(), json!(build.status));
    if let Some(reason) = build.reason.as_deref() {
        let trimmed = reason.trim();
        if !trimmed.is_empty() {
            obj.insert("reason".to_string(), json!(trimmed));
        }
    }
    if let Some(preview) = build.preview.as_deref() {
        let trimmed = preview.trim();
        if !trimmed.is_empty() {
            obj.insert("preview".to_string(), json!(trimmed));
        }
    }
    if let Some(duration_ms) = build.duration_ms {
        obj.insert("durationMs".to_string(), json!(duration_ms));
    }
    if let Some(silent) = build.silent {
        obj.insert("silent".to_string(), json!(silent));
    }
    if let Some(indicator) = resolve_indicator_type(build.status) {
        obj.insert("indicatorType".to_string(), json!(indicator));
    }

    let payload = serde_json::Value::Object(obj);
    let _ = state.openclaw_set_last_heartbeat(payload.clone()).await;
    crate::openclaw::broadcast_openclaw_event_opts(state, "heartbeat", payload, None, true).await;
}
