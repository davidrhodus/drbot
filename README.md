# drbot

A personal AI assistant written in Rust. Multi-channel, multi-provider, highly extensible.

## Features

- **Multiple AI Providers**: Anthropic Claude, OpenAI GPT, AWS Bedrock, Ollama (local models)
- **Multiple Channels**: WhatsApp, Telegram, Discord, Slack, Signal, iMessage, Matrix, WebChat
- **Streaming Responses**: Real-time token streaming with tool/function calling support
- **Session Management**: Persistent conversations with SQLite storage
- **Memory System**: Vector embeddings for semantic search across conversation history
- **Plugin System**: WASM-based plugins for extensibility
- **Browser Automation**: Chromium DevTools Protocol integration
- **Scheduled Tasks**: Cron-based job scheduling
- **Terminal UI**: Full-featured TUI chat interface
- **CLI**: Command-line interface for all operations

## Installation

### From Source

```bash
git clone https://github.com/david/drbot.git
cd drbot
cargo build --release
```

The binary will be at `target/release/drbot`.

### Requirements

- Rust 1.75+ (for building)
- Node.js 18+ (for WhatsApp Baileys bridge)
- macOS (for iMessage support)

## Quick Start

### Interactive Chat

```bash
# Set your API key
export ANTHROPIC_API_KEY=your-api-key

# Start interactive chat
drbot chat

# Or with a single message
drbot chat -M "What is the capital of France?"
```

### Terminal UI

```bash
drbot tui
```

### Gateway Server

```bash
# Start the WebSocket gateway
drbot gateway

# Or with custom host/port
drbot gateway -H 0.0.0.0 -p 8080
```

OpenClaw agent bash restrictions can be relaxed at startup:

```bash
drbot gateway --openclaw-agent-bash-allowlist "git,cargo,rg,npm,npx,pnpm,node"
# or (dangerous)
drbot gateway --openclaw-agent-bash-allow-all
```

## CLI Commands

```
drbot                      # Start gateway (default)
drbot wizard               # Interactive setup wizard
drbot gateway              # Start WebSocket gateway server
drbot chat                 # Interactive chat with AI
drbot tui                  # Terminal UI chat interface
drbot config               # Show current configuration
drbot doctor               # Run health checks
```

### Setup Wizard

Run the interactive setup wizard to configure drbot:

```bash
drbot wizard
```

The wizard will guide you through:
- Configuring AI providers (Anthropic, OpenAI, Ollama)
- Setting API keys (with environment variable detection)
- Choosing default models
- Gateway server settings
- Saving the configuration file

### Chat Options

```
drbot chat [OPTIONS]

Options:
  -p, --provider <PROVIDER> Provider to use (anthropic, openai, ollama, auto)
  -m, --model <MODEL>      Model to use (e.g., claude-sonnet-4-20250514)
  -s, --system <PROMPT>    System prompt
      --skill-url <URL>    Load an OpenClaw-style SKILL.md from a URL (and linked relative docs)
      --agent              Enable tool use (bash, read/write files, search)
      --root <PATH>        Root directory for tool access (defaults to current directory)
  -M, --message <MSG>      Single message (non-interactive)
      --no-stream          Disable streaming
```

## Configuration

drbot looks for configuration in:
1. `~/.config/drbot/config.toml` (Linux/macOS)
2. `%APPDATA%\drbot\config.toml` (Windows)

Example configuration:

```toml
[gateway]
host = "127.0.0.1"
port = 18789

[providers]
default_provider = "anthropic"

[providers.anthropic]
api_key = "sk-ant-..."
default_model = "claude-sonnet-4-20250514"

[providers.openai]
api_key = "sk-..."
default_model = "gpt-4o"

[providers.ollama]
url = "http://localhost:11434"
default_model = "llama3.2"

[storage]
database_path = "~/.local/share/drbot/drbot.db"
media_path = "~/.local/share/drbot/media"
```

Environment variables override config file values:
- `ANTHROPIC_API_KEY`
- `OPENAI_API_KEY`

## Architecture

