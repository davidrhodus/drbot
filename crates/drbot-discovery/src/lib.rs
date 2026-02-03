//! Service discovery for drbot.
//!
//! This crate provides:
//! - Service discovery strategies
//! - DNS-based discovery
//! - Watch-based updates
//! - Caching discovery results

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};

/// Discovery error types.
#[derive(Error, Debug)]
pub enum DiscoveryError {
    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Discovery failed: {0}")]
    DiscoveryFailed(String),

    #[error("No healthy instances")]
    NoHealthyInstances,

    #[error("Timeout")]
    Timeout,
}

/// Result type for discovery operations.
pub type Result<T> = std::result::Result<T, DiscoveryError>;

/// A discovered endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    /// Endpoint address.
    pub address: String,
    /// Port.
    pub port: u16,
    /// Weight for load balancing.
    pub weight: u32,
    /// Metadata.
    pub metadata: HashMap<String, String>,
    /// Is healthy.
    pub healthy: bool,
}

impl Endpoint {
    /// Create a new endpoint.
    pub fn new(address: impl Into<String>, port: u16) -> Self {
        Self {
            address: address.into(),
            port,
            weight: 100,
            metadata: HashMap::new(),
            healthy: true,
        }
    }

    /// Set weight.
    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    /// Add metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Get URL.
    pub fn url(&self, scheme: &str) -> String {
        format!("{}://{}:{}", scheme, self.address, self.port)
    }
}

/// Discovery result.
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    /// Service name.
    pub service_name: String,
    /// Discovered endpoints.
    pub endpoints: Vec<Endpoint>,
    /// Discovery timestamp.
    pub discovered_at: DateTime<Utc>,
    /// Time-to-live.
    pub ttl: Duration,
}

impl DiscoveryResult {
    /// Check if result is expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.discovered_at + self.ttl
    }

    /// Get healthy endpoints.
    pub fn healthy_endpoints(&self) -> Vec<&Endpoint> {
        self.endpoints.iter().filter(|e| e.healthy).collect()
    }
}

/// Service discovery trait.
#[async_trait]
pub trait ServiceDiscovery: Send + Sync {
    /// Discover endpoints for a service.
    async fn discover(&self, service_name: &str) -> Result<DiscoveryResult>;

    /// Watch for changes to a service.
    async fn watch(&self, service_name: &str) -> Result<broadcast::Receiver<DiscoveryResult>>;
}

/// Static discovery (configured endpoints).
pub struct StaticDiscovery {
    services: HashMap<String, Vec<Endpoint>>,
}

impl StaticDiscovery {
    /// Create new static discovery.
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    /// Add a service.
    pub fn add_service(mut self, name: impl Into<String>, endpoints: Vec<Endpoint>) -> Self {
        self.services.insert(name.into(), endpoints);
        self
    }
}

impl Default for StaticDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceDiscovery for StaticDiscovery {
    async fn discover(&self, service_name: &str) -> Result<DiscoveryResult> {
        let endpoints = self
            .services
            .get(service_name)
            .cloned()
            .ok_or_else(|| DiscoveryError::ServiceNotFound(service_name.to_string()))?;

        Ok(DiscoveryResult {
            service_name: service_name.to_string(),
            endpoints,
            discovered_at: Utc::now(),
            ttl: Duration::hours(24), // Static never expires really
        })
    }

    async fn watch(&self, _service_name: &str) -> Result<broadcast::Receiver<DiscoveryResult>> {
        // Static discovery doesn't support watching
        let (tx, rx) = broadcast::channel(1);
        drop(tx); // Close immediately
        Ok(rx)
    }
}

/// Cached discovery wrapper.
pub struct CachedDiscovery<D: ServiceDiscovery> {
    inner: Arc<D>,
    cache: RwLock<HashMap<String, DiscoveryResult>>,
    default_ttl: Duration,
}

impl<D: ServiceDiscovery> CachedDiscovery<D> {
    /// Create new cached discovery.
    pub fn new(inner: Arc<D>) -> Self {
        Self {
            inner,
            cache: RwLock::new(HashMap::new()),
            default_ttl: Duration::seconds(60),
        }
    }

    /// Set default TTL.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    /// Invalidate cache for a service.
    pub async fn invalidate(&self, service_name: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(service_name);
    }

    /// Invalidate all cache.
    pub async fn invalidate_all(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

#[async_trait]
impl<D: ServiceDiscovery + 'static> ServiceDiscovery for CachedDiscovery<D> {
    async fn discover(&self, service_name: &str) -> Result<DiscoveryResult> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(result) = cache.get(service_name) {
                if !result.is_expired() {
                    return Ok(result.clone());
                }
            }
        }

        // Fetch from inner
        let mut result = self.inner.discover(service_name).await?;
        result.ttl = self.default_ttl;

