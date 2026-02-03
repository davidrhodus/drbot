//! Service connectors for drbot.
//!
//! Pre-built integrations for popular services.
//!
//! # Features
//!
//! - GitHub integration
//! - Slack integration
//! - Notion integration
//! - Linear integration
//! - Jira integration

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Connector result type.
pub type Result<T> = std::result::Result<T, ConnectorError>;

/// Connector errors.
#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
    #[error("API error: {0}")]
    ApiError(String),
    #[error("Rate limited")]
    RateLimited,
    #[error("Connector not found: {0}")]
    NotFound(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

/// Service type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    GitHub,
    GitLab,
    Slack,
    Discord,
    Notion,
    Linear,
    Jira,
    Asana,
    Trello,
    GoogleDrive,
    Dropbox,
    Custom,
}

/// Connector status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorStatus {
    Connected,
    Disconnected,
    Error,
    RateLimited,
    Expired,
}

/// Connector configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    /// Connector ID.
    pub id: Uuid,
    /// Service type.
    pub service: ServiceType,
    /// Display name.
    pub name: String,
    /// API key or token.
    pub api_key: Option<String>,
    /// OAuth token.
    pub oauth_token: Option<String>,
    /// Refresh token.
    pub refresh_token: Option<String>,
    /// Base URL (for self-hosted).
    pub base_url: Option<String>,
    /// Workspace/org ID.
    pub workspace_id: Option<String>,
    /// Additional settings.
    pub settings: HashMap<String, String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl ConnectorConfig {
    /// Create a new connector config.
    pub fn new(service: ServiceType, name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            service,
            name: name.to_string(),
            api_key: None,
            oauth_token: None,
            refresh_token: None,
            base_url: None,
            workspace_id: None,
            settings: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Set API key.
    pub fn with_api_key(mut self, key: &str) -> Self {
        self.api_key = Some(key.to_string());
        self
    }

    /// Set OAuth token.
    pub fn with_oauth(mut self, token: &str, refresh: Option<&str>) -> Self {
        self.oauth_token = Some(token.to_string());
        self.refresh_token = refresh.map(|s| s.to_string());
        self
    }
}

/// Connector instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorInstance {
    /// Config.
    pub config: ConnectorConfig,
    /// Status.
    pub status: ConnectorStatus,
    /// Last sync.
    pub last_sync: Option<DateTime<Utc>>,
    /// Error message.
    pub error: Option<String>,
}

/// Generic item from a service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceItem {
    /// Item ID (from service).
    pub id: String,
    /// Item type.
    pub item_type: String,
    /// Title.
    pub title: String,
    /// Description.
    pub description: Option<String>,
    /// URL.
    pub url: Option<String>,
    /// Status.
    pub status: Option<String>,
    /// Created at.
    pub created_at: Option<DateTime<Utc>>,
    /// Updated at.
    pub updated_at: Option<DateTime<Utc>>,
    /// Author.
    pub author: Option<String>,
    /// Labels/tags.
    pub labels: Vec<String>,
    /// Raw data.
    pub raw: serde_json::Value,
}

/// Query for service items.
#[derive(Debug, Clone, Default)]
pub struct ServiceQuery {
    /// Item type filter.
    pub item_type: Option<String>,
    /// Status filter.
    pub status: Option<String>,
    /// Author filter.
    pub author: Option<String>,
    /// Search query.
    pub search: Option<String>,
    /// Limit.
    pub limit: Option<usize>,
    /// Since date.
    pub since: Option<DateTime<Utc>>,
}

/// Trait for service connectors.
#[async_trait]
pub trait ServiceConnector: Send + Sync {
    /// Get service type.
    fn service_type(&self) -> ServiceType;

    /// Test connection.
    async fn test_connection(&self) -> Result<()>;

    /// List items.
    async fn list(&self, query: &ServiceQuery) -> Result<Vec<ServiceItem>>;

    /// Get item by ID.
    async fn get(&self, id: &str) -> Result<ServiceItem>;

    /// Create item.
    async fn create(&self, item: &ServiceItem) -> Result<ServiceItem>;

    /// Update item.
    async fn update(&self, id: &str, item: &ServiceItem) -> Result<ServiceItem>;

