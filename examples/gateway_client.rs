//! Gateway WebSocket client example.
//!
//! First start the gateway:
//! ```
//! ANTHROPIC_API_KEY=your-key cargo run -- gateway
//! ```
//!
//! Then run this example:
//! ```
//! cargo run --example gateway_client
//! ```

use drbot_protocol::{Request, WsMessage};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = "ws://127.0.0.1:18789/ws";

    println!("Connecting to gateway at {}...", url);

    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();

    println!("Connected!\n");

    // Read the initial connected event
    if let Some(Ok(Message::Text(text))) = read.next().await {
        let msg: WsMessage = serde_json::from_str(&text)?;
        if let WsMessage::Event(event) = msg {
            println!("Received event: {}", event.event_type);
        }
    }

    // Send a ping request
    let ping_request = Request::new(Uuid::new_v4(), "system.ping", serde_json::json!({}));
    let ping_json = serde_json::to_string(&WsMessage::Request(ping_request))?;

    println!("Sending ping...");
    write.send(Message::Text(ping_json.into())).await?;

    // Read the ping response
    if let Some(Ok(Message::Text(text))) = read.next().await {
        let msg: WsMessage = serde_json::from_str(&text)?;
        if let WsMessage::Response(response) = msg {
            println!("Pong! Timestamp: {:?}", response.result);
        }
    }

    // Send a chat message
    let chat_request = Request::new(
        Uuid::new_v4(),
        "chat.send",
        serde_json::json!({
            "message": "Hello! Say 'Hi' back in exactly one word.",
            "stream": true
        }),
    );
    let chat_json = serde_json::to_string(&WsMessage::Request(chat_request))?;

    println!("\nSending chat message...");
    write.send(Message::Text(chat_json.into())).await?;

    // Read streaming response events
    println!("Response: ");
    loop {
        match read.next().await {
            Some(Ok(Message::Text(text))) => {
                let msg: WsMessage = serde_json::from_str(&text)?;
                match msg {
                    WsMessage::Event(event) => match event.event_type.as_str() {
                        "chat.stream.delta" => {
                            if let Some(delta) = event.data.get("delta") {
                                print!("{}", delta.as_str().unwrap_or(""));
                            }
                        }
                        "chat.stream.complete" => {
                            println!("\n\nStream complete!");
                            break;
                        }
                        "chat.stream.error" => {
                            if let Some(error) = event.data.get("error") {
                                eprintln!("\nError: {}", error);
                            }
                            break;
                        }
                        _ => {}
                    },
                    WsMessage::Response(response) => {
                        if response.error.is_some() {
                            eprintln!("Error: {:?}", response.error);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            Some(Ok(Message::Close(_))) => {
                println!("Connection closed");
                break;
            }
            None => break,
            _ => {}
        }
    }

    Ok(())
}
