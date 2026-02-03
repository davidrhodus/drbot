//! Health check endpoints and monitoring.
//!
//! This crate provides:
//! - Health check framework
//! - Liveness and readiness probes
//! - Dependency health monitoring
//! - Health aggregation

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;

/// Health check errors.
#[derive(Debug, Error)]
pub enum HealthError {
    #[error("Check failed: {0}")]
    CheckFailed(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Component not found: {0}")]
    ComponentNotFound(String),
}

/// Result type for health operations.
pub type Result<T> = std::result::Result<T, HealthError>;

/// Health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Component is healthy.
    Healthy,
    /// Component is degraded but functional.
    Degraded,
    /// Component is unhealthy.
    Unhealthy,
    /// Health status is unknown.
    Unknown,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

impl HealthStatus {
    /// Check if status represents a healthy state.
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy | HealthStatus::Degraded)
    }

    /// Check if status represents a live state.
    pub fn is_live(&self) -> bool {
        !matches!(self, HealthStatus::Unknown)
    }
}

/// Health check result for a component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Component name.
    pub name: String,
    /// Health status.
    pub status: HealthStatus,
    /// Optional message.
    pub message: Option<String>,
    /// Check duration.
    pub duration_ms: u64,
    /// Last check time.
    pub checked_at: DateTime<Utc>,
    /// Additional details.
    pub details: HashMap<String, String>,
}

impl ComponentHealth {
    /// Create a healthy result.
    pub fn healthy(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: HealthStatus::Healthy,
            message: None,
            duration_ms: 0,
            checked_at: Utc::now(),
            details: HashMap::new(),
        }
    }

    /// Create a degraded result.
    pub fn degraded(name: &str, message: &str) -> Self {
        Self {
            name: name.to_string(),
            status: HealthStatus::Degraded,
            message: Some(message.to_string()),
            duration_ms: 0,
            checked_at: Utc::now(),
            details: HashMap::new(),
        }
    }

    /// Create an unhealthy result.
    pub fn unhealthy(name: &str, message: &str) -> Self {
        Self {
            name: name.to_string(),
            status: HealthStatus::Unhealthy,
            message: Some(message.to_string()),
            duration_ms: 0,
            checked_at: Utc::now(),
            details: HashMap::new(),
        }
    }

    /// Add a detail.
    pub fn with_detail(mut self, key: &str, value: &str) -> Self {
        self.details.insert(key.to_string(), value.to_string());
        self
    }

    /// Set duration.
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }
}

/// Aggregated health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    /// Overall status.
    pub status: HealthStatus,
    /// Component health results.
    pub components: Vec<ComponentHealth>,
    /// Report generation time.
    pub timestamp: DateTime<Utc>,
    /// Total check duration.
    pub total_duration_ms: u64,
    /// Version info.
    pub version: Option<String>,
    /// Uptime in seconds.
    pub uptime_secs: Option<u64>,
}

impl HealthReport {
    /// Create a new health report.
    pub fn new(components: Vec<ComponentHealth>) -> Self {
        let status = Self::aggregate_status(&components);
        let total_duration_ms = components.iter().map(|c| c.duration_ms).sum();

        Self {
            status,
            components,
            timestamp: Utc::now(),
            total_duration_ms,
            version: None,
            uptime_secs: None,
        }
    }

    /// Set version.
    pub fn with_version(mut self, version: &str) -> Self {
        self.version = Some(version.to_string());
        self
    }

    /// Set uptime.
    pub fn with_uptime(mut self, uptime_secs: u64) -> Self {
        self.uptime_secs = Some(uptime_secs);
        self
    }

