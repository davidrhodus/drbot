//! Iron workflows for drbot.
//!
//! This crate provides a WASM-first workflow runner intended to reach parity with
//! IronClaw-style "routines" and sandboxed workflow execution.

mod manifest;
mod runner;
mod tools;

/// The canonical WIT definition for drbot Iron workflows.
pub const IRON_WORKFLOW_WIT: &str = include_str!("../wit/workflow.wit");

pub use manifest::IronWorkflowManifest;
pub use runner::{IronRunner, IronRunnerConfig};
pub use tools::{IronToolHostConfig, IronToolResult};
