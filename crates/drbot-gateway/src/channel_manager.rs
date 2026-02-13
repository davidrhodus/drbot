//! Runtime channel manager for outbound sends.
//!
//! drbot's core config can enable multiple messaging channels (Telegram, Slack,
//! etc). The legacy gateway currently doesn't manage channels, but OpenClaw's
//! protocol expects `send` / `poll` to work when channels are configured.

use crate::openclaw_web_login::OpenclawWebLoginStore;
use drbot_channels::Channel;
use drbot_core::message::IncomingMessage;
use drbot_core::message::OutgoingMessage;
use drbot_core::{Error, Result};
use ring::digest;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tracing::{info, warn};

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(0)
}

pub struct ChannelManager {
    slots: HashMap<String, Arc<ChannelSlot>>,
    openclaw_web_login: Arc<OpenclawWebLoginStore>,
}

#[derive(Debug, Clone)]
pub struct ChannelRuntimeSnapshot {
    pub channel_type: String,
    pub enabled: bool,
    pub configured: bool,
    pub running: bool,
    pub connected: bool,
    pub reconnect_attempts: u64,
    pub last_connected_at_ms: Option<u64>,
    pub last_error: Option<String>,
    pub last_start_at_ms: Option<u64>,
    pub last_stop_at_ms: Option<u64>,
    pub last_inbound_at_ms: Option<u64>,
    pub last_outbound_at_ms: Option<u64>,
}

struct ChannelSlot {
    channel_type: String,
    enabled: AtomicBool,
    configured: AtomicBool,
    state: Mutex<ChannelSlotState>,
}

struct ChannelSlotState {
    channel: Option<Box<dyn Channel>>,
    config_hash: Option<String>,
    running: bool,
    connected: bool,
    reconnect_attempts: u64,
    last_connected_at_ms: Option<u64>,
    last_error: Option<String>,
    last_start_at_ms: Option<u64>,
    last_stop_at_ms: Option<u64>,
    last_inbound_at_ms: Option<u64>,
    last_outbound_at_ms: Option<u64>,
}

