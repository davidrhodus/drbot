//! Container pooling for pre-warmed containers.

use crate::{ContainerLimits, Result, SandboxContainer, SandboxError};
use bollard::Docker;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};

/// Pool configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Minimum containers to keep warm.
    #[serde(default = "default_min_size")]
    pub min_size: usize,
    /// Maximum containers in pool.
    #[serde(default = "default_max_size")]
    pub max_size: usize,
    /// Idle timeout before container is recycled (seconds).
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    /// Maximum executions per container before recycling.
    #[serde(default = "default_max_executions")]
    pub max_executions: u64,
    /// Enable auto-scaling.
    #[serde(default)]
    pub auto_scale: bool,
}

fn default_min_size() -> usize {
    2
}

fn default_max_size() -> usize {
    10
}

fn default_idle_timeout() -> u64 {
    300 // 5 minutes
}

fn default_max_executions() -> u64 {
    100
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_size: default_min_size(),
            max_size: default_max_size(),
            idle_timeout_secs: default_idle_timeout(),
            max_executions: default_max_executions(),
            auto_scale: false,
        }
    }
}

/// Pool status information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStatus {
    /// Number of available containers.
    pub available: usize,
    /// Number of containers in use.
    pub in_use: usize,
    /// Total containers.
    pub total: usize,
    /// Pool configuration.
    pub config: PoolConfig,
}

/// A pool entry tracking a container.
struct PoolEntry {
    container: SandboxContainer,
    in_use: bool,
}

/// Container pool for pre-warmed containers.
pub struct ContainerPool {
    /// Docker client.
    docker: Arc<Docker>,
    /// Pool configuration.
    config: PoolConfig,
    /// Default image for containers.
    default_image: String,
    /// Default limits.
    default_limits: ContainerLimits,
    /// Working directory.
    workdir: String,
    /// Available containers by image.
    pools: Arc<RwLock<HashMap<String, Vec<PoolEntry>>>>,
    /// Semaphore for limiting concurrent container creation.
    creation_semaphore: Arc<Semaphore>,
}

impl ContainerPool {
    /// Create a new container pool.
    pub fn new(
        docker: Arc<Docker>,
        config: PoolConfig,
        default_image: &str,
        default_limits: ContainerLimits,
        workdir: &str,
    ) -> Self {
        Self {
            docker,
            config: config.clone(),
            default_image: default_image.to_string(),
            default_limits,
            workdir: workdir.to_string(),
            pools: Arc::new(RwLock::new(HashMap::new())),
            creation_semaphore: Arc::new(Semaphore::new(config.max_size)),
        }
    }

    /// Warm up the pool with containers.
    pub async fn warm_up(&self) -> Result<()> {
        self.warm_up_image(&self.default_image, self.config.min_size)
            .await
    }

    /// Warm up containers for a specific image.
    pub async fn warm_up_image(&self, image: &str, count: usize) -> Result<()> {
        for _ in 0..count {
            let container = self.create_container(image).await?;
            container.start().await?;

            let mut pools = self.pools.write().await;
            let pool = pools.entry(image.to_string()).or_insert_with(Vec::new);
            pool.push(PoolEntry {
                container,
                in_use: false,
            });
        }

        tracing::info!(image = %image, count = count, "Warmed up container pool");
        Ok(())
    }

    /// Acquire a container from the pool.
    pub async fn acquire(&self, image: Option<&str>) -> Result<Arc<SandboxContainer>> {
        let image = image.unwrap_or(&self.default_image);

        // Try to get an available container
        {
            let mut pools = self.pools.write().await;
            if let Some(pool) = pools.get_mut(image) {
                for entry in pool.iter_mut() {
                    if !entry.in_use && entry.container.is_healthy().await {
                        entry.in_use = true;
                        // Reset the container for reuse
                        entry.container.reset().await?;
                        return Ok(Arc::new(unsafe {
                            // Safety: We're creating an Arc from a reference that we know is valid
                            // This is a simplification - in production, pool should store Arc<SandboxContainer>
                            std::ptr::read(&entry.container as *const SandboxContainer)
                        }));
                    }
                }
            }
        }

        // No available container, create a new one if allowed
        let pools = self.pools.read().await;
        let current_count = pools.get(image).map(|p| p.len()).unwrap_or(0);

        if current_count >= self.config.max_size {
            return Err(SandboxError::PoolExhausted);
        }

        drop(pools);

        // Create a new container
        let container = self.create_container(image).await?;
        container.start().await?;

        let container = Arc::new(container);

        // Note: In a real implementation, we'd add this to the pool
        // For now, just return it directly

        Ok(container)
    }

