//! Runtime channel manager for outbound sends.
//!
//! drbot's core config can enable multiple messaging channels (Telegram, Slack,
//! etc). The legacy gateway currently doesn't manage channels, but OpenClaw's
//! protocol expects `send` / `poll` to work when channels are configured.

use drbot_channels::Channel;
use drbot_core::message::OutgoingMessage;
use drbot_core::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

pub struct ChannelManager {
    slots: HashMap<String, Arc<ChannelSlot>>,
    default_channel: Option<String>,
}

struct ChannelSlot {
    channel_type: String,
    enabled: bool,
    configured: bool,
    state: Mutex<ChannelSlotState>,
}

struct ChannelSlotState {
    channel: Option<Box<dyn Channel>>,
    connected: bool,
    last_error: Option<String>,
}

impl ChannelManager {
    pub fn new(config: &drbot_core::Config) -> Self {
        let mut slots: HashMap<String, Arc<ChannelSlot>> = HashMap::new();

        let is_enabled = |name: &str| config.channels.enabled.iter().any(|c| c == name);

        // Helper to insert a slot even if construction fails; this keeps status/introspection sane.
        let mut insert_slot = |name: &str,
                               enabled: bool,
                               configured: bool,
                               channel: Option<Box<dyn Channel>>,
                               err: Option<String>| {
            slots.insert(
                name.to_string(),
                Arc::new(ChannelSlot {
                    channel_type: name.to_string(),
                    enabled,
                    configured,
                    state: Mutex::new(ChannelSlotState {
                        channel,
                        connected: false,
                        last_error: err,
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
                insert_slot("webchat", enabled, configured, channel, None);
            } else {
                insert_slot("webchat", enabled, configured, None, None);
            }
        }

        // telegram
        {
            let configured = config.channels.telegram.is_some();
            let enabled = is_enabled("telegram");
            if let (true, true) = (configured, enabled) {
                let token = config
                    .channels
                    .telegram
                    .as_ref()
                    .map(|c| c.bot_token.clone())
                    .unwrap_or_default();
                let channel = if token.trim().is_empty() {
                    None
                } else {
                    Some(Box::new(drbot_telegram::TelegramChannel::from_token(token)) as Box<dyn Channel>)
                };
                insert_slot("telegram", enabled, configured, channel, None);
            } else {
                insert_slot("telegram", enabled, configured, None, None);
            }
        }

        // discord
        {
            let configured = config.channels.discord.is_some();
            let enabled = is_enabled("discord");
            if let (true, true) = (configured, enabled) {
                let token = config
                    .channels
                    .discord
                    .as_ref()
                    .map(|c| c.bot_token.clone())
                    .unwrap_or_default();
                let channel = if token.trim().is_empty() {
                    None
                } else {
                    Some(Box::new(drbot_discord::DiscordChannel::from_token(token)) as Box<dyn Channel>)
                };
                insert_slot("discord", enabled, configured, channel, None);
            } else {
                insert_slot("discord", enabled, configured, None, None);
            }
        }

        // slack
        {
            let configured = config.channels.slack.is_some();
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
                let channel = if bot_token.trim().is_empty() || app_token.trim().is_empty() {
                    None
                } else {
                    let cfg = drbot_slack::SlackConfig::new(bot_token, app_token);
                    Some(Box::new(drbot_slack::SlackChannel::new(cfg)) as Box<dyn Channel>)
                };
                insert_slot("slack", enabled, configured, channel, None);
            } else {
                insert_slot("slack", enabled, configured, None, None);
            }
        }

        // whatsapp
        {
            let configured = config.channels.whatsapp.is_some();
            let enabled = is_enabled("whatsapp");
            if let (true, true) = (configured, enabled) {
                let cfg = config.channels.whatsapp.as_ref().unwrap();
                let bridge = cfg.bridge_url.clone();
                let session_dir = cfg.session_path.to_string_lossy().to_string();
                let channel = Box::new(
                    drbot_whatsapp::WhatsAppChannel::new(bridge).with_session_dir(session_dir),
                ) as Box<dyn Channel>;
                insert_slot("whatsapp", enabled, configured, Some(channel), None);
            } else {
                insert_slot("whatsapp", enabled, configured, None, None);
            }
        }

        // signal
        {
            let configured = config.channels.signal.is_some();
            let enabled = is_enabled("signal");
            if let (true, true) = (configured, enabled) {
                let cfg = config.channels.signal.as_ref().unwrap();
                let scfg = drbot_signal::SignalConfig::new(cfg.phone_number.clone())
                    .with_socket_path(cfg.socket_path.clone());
                let channel = Box::new(drbot_signal::SignalChannel::new(scfg)) as Box<dyn Channel>;
                insert_slot("signal", enabled, configured, Some(channel), None);
            } else {
                insert_slot("signal", enabled, configured, None, None);
            }
        }

        // matrix
        {
            let configured = config.channels.matrix.is_some();
            let enabled = is_enabled("matrix");
            if let (true, true) = (configured, enabled) {
                let cfg = config.channels.matrix.as_ref().unwrap();
                let mut mcfg = drbot_matrix::MatrixConfig::new(
                    cfg.homeserver_url.clone(),
                    cfg.user_id.clone(),
                    cfg.access_token.clone(),
                );
                mcfg.allowed_rooms = cfg.allowed_rooms.clone();
                let channel = Box::new(drbot_matrix::MatrixChannel::new(mcfg)) as Box<dyn Channel>;
                insert_slot("matrix", enabled, configured, Some(channel), None);
            } else {
                insert_slot("matrix", enabled, configured, None, None);
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
                        Some(Box::new(channel) as Box<dyn Channel>),
                        None,
                    ),
                    Err(e) => {
                        warn!(error = %e, "Failed to construct iMessage channel");
                        insert_slot("imessage", enabled, configured, None, Some(e.to_string()));
                    }
                }
            } else {
                insert_slot("imessage", enabled, configured, None, None);
            }
        }

        // Pick a default channel for convenience (used when callers omit `channel`).
        let default_channel = [
            "telegram",
            "slack",
            "discord",
            "signal",
            "whatsapp",
            "imessage",
            "matrix",
            "webchat",
        ]
        .into_iter()
        .find(|name| {
            slots
                .get(*name)
                .is_some_and(|slot| slot.enabled && slot.configured)
        })
        .map(|s| s.to_string());

        if let Some(name) = default_channel.as_deref() {
            info!(channel = %name, "OpenClaw: default outbound channel selected");
        }

        Self {
            slots,
            default_channel,
        }
    }

    pub fn default_channel(&self) -> Option<&str> {
        self.default_channel.as_deref()
    }

    pub fn has_channel(&self, channel: &str) -> bool {
        self.slots.contains_key(channel)
    }

    pub fn is_enabled(&self, channel: &str) -> bool {
        self.slots.get(channel).is_some_and(|s| s.enabled)
    }

    pub fn is_configured(&self, channel: &str) -> bool {
        self.slots.get(channel).is_some_and(|s| s.configured)
    }

    pub async fn send(
        &self,
        channel_type: &str,
        to: &str,
        message: OutgoingMessage,
    ) -> Result<()> {
        let channel_type = channel_type.trim();
        if channel_type.is_empty() {
            return Err(Error::InvalidInput("channel required".to_string()));
        }
        let Some(slot) = self.slots.get(channel_type) else {
            return Err(Error::NotFound(format!("Unknown channel: {}", channel_type)));
        };
        if !slot.enabled {
            return Err(Error::Config(format!("Channel '{}' is disabled", channel_type)));
        }
        if !slot.configured {
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

        if !st.connected {
            let connect_res = {
                let ch = st.channel.as_mut().expect("checked above");
                ch.connect().await
            };
            if let Err(e) = connect_res {
                st.last_error = Some(e.to_string());
                return Err(e);
            }
            st.connected = true;
            st.last_error = None;
        }

        let ch = st.channel.as_ref().expect("checked above");
        ch.send(to, message).await
    }
}