impl ChannelManager {
    pub fn new(
        config: &drbot_core::Config,
        openclaw_web_login: Arc<OpenclawWebLoginStore>,
    ) -> Self {
        let mut slots: HashMap<String, Arc<ChannelSlot>> = HashMap::new();

        let is_enabled = |name: &str| config.channels.enabled.iter().any(|c| c == name);
        let is_missing_secret = |value: &str| {
            let trimmed = value.trim();
            trimmed.is_empty() || trimmed == "<redacted>"
        };

        // Helper to insert a slot even if construction fails; this keeps status/introspection sane.
        let mut insert_slot = |name: &str,
                               enabled: bool,
                               configured: bool,
                               config_hash: Option<String>,
                               channel: Option<Box<dyn Channel>>,
                               err: Option<String>| {
            slots.insert(
                name.to_string(),
                Arc::new(ChannelSlot {
                    channel_type: name.to_string(),
                    enabled: AtomicBool::new(enabled),
                    configured: AtomicBool::new(configured),
                    state: Mutex::new(ChannelSlotState {
                        channel,
                        config_hash,
                        running: enabled && configured,
                        connected: false,
                        reconnect_attempts: 0,
                        last_connected_at_ms: None,
                        last_error: err,
                        last_start_at_ms: None,
                        last_stop_at_ms: None,
                        last_inbound_at_ms: None,
                        last_outbound_at_ms: None,
                    }),
                }),
            );
        };

        // webchat
        {
            let configured = config.channels.webchat.is_some();
            let enabled = is_enabled("webchat");
            if configured && enabled {
                let channel = drbot_webchat::WebChatChannel::from_core_config(config)
                    .map(|c| Box::new(c) as Box<dyn Channel>);
                insert_slot(
                    "webchat",
                    enabled,
                    configured,
                    Some(hash_parts(&["webchat"])),
                    channel,
                    None,
                );
            } else {
                insert_slot("webchat", enabled, configured, None, None, None);
            }
        }

        // telegram
        {
            let configured = config
                .channels
                .telegram
                .as_ref()
                .is_some_and(|c| !is_missing_secret(&c.bot_token));
            let enabled = is_enabled("telegram");
            if let (true, true) = (configured, enabled) {
                let token = config
                    .channels
                    .telegram
                    .as_ref()
                    .map(|c| c.bot_token.clone())
                    .unwrap_or_default();
                let config_hash = Some(hash_parts(&["telegram", token.trim()]));
                let channel = if token.trim().is_empty() {
                    None
                } else {
                    Some(Box::new(drbot_telegram::TelegramChannel::from_token(token))
                        as Box<dyn Channel>)
                };
                insert_slot("telegram", enabled, configured, config_hash, channel, None);
            } else {
                insert_slot("telegram", enabled, configured, None, None, None);
            }
        }

        // discord
        {
            let configured = config
                .channels
                .discord
                .as_ref()
                .is_some_and(|c| !is_missing_secret(&c.bot_token));
            let enabled = is_enabled("discord");
            if let (true, true) = (configured, enabled) {
                let token = config
                    .channels
                    .discord
                    .as_ref()
                    .map(|c| c.bot_token.clone())
                    .unwrap_or_default();
                let config_hash = Some(hash_parts(&["discord", token.trim()]));
                let channel = if token.trim().is_empty() {
                    None
                } else {
                    Some(Box::new(drbot_discord::DiscordChannel::from_token(token))
                        as Box<dyn Channel>)
                };
                insert_slot("discord", enabled, configured, config_hash, channel, None);
            } else {
                insert_slot("discord", enabled, configured, None, None, None);
            }
        }

        // slack
        {
            let configured = config.channels.slack.as_ref().is_some_and(|c| {
                !is_missing_secret(&c.bot_token) && !is_missing_secret(&c.app_token)
            });
            let enabled = is_enabled("slack");
            if let (true, true) = (configured, enabled) {
                let bot_token = config
                    .channels
                    .slack
                    .as_ref()
                    .map(|c| c.bot_token.clone())
                    .unwrap_or_default();
                let app_token = config
                    .channels
                    .slack
                    .as_ref()
                    .map(|c| c.app_token.clone())
                    .unwrap_or_default();
                let config_hash = Some(hash_parts(&["slack", bot_token.trim(), app_token.trim()]));
                let channel = if bot_token.trim().is_empty() || app_token.trim().is_empty() {
                    None
                } else {
                    let cfg = drbot_slack::SlackConfig::new(bot_token, app_token);
                    Some(Box::new(drbot_slack::SlackChannel::new(cfg)) as Box<dyn Channel>)
                };
                insert_slot("slack", enabled, configured, config_hash, channel, None);
            } else {
                insert_slot("slack", enabled, configured, None, None, None);
            }
        }

        // whatsapp
        {
            let configured = config
                .channels
                .whatsapp
                .as_ref()
                .is_some_and(|c| !c.bridge_url.trim().is_empty());
            let enabled = is_enabled("whatsapp");
            if let (true, true) = (configured, enabled) {
                let cfg = config.channels.whatsapp.as_ref().unwrap();
                let bridge = cfg.bridge_url.clone();
                let session_dir = cfg.session_path.to_string_lossy().to_string();
                let config_hash =
                    Some(hash_parts(&["whatsapp", bridge.trim(), session_dir.trim()]));
                let login_qr = openclaw_web_login.clone();
                let login_status = openclaw_web_login.clone();
                let channel = Box::new(
                    drbot_whatsapp::WhatsAppChannel::new(bridge)
                        .with_session_dir(session_dir)
                        .with_qr_callback(move |qr| login_qr.note_whatsapp_qr(qr))
                        .with_status_callback(move |status| {
                            login_status.note_whatsapp_status(status)
                        }),
                ) as Box<dyn Channel>;
                insert_slot(
                    "whatsapp",
                    enabled,
                    configured,
                    config_hash,
                    Some(channel),
                    None,
                );
            } else {
                insert_slot("whatsapp", enabled, configured, None, None, None);
            }
        }

        // signal
        {
            let configured = config.channels.signal.as_ref().is_some_and(|c| {
                !c.socket_path.trim().is_empty() && !c.phone_number.trim().is_empty()
            });
            let enabled = is_enabled("signal");
            if let (true, true) = (configured, enabled) {
                let cfg = config.channels.signal.as_ref().unwrap();
                let scfg = drbot_signal::SignalConfig::new(cfg.phone_number.clone())
                    .with_socket_path(cfg.socket_path.clone());
                let config_hash = Some(hash_parts(&[
                    "signal",
                    cfg.phone_number.trim(),
                    cfg.socket_path.trim(),
                ]));
                let channel = Box::new(drbot_signal::SignalChannel::new(scfg)) as Box<dyn Channel>;
                insert_slot(
                    "signal",
                    enabled,
                    configured,
                    config_hash,
                    Some(channel),
                    None,
                );
            } else {
                insert_slot("signal", enabled, configured, None, None, None);
            }
        }

        // matrix
        {
            let configured = config.channels.matrix.as_ref().is_some_and(|c| {
                !c.homeserver_url.trim().is_empty()
                    && !c.user_id.trim().is_empty()
                    && !is_missing_secret(&c.access_token)
            });
            let enabled = is_enabled("matrix");
            if let (true, true) = (configured, enabled) {
                let cfg = config.channels.matrix.as_ref().unwrap();
                let mut mcfg = drbot_matrix::MatrixConfig::new(
                    cfg.homeserver_url.clone(),
                    cfg.user_id.clone(),
                    cfg.access_token.clone(),
                );
                mcfg.allowed_rooms = cfg.allowed_rooms.clone();
                let allowed = cfg.allowed_rooms.join(",");
                let config_hash = Some(hash_parts(&[
                    "matrix",
                    cfg.homeserver_url.trim(),
                    cfg.user_id.trim(),
                    cfg.access_token.trim(),
                    allowed.trim(),
                ]));
                let channel = Box::new(drbot_matrix::MatrixChannel::new(mcfg)) as Box<dyn Channel>;
                insert_slot(
                    "matrix",
                    enabled,
                    configured,
                    config_hash,
                    Some(channel),
                    None,
                );
            } else {
                insert_slot("matrix", enabled, configured, None, None, None);
            }
        }

        // imessage (best-effort; may be unavailable on non-macOS)
        {
            let configured = config.channels.imessage.is_some();
            let enabled = is_enabled("imessage");
            if let (true, true) = (configured, enabled) {
                match drbot_imessage::IMessageChannel::new() {
                    Ok(channel) => insert_slot(
                        "imessage",
                        enabled,
                        configured,
                        Some(hash_parts(&["imessage"])),
                        Some(Box::new(channel) as Box<dyn Channel>),
                        None,
                    ),
                    Err(e) => {
                        warn!(error = %e, "Failed to construct iMessage channel");
                        insert_slot(
                            "imessage",
                            enabled,
                            configured,
                            Some(hash_parts(&["imessage"])),
                            None,
                            Some(e.to_string()),
                        );
                    }
                }
            } else {
                insert_slot("imessage", enabled, configured, None, None, None);
            }
        }

        if let Some(name) = pick_default_channel(&slots) {
            info!(channel = %name, "OpenClaw: default outbound channel selected");
        }

        Self {
            slots,
            openclaw_web_login,
        }
    }

