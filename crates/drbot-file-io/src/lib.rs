//! File I/O utilities for drbot.
//!
//! This crate provides:
//! - Async file operations
//! - File watching
//! - Atomic file writes
//! - Directory traversal

use async_trait::async_trait;
use serde::{de::DeserializeOwned, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// File I/O error types.
#[derive(Error, Debug)]
pub enum FileError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("File not found: {0}")]
    NotFound(PathBuf),

    #[error("Permission denied: {0}")]
    PermissionDenied(PathBuf),

    #[error("Path exists: {0}")]
    AlreadyExists(PathBuf),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Result type for file operations.
pub type Result<T> = std::result::Result<T, FileError>;

/// File reader.
pub struct FileReader;

impl FileReader {
    /// Read entire file to string.
    pub async fn read_string(path: impl AsRef<Path>) -> Result<String> {
        let path = path.as_ref();
        fs::read_to_string(path)
            .await
            .map_err(|e| Self::map_error(e, path))
    }

    /// Read entire file to bytes.
    pub async fn read_bytes(path: impl AsRef<Path>) -> Result<Vec<u8>> {
        let path = path.as_ref();
        fs::read(path).await.map_err(|e| Self::map_error(e, path))
    }

    /// Read file as JSON.
    pub async fn read_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
        let content = Self::read_string(path).await?;
        serde_json::from_str(&content).map_err(|e| FileError::SerializationError(e.to_string()))
    }

    /// Read lines.
    pub async fn read_lines(path: impl AsRef<Path>) -> Result<Vec<String>> {
        let path = path.as_ref();
        let file = File::open(path)
            .await
            .map_err(|e| Self::map_error(e, path))?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut result = Vec::new();

        while let Some(line) = lines.next_line().await? {
            result.push(line);
        }

        Ok(result)
    }

    fn map_error(e: io::Error, path: &Path) -> FileError {
        match e.kind() {
            io::ErrorKind::NotFound => FileError::NotFound(path.to_path_buf()),
            io::ErrorKind::PermissionDenied => FileError::PermissionDenied(path.to_path_buf()),
            _ => FileError::IoError(e),
        }
    }
}

/// File writer.
pub struct FileWriter;

impl FileWriter {
    /// Write string to file.
    pub async fn write_string(path: impl AsRef<Path>, content: &str) -> Result<()> {
        fs::write(path.as_ref(), content).await?;
        Ok(())
    }

    /// Write bytes to file.
    pub async fn write_bytes(path: impl AsRef<Path>, content: &[u8]) -> Result<()> {
        fs::write(path.as_ref(), content).await?;
        Ok(())
    }

    /// Write JSON to file.
    pub async fn write_json<T: Serialize>(path: impl AsRef<Path>, value: &T) -> Result<()> {
        let content = serde_json::to_string_pretty(value)
            .map_err(|e| FileError::SerializationError(e.to_string()))?;
        Self::write_string(path, &content).await
    }

    /// Append to file.
    pub async fn append(path: impl AsRef<Path>, content: &[u8]) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.as_ref())
            .await?;
        file.write_all(content).await?;
        Ok(())
    }

    /// Append line to file.
    pub async fn append_line(path: impl AsRef<Path>, line: &str) -> Result<()> {
        let content = format!("{}\n", line);
        Self::append(path, content.as_bytes()).await
    }
}

/// Atomic file writer (write to temp then rename).
pub struct AtomicWriter;

impl AtomicWriter {
    /// Write atomically.
    pub async fn write(path: impl AsRef<Path>, content: &[u8]) -> Result<()> {
        let path = path.as_ref();
        let temp_path = Self::temp_path(path);

        // Write to temp file
        fs::write(&temp_path, content).await?;

        // Rename to target
        fs::rename(&temp_path, path).await?;

        Ok(())
    }

    /// Write string atomically.
    pub async fn write_string(path: impl AsRef<Path>, content: &str) -> Result<()> {
        Self::write(path, content.as_bytes()).await
    }

    /// Write JSON atomically.
    pub async fn write_json<T: Serialize>(path: impl AsRef<Path>, value: &T) -> Result<()> {
        let content = serde_json::to_string_pretty(value)
            .map_err(|e| FileError::SerializationError(e.to_string()))?;
        Self::write_string(path, &content).await
    }

    fn temp_path(path: &Path) -> PathBuf {
        let mut temp = path.to_path_buf();
        let name = temp
            .file_name()
            .map(|n| format!(".{}.tmp", n.to_string_lossy()))
            .unwrap_or_else(|| ".tmp".to_string());
        temp.set_file_name(name);
        temp
    }
}

/// Directory operations.
pub struct DirOps;

impl DirOps {
    /// Create directory recursively.
    pub async fn create(path: impl AsRef<Path>) -> Result<()> {
        fs::create_dir_all(path.as_ref()).await?;
        Ok(())
    }

    /// Remove directory recursively.
    pub async fn remove(path: impl AsRef<Path>) -> Result<()> {
        fs::remove_dir_all(path.as_ref()).await?;
        Ok(())
    }

    /// List directory entries.
    pub async fn list(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(path.as_ref()).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            entries.push(entry.path());
        }

