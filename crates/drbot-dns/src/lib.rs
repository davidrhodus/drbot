//! DNS utilities for drbot.
//!
//! This crate provides:
//! - DNS resolution
//! - DNS caching
//! - DNS lookup utilities

use async_trait::async_trait;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::net::lookup_host;
use tokio::sync::RwLock;

/// DNS error types.
#[derive(Error, Debug)]
pub enum DnsError {
    #[error("Resolution failed: {0}")]
    ResolutionFailed(String),

    #[error("No addresses found")]
    NoAddresses,

    #[error("Timeout")]
    Timeout,

    #[error("Invalid hostname: {0}")]
    InvalidHostname(String),
}

/// Result type for DNS operations.
pub type Result<T> = std::result::Result<T, DnsError>;

/// DNS record.
#[derive(Debug, Clone)]
pub struct DnsRecord {
    /// IP addresses.
    pub addresses: Vec<IpAddr>,
    /// Time to live.
    pub ttl: Duration,
    /// Lookup time.
    pub lookup_time: Instant,
}

impl DnsRecord {
    /// Check if record is expired.
    pub fn is_expired(&self) -> bool {
        self.lookup_time.elapsed() > self.ttl
    }

    /// Get IPv4 addresses.
    pub fn ipv4_addresses(&self) -> Vec<Ipv4Addr> {
        self.addresses
            .iter()
            .filter_map(|addr| match addr {
                IpAddr::V4(v4) => Some(*v4),
                _ => None,
            })
            .collect()
    }

    /// Get IPv6 addresses.
    pub fn ipv6_addresses(&self) -> Vec<Ipv6Addr> {
        self.addresses
            .iter()
            .filter_map(|addr| match addr {
                IpAddr::V6(v6) => Some(*v6),
                _ => None,
            })
            .collect()
    }
}

/// DNS resolver trait.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Resolve hostname to IP addresses.
    async fn resolve(&self, hostname: &str) -> Result<Vec<IpAddr>>;

    /// Resolve hostname to DNS record.
    async fn resolve_record(&self, hostname: &str) -> Result<DnsRecord>;
}

/// System DNS resolver.
pub struct SystemResolver {
    default_ttl: Duration,
}

impl SystemResolver {
    /// Create new system resolver.
    pub fn new() -> Self {
        Self {
            default_ttl: Duration::from_secs(300),
        }
    }

    /// Set default TTL.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }
}

impl Default for SystemResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Resolver for SystemResolver {
    async fn resolve(&self, hostname: &str) -> Result<Vec<IpAddr>> {
        let addr = format!("{}:0", hostname);
        let addresses: Vec<IpAddr> = lookup_host(&addr)
            .await
            .map_err(|e| DnsError::ResolutionFailed(e.to_string()))?
            .map(|socket_addr| socket_addr.ip())
            .collect();

        if addresses.is_empty() {
            Err(DnsError::NoAddresses)
        } else {
            Ok(addresses)
        }
    }

    async fn resolve_record(&self, hostname: &str) -> Result<DnsRecord> {
        let addresses = self.resolve(hostname).await?;
        Ok(DnsRecord {
            addresses,
            ttl: self.default_ttl,
            lookup_time: Instant::now(),
        })
    }
}

/// Caching DNS resolver.
pub struct CachingResolver<R> {
    resolver: R,
    cache: Arc<RwLock<HashMap<String, DnsRecord>>>,
    negative_ttl: Duration,
}

impl<R: Resolver> CachingResolver<R> {
    /// Create new caching resolver.
    pub fn new(resolver: R) -> Self {
        Self {
            resolver,
            cache: Arc::new(RwLock::new(HashMap::new())),
            negative_ttl: Duration::from_secs(60),
        }
    }

    /// Set negative cache TTL.
    pub fn negative_ttl(mut self, ttl: Duration) -> Self {
        self.negative_ttl = ttl;
        self
    }

    /// Clear cache.
    pub async fn clear(&self) {
        self.cache.write().await.clear();
    }

    /// Remove expired entries.
    pub async fn cleanup(&self) {
        let mut cache = self.cache.write().await;
        cache.retain(|_, record| !record.is_expired());
    }
}

#[async_trait]
impl<R: Resolver> Resolver for CachingResolver<R> {
    async fn resolve(&self, hostname: &str) -> Result<Vec<IpAddr>> {
        let record = self.resolve_record(hostname).await?;
        Ok(record.addresses)
    }

    async fn resolve_record(&self, hostname: &str) -> Result<DnsRecord> {
        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(record) = cache.get(hostname) {
                if !record.is_expired() {
                    return Ok(record.clone());
                }
            }
        }

        // Resolve and cache
        let record = self.resolver.resolve_record(hostname).await?;
        {
            let mut cache = self.cache.write().await;
            cache.insert(hostname.to_string(), record.clone());
        }

