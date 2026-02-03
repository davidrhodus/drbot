//! Load balancing for drbot.
//!
//! This crate provides:
//! - Load balancing algorithms
//! - Health-aware routing
//! - Sticky sessions
//! - Connection tracking

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

/// Load balancer error types.
#[derive(Error, Debug)]
pub enum LoadBalancerError {
    #[error("No backends available")]
    NoBackends,

    #[error("Backend not found: {0}")]
    BackendNotFound(String),

    #[error("All backends unhealthy")]
    AllUnhealthy,

    #[error("Circuit open for: {0}")]
    CircuitOpen(String),
}

/// Result type for load balancer operations.
pub type Result<T> = std::result::Result<T, LoadBalancerError>;

/// Backend health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendHealth {
    /// Backend is healthy.
    Healthy,
    /// Backend is degraded.
    Degraded,
    /// Backend is unhealthy.
    Unhealthy,
    /// Health unknown.
    Unknown,
}

/// A backend server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backend {
    /// Backend ID.
    pub id: String,
    /// Address.
    pub address: String,
    /// Port.
    pub port: u16,
    /// Weight for weighted algorithms.
    pub weight: u32,
    /// Priority (lower is higher priority).
    pub priority: u32,
    /// Health status.
    pub health: BackendHealth,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl Backend {
    /// Create a new backend.
    pub fn new(id: impl Into<String>, address: impl Into<String>, port: u16) -> Self {
        Self {
            id: id.into(),
            address: address.into(),
            port,
            weight: 100,
            priority: 0,
            health: BackendHealth::Unknown,
            metadata: HashMap::new(),
        }
    }

    /// Set weight.
    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// Get endpoint URL.
    pub fn endpoint(&self, scheme: &str) -> String {
        format!("{}://{}:{}", scheme, self.address, self.port)
    }

    /// Check if healthy.
    pub fn is_healthy(&self) -> bool {
        matches!(
            self.health,
            BackendHealth::Healthy | BackendHealth::Degraded
        )
    }
}

/// Backend statistics.
#[derive(Debug, Default)]
pub struct BackendStats {
    /// Total requests.
    pub requests: AtomicU64,
    /// Active connections.
    pub active_connections: AtomicU64,
    /// Total failures.
    pub failures: AtomicU64,
    /// Total successes.
    pub successes: AtomicU64,
    /// Total latency (for averaging).
    pub total_latency_ms: AtomicU64,
}

impl BackendStats {
    /// Record a request start.
    pub fn request_start(&self) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a request end.
    pub fn request_end(&self, success: bool, latency_ms: u64) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
        self.total_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);

        if success {
            self.successes.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get average latency.
    pub fn avg_latency_ms(&self) -> f64 {
        let total = self.successes.load(Ordering::Relaxed) + self.failures.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.total_latency_ms.load(Ordering::Relaxed) as f64 / total as f64
        }
    }

    /// Get success rate.
    pub fn success_rate(&self) -> f64 {
        let successes = self.successes.load(Ordering::Relaxed);
        let failures = self.failures.load(Ordering::Relaxed);
        let total = successes + failures;
        if total == 0 {
            1.0
        } else {
            successes as f64 / total as f64
        }
    }
}

/// Load balancing algorithm.
#[async_trait]
pub trait LoadBalancingAlgorithm: Send + Sync {
    /// Select a backend.
    async fn select(&self, backends: &[Backend], context: &SelectionContext) -> Result<usize>;

    /// Algorithm name.
    fn name(&self) -> &str;
}

/// Selection context.
#[derive(Debug, Clone, Default)]
pub struct SelectionContext {
    /// Client identifier (for sticky sessions).
    pub client_id: Option<String>,
    /// Request path.
    pub path: Option<String>,
    /// Request headers.
    pub headers: HashMap<String, String>,
}

/// Round-robin algorithm.
pub struct RoundRobin {
    counter: AtomicUsize,
}