    /// Delete item.
    async fn delete(&self, id: &str) -> Result<()>;

    /// Search.
    async fn search(&self, query: &str) -> Result<Vec<ServiceItem>>;
}

/// Connector manager.
pub struct ConnectorManager {
    connectors: Arc<RwLock<HashMap<Uuid, ConnectorInstance>>>,
    handlers: Arc<RwLock<HashMap<Uuid, Arc<dyn ServiceConnector>>>>,
}

impl ConnectorManager {
    /// Create a new connector manager.
    pub fn new() -> Self {
        Self {
            connectors: Arc::new(RwLock::new(HashMap::new())),
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a connector.
    pub async fn register(
        &self,
        config: ConnectorConfig,
        handler: Arc<dyn ServiceConnector>,
    ) -> Result<Uuid> {
        let id = config.id;

        // Test connection
        handler.test_connection().await?;

        let instance = ConnectorInstance {
            config,
            status: ConnectorStatus::Connected,
            last_sync: None,
            error: None,
        };

        self.connectors.write().await.insert(id, instance);
        self.handlers.write().await.insert(id, handler);

        Ok(id)
    }

    /// Get connector.
    pub async fn get(&self, id: Uuid) -> Option<ConnectorInstance> {
        self.connectors.read().await.get(&id).cloned()
    }

    /// List connectors.
    pub async fn list(&self) -> Vec<ConnectorInstance> {
        self.connectors.read().await.values().cloned().collect()
    }

    /// List by service type.
    pub async fn list_by_service(&self, service: ServiceType) -> Vec<ConnectorInstance> {
        self.connectors
            .read()
            .await
            .values()
            .filter(|c| c.config.service == service)
            .cloned()
            .collect()
    }

    /// Execute query on connector.
    pub async fn query(
        &self,
        connector_id: Uuid,
        query: &ServiceQuery,
    ) -> Result<Vec<ServiceItem>> {
        let handler = self
            .handlers
            .read()
            .await
            .get(&connector_id)
            .cloned()
            .ok_or(ConnectorError::NotFound(connector_id.to_string()))?;

        handler.list(query).await
    }

    /// Search across all connectors.
    pub async fn search_all(&self, query: &str) -> Vec<(Uuid, Vec<ServiceItem>)> {
        let handlers = self.handlers.read().await;
        let mut results = Vec::new();

        for (id, handler) in handlers.iter() {
            if let Ok(items) = handler.search(query).await {
                if !items.is_empty() {
                    results.push((*id, items));
                }
            }
        }

        results
    }

    /// Disconnect a connector.
    pub async fn disconnect(&self, id: Uuid) -> Result<()> {
        if let Some(mut instance) = self.connectors.write().await.get_mut(&id) {
            instance.status = ConnectorStatus::Disconnected;
        }
        self.handlers.write().await.remove(&id);
        Ok(())
    }

    /// Get statistics.
    pub async fn stats(&self) -> ConnectorStats {
        let connectors = self.connectors.read().await;

        let connected = connectors
            .values()
            .filter(|c| c.status == ConnectorStatus::Connected)
            .count();

        let mut by_service: HashMap<ServiceType, usize> = HashMap::new();
        for conn in connectors.values() {
            *by_service.entry(conn.config.service).or_insert(0) += 1;
        }

        ConnectorStats {
            total: connectors.len(),
            connected,
            by_service,
        }
    }
}

impl Default for ConnectorManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Connector statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorStats {
    pub total: usize,
    pub connected: usize,
    pub by_service: HashMap<ServiceType, usize>,
}

/// Mock connector for testing.
pub struct MockConnector {
    service: ServiceType,
    items: Arc<RwLock<Vec<ServiceItem>>>,
}

impl MockConnector {
    pub fn new(service: ServiceType) -> Self {
        Self {
            service,
            items: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_item(&self, item: ServiceItem) {
        self.items.write().await.push(item);
    }
}

#[async_trait]
impl ServiceConnector for MockConnector {
    fn service_type(&self) -> ServiceType {
        self.service
    }

    async fn test_connection(&self) -> Result<()> {
        Ok(())
    }

    async fn list(&self, query: &ServiceQuery) -> Result<Vec<ServiceItem>> {
        let items = self.items.read().await;
        let mut result: Vec<_> = items.iter().cloned().collect();

        if let Some(item_type) = &query.item_type {
            result.retain(|i| &i.item_type == item_type);
        }

        if let Some(limit) = query.limit {
            result.truncate(limit);
        }

        Ok(result)
    }

    async fn get(&self, id: &str) -> Result<ServiceItem> {
        self.items
            .read()
            .await
            .iter()
            .find(|i| i.id == id)
            .cloned()
            .ok_or(ConnectorError::NotFound(id.to_string()))
    }

    async fn create(&self, item: &ServiceItem) -> Result<ServiceItem> {
        let mut new_item = item.clone();
        new_item.created_at = Some(Utc::now());
        self.items.write().await.push(new_item.clone());
        Ok(new_item)
    }

    async fn update(&self, id: &str, item: &ServiceItem) -> Result<ServiceItem> {
        let mut items = self.items.write().await;
        if let Some(existing) = items.iter_mut().find(|i| i.id == id) {
            existing.title = item.title.clone();
            existing.description = item.description.clone();
            existing.status = item.status.clone();
            existing.updated_at = Some(Utc::now());
            return Ok(existing.clone());
        }
        Err(ConnectorError::NotFound(id.to_string()))
    }

    async fn delete(&self, id: &str) -> Result<()> {
        self.items.write().await.retain(|i| i.id != id);
        Ok(())
    }

    async fn search(&self, query: &str) -> Result<Vec<ServiceItem>> {
        let query_lower = query.to_lowercase();
        Ok(self
            .items
            .read()
            .await
            .iter()
            .filter(|i| {
                i.title.to_lowercase().contains(&query_lower)
                    || i.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_connector() {
        let manager = ConnectorManager::new();
        let config = ConnectorConfig::new(ServiceType::GitHub, "My GitHub");
        let handler = Arc::new(MockConnector::new(ServiceType::GitHub));

        let id = manager.register(config, handler).await.unwrap();
        let connector = manager.get(id).await.unwrap();
        assert_eq!(connector.config.service, ServiceType::GitHub);
    }

    #[tokio::test]
    async fn test_query_connector() {
        let manager = ConnectorManager::new();
        let config = ConnectorConfig::new(ServiceType::GitHub, "Test");
        let handler = Arc::new(MockConnector::new(ServiceType::GitHub));

        handler
            .add_item(ServiceItem {
                id: "1".to_string(),
                item_type: "issue".to_string(),
                title: "Bug fix".to_string(),
                description: None,
                url: None,
                status: Some("open".to_string()),
                created_at: None,
                updated_at: None,
                author: None,
                labels: Vec::new(),
                raw: serde_json::Value::Null,
            })
            .await;

        let id = manager.register(config, handler).await.unwrap();
        let results = manager.query(id, &ServiceQuery::default()).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_search_all() {
        let manager = ConnectorManager::new();

        let github = Arc::new(MockConnector::new(ServiceType::GitHub));
        github
            .add_item(ServiceItem {
                id: "1".to_string(),
                item_type: "issue".to_string(),
                title: "Authentication bug".to_string(),
                description: None,
                url: None,
                status: None,
                created_at: None,
                updated_at: None,
                author: None,
                labels: Vec::new(),
                raw: serde_json::Value::Null,
            })
            .await;

        let slack = Arc::new(MockConnector::new(ServiceType::Slack));
        slack
            .add_item(ServiceItem {
                id: "2".to_string(),
                item_type: "message".to_string(),
                title: "Discussion about bugs".to_string(),
                description: None,
                url: None,
                status: None,
                created_at: None,
                updated_at: None,
                author: None,
                labels: Vec::new(),
                raw: serde_json::Value::Null,
            })
            .await;

        manager
            .register(ConnectorConfig::new(ServiceType::GitHub, "GH"), github)
            .await
            .unwrap();
        manager
            .register(ConnectorConfig::new(ServiceType::Slack, "Slack"), slack)
            .await
            .unwrap();

        let results = manager.search_all("bug").await;
        assert_eq!(results.len(), 2);
    }
}
