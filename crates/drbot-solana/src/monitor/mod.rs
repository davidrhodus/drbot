//! On-chain monitoring for smart contract upgrades.
//!
//! This module provides real-time monitoring of Solana programs
//! for upgrades and suspicious changes.

pub mod diff_analyzer;
pub mod program_watcher;
pub mod upgrade_detector;

pub use diff_analyzer::*;
pub use program_watcher::*;
pub use upgrade_detector::*;