impl RoundRobin {
    /// Create new round-robin.
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl Default for RoundRobin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LoadBalancingAlgorithm for RoundRobin {
    async fn select(&self, backends: &[Backend], _context: &SelectionContext) -> Result<usize> {
        let healthy: Vec<_> = backends
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_healthy())
            .collect();

        if healthy.is_empty() {
            return Err(LoadBalancerError::AllUnhealthy);
        }

        let index = self.counter.fetch_add(1, Ordering::Relaxed) % healthy.len();
        Ok(healthy[index].0)
    }

    fn name(&self) -> &str {
        "round-robin"
    }
}

/// Weighted round-robin algorithm.
pub struct WeightedRoundRobin {
    state: RwLock<WeightedState>,
}

struct WeightedState {
    current_index: usize,
    current_weight: i32,
}

impl WeightedRoundRobin {
    /// Create new weighted round-robin.
    pub fn new() -> Self {
        Self {
            state: RwLock::new(WeightedState {
                current_index: 0,
                current_weight: 0,
            }),
        }
    }
}

impl Default for WeightedRoundRobin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LoadBalancingAlgorithm for WeightedRoundRobin {
    async fn select(&self, backends: &[Backend], _context: &SelectionContext) -> Result<usize> {
        let healthy: Vec<_> = backends
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_healthy())
            .collect();

        if healthy.is_empty() {
            return Err(LoadBalancerError::AllUnhealthy);
        }

        let max_weight = healthy.iter().map(|(_, b)| b.weight).max().unwrap_or(1) as i32;
        let gcd = healthy.iter().map(|(_, b)| b.weight).fold(0, gcd_fn) as i32;

        let mut state = self.state.write().await;

        loop {
            state.current_index = (state.current_index + 1) % healthy.len();

            if state.current_index == 0 {
                state.current_weight -= gcd;
                if state.current_weight <= 0 {
                    state.current_weight = max_weight;
                }
            }

            let (original_index, backend) = healthy[state.current_index];
            if backend.weight as i32 >= state.current_weight {
                return Ok(original_index);
            }
        }
    }

    fn name(&self) -> &str {
        "weighted-round-robin"
    }
}

fn gcd_fn(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd_fn(b, a % b)
    }
}

/// Least connections algorithm.
pub struct LeastConnections {
    connections: RwLock<HashMap<String, u64>>,
}

impl LeastConnections {
    /// Create new least connections.
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
        }
    }

    /// Update connection count.
    pub async fn update(&self, backend_id: &str, delta: i64) {
        let mut connections = self.connections.write().await;
        let count = connections.entry(backend_id.to_string()).or_insert(0);
        *count = (*count as i64 + delta).max(0) as u64;
    }
}

impl Default for LeastConnections {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LoadBalancingAlgorithm for LeastConnections {
    async fn select(&self, backends: &[Backend], _context: &SelectionContext) -> Result<usize> {
        let healthy: Vec<_> = backends
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_healthy())
            .collect();

        if healthy.is_empty() {
            return Err(LoadBalancerError::AllUnhealthy);
        }

        let connections = self.connections.read().await;

        let (index, _) = healthy
            .iter()
            .min_by_key(|(_, b)| connections.get(&b.id).copied().unwrap_or(0))
            .unwrap();

        Ok(*index)
    }

    fn name(&self) -> &str {
        "least-connections"
    }
}

/// Consistent hash algorithm (for sticky sessions).
pub struct ConsistentHash {
    replicas: usize,
}

impl ConsistentHash {
    /// Create new consistent hash.
    pub fn new(replicas: usize) -> Self {
        Self { replicas }
    }