    pub fn default_channel(&self) -> Option<&str> {
        pick_default_channel(&self.slots)
    }

    pub fn has_channel(&self, channel: &str) -> bool {
        self.slots.contains_key(channel)
    }

    pub fn is_enabled(&self, channel: &str) -> bool {
        self.slots
            .get(channel)
            .is_some_and(|s| s.enabled.load(Ordering::Relaxed))
    }

    pub fn is_configured(&self, channel: &str) -> bool {
        self.slots
            .get(channel)
            .is_some_and(|s| s.configured.load(Ordering::Relaxed))
    }

    pub async fn is_running(&self, channel: &str) -> bool {
        let channel = channel.trim();
        let Some(slot) = self.slots.get(channel) else {
            return false;
        };
        let st = slot.state.lock().await;
        st.running
    }

    pub fn list_channel_types(&self) -> Vec<String> {
        let mut keys = self.slots.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        keys
    }

    pub async fn runtime_snapshot(&self) -> HashMap<String, ChannelRuntimeSnapshot> {
        let mut out = HashMap::new();
        for (name, slot) in &self.slots {
            let st = slot.state.lock().await;
            out.insert(
                name.clone(),
                ChannelRuntimeSnapshot {
                    channel_type: slot.channel_type.clone(),
                    enabled: slot.enabled.load(Ordering::Relaxed),
                    configured: slot.configured.load(Ordering::Relaxed),
                    running: st.running,
                    connected: st.connected,
                    reconnect_attempts: st.reconnect_attempts,
                    last_connected_at_ms: st.last_connected_at_ms,
                    last_error: st.last_error.clone(),
                    last_start_at_ms: st.last_start_at_ms,
                    last_stop_at_ms: st.last_stop_at_ms,
                    last_inbound_at_ms: st.last_inbound_at_ms,
                    last_outbound_at_ms: st.last_outbound_at_ms,
                },
            );
        }
        out
    }

