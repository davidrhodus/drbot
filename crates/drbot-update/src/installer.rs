//! Update installation functionality.

use crate::{Result, RollbackManager, UpdateError, UpdateProgress, UpdateStage};
use std::path::{Path, PathBuf};

/// Update installer.
pub struct UpdateInstaller {
    rollback_manager: RollbackManager,
}

impl UpdateInstaller {
    /// Create a new installer.
    pub fn new() -> Self {
        Self {
            rollback_manager: RollbackManager::new(),
        }
    }

    /// Install an update from a downloaded binary.
    pub async fn install<F>(
        &self,
        downloaded_binary: &Path,
        progress_callback: Option<F>,
    ) -> Result<()>
    where
        F: Fn(UpdateProgress) + Send,
    {
        let current_exe = std::env::current_exe()?;

        // Backup current version
        if let Some(ref callback) = progress_callback {
            callback(UpdateProgress {
                stage: UpdateStage::BackingUp,
                percent: 0,
                bytes_downloaded: None,
                total_bytes: None,
                message: Some("Backing up current version...".to_string()),
            });
        }

        self.rollback_manager.backup_current().await?;

        if let Some(ref callback) = progress_callback {
            callback(UpdateProgress {
                stage: UpdateStage::BackingUp,
                percent: 100,
                bytes_downloaded: None,
                total_bytes: None,
                message: Some("Backup complete".to_string()),
            });
        }

        // Install new version
        if let Some(ref callback) = progress_callback {
            callback(UpdateProgress {
                stage: UpdateStage::Installing,
                percent: 0,
                bytes_downloaded: None,
                total_bytes: None,
                message: Some("Installing update...".to_string()),
            });
        }

        // Platform-specific installation
        #[cfg(unix)]
        {
            self.install_unix(downloaded_binary, &current_exe).await?;
        }

        #[cfg(windows)]
        {
            self.install_windows(downloaded_binary, &current_exe)
                .await?;
        }

        if let Some(ref callback) = progress_callback {
            callback(UpdateProgress {
                stage: UpdateStage::Installing,
                percent: 100,
                bytes_downloaded: None,
                total_bytes: None,
                message: Some("Installation complete".to_string()),
            });
        }

        // Finalize
        if let Some(ref callback) = progress_callback {
            callback(UpdateProgress {
                stage: UpdateStage::Finalizing,
                percent: 0,
                bytes_downloaded: None,
                total_bytes: None,
                message: Some("Finalizing...".to_string()),
            });
        }

        // Cleanup
        tokio::fs::remove_file(downloaded_binary).await.ok();

        if let Some(ref callback) = progress_callback {
            callback(UpdateProgress {
                stage: UpdateStage::Complete,
                percent: 100,
                bytes_downloaded: None,
                total_bytes: None,
                message: Some("Update complete. Please restart.".to_string()),
            });
        }

        Ok(())
    }

    #[cfg(unix)]
    async fn install_unix(&self, new_binary: &Path, current_exe: &Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        // Make new binary executable
        let mut perms = tokio::fs::metadata(new_binary).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(new_binary, perms).await?;

        // On Unix, we rename the old binary and move the new one in place
        let backup_path = current_exe.with_extension("old");

        // Remove any existing backup
        tokio::fs::remove_file(&backup_path).await.ok();

        // Rename current to backup
        tokio::fs::rename(current_exe, &backup_path)
            .await
            .map_err(|e| {
                UpdateError::InstallationFailed(format!("Failed to backup current: {}", e))
            })?;

        // Copy new binary to current location
        tokio::fs::copy(new_binary, current_exe)
            .await
            .map_err(|e| {
                // Try to restore backup
                let _ = std::fs::rename(&backup_path, current_exe);
                UpdateError::InstallationFailed(format!("Failed to install new binary: {}", e))
            })?;

        // Remove backup (leave it if removal fails - not critical)
        tokio::fs::remove_file(&backup_path).await.ok();

        Ok(())
    }

    #[cfg(windows)]
    async fn install_windows(&self, new_binary: &Path, current_exe: &Path) -> Result<()> {
        // On Windows, we can't replace a running executable directly
        // Create a batch script to do the replacement after exit
        let script_path = current_exe.with_extension("update.bat");
        let new_binary_str = new_binary.to_string_lossy();
        let current_exe_str = current_exe.to_string_lossy();

        let script = format!(
            r#"@echo off
:retry
timeout /t 1 /nobreak >nul
del "{current_exe}" 2>nul
if exist "{current_exe}" goto retry
copy "{new_binary}" "{current_exe}"
del "{new_binary}"
del "%~f0"
"#,
            current_exe = current_exe_str,
            new_binary = new_binary_str,
        );

        tokio::fs::write(&script_path, script).await?;

        // Schedule the script to run after we exit
        // The parent process should detect this and not restart immediately
        tracing::info!(
            script_path = %script_path.display(),
            "Created update script for Windows"
        );

        Ok(())
    }

    /// Get the current executable path.
    pub fn current_exe_path() -> Result<PathBuf> {
        std::env::current_exe().map_err(UpdateError::from)
    }
}

impl Default for UpdateInstaller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_exe_path() {
        let result = UpdateInstaller::current_exe_path();
        assert!(result.is_ok());
    }
}
