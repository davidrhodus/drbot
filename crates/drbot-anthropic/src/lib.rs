//! Anthropic Claude provider for drbot.
//!
//! This crate provides integration with Anthropic's Claude API.

mod api;
mod client;

pub use client::AnthropicProvider;

/// Default Anthropic API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Current API version.
pub const API_VERSION: &str = "2023-06-01";