    pub async fn apply_core_config(&self, config: &drbot_core::Config) {
        let is_enabled = |name: &str| config.channels.enabled.iter().any(|c| c == name);
        let is_missing_secret = |value: &str| {
            let trimmed = value.trim();
            trimmed.is_empty() || trimmed == "<redacted>"
        };

        for slot in self.slots.values() {
            let channel_type = slot.channel_type.as_str();
            let enabled = is_enabled(channel_type);
            let (configured, config_hash) = match channel_type {
                "webchat" => {
                    let configured = config.channels.webchat.is_some();
                    (configured, configured.then(|| hash_parts(&["webchat"])))
                }
                "telegram" => {
                    let token = config
                        .channels
                        .telegram
                        .as_ref()
                        .map(|c| c.bot_token.trim())
                        .unwrap_or("");
                    let configured = !is_missing_secret(token);
                    (
                        configured,
                        configured.then(|| hash_parts(&["telegram", token])),
                    )
                }
                "discord" => {
                    let token = config
                        .channels
                        .discord
                        .as_ref()
                        .map(|c| c.bot_token.trim())
                        .unwrap_or("");
                    let configured = !is_missing_secret(token);
                    (
                        configured,
                        configured.then(|| hash_parts(&["discord", token])),
                    )
                }
                "slack" => {
                    let (bot_token, app_token) = config
                        .channels
                        .slack
                        .as_ref()
                        .map(|c| (c.bot_token.trim(), c.app_token.trim()))
                        .unwrap_or(("", ""));
                    let configured = !is_missing_secret(bot_token) && !is_missing_secret(app_token);
                    (
                        configured,
                        configured.then(|| hash_parts(&["slack", bot_token, app_token])),
                    )
                }
                "whatsapp" => {
                    let (bridge_url, session_path) = config
                        .channels
                        .whatsapp
                        .as_ref()
                        .map(|c| (c.bridge_url.trim(), c.session_path.to_string_lossy()))
                        .unwrap_or_default();
                    let configured = !bridge_url.trim().is_empty();
                    (
                        configured,
                        configured.then(|| {
                            hash_parts(&["whatsapp", bridge_url.trim(), session_path.trim()])
                        }),
                    )
                }
                "signal" => {
                    let (phone_number, socket_path) = config
                        .channels
                        .signal
                        .as_ref()
                        .map(|c| (c.phone_number.trim(), c.socket_path.trim()))
                        .unwrap_or(("", ""));
                    let configured = !phone_number.is_empty() && !socket_path.is_empty();
                    (
                        configured,
                        configured.then(|| hash_parts(&["signal", phone_number, socket_path])),
                    )
                }
                "matrix" => {
                    let cfg = config.channels.matrix.as_ref();
                    let (homeserver_url, user_id, access_token, allowed_rooms) = cfg
                        .map(|c| {
                            (
                                c.homeserver_url.trim(),
                                c.user_id.trim(),
                                c.access_token.trim(),
                                c.allowed_rooms.join(","),
                            )
                        })
                        .unwrap_or_else(|| ("", "", "", String::new()));
                    let configured = !homeserver_url.is_empty()
                        && !user_id.is_empty()
                        && !is_missing_secret(access_token);
                    (
                        configured,
                        configured.then(|| {
                            hash_parts(&[
                                "matrix",
                                homeserver_url,
                                user_id,
                                access_token,
                                allowed_rooms.trim(),
                            ])
                        }),
                    )
                }
                "imessage" => {
                    let configured = config.channels.imessage.is_some();
                    (configured, configured.then(|| hash_parts(&["imessage"])))
                }
                _ => continue,
            };

            let prev_enabled = slot.enabled.load(Ordering::Relaxed);
            let prev_configured = slot.configured.load(Ordering::Relaxed);
            let prev_eligible = prev_enabled && prev_configured;

            slot.enabled.store(enabled, Ordering::Relaxed);
            slot.configured.store(configured, Ordering::Relaxed);
            let eligible = enabled && configured;

            let mut st = slot.state.lock().await;

            if !eligible {
                if st.connected {
                    if let Some(ch) = st.channel.as_mut() {
                        if let Err(e) = ch.disconnect().await {
                            st.last_error = Some(e.to_string());
                        }
                    }
                    st.connected = false;
                }

                if st.running {
                    st.last_stop_at_ms = Some(now_ms());
                }
                st.running = false;

                if channel_type == "whatsapp" {
                    self.openclaw_web_login.reset_whatsapp();
                }

                if !configured {
                    st.channel = None;
                    st.config_hash = None;
                    st.last_error = None;
                    st.reconnect_attempts = 0;
                }

                continue;
            }

            if !prev_eligible {
                st.running = true;
            }

            let needs_rebuild =
                st.channel.is_none() || st.config_hash.as_deref() != config_hash.as_deref();

            if !needs_rebuild {
                continue;
            }

            if st.connected {
                if let Some(ch) = st.channel.as_mut() {
                    if let Err(e) = ch.disconnect().await {
                        st.last_error = Some(e.to_string());
                    }
                }
                st.connected = false;
                st.last_stop_at_ms = Some(now_ms());
            }

            let (next_channel, next_err) = match channel_type {
                "webchat" => (
                    drbot_webchat::WebChatChannel::from_core_config(config)
                        .map(|c| Box::new(c) as Box<dyn Channel>),
                    None,
                ),
                "telegram" => {
                    let token = config
                        .channels
                        .telegram
                        .as_ref()
                        .map(|c| c.bot_token.clone())
                        .unwrap_or_default();
                    let channel = if token.trim().is_empty() {
                        None
                    } else {
                        Some(Box::new(drbot_telegram::TelegramChannel::from_token(token))
                            as Box<dyn Channel>)
                    };
                    (channel, None)
                }
                "discord" => {
                    let token = config
                        .channels
                        .discord
                        .as_ref()
                        .map(|c| c.bot_token.clone())
                        .unwrap_or_default();
                    let channel = if token.trim().is_empty() {
                        None
                    } else {
                        Some(Box::new(drbot_discord::DiscordChannel::from_token(token))
                            as Box<dyn Channel>)
                    };
                    (channel, None)
                }
                "slack" => {
                    let bot_token = config
                        .channels
                        .slack
                        .as_ref()
                        .map(|c| c.bot_token.clone())
                        .unwrap_or_default();
                    let app_token = config
                        .channels
                        .slack
                        .as_ref()
                        .map(|c| c.app_token.clone())
                        .unwrap_or_default();
                    let channel = if bot_token.trim().is_empty() || app_token.trim().is_empty() {
                        None
                    } else {
                        let cfg = drbot_slack::SlackConfig::new(bot_token, app_token);
                        Some(Box::new(drbot_slack::SlackChannel::new(cfg)) as Box<dyn Channel>)
                    };
                    (channel, None)
                }
                "whatsapp" => {
                    if let Some(cfg) = config.channels.whatsapp.as_ref() {
                        let bridge = cfg.bridge_url.clone();
                        let session_dir = cfg.session_path.to_string_lossy().to_string();
                        let login_qr = self.openclaw_web_login.clone();
                        let login_status = self.openclaw_web_login.clone();
                        let channel = Box::new(
                            drbot_whatsapp::WhatsAppChannel::new(bridge)
                                .with_session_dir(session_dir)
                                .with_qr_callback(move |qr| login_qr.note_whatsapp_qr(qr))
                                .with_status_callback(move |status| {
                                    login_status.note_whatsapp_status(status)
                                }),
                        ) as Box<dyn Channel>;
                        (Some(channel), None)
                    } else {
                        (None, None)
                    }
                }
                "signal" => {
                    if let Some(cfg) = config.channels.signal.as_ref() {
                        let scfg = drbot_signal::SignalConfig::new(cfg.phone_number.clone())
                            .with_socket_path(cfg.socket_path.clone());
                        let channel =
                            Box::new(drbot_signal::SignalChannel::new(scfg)) as Box<dyn Channel>;
                        (Some(channel), None)
                    } else {
                        (None, None)
                    }
                }
                "matrix" => {
                    if let Some(cfg) = config.channels.matrix.as_ref() {
                        let mut mcfg = drbot_matrix::MatrixConfig::new(
                            cfg.homeserver_url.clone(),
                            cfg.user_id.clone(),
                            cfg.access_token.clone(),
                        );
                        mcfg.allowed_rooms = cfg.allowed_rooms.clone();
                        let channel =
                            Box::new(drbot_matrix::MatrixChannel::new(mcfg)) as Box<dyn Channel>;
                        (Some(channel), None)
                    } else {
                        (None, None)
                    }
                }
                "imessage" => match drbot_imessage::IMessageChannel::new() {
                    Ok(channel) => (Some(Box::new(channel) as Box<dyn Channel>), None),
                    Err(e) => (None, Some(e.to_string())),
                },
                _ => (None, None),
            };

            st.channel = next_channel;
            st.config_hash = config_hash;
            st.connected = false;
            st.reconnect_attempts = 0;
            st.last_error = next_err;
        }
    }

