//! DeFi protocol integration for Solana.
//!
//! This module provides integration with major Solana DeFi protocols
//! including lending (Solend, Marginfi), vaults (Kamino), and staking
//! (Marinade, Jito).

pub mod approval;
pub mod protocols;
pub mod yield_discovery;

pub use approval::*;
pub use protocols::*;
pub use yield_discovery::*;
