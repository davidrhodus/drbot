//! Iron workflows for drbot.
//!
//! This crate provides a WASM-first workflow runner intended to reach parity with
//! IronClaw-style "routines" and sandboxed workflow execution.

mod manifest;
mod runner;
mod tools;
mod bundle;

/// The canonical WIT definition for drbot Iron workflows.
pub const IRON_WORKFLOW_WIT: &str = include_str!("../wit/workflow.wit");

pub use manifest::IronWorkflowManifest;
pub use runner::{IronLoadedWorkflow, IronRunner, IronRunnerConfig};
pub use tools::{IronToolHostConfig, IronToolResult};
pub use bundle::{create_bundle_tar_gz, unpack_bundle_tar_gz};
