//! OpenClaw `web.login.*` runtime helpers.
//!
//! OpenClaw exposes `web.login.start` / `web.login.wait` for channels that require
//! scanning a QR code (e.g., WhatsApp Web). drbot's WhatsApp integration uses a
//! Node bridge; we surface its QR + connection status through this in-memory store.

use drbot_whatsapp::ConnectionStatus;
use qrcode::render::svg;
use qrcode::QrCode;
use tokio::sync::watch;
use tracing::warn;

fn now_ms() -> u64 {
    chrono::Utc::now()
        .timestamp_millis()
        .try_into()
        .unwrap_or(0)
}

#[derive(Debug, Clone)]
pub struct WhatsAppWebLoginState {
    pub connected: bool,
    pub status: ConnectionStatus,
    pub qr_data_url: Option<String>,
    pub updated_at_ms: u64,
}

impl Default for WhatsAppWebLoginState {
    fn default() -> Self {
        Self {
            connected: false,
            status: ConnectionStatus::Close,
            qr_data_url: None,
            updated_at_ms: 0,
        }
    }
}

#[derive(Debug)]
pub struct OpenclawWebLoginStore {
    whatsapp_tx: watch::Sender<WhatsAppWebLoginState>,
}

impl OpenclawWebLoginStore {
    pub fn new() -> Self {
        let (whatsapp_tx, _rx) = watch::channel(WhatsAppWebLoginState::default());
        Self { whatsapp_tx }
    }

    pub fn subscribe_whatsapp(&self) -> watch::Receiver<WhatsAppWebLoginState> {
        self.whatsapp_tx.subscribe()
    }

    pub fn snapshot_whatsapp(&self) -> WhatsAppWebLoginState {
        self.whatsapp_tx.borrow().clone()
    }

    pub fn reset_whatsapp(&self) {
        let mut next = self.snapshot_whatsapp();
        next.connected = false;
        next.qr_data_url = None;
        next.updated_at_ms = now_ms();
        let _ = self.whatsapp_tx.send(next);
    }

    pub fn note_whatsapp_status(&self, status: ConnectionStatus) {
        let mut next = self.snapshot_whatsapp();
        next.status = status;
        next.connected = status == ConnectionStatus::Open;
        if next.connected {
            next.qr_data_url = None;
        }
        next.updated_at_ms = now_ms();
        let _ = self.whatsapp_tx.send(next);
    }

    pub fn note_whatsapp_qr(&self, qr: &str) {
        let qr = qr.trim();
        if qr.is_empty() {
            return;
        }
        let code = match QrCode::new(qr.as_bytes()) {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "OpenClaw web login: failed to generate QR");
                return;
            }
        };
        let svg = code.render::<svg::Color>().min_dimensions(256, 256).build();
        let b64 = drbot_base64_util::encode(svg.as_bytes());
        let data_url = format!("data:image/svg+xml;base64,{}", b64);

        let mut next = self.snapshot_whatsapp();
        next.connected = false;
        next.status = ConnectionStatus::Connecting;
        next.qr_data_url = Some(data_url);
        next.updated_at_ms = now_ms();
        let _ = self.whatsapp_tx.send(next);
    }
}
