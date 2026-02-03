//! OpenAI GPT provider for drbot.
//!
//! This crate provides integration with OpenAI's Chat Completions API,
//! including support for streaming responses.

mod api;
mod client;

pub use client::{OpenAIProvider, DEFAULT_BASE_URL, DEFAULT_MODEL};
