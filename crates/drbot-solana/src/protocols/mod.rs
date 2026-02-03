//! DeFi protocol integrations.
//!
//! This module provides clients for various Solana DeFi protocols:
//! - Pyth: Price oracle feeds
//! - Drift: Perpetual futures trading
//! - Raydium: AMM liquidity pools
//! - Meteora: Dynamic liquidity market maker
//! - NFT: NFT marketplace integrations (Tensor, Magic Eden)

pub mod drift;
pub mod meteora;
pub mod nft;
pub mod pyth;
pub mod raydium;
pub mod said;

pub use drift::*;
pub use meteora::*;
pub use nft::*;
pub use pyth::*;
pub use raydium::*;
pub use said::*;
