//! Basic chat example using the Anthropic provider.
//!
//! Run with:
//! ```
//! ANTHROPIC_API_KEY=your-key cargo run --example basic_chat
//! ```

use drbot_anthropic::AnthropicProvider;
use drbot_core::message::Message;
use drbot_providers::{ChatOptions, Provider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Get API key from environment
    let api_key =
        std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY environment variable not set");

    // Create the provider
    let provider = AnthropicProvider::new(api_key);

    // Build conversation messages
    let messages = vec![
        Message::system("You are a helpful assistant. Be concise."),
        Message::user("What is the capital of France?"),
    ];

    // Chat options
    let options = ChatOptions {
        model: Some("claude-sonnet-4-20250514".to_string()),
        max_tokens: Some(1024),
        ..Default::default()
    };

    // Send the request
    println!("Sending request...\n");
    let response = provider.chat(&messages, options).await?;

    // Print the response
    println!("Response: {}", response.content);

    if let Some(usage) = response.usage {
        println!(
            "\nTokens: {} input, {} output",
            usage.input_tokens, usage.output_tokens
        );
    }

    Ok(())
}