        Ok(record)
    }
}

/// Round-robin DNS selector.
pub struct RoundRobinSelector {
    counter: Arc<std::sync::atomic::AtomicUsize>,
}

impl RoundRobinSelector {
    /// Create new selector.
    pub fn new() -> Self {
        Self {
            counter: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Select next address.
    pub fn select(&self, addresses: &[IpAddr]) -> Option<IpAddr> {
        if addresses.is_empty() {
            return None;
        }

        let index = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % addresses.len();
        Some(addresses[index])
    }
}

impl Default for RoundRobinSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// DNS utilities.
pub struct DnsUtils;

impl DnsUtils {
    /// Check if hostname is valid.
    pub fn is_valid_hostname(hostname: &str) -> bool {
        if hostname.is_empty() || hostname.len() > 253 {
            return false;
        }

        for label in hostname.split('.') {
            if label.is_empty() || label.len() > 63 {
                return false;
            }
            if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return false;
            }
            if label.starts_with('-') || label.ends_with('-') {
                return false;
            }
        }

        true
    }

    /// Check if string is IP address.
    pub fn is_ip_address(s: &str) -> bool {
        s.parse::<IpAddr>().is_ok()
    }

    /// Parse host and port.
    pub fn parse_host_port(s: &str, default_port: u16) -> Result<(String, u16)> {
        if let Some(colon_pos) = s.rfind(':') {
            // Check if it's IPv6 address
            if s.starts_with('[') {
                if let Some(bracket_pos) = s.find("]:") {
                    let host = s[1..bracket_pos].to_string();
                    let port: u16 = s[bracket_pos + 2..]
                        .parse()
                        .map_err(|_| DnsError::InvalidHostname("Invalid port".to_string()))?;
                    return Ok((host, port));
                } else if s.ends_with(']') {
                    return Ok((s[1..s.len() - 1].to_string(), default_port));
                }
            }

            let host = s[..colon_pos].to_string();
            let port: u16 = s[colon_pos + 1..]
                .parse()
                .map_err(|_| DnsError::InvalidHostname("Invalid port".to_string()))?;
            Ok((host, port))
        } else {
            Ok((s.to_string(), default_port))
        }
    }

    /// Create socket address from host and port.
    pub async fn to_socket_addr(host: &str, port: u16) -> Result<SocketAddr> {
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(SocketAddr::new(ip, port));
        }

        let resolver = SystemResolver::new();
        let addresses = resolver.resolve(host).await?;
        addresses
            .first()
            .map(|ip| SocketAddr::new(*ip, port))
            .ok_or(DnsError::NoAddresses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_hostname() {
        assert!(DnsUtils::is_valid_hostname("example.com"));
        assert!(DnsUtils::is_valid_hostname("sub.example.com"));
        assert!(DnsUtils::is_valid_hostname("my-site.com"));

        assert!(!DnsUtils::is_valid_hostname(""));
        assert!(!DnsUtils::is_valid_hostname("-invalid.com"));
        assert!(!DnsUtils::is_valid_hostname("invalid-.com"));
    }

    #[test]
    fn test_is_ip_address() {
        assert!(DnsUtils::is_ip_address("192.168.1.1"));
        assert!(DnsUtils::is_ip_address("::1"));
        assert!(!DnsUtils::is_ip_address("example.com"));
    }

    #[test]
    fn test_parse_host_port() {
        assert_eq!(
            DnsUtils::parse_host_port("example.com:8080", 80).unwrap(),
            ("example.com".to_string(), 8080)
        );
        assert_eq!(
            DnsUtils::parse_host_port("example.com", 80).unwrap(),
            ("example.com".to_string(), 80)
        );
        assert_eq!(
            DnsUtils::parse_host_port("[::1]:8080", 80).unwrap(),
            ("::1".to_string(), 8080)
        );
    }

    #[test]
    fn test_dns_record_expired() {
        let record = DnsRecord {
            addresses: vec![IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))],
            ttl: Duration::from_millis(1),
            lookup_time: Instant::now(),
        };

        std::thread::sleep(Duration::from_millis(2));
        assert!(record.is_expired());
    }

    #[test]
    fn test_round_robin() {
        let selector = RoundRobinSelector::new();
        let addresses = vec![
            IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
            IpAddr::V4(Ipv4Addr::new(2, 2, 2, 2)),
            IpAddr::V4(Ipv4Addr::new(3, 3, 3, 3)),
        ];

        let first = selector.select(&addresses);
        let second = selector.select(&addresses);
        let third = selector.select(&addresses);
        let fourth = selector.select(&addresses);

        assert_ne!(first, second);
        assert_eq!(first, fourth); // Wraps around
    }
}
