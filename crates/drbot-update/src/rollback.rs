//! Rollback functionality.

use crate::{Result, UpdateError};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Rollback manager for reverting to previous versions.
pub struct RollbackManager {
    backup_dir: PathBuf,
    max_backups: usize,
}

impl RollbackManager {
    /// Create a new rollback manager.
    pub fn new() -> Self {
        Self {
            backup_dir: Self::default_backup_dir(),
            max_backups: 2,
        }
    }

    /// Create with custom settings.
    pub fn with_settings(backup_dir: PathBuf, max_backups: usize) -> Self {
        Self {
            backup_dir,
            max_backups,
        }
    }

    /// Get the default backup directory.
    pub fn default_backup_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("drbot")
            .join("backups")
    }

    /// Backup the current binary.
    pub async fn backup_current(&self) -> Result<PathBuf> {
        let current_exe = std::env::current_exe()?;
        let version = env!("CARGO_PKG_VERSION");

        // Create backup directory
        tokio::fs::create_dir_all(&self.backup_dir).await?;

        // Create backup with timestamp
        let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
        let backup_name = format!("drbot-{}-{}", version, timestamp);

        #[cfg(windows)]
        let backup_name = format!("{}.exe", backup_name);

        let backup_path = self.backup_dir.join(&backup_name);

        // Copy current binary
        tokio::fs::copy(&current_exe, &backup_path).await?;

        // Update backup metadata
        self.update_metadata(version, &backup_path).await?;

        // Prune old backups
        self.prune_old_backups().await?;

        tracing::info!(
            backup_path = %backup_path.display(),
            "Created backup of current version"
        );

        Ok(backup_path)
    }

    /// List available backups.
    pub async fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let metadata = self.load_metadata().await?;
        Ok(metadata.backups)
    }

    /// Rollback to the most recent backup.
    pub async fn rollback(&self) -> Result<()> {
        let metadata = self.load_metadata().await?;

        let backup = metadata
            .backups
            .first()
            .ok_or(UpdateError::NoRollbackAvailable)?;

        self.rollback_to(&backup.path).await
    }

    /// Rollback to a specific version.
    pub async fn rollback_to_version(&self, version: &str) -> Result<()> {
        let metadata = self.load_metadata().await?;

        let backup = metadata
            .backups
            .iter()
            .find(|b| b.version == version)
            .ok_or(UpdateError::NoRollbackAvailable)?;

        self.rollback_to(&backup.path).await
    }

    /// Rollback to a specific backup.
    async fn rollback_to(&self, backup_path: &Path) -> Result<()> {
        if !backup_path.exists() {
            return Err(UpdateError::RollbackFailed(
                "Backup file not found".to_string(),
            ));
        }

        let current_exe = std::env::current_exe()?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            // Make backup executable
            let mut perms = tokio::fs::metadata(backup_path).await?.permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(backup_path, perms).await?;

            // Rename current and replace
            let temp_path = current_exe.with_extension("rollback-temp");
            tokio::fs::rename(&current_exe, &temp_path)
                .await
                .map_err(|e| UpdateError::RollbackFailed(e.to_string()))?;

            tokio::fs::copy(backup_path, &current_exe)
                .await
                .map_err(|e| {
                    // Try to restore
                    let _ = std::fs::rename(&temp_path, &current_exe);
                    UpdateError::RollbackFailed(e.to_string())
                })?;

            tokio::fs::remove_file(&temp_path).await.ok();
        }

        #[cfg(windows)]
        {
            // Similar to install, create a batch script
            let script_path = current_exe.with_extension("rollback.bat");
            let backup_str = backup_path.to_string_lossy();
            let current_str = current_exe.to_string_lossy();

            let script = format!(
                r#"@echo off
:retry
timeout /t 1 /nobreak >nul
del "{current}" 2>nul
if exist "{current}" goto retry
copy "{backup}" "{current}"
del "%~f0"
"#,
                current = current_str,
                backup = backup_str,
            );

            tokio::fs::write(&script_path, script).await?;
        }

        tracing::info!(
            backup_path = %backup_path.display(),
            "Rolled back to backup"
        );

        Ok(())
    }

    /// Update backup metadata.
    async fn update_metadata(&self, version: &str, backup_path: &Path) -> Result<()> {
        let mut metadata = self.load_metadata().await.unwrap_or_default();

        metadata.backups.insert(
            0,
            BackupInfo {
                version: version.to_string(),
                path: backup_path.to_path_buf(),
                created_at: chrono::Utc::now(),
            },
        );

        // Keep only max_backups entries
        metadata.backups.truncate(self.max_backups);

        self.save_metadata(&metadata).await
    }

    /// Load backup metadata.
    async fn load_metadata(&self) -> Result<BackupMetadata> {
        let path = self.backup_dir.join("metadata.json");
        if !path.exists() {
            return Ok(BackupMetadata::default());
        }

        let content = tokio::fs::read_to_string(&path).await?;
        let metadata: BackupMetadata = serde_json::from_str(&content)?;
        Ok(metadata)
    }

    /// Save backup metadata.
    async fn save_metadata(&self, metadata: &BackupMetadata) -> Result<()> {
        let path = self.backup_dir.join("metadata.json");
        let content = serde_json::to_string_pretty(metadata)?;
        tokio::fs::write(&path, content).await?;
        Ok(())
    }

    /// Prune old backups beyond the limit.
    async fn prune_old_backups(&self) -> Result<()> {
        let metadata = self.load_metadata().await?;

        // Remove backup files not in metadata
        let mut entries = tokio::fs::read_dir(&self.backup_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Skip metadata file
            if path
                .file_name()
                .map(|n| n == "metadata.json")
                .unwrap_or(false)
            {
                continue;
            }

            // Check if path is in current backups
            let in_metadata = metadata.backups.iter().any(|b| b.path == path);
            if !in_metadata {
                tokio::fs::remove_file(&path).await.ok();
            }
        }

        Ok(())
    }
}

impl Default for RollbackManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Backup metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupMetadata {
    /// List of available backups (newest first).
    pub backups: Vec<BackupInfo>,
}

/// Information about a backup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Version of the backup.
    pub version: String,
    /// Path to the backup file.
    pub path: PathBuf,
    /// Creation timestamp.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_backup_dir() {
        let dir = RollbackManager::default_backup_dir();
        assert!(dir.ends_with("backups"));
    }

    #[tokio::test]
    async fn test_list_empty_backups() {
        let manager =
            RollbackManager::with_settings(PathBuf::from("/nonexistent/drbot/backups"), 2);
        let backups = manager.list_backups().await.unwrap_or_default();
        assert!(backups.is_empty());
    }
}
