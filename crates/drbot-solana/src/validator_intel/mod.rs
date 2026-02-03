//! Validator intelligence (validator-intel).
//!
//! This module collects and joins information from Solana RPC endpoints:
//! - `getVoteAccounts` (stake, commission, delinquency, vote stats)
//! - `getClusterNodes` (gossip/TPU/RPC contact info, version, shred version)
//! - optionally `getBlockProduction` (leader slots vs blocks produced)
//!
//! The result is a structured view of validators suitable for staking heuristics,
//! monitoring, and agent reasoning.

pub mod analytics;
pub mod client;
pub mod scoring;
pub mod types;

pub use analytics::*;
pub use client::*;
pub use scoring::*;
pub use types::*;
