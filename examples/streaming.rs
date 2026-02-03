//! Streaming chat example.
//!
//! Run with:
//! ```
//! ANTHROPIC_API_KEY=your-key cargo run --example streaming
//! ```

use drbot_anthropic::AnthropicProvider;
use drbot_core::message::Message;
use drbot_providers::{ChatOptions, Provider, StreamEvent};
use futures::StreamExt;
use std::io::{self, Write};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Get API key from environment
    let api_key =
        std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY environment variable not set");

    // Create the provider
    let provider = AnthropicProvider::new(api_key);

    // Build conversation messages
    let messages = vec![
        Message::system("You are a helpful assistant."),
        Message::user("Write a short poem about Rust programming."),
    ];

    // Chat options
    let options = ChatOptions {
        model: Some("claude-sonnet-4-20250514".to_string()),
        max_tokens: Some(1024),
        ..Default::default()
    };

    // Start streaming
    println!("Streaming response:\n");
    let mut stream = provider.stream(&messages, options).await?;

    // Process stream events
    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::Start { model } => {
                println!("[Model: {}]\n", model);
            }
            StreamEvent::Delta { content } => {
                // Print content as it streams
                print!("{}", content);
                io::stdout().flush()?;
            }
            StreamEvent::ToolUse { id, name, input } => {
                println!("\n[Tool Call: {} ({})]", name, id);
                println!("Input: {}", serde_json::to_string_pretty(&input)?);
            }
            StreamEvent::Stop { reason, usage } => {
                println!("\n\n[Stopped: {}]", reason);
                if let Some(u) = usage {
                    println!(
                        "Tokens: {} input, {} output",
                        u.input_tokens, u.output_tokens
                    );
                }
            }
            StreamEvent::Error { message } => {
                eprintln!("\nError: {}", message);
            }
        }
    }

    Ok(())
}
