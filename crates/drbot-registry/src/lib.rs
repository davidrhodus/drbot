//! Service registry for drbot.
//!
//! This crate provides:
//! - Service registration
//! - Service lookup
//! - Health status tracking
//! - Metadata management

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Registry error types.
#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Service already exists: {0}")]
    ServiceExists(String),

    #[error("Instance not found: {0}")]
    InstanceNotFound(String),

    #[error("Registration failed: {0}")]
    RegistrationFailed(String),
}

/// Result type for registry operations.
pub type Result<T> = std::result::Result<T, RegistryError>;

/// Service health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Service is healthy.
    Healthy,
    /// Service is degraded but functional.
    Degraded,
    /// Service is unhealthy.
    Unhealthy,
    /// Health status unknown.
    Unknown,
}

/// A service instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    /// Instance ID.
    pub id: String,
    /// Service name.
    pub service_name: String,
    /// Host address.
    pub host: String,
    /// Port.
    pub port: u16,
    /// Protocol (http, https, grpc, etc.).
    pub protocol: String,
    /// Health status.
    pub status: HealthStatus,
    /// Instance metadata.
    pub metadata: HashMap<String, String>,
    /// Tags for filtering.
    pub tags: Vec<String>,
    /// Weight for load balancing.
    pub weight: u32,
    /// Registered at.
    pub registered_at: DateTime<Utc>,
    /// Last heartbeat.
    pub last_heartbeat: DateTime<Utc>,
    /// Version.
    pub version: Option<String>,
}

impl ServiceInstance {
    /// Create a new instance.
    pub fn new(service_name: impl Into<String>, host: impl Into<String>, port: u16) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            service_name: service_name.into(),
            host: host.into(),
            port,
            protocol: "http".to_string(),
            status: HealthStatus::Unknown,
            metadata: HashMap::new(),
            tags: Vec::new(),
            weight: 100,
            registered_at: now,
            last_heartbeat: now,
            version: None,
        }
    }

    /// Set protocol.
    pub fn with_protocol(mut self, protocol: impl Into<String>) -> Self {
        self.protocol = protocol.into();
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Add tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set weight.
    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    /// Set version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Get endpoint URL.
    pub fn endpoint(&self) -> String {
        format!("{}://{}:{}", self.protocol, self.host, self.port)
    }

    /// Check if instance is healthy.
    pub fn is_healthy(&self) -> bool {
        self.status == HealthStatus::Healthy
    }

    /// Check if heartbeat is stale.
    pub fn is_stale(&self, timeout: Duration) -> bool {
        Utc::now() - self.last_heartbeat > timeout
    }
}

/// Service definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    /// Service name.
    pub name: String,
    /// Service description.
    pub description: Option<String>,
    /// Service metadata.
    pub metadata: HashMap<String, String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

impl Service {
    /// Create a new service.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Set description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Service registry trait.
#[async_trait]
pub trait Registry: Send + Sync {
    /// Register a service.
    async fn register_service(&self, service: Service) -> Result<()>;

    /// Register an instance.
    async fn register_instance(&self, instance: ServiceInstance) -> Result<()>;

    /// Deregister an instance.
    async fn deregister_instance(&self, instance_id: &str) -> Result<()>;

    /// Get instances for a service.
    async fn get_instances(&self, service_name: &str) -> Result<Vec<ServiceInstance>>;

    /// Get healthy instances for a service.
    async fn get_healthy_instances(&self, service_name: &str) -> Result<Vec<ServiceInstance>>;

    /// Update instance heartbeat.
    async fn heartbeat(&self, instance_id: &str) -> Result<()>;

    /// Update instance health.
    async fn update_health(&self, instance_id: &str, status: HealthStatus) -> Result<()>;

    /// List all services.
    async fn list_services(&self) -> Result<Vec<Service>>;

    /// Get service by name.
    async fn get_service(&self, name: &str) -> Result<Option<Service>>;
}

/// In-memory registry implementation.
pub struct InMemoryRegistry {
    services: RwLock<HashMap<String, Service>>,
    instances: RwLock<HashMap<String, ServiceInstance>>,
    heartbeat_timeout: Duration,
}

impl InMemoryRegistry {
    /// Create a new registry.
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
            instances: RwLock::new(HashMap::new()),
            heartbeat_timeout: Duration::seconds(30),
        }
    }

    /// Set heartbeat timeout.
    pub fn with_heartbeat_timeout(mut self, timeout: Duration) -> Self {
        self.heartbeat_timeout = timeout;
        self
    }
}