        // Cache result
        {
            let mut cache = self.cache.write().await;
            cache.insert(service_name.to_string(), result.clone());
        }

        Ok(result)
    }

    async fn watch(&self, service_name: &str) -> Result<broadcast::Receiver<DiscoveryResult>> {
        self.inner.watch(service_name).await
    }
}

/// Round-robin endpoint selector.
pub struct RoundRobinSelector {
    counters: RwLock<HashMap<String, usize>>,
}

impl RoundRobinSelector {
    /// Create new selector.
    pub fn new() -> Self {
        Self {
            counters: RwLock::new(HashMap::new()),
        }
    }

    /// Select next endpoint.
    pub async fn select<'a>(&self, result: &'a DiscoveryResult) -> Option<&'a Endpoint> {
        let healthy: Vec<_> = result.healthy_endpoints();
        if healthy.is_empty() {
            return None;
        }

        let mut counters = self.counters.write().await;
        let counter = counters.entry(result.service_name.clone()).or_insert(0);
        let index = *counter % healthy.len();
        *counter = counter.wrapping_add(1);

        Some(healthy[index])
    }
}

impl Default for RoundRobinSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// Weighted random endpoint selector.
pub struct WeightedSelector;

impl WeightedSelector {
    /// Select endpoint based on weight.
    pub fn select<'a>(result: &'a DiscoveryResult) -> Option<&'a Endpoint> {
        let healthy: Vec<_> = result.healthy_endpoints();
        if healthy.is_empty() {
            return None;
        }

        let total_weight: u32 = healthy.iter().map(|e| e.weight).sum();
        if total_weight == 0 {
            return healthy.first().copied();
        }

        let mut rng_value = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % total_weight as u128) as u32;

        for endpoint in &healthy {
            if rng_value < endpoint.weight {
                return Some(*endpoint);
            }
            rng_value -= endpoint.weight;
        }

        healthy.last().copied()
    }
}

/// Discovery with fallback.
pub struct FallbackDiscovery {
    primary: Arc<dyn ServiceDiscovery>,
    fallback: Arc<dyn ServiceDiscovery>,
}

impl FallbackDiscovery {
    /// Create new fallback discovery.
    pub fn new(primary: Arc<dyn ServiceDiscovery>, fallback: Arc<dyn ServiceDiscovery>) -> Self {
        Self { primary, fallback }
    }
}

#[async_trait]
impl ServiceDiscovery for FallbackDiscovery {
    async fn discover(&self, service_name: &str) -> Result<DiscoveryResult> {
        match self.primary.discover(service_name).await {
            Ok(result) if !result.endpoints.is_empty() => Ok(result),
            _ => self.fallback.discover(service_name).await,
        }
    }

    async fn watch(&self, service_name: &str) -> Result<broadcast::Receiver<DiscoveryResult>> {
        // Try primary first
        if let Ok(rx) = self.primary.watch(service_name).await {
            return Ok(rx);
        }
        self.fallback.watch(service_name).await
    }
}

/// Discovery event.
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// Endpoints added.
    Added(Vec<Endpoint>),
    /// Endpoints removed.
    Removed(Vec<Endpoint>),
    /// Endpoints updated.
    Updated(Vec<Endpoint>),
    /// Full refresh.
    Refresh(DiscoveryResult),
}

/// Discovery watcher.
pub struct DiscoveryWatcher<D: ServiceDiscovery> {
    discovery: Arc<D>,
    service_name: String,
    poll_interval: std::time::Duration,
}