    fn hash_key(key: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for ConsistentHash {
    fn default() -> Self {
        Self::new(150)
    }
}

#[async_trait]
impl LoadBalancingAlgorithm for ConsistentHash {
    async fn select(&self, backends: &[Backend], context: &SelectionContext) -> Result<usize> {
        let healthy: Vec<_> = backends
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_healthy())
            .collect();

        if healthy.is_empty() {
            return Err(LoadBalancerError::AllUnhealthy);
        }

        let key = context.client_id.as_deref().unwrap_or("default");
        let hash = Self::hash_key(key);

        // Build virtual nodes
        let mut ring: Vec<(u64, usize)> = Vec::new();
        for (idx, (original_idx, backend)) in healthy.iter().enumerate() {
            for replica in 0..self.replicas {
                let virtual_key = format!("{}:{}", backend.id, replica);
                let virtual_hash = Self::hash_key(&virtual_key);
                ring.push((virtual_hash, *original_idx));
            }
        }
        ring.sort_by_key(|(h, _)| *h);

        // Find the first node >= hash
        let idx = ring.iter().position(|(h, _)| *h >= hash).unwrap_or(0);

        Ok(ring[idx].1)
    }

    fn name(&self) -> &str {
        "consistent-hash"
    }
}

/// Random algorithm.
pub struct Random;

#[async_trait]
impl LoadBalancingAlgorithm for Random {
    async fn select(&self, backends: &[Backend], _context: &SelectionContext) -> Result<usize> {
        let healthy: Vec<_> = backends
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_healthy())
            .collect();

        if healthy.is_empty() {
            return Err(LoadBalancerError::AllUnhealthy);
        }

        // Simple pseudo-random using time
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as usize;

        let idx = now % healthy.len();
        Ok(healthy[idx].0)
    }

    fn name(&self) -> &str {
        "random"
    }
}

/// Load balancer.
pub struct LoadBalancer {
    backends: RwLock<Vec<Backend>>,
    algorithm: Arc<dyn LoadBalancingAlgorithm>,
    stats: RwLock<HashMap<String, Arc<BackendStats>>>,
}

impl LoadBalancer {
    /// Create a new load balancer.
    pub fn new(algorithm: Arc<dyn LoadBalancingAlgorithm>) -> Self {
        Self {
            backends: RwLock::new(Vec::new()),
            algorithm,
            stats: RwLock::new(HashMap::new()),
        }
    }

    /// Add a backend.
    pub async fn add_backend(&self, backend: Backend) {
        let mut backends = self.backends.write().await;
        let mut stats = self.stats.write().await;

        stats.insert(backend.id.clone(), Arc::new(BackendStats::default()));
        backends.push(backend);
    }

    /// Remove a backend.
    pub async fn remove_backend(&self, id: &str) {
        let mut backends = self.backends.write().await;
        backends.retain(|b| b.id != id);

        let mut stats = self.stats.write().await;
        stats.remove(id);
    }

    /// Update backend health.
    pub async fn set_health(&self, id: &str, health: BackendHealth) {
        let mut backends = self.backends.write().await;
        if let Some(backend) = backends.iter_mut().find(|b| b.id == id) {
            backend.health = health;
        }
    }

    /// Select a backend.
    pub async fn select(&self, context: &SelectionContext) -> Result<Backend> {
        let backends = self.backends.read().await;

        if backends.is_empty() {
            return Err(LoadBalancerError::NoBackends);
        }

        let index = self.algorithm.select(&backends, context).await?;
        Ok(backends[index].clone())
    }

    /// Get backend stats.
    pub async fn get_stats(&self, id: &str) -> Option<Arc<BackendStats>> {
        let stats = self.stats.read().await;
        stats.get(id).cloned()
    }

    /// Record request.
    pub async fn record_request(&self, id: &str, success: bool, latency_ms: u64) {
        let stats = self.stats.read().await;
        if let Some(s) = stats.get(id) {
            s.request_end(success, latency_ms);
        }
    }

