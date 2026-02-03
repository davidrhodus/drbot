//! Provider fallback chains.
//!
//! This crate provides:
//! - Fallback chain management
//! - Health-based routing
//! - Priority-based selection
//! - Automatic failover

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;

/// Fallback errors.
#[derive(Debug, Error)]
pub enum FallbackError {
    #[error("All providers failed")]
    AllProvidersFailed,

    #[error("No providers available")]
    NoProvidersAvailable,

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Timeout: {0}")]
    Timeout(String),
}

/// Result type for fallback operations.
pub type Result<T> = std::result::Result<T, FallbackError>;

/// Provider health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Provider is healthy.
    Healthy,
    /// Provider is degraded but usable.
    Degraded,
    /// Provider is unhealthy.
    Unhealthy,
    /// Provider health is unknown.
    Unknown,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Provider information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Provider identifier.
    pub id: String,
    /// Provider name.
    pub name: String,
    /// Priority (higher = preferred).
    pub priority: i32,
    /// Health status.
    pub health: HealthStatus,
    /// Last check time.
    pub last_check: Option<DateTime<Utc>>,
    /// Success rate (0-1).
    pub success_rate: f64,
    /// Average latency in ms.
    pub avg_latency_ms: u64,
    /// Is enabled.
    pub enabled: bool,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

impl ProviderInfo {
    /// Create new provider info.
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            priority: 0,
            health: HealthStatus::Unknown,
            last_check: None,
            success_rate: 1.0,
            avg_latency_ms: 0,
            enabled: true,
            metadata: HashMap::new(),
        }
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Check if provider is available.
    pub fn is_available(&self) -> bool {
        self.enabled
            && matches!(
                self.health,
                HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unknown
            )
    }
}

/// Provider statistics.
#[derive(Debug, Clone, Default)]
struct ProviderStats {
    total_calls: usize,
    successful_calls: usize,
    failed_calls: usize,
    total_latency_ms: u64,
}

/// Fallback chain configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackConfig {
    /// Health check interval.
    pub health_check_interval: Duration,
    /// Timeout per provider.
    pub timeout_per_provider: Duration,
    /// Maximum retries per provider.
    pub max_retries_per_provider: u32,
    /// Cooldown after failure.
    pub failure_cooldown: Duration,
    /// Minimum success rate to stay healthy.
    pub min_success_rate: f64,
    /// Selection strategy.
    pub selection_strategy: SelectionStrategy,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(30),
            timeout_per_provider: Duration::from_secs(30),
            max_retries_per_provider: 1,
            failure_cooldown: Duration::from_secs(60),
            min_success_rate: 0.8,
            selection_strategy: SelectionStrategy::Priority,
        }
    }
}

/// Provider selection strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionStrategy {
    /// Select by priority.
    Priority,
    /// Round robin.
    RoundRobin,
    /// Least latency.
    LeastLatency,
    /// Weighted by success rate.
    WeightedSuccessRate,
    /// Random.
    Random,
}

impl Default for SelectionStrategy {
    fn default() -> Self {
        Self::Priority
    }
}

/// Fallback execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackResult<T> {
    /// The result value.
    pub value: T,
    /// Provider that succeeded.
    pub provider_id: String,
    /// Providers tried before success.
    pub providers_tried: Vec<String>,
    /// Total attempts.
    pub total_attempts: u32,
    /// Duration.
    pub duration: Duration,
}

/// Provider trait for fallback chain.
#[async_trait]
pub trait FallbackProvider: Send + Sync {
    /// Get provider ID.
    fn id(&self) -> &str;

    /// Get provider name.
    fn name(&self) -> &str;

    /// Health check.
    async fn health_check(&self) -> HealthStatus;
}

/// The fallback chain.
pub struct FallbackChain<P> {
    /// Providers.
    providers: Arc<RwLock<Vec<(Arc<P>, ProviderInfo)>>>,
    /// Configuration.
    config: FallbackConfig,
    /// Statistics.
    stats: Arc<RwLock<HashMap<String, ProviderStats>>>,
    /// Round robin index.
    round_robin_idx: Arc<RwLock<usize>>,
    /// Failure timestamps.
    failure_times: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
}

