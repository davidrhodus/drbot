//! Marketplace registry for agents and plugins.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{Creator, MarketplaceError, Result};

/// Registry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    /// Registry URL.
    pub url: String,
    /// Cache TTL in seconds.
    pub cache_ttl_secs: u64,
    /// Enable caching.
    pub enable_cache: bool,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: "https://marketplace.drbot.dev/api".to_string(),
            cache_ttl_secs: 3600,
            enable_cache: true,
        }
    }
}

/// Marketplace item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceItem {
    /// Item ID.
    pub id: Uuid,
    /// Item slug (unique identifier).
    pub slug: String,
    /// Item name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Item type.
    pub item_type: ItemType,
    /// Creator.
    pub creator: Creator,
    /// Latest version.
    pub version: String,
    /// Download count.
    pub downloads: u64,
    /// Average rating.
    pub rating: f32,
    /// Review count.
    pub review_count: u32,
    /// Tags.
    pub tags: Vec<String>,
    /// Icon URL.
    pub icon_url: Option<String>,
    /// Screenshots.
    pub screenshots: Vec<String>,
    /// Repository URL.
    pub repository_url: Option<String>,
    /// Documentation URL.
    pub docs_url: Option<String>,
    /// License.
    pub license: String,
    /// Status.
    pub status: ItemStatus,
    /// Published at.
    pub published_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
}

/// Item type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    /// AI Agent.
    Agent,
    /// Plugin/Extension.
    Plugin,
    /// Workflow template.
    Workflow,
    /// Prompt template.
    Prompt,
    /// Integration connector.
    Integration,
    /// Theme.
    Theme,
}

/// Item status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    /// Published and available.
    Published,
    /// Under review.
    PendingReview,
    /// Draft.
    Draft,
    /// Deprecated.
    Deprecated,
    /// Removed.
    Removed,
}

/// Search filters.
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    /// Item type filter.
    pub item_type: Option<ItemType>,
    /// Tags filter.
    pub tags: Option<Vec<String>>,
    /// Creator filter.
    pub creator_id: Option<String>,
    /// Minimum rating.
    pub min_rating: Option<f32>,
    /// Only verified creators.
    pub verified_only: bool,
}

/// Search sort options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchSort {
    /// By relevance.
    #[default]
    Relevance,
    /// Most downloaded.
    Downloads,
    /// Highest rated.
    Rating,
    /// Most recent.
    Recent,
    /// Recently updated.
    Updated,
}

/// Marketplace registry.
pub struct Registry {
    config: RegistryConfig,
    cache: Arc<RwLock<HashMap<String, CachedItem>>>,
    installed: Arc<RwLock<HashMap<Uuid, crate::InstalledItem>>>,
}

#[derive(Clone)]
struct CachedItem {
    item: MarketplaceItem,
    cached_at: DateTime<Utc>,
}

impl Registry {
    /// Create a new registry.
    pub fn new(config: RegistryConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            installed: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Search the marketplace.
    pub async fn search(
        &self,
        query: &str,
        filters: SearchFilters,
        sort: SearchSort,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<MarketplaceItem>> {
        // In a real implementation, this would call the registry API
        // For now, return empty results
        Ok(Vec::new())
    }

    /// Get item by slug.
    pub async fn get_by_slug(&self, slug: &str) -> Result<MarketplaceItem> {
        // Check cache first
        if self.config.enable_cache {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(slug) {
                let age = Utc::now()
                    .signed_duration_since(cached.cached_at)
                    .num_seconds() as u64;
                if age < self.config.cache_ttl_secs {
                    return Ok(cached.item.clone());
                }
            }
        }

        // Would fetch from API
        Err(MarketplaceError::NotFound(slug.to_string()))
    }

    /// Get item by ID.
    pub async fn get(&self, id: Uuid) -> Result<MarketplaceItem> {
        // Would fetch from API
        Err(MarketplaceError::NotFound(id.to_string()))
    }

    /// Get featured items.
    pub async fn featured(&self) -> Result<Vec<MarketplaceItem>> {
        Ok(Vec::new())
    }

    /// Get trending items.
    pub async fn trending(&self, limit: usize) -> Result<Vec<MarketplaceItem>> {
        Ok(Vec::new())
    }

    /// Get items by category/type.
    pub async fn by_type(&self, item_type: ItemType, limit: usize) -> Result<Vec<MarketplaceItem>> {
        Ok(Vec::new())
    }

    /// Install an item.
    pub async fn install(&self, id: Uuid) -> Result<crate::InstalledItem> {
        let item = self.get(id).await?;

        // Download and install
        let installed = crate::InstalledItem {
            id: item.id,
            name: item.name.clone(),
            item_type: item.item_type,
            version: item.version.clone(),
            latest_version: Some(item.version.clone()),
            installed_at: Utc::now(),
            updated_at: Utc::now(),
            enabled: true,
            config: serde_json::Value::Object(serde_json::Map::new()),
        };

        let mut installed_items = self.installed.write().await;
        installed_items.insert(id, installed.clone());

        Ok(installed)
    }

    /// Uninstall an item.
    pub async fn uninstall(&self, id: Uuid) -> Result<()> {
        let mut installed = self.installed.write().await;
        installed.remove(&id);
        Ok(())
    }

    /// Update an item.
    pub async fn update(&self, id: Uuid) -> Result<crate::InstalledItem> {
        let mut installed = self.installed.write().await;
        if let Some(item) = installed.get_mut(&id) {
            item.updated_at = Utc::now();
            if let Some(latest) = &item.latest_version {
                item.version = latest.clone();
            }
            Ok(item.clone())
        } else {
            Err(MarketplaceError::NotFound(id.to_string()))
        }
    }

    /// Get installed items.
    pub async fn installed(&self) -> Vec<crate::InstalledItem> {
        self.installed.read().await.values().cloned().collect()
    }

    /// Check for updates.
    pub async fn check_updates(&self) -> Vec<(Uuid, String, String)> {
        let installed = self.installed.read().await;
        let mut updates = Vec::new();

        for item in installed.values() {
            if let Some(latest) = &item.latest_version {
                if latest != &item.version {
                    updates.push((item.id, item.version.clone(), latest.clone()));
                }
            }
        }

        updates
    }

    /// Enable/disable an item.
    pub async fn set_enabled(&self, id: Uuid, enabled: bool) -> Result<()> {
        let mut installed = self.installed.write().await;
        if let Some(item) = installed.get_mut(&id) {
            item.enabled = enabled;
            Ok(())
        } else {
            Err(MarketplaceError::NotFound(id.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry() {
        let config = RegistryConfig::default();
        let registry = Registry::new(config);

        let results = registry
            .search(
                "test",
                SearchFilters::default(),
                SearchSort::default(),
                10,
                0,
            )
            .await
            .unwrap();
        assert!(results.is_empty()); // No real API

        let installed = registry.installed().await;
        assert!(installed.is_empty());
    }
}
