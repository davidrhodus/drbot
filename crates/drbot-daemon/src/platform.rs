//! Platform detection.

/// Supported platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOS,
    Linux,
    Windows,
    Unknown,
}

impl Platform {
    /// Check if this platform supports daemon installation.
    pub fn supports_daemon(&self) -> bool {
        matches!(self, Platform::MacOS | Platform::Linux)
    }

    /// Get the service manager name.
    pub fn service_manager(&self) -> Option<&'static str> {
        match self {
            Platform::MacOS => Some("launchd"),
            Platform::Linux => Some("systemd"),
            Platform::Windows => Some("sc"),
            Platform::Unknown => None,
        }
    }
}

/// Detect the current platform.
pub fn detect_platform() -> Platform {
    #[cfg(target_os = "macos")]
    {
        Platform::MacOS
    }

    #[cfg(target_os = "linux")]
    {
        Platform::Linux
    }

    #[cfg(target_os = "windows")]
    {
        Platform::Windows
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Platform::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_supports_daemon() {
        assert!(Platform::MacOS.supports_daemon());
        assert!(Platform::Linux.supports_daemon());
        assert!(!Platform::Unknown.supports_daemon());
    }
}
