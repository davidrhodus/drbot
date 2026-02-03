//! Volume mounting for sandboxed containers.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Mount mode for volumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MountMode {
    /// Read-only access.
    #[default]
    ReadOnly,
    /// Read-write access.
    ReadWrite,
}

impl MountMode {
    /// Get the Docker mount mode string.
    pub fn to_docker_mode(&self) -> &'static str {
        match self {
            Self::ReadOnly => "ro",
            Self::ReadWrite => "rw",
        }
    }
}

impl std::fmt::Display for MountMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadOnly => write!(f, "ro"),
            Self::ReadWrite => write!(f, "rw"),
        }
    }
}

/// A volume mount specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    /// Host path to mount.
    pub host_path: PathBuf,
    /// Container path to mount at.
    pub container_path: PathBuf,
    /// Mount mode.
    #[serde(default)]
    pub mode: MountMode,
    /// Create host path if it doesn't exist.
    #[serde(default)]
    pub create_if_missing: bool,
}

impl VolumeMount {
    /// Create a new volume mount.
    pub fn new(host_path: impl Into<PathBuf>, container_path: impl Into<PathBuf>) -> Self {
        Self {
            host_path: host_path.into(),
            container_path: container_path.into(),
            mode: MountMode::ReadOnly,
            create_if_missing: false,
        }
    }

    /// Set to read-write mode.
    pub fn read_write(mut self) -> Self {
        self.mode = MountMode::ReadWrite;
        self
    }

    /// Set to read-only mode.
    pub fn read_only(mut self) -> Self {
        self.mode = MountMode::ReadOnly;
        self
    }

    /// Enable creating host path if missing.
    pub fn create_if_missing(mut self) -> Self {
        self.create_if_missing = true;
        self
    }

    /// Convert to Docker bind mount string.
    pub fn to_docker_bind(&self) -> String {
        format!(
            "{}:{}:{}",
            self.host_path.display(),
            self.container_path.display(),
            self.mode.to_docker_mode()
        )
    }

    /// Validate the mount configuration.
    pub fn validate(&self) -> Result<(), String> {
        if !self.host_path.is_absolute() {
            return Err("Host path must be absolute".to_string());
        }

        if !self.container_path.is_absolute() {
            return Err("Container path must be absolute".to_string());
        }

        // Check for path traversal attempts
        let host_str = self.host_path.to_string_lossy();
        if host_str.contains("..") {
            return Err("Host path must not contain '..'".to_string());
        }

        Ok(())
    }

    /// Ensure the host path exists.
    pub fn ensure_exists(&self) -> std::io::Result<()> {
        if self.create_if_missing && !self.host_path.exists() {
            std::fs::create_dir_all(&self.host_path)?;
        }
        Ok(())
    }
}

/// Builder for creating multiple volume mounts.
pub struct VolumeMountBuilder {
    mounts: Vec<VolumeMount>,
}

impl VolumeMountBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self { mounts: Vec::new() }
    }

    /// Add a read-only mount.
    pub fn add_readonly(
        mut self,
        host_path: impl Into<PathBuf>,
        container_path: impl Into<PathBuf>,
    ) -> Self {
        self.mounts
            .push(VolumeMount::new(host_path, container_path).read_only());
        self
    }

    /// Add a read-write mount.
    pub fn add_readwrite(
        mut self,
        host_path: impl Into<PathBuf>,
        container_path: impl Into<PathBuf>,
    ) -> Self {
        self.mounts
            .push(VolumeMount::new(host_path, container_path).read_write());
        self
    }

    /// Add a custom mount.
    pub fn add(mut self, mount: VolumeMount) -> Self {
        self.mounts.push(mount);
        self
    }

    /// Build the list of mounts.
    pub fn build(self) -> Vec<VolumeMount> {
        self.mounts
    }

    /// Build and convert to Docker bind strings.
    pub fn to_docker_binds(self) -> Vec<String> {
        self.mounts
            .into_iter()
            .map(|m| m.to_docker_bind())
            .collect()
    }
}

impl Default for VolumeMountBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_mount() {
        let mount = VolumeMount::new("/host/path", "/container/path").read_write();

        assert_eq!(mount.mode, MountMode::ReadWrite);
        assert_eq!(mount.to_docker_bind(), "/host/path:/container/path:rw");
    }

    #[test]
    fn test_volume_mount_validation() {
        let valid = VolumeMount::new("/host/path", "/container/path");
        assert!(valid.validate().is_ok());

        let invalid_host = VolumeMount::new("relative/path", "/container/path");
        assert!(invalid_host.validate().is_err());

        let invalid_container = VolumeMount::new("/host/path", "relative/path");
        assert!(invalid_container.validate().is_err());
    }

    #[test]
    fn test_volume_mount_builder() {
        let binds = VolumeMountBuilder::new()
            .add_readonly("/data", "/mnt/data")
            .add_readwrite("/workspace", "/workspace")
            .to_docker_binds();

        assert_eq!(binds.len(), 2);
        assert!(binds[0].ends_with(":ro"));
        assert!(binds[1].ends_with(":rw"));
    }
}
