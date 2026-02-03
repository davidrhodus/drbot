//! Bridge configuration for mobile devices.

use serde::{Deserialize, Serialize};

/// Bridge configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    /// Port to listen on.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Host to bind to.
    #[serde(default = "default_host")]
    pub host: String,
    /// Enable TLS.
    #[serde(default)]
    pub tls_enabled: bool,
    /// TLS certificate path.
    pub tls_cert: Option<String>,
    /// TLS key path.
    pub tls_key: Option<String>,
    /// Authentication token (optional).
    pub auth_token: Option<String>,
    /// Maximum concurrent connections.
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
}

fn default_port() -> u16 {
    8765
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_max_connections() -> usize {
    10
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            host: default_host(),
            tls_enabled: false,
            tls_cert: None,
            tls_key: None,
            auth_token: None,
            max_connections: default_max_connections(),
        }
    }
}

/// Mobile bridge server for accepting device connections.
pub struct MobileBridge {
    /// Configuration.
    config: BridgeConfig,
    /// Running state.
    running: bool,
}

impl MobileBridge {
    /// Create a new mobile bridge.
    pub fn new(config: BridgeConfig) -> Self {
        Self {
            config,
            running: false,
        }
    }

    /// Start the bridge server.
    pub async fn start(&mut self) -> crate::Result<()> {
        if self.running {
            return Ok(());
        }

        // In a real implementation, this would start a WebSocket server
        self.running = true;

        tracing::info!(
            host = %self.config.host,
            port = self.config.port,
            "Mobile bridge started"
        );

        Ok(())
    }

    /// Stop the bridge server.
    pub async fn stop(&mut self) -> crate::Result<()> {
        if !self.running {
            return Ok(());
        }

        self.running = false;
        tracing::info!("Mobile bridge stopped");

        Ok(())
    }

    /// Check if bridge is running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Get the bridge URL.
    pub fn url(&self) -> String {
        let scheme = if self.config.tls_enabled { "wss" } else { "ws" };
        format!("{}://{}:{}", scheme, self.config.host, self.config.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_config_default() {
        let config = BridgeConfig::default();
        assert_eq!(config.port, 8765);
        assert_eq!(config.host, "0.0.0.0");
        assert!(!config.tls_enabled);
    }

    #[tokio::test]
    async fn test_mobile_bridge() {
        let config = BridgeConfig::default();
        let mut bridge = MobileBridge::new(config);

        assert!(!bridge.is_running());

        bridge.start().await.unwrap();
        assert!(bridge.is_running());
        assert_eq!(bridge.url(), "ws://0.0.0.0:8765");

        bridge.stop().await.unwrap();
        assert!(!bridge.is_running());
    }
}