        Ok(entries)
    }

    /// List files only.
    pub async fn list_files(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        let entries = Self::list(path).await?;
        let mut files = Vec::new();

        for entry in entries {
            if entry.is_file() {
                files.push(entry);
            }
        }

        Ok(files)
    }

    /// List directories only.
    pub async fn list_dirs(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        let entries = Self::list(path).await?;
        let mut dirs = Vec::new();

        for entry in entries {
            if entry.is_dir() {
                dirs.push(entry);
            }
        }

        Ok(dirs)
    }

    /// Walk directory recursively.
    pub async fn walk(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
        let mut result = Vec::new();
        Self::walk_recursive(path.as_ref(), &mut result).await?;
        Ok(result)
    }

    #[async_recursion::async_recursion]
    async fn walk_recursive(path: &Path, result: &mut Vec<PathBuf>) -> Result<()> {
        let mut read_dir = fs::read_dir(path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            result.push(path.clone());

            if path.is_dir() {
                Self::walk_recursive(&path, result).await?;
            }
        }

        Ok(())
    }
}

/// File utilities.
pub struct FileUtils;

impl FileUtils {
    /// Check if file exists.
    pub async fn exists(path: impl AsRef<Path>) -> bool {
        fs::metadata(path.as_ref()).await.is_ok()
    }

    /// Check if path is file.
    pub async fn is_file(path: impl AsRef<Path>) -> bool {
        fs::metadata(path.as_ref())
            .await
            .map(|m| m.is_file())
            .unwrap_or(false)
    }

    /// Check if path is directory.
    pub async fn is_dir(path: impl AsRef<Path>) -> bool {
        fs::metadata(path.as_ref())
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
    }

    /// Get file size.
    pub async fn size(path: impl AsRef<Path>) -> Result<u64> {
        let metadata = fs::metadata(path.as_ref()).await?;
        Ok(metadata.len())
    }

    /// Copy file.
    pub async fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<u64> {
        let copied = fs::copy(from.as_ref(), to.as_ref()).await?;
        Ok(copied)
    }

    /// Move/rename file.
    pub async fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
        fs::rename(from.as_ref(), to.as_ref()).await?;
        Ok(())
    }

    /// Remove file.
    pub async fn remove(path: impl AsRef<Path>) -> Result<()> {
        fs::remove_file(path.as_ref()).await?;
        Ok(())
    }

    /// Get file extension.
    pub fn extension(path: impl AsRef<Path>) -> Option<String> {
        path.as_ref()
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string())
    }

    /// Get file name without extension.
    pub fn stem(path: impl AsRef<Path>) -> Option<String> {
        path.as_ref()
            .file_stem()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string())
    }

    /// Join path components.
    pub fn join<P: AsRef<Path>>(base: impl AsRef<Path>, parts: &[P]) -> PathBuf {
        let mut result = base.as_ref().to_path_buf();
        for part in parts {
            result = result.join(part);
        }
        result
    }
}

/// Path builder.
pub struct PathBuilder {
    path: PathBuf,
}

impl PathBuilder {
    /// Create new builder.
    pub fn new(base: impl AsRef<Path>) -> Self {
        Self {
            path: base.as_ref().to_path_buf(),
        }
    }

    /// Add path segment.
    pub fn push(mut self, segment: impl AsRef<Path>) -> Self {
        self.path.push(segment);
        self
    }

    /// Add extension.
    pub fn with_extension(mut self, ext: &str) -> Self {
        self.path.set_extension(ext);
        self
    }

    /// Build path.
    pub fn build(self) -> PathBuf {
        self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_read_write_string() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");

        FileWriter::write_string(&path, "hello").await.unwrap();
        let content = FileReader::read_string(&path).await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn test_read_write_json() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.json");

        let data = serde_json::json!({"name": "test", "value": 42});
        FileWriter::write_json(&path, &data).await.unwrap();

        let loaded: serde_json::Value = FileReader::read_json(&path).await.unwrap();
        assert_eq!(loaded["name"], "test");
    }

    #[tokio::test]
    async fn test_append() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");

        FileWriter::append_line(&path, "line1").await.unwrap();
        FileWriter::append_line(&path, "line2").await.unwrap();

        let lines = FileReader::read_lines(&path).await.unwrap();
        assert_eq!(lines, vec!["line1", "line2"]);
    }

    #[tokio::test]
    async fn test_atomic_write() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");

        AtomicWriter::write_string(&path, "content").await.unwrap();
        let content = FileReader::read_string(&path).await.unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_dir_ops() {
        let dir = tempdir().unwrap();
        let sub_dir = dir.path().join("sub");

        DirOps::create(&sub_dir).await.unwrap();
        assert!(FileUtils::is_dir(&sub_dir).await);

        FileWriter::write_string(sub_dir.join("file.txt"), "test")
            .await
            .unwrap();

        let files = DirOps::list_files(&sub_dir).await.unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_path_builder() {
        let path = PathBuilder::new("/home")
            .push("user")
            .push("file")
            .with_extension("txt")
            .build();

        assert_eq!(path, PathBuf::from("/home/user/file.txt"));
    }

    #[test]
    fn test_file_utils() {
        assert_eq!(FileUtils::extension("file.txt"), Some("txt".to_string()));
        assert_eq!(FileUtils::stem("file.txt"), Some("file".to_string()));
    }
}
