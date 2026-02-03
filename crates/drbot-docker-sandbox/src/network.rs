//! Network configuration for sandboxed containers.

use serde::{Deserialize, Serialize};

/// Network mode for containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    /// No network access.
    #[default]
    None,
    /// Default bridge network (can access internet).
    Bridge,
    /// Host network (not recommended for sandboxing).
    Host,
    /// Custom network with filtering.
    Filtered,
}

impl NetworkMode {
    /// Get the Docker network mode string.
    pub fn to_docker_mode(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Bridge => "bridge",
            Self::Host => "host",
            Self::Filtered => "bridge", // Custom handling needed
        }
    }

    /// Check if network access is allowed.
    pub fn allows_network(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Check if internet access is allowed.
    pub fn allows_internet(&self) -> bool {
        matches!(self, Self::Bridge | Self::Host)
    }
}

impl std::fmt::Display for NetworkMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Bridge => write!(f, "bridge"),
            Self::Host => write!(f, "host"),
            Self::Filtered => write!(f, "filtered"),
        }
    }
}

impl std::str::FromStr for NetworkMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" | "disabled" | "off" => Ok(Self::None),
            "bridge" | "default" => Ok(Self::Bridge),
            "host" => Ok(Self::Host),
            "filtered" | "restricted" => Ok(Self::Filtered),
            _ => Err(format!("Unknown network mode: {}", s)),
        }
    }
}

/// Network configuration for sandboxed containers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Network mode.
    #[serde(default)]
    pub mode: NetworkMode,
    /// Allowed domains for filtered mode.
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    /// Allowed IP ranges for filtered mode.
    #[serde(default)]
    pub allowed_ip_ranges: Vec<String>,
    /// Blocked ports.
    #[serde(default)]
    pub blocked_ports: Vec<u16>,
    /// DNS servers to use.
    #[serde(default)]
    pub dns_servers: Vec<String>,
    /// Extra hosts to add to /etc/hosts.
    #[serde(default)]
    pub extra_hosts: Vec<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: NetworkMode::None,
            allowed_domains: Vec::new(),
            allowed_ip_ranges: Vec::new(),
            blocked_ports: Vec::new(),
            dns_servers: Vec::new(),
            extra_hosts: Vec::new(),
        }
    }
}

impl NetworkConfig {
    /// Create a config with no network access.
    pub fn isolated() -> Self {
        Self {
            mode: NetworkMode::None,
            ..Default::default()
        }
    }

    /// Create a config with bridge network.
    pub fn bridged() -> Self {
        Self {
            mode: NetworkMode::Bridge,
            ..Default::default()
        }
    }

    /// Create a filtered network config.
    pub fn filtered() -> Self {
        Self {
            mode: NetworkMode::Filtered,
            ..Default::default()
        }
    }

    /// Add allowed domains.
    pub fn with_allowed_domains(mut self, domains: Vec<String>) -> Self {
        self.allowed_domains = domains;
        self
    }

    /// Add allowed IP ranges.
    pub fn with_allowed_ip_ranges(mut self, ranges: Vec<String>) -> Self {
        self.allowed_ip_ranges = ranges;
        self
    }

    /// Add blocked ports.
    pub fn with_blocked_ports(mut self, ports: Vec<u16>) -> Self {
        self.blocked_ports = ports;
        self
    }

    /// Set DNS servers.
    pub fn with_dns(mut self, servers: Vec<String>) -> Self {
        self.dns_servers = servers;
        self
    }

    /// Add extra hosts.
    pub fn with_extra_hosts(mut self, hosts: Vec<String>) -> Self {
        self.extra_hosts = hosts;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_mode() {
        assert!(!NetworkMode::None.allows_network());
        assert!(NetworkMode::Bridge.allows_network());
        assert!(NetworkMode::Bridge.allows_internet());
        assert!(!NetworkMode::Filtered.allows_internet());
    }

    #[test]
    fn test_network_mode_from_str() {
        assert_eq!("none".parse::<NetworkMode>().unwrap(), NetworkMode::None);
        assert_eq!(
            "bridge".parse::<NetworkMode>().unwrap(),
            NetworkMode::Bridge
        );
        assert_eq!(
            "filtered".parse::<NetworkMode>().unwrap(),
            NetworkMode::Filtered
        );
    }

    #[test]
    fn test_network_config() {
        let config = NetworkConfig::filtered()
            .with_allowed_domains(vec!["pypi.org".to_string()])
            .with_blocked_ports(vec![22, 25]);

        assert_eq!(config.mode, NetworkMode::Filtered);
        assert_eq!(config.allowed_domains, vec!["pypi.org"]);
        assert_eq!(config.blocked_ports, vec![22, 25]);
    }
}
