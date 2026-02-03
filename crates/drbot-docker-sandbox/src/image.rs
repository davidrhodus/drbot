//! Docker image management.

use crate::{Result, SandboxError};
use bollard::image::{CreateImageOptions, ListImagesOptions};
use bollard::Docker;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Image configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    /// Image name (e.g., "python:3.11-slim").
    pub name: String,
    /// Aliases for this image.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Whether to auto-pull if not present.
    #[serde(default = "default_auto_pull")]
    pub auto_pull: bool,
    /// Pull timeout in seconds.
    #[serde(default = "default_pull_timeout")]
    pub pull_timeout_secs: u64,
}

fn default_auto_pull() -> bool {
    true
}

fn default_pull_timeout() -> u64 {
    300 // 5 minutes
}

impl ImageConfig {
    /// Create a new image config.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            aliases: Vec::new(),
            auto_pull: default_auto_pull(),
            pull_timeout_secs: default_pull_timeout(),
        }
    }

    /// Add an alias.
    pub fn with_alias(mut self, alias: &str) -> Self {
        self.aliases.push(alias.to_string());
        self
    }
}

/// Progress information for image pulling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePullProgress {
    /// Image being pulled.
    pub image: String,
    /// Current status.
    pub status: String,
    /// Progress percentage (0-100, if available).
    pub progress_percent: Option<u8>,
    /// Downloaded bytes.
    pub downloaded_bytes: Option<u64>,
    /// Total bytes.
    pub total_bytes: Option<u64>,
    /// Whether the pull is complete.
    pub complete: bool,
    /// Error message (if failed).
    pub error: Option<String>,
}

/// Manages Docker images for sandboxing.
pub struct ImageManager {
    /// Docker client.
    docker: Arc<Docker>,
    /// Available images.
    available: Arc<RwLock<HashMap<String, bool>>>,
    /// Image configs.
    configs: Arc<RwLock<HashMap<String, ImageConfig>>>,
}

impl ImageManager {
    /// Create a new image manager.
    pub fn new(docker: Arc<Docker>) -> Self {
        Self {
            docker,
            available: Arc::new(RwLock::new(HashMap::new())),
            configs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register an image config.
    pub async fn register(&self, config: ImageConfig) {
        let mut configs = self.configs.write().await;
        configs.insert(config.name.clone(), config);
    }

    /// Check if an image is available locally.
    pub async fn is_available(&self, image: &str) -> Result<bool> {
        // Check cache first
        {
            let available = self.available.read().await;
            if let Some(&is_available) = available.get(image) {
                return Ok(is_available);
            }
        }

        // Query Docker
        let options = ListImagesOptions::<String> {
            all: false,
            ..Default::default()
        };

        let images = self
            .docker
            .list_images(Some(options))
            .await
            .map_err(|e| SandboxError::DockerError(e.to_string()))?;

        let is_available = images.iter().any(|img| {
            // repo_tags is Vec<String> in current bollard version
            img.repo_tags
                .iter()
                .any(|tag: &String| tag == image || tag.starts_with(&format!("{}:", image)))
        });

        // Update cache
        {
            let mut available = self.available.write().await;
            available.insert(image.to_string(), is_available);
        }

        Ok(is_available)
    }

    /// Pull an image.
    pub async fn pull(&self, image: &str) -> Result<()> {
        let (image_name, tag) = parse_image_tag(image);

        let options = CreateImageOptions {
            from_image: image_name.to_string(),
            tag: tag.to_string(),
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(options), None, None);

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(error) = info.error {
                        return Err(SandboxError::ImagePullFailed(error));
                    }
                    // Could report progress here via callback
                    tracing::debug!(
                        status = ?info.status,
                        progress = ?info.progress,
                        "Image pull progress"
                    );
                }
                Err(e) => {
                    return Err(SandboxError::ImagePullFailed(e.to_string()));
                }
            }
        }

        // Update cache
        {
            let mut available = self.available.write().await;
            available.insert(image.to_string(), true);
        }