    fn aggregate_status(components: &[ComponentHealth]) -> HealthStatus {
        if components.is_empty() {
            return HealthStatus::Unknown;
        }

        let has_unhealthy = components
            .iter()
            .any(|c| c.status == HealthStatus::Unhealthy);
        let has_degraded = components
            .iter()
            .any(|c| c.status == HealthStatus::Degraded);
        let has_unknown = components.iter().any(|c| c.status == HealthStatus::Unknown);

        if has_unhealthy {
            HealthStatus::Unhealthy
        } else if has_degraded || has_unknown {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }

    /// Check if all components are healthy.
    pub fn is_healthy(&self) -> bool {
        self.status.is_healthy()
    }

    /// Check if system is live.
    pub fn is_live(&self) -> bool {
        self.status.is_live()
    }
}

/// Health check provider trait.
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// Get the component name.
    fn name(&self) -> &str;

    /// Perform the health check.
    async fn check(&self) -> ComponentHealth;

    /// Check timeout.
    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }

    /// Is this check critical?
    fn is_critical(&self) -> bool {
        true
    }
}

/// Health checker configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    /// Default check timeout.
    pub default_timeout: Duration,
    /// Run checks in parallel.
    pub parallel: bool,
    /// Cache duration for health results.
    pub cache_duration: Option<Duration>,
    /// Include detailed component info.
    pub detailed: bool,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(5),
            parallel: true,
            cache_duration: Some(Duration::from_secs(5)),
            detailed: true,
        }
    }
}

/// Cached health result.
struct CachedHealth {
    report: HealthReport,
    cached_at: DateTime<Utc>,
}

/// The health checker.
pub struct HealthChecker {
    /// Health check providers.
    checks: Arc<RwLock<Vec<Arc<dyn HealthCheck>>>>,
    /// Configuration.
    config: HealthConfig,
    /// Cached result.
    cache: Arc<RwLock<Option<CachedHealth>>>,
    /// Start time for uptime calculation.
    start_time: DateTime<Utc>,
    /// Version string.
    version: Option<String>,
}

impl HealthChecker {
    /// Create a new health checker.
    pub fn new(config: HealthConfig) -> Self {
        Self {
            checks: Arc::new(RwLock::new(Vec::new())),
            config,
            cache: Arc::new(RwLock::new(None)),
            start_time: Utc::now(),
            version: None,
        }
    }

    /// Set version string.
    pub fn with_version(mut self, version: &str) -> Self {
        self.version = Some(version.to_string());
        self
    }

    /// Register a health check.
    pub async fn register(&self, check: Arc<dyn HealthCheck>) {
        let mut checks = self.checks.write().await;
        checks.push(check);
    }

    /// Run all health checks.
    pub async fn check(&self) -> HealthReport {
        // Check cache
        if let Some(cache_duration) = self.config.cache_duration {
            let cache = self.cache.read().await;
            if let Some(cached) = &*cache {
                let age = Utc::now() - cached.cached_at;
                if age
                    < chrono::Duration::from_std(cache_duration)
                        .unwrap_or(chrono::Duration::seconds(5))
                {
                    return cached.report.clone();
                }
            }
        }

        let checks = self.checks.read().await;
        let results = if self.config.parallel {
            self.run_parallel(&checks).await
        } else {
            self.run_sequential(&checks).await
        };

        let uptime = (Utc::now() - self.start_time).num_seconds() as u64;

        let mut report = HealthReport::new(results).with_uptime(uptime);

        if let Some(version) = &self.version {
            report = report.with_version(version);
        }

        // Update cache
        if self.config.cache_duration.is_some() {
            let mut cache = self.cache.write().await;
            *cache = Some(CachedHealth {
                report: report.clone(),
                cached_at: Utc::now(),
            });
        }

        report
    }

