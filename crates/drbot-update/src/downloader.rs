//! Update download functionality.

use crate::{BinaryInfo, Result, UpdateError, UpdateProgress, UpdateStage};
use ring::digest::{Context, SHA256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Update downloader.
pub struct UpdateDownloader {
    client: reqwest::Client,
}

impl UpdateDownloader {
    /// Create a new downloader.
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Download an update binary.
    pub async fn download<F>(
        &self,
        binary_info: &BinaryInfo,
        target_path: &Path,
        progress_callback: Option<F>,
    ) -> Result<PathBuf>
    where
        F: Fn(UpdateProgress) + Send,
    {
        // Create parent directory
        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Start download
        let response = self.client.get(&binary_info.url).send().await?;

        if !response.status().is_success() {
            return Err(UpdateError::DownloadFailed(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let total_size = response.content_length().unwrap_or(binary_info.size);
        let mut downloaded: u64 = 0;

        // Create temp file
        let temp_path = target_path.with_extension("download");
        let mut file = tokio::fs::File::create(&temp_path).await?;
        let mut hasher = Context::new(&SHA256);

        // Download with progress
        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| UpdateError::DownloadFailed(e.to_string()))?;

            file.write_all(&chunk).await?;
            hasher.update(&chunk);

            downloaded += chunk.len() as u64;

            if let Some(ref callback) = progress_callback {
                let percent = ((downloaded as f64 / total_size as f64) * 100.0) as u8;
                callback(UpdateProgress {
                    stage: UpdateStage::Downloading,
                    percent,
                    bytes_downloaded: Some(downloaded),
                    total_bytes: Some(total_size),
                    message: None,
                });
            }
        }

        file.flush().await?;
        drop(file);

        // Verify checksum
        if let Some(ref callback) = progress_callback {
            callback(UpdateProgress {
                stage: UpdateStage::Verifying,
                percent: 0,
                bytes_downloaded: None,
                total_bytes: None,
                message: Some("Verifying checksum...".to_string()),
            });
        }

        let digest = hasher.finish();
        let calculated_hash = hex::encode(digest.as_ref());

        if calculated_hash != binary_info.sha256.to_lowercase() {
            tokio::fs::remove_file(&temp_path).await.ok();
            return Err(UpdateError::ChecksumMismatch);
        }

        if let Some(ref callback) = progress_callback {
            callback(UpdateProgress {
                stage: UpdateStage::Verifying,
                percent: 100,
                bytes_downloaded: None,
                total_bytes: None,
                message: Some("Checksum verified".to_string()),
            });
        }

        // Rename to final path
        tokio::fs::rename(&temp_path, target_path).await?;

        Ok(target_path.to_path_buf())
    }

    /// Get the download directory.
    pub fn download_dir() -> PathBuf {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("drbot")
            .join("updates")
    }

    /// Clean up old downloads.
    pub async fn cleanup_old_downloads() -> Result<()> {
        let dir = Self::download_dir();
        if !dir.exists() {
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "download").unwrap_or(false) {
                tokio::fs::remove_file(&path).await.ok();
            }
        }

        Ok(())
    }
}

impl Default for UpdateDownloader {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate SHA256 hash of a file.
pub async fn hash_file(path: &Path) -> Result<String> {
    let content = tokio::fs::read(path).await?;
    let digest = ring::digest::digest(&SHA256, &content);
    Ok(hex::encode(digest.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_dir() {
        let dir = UpdateDownloader::download_dir();
        assert!(dir.ends_with("updates"));
    }

    #[tokio::test]
    async fn test_cleanup_old_downloads() {
        // Should not fail even if directory doesn't exist
        let result = UpdateDownloader::cleanup_old_downloads().await;
        assert!(result.is_ok());
    }
}
