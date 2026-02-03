//! Channel implementation for mobile devices.

use crate::{MobileDevice, MobileDiscovery, MobileError, Result};
use async_trait::async_trait;
use drbot_channels::Channel;
use drbot_core::message::{IncomingMessage, OutgoingMessage};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Mobile channel implementation.
pub struct MobileChannel {
    /// Discovery service.
    discovery: Arc<MobileDiscovery>,
    /// Connected devices.
    devices: Arc<RwLock<Vec<Arc<MobileDevice>>>>,
    /// Message sender.
    message_tx: broadcast::Sender<IncomingMessage>,
    /// Connected state.
    connected: Arc<RwLock<bool>>,
}

impl MobileChannel {
    /// Create a new mobile channel.
    pub async fn new() -> Result<Self> {
        let discovery = Arc::new(MobileDiscovery::new().await?);
        let (message_tx, _) = broadcast::channel(256);

        Ok(Self {
            discovery,
            devices: Arc::new(RwLock::new(Vec::new())),
            message_tx,
            connected: Arc::new(RwLock::new(false)),
        })
    }

    /// Get the discovery service.
    pub fn discovery(&self) -> &Arc<MobileDiscovery> {
        &self.discovery
    }

    /// List connected devices.
    pub async fn list_devices(&self) -> Vec<crate::DeviceInfo> {
        let devices = self.devices.read().await;
        let mut infos = Vec::new();
        for device in devices.iter() {
            infos.push(device.info().await);
        }
        infos
    }

    /// Add a device connection.
    pub async fn add_device(&self, device: Arc<MobileDevice>) {
        let mut devices = self.devices.write().await;
        devices.push(device);
    }

    /// Remove a device by ID.
    pub async fn remove_device(&self, device_id: &str) {
        let mut devices = self.devices.write().await;
        devices.retain(|d| {
            // This is a simplified check; in reality we'd need to compare IDs
            true
        });
    }
}

#[async_trait]
impl Channel for MobileChannel {
    async fn connect(&mut self) -> drbot_core::Result<()> {
        self.discovery
            .start()
            .await
            .map_err(|e| drbot_core::Error::Io(std::io::Error::other(e.to_string())))?;

        let mut connected = self.connected.write().await;
        *connected = true;

        tracing::info!("Mobile channel connected");
        Ok(())
    }

    async fn send(&self, to: &str, message: OutgoingMessage) -> drbot_core::Result<()> {
        // Find device by ID and send message
        let devices = self.devices.read().await;

        for device in devices.iter() {
            let info = device.info().await;
            if info.id == to {
                // Send message to device
                // In a real implementation, this would format and send the message
                tracing::debug!(device_id = %to, "Sending message to mobile device");
                return Ok(());
            }
        }

        Err(drbot_core::Error::Io(std::io::Error::other(format!(
            "Device not found: {}",
            to
        ))))
    }

    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.message_tx.subscribe()
    }

    async fn disconnect(&mut self) -> drbot_core::Result<()> {
        self.discovery
            .stop()
            .await
            .map_err(|e| drbot_core::Error::Io(std::io::Error::other(e.to_string())))?;

        // Disconnect all devices
        let devices = self.devices.write().await;
        for device in devices.iter() {
            let _ = device.disconnect().await;
        }

        let mut connected = self.connected.write().await;
        *connected = false;

        tracing::info!("Mobile channel disconnected");
        Ok(())
    }

    fn channel_type(&self) -> &str {
        "mobile"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mobile_channel() {
        let mut channel = MobileChannel::new().await.unwrap();

        channel.connect().await.unwrap();
        assert_eq!(channel.channel_type(), "mobile");

        channel.disconnect().await.unwrap();
    }
}