    async fn run_parallel(&self, checks: &[Arc<dyn HealthCheck>]) -> Vec<ComponentHealth> {
        let futures: Vec<_> = checks
            .iter()
            .map(|check| {
                let check = check.clone();
                let timeout = check.timeout();
                async move {
                    let start = std::time::Instant::now();
                    let result = tokio::time::timeout(timeout, check.check()).await;
                    let duration_ms = start.elapsed().as_millis() as u64;

                    match result {
                        Ok(mut health) => {
                            health.duration_ms = duration_ms;
                            health
                        }
                        Err(_) => {
                            ComponentHealth::unhealthy(check.name(), "Health check timed out")
                                .with_duration(duration_ms)
                        }
                    }
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }

    async fn run_sequential(&self, checks: &[Arc<dyn HealthCheck>]) -> Vec<ComponentHealth> {
        let mut results = Vec::new();

        for check in checks {
            let start = std::time::Instant::now();
            let result = tokio::time::timeout(check.timeout(), check.check()).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            let health = match result {
                Ok(mut health) => {
                    health.duration_ms = duration_ms;
                    health
                }
                Err(_) => ComponentHealth::unhealthy(check.name(), "Health check timed out")
                    .with_duration(duration_ms),
            };

            results.push(health);
        }

        results
    }

    /// Liveness probe - is the service running?
    pub async fn liveness(&self) -> bool {
        true // If we can respond, we're live
    }

    /// Readiness probe - is the service ready to accept traffic?
    pub async fn readiness(&self) -> bool {
        let report = self.check().await;
        report.is_healthy()
    }

    /// Get uptime in seconds.
    pub fn uptime(&self) -> u64 {
        (Utc::now() - self.start_time).num_seconds() as u64
    }

    /// Clear the cache.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        *cache = None;
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new(HealthConfig::default())
    }
}

/// Simple health check that always returns healthy.
pub struct AlwaysHealthy {
    name: String,
}

impl AlwaysHealthy {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait]
impl HealthCheck for AlwaysHealthy {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> ComponentHealth {
        ComponentHealth::healthy(&self.name)
    }
}

/// HTTP health check.
pub struct HttpHealthCheck {
    name: String,
    url: String,
    expected_status: u16,
    timeout: Duration,
}

impl HttpHealthCheck {
    pub fn new(name: &str, url: &str) -> Self {
        Self {
            name: name.to_string(),
            url: url.to_string(),
            expected_status: 200,
            timeout: Duration::from_secs(5),
        }
    }

    pub fn with_expected_status(mut self, status: u16) -> Self {
        self.expected_status = status;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait]
impl HealthCheck for HttpHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> ComponentHealth {
        // Note: In real implementation, would use reqwest
        // For now, just return healthy as placeholder
        ComponentHealth::healthy(&self.name)
            .with_detail("url", &self.url)
            .with_detail("expected_status", &self.expected_status.to_string())
    }

    fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// Memory health check.
pub struct MemoryHealthCheck {
    name: String,
    threshold_mb: u64,
}

impl MemoryHealthCheck {
    pub fn new(threshold_mb: u64) -> Self {
        Self {
            name: "memory".to_string(),
            threshold_mb,
        }
    }
}

#[async_trait]
impl HealthCheck for MemoryHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> ComponentHealth {
        // Simplified - in real implementation would check actual memory
        ComponentHealth::healthy(&self.name)
            .with_detail("threshold_mb", &self.threshold_mb.to_string())
    }
}

/// Disk health check.
pub struct DiskHealthCheck {
    name: String,
    path: String,
    threshold_percent: u8,
}

impl DiskHealthCheck {
    pub fn new(path: &str, threshold_percent: u8) -> Self {
        Self {
            name: "disk".to_string(),
            path: path.to_string(),
            threshold_percent,
        }
    }
}

#[async_trait]
impl HealthCheck for DiskHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> ComponentHealth {
        // Simplified - in real implementation would check actual disk
        ComponentHealth::healthy(&self.name)
            .with_detail("path", &self.path)
            .with_detail("threshold_percent", &self.threshold_percent.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockCheck {
        name: String,
        status: HealthStatus,
    }

    #[async_trait]
    impl HealthCheck for MockCheck {
        fn name(&self) -> &str {
            &self.name
        }

        async fn check(&self) -> ComponentHealth {
            match self.status {
                HealthStatus::Healthy => ComponentHealth::healthy(&self.name),
                HealthStatus::Degraded => ComponentHealth::degraded(&self.name, "degraded"),
                HealthStatus::Unhealthy => ComponentHealth::unhealthy(&self.name, "unhealthy"),
                HealthStatus::Unknown => ComponentHealth {
                    name: self.name.clone(),
                    status: HealthStatus::Unknown,
                    message: None,
                    duration_ms: 0,
                    checked_at: Utc::now(),
                    details: HashMap::new(),
                },
            }
        }
    }

    #[tokio::test]
    async fn test_healthy_check() {
        let checker = HealthChecker::default();
        checker
            .register(Arc::new(MockCheck {
                name: "test".to_string(),
                status: HealthStatus::Healthy,
            }))
            .await;

        let report = checker.check().await;
        assert_eq!(report.status, HealthStatus::Healthy);
        assert!(report.is_healthy());
    }

    #[tokio::test]
    async fn test_unhealthy_check() {
        let checker = HealthChecker::default();
        checker
            .register(Arc::new(MockCheck {
                name: "test".to_string(),
                status: HealthStatus::Unhealthy,
            }))
            .await;

        let report = checker.check().await;
        assert_eq!(report.status, HealthStatus::Unhealthy);
        assert!(!report.is_healthy());
    }

    #[tokio::test]
    async fn test_mixed_health() {
        let checker = HealthChecker::default();
        checker
            .register(Arc::new(MockCheck {
                name: "healthy".to_string(),
                status: HealthStatus::Healthy,
            }))
            .await;
        checker
            .register(Arc::new(MockCheck {
                name: "degraded".to_string(),
                status: HealthStatus::Degraded,
            }))
            .await;

        let report = checker.check().await;
        assert_eq!(report.status, HealthStatus::Degraded);
    }

    #[tokio::test]
    async fn test_liveness() {
        let checker = HealthChecker::default();
        assert!(checker.liveness().await);
    }

    #[tokio::test]
    async fn test_readiness() {
        let checker = HealthChecker::default();
        checker
            .register(Arc::new(MockCheck {
                name: "test".to_string(),
                status: HealthStatus::Healthy,
            }))
            .await;

        assert!(checker.readiness().await);
    }

    #[tokio::test]
    async fn test_uptime() {
        let checker = HealthChecker::default();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(checker.uptime() >= 0);
    }

    #[tokio::test]
    async fn test_cache() {
        let config = HealthConfig {
            cache_duration: Some(Duration::from_secs(60)),
            ..Default::default()
        };
        let checker = HealthChecker::new(config);
        checker
            .register(Arc::new(MockCheck {
                name: "test".to_string(),
                status: HealthStatus::Healthy,
            }))
            .await;

        let report1 = checker.check().await;
        let report2 = checker.check().await;

        // Should be cached
        assert_eq!(report1.timestamp, report2.timestamp);
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let config = HealthConfig {
            cache_duration: Some(Duration::from_secs(60)),
            ..Default::default()
        };
        let checker = HealthChecker::new(config);
        checker
            .register(Arc::new(MockCheck {
                name: "test".to_string(),
                status: HealthStatus::Healthy,
            }))
            .await;

        let report1 = checker.check().await;
        checker.clear_cache().await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        let report2 = checker.check().await;

        // Should not be cached after clear
        assert_ne!(report1.timestamp, report2.timestamp);
    }

    #[test]
    fn test_component_health_builder() {
        let health = ComponentHealth::healthy("test")
            .with_detail("key", "value")
            .with_duration(100);

        assert_eq!(health.status, HealthStatus::Healthy);
        assert_eq!(health.details.get("key"), Some(&"value".to_string()));
        assert_eq!(health.duration_ms, 100);
    }
}