```
drbot/
├── src/main.rs              # Binary entry point
├── crates/
│   ├── drbot-core/          # Core types, traits, config
│   ├── drbot-gateway/       # WebSocket gateway server
│   ├── drbot-protocol/      # Gateway protocol definitions
│   ├── drbot-providers/     # AI provider abstraction
│   ├── drbot-anthropic/     # Anthropic Claude
│   ├── drbot-openai/        # OpenAI GPT
│   ├── drbot-bedrock/       # AWS Bedrock
│   ├── drbot-ollama/        # Local Ollama
│   ├── drbot-channels/      # Channel abstraction
│   ├── drbot-whatsapp/      # WhatsApp via Baileys
│   ├── drbot-telegram/      # Telegram Bot API
│   ├── drbot-discord/       # Discord gateway
│   ├── drbot-slack/         # Slack Bolt
│   ├── drbot-signal/        # Signal protocol
│   ├── drbot-imessage/      # iMessage (macOS)
│   ├── drbot-matrix/        # Matrix protocol
│   ├── drbot-webchat/       # WebChat interface
│   ├── drbot-sessions/      # Session management
│   ├── drbot-memory/        # Vector memory storage
│   ├── drbot-browser/       # Browser automation
│   ├── drbot-cron/          # Scheduled tasks
│   ├── drbot-hooks/         # Hook system
│   ├── drbot-plugins/       # WASM plugin runtime
│   ├── drbot-media/         # Media processing
│   ├── drbot-tui/           # Terminal UI
│   └── drbot-cli/           # CLI commands
```

## Gateway Protocol

The gateway exposes a WebSocket API at `ws://localhost:18789/ws`.

drbot also exposes an OpenClaw Gateway v3 compatible endpoint at `ws://localhost:18789/openclaw/ws`
for interoperability with OpenClaw clients (Control UI, nodes).

### Methods

| Method | Description |
|--------|-------------|
| `auth.login` | Authenticate with token |
| `chat.send` | Send a message to AI |
| `session.create` | Create a new session |
| `session.list` | List sessions |
| `provider.list` | List available providers |
| `system.ping` | Health check |
| `system.info` | Server information |

### Example: Sending a Message

```json
{
  "jsonrpc": "2.0",
  "id": "uuid",
  "method": "chat.send",
  "params": {
    "message": "Hello!",
    "stream": true
  }
}
```

### OpenClaw Notes

- Heartbeats (`set-heartbeats`, `wake`) follow OpenClaw semantics (they run `HEARTBEAT.md`, not WS keepalives).
- Outbound `send` / `poll` are approval-gated by default via `exec.approval.*`; set `DRBOT_OPENCLAW_SEND_WRITE=1`
  to bypass approvals.
- OpenClaw agent runs include a restricted `bash` tool by default; set `DRBOT_OPENCLAW_AGENT_BASH_ALLOWLIST`
  (comma-separated) or `DRBOT_OPENCLAW_AGENT_BASH_ALLOW_ALL=1` to relax it.

## Providers

### Anthropic Claude

```rust
use drbot_anthropic::AnthropicProvider;
use drbot_providers::Provider;

let provider = AnthropicProvider::new("your-api-key")
    .with_default_model("claude-sonnet-4-20250514");

let response = provider.chat(&messages, options).await?;
```

### OpenAI

```rust
use drbot_openai::OpenAIProvider;

let provider = OpenAIProvider::new("your-api-key")
    .with_default_model("gpt-4o");
```

### Ollama (Local)

```rust
use drbot_ollama::OllamaProvider;

let provider = OllamaProvider::new()
    .with_url("http://localhost:11434")
    .with_default_model("llama3.2");
```

## Channels

### WebChat

Built-in web interface for testing:

```rust
use drbot_webchat::WebChatServer;

let server = WebChatServer::new()
    .with_port(8080);
server.run().await?;
```

### Telegram

```rust
use drbot_telegram::TelegramChannel;

let channel = TelegramChannel::new("bot-token");
channel.connect().await?;
```

### WhatsApp

Requires the Baileys Node.js bridge:

```rust
use drbot_whatsapp::WhatsAppChannel;

let channel = WhatsAppChannel::new("ws://localhost:3001")
    .with_session_dir(".whatsapp")
    .with_qr_callback(|qr| println!("Scan QR: {}", qr));

channel.connect().await?;
```

## Development

### Building

```bash
cargo build
```

### Testing

```bash
cargo test --workspace
```

### Running with Logging

```bash
RUST_LOG=debug cargo run -- gateway
```

## License

MIT

## Contributing

Contributions welcome! Please read the contributing guidelines first.
# drbot
