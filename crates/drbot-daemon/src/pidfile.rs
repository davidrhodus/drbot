//! PID file management.

use std::fs;
use std::path::{Path, PathBuf};

/// PID file errors.
#[derive(Debug, thiserror::Error)]
pub enum PidFileError {
    #[error("PID file already exists")]
    AlreadyExists,
    #[error("PID file not found")]
    NotFound,
    #[error("Invalid PID in file")]
    InvalidPid,
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// PID file manager.
pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// Create a new PID file manager.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Get the default PID file path.
    pub fn default_path() -> PathBuf {
        dirs::runtime_dir()
            .or_else(|| dirs::data_dir())
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("drbot.pid")
    }

    /// Create the PID file with the current process ID.
    pub fn create(&self) -> Result<(), PidFileError> {
        if self.path.exists() {
            // Check if the existing process is still running
            if let Ok(pid) = self.read() {
                if is_process_running(pid) {
                    return Err(PidFileError::AlreadyExists);
                }
            }
            // Stale PID file, remove it
            fs::remove_file(&self.path)?;
        }

        // Create parent directory if needed
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let pid = std::process::id();
        fs::write(&self.path, pid.to_string())?;

        Ok(())
    }

    /// Read the PID from the file.
    pub fn read(&self) -> Result<u32, PidFileError> {
        if !self.path.exists() {
            return Err(PidFileError::NotFound);
        }

        let content = fs::read_to_string(&self.path)?;
        content.trim().parse().map_err(|_| PidFileError::InvalidPid)
    }

    /// Remove the PID file.
    pub fn remove(&self) -> Result<(), PidFileError> {
        if !self.path.exists() {
            return Ok(());
        }

        fs::remove_file(&self.path)?;
        Ok(())
    }

    /// Check if the PID file exists.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Check if the process in the PID file is still running.
    pub fn is_running(&self) -> bool {
        if let Ok(pid) = self.read() {
            is_process_running(pid)
        } else {
            false
        }
    }

    /// Get the PID file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        // Only remove if the PID matches our process
        if let Ok(pid) = self.read() {
            if pid == std::process::id() {
                let _ = self.remove();
            }
        }
    }
}

/// Check if a process with the given PID is running.
fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Send signal 0 to check if process exists
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[cfg(windows)]
    {
        // On Windows, we'd use OpenProcess
        false
    }

    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_pidfile() {
        let temp_dir = TempDir::new().unwrap();
        let pid_path = temp_dir.path().join("test.pid");

        let pidfile = PidFile::new(&pid_path);

        assert!(!pidfile.exists());

        pidfile.create().unwrap();
        assert!(pidfile.exists());

        let pid = pidfile.read().unwrap();
        assert_eq!(pid, std::process::id());

        pidfile.remove().unwrap();
        assert!(!pidfile.exists());
    }
}