impl<P: FallbackProvider + 'static> FallbackChain<P> {
    /// Create a new fallback chain.
    pub fn new(config: FallbackConfig) -> Self {
        Self {
            providers: Arc::new(RwLock::new(Vec::new())),
            config,
            stats: Arc::new(RwLock::new(HashMap::new())),
            round_robin_idx: Arc::new(RwLock::new(0)),
            failure_times: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a provider.
    pub async fn add_provider(&self, provider: Arc<P>, info: ProviderInfo) {
        let mut providers = self.providers.write().await;
        providers.push((provider, info));

        // Sort by priority
        providers.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));
    }

    /// Remove a provider.
    pub async fn remove_provider(&self, id: &str) {
        let mut providers = self.providers.write().await;
        providers.retain(|(_, info)| info.id != id);
    }

    /// Get available providers.
    pub async fn available_providers(&self) -> Vec<ProviderInfo> {
        let providers = self.providers.read().await;
        let failure_times = self.failure_times.read().await;
        let now = Utc::now();

        providers
            .iter()
            .filter(|(_, info)| {
                if !info.is_available() {
                    return false;
                }

                // Check cooldown
                if let Some(failure_time) = failure_times.get(&info.id) {
                    let elapsed = (now - *failure_time).to_std().unwrap_or(Duration::ZERO);
                    if elapsed < self.config.failure_cooldown {
                        return false;
                    }
                }

                true
            })
            .map(|(_, info)| info.clone())
            .collect()
    }

    /// Execute with fallback.
    pub async fn execute<F, Fut, T, E>(&self, operation: F) -> Result<FallbackResult<T>>
    where
        F: Fn(Arc<P>) -> Fut + Clone,
        Fut: Future<Output = std::result::Result<T, E>>,
        E: std::fmt::Display,
    {
        let start = std::time::Instant::now();
        let mut providers_tried = Vec::new();
        let mut total_attempts = 0;

        let ordered_providers = self.get_ordered_providers().await;

        if ordered_providers.is_empty() {
            return Err(FallbackError::NoProvidersAvailable);
        }

        for (provider, info) in ordered_providers {
            providers_tried.push(info.id.clone());

            for attempt in 0..self.config.max_retries_per_provider {
                total_attempts += 1;

                let op_start = std::time::Instant::now();

                let result = tokio::time::timeout(
                    self.config.timeout_per_provider,
                    operation(provider.clone()),
                )
                .await;

                let latency = op_start.elapsed();

                match result {
                    Ok(Ok(value)) => {
                        self.record_success(&info.id, latency).await;

                        return Ok(FallbackResult {
                            value,
                            provider_id: info.id,
                            providers_tried,
                            total_attempts,
                            duration: start.elapsed(),
                        });
                    }
                    Ok(Err(_e)) => {
                        self.record_failure(&info.id).await;

                        // If this is last retry for this provider, move to next
                        if attempt + 1 >= self.config.max_retries_per_provider {
                            break;
                        }
                    }
                    Err(_timeout) => {
                        self.record_failure(&info.id).await;
                        break; // Timeout, try next provider
                    }
                }
            }
        }

        Err(FallbackError::AllProvidersFailed)
    }

    /// Get providers in order based on selection strategy.
    async fn get_ordered_providers(&self) -> Vec<(Arc<P>, ProviderInfo)> {
        let providers = self.providers.read().await;
        let failure_times = self.failure_times.read().await;
        let _stats = self.stats.read().await;
        let now = Utc::now();

        // Filter available providers
        let mut available: Vec<_> = providers
            .iter()
            .filter(|(_, info)| {
                if !info.is_available() {
                    return false;
                }

                if let Some(failure_time) = failure_times.get(&info.id) {
                    let elapsed = (now - *failure_time).to_std().unwrap_or(Duration::ZERO);
                    if elapsed < self.config.failure_cooldown {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        // Sort based on strategy
        match self.config.selection_strategy {
            SelectionStrategy::Priority => {
                available.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));
            }
            SelectionStrategy::LeastLatency => {
                available.sort_by(|a, b| a.1.avg_latency_ms.cmp(&b.1.avg_latency_ms));
            }
            SelectionStrategy::WeightedSuccessRate => {
                available.sort_by(|a, b| b.1.success_rate.partial_cmp(&a.1.success_rate).unwrap());
            }
            SelectionStrategy::RoundRobin => {
                let mut idx = self.round_robin_idx.write().await;
                if !available.is_empty() {
                    let rotation = *idx % available.len();
                    available.rotate_left(rotation);
                    *idx = (*idx + 1) % usize::MAX;
                }
            }
            SelectionStrategy::Random => {
                // Simple shuffle using timestamp
                use std::time::SystemTime;
                let seed = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as usize;

                if !available.is_empty() {
                    for i in (1..available.len()).rev() {
                        let j = (seed + i) % (i + 1);
                        available.swap(i, j);
                    }
                }
            }
        }

        available
    }

    /// Record successful call.
    async fn record_success(&self, provider_id: &str, latency: Duration) {
        let mut stats = self.stats.write().await;
        let entry = stats.entry(provider_id.to_string()).or_default();

        entry.total_calls += 1;
        entry.successful_calls += 1;
        entry.total_latency_ms += latency.as_millis() as u64;

        // Update provider info
        drop(stats);
        self.update_provider_info(provider_id).await;
    }

    /// Record failed call.
    async fn record_failure(&self, provider_id: &str) {
        let mut stats = self.stats.write().await;
        let entry = stats.entry(provider_id.to_string()).or_default();

        entry.total_calls += 1;
        entry.failed_calls += 1;

        // Update failure time
        drop(stats);
        let mut failure_times = self.failure_times.write().await;
        failure_times.insert(provider_id.to_string(), Utc::now());

        // Update provider info
        drop(failure_times);
        self.update_provider_info(provider_id).await;
    }

    /// Update provider info from stats.
    async fn update_provider_info(&self, provider_id: &str) {
        let stats = self.stats.read().await;
        let mut providers = self.providers.write().await;

        if let Some(provider_stats) = stats.get(provider_id) {
            if let Some((_, info)) = providers.iter_mut().find(|(_, i)| i.id == provider_id) {
                if provider_stats.total_calls > 0 {
                    info.success_rate =
                        provider_stats.successful_calls as f64 / provider_stats.total_calls as f64;
                    info.avg_latency_ms =
                        provider_stats.total_latency_ms / provider_stats.total_calls as u64;

                    // Update health based on success rate
                    if info.success_rate < self.config.min_success_rate {
                        info.health = HealthStatus::Degraded;
                    } else {
                        info.health = HealthStatus::Healthy;
                    }
                }

                info.last_check = Some(Utc::now());
            }
        }
    }

    /// Run health checks on all providers.
    pub async fn run_health_checks(&self) {
        // Collect provider IDs and references first
        let provider_ids: Vec<String> = {
            let providers = self.providers.read().await;
            providers.iter().map(|(_, info)| info.id.clone()).collect()
        };

        for id in provider_ids {
            let provider = {
                let providers = self.providers.read().await;
                providers
                    .iter()
                    .find(|(_, info)| info.id == id)
                    .map(|(p, _)| p.clone())
            };

            if let Some(provider) = provider {
                let health = provider.health_check().await;

                // Update health status
                let mut providers = self.providers.write().await;
                if let Some((_, info)) = providers.iter_mut().find(|(_, i)| i.id == id) {
                    info.health = health;
                    info.last_check = Some(Utc::now());
                }
            }
        }
    }

    /// Get provider info by ID.
    pub async fn get_provider_info(&self, id: &str) -> Option<ProviderInfo> {
        let providers = self.providers.read().await;
        providers
            .iter()
            .find(|(_, info)| info.id == id)
            .map(|(_, info)| info.clone())
    }

    /// List all providers.
    pub async fn list_providers(&self) -> Vec<ProviderInfo> {
        let providers = self.providers.read().await;
        providers.iter().map(|(_, info)| info.clone()).collect()
    }

    /// Enable a provider.
    pub async fn enable_provider(&self, id: &str) {
        let mut providers = self.providers.write().await;
        if let Some((_, info)) = providers.iter_mut().find(|(_, i)| i.id == id) {
            info.enabled = true;
        }
    }

    /// Disable a provider.
    pub async fn disable_provider(&self, id: &str) {
        let mut providers = self.providers.write().await;
        if let Some((_, info)) = providers.iter_mut().find(|(_, i)| i.id == id) {
            info.enabled = false;
        }
    }

    /// Set provider priority.
    pub async fn set_priority(&self, id: &str, priority: i32) {
        let mut providers = self.providers.write().await;
        if let Some((_, info)) = providers.iter_mut().find(|(_, i)| i.id == id) {
            info.priority = priority;
        }

        // Re-sort by priority
        providers.sort_by(|a, b| b.1.priority.cmp(&a.1.priority));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        id: String,
        name: String,
        should_fail: Arc<RwLock<bool>>,
    }

    impl MockProvider {
        fn new(id: &str, name: &str) -> Self {
            Self {
                id: id.to_string(),
                name: name.to_string(),
                should_fail: Arc::new(RwLock::new(false)),
            }
        }

        async fn set_should_fail(&self, fail: bool) {
            let mut f = self.should_fail.write().await;
            *f = fail;
        }
    }

    #[async_trait]
    impl FallbackProvider for MockProvider {
        fn id(&self) -> &str {
            &self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        async fn health_check(&self) -> HealthStatus {
            let fail = self.should_fail.read().await;
            if *fail {
                HealthStatus::Unhealthy
            } else {
                HealthStatus::Healthy
            }
        }
    }

    #[tokio::test]
    async fn test_single_provider_success() {
        let chain: FallbackChain<MockProvider> = FallbackChain::new(FallbackConfig::default());

        let provider = Arc::new(MockProvider::new("p1", "Provider 1"));
        chain
            .add_provider(provider.clone(), ProviderInfo::new("p1", "Provider 1"))
            .await;

        let result = chain
            .execute(|_p| async { Ok::<_, &str>("success") })
            .await
            .unwrap();

        assert_eq!(result.value, "success");
        assert_eq!(result.provider_id, "p1");
        assert_eq!(result.providers_tried.len(), 1);
    }

    #[tokio::test]
    async fn test_fallback_to_second_provider() {
        let chain: FallbackChain<MockProvider> = FallbackChain::new(FallbackConfig {
            max_retries_per_provider: 1,
            failure_cooldown: Duration::from_secs(0),
            ..Default::default()
        });

        let p1 = Arc::new(MockProvider::new("p1", "Provider 1"));
        let p2 = Arc::new(MockProvider::new("p2", "Provider 2"));

        chain
            .add_provider(
                p1.clone(),
                ProviderInfo::new("p1", "Provider 1").with_priority(10),
            )
            .await;
        chain
            .add_provider(
                p2.clone(),
                ProviderInfo::new("p2", "Provider 2").with_priority(5),
            )
            .await;

        let call_count = Arc::new(RwLock::new(0));
        let cc = call_count.clone();

        let result = chain
            .execute(move |p| {
                let cc = cc.clone();
                async move {
                    let mut count = cc.write().await;
                    *count += 1;
                    if p.id() == "p1" {
                        Err("p1 fails")
                    } else {
                        Ok("p2 success")
                    }
                }
            })
            .await
            .unwrap();

        assert_eq!(result.value, "p2 success");
        assert_eq!(result.provider_id, "p2");
        assert!(result.providers_tried.contains(&"p1".to_string()));
    }

    #[tokio::test]
    async fn test_all_providers_fail() {
        let chain: FallbackChain<MockProvider> = FallbackChain::new(FallbackConfig {
            max_retries_per_provider: 1,
            failure_cooldown: Duration::from_secs(0),
            ..Default::default()
        });

        let p1 = Arc::new(MockProvider::new("p1", "Provider 1"));
        chain
            .add_provider(p1.clone(), ProviderInfo::new("p1", "Provider 1"))
            .await;

        let result = chain
            .execute(|_p| async { Err::<(), _>("always fails") })
            .await;

        assert!(matches!(result, Err(FallbackError::AllProvidersFailed)));
    }

    #[tokio::test]
    async fn test_no_providers() {
        let chain: FallbackChain<MockProvider> = FallbackChain::new(FallbackConfig::default());

        let result = chain.execute(|_p| async { Ok::<_, &str>("success") }).await;

        assert!(matches!(result, Err(FallbackError::NoProvidersAvailable)));
    }

    #[tokio::test]
    async fn test_disable_provider() {
        let chain: FallbackChain<MockProvider> = FallbackChain::new(FallbackConfig::default());

        let p1 = Arc::new(MockProvider::new("p1", "Provider 1"));
        chain
            .add_provider(p1.clone(), ProviderInfo::new("p1", "Provider 1"))
            .await;

        chain.disable_provider("p1").await;

        let available = chain.available_providers().await;
        assert!(available.is_empty());
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let chain: FallbackChain<MockProvider> = FallbackChain::new(FallbackConfig::default());

        let p1 = Arc::new(MockProvider::new("p1", "Provider 1"));
        let p2 = Arc::new(MockProvider::new("p2", "Provider 2"));
        let p3 = Arc::new(MockProvider::new("p3", "Provider 3"));

        chain
            .add_provider(
                p1.clone(),
                ProviderInfo::new("p1", "Provider 1").with_priority(5),
            )
            .await;
        chain
            .add_provider(
                p2.clone(),
                ProviderInfo::new("p2", "Provider 2").with_priority(10),
            )
            .await;
        chain
            .add_provider(
                p3.clone(),
                ProviderInfo::new("p3", "Provider 3").with_priority(1),
            )
            .await;

        let providers = chain.list_providers().await;
        assert_eq!(providers[0].id, "p2"); // Highest priority
        assert_eq!(providers[1].id, "p1");
        assert_eq!(providers[2].id, "p3"); // Lowest priority
    }

    #[tokio::test]
    async fn test_set_priority() {
        let chain: FallbackChain<MockProvider> = FallbackChain::new(FallbackConfig::default());

        let p1 = Arc::new(MockProvider::new("p1", "Provider 1"));
        chain
            .add_provider(
                p1.clone(),
                ProviderInfo::new("p1", "Provider 1").with_priority(5),
            )
            .await;

        chain.set_priority("p1", 100).await;

        let info = chain.get_provider_info("p1").await.unwrap();
        assert_eq!(info.priority, 100);
    }
}
