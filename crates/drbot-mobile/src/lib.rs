//! Mobile device nodes (iOS/Android via Bonjour) for drbot.
//!
//! This crate provides integration with mobile devices for camera, screen,
//! and notification access via mDNS/Bonjour service discovery.
//!
//! # Features
//!
//! - mDNS service discovery (`_drbot._tcp.local.`)
//! - Device connection via WebSocket
//! - Camera capture
//! - Screen mirroring
//! - Notification access
//! - Implements the Channel trait
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_mobile::{MobileDiscovery, MobileDevice};
//!
//! async fn example() {
//!     let discovery = MobileDiscovery::new().await.unwrap();
//!     discovery.start().await.unwrap();
//!
//!     // Wait for devices to be discovered
//!     let devices = discovery.list_devices().await;
//!     for device in devices {
//!         println!("Found device: {}", device.name);
//!     }
//! }
//! ```

mod bridge;
mod capabilities;
mod channel;
mod device;
mod discovery;
mod protocol;

pub use bridge::{BridgeConfig, MobileBridge};
pub use capabilities::{CameraCapability, DeviceCapabilities, ScreenCapability};
pub use channel::MobileChannel;
pub use device::{DeviceInfo, DeviceStatus, MobileDevice};
pub use discovery::{DiscoveredDevice, DiscoveryEvent, MobileDiscovery};
pub use protocol::{MobileEvent, MobileRequest, MobileResponse};

use serde::{Deserialize, Serialize};

/// Result type for mobile operations.
pub type Result<T> = std::result::Result<T, MobileError>;

/// Mobile device errors.
#[derive(Debug, thiserror::Error)]
pub enum MobileError {
    #[error("Discovery failed: {0}")]
    DiscoveryFailed(String),
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Operation not supported: {0}")]
    NotSupported(String),
    #[error("Timeout")]
    Timeout,
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Mobile configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileConfig {
    /// Service type for mDNS discovery.
    #[serde(default = "default_service_type")]
    pub service_type: String,
    /// Connection timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// Auto-reconnect on disconnect.
    #[serde(default)]
    pub auto_reconnect: bool,
    /// Maximum reconnect attempts.
    #[serde(default = "default_max_reconnects")]
    pub max_reconnects: u32,
    /// Enable encryption.
    #[serde(default = "default_encryption")]
    pub encryption_enabled: bool,
}

fn default_service_type() -> String {
    "_drbot._tcp.local.".to_string()
}

fn default_timeout() -> u64 {
    10
}

fn default_max_reconnects() -> u32 {
    3
}

fn default_encryption() -> bool {
    true
}

impl Default for MobileConfig {
    fn default() -> Self {
        Self {
            service_type: default_service_type(),
            timeout_secs: default_timeout(),
            auto_reconnect: false,
            max_reconnects: default_max_reconnects(),
            encryption_enabled: default_encryption(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mobile_config_default() {
        let config = MobileConfig::default();
        assert_eq!(config.service_type, "_drbot._tcp.local.");
        assert_eq!(config.timeout_secs, 10);
    }
}