    pub async fn start_channel(&self, channel_type: &str) -> Result<()> {
        let channel_type = channel_type.trim();
        if channel_type.is_empty() {
            return Err(Error::InvalidInput("channel required".to_string()));
        }
        let Some(slot) = self.slots.get(channel_type) else {
            return Err(Error::NotFound(format!(
                "Unknown channel: {}",
                channel_type
            )));
        };
        if !slot.enabled.load(Ordering::Relaxed) {
            return Err(Error::Config(format!(
                "Channel '{}' is disabled",
                channel_type
            )));
        }
        if !slot.configured.load(Ordering::Relaxed) {
            return Err(Error::Config(format!(
                "Channel '{}' is not configured",
                channel_type
            )));
        }

        let mut st = slot.state.lock().await;
        st.running = true;
        st.last_start_at_ms = Some(now_ms());

        if st.channel.is_none() {
            if let Some(err) = st.last_error.as_deref() {
                return Err(Error::Channel(err.to_string()));
            }
            return Err(Error::Channel(format!(
                "Channel '{}' is unavailable",
                channel_type
            )));
        }
        if st.connected {
            return Ok(());
        }

        st.reconnect_attempts = st.reconnect_attempts.saturating_add(1);
        let connect_res = {
            let ch = st.channel.as_mut().expect("checked above");
            ch.connect().await
        };
        if let Err(e) = connect_res {
            st.last_error = Some(e.to_string());
            st.connected = false;
            st.last_stop_at_ms = Some(now_ms());
            return Err(e);
        }
        st.connected = true;
        st.last_connected_at_ms = Some(now_ms());
        st.last_error = None;
        Ok(())
    }