        Ok(())
    }

    /// Pull an image with progress reporting.
    pub async fn pull_with_progress<F>(&self, image: &str, mut progress_callback: F) -> Result<()>
    where
        F: FnMut(ImagePullProgress),
    {
        let (image_name, tag) = parse_image_tag(image);

        let options = CreateImageOptions {
            from_image: image_name.to_string(),
            tag: tag.to_string(),
            ..Default::default()
        };

        let mut stream = self.docker.create_image(Some(options), None, None);
        let mut layer_progress: HashMap<String, (u64, u64)> = HashMap::new();

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(error) = info.error {
                        progress_callback(ImagePullProgress {
                            image: image.to_string(),
                            status: "failed".to_string(),
                            progress_percent: None,
                            downloaded_bytes: None,
                            total_bytes: None,
                            complete: true,
                            error: Some(error.clone()),
                        });
                        return Err(SandboxError::ImagePullFailed(error));
                    }

                    // Track per-layer progress
                    if let (Some(id), Some(detail)) = (&info.id, &info.progress_detail) {
                        if let (Some(current), Some(total)) = (detail.current, detail.total) {
                            layer_progress.insert(id.clone(), (current as u64, total as u64));
                        }
                    }

                    // Calculate overall progress
                    let (downloaded, total): (u64, u64) = layer_progress
                        .values()
                        .fold((0, 0), |(d, t), (cd, ct)| (d + cd, t + ct));

                    let progress_percent = if total > 0 {
                        Some(((downloaded as f64 / total as f64) * 100.0) as u8)
                    } else {
                        None
                    };

                    progress_callback(ImagePullProgress {
                        image: image.to_string(),
                        status: info.status.unwrap_or_default(),
                        progress_percent,
                        downloaded_bytes: Some(downloaded),
                        total_bytes: Some(total),
                        complete: false,
                        error: None,
                    });
                }
                Err(e) => {
                    progress_callback(ImagePullProgress {
                        image: image.to_string(),
                        status: "failed".to_string(),
                        progress_percent: None,
                        downloaded_bytes: None,
                        total_bytes: None,
                        complete: true,
                        error: Some(e.to_string()),
                    });
                    return Err(SandboxError::ImagePullFailed(e.to_string()));
                }
            }
        }

        // Report completion
        progress_callback(ImagePullProgress {
            image: image.to_string(),
            status: "complete".to_string(),
            progress_percent: Some(100),
            downloaded_bytes: None,
            total_bytes: None,
            complete: true,
            error: None,
        });

        // Update cache
        {
            let mut available = self.available.write().await;
            available.insert(image.to_string(), true);
        }

        Ok(())
    }

    /// Ensure an image is available, pulling if necessary.
    pub async fn ensure_available(&self, image: &str) -> Result<()> {
        if !self.is_available(image).await? {
            tracing::info!(image = %image, "Pulling image");
            self.pull(image).await?;
        }
        Ok(())
    }

    /// Invalidate the cache for an image.
    pub async fn invalidate_cache(&self, image: &str) {
        let mut available = self.available.write().await;
        available.remove(image);
    }

    /// Clear all cache.
    pub async fn clear_cache(&self) {
        let mut available = self.available.write().await;
        available.clear();
    }

    /// List all locally available images.
    pub async fn list_local(&self) -> Result<Vec<String>> {
        let options = ListImagesOptions::<String> {
            all: false,
            ..Default::default()
        };

        let images = self
            .docker
            .list_images(Some(options))
            .await
            .map_err(|e| SandboxError::DockerError(e.to_string()))?;

        let names: Vec<String> = images
            .into_iter()
            .flat_map(|image| image.repo_tags)
            .filter(|tag: &String| !tag.is_empty() && tag != "<none>:<none>")
            .collect();

        Ok(names)
    }
}

/// Parse an image string into name and tag.
fn parse_image_tag(image: &str) -> (&str, &str) {
    if let Some(idx) = image.rfind(':') {
        // Check if this colon is part of a port number (e.g., registry:5000/image)
        let after_colon = &image[idx + 1..];
        if !after_colon.contains('/') {
            return (&image[..idx], after_colon);
        }
    }
    (image, "latest")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_image_tag() {
        assert_eq!(parse_image_tag("python:3.11"), ("python", "3.11"));
        assert_eq!(parse_image_tag("python"), ("python", "latest"));
        assert_eq!(
            parse_image_tag("registry:5000/image:tag"),
            ("registry:5000/image", "tag")
        );
        assert_eq!(
            parse_image_tag("registry:5000/image"),
            ("registry:5000/image", "latest")
        );
    }

    #[test]
    fn test_image_config() {
        let config = ImageConfig::new("python:3.11").with_alias("py3");
        assert_eq!(config.name, "python:3.11");
        assert_eq!(config.aliases, vec!["py3"]);
    }
}