impl Default for InMemoryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Registry for InMemoryRegistry {
    async fn register_service(&self, service: Service) -> Result<()> {
        let mut services = self.services.write().await;
        services.insert(service.name.clone(), service);
        Ok(())
    }

    async fn register_instance(&self, mut instance: ServiceInstance) -> Result<()> {
        instance.status = HealthStatus::Healthy;
        instance.last_heartbeat = Utc::now();

        // Auto-create service if needed
        {
            let services = self.services.read().await;
            if !services.contains_key(&instance.service_name) {
                drop(services);
                let mut services = self.services.write().await;
                services.insert(
                    instance.service_name.clone(),
                    Service::new(&instance.service_name),
                );
            }
        }

        let mut instances = self.instances.write().await;
        instances.insert(instance.id.clone(), instance);
        Ok(())
    }

    async fn deregister_instance(&self, instance_id: &str) -> Result<()> {
        let mut instances = self.instances.write().await;
        instances.remove(instance_id);
        Ok(())
    }

    async fn get_instances(&self, service_name: &str) -> Result<Vec<ServiceInstance>> {
        let instances = self.instances.read().await;
        let result: Vec<_> = instances
            .values()
            .filter(|i| i.service_name == service_name)
            .cloned()
            .collect();
        Ok(result)
    }

    async fn get_healthy_instances(&self, service_name: &str) -> Result<Vec<ServiceInstance>> {
        let instances = self.instances.read().await;
        let result: Vec<_> = instances
            .values()
            .filter(|i| {
                i.service_name == service_name
                    && i.is_healthy()
                    && !i.is_stale(self.heartbeat_timeout)
            })
            .cloned()
            .collect();
        Ok(result)
    }

    async fn heartbeat(&self, instance_id: &str) -> Result<()> {
        let mut instances = self.instances.write().await;
        if let Some(instance) = instances.get_mut(instance_id) {
            instance.last_heartbeat = Utc::now();
            Ok(())
        } else {
            Err(RegistryError::InstanceNotFound(instance_id.to_string()))
        }
    }

    async fn update_health(&self, instance_id: &str, status: HealthStatus) -> Result<()> {
        let mut instances = self.instances.write().await;
        if let Some(instance) = instances.get_mut(instance_id) {
            instance.status = status;
            Ok(())
        } else {
            Err(RegistryError::InstanceNotFound(instance_id.to_string()))
        }
    }

    async fn list_services(&self) -> Result<Vec<Service>> {
        let services = self.services.read().await;
        Ok(services.values().cloned().collect())
    }

    async fn get_service(&self, name: &str) -> Result<Option<Service>> {
        let services = self.services.read().await;
        Ok(services.get(name).cloned())
    }
}

/// Registry client for service registration.
pub struct RegistryClient<R: Registry> {
    registry: Arc<R>,
    instance_id: Option<String>,
}

impl<R: Registry> RegistryClient<R> {
    /// Create a new client.
    pub fn new(registry: Arc<R>) -> Self {
        Self {
            registry,
            instance_id: None,
        }
    }

    /// Register this instance.
    pub async fn register(&mut self, instance: ServiceInstance) -> Result<()> {
        let id = instance.id.clone();
        self.registry.register_instance(instance).await?;
        self.instance_id = Some(id);
        Ok(())
    }

    /// Send heartbeat.
    pub async fn heartbeat(&self) -> Result<()> {
        if let Some(id) = &self.instance_id {
            self.registry.heartbeat(id).await
        } else {
            Ok(())
        }
    }

    /// Deregister.
    pub async fn deregister(&mut self) -> Result<()> {
        if let Some(id) = self.instance_id.take() {
            self.registry.deregister_instance(&id).await
        } else {
            Ok(())
        }
    }

    /// Discover instances.
    pub async fn discover(&self, service_name: &str) -> Result<Vec<ServiceInstance>> {
        self.registry.get_healthy_instances(service_name).await
    }
}