    pub async fn stop_channel(&self, channel_type: &str) -> Result<()> {
        let channel_type = channel_type.trim();
        if channel_type.is_empty() {
            return Err(Error::InvalidInput("channel required".to_string()));
        }
        let Some(slot) = self.slots.get(channel_type) else {
            return Err(Error::NotFound(format!(
                "Unknown channel: {}",
                channel_type
            )));
        };

        let mut st = slot.state.lock().await;
        st.running = false;
        st.last_stop_at_ms = Some(now_ms());

        if channel_type == "whatsapp" {
            self.openclaw_web_login.reset_whatsapp();
        }

        if st.channel.is_none() {
            st.connected = false;
            return Ok(());
        }
        if st.connected {
            let disconnect_res = {
                let ch = st.channel.as_mut().expect("checked above");
                ch.disconnect().await
            };
            if let Err(e) = disconnect_res {
                st.last_error = Some(e.to_string());
                st.connected = false;
                return Err(e);
            }
        }
        st.connected = false;
        Ok(())
    }

    pub async fn connect_and_subscribe(
        &self,
        channel_type: &str,
    ) -> Result<broadcast::Receiver<IncomingMessage>> {
        let channel_type = channel_type.trim();
        if channel_type.is_empty() {
            return Err(Error::InvalidInput("channel required".to_string()));
        }
        let Some(slot) = self.slots.get(channel_type) else {
            return Err(Error::NotFound(format!(
                "Unknown channel: {}",
                channel_type
            )));
        };
        if !slot.enabled.load(Ordering::Relaxed) {
            return Err(Error::Config(format!(
                "Channel '{}' is disabled",
                channel_type
            )));
        }
        if !slot.configured.load(Ordering::Relaxed) {
            return Err(Error::Config(format!(
                "Channel '{}' is not configured",
                channel_type
            )));
        }

        let mut st = slot.state.lock().await;
        if st.channel.is_none() {
            if let Some(err) = st.last_error.as_deref() {
                return Err(Error::Channel(err.to_string()));
            }
            return Err(Error::Channel(format!(
                "Channel '{}' is unavailable",
                channel_type
            )));
        }
        if !st.running {
            return Err(Error::Config(format!(
                "Channel '{}' is stopped",
                channel_type
            )));
        }

        if !st.connected {
            st.reconnect_attempts = st.reconnect_attempts.saturating_add(1);
            st.last_start_at_ms = Some(now_ms());
            let connect_res = {
                let ch = st.channel.as_mut().expect("checked above");
                ch.connect().await
            };
            if let Err(e) = connect_res {
                st.last_error = Some(e.to_string());
                st.connected = false;
                st.last_stop_at_ms = Some(now_ms());
                return Err(e);
            }
            st.connected = true;
            st.last_connected_at_ms = Some(now_ms());
            st.last_error = None;
        }

        let rx = st.channel.as_ref().expect("checked above").subscribe();
        Ok(rx)
    }

