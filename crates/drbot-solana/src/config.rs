//! Configuration for Solana integration.

use serde::{Deserialize, Serialize};

/// Solana network configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SolanaNetwork {
    /// Mainnet-beta.
    Mainnet,
    /// Devnet.
    Devnet,
    /// Testnet.
    Testnet,
    /// Localnet.
    Localnet,
    /// Custom RPC URL.
    Custom(String),
}

impl SolanaNetwork {
    /// Get the RPC URL for this network.
    pub fn rpc_url(&self) -> &str {
        match self {
            Self::Mainnet => "https://api.mainnet-beta.solana.com",
            Self::Devnet => "https://api.devnet.solana.com",
            Self::Testnet => "https://api.testnet.solana.com",
            Self::Localnet => "http://localhost:8899",
            Self::Custom(url) => url,
        }
    }

    /// Check if this is mainnet.
    pub fn is_mainnet(&self) -> bool {
        matches!(self, Self::Mainnet)
    }
}

impl Default for SolanaNetwork {
    fn default() -> Self {
        Self::Devnet
    }
}

/// Solana plugin configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SolanaConfig {
    /// Solana network to connect to.
    pub network: SolanaNetwork,

    /// Secret key name in drbot-secrets for the keypair.
    /// The secret should contain the base58-encoded private key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keypair_secret_key: Option<String>,

    /// Jupiter API URL.
    pub jupiter_api_url: String,

    /// DexScreener API URL.
    pub dexscreener_api_url: String,

    /// GeckoTerminal API URL.
    pub geckoterminal_api_url: String,

    /// Default slippage in basis points (100 = 1%).
    pub default_slippage_bps: u16,

    /// Enable transaction simulation before execution.
    pub simulate_before_execute: bool,

    /// Confirmation timeout in seconds.
    pub confirmation_timeout_secs: u64,

    /// Maximum retries for transactions.
    pub max_retries: u32,
}

impl Default for SolanaConfig {
    fn default() -> Self {
        Self {
            network: SolanaNetwork::default(),
            keypair_secret_key: None,
            jupiter_api_url: "https://quote-api.jup.ag/v6".to_string(),
            dexscreener_api_url: "https://api.dexscreener.com".to_string(),
            geckoterminal_api_url: "https://api.geckoterminal.com/api/v2".to_string(),
            default_slippage_bps: 50, // 0.5%
            simulate_before_execute: true,
            confirmation_timeout_secs: 60,
            max_retries: 3,
        }
    }
}

impl SolanaConfig {
    /// Create a mainnet configuration.
    pub fn mainnet() -> Self {
        Self {
            network: SolanaNetwork::Mainnet,
            ..Default::default()
        }
    }

    /// Create a devnet configuration.
    pub fn devnet() -> Self {
        Self {
            network: SolanaNetwork::Devnet,
            ..Default::default()
        }
    }

    /// Set the keypair secret key.
    pub fn with_keypair_secret(mut self, secret_key: &str) -> Self {
        self.keypair_secret_key = Some(secret_key.to_string());
        self
    }

    /// Set the slippage.
    pub fn with_slippage_bps(mut self, bps: u16) -> Self {
        self.default_slippage_bps = bps;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_rpc_urls() {
        assert!(SolanaNetwork::Mainnet.rpc_url().contains("mainnet"));
        assert!(SolanaNetwork::Devnet.rpc_url().contains("devnet"));
        assert!(SolanaNetwork::Testnet.rpc_url().contains("testnet"));
        assert!(SolanaNetwork::Localnet.rpc_url().contains("localhost"));

        let custom = SolanaNetwork::Custom("https://my-rpc.com".to_string());
        assert_eq!(custom.rpc_url(), "https://my-rpc.com");
    }

    #[test]
    fn test_config_defaults() {
        let config = SolanaConfig::default();
        assert!(matches!(config.network, SolanaNetwork::Devnet));
        assert_eq!(config.default_slippage_bps, 50);
        assert!(config.simulate_before_execute);
    }

    #[test]
    fn test_config_builder() {
        let config = SolanaConfig::mainnet()
            .with_keypair_secret("my-wallet")
            .with_slippage_bps(100);

        assert!(config.network.is_mainnet());
        assert_eq!(config.keypair_secret_key, Some("my-wallet".to_string()));
        assert_eq!(config.default_slippage_bps, 100);
    }
}
