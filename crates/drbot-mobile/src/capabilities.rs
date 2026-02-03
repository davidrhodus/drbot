//! Device capability definitions.

use serde::{Deserialize, Serialize};

/// Camera capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraCapability {
    /// Available cameras (front, back, etc.).
    pub cameras: Vec<String>,
    /// Maximum resolution.
    pub max_resolution: Option<(u32, u32)>,
    /// Supports video recording.
    pub video_supported: bool,
    /// Supports flash.
    pub flash_supported: bool,
}

impl Default for CameraCapability {
    fn default() -> Self {
        Self {
            cameras: vec!["back".to_string(), "front".to_string()],
            max_resolution: Some((4032, 3024)),
            video_supported: true,
            flash_supported: true,
        }
    }
}

/// Screen capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenCapability {
    /// Screen resolution.
    pub resolution: (u32, u32),
    /// Screen scale factor.
    pub scale: f32,
    /// Supports screen mirroring.
    pub mirroring_supported: bool,
    /// Maximum mirror frame rate.
    pub max_mirror_fps: u32,
}

impl Default for ScreenCapability {
    fn default() -> Self {
        Self {
            resolution: (1170, 2532),
            scale: 3.0,
            mirroring_supported: true,
            max_mirror_fps: 30,
        }
    }
}

/// Device capabilities.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    /// Camera capabilities.
    pub camera: Option<CameraCapability>,
    /// Screen capabilities.
    pub screen: Option<ScreenCapability>,
    /// Can access notifications.
    pub notifications: bool,
    /// Can access clipboard.
    pub clipboard: bool,
    /// Can access location.
    pub location: bool,
    /// Can access contacts.
    pub contacts: bool,
    /// Can access calendar.
    pub calendar: bool,
    /// Can access photos library.
    pub photos: bool,
    /// Can play audio.
    pub audio_playback: bool,
    /// Can record audio.
    pub audio_recording: bool,
    /// Supports haptic feedback.
    pub haptics: bool,
    /// Supports biometric auth.
    pub biometrics: bool,
}

impl DeviceCapabilities {
    /// Create capabilities for a typical iOS device.
    pub fn ios_default() -> Self {
        Self {
            camera: Some(CameraCapability::default()),
            screen: Some(ScreenCapability::default()),
            notifications: true,
            clipboard: true,
            location: true,
            contacts: true,
            calendar: true,
            photos: true,
            audio_playback: true,
            audio_recording: true,
            haptics: true,
            biometrics: true,
        }
    }

    /// Create capabilities for a typical Android device.
    pub fn android_default() -> Self {
        Self {
            camera: Some(CameraCapability::default()),
            screen: Some(ScreenCapability {
                resolution: (1080, 2400),
                scale: 2.75,
                mirroring_supported: true,
                max_mirror_fps: 30,
            }),
            notifications: true,
            clipboard: true,
            location: true,
            contacts: true,
            calendar: true,
            photos: true,
            audio_playback: true,
            audio_recording: true,
            haptics: true,
            biometrics: true,
        }
    }

    /// Check if camera is available.
    pub fn has_camera(&self) -> bool {
        self.camera.is_some()
    }

    /// Check if screen mirroring is available.
    pub fn can_mirror_screen(&self) -> bool {
        self.screen
            .as_ref()
            .map(|s| s.mirroring_supported)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_capabilities() {
        let caps = DeviceCapabilities::ios_default();
        assert!(caps.has_camera());
        assert!(caps.can_mirror_screen());
        assert!(caps.notifications);
    }
}
