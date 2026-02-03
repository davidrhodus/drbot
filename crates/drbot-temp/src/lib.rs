//! Temporary file/directory utilities for drbot.
//!
//! This crate provides:
//! - Temporary files
//! - Temporary directories
//! - Auto-cleanup on drop

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::fs;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Temp error types.
#[derive(Error, Debug)]
pub enum TempError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Failed to create temp: {0}")]
    CreateFailed(String),
}

/// Result type for temp operations.
pub type Result<T> = std::result::Result<T, TempError>;

/// Get system temp directory.
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

/// Generate unique temp name.
pub fn unique_name(prefix: &str, suffix: &str) -> String {
    let id = Uuid::new_v4().to_string()[..8].to_string();
    format!("{}{}{}", prefix, id, suffix)
}

/// Temporary file.
pub struct TempFile {
    path: PathBuf,
    cleanup: bool,
}

impl TempFile {
    /// Create new temp file.
    pub async fn new() -> Result<Self> {
        Self::with_prefix("tmp").await
    }

    /// Create with prefix.
    pub async fn with_prefix(prefix: &str) -> Result<Self> {
        Self::with_prefix_suffix(prefix, "").await
    }

    /// Create with prefix and suffix.
    pub async fn with_prefix_suffix(prefix: &str, suffix: &str) -> Result<Self> {
        let name = unique_name(prefix, suffix);
        let path = temp_dir().join(name);
        fs::write(&path, b"").await?;
        Ok(Self {
            path,
            cleanup: true,
        })
    }

    /// Create in specific directory.
    pub async fn in_dir(dir: impl AsRef<Path>, prefix: &str, suffix: &str) -> Result<Self> {
        let name = unique_name(prefix, suffix);
        let path = dir.as_ref().join(name);
        fs::write(&path, b"").await?;
        Ok(Self {
            path,
            cleanup: true,
        })
    }

    /// Get path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keep file (don't cleanup on drop).
    pub fn keep(mut self) -> PathBuf {
        self.cleanup = false;
        self.path.clone()
    }

    /// Write content.
    pub async fn write(&self, content: &[u8]) -> Result<()> {
        fs::write(&self.path, content).await?;
        Ok(())
    }

    /// Write string.
    pub async fn write_str(&self, content: &str) -> Result<()> {
        self.write(content.as_bytes()).await
    }

    /// Read content.
    pub async fn read(&self) -> Result<Vec<u8>> {
        let content = fs::read(&self.path).await?;
        Ok(content)
    }

