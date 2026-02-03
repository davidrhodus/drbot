//! Solana blockchain integration for drbot.
//!
//! This crate provides wallet operations, Jupiter token swaps, and opportunity
//! discovery via DexScreener and GeckoTerminal.
//!
//! # Features
//!
//! - Wallet management (balance queries, transfers)
//! - Jupiter DEX aggregator integration (quotes, swaps)
//! - Market discovery (DexScreener, GeckoTerminal)
//! - DeFi protocol integration (Solend, Marginfi, Kamino, Marinade, Jito)
//! - Risk analysis and correlation detection
//! - Agent-to-agent OTC negotiation
//! - Smart contract upgrade monitoring
//! - Market neutral hedging
//! - Skills and tools for agent use
//! - OSINT marketplace integration for research bounties

pub mod defi;
pub mod discovery;
pub mod hedging;
pub mod monitor;
pub mod osint;
pub mod otc;
pub mod protocols;
pub mod risk;
pub mod skills;
pub mod tools;
pub mod trading;
pub mod validator_intel;
pub mod wallet;

mod config;
mod error;

// Kani formal verification proofs
#[cfg(kani)]
mod kani_proofs;

pub use config::*;
pub use error::*;

use async_trait::async_trait;
use drbot_plugins::{
    Plugin, PluginCapability, PluginContext, PluginEvent, PluginMetadata, PluginResponse,
};
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;
use tracing::info;

/// Solana plugin providing blockchain capabilities.
pub struct SolanaPlugin {
    metadata: PluginMetadata,
    config: SolanaConfig,
    rpc_client: Option<Arc<RpcClient>>,
    jupiter: Option<trading::JupiterClient>,
    dexscreener: Option<discovery::DexScreenerClient>,
    geckoterminal: Option<discovery::GeckoTerminalClient>,
    keypair_manager: Option<wallet::KeypairManager>,
}

impl SolanaPlugin {
    /// Create a new Solana plugin with configuration.
    pub fn new(config: SolanaConfig) -> Self {
        let metadata = PluginMetadata::new("solana", env!("CARGO_PKG_VERSION"))
            .with_description("Solana blockchain integration")
            .with_author("drbot contributors")
            .with_capability(PluginCapability::ToolProvider)
            .with_capability(PluginCapability::NetworkAccess);

        Self {
            metadata,
            config,
            rpc_client: None,
            jupiter: None,
            dexscreener: None,
            geckoterminal: None,
            keypair_manager: None,
        }
    }

    /// Get the RPC client.
    pub fn rpc_client(&self) -> Option<&Arc<RpcClient>> {
        self.rpc_client.as_ref()
    }

    /// Get the Jupiter client.
    pub fn jupiter(&self) -> Option<&trading::JupiterClient> {
        self.jupiter.as_ref()
    }

    /// Get the DexScreener client.
    pub fn dexscreener(&self) -> Option<&discovery::DexScreenerClient> {
        self.dexscreener.as_ref()
    }

    /// Get the GeckoTerminal client.
    pub fn geckoterminal(&self) -> Option<&discovery::GeckoTerminalClient> {
        self.geckoterminal.as_ref()
    }

    /// Get the keypair manager.
    pub fn keypair_manager(&self) -> Option<&wallet::KeypairManager> {
        self.keypair_manager.as_ref()
    }
}

#[async_trait]
impl Plugin for SolanaPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    async fn init(&mut self, _context: &PluginContext) -> drbot_core::Result<()> {
        info!("Initializing Solana plugin");

        // Create RPC client
        let rpc_url = self.config.network.rpc_url();
        self.rpc_client = Some(Arc::new(RpcClient::new(rpc_url.to_string())));

        // Create Jupiter client
        self.jupiter = Some(trading::JupiterClient::new(
            self.config.jupiter_api_url.clone(),
        ));

        // Create discovery clients
        self.dexscreener = Some(discovery::DexScreenerClient::new(
            self.config.dexscreener_api_url.clone(),
        ));
        self.geckoterminal = Some(discovery::GeckoTerminalClient::new(
            self.config.geckoterminal_api_url.clone(),
        ));

        // Create keypair manager if secret key is configured
        if let Some(ref secret_key) = self.config.keypair_secret_key {
            self.keypair_manager = Some(wallet::KeypairManager::new(secret_key.clone()));
        }

        Ok(())
    }

    async fn start(&mut self, _context: &PluginContext) -> drbot_core::Result<()> {
        info!("Starting Solana plugin");
        Ok(())
    }

    async fn stop(&mut self, _context: &PluginContext) -> drbot_core::Result<()> {
        info!("Stopping Solana plugin");
        Ok(())
    }

    async fn handle_event(
        &self,
        _event: &PluginEvent,
        _context: &PluginContext,
    ) -> drbot_core::Result<PluginResponse> {
        Ok(PluginResponse::unhandled())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_metadata() {
        let config = SolanaConfig::default();
        let plugin = SolanaPlugin::new(config);
        let metadata = plugin.metadata();
        assert_eq!(metadata.name, "solana");
        assert!(metadata
            .capabilities
            .contains(&PluginCapability::ToolProvider));
    }
}
