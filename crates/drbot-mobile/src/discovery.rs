//! mDNS service discovery for mobile devices.

use crate::{MobileConfig, MobileError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// A discovered mobile device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredDevice {
    /// Device ID.
    pub id: String,
    /// Device name.
    pub name: String,
    /// Device type (ios, android).
    pub device_type: String,
    /// IP address.
    pub ip: IpAddr,
    /// Port number.
    pub port: u16,
    /// When discovered.
    pub discovered_at: DateTime<Utc>,
    /// Last seen.
    pub last_seen: DateTime<Utc>,
    /// Additional properties from TXT record.
    pub properties: HashMap<String, String>,
}

impl DiscoveredDevice {
    /// Get the WebSocket URL for this device.
    pub fn websocket_url(&self) -> String {
        format!("ws://{}:{}/drbot", self.ip, self.port)
    }

    /// Check if the device is iOS.
    pub fn is_ios(&self) -> bool {
        self.device_type.to_lowercase() == "ios"
    }

    /// Check if the device is Android.
    pub fn is_android(&self) -> bool {
        self.device_type.to_lowercase() == "android"
    }
}

/// Discovery events.
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A new device was discovered.
    DeviceFound(DiscoveredDevice),
    /// A device was lost.
    DeviceLost(String),
    /// Discovery started.
    Started,
    /// Discovery stopped.
    Stopped,
    /// Discovery error.
    Error(String),
}

/// Mobile device discovery via mDNS.
pub struct MobileDiscovery {
    /// Configuration.
    config: MobileConfig,
    /// Discovered devices.
    devices: Arc<RwLock<HashMap<String, DiscoveredDevice>>>,
    /// Event sender.
    event_tx: broadcast::Sender<DiscoveryEvent>,
    /// Running state.
    running: Arc<RwLock<bool>>,
}

impl MobileDiscovery {
    /// Create a new mobile discovery instance.
    pub async fn new() -> Result<Self> {
        Self::with_config(MobileConfig::default()).await
    }

    /// Create with custom configuration.
    pub async fn with_config(config: MobileConfig) -> Result<Self> {
        let (event_tx, _) = broadcast::channel(64);

        Ok(Self {
            config,
            devices: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Start discovery.
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            return Ok(());
        }
        *running = true;

        // In a real implementation, this would use mdns-sd to browse for services
        // For now, we'll simulate the discovery process

        let _ = self.event_tx.send(DiscoveryEvent::Started);

        tracing::info!(
            service_type = %self.config.service_type,
            "Started mDNS discovery"
        );

        Ok(())
    }

    /// Stop discovery.
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Ok(());
        }
        *running = false;

        let _ = self.event_tx.send(DiscoveryEvent::Stopped);

        tracing::info!("Stopped mDNS discovery");

        Ok(())
    }

    /// Check if discovery is running.
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// List all discovered devices.
    pub async fn list_devices(&self) -> Vec<DiscoveredDevice> {
        let devices = self.devices.read().await;
        devices.values().cloned().collect()
    }

    /// Get a device by ID.
    pub async fn get_device(&self, id: &str) -> Option<DiscoveredDevice> {
        let devices = self.devices.read().await;
        devices.get(id).cloned()
    }

    /// Subscribe to discovery events.
    pub fn subscribe(&self) -> broadcast::Receiver<DiscoveryEvent> {
        self.event_tx.subscribe()
    }

    /// Manually add a device (for testing or direct connection).
    pub async fn add_device(&self, device: DiscoveredDevice) {
        let id = device.id.clone();
        let mut devices = self.devices.write().await;
        devices.insert(id, device.clone());

        let _ = self.event_tx.send(DiscoveryEvent::DeviceFound(device));
    }

    /// Remove a device.
    pub async fn remove_device(&self, id: &str) {
        let mut devices = self.devices.write().await;
        if devices.remove(id).is_some() {
            let _ = self
                .event_tx
                .send(DiscoveryEvent::DeviceLost(id.to_string()));
        }
    }

    /// Clean up stale devices.
    pub async fn cleanup_stale(&self, max_age_secs: i64) -> usize {
        let now = Utc::now();
        let threshold = chrono::Duration::seconds(max_age_secs);

        let mut devices = self.devices.write().await;
        let initial_len = devices.len();

        let stale: Vec<String> = devices
            .iter()
            .filter(|(_, d)| now - d.last_seen > threshold)
            .map(|(id, _)| id.clone())
            .collect();

        for id in &stale {
            devices.remove(id);
            let _ = self.event_tx.send(DiscoveryEvent::DeviceLost(id.clone()));
        }

        initial_len - devices.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn test_mobile_discovery() {
        let discovery = MobileDiscovery::new().await.unwrap();

        discovery.start().await.unwrap();
        assert!(discovery.is_running().await);

        let device = DiscoveredDevice {
            id: "test-device".to_string(),
            name: "Test iPhone".to_string(),
            device_type: "ios".to_string(),
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)),
            port: 8080,
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
            properties: HashMap::new(),
        };

        discovery.add_device(device.clone()).await;

        let devices = discovery.list_devices().await;
        assert_eq!(devices.len(), 1);
        assert!(devices[0].is_ios());

        discovery.stop().await.unwrap();
        assert!(!discovery.is_running().await);
    }
}