    /// Read as string.
    pub async fn read_string(&self) -> Result<String> {
        let content = fs::read_to_string(&self.path).await?;
        Ok(content)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if self.cleanup && self.path.exists() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Temporary directory.
pub struct TempDir {
    path: PathBuf,
    cleanup: bool,
}

impl TempDir {
    /// Create new temp directory.
    pub async fn new() -> Result<Self> {
        Self::with_prefix("tmpdir").await
    }

    /// Create with prefix.
    pub async fn with_prefix(prefix: &str) -> Result<Self> {
        let name = unique_name(prefix, "");
        let path = temp_dir().join(name);
        fs::create_dir_all(&path).await?;
        Ok(Self {
            path,
            cleanup: true,
        })
    }

    /// Create in specific directory.
    pub async fn in_dir(dir: impl AsRef<Path>, prefix: &str) -> Result<Self> {
        let name = unique_name(prefix, "");
        let path = dir.as_ref().join(name);
        fs::create_dir_all(&path).await?;
        Ok(Self {
            path,
            cleanup: true,
        })
    }

    /// Get path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keep directory (don't cleanup on drop).
    pub fn keep(mut self) -> PathBuf {
        self.cleanup = false;
        self.path.clone()
    }

    /// Create file in this directory.
    pub async fn create_file(&self, name: &str) -> Result<PathBuf> {
        let path = self.path.join(name);
        fs::write(&path, b"").await?;
        Ok(path)
    }

    /// Create subdirectory.
    pub async fn create_dir(&self, name: &str) -> Result<PathBuf> {
        let path = self.path.join(name);
        fs::create_dir_all(&path).await?;
        Ok(path)
    }

    /// Get child path.
    pub fn child(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.cleanup && self.path.exists() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// Temp file pool for reuse.
pub struct TempFilePool {
    dir: PathBuf,
    prefix: String,
    suffix: String,
    available: Arc<Mutex<Vec<PathBuf>>>,
}

impl TempFilePool {
    /// Create new pool.
    pub async fn new(prefix: &str, suffix: &str) -> Result<Self> {
        let dir = TempDir::with_prefix("pool").await?;
        Ok(Self {
            dir: dir.keep(),
            prefix: prefix.to_string(),
            suffix: suffix.to_string(),
            available: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Get temp file from pool.
    pub async fn get(&self) -> Result<PooledTempFile> {
        let path = {
            let mut available = self.available.lock().await;
            available.pop()
        };

        let path = match path {
            Some(p) => p,
            None => {
                let name = unique_name(&self.prefix, &self.suffix);
                let path = self.dir.join(name);
                fs::write(&path, b"").await?;
                path
            }
        };

        Ok(PooledTempFile {
            path,
            pool: Arc::clone(&self.available),
        })
    }
}

/// Pooled temp file.
pub struct PooledTempFile {
    path: PathBuf,
    pool: Arc<Mutex<Vec<PathBuf>>>,
}

impl PooledTempFile {
    /// Get path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write content.
    pub async fn write(&self, content: &[u8]) -> Result<()> {
        fs::write(&self.path, content).await?;
        Ok(())
    }

    /// Read content.
    pub async fn read(&self) -> Result<Vec<u8>> {
        let content = fs::read(&self.path).await?;
        Ok(content)
    }
}

impl Drop for PooledTempFile {
    fn drop(&mut self) {
        let path = self.path.clone();
        let pool = Arc::clone(&self.pool);

        // Clear file and return to pool
        tokio::spawn(async move {
            let _ = fs::write(&path, b"").await;
            let mut available = pool.lock().await;
            available.push(path);
        });
    }
}

/// Scoped temp context.
pub struct TempContext {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
}

impl TempContext {
    /// Create new context.
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            dirs: Vec::new(),
        }
    }

    /// Create temp file.
    pub async fn file(&mut self, prefix: &str) -> Result<PathBuf> {
        let temp = TempFile::with_prefix(prefix).await?;
        let path = temp.keep();
        self.files.push(path.clone());
        Ok(path)
    }

    /// Create temp directory.
    pub async fn dir(&mut self, prefix: &str) -> Result<PathBuf> {
        let temp = TempDir::with_prefix(prefix).await?;
        let path = temp.keep();
        self.dirs.push(path.clone());
        Ok(path)
    }

    /// Track existing path for cleanup.
    pub fn track_file(&mut self, path: PathBuf) {
        self.files.push(path);
    }

    /// Track existing directory for cleanup.
    pub fn track_dir(&mut self, path: PathBuf) {
        self.dirs.push(path);
    }
}

impl Default for TempContext {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TempContext {
    fn drop(&mut self) {
        for path in &self.files {
            let _ = std::fs::remove_file(path);
        }
        for path in &self.dirs {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_temp_file() {
        let temp = TempFile::new().await.unwrap();
        temp.write_str("hello").await.unwrap();

        let content = temp.read_string().await.unwrap();
        assert_eq!(content, "hello");

        let path = temp.path().to_path_buf();
        drop(temp);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn test_temp_file_keep() {
        let temp = TempFile::new().await.unwrap();
        let path = temp.keep();
        assert!(path.exists());

        fs::remove_file(&path).await.unwrap();
    }

    #[tokio::test]
    async fn test_temp_dir() {
        let temp = TempDir::new().await.unwrap();
        let file_path = temp.create_file("test.txt").await.unwrap();
        assert!(file_path.exists());

        let dir_path = temp.path().to_path_buf();
        drop(temp);
        assert!(!dir_path.exists());
    }

    #[tokio::test]
    async fn test_unique_name() {
        let name1 = unique_name("test", ".txt");
        let name2 = unique_name("test", ".txt");
        assert_ne!(name1, name2);
        assert!(name1.starts_with("test"));
        assert!(name1.ends_with(".txt"));
    }

    #[tokio::test]
    async fn test_temp_context() {
        let file_path;
        let dir_path;

        {
            let mut ctx = TempContext::new();
            file_path = ctx.file("test").await.unwrap();
            dir_path = ctx.dir("testdir").await.unwrap();

            assert!(file_path.exists());
            assert!(dir_path.exists());
        }

        assert!(!file_path.exists());
        assert!(!dir_path.exists());
    }
}
