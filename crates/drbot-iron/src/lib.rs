//! Iron workflows for drbot.
//!
//! This crate provides a WASM-first workflow runner intended to reach parity with
//! IronClaw-style "routines" and sandboxed workflow execution.

mod bundle;
mod manifest;
mod runner;
mod tools;

/// The canonical WIT definition for drbot Iron workflows.
pub const IRON_WORKFLOW_WIT: &str = include_str!("../wit/workflow.wit");

pub use bundle::{create_bundle_tar_gz, create_bundle_tar_gz_signed, unpack_bundle_tar_gz};
pub use manifest::{
    IronHttpCapability, IronWorkflowCapabilities, IronWorkflowIntegrity, IronWorkflowManifest,
    IronWorkflowSignature,
};
pub use runner::{IronLoadedWorkflow, IronRunOutput, IronRunner, IronRunnerConfig};
pub use tools::{IronToolHostConfig, IronToolResult};
