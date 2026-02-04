//! OpenClaw inbound channel bridge (best-effort).
//!
//! OpenClaw's gateway runtime starts enabled messaging channels and routes
//! inbound messages into session transcripts. drbot's legacy gateway did not
//! manage channels; this bridge closes that parity gap for `/openclaw/ws`.

use crate::state::GatewayState;
use drbot_core::message::{IncomingMessage, Message, Role};
use drbot_core::Error;
use serde_json::json;
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;
use tracing::{info, warn};
use uuid::Uuid;

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(0)
}

fn incoming_to_session_message(incoming: &IncomingMessage) -> Message {
    let mut metadata = serde_json::Map::new();
    metadata.insert("channelType".to_string(), json!(incoming.channel_type));
    metadata.insert("channelId".to_string(), json!(incoming.channel_id));
    metadata.insert("senderId".to_string(), json!(incoming.sender.id));
    if let Some(name) = incoming.sender.name.as_deref() {
        metadata.insert("senderName".to_string(), json!(name));
    }
    if let Some(username) = incoming.sender.username.as_deref() {
        metadata.insert("senderUsername".to_string(), json!(username));
    }
    if let Some(reply_to) = incoming.reply_to.as_deref() {
        metadata.insert("replyTo".to_string(), json!(reply_to));
    }
    if let Some(raw) = incoming.raw.as_ref() {
        metadata.insert("raw".to_string(), raw.clone());
    }

    Message {
        id: incoming.id,
        role: Role::User,
        content: incoming.content.clone(),
        created_at: incoming.received_at,
        metadata,
    }
}

async fn persist_incoming(state: &GatewayState, incoming: &IncomingMessage) {
    let Some(store) = state.session_store() else {
        return;
    };

    // Stable operator user id (single-user gateway).
    let user_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"openclaw-operator");

    let channel_type = incoming.channel_type.trim();
    let channel_id = incoming.channel_id.trim();
    if channel_type.is_empty() || channel_id.is_empty() {
        return;
    }

    let mut session = match store
        .get_or_create(user_id, channel_type, channel_id)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, channel = %channel_type, channel_id = %channel_id, "OpenClaw inbound: failed to load/create session");
            return;
        }
    };

    session.add_message(incoming_to_session_message(incoming));
    session.update_timestamp();
    if let Err(e) = store.update(&session).await {
        warn!(error = %e, channel = %channel_type, channel_id = %channel_id, "OpenClaw inbound: failed to persist session");
    }
}

async fn handle_incoming(state: &GatewayState, incoming: IncomingMessage) {
    persist_incoming(state, &incoming).await;

    // Poll resolution hooks live in `openclaw_polls`; keep this call best-effort.
    crate::openclaw_polls::maybe_resolve_from_incoming(state, &incoming).await;
}

async fn run_channel_loop(state: GatewayState, channel_type: String) {
    let mut backoff_ms = 1_000u64;
    loop {
        // Avoid noisy retries when a channel is explicitly stopped (e.g. WhatsApp logout).
        if !state.channel_manager().is_running(&channel_type).await {
            tokio::time::sleep(Duration::from_millis(2_000)).await;
            backoff_ms = 1_000;
            continue;
        }

        let rx = state.channel_manager().connect_and_subscribe(&channel_type).await;
        let mut rx = match rx {
            Ok(r) => {
                backoff_ms = 1_000;
                r
            }
            Err(e) => {
                if matches!(&e, Error::Config(msg) if msg.contains("stopped")) {
                    tokio::time::sleep(Duration::from_millis(2_000)).await;
                    backoff_ms = 1_000;
                    continue;
                }
                warn!(error = %e, channel = %channel_type, "OpenClaw inbound: connect failed (will retry)");
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(60_000);
                continue;
            }
        };

        info!(channel = %channel_type, "OpenClaw inbound: listening");

        loop {
            match rx.recv().await {
                Ok(msg) => {
                    state.channel_manager().note_inbound(&channel_type).await;
                    handle_incoming(&state, msg).await;
                }
                Err(RecvError::Lagged(count)) => {
                    warn!(channel = %channel_type, dropped = count, "OpenClaw inbound: receiver lagged");
                }
                Err(RecvError::Closed) => {
                    warn!(channel = %channel_type, "OpenClaw inbound: channel closed (will reconnect)");
                    state
                        .channel_manager()
                        .note_disconnect(&channel_type, Some("receiver closed".to_string()))
                        .await;
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms * 2).min(60_000);
    }
}

pub async fn start_inbound_bridge(state: GatewayState) {
    // Only start listeners for channels that are both enabled and configured.
    let mut started_any = false;
    for channel in state.channel_manager().list_channel_types() {
        if !state.channel_manager().is_enabled(&channel) {
            continue;
        }
        if !state.channel_manager().is_configured(&channel) {
            continue;
        }
        if !state.openclaw_try_start_inbound_channel(&channel).await {
            continue;
        }
        started_any = true;
        let st = state.clone();
        let channel_type = channel.clone();
        tokio::spawn(async move {
            run_channel_loop(st, channel_type).await;
        });
    }

    if started_any {
        info!(ts = now_ms(), "OpenClaw inbound bridge started");
    }
}
