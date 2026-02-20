//! Workspace knowledge-base recall.
//!
//! The gateway re-exports the shared implementation from `drbot-core` so all surfaces
//! (gateway chat, OpenClaw, direct CLI, TUI) can stay consistent.

pub use drbot_core::workspace_notes_recall::*;