    /// List backends.
    pub async fn backends(&self) -> Vec<Backend> {
        self.backends.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_backends() -> Vec<Backend> {
        vec![
            Backend::new("b1", "host1", 8080).with_weight(100),
            Backend::new("b2", "host2", 8080).with_weight(200),
            Backend::new("b3", "host3", 8080).with_weight(100),
        ]
    }

    fn create_healthy_backends() -> Vec<Backend> {
        create_backends()
            .into_iter()
            .map(|mut b| {
                b.health = BackendHealth::Healthy;
                b
            })
            .collect()
    }

    #[test]
    fn test_backend_creation() {
        let backend = Backend::new("b1", "localhost", 8080)
            .with_weight(150)
            .with_priority(1);

        assert_eq!(backend.endpoint("http"), "http://localhost:8080");
        assert_eq!(backend.weight, 150);
    }

    #[tokio::test]
    async fn test_round_robin() {
        let algo = RoundRobin::new();
        let backends = create_healthy_backends();
        let context = SelectionContext::default();

        let i1 = algo.select(&backends, &context).await.unwrap();
        let i2 = algo.select(&backends, &context).await.unwrap();
        let i3 = algo.select(&backends, &context).await.unwrap();
        let i4 = algo.select(&backends, &context).await.unwrap();

        // Should cycle through
        assert_eq!(i1, 0);
        assert_eq!(i2, 1);
        assert_eq!(i3, 2);
        assert_eq!(i4, 0);
    }

    #[tokio::test]
    async fn test_round_robin_skips_unhealthy() {
        let algo = RoundRobin::new();
        let mut backends = create_healthy_backends();
        backends[1].health = BackendHealth::Unhealthy;
        let context = SelectionContext::default();

        for _ in 0..10 {
            let idx = algo.select(&backends, &context).await.unwrap();
            assert_ne!(idx, 1); // Should never select unhealthy
        }
    }

    #[tokio::test]
    async fn test_consistent_hash_sticky() {
        let algo = ConsistentHash::new(150);
        let backends = create_healthy_backends();

        let context1 = SelectionContext {
            client_id: Some("client-123".to_string()),
            ..Default::default()
        };

        // Same client should always get same backend
        let first = algo.select(&backends, &context1).await.unwrap();
        for _ in 0..10 {
            let idx = algo.select(&backends, &context1).await.unwrap();
            assert_eq!(idx, first);
        }
    }

    #[tokio::test]
    async fn test_least_connections() {
        let algo = LeastConnections::new();
        let backends = create_healthy_backends();
        let context = SelectionContext::default();

        // Add some connections
        algo.update("b1", 5).await;
        algo.update("b2", 10).await;
        algo.update("b3", 2).await;

        let idx = algo.select(&backends, &context).await.unwrap();
        assert_eq!(idx, 2); // b3 has least connections
    }

    #[tokio::test]
    async fn test_load_balancer() {
        let algo = Arc::new(RoundRobin::new());
        let lb = LoadBalancer::new(algo);

        for mut b in create_backends() {
            b.health = BackendHealth::Healthy;
            lb.add_backend(b).await;
        }

        let backend = lb.select(&SelectionContext::default()).await.unwrap();
        assert!(!backend.id.is_empty());
    }

    #[tokio::test]
    async fn test_load_balancer_no_backends() {
        let algo = Arc::new(RoundRobin::new());
        let lb = LoadBalancer::new(algo);

        let result = lb.select(&SelectionContext::default()).await;
        assert!(matches!(result, Err(LoadBalancerError::NoBackends)));
    }

    #[test]
    fn test_backend_stats() {
        let stats = BackendStats::default();

        stats.request_start();
        assert_eq!(stats.active_connections.load(Ordering::Relaxed), 1);

        stats.request_end(true, 100);
        assert_eq!(stats.active_connections.load(Ordering::Relaxed), 0);
        assert_eq!(stats.successes.load(Ordering::Relaxed), 1);
        assert_eq!(stats.avg_latency_ms(), 100.0);
    }

    #[tokio::test]
    async fn test_remove_backend() {
        let algo = Arc::new(RoundRobin::new());
        let lb = LoadBalancer::new(algo);

        let mut b = Backend::new("b1", "host1", 8080);
        b.health = BackendHealth::Healthy;
        lb.add_backend(b).await;

        lb.remove_backend("b1").await;

        let backends = lb.backends().await;
        assert!(backends.is_empty());
    }

    #[tokio::test]
    async fn test_set_health() {
        let algo = Arc::new(RoundRobin::new());
        let lb = LoadBalancer::new(algo);

        let b = Backend::new("b1", "host1", 8080);
        lb.add_backend(b).await;

        lb.set_health("b1", BackendHealth::Healthy).await;

        let backends = lb.backends().await;
        assert_eq!(backends[0].health, BackendHealth::Healthy);
    }
}