    pub async fn note_inbound(&self, channel_type: &str) {
        let channel_type = channel_type.trim();
        let Some(slot) = self.slots.get(channel_type) else {
            return;
        };
        let mut st = slot.state.lock().await;
        st.last_inbound_at_ms = Some(now_ms());
    }

    pub async fn note_outbound(&self, channel_type: &str) {
        let channel_type = channel_type.trim();
        let Some(slot) = self.slots.get(channel_type) else {
            return;
        };
        let mut st = slot.state.lock().await;
        st.last_outbound_at_ms = Some(now_ms());
    }

    pub async fn note_disconnect(&self, channel_type: &str, error: Option<String>) {
        let channel_type = channel_type.trim();
        let Some(slot) = self.slots.get(channel_type) else {
            return;
        };
        let mut st = slot.state.lock().await;
        st.connected = false;
        st.last_stop_at_ms = Some(now_ms());
        if let Some(err) = error {
            st.last_error = Some(err);
        }
    }

    pub async fn send(&self, channel_type: &str, to: &str, message: OutgoingMessage) -> Result<()> {
        let channel_type = channel_type.trim();
        if channel_type.is_empty() {
            return Err(Error::InvalidInput("channel required".to_string()));
        }
        let Some(slot) = self.slots.get(channel_type) else {
            return Err(Error::NotFound(format!(
                "Unknown channel: {}",
                channel_type
            )));
        };
        if !slot.enabled.load(Ordering::Relaxed) {
            return Err(Error::Config(format!(
                "Channel '{}' is disabled",
                channel_type
            )));
        }
        if !slot.configured.load(Ordering::Relaxed) {
            return Err(Error::Config(format!(
                "Channel '{}' is not configured",
                channel_type
            )));
        }

        let mut st = slot.state.lock().await;
        if st.channel.is_none() {
            if let Some(err) = st.last_error.as_deref() {
                return Err(Error::Channel(err.to_string()));
            }
            return Err(Error::Channel(format!(
                "Channel '{}' is unavailable",
                channel_type
            )));
        }
        if !st.running {
            return Err(Error::Config(format!(
                "Channel '{}' is stopped",
                channel_type
            )));
        }

        if !st.connected {
            st.reconnect_attempts = st.reconnect_attempts.saturating_add(1);
            st.last_start_at_ms = Some(now_ms());
            let connect_res = {
                let ch = st.channel.as_mut().expect("checked above");
                ch.connect().await
            };
            if let Err(e) = connect_res {
                st.last_error = Some(e.to_string());
                st.connected = false;
                st.last_stop_at_ms = Some(now_ms());
                return Err(e);
            }
            st.connected = true;
            st.last_connected_at_ms = Some(now_ms());
            st.last_error = None;
        }

        let ch = st.channel.as_ref().expect("checked above");
        let res = ch.send(to, message).await;
        if res.is_ok() {
            st.last_outbound_at_ms = Some(now_ms());
        }
        res
    }

