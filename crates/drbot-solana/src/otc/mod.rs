//! Agent-to-Agent OTC negotiation.
//!
//! This module provides peer-to-peer trade negotiation between agents
//! with trustless escrow mechanisms.

pub mod a2a;
pub mod desk;
pub mod escrow;
pub mod negotiation;
pub mod protocol;
pub mod runner;
pub mod settlement;
pub mod trader;
pub mod watcher;

pub use a2a::*;
pub use desk::*;
pub use escrow::*;
pub use negotiation::*;
pub use protocol::*;
pub use runner::*;
pub use settlement::*;
pub use trader::*;
pub use watcher::*;