/// Service query with filters.
#[derive(Debug, Clone, Default)]
pub struct ServiceQuery {
    /// Service name.
    pub service_name: String,
    /// Required tags.
    pub tags: Vec<String>,
    /// Required metadata.
    pub metadata: HashMap<String, String>,
    /// Minimum health status.
    pub min_health: Option<HealthStatus>,
    /// Version filter.
    pub version: Option<String>,
}

impl ServiceQuery {
    /// Create a new query.
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            ..Default::default()
        }
    }

    /// Require tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Require metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Require version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Check if instance matches query.
    pub fn matches(&self, instance: &ServiceInstance) -> bool {
        if instance.service_name != self.service_name {
            return false;
        }

        // Check tags
        for tag in &self.tags {
            if !instance.tags.contains(tag) {
                return false;
            }
        }

        // Check metadata
        for (key, value) in &self.metadata {
            if instance.metadata.get(key) != Some(value) {
                return false;
            }
        }

        // Check version
        if let Some(v) = &self.version {
            if instance.version.as_ref() != Some(v) {
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_instance_creation() {
        let instance = ServiceInstance::new("api", "localhost", 8080)
            .with_protocol("https")
            .with_tag("primary")
            .with_metadata("region", "us-east");

        assert_eq!(instance.service_name, "api");
        assert_eq!(instance.endpoint(), "https://localhost:8080");
        assert!(instance.tags.contains(&"primary".to_string()));
    }

    #[tokio::test]
    async fn test_register_instance() {
        let registry = InMemoryRegistry::new();

        let instance = ServiceInstance::new("api", "localhost", 8080);
        registry.register_instance(instance).await.unwrap();

        let instances = registry.get_instances("api").await.unwrap();
        assert_eq!(instances.len(), 1);
    }

    #[tokio::test]
    async fn test_deregister_instance() {
        let registry = InMemoryRegistry::new();

        let instance = ServiceInstance::new("api", "localhost", 8080);
        let id = instance.id.clone();
        registry.register_instance(instance).await.unwrap();

        registry.deregister_instance(&id).await.unwrap();

        let instances = registry.get_instances("api").await.unwrap();
        assert!(instances.is_empty());
    }

    #[tokio::test]
    async fn test_healthy_instances() {
        let registry = InMemoryRegistry::new();

        let healthy = ServiceInstance::new("api", "localhost", 8080);
        let unhealthy = ServiceInstance::new("api", "localhost", 8081);

        registry.register_instance(healthy.clone()).await.unwrap();
        registry.register_instance(unhealthy.clone()).await.unwrap();

        registry
            .update_health(&unhealthy.id, HealthStatus::Unhealthy)
            .await
            .unwrap();

        let healthy_instances = registry.get_healthy_instances("api").await.unwrap();
        assert_eq!(healthy_instances.len(), 1);
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let registry = InMemoryRegistry::new();

        let instance = ServiceInstance::new("api", "localhost", 8080);
        let id = instance.id.clone();
        registry.register_instance(instance).await.unwrap();

        registry.heartbeat(&id).await.unwrap();

        let instances = registry.get_instances("api").await.unwrap();
        assert!(!instances.is_empty());
    }

    #[tokio::test]
    async fn test_registry_client() {
        let registry = Arc::new(InMemoryRegistry::new());
        let mut client = RegistryClient::new(registry.clone());

        let instance = ServiceInstance::new("api", "localhost", 8080);
        client.register(instance).await.unwrap();

        let discovered = client.discover("api").await.unwrap();
        assert_eq!(discovered.len(), 1);

        client.deregister().await.unwrap();

        let discovered = client.discover("api").await.unwrap();
        assert!(discovered.is_empty());
    }

    #[test]
    fn test_service_query() {
        let query = ServiceQuery::new("api")
            .with_tag("primary")
            .with_metadata("region", "us-east");

        let matching = ServiceInstance::new("api", "localhost", 8080)
            .with_tag("primary")
            .with_metadata("region", "us-east");

        let non_matching = ServiceInstance::new("api", "localhost", 8081).with_tag("secondary");

        assert!(query.matches(&matching));
        assert!(!query.matches(&non_matching));
    }

    #[tokio::test]
    async fn test_list_services() {
        let registry = InMemoryRegistry::new();

        registry
            .register_service(Service::new("api"))
            .await
            .unwrap();
        registry
            .register_service(Service::new("worker"))
            .await
            .unwrap();

        let services = registry.list_services().await.unwrap();
        assert_eq!(services.len(), 2);
    }
}