    pub async fn logout_channel(&self, channel_type: &str) -> Result<()> {
        let channel_type = channel_type.trim();
        if channel_type.is_empty() {
            return Err(Error::InvalidInput("channel required".to_string()));
        }
        let Some(slot) = self.slots.get(channel_type) else {
            return Err(Error::NotFound(format!(
                "Unknown channel: {}",
                channel_type
            )));
        };

        let _ = self.stop_channel(channel_type).await;
        slot.configured.store(false, Ordering::Relaxed);

        let mut st = slot.state.lock().await;
        st.channel = None;
        st.config_hash = None;
        st.running = false;
        st.connected = false;
        st.reconnect_attempts = 0;
        st.last_error = None;
        Ok(())
    }
}

fn pick_default_channel(slots: &HashMap<String, Arc<ChannelSlot>>) -> Option<&'static str> {
    [
        "telegram", "slack", "discord", "signal", "whatsapp", "imessage", "matrix", "webchat",
    ]
    .into_iter()
    .find(|name| {
        slots.get(*name).is_some_and(|slot| {
            slot.enabled.load(Ordering::Relaxed) && slot.configured.load(Ordering::Relaxed)
        })
    })
}

fn hash_parts(parts: &[&str]) -> String {
    let mut bytes: Vec<u8> = Vec::new();
    for (idx, part) in parts.iter().enumerate() {
        if idx > 0 {
            bytes.push(0);
        }
        bytes.extend_from_slice(part.as_bytes());
    }
    let d = digest::digest(&digest::SHA256, &bytes);
    drbot_hex_util::encode(d.as_ref())
}