impl<D: ServiceDiscovery + 'static> DiscoveryWatcher<D> {
    /// Create new watcher.
    pub fn new(discovery: Arc<D>, service_name: impl Into<String>) -> Self {
        Self {
            discovery,
            service_name: service_name.into(),
            poll_interval: std::time::Duration::from_secs(30),
        }
    }

    /// Set poll interval.
    pub fn with_poll_interval(mut self, interval: std::time::Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Start watching (returns channel for events).
    pub fn start(self) -> broadcast::Receiver<DiscoveryEvent> {
        let (tx, rx) = broadcast::channel(100);

        tokio::spawn(async move {
            let mut last_endpoints: Vec<String> = Vec::new();

            loop {
                if let Ok(result) = self.discovery.discover(&self.service_name).await {
                    let current_endpoints: Vec<String> = result
                        .endpoints
                        .iter()
                        .map(|e| format!("{}:{}", e.address, e.port))
                        .collect();

                    if current_endpoints != last_endpoints {
                        let _ = tx.send(DiscoveryEvent::Refresh(result));
                        last_endpoints = current_endpoints;
                    }
                }

                tokio::time::sleep(self.poll_interval).await;
            }
        });

        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_creation() {
        let endpoint = Endpoint::new("localhost", 8080)
            .with_weight(200)
            .with_metadata("zone", "us-east");

        assert_eq!(endpoint.address, "localhost");
        assert_eq!(endpoint.weight, 200);
        assert_eq!(endpoint.url("http"), "http://localhost:8080");
    }

    #[tokio::test]
    async fn test_static_discovery() {
        let discovery = StaticDiscovery::new().add_service(
            "api",
            vec![
                Endpoint::new("localhost", 8080),
                Endpoint::new("localhost", 8081),
            ],
        );

        let result = discovery.discover("api").await.unwrap();
        assert_eq!(result.endpoints.len(), 2);
    }

    #[tokio::test]
    async fn test_static_discovery_not_found() {
        let discovery = StaticDiscovery::new();
        let result = discovery.discover("missing").await;
        assert!(matches!(result, Err(DiscoveryError::ServiceNotFound(_))));
    }

    #[tokio::test]
    async fn test_cached_discovery() {
        let static_discovery = Arc::new(
            StaticDiscovery::new().add_service("api", vec![Endpoint::new("localhost", 8080)]),
        );

        let cached = CachedDiscovery::new(static_discovery).with_ttl(Duration::seconds(60));

        // First call populates cache
        let result1 = cached.discover("api").await.unwrap();
        assert_eq!(result1.endpoints.len(), 1);

        // Second call uses cache
        let result2 = cached.discover("api").await.unwrap();
        assert_eq!(result2.endpoints.len(), 1);
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let static_discovery = Arc::new(
            StaticDiscovery::new().add_service("api", vec![Endpoint::new("localhost", 8080)]),
        );

        let cached = CachedDiscovery::new(static_discovery);

        cached.discover("api").await.unwrap();
        cached.invalidate("api").await;

        // Should refetch
        let result = cached.discover("api").await.unwrap();
        assert_eq!(result.endpoints.len(), 1);
    }

    #[tokio::test]
    async fn test_round_robin_selector() {
        let result = DiscoveryResult {
            service_name: "api".to_string(),
            endpoints: vec![
                Endpoint::new("host1", 8080),
                Endpoint::new("host2", 8080),
                Endpoint::new("host3", 8080),
            ],
            discovered_at: Utc::now(),
            ttl: Duration::hours(1),
        };

        let selector = RoundRobinSelector::new();

        let e1 = selector.select(&result).await.unwrap();
        let e2 = selector.select(&result).await.unwrap();
        let e3 = selector.select(&result).await.unwrap();
        let e4 = selector.select(&result).await.unwrap();

        assert_eq!(e1.address, "host1");
        assert_eq!(e2.address, "host2");
        assert_eq!(e3.address, "host3");
        assert_eq!(e4.address, "host1"); // Wraps around
    }

    #[test]
    fn test_weighted_selector() {
        let result = DiscoveryResult {
            service_name: "api".to_string(),
            endpoints: vec![
                Endpoint::new("host1", 8080).with_weight(100),
                Endpoint::new("host2", 8080).with_weight(0), // Should never be selected
            ],
            discovered_at: Utc::now(),
            ttl: Duration::hours(1),
        };

        // With weight 0 for host2, only host1 should be selected
        for _ in 0..10 {
            let endpoint = WeightedSelector::select(&result).unwrap();
            // Due to the simple RNG, we can't guarantee but host1 should be favored
            assert!(endpoint.weight > 0 || result.endpoints.len() == 1);
        }
    }

    #[tokio::test]
    async fn test_fallback_discovery() {
        let empty = Arc::new(StaticDiscovery::new());
        let with_endpoints = Arc::new(
            StaticDiscovery::new().add_service("api", vec![Endpoint::new("localhost", 8080)]),
        );

        let fallback = FallbackDiscovery::new(empty, with_endpoints);

        let result = fallback.discover("api").await.unwrap();
        assert_eq!(result.endpoints.len(), 1);
    }

    #[test]
    fn test_discovery_result_expiry() {
        let result = DiscoveryResult {
            service_name: "api".to_string(),
            endpoints: vec![],
            discovered_at: Utc::now() - Duration::hours(2),
            ttl: Duration::hours(1),
        };

        assert!(result.is_expired());
    }

    #[test]
    fn test_healthy_endpoints() {
        let result = DiscoveryResult {
            service_name: "api".to_string(),
            endpoints: vec![
                Endpoint::new("host1", 8080),
                Endpoint {
                    healthy: false,
                    ..Endpoint::new("host2", 8080)
                },
            ],
            discovered_at: Utc::now(),
            ttl: Duration::hours(1),
        };

        let healthy = result.healthy_endpoints();
        assert_eq!(healthy.len(), 1);
        assert_eq!(healthy[0].address, "host1");
    }
}