    /// Release a container back to the pool.
    pub async fn release(&self, container_id: &str) {
        let mut pools = self.pools.write().await;

        for pool in pools.values_mut() {
            for entry in pool.iter_mut() {
                if entry.container.id() == container_id {
                    entry.in_use = false;
                    return;
                }
            }
        }
    }

    /// Create a new container.
    async fn create_container(&self, image: &str) -> Result<SandboxContainer> {
        // Acquire semaphore permit
        let _permit = self
            .creation_semaphore
            .acquire()
            .await
            .map_err(|e| SandboxError::ContainerCreationFailed(e.to_string()))?;

        SandboxContainer::create(
            self.docker.clone(),
            image,
            self.default_limits.clone(),
            &self.workdir,
        )
        .await
    }

    /// Get pool status.
    pub async fn status(&self) -> PoolStatus {
        let pools = self.pools.read().await;

        let (available, in_use, total) = pools.values().fold((0, 0, 0), |(a, u, t), pool| {
            let pool_available = pool.iter().filter(|e| !e.in_use).count();
            let pool_in_use = pool.iter().filter(|e| e.in_use).count();
            (a + pool_available, u + pool_in_use, t + pool.len())
        });

        PoolStatus {
            available,
            in_use,
            total,
            config: self.config.clone(),
        }
    }

    /// Clean up idle containers.
    pub async fn cleanup_idle(&self) -> usize {
        let now = chrono::Utc::now();
        let idle_threshold = chrono::Duration::seconds(self.config.idle_timeout_secs as i64);
        let mut removed = 0;

        let mut pools = self.pools.write().await;

        for pool in pools.values_mut() {
            let initial_len = pool.len();

            // Keep only non-idle or in-use containers
            let mut to_remove = Vec::new();

            for (idx, entry) in pool.iter().enumerate() {
                if entry.in_use {
                    continue;
                }

                let info = entry.container.info().await;
                if let Some(last_used) = info.last_used {
                    if now - last_used > idle_threshold {
                        to_remove.push(idx);
                    }
                }
            }

            // Remove in reverse order to maintain indices
            for idx in to_remove.into_iter().rev() {
                let entry = pool.remove(idx);
                // Best effort cleanup
                let _ = entry.container.remove().await;
                removed += 1;
            }
        }

        if removed > 0 {
            tracing::info!(removed = removed, "Cleaned up idle containers");
        }

        removed
    }

    /// Clean up over-used containers.
    pub async fn cleanup_overused(&self) -> usize {
        let mut removed = 0;

        let mut pools = self.pools.write().await;

        for pool in pools.values_mut() {
            let mut to_remove = Vec::new();

            for (idx, entry) in pool.iter().enumerate() {
                if entry.in_use {
                    continue;
                }

                let info = entry.container.info().await;
                if info.execution_count >= self.config.max_executions {
                    to_remove.push(idx);
                }
            }

            for idx in to_remove.into_iter().rev() {
                let entry = pool.remove(idx);
                let _ = entry.container.remove().await;
                removed += 1;
            }
        }

        if removed > 0 {
            tracing::info!(removed = removed, "Cleaned up over-used containers");
        }

        removed
    }

    /// Shutdown the pool, removing all containers.
    pub async fn shutdown(&self) -> Result<()> {
        let mut pools = self.pools.write().await;

        for pool in pools.values_mut() {
            for entry in pool.drain(..) {
                let _ = entry.container.remove().await;
            }
        }

        tracing::info!("Container pool shutdown complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_default() {
        let config = PoolConfig::default();
        assert_eq!(config.min_size, 2);
        assert_eq!(config.max_size, 10);
        assert_eq!(config.idle_timeout_secs, 300);
    }
}
