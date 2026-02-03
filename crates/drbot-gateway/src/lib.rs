//! WebSocket gateway server for drbot.
//!
//! This crate provides the main gateway server that handles WebSocket connections
//! from clients and routes messages to the appropriate handlers.

mod agentwallet;
mod channel_manager;
mod client;
mod colosseum;
mod moltbook;
mod openclaw;
mod openclaw_agent_tools;
mod openclaw_exec_approvals;
mod openclaw_heartbeat;
mod openclaw_skills;
mod router;
mod server;
mod state;

pub use server::{Gateway, GatewayBuilder};
pub use state::GatewayState;

/// Re-export protocol types for convenience.
pub use drbot_protocol;
