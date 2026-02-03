//! Mobile device connection and management.

use crate::{DeviceCapabilities, MobileError, MobileRequest, MobileResponse, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Device connection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    /// Not connected.
    Disconnected,
    /// Connecting.
    Connecting,
    /// Connected and ready.
    Connected,
    /// Connection error.
    Error,
}

/// Device information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Device ID.
    pub id: String,
    /// Device name.
    pub name: String,
    /// Device type (ios, android).
    pub device_type: String,
    /// Device model.
    pub model: Option<String>,
    /// OS version.
    pub os_version: Option<String>,
    /// App version.
    pub app_version: Option<String>,
    /// Device capabilities.
    pub capabilities: DeviceCapabilities,
    /// Connection status.
    pub status: DeviceStatus,
    /// Connected at.
    pub connected_at: Option<DateTime<Utc>>,
}

/// A connected mobile device.
pub struct MobileDevice {
    /// Device info.
    info: Arc<RwLock<DeviceInfo>>,
    /// WebSocket URL.
    ws_url: String,
    /// Request sender.
    request_tx: broadcast::Sender<MobileRequest>,
    /// Response receiver.
    response_tx: broadcast::Sender<MobileResponse>,
}

impl MobileDevice {
    /// Create a new mobile device connection.
    pub async fn connect(ws_url: &str, info: DeviceInfo) -> Result<Self> {
        let (request_tx, _) = broadcast::channel(64);
        let (response_tx, _) = broadcast::channel(64);

        let device = Self {
            info: Arc::new(RwLock::new(info)),
            ws_url: ws_url.to_string(),
            request_tx,
            response_tx,
        };

        // In a real implementation, this would establish a WebSocket connection
        // For now, we'll just mark as connected

        {
            let mut info = device.info.write().await;
            info.status = DeviceStatus::Connected;
            info.connected_at = Some(Utc::now());
        }

        tracing::info!(ws_url = %ws_url, "Connected to mobile device");

        Ok(device)
    }

    /// Get device info.
    pub async fn info(&self) -> DeviceInfo {
        self.info.read().await.clone()
    }

    /// Get device ID.
    pub async fn id(&self) -> String {
        self.info.read().await.id.clone()
    }

    /// Get device status.
    pub async fn status(&self) -> DeviceStatus {
        self.info.read().await.status
    }

    /// Check if device is connected.
    pub async fn is_connected(&self) -> bool {
        self.info.read().await.status == DeviceStatus::Connected
    }

    /// Send a request to the device.
    pub async fn send(&self, request: MobileRequest) -> Result<MobileResponse> {
        if !self.is_connected().await {
            return Err(MobileError::ConnectionFailed("Not connected".into()));
        }

        // Send request
        let _ = self.request_tx.send(request.clone());

        // In a real implementation, we'd wait for the response
        // For now, return a mock response
        Ok(MobileResponse::success(request.id, serde_json::json!({})))
    }

    /// Take a photo using the device camera.
    pub async fn take_photo(&self, camera: Option<&str>) -> Result<Vec<u8>> {
        let request = MobileRequest::take_photo(camera);
        let response = self.send(request).await?;

        if response.success {
            // In a real implementation, decode the base64 image data
            Ok(Vec::new())
        } else {
            Err(MobileError::ProtocolError(
                response.error.unwrap_or_default(),
            ))
        }
    }

    /// Take a screenshot.
    pub async fn take_screenshot(&self) -> Result<Vec<u8>> {
        let request = MobileRequest::screenshot();
        let response = self.send(request).await?;

        if response.success {
            Ok(Vec::new())
        } else {
            Err(MobileError::ProtocolError(
                response.error.unwrap_or_default(),
            ))
        }
    }

    /// Get notifications.
    pub async fn get_notifications(
        &self,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<serde_json::Value>> {
        let request = MobileRequest::get_notifications(since);
        let response = self.send(request).await?;

        if response.success {
            let notifications = response
                .data
                .as_array()
                .map(|arr| arr.to_vec())
                .unwrap_or_default();
            Ok(notifications)
        } else {
            Err(MobileError::ProtocolError(
                response.error.unwrap_or_default(),
            ))
        }
    }

    /// Start screen mirroring.
    pub async fn start_screen_mirror(&self) -> Result<()> {
        let request = MobileRequest::start_screen_mirror();
        let response = self.send(request).await?;

        if response.success {
            Ok(())
        } else {
            Err(MobileError::ProtocolError(
                response.error.unwrap_or_default(),
            ))
        }
    }

    /// Stop screen mirroring.
    pub async fn stop_screen_mirror(&self) -> Result<()> {
        let request = MobileRequest::stop_screen_mirror();
        let response = self.send(request).await?;

        if response.success {
            Ok(())
        } else {
            Err(MobileError::ProtocolError(
                response.error.unwrap_or_default(),
            ))
        }
    }

    /// Disconnect from the device.
    pub async fn disconnect(&self) -> Result<()> {
        let mut info = self.info.write().await;
        info.status = DeviceStatus::Disconnected;

        tracing::info!(device_id = %info.id, "Disconnected from mobile device");

        Ok(())
    }

    /// Subscribe to responses.
    pub fn subscribe_responses(&self) -> broadcast::Receiver<MobileResponse> {
        self.response_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DeviceCapabilities;

    #[tokio::test]
    async fn test_mobile_device() {
        let info = DeviceInfo {
            id: "test-device".to_string(),
            name: "Test iPhone".to_string(),
            device_type: "ios".to_string(),
            model: Some("iPhone 15".to_string()),
            os_version: Some("17.0".to_string()),
            app_version: Some("1.0.0".to_string()),
            capabilities: DeviceCapabilities::default(),
            status: DeviceStatus::Disconnected,
            connected_at: None,
        };

        let device = MobileDevice::connect("ws://192.168.1.100:8080/drbot", info)
            .await
            .unwrap();

        assert!(device.is_connected().await);
        assert_eq!(device.status().await, DeviceStatus::Connected);

        device.disconnect().await.unwrap();
        assert_eq!(device.status().await, DeviceStatus::Disconnected);
    }
}
