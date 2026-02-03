//! Device management for sync.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Information about a device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Unique device ID.
    pub id: Uuid,
    /// Device name.
    pub name: String,
    /// Device type.
    pub device_type: DeviceType,
    /// Operating system.
    pub os: String,
    /// Last seen timestamp.
    pub last_seen: DateTime<Utc>,
    /// Whether device is currently online.
    pub is_online: bool,
}

impl DeviceInfo {
    /// Create a new device info.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            device_type: DeviceType::Desktop,
            os: std::env::consts::OS.to_string(),
            last_seen: Utc::now(),
            is_online: true,
        }
    }

    /// Set device type.
    pub fn with_type(mut self, device_type: DeviceType) -> Self {
        self.device_type = device_type;
        self
    }
}

/// Device type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceType {
    Desktop,
    Laptop,
    Mobile,
    Tablet,
    Server,
    Other,
}

/// Device linking information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceLinkInfo {
    /// Device ID to link.
    pub device_id: Uuid,
    /// Device name.
    pub device_name: String,
    /// Link code (short, human-readable).
    pub code: String,
    /// When the link expires.
    pub expires_at: DateTime<Utc>,
}

impl DeviceLinkInfo {
    /// Check if the link has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Format as QR code content.
    pub fn to_qr_content(&self) -> String {
        format!("drbot://link?id={}&code={}", self.device_id, self.code)
    }
}

/// Registry of linked devices.
#[derive(Debug, Clone, Default)]
pub struct DeviceRegistry {
    devices: HashMap<Uuid, DeviceInfo>,
    this_device: Option<Uuid>,
}

impl DeviceRegistry {
    /// Create a new registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set this device.
    pub fn set_this_device(&mut self, device: DeviceInfo) {
        let id = device.id;
        self.devices.insert(id, device);
        self.this_device = Some(id);
    }

    /// Get this device.
    pub fn this_device(&self) -> Option<&DeviceInfo> {
        self.this_device.and_then(|id| self.devices.get(&id))
    }

    /// Add a linked device.
    pub fn add_device(&mut self, device: DeviceInfo) {
        self.devices.insert(device.id, device);
    }

    /// Remove a device.
    pub fn remove_device(&mut self, id: Uuid) -> Option<DeviceInfo> {
        self.devices.remove(&id)
    }

    /// Get a device by ID.
    pub fn get(&self, id: Uuid) -> Option<&DeviceInfo> {
        self.devices.get(&id)
    }

    /// Get all devices.
    pub fn all_devices(&self) -> impl Iterator<Item = &DeviceInfo> {
        self.devices.values()
    }

    /// Get device count.
    pub fn count(&self) -> usize {
        self.devices.len()
    }

    /// Update device last seen.
    pub fn update_last_seen(&mut self, id: Uuid) {
        if let Some(device) = self.devices.get_mut(&id) {
            device.last_seen = Utc::now();
            device.is_online = true;
        }
    }

    /// Mark device as offline.
    pub fn mark_offline(&mut self, id: Uuid) {
        if let Some(device) = self.devices.get_mut(&id) {
            device.is_online = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_info() {
        let device = DeviceInfo::new("My MacBook").with_type(DeviceType::Laptop);

        assert_eq!(device.name, "My MacBook");
        assert_eq!(device.device_type, DeviceType::Laptop);
    }

    #[test]
    fn test_device_link_info() {
        let link = DeviceLinkInfo {
            device_id: Uuid::new_v4(),
            device_name: "Test".to_string(),
            code: "ABC123".to_string(),
            expires_at: Utc::now() + chrono::Duration::minutes(5),
        };

        assert!(!link.is_expired());
        assert!(link.to_qr_content().contains("drbot://link"));
    }

    #[test]
    fn test_device_registry() {
        let mut registry = DeviceRegistry::new();

        let device1 = DeviceInfo::new("Device 1");
        let device2 = DeviceInfo::new("Device 2");

        registry.set_this_device(device1.clone());
        registry.add_device(device2);

        assert_eq!(registry.count(), 2);
        assert!(registry.this_device().is_some());
    }
}
