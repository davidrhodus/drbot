//! Multi-provider example showing how to use different AI providers.
//!
//! Run with:
//! ```
//! ANTHROPIC_API_KEY=your-key OPENAI_API_KEY=your-key cargo run --example multi_provider
//! ```

use drbot_anthropic::AnthropicProvider;
use drbot_core::message::Message;
use drbot_openai::OpenAIProvider;
use drbot_providers::{ChatOptions, Provider};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create providers
    let providers: Vec<Arc<dyn Provider>> = vec![
        // Anthropic (if API key available)
        std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .map(|key| Arc::new(AnthropicProvider::new(key)) as Arc<dyn Provider>),
        // OpenAI (if API key available)
        std::env::var("OPENAI_API_KEY")
            .ok()
            .map(|key| Arc::new(OpenAIProvider::new(key)) as Arc<dyn Provider>),
    ]
    .into_iter()
    .flatten()
    .collect();

    if providers.is_empty() {
        eprintln!("No providers configured. Set ANTHROPIC_API_KEY or OPENAI_API_KEY.");
        return Ok(());
    }

    // The question to ask each provider
    let messages = vec![Message::user(
        "In one sentence, what makes you unique as an AI?",
    )];

    let options = ChatOptions {
        max_tokens: Some(100),
        ..Default::default()
    };

    // Ask each provider
    for provider in providers {
        println!("\n=== {} ===", provider.name());

        // List available models
        println!("Available models:");
        for model in provider.models().iter().take(3) {
            println!("  - {} ({})", model.name, model.id);
        }

        // Send the chat request
        match provider.chat(&messages, options.clone()).await {
            Ok(response) => {
                println!("\nResponse: {}", response.content);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
    }

    Ok(())
}
