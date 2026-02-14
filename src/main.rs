//! drbot - A personal AI assistant
//!
//! This is the main entry point for the drbot binary.

use anyhow::Result;
use base64::Engine as _;
use clap::{Parser, Subcommand};
use clap_complete::Shell;
use drbot_anthropic::AnthropicProvider;
use drbot_context::{ContextConfig, ContextManager};
use drbot_core::message::Message;
use drbot_core::session::Session;
use drbot_core::Config;
use drbot_gateway::Gateway;
use drbot_personas::{Persona, PersonaRegistry, PersonaStyle, PersonaTrait};
use drbot_providers::{ChatOptions, CliProvider, Provider, StreamEvent};
use drbot_sessions::{ListOptions, SessionStore, SqliteSessionStore};
use drbot_tui::AppConfig;
use futures::StreamExt;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "drbot")]
#[command(author, version, about = "A personal AI assistant", long_about = None)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, global = true)]
    config: Option<String>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway server
    Gateway {
        /// Host to bind to
        #[arg(short = 'H', long)]
        host: Option<String>,

        /// Port to listen on
        #[arg(short, long)]
        port: Option<u16>,

        /// Allow all bash commands for OpenClaw agent runs (dangerous)
        #[arg(long, default_value_t = false)]
        openclaw_agent_bash_allow_all: bool,

        /// Comma-separated allowlist of bash command prefixes for OpenClaw agent runs
        #[arg(long)]
        openclaw_agent_bash_allowlist: Option<String>,
    },

    /// Interactive chat with AI
    Chat {
        /// Provider to use (auto, anthropic/claude, openai/gpt, ollama/local, claude-cli, codex-cli)
        #[arg(short, long)]
        provider: Option<String>,

        /// Model to use (e.g., claude-sonnet-4-20250514, gpt-4o)
        #[arg(short, long)]
        model: Option<String>,

        /// System prompt
        #[arg(short, long)]
        system: Option<String>,

        /// Load an OpenClaw-style SKILL.md from a URL (and linked relative docs)
        #[arg(long)]
        skill_url: Option<String>,

        /// Enable Codex-like tool use (bash, read/write files, search)
        #[arg(long, alias = "tools")]
        agent: bool,

        /// Auto-approve tool use (use with --agent)
        #[arg(short = 'y', long)]
        yes: bool,

        /// Auto-approve bash commands with these prefixes (comma-separated). Use with --agent -y.
        /// This list is additive to the default safe prefixes.
        #[arg(long)]
        bash_auto_approve_prefixes: Option<String>,

        /// Override the bash auto-approve allowlist entirely (comma-separated). Use with --agent -y.
        #[arg(long)]
        bash_auto_approve_allowlist: Option<String>,

        /// Auto-approve any bash command (still blocks rm/sudo/etc). Use with --agent -y. (Dangerous)
        #[arg(long, default_value_t = false)]
        bash_auto_approve_all: bool,

        /// Strict agent mode: if the assistant gives instructions but no tool calls, reprompt for tools.
        #[arg(long, default_value_t = false)]
        agent_strict: bool,

        /// Root directory for tool access (defaults to current directory)
        #[arg(long)]
        root: Option<String>,

        /// Maximum tool/LLM back-and-forth rounds per user message
        #[arg(long, default_value_t = 10)]
        max_tool_rounds: usize,

        /// Single message (non-interactive mode)
        #[arg(short = 'M', long)]
        message: Option<String>,

        /// Read the single message from a file (or '-' for stdin)
        #[arg(long)]
        message_file: Option<String>,

        /// Disable streaming
        #[arg(long)]
        no_stream: bool,

        /// Resume a specific session by ID
        #[arg(long)]
        session: Option<String>,

        /// Start a new session (don't resume last)
        #[arg(long, short = 'n')]
        new_session: bool,

        /// List recent sessions
        #[arg(long)]
        list_sessions: bool,

        /// Session title for new sessions
        #[arg(long)]
        title: Option<String>,

        /// Use a persona (e.g., concise, professional, creative)
        #[arg(long)]
        persona: Option<String>,

        /// List available personas
        #[arg(long)]
        list_personas: bool,

        /// Context window size in tokens (default: 100000)
        #[arg(long)]
        context_size: Option<usize>,
    },

    /// Terminal UI chat interface
    Tui {
        /// Provider to use (anthropic, openai, ollama)
        #[arg(short, long)]
        provider: Option<String>,

        /// Model to use
        #[arg(short, long)]
        model: Option<String>,

        /// System prompt
        #[arg(short, long)]
        system: Option<String>,
    },

    /// Manage channels
    Channels {
        #[command(subcommand)]
        action: ChannelsAction,
    },

    /// Manage OpenClaw-style skills (local + configured remote skills)
    Skills {
        #[command(subcommand)]
        action: SkillsAction,

        /// Workspace directory to evaluate skills against (defaults to the OpenClaw agent workspace "default")
        #[arg(long)]
        workspace: Option<String>,

        /// Output compact JSON (status/install only)
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Run and manage Iron (WASM-only) workflows
    Iron {
        #[command(subcommand)]
        action: IronAction,
    },

    /// Interactive setup wizard
    Wizard,

    /// Show configuration
    Config,

    /// Run health checks
    Doctor,

    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

/// Channels subcommands.
#[derive(Subcommand)]
enum ChannelsAction {
    /// List configured channels
    List,

    /// Show channel status
    Status {
        /// Channel name
        name: Option<String>,
    },

    /// Enable a channel
    Enable {
        /// Channel name
        name: String,
    },

    /// Disable a channel
    Disable {
        /// Channel name
        name: String,
    },
}

/// Skills subcommands.
#[derive(Subcommand)]
enum SkillsAction {
    /// Show skill status for a workspace
    Status,

    /// Sync configured remote skills (and Colosseum docs) into the managed skills dir
    Sync,

    /// Print the skills prompt injected into agent runs (best-effort)
    Prompt,

    /// List required bins across eligible skills for a workspace
    Bins,

    /// Install a skill from ClawHub into the managed skills directory
    ClawhubInstall {
        /// Skill slug (or ClawHub skill URL)
        skill: String,

        /// ClawHub registry URL override
        #[arg(long)]
        registry: Option<String>,

        /// ClawHub site URL override
        #[arg(long)]
        site: Option<String>,

        /// Skill version (defaults to latest)
        #[arg(long)]
        version: Option<String>,

        /// Override the ClawHub workdir
        #[arg(long)]
        workdir: Option<String>,

        /// Override the ClawHub skills dir (relative to workdir)
        #[arg(long)]
        dir: Option<String>,

        /// Absolute path to the skills directory (overrides workdir/dir)
        #[arg(long)]
        skills_dir: Option<String>,

        /// Override the ClawHub CLI binary
        #[arg(long)]
        bin: Option<String>,

        /// Overwrite existing skill directory
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Timeout in milliseconds
        #[arg(long)]
        timeout_ms: Option<u64>,
    },
}

/// Iron workflow subcommands.
#[derive(Subcommand)]
enum IronAction {
    /// Create a new Iron workflow template on disk
    Init {
        /// Workflow name (also used as directory name)
        name: String,

        /// Target directory (defaults to ./iron/<name>)
        #[arg(long)]
        dir: Option<String>,

        /// Overwrite existing files
        #[arg(long, default_value_t = false)]
        force: bool,

        /// Create a Rust component template in <dir>/rust
        #[arg(long, default_value_t = false)]
        rust: bool,
    },

    /// Build an Iron workflow (e.g., from the bundled Rust template)
    Build {
        /// Path to a workflow directory (containing iron.json)
        path: String,

        /// Build in release mode
        #[arg(long, default_value_t = false)]
        release: bool,

        /// Rust project directory name (defaults to "rust")
        #[arg(long, default_value = "rust")]
        rust_dir: String,
    },

    /// Bundle an Iron workflow for distribution (.tar.gz)
    Bundle {
        /// Path to a workflow directory (containing iron.json)
        path: String,

        /// Output bundle path (defaults to <dir>/<name>-<version>.iron.tgz)
        #[arg(long)]
        out: Option<String>,

        /// Build first (uses the Rust template if present)
        #[arg(long, default_value_t = false)]
        build: bool,

        /// Build in release mode (used with --build)
        #[arg(long, default_value_t = false)]
        release: bool,

        /// Rust project directory name (defaults to "rust") (used with --build)
        #[arg(long, default_value = "rust")]
        rust_dir: String,

        /// Exclude WIT from the bundle
        #[arg(long, default_value_t = false)]
        no_wit: bool,

        /// Sign the bundle manifest with an Ed25519 seed key file (32 raw bytes, or base64/hex text)
        #[arg(long)]
        sign_key: Option<String>,
    },
    /// Run an Iron workflow component
    Run {
        /// Path to a workflow directory (containing iron.json) or a .wasm component
        path: String,

        /// Event JSON string (defaults to {"type":"manual"})
        #[arg(long)]
        event: Option<String>,

        /// Read event JSON from a file (or "-" for stdin)
        #[arg(long)]
        event_file: Option<String>,

        /// Timeout in milliseconds
        #[arg(long, default_value_t = 120_000)]
        timeout_ms: u64,

        /// Fuel limit for deterministic execution (instruction units)
        #[arg(long)]
        fuel: Option<u64>,

        /// Maximum linear memory per guest memory (MB)
        #[arg(long)]
        max_memory_mb: Option<u64>,

        /// Working directory for relative paths
        #[arg(long)]
        workdir: Option<String>,

        /// Allow filesystem roots for fs.* tools (repeatable)
        #[arg(long = "fs-root")]
        fs_roots: Vec<String>,

        /// Allow bash commands starting with these prefixes (repeatable)
        #[arg(long = "allow-bash-prefix")]
        allow_bash_prefixes: Vec<String>,

        /// Allow any bash command (dangerous)
        #[arg(long, default_value_t = false)]
        allow_bash_all: bool,

        /// Allow HTTP requests to these domains (repeatable)
        #[arg(long = "allow-http-domain")]
        allow_http_domains: Vec<String>,

        /// HTTP timeout in milliseconds
        #[arg(long, default_value_t = 20_000)]
        http_timeout_ms: u64,

        /// Maximum HTTP response bytes
        #[arg(long, default_value_t = 1_000_000)]
        http_max_bytes: u64,

        /// Path to a SQLite file to enable kv.* tools
        #[arg(long)]
        kv_path: Option<String>,

        /// KV namespace (defaults to "default")
        #[arg(long)]
        kv_namespace: Option<String>,

        /// Maximum KV value size in bytes
        #[arg(long, default_value_t = 1_000_000)]
        kv_max_value_bytes: u64,

        /// Provide a secret as NAME=VALUE (repeatable)
        #[arg(long = "secret")]
        secrets: Vec<String>,

        /// Read secrets from a JSON file ({"NAME":"VALUE", ...})
        #[arg(long)]
        secrets_file: Option<String>,

        /// Require the workflow to be signed by a trusted key
        #[arg(long, default_value_t = false)]
        require_signature: bool,

        /// Trusted Ed25519 public key (base64 or base64:<...>) (repeatable)
        #[arg(long = "trust-pubkey")]
        trust_pubkeys: Vec<String>,

        /// Read a trusted Ed25519 public key from a file (repeatable)
        #[arg(long = "trust-pubkey-file")]
        trust_pubkey_files: Vec<String>,
    },

    /// Run a directory of JSON fixtures against a workflow
    Test {
        /// Path to a workflow directory (containing iron.json), a .wasm component, or a .iron.tgz bundle
        path: String,

        /// Fixtures directory (defaults to <workflow_dir>/fixtures)
        #[arg(long)]
        fixtures: Option<String>,

        /// Write expected outputs for fixtures
        #[arg(long, default_value_t = false)]
        update: bool,

        /// Timeout in milliseconds
        #[arg(long, default_value_t = 120_000)]
        timeout_ms: u64,

        /// Fuel limit for deterministic execution (instruction units)
        #[arg(long)]
        fuel: Option<u64>,

        /// Maximum linear memory per guest memory (MB)
        #[arg(long)]
        max_memory_mb: Option<u64>,

        /// Working directory for relative paths
        #[arg(long)]
        workdir: Option<String>,

        /// Allow filesystem roots for fs.* tools (repeatable)
        #[arg(long = "fs-root")]
        fs_roots: Vec<String>,

        /// Allow bash commands starting with these prefixes (repeatable)
        #[arg(long = "allow-bash-prefix")]
        allow_bash_prefixes: Vec<String>,

        /// Allow any bash command (dangerous)
        #[arg(long, default_value_t = false)]
        allow_bash_all: bool,

        /// Allow HTTP requests to these domains (repeatable)
        #[arg(long = "allow-http-domain")]
        allow_http_domains: Vec<String>,

        /// HTTP timeout in milliseconds
        #[arg(long, default_value_t = 20_000)]
        http_timeout_ms: u64,

        /// Maximum HTTP response bytes
        #[arg(long, default_value_t = 1_000_000)]
        http_max_bytes: u64,

        /// Path to a SQLite file to enable kv.* tools
        #[arg(long)]
        kv_path: Option<String>,

        /// KV namespace (defaults to "default")
        #[arg(long)]
        kv_namespace: Option<String>,

        /// Maximum KV value size in bytes
        #[arg(long, default_value_t = 1_000_000)]
        kv_max_value_bytes: u64,

        /// Provide a secret as NAME=VALUE (repeatable)
        #[arg(long = "secret")]
        secrets: Vec<String>,

        /// Read secrets from a JSON file ({"NAME":"VALUE", ...})
        #[arg(long)]
        secrets_file: Option<String>,

        /// Require the workflow to be signed by a trusted key
        #[arg(long, default_value_t = false)]
        require_signature: bool,

        /// Trusted Ed25519 public key (base64 or base64:<...>) (repeatable)
        #[arg(long = "trust-pubkey")]
        trust_pubkeys: Vec<String>,

        /// Read a trusted Ed25519 public key from a file (repeatable)
        #[arg(long = "trust-pubkey-file")]
        trust_pubkey_files: Vec<String>,
    },

    /// Serve an Iron workflow over HTTP (local worker)
    Serve {
        /// Path to a workflow directory (containing iron.json), a .wasm component, or a .iron.tgz bundle
        path: String,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Port to listen on
        #[arg(long, default_value_t = 18790)]
        port: u16,

        /// Reload the workflow when the compiled WASM changes
        #[arg(long, default_value_t = false)]
        watch: bool,

        /// Timeout in milliseconds (per run)
        #[arg(long, default_value_t = 120_000)]
        timeout_ms: u64,

        /// Fuel limit for deterministic execution (instruction units)
        #[arg(long)]
        fuel: Option<u64>,

        /// Maximum linear memory per guest memory (MB)
        #[arg(long)]
        max_memory_mb: Option<u64>,

        /// Working directory for relative paths
        #[arg(long)]
        workdir: Option<String>,

        /// Allow filesystem roots for fs.* tools (repeatable)
        #[arg(long = "fs-root")]
        fs_roots: Vec<String>,

        /// Allow bash commands starting with these prefixes (repeatable)
        #[arg(long = "allow-bash-prefix")]
        allow_bash_prefixes: Vec<String>,

        /// Allow any bash command (dangerous)
        #[arg(long, default_value_t = false)]
        allow_bash_all: bool,

        /// Allow HTTP requests to these domains (repeatable)
        #[arg(long = "allow-http-domain")]
        allow_http_domains: Vec<String>,

        /// HTTP timeout in milliseconds
        #[arg(long, default_value_t = 20_000)]
        http_timeout_ms: u64,

        /// Maximum HTTP response bytes
        #[arg(long, default_value_t = 1_000_000)]
        http_max_bytes: u64,

        /// Path to a SQLite file to enable kv.* tools
        #[arg(long)]
        kv_path: Option<String>,

        /// KV namespace (defaults to "default")
        #[arg(long)]
        kv_namespace: Option<String>,

        /// Maximum KV value size in bytes
        #[arg(long, default_value_t = 1_000_000)]
        kv_max_value_bytes: u64,

        /// Provide a secret as NAME=VALUE (repeatable)
        #[arg(long = "secret")]
        secrets: Vec<String>,

        /// Read secrets from a JSON file ({"NAME":"VALUE", ...})
        #[arg(long)]
        secrets_file: Option<String>,

        /// Require this bearer token for /run (recommended with --public)
        #[arg(long, env = "DRBOT_IRON_AUTH_TOKEN")]
        auth_token: Option<String>,

        /// Read bearer token from a file (safer than CLI args)
        #[arg(long)]
        auth_token_file: Option<String>,

        /// TLS certificate PEM path (enables HTTPS)
        #[arg(long)]
        tls_cert: Option<String>,

        /// TLS private key PEM path (enables HTTPS)
        #[arg(long)]
        tls_key: Option<String>,

        /// Allow binding to non-local addresses (dangerous)
        #[arg(long, default_value_t = false)]
        public: bool,

        /// Maximum request body size for /run (bytes)
        #[arg(long, default_value_t = 262_144)]
        max_event_bytes: u64,

        /// Maximum workflow output size to return (bytes)
        #[arg(long, default_value_t = 200_000)]
        max_output_bytes: u64,

        /// Maximum concurrent /run requests
        #[arg(long, default_value_t = 4)]
        max_concurrency: usize,

        /// Enable Prometheus-style metrics at /metrics
        #[arg(long, default_value_t = false)]
        metrics: bool,

        /// Rate limit (requests per second). 0 disables.
        #[arg(long, default_value_t = 0)]
        rate_limit_rps: u64,

        /// Rate limit burst capacity (tokens)
        #[arg(long, default_value_t = 20)]
        rate_limit_burst: u64,

        /// Require the workflow to be signed by a trusted key
        #[arg(long, default_value_t = false)]
        require_signature: bool,

        /// Trusted Ed25519 public key (base64 or base64:<...>) (repeatable)
        #[arg(long = "trust-pubkey")]
        trust_pubkeys: Vec<String>,

        /// Read a trusted Ed25519 public key from a file (repeatable)
        #[arg(long = "trust-pubkey-file")]
        trust_pubkey_files: Vec<String>,
    },

    /// Call an Iron workflow HTTP server (/run)
    Call {
        /// Server URL (e.g., http://127.0.0.1:18790)
        url: String,

        /// Bearer token to send (if server requires auth)
        #[arg(long, env = "DRBOT_IRON_AUTH_TOKEN")]
        auth_token: Option<String>,

        /// Read bearer token from a file (safer than CLI args)
        #[arg(long)]
        auth_token_file: Option<String>,

        /// Event JSON string (defaults to {"type":"manual"})
        #[arg(long)]
        event: Option<String>,

        /// Read event JSON from a file (or "-" for stdin)
        #[arg(long)]
        event_file: Option<String>,

        /// Request timeout in milliseconds
        #[arg(long, default_value_t = 60_000)]
        timeout_ms: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("drbot={},drbot_gateway={}", log_level, log_level).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let mut config = if let Some(path) = &cli.config {
        let expanded = expand_tilde(path);
        std::env::set_var("DRBOT_CONFIG_PATH", expanded.clone());
        Config::from_file(expanded)?
    } else {
        Config::load().unwrap_or_default()
    };

    match cli.command {
        Some(Commands::Completions { shell }) => {
            use clap::CommandFactory as _;
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "drbot", &mut io::stdout());
            Ok(())
        }
        Some(Commands::Gateway {
            host,
            port,
            openclaw_agent_bash_allow_all,
            openclaw_agent_bash_allowlist,
        }) => {
            if let Some(host) = host {
                config.gateway.host = host;
            }
            if let Some(port) = port {
                config.gateway.port = port;
            }
            if openclaw_agent_bash_allow_all {
                std::env::set_var("DRBOT_OPENCLAW_AGENT_BASH_ALLOW_ALL", "1");
            }
            if let Some(raw) = openclaw_agent_bash_allowlist
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                std::env::set_var("DRBOT_OPENCLAW_AGENT_BASH_ALLOWLIST", raw);
            }
            run_gateway(config).await
        }
        Some(Commands::Chat {
            provider,
            model,
            system,
            skill_url,
            agent,
            yes,
            bash_auto_approve_prefixes,
            bash_auto_approve_allowlist,
            bash_auto_approve_all,
            agent_strict,
            root,
            max_tool_rounds,
            message,
            message_file,
            no_stream,
            session,
            new_session,
            list_sessions,
            title,
            persona,
            list_personas,
            context_size,
        }) => {
            run_chat(
                &config,
                provider,
                model,
                system,
                skill_url,
                agent,
                yes,
                bash_auto_approve_prefixes,
                bash_auto_approve_allowlist,
                bash_auto_approve_all,
                agent_strict,
                root,
                max_tool_rounds,
                message,
                message_file,
                !no_stream,
                session,
                new_session,
                list_sessions,
                title,
                persona,
                list_personas,
                context_size,
            )
            .await
        }
        Some(Commands::Tui {
            provider,
            model,
            system,
        }) => run_tui(&config, provider, model, system).await,
        Some(Commands::Channels { action }) => {
            run_channels(&config, action, cli.config.as_deref()).await
        }
        Some(Commands::Skills {
            action,
            workspace,
            json,
        }) => run_skills(&config, action, workspace, json).await,
        Some(Commands::Iron { action }) => run_iron(&config, action).await,
        Some(Commands::Wizard) => run_wizard().await,
        Some(Commands::Config) => {
            show_config(&config);
            Ok(())
        }
        Some(Commands::Doctor) => run_doctor(&config).await,
        None => {
            // Default: start gateway
            run_gateway(config).await
        }
    }
}

async fn run_tui(
    config: &Config,
    provider_name: Option<String>,
    model: Option<String>,
    system: Option<String>,
) -> Result<()> {
    use drbot_tui::ProviderType;

    // Determine provider type
    let provider_name = provider_name
        .or_else(|| config.providers.default_provider.clone())
        .unwrap_or_else(|| "auto".to_string());

    let provider_type = ProviderType::from_str(&provider_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", provider_name))?;

    // Get provider-specific config
    let (api_key, base_url, default_model) = match provider_type {
        ProviderType::Anthropic => {
            let cfg = config.providers.anthropic.as_ref();
            (
                cfg.map(|c| c.api_key.clone()),
                cfg.and_then(|c| c.base_url.clone()),
                cfg.and_then(|c| c.default_model.clone()),
            )
        }
        ProviderType::OpenAI => {
            let cfg = config.providers.openai.as_ref();
            (
                cfg.map(|c| c.api_key.clone()),
                cfg.and_then(|c| c.base_url.clone()),
                cfg.and_then(|c| c.default_model.clone()),
            )
        }
        ProviderType::Ollama => {
            let cfg = config.providers.ollama.as_ref();
            (
                None, // Ollama doesn't need API key
                cfg.map(|c| c.url.clone()),
                cfg.and_then(|c| c.default_model.clone()),
            )
        }
    };

    let tui_config = AppConfig {
        provider_type,
        api_key,
        base_url,
        model: model.or(default_model),
        system_prompt: system,
        max_history: 100,
    };

    drbot_tui::run(tui_config)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))
}

async fn run_gateway(config: Config) -> Result<()> {
    info!("drbot v{} starting...", env!("CARGO_PKG_VERSION"));

    let gateway = Gateway::new(config);
    let state_for_shutdown = gateway.state();

    const ACTION_STOP: u8 = 1;
    const ACTION_RESTART: u8 = 2;
    let shutdown_action = Arc::new(std::sync::atomic::AtomicU8::new(0));

    // Set up graceful shutdown
    let shutdown_action_for_shutdown = shutdown_action.clone();
    let shutdown = async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sigterm =
                signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");
            let mut sigusr1 =
                signal(SignalKind::user_defined1()).expect("Failed to install SIGUSR1 handler");

            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        info!("signal SIGINT received");
                        shutdown_action_for_shutdown.store(ACTION_STOP, std::sync::atomic::Ordering::Relaxed);
                        break;
                    }
                    v = sigterm.recv() => {
                        if v.is_some() {
                            info!("signal SIGTERM received");
                            shutdown_action_for_shutdown.store(ACTION_STOP, std::sync::atomic::Ordering::Relaxed);
                            break;
                        }
                    }
                    v = sigusr1.recv() => {
                        if v.is_none() {
                            continue;
                        }
                        info!("signal SIGUSR1 received");
                        let authorized = drbot_gateway::openclaw_restart::consume_sigusr1_restart_authorization();
                        if !authorized && !drbot_gateway::openclaw_restart::is_sigusr1_restart_externally_allowed() {
                            warn!("SIGUSR1 restart ignored (not authorized; set DRBOT_OPENCLAW_ALLOW_EXTERNAL_RESTART=1 or use the gateway tool)");
                            continue;
                        }
                        info!("restart requested (SIGUSR1)");

                        let drain_timeout_ms = std::env::var("DRBOT_OPENCLAW_RESTART_DRAIN_TIMEOUT_MS")
                            .ok()
                            .and_then(|v| v.trim().parse::<u64>().ok())
                            .filter(|v| *v > 0)
                            .unwrap_or(60_000);
                        let started = std::time::Instant::now();
                        loop {
                            let inflight = state_for_shutdown.openclaw_main_inflight();
                            if inflight == 0 {
                                break;
                            }
                            if started.elapsed() >= std::time::Duration::from_millis(drain_timeout_ms) {
                                warn!(
                                    inflight,
                                    drain_timeout_ms,
                                    "restart drain timeout elapsed; continuing shutdown"
                                );
                                break;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }

                        shutdown_action_for_shutdown.store(ACTION_RESTART, std::sync::atomic::Ordering::Relaxed);
                        break;
                    }
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = &state_for_shutdown;
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C handler");
            info!("Received shutdown signal");
            shutdown_action_for_shutdown.store(ACTION_STOP, std::sync::atomic::Ordering::Relaxed);
        }
    };

    gateway
        .run_with_shutdown(shutdown)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if shutdown_action.load(std::sync::atomic::Ordering::Relaxed) == ACTION_RESTART {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;

            let exe = std::env::current_exe()?;
            let args = std::env::args_os().skip(1).collect::<Vec<_>>();
            info!(exe = %exe.to_string_lossy(), "Restarting drbot via exec()");
            let err = std::process::Command::new(exe).args(args).exec();
            return Err(anyhow::anyhow!("failed to restart via exec(): {}", err));
        }
        #[cfg(not(unix))]
        {
            warn!("Restart requested but exec() is not supported on this platform; exiting");
        }
    }

    Ok(())
}

async fn run_skills(
    config: &Config,
    action: SkillsAction,
    workspace: Option<String>,
    json: bool,
) -> Result<()> {
    let workspace_dir = workspace
        .as_deref()
        .map(|p| PathBuf::from(expand_tilde(p)))
        .unwrap_or_else(|| drbot_gateway::openclaw_paths::resolve_agent_workspace_dir("default"));

    match action {
        SkillsAction::Status => {
            let report =
                drbot_gateway::openclaw_skills::build_skills_status_report(&workspace_dir, config);
            let raw = if json {
                serde_json::to_string(&report)?
            } else {
                serde_json::to_string_pretty(&report)?
            };
            println!("{}", raw);
        }
        SkillsAction::Sync => {
            drbot_gateway::openclaw_skills::sync_configured_remote_skills_best_effort(config).await;
            drbot_gateway::colosseum::sync_colosseum_docs_best_effort(config).await;
            println!("ok");
        }
        SkillsAction::Prompt => {
            let prompt = drbot_gateway::openclaw_skills::build_workspace_skills_prompt(
                &workspace_dir,
                config,
            );
            print!("{}", prompt);
        }
        SkillsAction::Bins => {
            let bins = drbot_gateway::openclaw_skills::collect_skill_bins(
                &[workspace_dir.clone()],
                config,
            );
            for bin in bins {
                println!("{}", bin);
            }
        }
        SkillsAction::ClawhubInstall {
            skill,
            registry,
            site,
            version,
            workdir,
            dir,
            skills_dir,
            bin,
            force,
            timeout_ms,
        } => {
            let workdir = workdir.as_deref().map(|p| PathBuf::from(expand_tilde(p)));
            let skills_dir = skills_dir
                .as_deref()
                .map(|p| PathBuf::from(expand_tilde(p)));
            let params = drbot_gateway::openclaw_skills::ClawhubInstallParams {
                skill,
                registry,
                site,
                version,
                workdir,
                dir,
                skills_dir,
                bin,
                force,
                timeout_ms,
            };
            let result =
                drbot_gateway::openclaw_skills::install_clawhub_skill(config, params).await;
            if json {
                let raw = serde_json::to_string(&result)?;
                println!("{}", raw);
            } else if result.ok {
                println!("{}", result.message);
            } else {
                eprintln!("{}", result.message);
                if !result.stderr.trim().is_empty() {
                    eprintln!("{}", result.stderr.trim());
                } else if !result.stdout.trim().is_empty() {
                    eprintln!("{}", result.stdout.trim());
                }
            }

            if !result.ok {
                return Err(anyhow::anyhow!("{}", result.message));
            }
        }
    }

    Ok(())
}

fn hash_to_u64<T: Hash>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

struct IronHttpMetrics {
    start: std::time::Instant,
    request_id_seq: AtomicU64,

    run_requests_total: AtomicU64,
    run_ok_total: AtomicU64,
    run_error_total: AtomicU64,
    run_unauthorized_total: AtomicU64,
    run_busy_total: AtomicU64,
    run_rate_limited_total: AtomicU64,
    run_bad_request_total: AtomicU64,

    metrics_scrapes_total: AtomicU64,

    run_bytes_in_total: AtomicU64,
    run_output_bytes_total: AtomicU64,
    run_output_truncated_total: AtomicU64,
    run_exec_duration_ms_total: AtomicU64,
    run_exec_duration_ms_count: AtomicU64,
    run_fuel_consumed_total: AtomicU64,
}

impl IronHttpMetrics {
    fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
            request_id_seq: AtomicU64::new(1),
            run_requests_total: AtomicU64::new(0),
            run_ok_total: AtomicU64::new(0),
            run_error_total: AtomicU64::new(0),
            run_unauthorized_total: AtomicU64::new(0),
            run_busy_total: AtomicU64::new(0),
            run_rate_limited_total: AtomicU64::new(0),
            run_bad_request_total: AtomicU64::new(0),
            metrics_scrapes_total: AtomicU64::new(0),
            run_bytes_in_total: AtomicU64::new(0),
            run_output_bytes_total: AtomicU64::new(0),
            run_output_truncated_total: AtomicU64::new(0),
            run_exec_duration_ms_total: AtomicU64::new(0),
            run_exec_duration_ms_count: AtomicU64::new(0),
            run_fuel_consumed_total: AtomicU64::new(0),
        }
    }

    fn next_request_id(&self) -> String {
        let n = self.request_id_seq.fetch_add(1, Ordering::Relaxed);
        format!("{:016x}", n)
    }

    fn record_run_exec(&self, duration: std::time::Duration) {
        let ms = duration.as_millis().min(u64::MAX as u128) as u64;
        self.run_exec_duration_ms_total
            .fetch_add(ms, Ordering::Relaxed);
        self.run_exec_duration_ms_count
            .fetch_add(1, Ordering::Relaxed);
    }

    fn render_prometheus(&self) -> String {
        use std::fmt::Write as _;

        fn u(v: &AtomicU64) -> u64 {
            v.load(Ordering::Relaxed)
        }

        let uptime = self.start.elapsed().as_secs_f64();
        let mut out = String::new();

        let _ = writeln!(
            &mut out,
            "# HELP drbot_iron_http_uptime_seconds Uptime of the Iron HTTP server in seconds.",
        );
        let _ = writeln!(&mut out, "# TYPE drbot_iron_http_uptime_seconds gauge");
        let _ = writeln!(&mut out, "drbot_iron_http_uptime_seconds {:.3}", uptime);

        macro_rules! counter {
            ($name:literal, $help:literal, $value:expr) => {{
                let _ = writeln!(&mut out, "# HELP {} {}", $name, $help);
                let _ = writeln!(&mut out, "# TYPE {} counter", $name);
                let _ = writeln!(&mut out, "{} {}", $name, $value);
            }};
        }

        counter!(
            "drbot_iron_http_run_requests_total",
            "Total /run requests received.",
            u(&self.run_requests_total)
        );
        counter!(
            "drbot_iron_http_run_ok_total",
            "Total /run responses with ok=true.",
            u(&self.run_ok_total)
        );
        counter!(
            "drbot_iron_http_run_error_total",
            "Total /run responses with ok=false (including timeouts).",
            u(&self.run_error_total)
        );
        counter!(
            "drbot_iron_http_run_unauthorized_total",
            "Total unauthorized /run requests.",
            u(&self.run_unauthorized_total)
        );
        counter!(
            "drbot_iron_http_run_busy_total",
            "Total busy /run requests (concurrency limit).",
            u(&self.run_busy_total)
        );
        counter!(
            "drbot_iron_http_run_rate_limited_total",
            "Total rate limited /run requests.",
            u(&self.run_rate_limited_total)
        );
        counter!(
            "drbot_iron_http_run_bad_request_total",
            "Total bad request /run requests (invalid body/json).",
            u(&self.run_bad_request_total)
        );
        counter!(
            "drbot_iron_http_metrics_scrapes_total",
            "Total /metrics scrapes.",
            u(&self.metrics_scrapes_total)
        );
        counter!(
            "drbot_iron_http_run_bytes_in_total",
            "Total request body bytes read for /run.",
            u(&self.run_bytes_in_total)
        );
        counter!(
            "drbot_iron_http_run_output_bytes_total",
            "Total workflow output bytes produced (pre-truncation).",
            u(&self.run_output_bytes_total)
        );
        counter!(
            "drbot_iron_http_run_output_truncated_total",
            "Total /run responses with output truncated.",
            u(&self.run_output_truncated_total)
        );
        counter!(
            "drbot_iron_http_run_exec_duration_ms_total",
            "Total execution time spent in WASM for /run, in milliseconds.",
            u(&self.run_exec_duration_ms_total)
        );
        counter!(
            "drbot_iron_http_run_exec_duration_ms_count",
            "Count of WASM executions for /run.",
            u(&self.run_exec_duration_ms_count)
        );
        counter!(
            "drbot_iron_http_run_fuel_consumed_total",
            "Total fuel consumed for /run (when fuel metering is enabled).",
            u(&self.run_fuel_consumed_total)
        );

        out
    }
}

struct IronRateLimiter {
    rps: f64,
    burst: f64,
    inner: tokio::sync::Mutex<IronRateLimiterInner>,
}

struct IronRateLimiterInner {
    last_prune: std::time::Instant,
    clients: HashMap<u64, IronRateLimiterClient>,
}

struct IronRateLimiterClient {
    tokens: f64,
    last_refill: std::time::Instant,
    last_seen: std::time::Instant,
}

struct IronRateLimitDecision {
    allowed: bool,
    retry_after: Option<std::time::Duration>,
}

impl IronRateLimiter {
    fn new(rps: u64, burst: u64) -> Self {
        let rps = rps.max(1) as f64;
        let burst = burst.max(1) as f64;
        Self {
            rps,
            burst,
            inner: tokio::sync::Mutex::new(IronRateLimiterInner {
                last_prune: std::time::Instant::now(),
                clients: HashMap::new(),
            }),
        }
    }

    async fn check(&self, key: u64) -> IronRateLimitDecision {
        let now = std::time::Instant::now();
        let mut inner = self.inner.lock().await;

        if inner.last_prune.elapsed() > std::time::Duration::from_secs(30) {
            let ttl = std::time::Duration::from_secs(5 * 60);
            inner
                .clients
                .retain(|_, c| now.saturating_duration_since(c.last_seen) <= ttl);
            inner.last_prune = now;
        }

        let client = inner
            .clients
            .entry(key)
            .or_insert_with(|| IronRateLimiterClient {
                tokens: self.burst,
                last_refill: now,
                last_seen: now,
            });

        let elapsed = now
            .saturating_duration_since(client.last_refill)
            .as_secs_f64();
        if elapsed > 0.0 {
            client.tokens = (client.tokens + elapsed * self.rps).min(self.burst);
            client.last_refill = now;
        }

        client.last_seen = now;

        if client.tokens >= 1.0 {
            client.tokens -= 1.0;
            return IronRateLimitDecision {
                allowed: true,
                retry_after: None,
            };
        }

        let needed = 1.0 - client.tokens;
        let wait_secs = needed / self.rps;
        let retry_after = if wait_secs.is_finite() && wait_secs > 0.0 {
            Some(std::time::Duration::from_secs_f64(wait_secs))
        } else {
            None
        };

        IronRateLimitDecision {
            allowed: false,
            retry_after,
        }
    }
}

struct IronHttpState {
    runner: Arc<drbot_iron::IronRunner>,
    workflow: tokio::sync::RwLock<Arc<drbot_iron::IronLoadedWorkflow>>,
    cfg: drbot_iron::IronRunnerConfig,
    auth_token: Option<String>,
    max_event_bytes: usize,
    semaphore: Arc<tokio::sync::Semaphore>,
    max_output_bytes: usize,
    metrics: Arc<IronHttpMetrics>,
    rate_limiter: Option<Arc<IronRateLimiter>>,
}

async fn iron_http_healthz() -> &'static str {
    "ok"
}

fn iron_extract_request_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    use axum::http::header;

    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().strip_prefix("Bearer "))
        .map(|v| v.trim())
        .filter(|v| !v.is_empty());
    let alt = headers
        .get("x-drbot-iron-token")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim())
        .filter(|v| !v.is_empty());

    bearer.or(alt)
}

fn iron_set_request_id(resp: &mut axum::response::Response, request_id: &str) {
    use axum::http::HeaderValue;

    resp.headers_mut().insert(
        "x-request-id",
        HeaderValue::from_str(request_id).expect("request_id must be a valid header value"),
    );
}

fn iron_json_response(
    request_id: &str,
    status: axum::http::StatusCode,
    body: serde_json::Value,
) -> axum::response::Response {
    use axum::response::IntoResponse as _;

    let mut resp = (status, axum::Json(body)).into_response();
    iron_set_request_id(&mut resp, request_id);
    resp
}

fn iron_text_response(
    request_id: &str,
    status: axum::http::StatusCode,
    content_type: &'static str,
    body: String,
) -> axum::response::Response {
    use axum::http::{header, HeaderValue};

    let mut resp = axum::response::Response::new(axum::body::Body::from(body));
    *resp.status_mut() = status;
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    iron_set_request_id(&mut resp, request_id);
    resp
}

async fn iron_http_metrics(
    axum::extract::State(state): axum::extract::State<Arc<IronHttpState>>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    req: axum::extract::Request,
) -> axum::response::Response {
    use axum::http::StatusCode;

    let request_id = state.metrics.next_request_id();
    let started = std::time::Instant::now();
    let (parts, _body) = req.into_parts();

    if let Some(expected) = state.auth_token.as_deref() {
        let token = iron_extract_request_token(&parts.headers);
        if token != Some(expected) {
            let resp = iron_text_response(
                request_id.as_str(),
                StatusCode::UNAUTHORIZED,
                "text/plain; charset=utf-8",
                "unauthorized".to_string(),
            );
            info!(
                request_id = %request_id,
                client_ip = %addr.ip(),
                status = %StatusCode::UNAUTHORIZED.as_u16(),
                duration_ms = %started.elapsed().as_millis(),
                "iron http /metrics"
            );
            return resp;
        }
    }

    state
        .metrics
        .metrics_scrapes_total
        .fetch_add(1, Ordering::Relaxed);

    let body = state.metrics.render_prometheus();
    let resp = iron_text_response(
        request_id.as_str(),
        StatusCode::OK,
        "text/plain; version=0.0.4; charset=utf-8",
        body,
    );

    info!(
        request_id = %request_id,
        client_ip = %addr.ip(),
        status = %StatusCode::OK.as_u16(),
        duration_ms = %started.elapsed().as_millis(),
        "iron http /metrics"
    );
    resp
}

async fn iron_http_run(
    axum::extract::State(state): axum::extract::State<Arc<IronHttpState>>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    req: axum::extract::Request,
) -> axum::response::Response {
    use axum::http::{header, StatusCode};

    let request_id = state.metrics.next_request_id();
    let started = std::time::Instant::now();
    state
        .metrics
        .run_requests_total
        .fetch_add(1, Ordering::Relaxed);

    let (parts, body) = req.into_parts();
    let token = iron_extract_request_token(&parts.headers);

    if let Some(expected) = state.auth_token.as_deref() {
        if token != Some(expected) {
            state
                .metrics
                .run_unauthorized_total
                .fetch_add(1, Ordering::Relaxed);

            let resp = iron_json_response(
                request_id.as_str(),
                StatusCode::UNAUTHORIZED,
                serde_json::json!({
                    "ok": false,
                    "error": "unauthorized",
                    "request_id": request_id,
                }),
            );

            info!(
                request_id = %request_id,
                client_ip = %addr.ip(),
                status = %StatusCode::UNAUTHORIZED.as_u16(),
                duration_ms = %started.elapsed().as_millis(),
                "iron http /run"
            );
            return resp;
        }
    }

    if let Some(limiter) = state.rate_limiter.as_ref() {
        let key = if state.auth_token.is_some() {
            token
                .map(|t| hash_to_u64(&t))
                .unwrap_or_else(|| hash_to_u64(&addr.ip()))
        } else {
            hash_to_u64(&addr.ip())
        };

        let decision = limiter.check(key).await;
        if !decision.allowed {
            state
                .metrics
                .run_rate_limited_total
                .fetch_add(1, Ordering::Relaxed);

            let mut body_json = serde_json::json!({
                "ok": false,
                "error": "rate limited",
                "request_id": request_id,
            });

            if let Some(retry_after) = decision.retry_after {
                let ms = retry_after.as_millis().min(u64::MAX as u128) as u64;
                body_json["retry_after_ms"] = serde_json::Value::from(ms);
            }

            let mut resp = iron_json_response(
                request_id.as_str(),
                StatusCode::TOO_MANY_REQUESTS,
                body_json,
            );

            if let Some(retry_after) = decision.retry_after {
                let secs = retry_after.as_secs_f64().ceil().max(1.0) as u64;
                if let Ok(v) = axum::http::HeaderValue::from_str(&secs.to_string()) {
                    resp.headers_mut().insert(header::RETRY_AFTER, v);
                }
            }

            info!(
                request_id = %request_id,
                client_ip = %addr.ip(),
                status = %StatusCode::TOO_MANY_REQUESTS.as_u16(),
                duration_ms = %started.elapsed().as_millis(),
                "iron http /run"
            );
            return resp;
        }
    }

    let _permit = match state.semaphore.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            state.metrics.run_busy_total.fetch_add(1, Ordering::Relaxed);

            let resp = iron_json_response(
                request_id.as_str(),
                StatusCode::TOO_MANY_REQUESTS,
                serde_json::json!({
                    "ok": false,
                    "error": "server busy (concurrency limit reached)",
                    "request_id": request_id,
                }),
            );

            info!(
                request_id = %request_id,
                client_ip = %addr.ip(),
                status = %StatusCode::TOO_MANY_REQUESTS.as_u16(),
                duration_ms = %started.elapsed().as_millis(),
                "iron http /run"
            );
            return resp;
        }
    };

    let bytes = match axum::body::to_bytes(body, state.max_event_bytes).await {
        Ok(v) => v,
        Err(e) => {
            state
                .metrics
                .run_bad_request_total
                .fetch_add(1, Ordering::Relaxed);

            let resp = iron_json_response(
                request_id.as_str(),
                StatusCode::PAYLOAD_TOO_LARGE,
                serde_json::json!({
                    "ok": false,
                    "error": format!("failed to read request body: {}", e),
                    "request_id": request_id,
                }),
            );

            info!(
                request_id = %request_id,
                client_ip = %addr.ip(),
                status = %StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
                duration_ms = %started.elapsed().as_millis(),
                "iron http /run"
            );
            return resp;
        }
    };

    state
        .metrics
        .run_bytes_in_total
        .fetch_add(bytes.len() as u64, Ordering::Relaxed);

    let event: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            state
                .metrics
                .run_bad_request_total
                .fetch_add(1, Ordering::Relaxed);

            let resp = iron_json_response(
                request_id.as_str(),
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "ok": false,
                    "error": format!("invalid JSON: {}", e),
                    "request_id": request_id,
                }),
            );

            info!(
                request_id = %request_id,
                client_ip = %addr.ip(),
                status = %StatusCode::BAD_REQUEST.as_u16(),
                duration_ms = %started.elapsed().as_millis(),
                "iron http /run"
            );
            return resp;
        }
    };

    let event_json = match serde_json::to_string(&event) {
        Ok(v) => v,
        Err(e) => {
            state
                .metrics
                .run_bad_request_total
                .fetch_add(1, Ordering::Relaxed);

            let resp = iron_json_response(
                request_id.as_str(),
                StatusCode::BAD_REQUEST,
                serde_json::json!({
                    "ok": false,
                    "error": format!("invalid JSON: {}", e),
                    "request_id": request_id,
                }),
            );

            info!(
                request_id = %request_id,
                client_ip = %addr.ip(),
                status = %StatusCode::BAD_REQUEST.as_u16(),
                duration_ms = %started.elapsed().as_millis(),
                "iron http /run"
            );
            return resp;
        }
    };

    let workflow = { state.workflow.read().await.clone() };

    let exec_started = std::time::Instant::now();
    let res = state
        .runner
        .run_loaded_with_stats(workflow.as_ref(), event_json.as_str(), state.cfg.clone())
        .await;
    let exec_duration = exec_started.elapsed();
    state.metrics.record_run_exec(exec_duration);

    match res {
        Ok(run_out) => {
            state.metrics.run_ok_total.fetch_add(1, Ordering::Relaxed);
            state
                .metrics
                .run_output_bytes_total
                .fetch_add(run_out.output.len() as u64, Ordering::Relaxed);
            if let Some(fuel) = run_out.fuel_consumed {
                state
                    .metrics
                    .run_fuel_consumed_total
                    .fetch_add(fuel, Ordering::Relaxed);
            }

            let mut out = run_out.output;
            let original_len = out.len();
            let truncated = if out.len() > state.max_output_bytes {
                out.truncate(state.max_output_bytes);
                state
                    .metrics
                    .run_output_truncated_total
                    .fetch_add(1, Ordering::Relaxed);
                true
            } else {
                false
            };

            let output_json = if truncated {
                None
            } else {
                serde_json::from_str::<serde_json::Value>(&out).ok()
            };

            let resp = iron_json_response(
                request_id.as_str(),
                StatusCode::OK,
                serde_json::json!({
                    "ok": true,
                    "request_id": request_id,
                    "output": out,
                    "output_json": output_json,
                    "truncated": truncated,
                    "output_len": original_len,
                    "fuel_consumed": run_out.fuel_consumed,
                    "exec_ms": exec_duration.as_millis().min(u64::MAX as u128) as u64,
                }),
            );

            info!(
                request_id = %request_id,
                client_ip = %addr.ip(),
                status = %StatusCode::OK.as_u16(),
                duration_ms = %started.elapsed().as_millis(),
                exec_ms = %exec_duration.as_millis(),
                output_len = %original_len,
                truncated = %truncated,
                fuel_consumed = ?run_out.fuel_consumed,
                "iron http /run"
            );
            resp
        }
        Err(e) => {
            state
                .metrics
                .run_error_total
                .fetch_add(1, Ordering::Relaxed);

            let msg = e.to_string();
            let status = if msg.contains("timed out") {
                StatusCode::REQUEST_TIMEOUT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

            let resp = iron_json_response(
                request_id.as_str(),
                status,
                serde_json::json!({
                    "ok": false,
                    "error": msg,
                    "request_id": request_id,
                    "exec_ms": exec_duration.as_millis().min(u64::MAX as u128) as u64,
                }),
            );

            info!(
                request_id = %request_id,
                client_ip = %addr.ip(),
                status = %status.as_u16(),
                duration_ms = %started.elapsed().as_millis(),
                exec_ms = %exec_duration.as_millis(),
                "iron http /run"
            );
            resp
        }
    }
}

async fn run_iron(config: &Config, action: IronAction) -> Result<()> {
    let _ = config;
    match action {
        IronAction::Init {
            name,
            dir,
            force,
            rust,
        } => {
            let base = dir
                .as_deref()
                .map(|p| PathBuf::from(expand_tilde(p)))
                .unwrap_or_else(|| PathBuf::from("iron").join(&name));
            init_iron_workflow_template(&base, &name, force, rust)?;
            println!("{}", base.display());
        }
        IronAction::Build {
            path,
            release,
            rust_dir,
        } => {
            let path = PathBuf::from(expand_tilde(&path));
            let workflow_dir = resolve_iron_workflow_dir(&path)?;
            let out = build_iron_rust_workflow(&workflow_dir, &rust_dir, release).await?;
            println!("{}", out.display());
        }
        IronAction::Bundle {
            path,
            out,
            build,
            release,
            rust_dir,
            no_wit,
            sign_key,
        } => {
            let path = PathBuf::from(expand_tilde(&path));
            let workflow_dir = resolve_iron_workflow_dir(&path)?;

            if build {
                let _ = build_iron_rust_workflow(&workflow_dir, &rust_dir, release).await?;
            }

            let manifest = drbot_iron::IronWorkflowManifest::load(&workflow_dir.join("iron.json"))?;
            let default_name = format!(
                "{}-{}.iron.tgz",
                slugify_filename(manifest.name.as_str()),
                slugify_filename(manifest.version.as_str())
            );
            let out_path = out
                .as_deref()
                .map(|p| PathBuf::from(expand_tilde(p)))
                .unwrap_or_else(|| workflow_dir.join(default_name));

            if let Some(p) = sign_key
                .as_deref()
                .map(|s| expand_tilde(s))
                .filter(|s| !s.trim().is_empty())
            {
                let seed = read_ed25519_seed_from_file(Path::new(p.as_str()))?;
                drbot_iron::create_bundle_tar_gz_signed(&workflow_dir, &out_path, !no_wit, &seed)?;
            } else {
                drbot_iron::create_bundle_tar_gz(&workflow_dir, &out_path, !no_wit)?;
            }
            println!("{}", out_path.display());
        }
        IronAction::Serve {
            path,
            host,
            port,
            watch,
            timeout_ms,
            fuel,
            max_memory_mb,
            workdir,
            fs_roots,
            allow_bash_prefixes,
            allow_bash_all,
            allow_http_domains,
            http_timeout_ms,
            http_max_bytes,
            kv_path,
            kv_namespace,
            kv_max_value_bytes,
            secrets,
            secrets_file,
            auth_token,
            auth_token_file,
            tls_cert,
            tls_key,
            public,
            max_event_bytes,
            max_output_bytes,
            max_concurrency,
            metrics,
            rate_limit_rps,
            rate_limit_burst,
            require_signature,
            trust_pubkeys,
            trust_pubkey_files,
        } => {
            if !public {
                let host_lower = host.trim().to_ascii_lowercase();
                let is_loopback = host_lower == "localhost"
                    || host_lower
                        .parse::<std::net::IpAddr>()
                        .map(|ip| ip.is_loopback())
                        .unwrap_or(false);
                if !is_loopback {
                    return Err(anyhow::anyhow!(
                        "refusing to bind to non-local host '{}' without --public",
                        host
                    ));
                }
            }

            let path = PathBuf::from(expand_tilde(&path));
            let source_path = path.clone();
            let (path_for_run, bundle_tmp) = if is_iron_bundle_path(&path) {
                let tmp =
                    std::env::temp_dir().join(format!("drbot-iron-bundle-{}", Uuid::new_v4()));
                std::fs::create_dir_all(&tmp)?;
                drbot_iron::unpack_bundle_tar_gz(&path, &tmp)?;
                (tmp.clone(), Some(TempDirGuard::new(tmp)))
            } else {
                (path, None)
            };

            let bundle_tmp_dir = bundle_tmp.as_ref().map(|g| g.path.clone());

            let trusted_pubkeys = read_trusted_ed25519_pubkeys(trust_pubkeys, trust_pubkey_files)?;
            let manifest_for_policy = try_load_iron_manifest(&path_for_run)?;
            enforce_iron_signature_policy(
                manifest_for_policy.as_ref(),
                trusted_pubkeys.as_slice(),
                require_signature,
            )?;

            let (wasm_path, _manifest_name) = resolve_iron_wasm_path(&path_for_run)?;

            let mut run_cfg = drbot_iron::IronRunnerConfig::default();
            run_cfg.timeout = std::time::Duration::from_millis(timeout_ms.max(1));
            if let Some(f) = fuel {
                run_cfg.fuel = Some(f);
            }
            if let Some(mb) = max_memory_mb {
                if mb == 0 {
                    run_cfg.max_memory_bytes = None;
                } else {
                    let bytes = mb.saturating_mul(1024 * 1024);
                    let bytes = bytes.min(usize::MAX as u64) as usize;
                    run_cfg.max_memory_bytes = Some(bytes);
                }
            }
            let mut tool_cfg = drbot_iron::IronToolHostConfig::default();
            if let Some(wd) = workdir.as_deref() {
                tool_cfg.workdir = PathBuf::from(expand_tilde(wd));
            }
            tool_cfg.fs_roots = fs_roots
                .iter()
                .map(|p| PathBuf::from(expand_tilde(p)))
                .collect();
            tool_cfg.bash_allow_prefixes = allow_bash_prefixes;
            tool_cfg.bash_allow_all = allow_bash_all;

            tool_cfg.http_allow_domains = allow_http_domains;
            tool_cfg.http_timeout = std::time::Duration::from_millis(http_timeout_ms.max(1));
            tool_cfg.http_max_bytes = http_max_bytes.max(1).min(usize::MAX as u64) as usize;

            tool_cfg.kv_path = kv_path
                .as_deref()
                .map(|p| expand_tilde(p))
                .filter(|p| !p.trim().is_empty())
                .map(PathBuf::from);
            tool_cfg.kv_namespace = kv_namespace;
            tool_cfg.kv_max_value_bytes = kv_max_value_bytes.max(1).min(usize::MAX as u64) as usize;

            tool_cfg.secrets = load_iron_secrets(secrets, secrets_file)?;

            if let Some(manifest) = manifest_for_policy.as_ref() {
                apply_iron_manifest_capabilities(manifest, &mut tool_cfg)?;
            }

            run_cfg.tools = tool_cfg;

            let auth_token = match auth_token
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                Some(v) => Some(v.to_string()),
                None => match auth_token_file
                    .as_deref()
                    .map(|p| expand_tilde(p))
                    .filter(|p| !p.trim().is_empty())
                {
                    Some(p) => {
                        let raw = std::fs::read_to_string(&p).map_err(|e| {
                            anyhow::anyhow!("failed to read auth token file '{}': {}", p, e)
                        })?;
                        let t = raw.trim().to_string();
                        if t.is_empty() {
                            None
                        } else {
                            Some(t)
                        }
                    }
                    None => None,
                },
            };

            if public && auth_token.is_none() {
                return Err(anyhow::anyhow!(
                    "--public requires --auth-token (or --auth-token-file)"
                ));
            }

            let tls_cert = tls_cert
                .as_deref()
                .map(|p| expand_tilde(p))
                .filter(|p| !p.trim().is_empty());
            let tls_key = tls_key
                .as_deref()
                .map(|p| expand_tilde(p))
                .filter(|p| !p.trim().is_empty());
            let use_tls = tls_cert.is_some() || tls_key.is_some();
            if use_tls && (tls_cert.is_none() || tls_key.is_none()) {
                return Err(anyhow::anyhow!(
                    "--tls-cert and --tls-key must be provided together"
                ));
            }

            let max_event_bytes = max_event_bytes.max(1).min(usize::MAX as u64) as usize;
            let max_output_bytes = max_output_bytes.max(1).min(usize::MAX as u64) as usize;
            let max_concurrency = max_concurrency.max(1);
            let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrency));

            let runner = Arc::new(drbot_iron::IronRunner::new()?);
            let loaded = runner.load_file(&wasm_path)?;

            let http_metrics = Arc::new(IronHttpMetrics::new());
            let rate_limiter = if rate_limit_rps > 0 {
                Some(Arc::new(IronRateLimiter::new(
                    rate_limit_rps,
                    rate_limit_burst,
                )))
            } else {
                None
            };

            let state = Arc::new(IronHttpState {
                runner: runner.clone(),
                workflow: tokio::sync::RwLock::new(Arc::new(loaded)),
                cfg: run_cfg,
                auth_token,
                max_event_bytes,
                semaphore,
                max_output_bytes,
                metrics: http_metrics,
                rate_limiter,
            });

            if watch {
                let state_for_watch = state.clone();
                let source_path_for_watch = source_path.clone();
                let path_for_run_for_watch = path_for_run.clone();
                let bundle_tmp_dir_for_watch = bundle_tmp_dir.clone();

                tokio::spawn(async move {
                    fn stamp(path: &Path) -> Option<(u128, u64)> {
                        let meta = std::fs::metadata(path).ok()?;
                        let modified = meta.modified().ok()?;
                        let nanos = modified
                            .duration_since(std::time::UNIX_EPOCH)
                            .ok()?
                            .as_nanos();
                        Some((nanos, meta.len()))
                    }

                    let mut last: Option<(u128, u64)> = None;
                    loop {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                        let loaded = if is_iron_bundle_path(&source_path_for_watch) {
                            let Some(tmp_dir) = bundle_tmp_dir_for_watch.as_ref() else {
                                warn!("watch enabled, but bundle tmp dir is missing");
                                return;
                            };

                            let current = match stamp(&source_path_for_watch) {
                                Some(v) => v,
                                None => continue,
                            };
                            if last == Some(current) {
                                continue;
                            }
                            last = Some(current);

                            let runner = state_for_watch.runner.clone();
                            let bundle_path = source_path_for_watch.clone();
                            let tmp_dir = tmp_dir.clone();
                            let res = tokio::task::spawn_blocking(
                                move || -> Result<drbot_iron::IronLoadedWorkflow> {
                                    let _ = std::fs::remove_dir_all(&tmp_dir);
                                    std::fs::create_dir_all(&tmp_dir)?;
                                    drbot_iron::unpack_bundle_tar_gz(&bundle_path, &tmp_dir)?;
                                    let (wasm_path, _) = resolve_iron_wasm_path(&tmp_dir)?;
                                    runner.load_file(&wasm_path)
                                },
                            )
                            .await;

                            match res {
                                Ok(Ok(loaded)) => Some(loaded),
                                Ok(Err(e)) => {
                                    warn!("watch reload failed: {}", e);
                                    None
                                }
                                Err(e) => {
                                    warn!("watch reload join error: {}", e);
                                    None
                                }
                            }
                        } else {
                            let wasm_path = if path_for_run_for_watch.is_dir() {
                                match resolve_iron_wasm_path(&path_for_run_for_watch) {
                                    Ok((p, _)) => p,
                                    Err(e) => {
                                        warn!("watch: failed to resolve workflow wasm: {}", e);
                                        continue;
                                    }
                                }
                            } else {
                                path_for_run_for_watch.clone()
                            };

                            let current = match stamp(&wasm_path) {
                                Some(v) => v,
                                None => continue,
                            };
                            if last == Some(current) {
                                continue;
                            }
                            last = Some(current);

                            let runner = state_for_watch.runner.clone();
                            let wasm_path = wasm_path.clone();
                            let res =
                                tokio::task::spawn_blocking(move || runner.load_file(&wasm_path))
                                    .await;

                            match res {
                                Ok(Ok(loaded)) => Some(loaded),
                                Ok(Err(e)) => {
                                    warn!("watch reload failed: {}", e);
                                    None
                                }
                                Err(e) => {
                                    warn!("watch reload join error: {}", e);
                                    None
                                }
                            }
                        };

                        if let Some(loaded) = loaded {
                            let mut w = state_for_watch.workflow.write().await;
                            *w = Arc::new(loaded);
                            info!("workflow reloaded");
                        }
                    }
                });
            }

            let mut app = axum::Router::new()
                .route("/healthz", axum::routing::get(iron_http_healthz))
                .route("/run", axum::routing::post(iron_http_run));

            if metrics {
                app = app.route("/metrics", axum::routing::get(iron_http_metrics));
            }

            let app = app
                .layer(axum::extract::DefaultBodyLimit::max(max_event_bytes))
                .with_state(state);

            let bind = format!("{}:{}", host, port);

            let _bundle_tmp = bundle_tmp;
            if use_tls {
                let cert = tls_cert.expect("tls_cert must be set when use_tls is true");
                let key = tls_key.expect("tls_key must be set when use_tls is true");
                let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await?;

                let listener = std::net::TcpListener::bind(&bind)?;
                listener.set_nonblocking(true)?;
                let addr = listener.local_addr()?;
                println!("listening on https://{}/run", addr);
                if metrics {
                    println!("metrics on https://{}/metrics", addr);
                }

                let handle = axum_server::Handle::new();
                let handle_for_shutdown = handle.clone();
                tokio::spawn(async move {
                    let _ = tokio::signal::ctrl_c().await;
                    handle_for_shutdown.graceful_shutdown(None);
                });

                axum_server::from_tcp_rustls(listener, tls)?
                    .handle(handle)
                    .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
                    .await?;
            } else {
                let listener = tokio::net::TcpListener::bind(&bind).await?;
                let addr = listener.local_addr()?;
                println!("listening on http://{}/run", addr);
                if metrics {
                    println!("metrics on http://{}/metrics", addr);
                }

                axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
                )
                .with_graceful_shutdown(async move {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
            }
        }
        IronAction::Call {
            url,
            auth_token,
            auth_token_file,
            event,
            event_file,
            timeout_ms,
        } => {
            let event_json = if let Some(raw) = event {
                raw
            } else if let Some(src) = event_file.as_deref() {
                read_message_from_source(src)?
            } else {
                r#"{"type":"manual"}"#.to_string()
            };
            let event_value: serde_json::Value = serde_json::from_str(event_json.as_str())
                .map_err(|e| anyhow::anyhow!("invalid --event JSON: {}", e))?;

            let mut endpoint = url.trim().to_string();
            if !endpoint.contains("://") {
                endpoint = format!("http://{}", endpoint);
            }
            endpoint = endpoint.trim_end_matches('/').to_string();
            if !endpoint.ends_with("/run") {
                endpoint = format!("{}/run", endpoint);
            }

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(timeout_ms.max(1)))
                .build()?;

            let auth_token = match auth_token
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                Some(v) => Some(v.to_string()),
                None => match auth_token_file
                    .as_deref()
                    .map(|p| expand_tilde(p))
                    .filter(|p| !p.trim().is_empty())
                {
                    Some(p) => {
                        let raw = std::fs::read_to_string(&p).map_err(|e| {
                            anyhow::anyhow!("failed to read auth token file '{}': {}", p, e)
                        })?;
                        let t = raw.trim().to_string();
                        if t.is_empty() {
                            None
                        } else {
                            Some(t)
                        }
                    }
                    None => None,
                },
            };

            let mut req = client.post(endpoint).json(&event_value);
            if let Some(token) = auth_token {
                req = req.bearer_auth(token);
            }

            let resp = req.send().await?;
            let status = resp.status();
            let txt = resp.text().await?;
            if !status.is_success() {
                return Err(anyhow::anyhow!("server returned {}: {}", status, txt));
            }

            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                println!("{}", txt);
            }
        }
        IronAction::Test {
            path,
            fixtures,
            update,
            timeout_ms,
            fuel,
            max_memory_mb,
            workdir,
            fs_roots,
            allow_bash_prefixes,
            allow_bash_all,
            allow_http_domains,
            http_timeout_ms,
            http_max_bytes,
            kv_path,
            kv_namespace,
            kv_max_value_bytes,
            secrets,
            secrets_file,
            require_signature,
            trust_pubkeys,
            trust_pubkey_files,
        } => {
            let path = PathBuf::from(expand_tilde(&path));
            let (path_for_run, _bundle_tmp) = if is_iron_bundle_path(&path) {
                let tmp =
                    std::env::temp_dir().join(format!("drbot-iron-bundle-{}", Uuid::new_v4()));
                std::fs::create_dir_all(&tmp)?;
                drbot_iron::unpack_bundle_tar_gz(&path, &tmp)?;
                (tmp.clone(), Some(TempDirGuard::new(tmp)))
            } else {
                (path, None)
            };

            let trusted_pubkeys = read_trusted_ed25519_pubkeys(trust_pubkeys, trust_pubkey_files)?;
            let manifest_for_policy = try_load_iron_manifest(&path_for_run)?;
            enforce_iron_signature_policy(
                manifest_for_policy.as_ref(),
                trusted_pubkeys.as_slice(),
                require_signature,
            )?;

            let (wasm_path, _manifest_name) = resolve_iron_wasm_path(&path_for_run)?;

            let fixtures_dir = if let Some(dir) = fixtures
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                PathBuf::from(expand_tilde(dir))
            } else if path_for_run.is_dir() {
                path_for_run.join("fixtures")
            } else {
                return Err(anyhow::anyhow!(
                    "--fixtures is required when path is not a workflow directory"
                ));
            };

            if !fixtures_dir.is_dir() {
                return Err(anyhow::anyhow!(
                    "fixtures dir not found: {}",
                    fixtures_dir.display()
                ));
            }

            let mut run_cfg = drbot_iron::IronRunnerConfig::default();
            run_cfg.timeout = std::time::Duration::from_millis(timeout_ms.max(1));
            if let Some(f) = fuel {
                run_cfg.fuel = Some(f);
            }
            if let Some(mb) = max_memory_mb {
                if mb == 0 {
                    run_cfg.max_memory_bytes = None;
                } else {
                    let bytes = mb.saturating_mul(1024 * 1024);
                    let bytes = bytes.min(usize::MAX as u64) as usize;
                    run_cfg.max_memory_bytes = Some(bytes);
                }
            }

            let mut tool_cfg = drbot_iron::IronToolHostConfig::default();
            if let Some(wd) = workdir.as_deref() {
                tool_cfg.workdir = PathBuf::from(expand_tilde(wd));
            }
            tool_cfg.fs_roots = fs_roots
                .iter()
                .map(|p| PathBuf::from(expand_tilde(p)))
                .collect();
            tool_cfg.bash_allow_prefixes = allow_bash_prefixes;
            tool_cfg.bash_allow_all = allow_bash_all;

            tool_cfg.http_allow_domains = allow_http_domains;
            tool_cfg.http_timeout = std::time::Duration::from_millis(http_timeout_ms.max(1));
            tool_cfg.http_max_bytes = http_max_bytes.max(1).min(usize::MAX as u64) as usize;

            tool_cfg.kv_path = kv_path
                .as_deref()
                .map(|p| expand_tilde(p))
                .filter(|p| !p.trim().is_empty())
                .map(PathBuf::from);
            tool_cfg.kv_namespace = kv_namespace;
            tool_cfg.kv_max_value_bytes = kv_max_value_bytes.max(1).min(usize::MAX as u64) as usize;

            tool_cfg.secrets = load_iron_secrets(secrets, secrets_file)?;

            if let Some(manifest) = manifest_for_policy.as_ref() {
                apply_iron_manifest_capabilities(manifest, &mut tool_cfg)?;
            }

            run_cfg.tools = tool_cfg;

            let runner = drbot_iron::IronRunner::new()?;
            let loaded = runner.load_file(&wasm_path)?;

            let mut inputs: Vec<PathBuf> = Vec::new();
            for entry in std::fs::read_dir(&fixtures_dir)? {
                let entry = entry?;
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if name.ends_with(".out.json") || name.ends_with(".out.txt") {
                    continue;
                }
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                inputs.push(path);
            }
            inputs.sort();

            if inputs.is_empty() {
                return Err(anyhow::anyhow!(
                    "no fixture inputs found under {}",
                    fixtures_dir.display()
                ));
            }

            let mut failures: usize = 0;
            for input_path in inputs {
                let name = input_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("(unknown)");

                let event_json = std::fs::read_to_string(&input_path).map_err(|e| {
                    anyhow::anyhow!("failed to read fixture {}: {}", input_path.display(), e)
                })?;

                let _: serde_json::Value =
                    serde_json::from_str(event_json.as_str()).map_err(|e| {
                        anyhow::anyhow!("invalid JSON in fixture {}: {}", input_path.display(), e)
                    })?;

                let out = runner
                    .run_loaded(&loaded, event_json.as_str(), run_cfg.clone())
                    .await;

                let out = match out {
                    Ok(v) => v,
                    Err(e) => {
                        failures += 1;
                        eprintln!("[FAIL] {}: {}", name, e);
                        continue;
                    }
                };

                let expected_json = input_path.with_extension("out.json");
                let expected_txt = input_path.with_extension("out.txt");

                let out_json = serde_json::from_str::<serde_json::Value>(&out).ok();

                if update {
                    if let Some(v) = out_json.as_ref() {
                        let pretty = serde_json::to_string_pretty(v)?;
                        std::fs::write(
                            &expected_json,
                            pretty
                                + "
",
                        )?;
                        println!("[UPDATE] {} -> {}", name, expected_json.display());
                    } else {
                        std::fs::write(&expected_txt, out.clone())?;
                        println!("[UPDATE] {} -> {}", name, expected_txt.display());
                    }
                    continue;
                }

                if expected_json.exists() {
                    let raw = std::fs::read_to_string(&expected_json).map_err(|e| {
                        anyhow::anyhow!(
                            "failed to read expected output {}: {}",
                            expected_json.display(),
                            e
                        )
                    })?;
                    let expected: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
                        anyhow::anyhow!("invalid expected JSON {}: {}", expected_json.display(), e)
                    })?;
                    let Some(actual) = out_json else {
                        failures += 1;
                        eprintln!("[FAIL] {}: output is not JSON", name);
                        continue;
                    };
                    if actual != expected {
                        failures += 1;
                        eprintln!("[FAIL] {}: JSON mismatch", name);
                        continue;
                    }
                    println!("[OK] {}", name);
                    continue;
                }

                if expected_txt.exists() {
                    let expected = std::fs::read_to_string(&expected_txt).map_err(|e| {
                        anyhow::anyhow!(
                            "failed to read expected output {}: {}",
                            expected_txt.display(),
                            e
                        )
                    })?;
                    if expected.trim_end() != out.trim_end() {
                        failures += 1;
                        eprintln!("[FAIL] {}: output mismatch", name);
                        continue;
                    }
                    println!("[OK] {}", name);
                    continue;
                }

                failures += 1;
                eprintln!(
                    "[FAIL] {}: missing expected output (create {} or {} or pass --update)",
                    name,
                    expected_json.display(),
                    expected_txt.display()
                );
            }

            if failures > 0 {
                return Err(anyhow::anyhow!("{} fixture(s) failed", failures));
            }
        }

        IronAction::Run {
            path,
            event,
            event_file,
            timeout_ms,
            fuel,
            max_memory_mb,
            workdir,
            fs_roots,
            allow_bash_prefixes,
            allow_bash_all,
            allow_http_domains,
            http_timeout_ms,
            http_max_bytes,
            kv_path,
            kv_namespace,
            kv_max_value_bytes,
            secrets,
            secrets_file,
            require_signature,
            trust_pubkeys,
            trust_pubkey_files,
        } => {
            let path = PathBuf::from(expand_tilde(&path));
            let (path_for_run, _bundle_tmp) = if is_iron_bundle_path(&path) {
                let tmp =
                    std::env::temp_dir().join(format!("drbot-iron-bundle-{}", Uuid::new_v4()));
                std::fs::create_dir_all(&tmp)?;
                drbot_iron::unpack_bundle_tar_gz(&path, &tmp)?;
                (tmp.clone(), Some(TempDirGuard::new(tmp)))
            } else {
                (path, None)
            };

            let trusted_pubkeys = read_trusted_ed25519_pubkeys(trust_pubkeys, trust_pubkey_files)?;
            let manifest_for_policy = try_load_iron_manifest(&path_for_run)?;
            enforce_iron_signature_policy(
                manifest_for_policy.as_ref(),
                trusted_pubkeys.as_slice(),
                require_signature,
            )?;

            let (wasm_path, _manifest_name) = resolve_iron_wasm_path(&path_for_run)?;
            let event_json = if let Some(raw) = event {
                raw
            } else if let Some(src) = event_file.as_deref() {
                read_message_from_source(src)?
            } else {
                "{\"type\":\"manual\"}".to_string()
            };
            // Validate JSON early for nicer errors.
            let _: serde_json::Value = serde_json::from_str(event_json.as_str())
                .map_err(|e| anyhow::anyhow!("invalid --event JSON: {}", e))?;

            let mut run_cfg = drbot_iron::IronRunnerConfig::default();
            run_cfg.timeout = std::time::Duration::from_millis(timeout_ms.max(1));
            if let Some(f) = fuel {
                run_cfg.fuel = Some(f);
            }
            if let Some(mb) = max_memory_mb {
                if mb == 0 {
                    run_cfg.max_memory_bytes = None;
                } else {
                    let bytes = mb.saturating_mul(1024 * 1024);
                    let bytes = bytes.min(usize::MAX as u64) as usize;
                    run_cfg.max_memory_bytes = Some(bytes);
                }
            }
            let mut tool_cfg = drbot_iron::IronToolHostConfig::default();
            if let Some(wd) = workdir.as_deref() {
                tool_cfg.workdir = PathBuf::from(expand_tilde(wd));
            }
            tool_cfg.fs_roots = fs_roots
                .iter()
                .map(|p| PathBuf::from(expand_tilde(p)))
                .collect();
            tool_cfg.bash_allow_prefixes = allow_bash_prefixes;
            tool_cfg.bash_allow_all = allow_bash_all;

            tool_cfg.http_allow_domains = allow_http_domains;
            tool_cfg.http_timeout = std::time::Duration::from_millis(http_timeout_ms.max(1));
            tool_cfg.http_max_bytes = http_max_bytes.max(1).min(usize::MAX as u64) as usize;

            tool_cfg.kv_path = kv_path
                .as_deref()
                .map(|p| expand_tilde(p))
                .filter(|p| !p.trim().is_empty())
                .map(PathBuf::from);
            tool_cfg.kv_namespace = kv_namespace;
            tool_cfg.kv_max_value_bytes = kv_max_value_bytes.max(1).min(usize::MAX as u64) as usize;

            tool_cfg.secrets = load_iron_secrets(secrets, secrets_file)?;

            if let Some(manifest) = manifest_for_policy.as_ref() {
                apply_iron_manifest_capabilities(manifest, &mut tool_cfg)?;
            }

            run_cfg.tools = tool_cfg;

            let runner = drbot_iron::IronRunner::new()?;
            let out = runner
                .run_file(&wasm_path, event_json.as_str(), run_cfg)
                .await?;
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                println!("{}", out);
            }
        }
    }

    Ok(())
}

fn resolve_iron_wasm_path(path: &Path) -> Result<(PathBuf, String)> {
    if path.is_file() {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext != "wasm" {
            return Err(anyhow::anyhow!(
                "expected a .wasm file or workflow directory, got: {}",
                path.display()
            ));
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("workflow")
            .to_string();
        return Ok((path.to_path_buf(), name));
    }

    if !path.is_dir() {
        return Err(anyhow::anyhow!("path not found: {}", path.display()));
    }

    let manifest_path = path.join("iron.json");
    let manifest = drbot_iron::IronWorkflowManifest::load(&manifest_path)?;
    let wasm_path = path.join(manifest.wasm_file.as_str());
    if !wasm_path.exists() {
        return Err(anyhow::anyhow!(
            "workflow wasmFile not found: {}",
            wasm_path.display()
        ));
    }
    Ok((wasm_path, manifest.name))
}

fn resolve_iron_workflow_dir(path: &Path) -> Result<PathBuf> {
    if path.is_dir() {
        return Ok(path.to_path_buf());
    }

    if path.is_file() {
        if path.file_name().and_then(|s| s.to_str()) == Some("iron.json") {
            let parent = path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("invalid path: {}", path.display()))?;
            return Ok(parent.to_path_buf());
        }

        return Err(anyhow::anyhow!(
            "expected a workflow directory (or iron.json), got file: {}",
            path.display()
        ));
    }

    Err(anyhow::anyhow!("path not found: {}", path.display()))
}

fn slugify_filename(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }

    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "workflow".to_string()
    } else {
        out
    }
}

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn is_iron_bundle_path(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    name.ends_with(".iron.tgz") || name.ends_with(".tgz") || name.ends_with(".tar.gz")
}

fn decode_base64_maybe_prefixed(input: &str) -> Result<Vec<u8>> {
    let raw = input.trim();
    let raw = raw
        .strip_prefix("base64:")
        .or_else(|| raw.strip_prefix("b64:"))
        .unwrap_or(raw)
        .trim();

    base64::engine::general_purpose::STANDARD
        .decode(raw)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(raw))
        .map_err(|e| anyhow::anyhow!("invalid base64: {}", e))
}

fn decode_hex(input: &str) -> Result<Vec<u8>> {
    fn hex_val(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }

    let raw = input.trim();
    let raw = raw.strip_prefix("hex:").unwrap_or(raw).trim();
    let bytes = raw.as_bytes();
    if bytes.len() % 2 != 0 {
        return Err(anyhow::anyhow!("invalid hex length"));
    }

    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_val(bytes[i]).ok_or_else(|| anyhow::anyhow!("invalid hex"))?;
        let lo = hex_val(bytes[i + 1]).ok_or_else(|| anyhow::anyhow!("invalid hex"))?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

fn read_key_material_from_file(path: &Path) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("failed to read key file '{}': {}", path.display(), e))?;

    if bytes.len() == 32 || bytes.len() == 64 {
        return Ok(bytes);
    }

    let txt = std::str::from_utf8(&bytes)
        .map_err(|_| anyhow::anyhow!("key file is not UTF-8 and is not 32/64 raw bytes"))?;
    let txt = txt.trim();
    if txt.is_empty() {
        return Err(anyhow::anyhow!("key file is empty: {}", path.display()));
    }

    if txt.starts_with("hex:") {
        decode_hex(txt)
    } else {
        decode_base64_maybe_prefixed(txt)
    }
}

fn read_ed25519_seed_from_file(path: &Path) -> Result<[u8; 32]> {
    let bytes = read_key_material_from_file(path)?;
    if bytes.len() == 32 {
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        return Ok(seed);
    }
    if bytes.len() == 64 {
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes[..32]);
        return Ok(seed);
    }
    Err(anyhow::anyhow!(
        "expected an Ed25519 seed (32 bytes) (or 64-byte keypair), got {} bytes",
        bytes.len()
    ))
}

fn read_trusted_ed25519_pubkeys(
    trust_pubkeys: Vec<String>,
    trust_pubkey_files: Vec<String>,
) -> Result<Vec<Vec<u8>>> {
    let mut out = Vec::new();

    for raw in trust_pubkeys {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let bytes = if raw.starts_with("hex:") {
            decode_hex(raw)?
        } else {
            decode_base64_maybe_prefixed(raw)?
        };
        if bytes.len() != 32 {
            return Err(anyhow::anyhow!(
                "trusted Ed25519 public key must be 32 bytes (got {})",
                bytes.len()
            ));
        }
        out.push(bytes);
    }

    for file in trust_pubkey_files {
        let file = file.trim();
        if file.is_empty() {
            continue;
        }
        let file = expand_tilde(file);
        let bytes = read_key_material_from_file(Path::new(&file))?;
        if bytes.len() != 32 {
            return Err(anyhow::anyhow!(
                "trusted Ed25519 public key file must contain 32 bytes (got {})",
                bytes.len()
            ));
        }
        out.push(bytes);
    }

    out.sort();
    out.dedup();
    Ok(out)
}

fn try_load_iron_manifest(path: &Path) -> Result<Option<drbot_iron::IronWorkflowManifest>> {
    if !path.is_dir() {
        return Ok(None);
    }
    let manifest_path = path.join("iron.json");
    if !manifest_path.exists() {
        return Ok(None);
    }
    Ok(Some(drbot_iron::IronWorkflowManifest::load(
        &manifest_path,
    )?))
}

fn enforce_iron_signature_policy(
    manifest: Option<&drbot_iron::IronWorkflowManifest>,
    trusted_pubkeys: &[Vec<u8>],
    require_signature: bool,
) -> Result<()> {
    if require_signature && trusted_pubkeys.is_empty() {
        return Err(anyhow::anyhow!(
            "--require-signature requires at least one --trust-pubkey (or --trust-pubkey-file)"
        ));
    }

    let Some(manifest) = manifest else {
        if require_signature || !trusted_pubkeys.is_empty() {
            return Err(anyhow::anyhow!(
                "signature policy requires a workflow directory with iron.json (a .wasm file has no manifest to verify)"
            ));
        }
        return Ok(());
    };

    if manifest.signature.is_none() {
        if require_signature || !trusted_pubkeys.is_empty() {
            return Err(anyhow::anyhow!("workflow is not signed"));
        }
        return Ok(());
    }

    let public_key = manifest.verify_embedded_signature()?;

    if trusted_pubkeys.is_empty() {
        warn!(
            "workflow manifest is signed, but no trusted public keys were provided; signature is not checked for trust"
        );
        return Ok(());
    }

    let trusted = trusted_pubkeys
        .iter()
        .any(|k| k.as_slice() == public_key.as_slice());
    if !trusted {
        return Err(anyhow::anyhow!(
            "workflow signed by an untrusted key (provide its public key via --trust-pubkey)"
        ));
    }

    Ok(())
}

fn load_iron_secrets(
    secrets: Vec<String>,
    secrets_file: Option<String>,
) -> Result<BTreeMap<String, String>> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();

    if let Some(file) = secrets_file
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let file = expand_tilde(file);
        let raw = std::fs::read_to_string(&file)
            .map_err(|e| anyhow::anyhow!("failed to read secrets file '{}': {}", file, e))?;
        let value: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("invalid secrets JSON ({}): {}", file, e))?;
        let obj = value
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("secrets file must be a JSON object: {}", file))?;

        for (k, v) in obj {
            let name = k.trim();
            if name.is_empty() {
                continue;
            }
            let val = v
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| v.to_string());
            out.insert(name.to_string(), val);
        }
    }

    for raw in secrets {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let (name, value) = raw
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--secret must be NAME=VALUE"))?;
        let name = name.trim();
        if name.is_empty() {
            return Err(anyhow::anyhow!("--secret name is empty"));
        }
        out.insert(name.to_string(), value.to_string());
    }

    Ok(out)
}

fn is_supported_iron_tool(name: &str) -> bool {
    matches!(
        name,
        "fs.read" | "fs.write" | "bash" | "http.fetch" | "kv.get" | "kv.put" | "secrets.get"
    )
}

fn is_iron_tool_enabled_by_host(cfg: &drbot_iron::IronToolHostConfig, tool: &str) -> bool {
    match tool {
        "fs.read" | "fs.write" => !cfg.fs_roots.is_empty(),
        "bash" => cfg.bash_allow_all || !cfg.bash_allow_prefixes.is_empty(),
        "http.fetch" => !cfg.http_allow_domains.is_empty(),
        "kv.get" | "kv.put" => cfg.kv_path.is_some(),
        "secrets.get" => !cfg.secrets.is_empty(),
        _ => false,
    }
}

fn apply_iron_manifest_capabilities(
    manifest: &drbot_iron::IronWorkflowManifest,
    tool_cfg: &mut drbot_iron::IronToolHostConfig,
) -> Result<()> {
    let mut required_tools: BTreeSet<String> = manifest
        .capabilities
        .tools
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if manifest.capabilities.http.is_some() {
        required_tools.insert("http.fetch".to_string());
    }

    if !manifest.capabilities.secrets.is_empty() {
        required_tools.insert("secrets.get".to_string());
    }

    if !required_tools.is_empty() {
        for tool in required_tools.iter() {
            if !is_supported_iron_tool(tool.as_str()) {
                return Err(anyhow::anyhow!(
                    "manifest requires unsupported tool: {}",
                    tool
                ));
            }
            if !is_iron_tool_enabled_by_host(tool_cfg, tool.as_str()) {
                return Err(anyhow::anyhow!(
                    "manifest requires tool '{}' but it is not enabled by the host",
                    tool
                ));
            }
        }

        tool_cfg.allowed_tools = Some(required_tools);
    }

    if !manifest.capabilities.secrets.is_empty() {
        let allowed: BTreeSet<String> = manifest
            .capabilities
            .secrets
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !allowed.is_empty() {
            tool_cfg.allowed_secret_names = Some(allowed);
        }
    }

    if let Some(http) = manifest.capabilities.http.as_ref() {
        if !http.allow_domains.is_empty() {
            let required_domains: BTreeSet<String> = http
                .allow_domains
                .iter()
                .map(|s| s.trim().trim_end_matches('.').to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect();

            let host_domains: BTreeSet<String> = tool_cfg
                .http_allow_domains
                .iter()
                .map(|s| s.trim().trim_end_matches('.').to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect();

            let mut intersection: Vec<String> = host_domains
                .intersection(&required_domains)
                .cloned()
                .collect();
            intersection.sort();

            if intersection.is_empty() {
                return Err(anyhow::anyhow!(
                    "manifest requires http.allowDomains, but none are permitted by the host"
                ));
            }

            tool_cfg.http_allow_domains = intersection;
        }

        if let Some(timeout_ms) = http.timeout_ms {
            let t = std::time::Duration::from_millis(timeout_ms.max(1));
            if t < tool_cfg.http_timeout {
                tool_cfg.http_timeout = t;
            }
        }

        if let Some(max_bytes) = http.max_bytes {
            let m = max_bytes.max(1).min(usize::MAX as u64) as usize;
            tool_cfg.http_max_bytes = tool_cfg.http_max_bytes.min(m);
        }
    }

    Ok(())
}

fn read_rust_package_name(cargo_toml: &Path) -> Result<String> {
    let raw = std::fs::read_to_string(cargo_toml)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", cargo_toml.display(), e))?;
    let parsed: toml::Value = toml::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("invalid Cargo.toml ({}): {}", cargo_toml.display(), e))?;
    let name = parsed
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return Err(anyhow::anyhow!(
            "Cargo.toml missing [package].name: {}",
            cargo_toml.display()
        ));
    }
    Ok(name.to_string())
}

async fn build_iron_rust_workflow(
    workflow_dir: &Path,
    rust_dir_name: &str,
    release: bool,
) -> Result<PathBuf> {
    use tokio::process::Command;

    let manifest_path = workflow_dir.join("iron.json");
    let manifest = drbot_iron::IronWorkflowManifest::load(&manifest_path)?;
    let out_wasm_path = workflow_dir.join(manifest.wasm_file.as_str());

    let rust_dir = workflow_dir.join(rust_dir_name);
    if !rust_dir.is_dir() {
        return Err(anyhow::anyhow!(
            "rust workflow project not found: {}",
            rust_dir.display()
        ));
    }

    let cargo_toml = rust_dir.join("Cargo.toml");
    let pkg_name = read_rust_package_name(&cargo_toml)?;

    let mut cmd = Command::new("cargo");
    cmd.arg("build").arg("--target").arg("wasm32-wasip2");
    if release {
        cmd.arg("--release");
    }
    cmd.current_dir(&rust_dir);

    let output = tokio::time::timeout(std::time::Duration::from_secs(30 * 60), cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("cargo build timed out"))?
        .map_err(|e| anyhow::anyhow!("failed to run cargo build: {}", e))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(anyhow::anyhow!(
            "cargo build failed (exit={}):
{}{}",
            output.status.code().unwrap_or(-1),
            if !stdout.trim().is_empty() {
                format!(
                    "stdout:
{}
",
                    stdout.trim_end()
                )
            } else {
                String::new()
            },
            if !stderr.trim().is_empty() {
                format!(
                    "stderr:
{}
",
                    stderr.trim_end()
                )
            } else {
                String::new()
            },
        ));
    }

    let profile = if release { "release" } else { "debug" };
    let out_dir = rust_dir.join("target").join("wasm32-wasip2").join(profile);

    let candidates = [
        out_dir.join(format!("{}.wasm", pkg_name)),
        out_dir.join(format!("{}.wasm", pkg_name.replace('-', "_"))),
    ];

    let artifact = candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .or_else(|| {
            let mut matches = Vec::new();
            if let Ok(rd) = std::fs::read_dir(&out_dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("wasm") {
                        matches.append(&mut vec![p]);
                    }
                }
            }
            if matches.len() == 1 {
                return Some(matches.remove(0));
            }
            None
        })
        .ok_or_else(|| {
            anyhow::anyhow!("built wasm artifact not found under {}", out_dir.display())
        })?;

    let parent = out_wasm_path.parent().unwrap_or(workflow_dir);
    std::fs::create_dir_all(parent)
        .map_err(|e| anyhow::anyhow!("failed to create {}: {}", parent.display(), e))?;
    std::fs::copy(&artifact, &out_wasm_path).map_err(|e| {
        anyhow::anyhow!(
            "failed to copy {} to {}: {}",
            artifact.display(),
            out_wasm_path.display(),
            e
        )
    })?;

    Ok(out_wasm_path)
}

fn init_iron_workflow_template(dir: &Path, name: &str, force: bool, rust: bool) -> Result<()> {
    if dir.exists() && !force {
        return Err(anyhow::anyhow!(
            "{} already exists (pass --force to overwrite)",
            dir.display()
        ));
    }

    std::fs::create_dir_all(dir)?;
    std::fs::create_dir_all(dir.join("wit"))?;
    std::fs::create_dir_all(dir.join("dist"))?;

    let manifest = drbot_iron::IronWorkflowManifest {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        wasm_file: "dist/workflow.wasm".to_string(),
        description: Some("Iron workflow".to_string()),
        capabilities: Default::default(),
        integrity: None,
        signature: None,
    };
    drbot_iron::IronWorkflowManifest::write(&dir.join("iron.json"), &manifest)?;
    std::fs::write(
        dir.join("wit").join("workflow.wit"),
        drbot_iron::IRON_WORKFLOW_WIT,
    )?;

    if rust {
        init_iron_rust_component_template(dir, force)?;
    }

    let readme = if rust {
        format!(
            "# {}

This is an Iron (WASM-only) workflow for drbot.

- ABI: `wit/workflow.wit`
- Manifest: `iron.json`
- Build output: `dist/workflow.wasm`
- Rust component project: `rust/`

Quickstart (Rust):
1. `drbot iron build . --release`
2. `drbot iron run . --fs-root .`

Notes:
- Install the Rust target once: `rustup target add wasm32-wasip2`
- Tool access is host-controlled (pass `--fs-root` / `--allow-bash-prefix` to `drbot iron run`)

",
            name
        )
    } else {
        format!(
            "# {}

This is an Iron (WASM-only) workflow for drbot.

- ABI: `wit/workflow.wit`
- Manifest: `iron.json`
- Build output: `dist/workflow.wasm`

Next steps:
1. Implement a component that matches the WIT world `workflow`.
2. Run it: `drbot iron run . --fs-root .`

",
            name
        )
    };

    std::fs::write(dir.join("README.md"), readme)?;
    Ok(())
}

fn init_iron_rust_component_template(workflow_dir: &Path, force: bool) -> Result<()> {
    let rust_dir = workflow_dir.join("rust");
    if rust_dir.exists() && !force {
        return Err(anyhow::anyhow!(
            "{} already exists (pass --force to overwrite)",
            rust_dir.display()
        ));
    }

    std::fs::create_dir_all(rust_dir.join("src"))?;
    std::fs::create_dir_all(rust_dir.join("wit"))?;

    let cargo_toml = r#"[package]
name = "workflow"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.51"
serde_json = "1"
"#;

    let lib_rs = r#"wit_bindgen::generate!({
    path: "wit",
    world: "workflow",
});

struct Component;

impl Guest for Component {
    fn run(event_json: String) -> String {
        drbot::iron::host::log("info", "iron workflow: run()");

        let event: serde_json::Value = serde_json::from_str(&event_json)
            .unwrap_or_else(|_| serde_json::json!({ "raw": event_json }));

        serde_json::json!({
            "ok": true,
            "event": event,
        })
        .to_string()
    }
}

export!(Component);
"#;

    std::fs::write(rust_dir.join("Cargo.toml"), cargo_toml)?;
    std::fs::write(rust_dir.join("src").join("lib.rs"), lib_rs)?;
    std::fs::write(
        rust_dir.join("wit").join("workflow.wit"),
        drbot_iron::IRON_WORKFLOW_WIT,
    )?;
    std::fs::write(
        rust_dir.join(".gitignore"),
        "/target
",
    )?;

    Ok(())
}
/// Create a provider from config.
fn create_provider(config: &Config, provider_name: &str) -> Result<Arc<dyn Provider>> {
    use drbot_ollama::OllamaProvider;
    use drbot_openai::OpenAIProvider;

    match provider_name {
        "auto" => {
            // Auto-select: prefer Ollama > Anthropic > OpenAI (local-first)
            if let Some(ollama_config) = &config.providers.ollama {
                let mut p = OllamaProvider::new().with_base_url(&ollama_config.url);
                if let Some(default_model) = &ollama_config.default_model {
                    p = p.with_default_model(default_model);
                }
                info!("Auto-selected provider: ollama");
                return Ok(Arc::new(p));
            }
            if let Some(anthropic_config) = &config.providers.anthropic {
                let mut p = AnthropicProvider::new(&anthropic_config.api_key);
                if let Some(base_url) = &anthropic_config.base_url {
                    p = p.with_base_url(base_url);
                }
                if let Some(default_model) = &anthropic_config.default_model {
                    p = p.with_default_model(default_model);
                }
                if let Some(max_tokens) = anthropic_config.max_tokens {
                    p = p.with_default_max_tokens(max_tokens);
                }
                info!("Auto-selected provider: anthropic");
                return Ok(Arc::new(p));
            }
            if let Some(openai_config) = &config.providers.openai {
                let mut p = OpenAIProvider::new(&openai_config.api_key);
                if let Some(base_url) = &openai_config.base_url {
                    p = p.with_base_url(base_url);
                }
                if let Some(default_model) = &openai_config.default_model {
                    p = p.with_default_model(default_model);
                }
                info!("Auto-selected provider: openai");
                return Ok(Arc::new(p));
            }
            Err(anyhow::anyhow!(
                "No providers configured. Run 'drbot wizard' to configure."
            ))
        }
        "anthropic" | "claude" => {
            let anthropic_config = config.providers.anthropic.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Anthropic provider not configured. Set ANTHROPIC_API_KEY or run 'drbot wizard'."
                )
            })?;
            let mut p = AnthropicProvider::new(&anthropic_config.api_key);
            if let Some(base_url) = &anthropic_config.base_url {
                p = p.with_base_url(base_url);
            }
            if let Some(default_model) = &anthropic_config.default_model {
                p = p.with_default_model(default_model);
            }
            if let Some(max_tokens) = anthropic_config.max_tokens {
                p = p.with_default_max_tokens(max_tokens);
            }
            Ok(Arc::new(p))
        }
        "openai" | "gpt" => {
            let openai_config = config.providers.openai.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "OpenAI provider not configured. Set OPENAI_API_KEY or run 'drbot wizard'."
                )
            })?;
            let mut p = OpenAIProvider::new(&openai_config.api_key);
            if let Some(base_url) = &openai_config.base_url {
                p = p.with_base_url(base_url);
            }
            if let Some(default_model) = &openai_config.default_model {
                p = p.with_default_model(default_model);
            }
            Ok(Arc::new(p))
        }
        "ollama" | "local" => {
            let ollama_config = config.providers.ollama.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Ollama provider not configured. Run 'drbot wizard' to configure.")
            })?;
            let mut p = OllamaProvider::new().with_base_url(&ollama_config.url);
            if let Some(default_model) = &ollama_config.default_model {
                p = p.with_default_model(default_model);
            }
            Ok(Arc::new(p))
        }
        "claude-cli" | "claude-code" => {
            let p = CliProvider::claude_cli();
            p.check_command_exists()?;
            Ok(Arc::new(p))
        }
        "codex-cli" | "codex" => {
            let p = CliProvider::codex_cli();
            p.check_command_exists()?;
            Ok(Arc::new(p))
        }
        other => {
            // Check custom CLI providers from config
            if let Some(cli_cfg) = config.providers.cli.iter().find(|c| c.name == other) {
                let p = CliProvider::from_config(cli_cfg);
                p.check_command_exists()?;
                return Ok(Arc::new(p));
            }
            Err(anyhow::anyhow!(
                "Unknown provider: {}. Supported: auto, anthropic/claude, openai/gpt, ollama/local, claude-cli, codex-cli",
                provider_name
            ))
        }
    }
}

/// Get the session store.
fn get_session_store(config: &Config) -> Result<SqliteSessionStore> {
    let db_path = &config.storage.database_path;
    SqliteSessionStore::new(db_path).map_err(|e| anyhow::anyhow!("{}", e))
}

/// Initialize the persona registry with built-in personas.
fn init_persona_registry() -> PersonaRegistry {
    let registry = PersonaRegistry::new();

    // Add built-in personas
    let concise = Persona::new("concise", "Concise Assistant")
        .with_description("Brief, to-the-point responses")
        .with_style(PersonaStyle::Concise)
        .with_trait(PersonaTrait::Accurate);
    let _ = registry.register(concise);

    let professional = Persona::new("professional", "Professional Assistant")
        .with_description("Formal, business-appropriate responses")
        .with_style(PersonaStyle::Professional)
        .with_trait(PersonaTrait::Accurate)
        .with_trait(PersonaTrait::Helpful);
    let _ = registry.register(professional);

    let creative = Persona::new("creative", "Creative Assistant")
        .with_description("Imaginative and expressive responses")
        .with_style(PersonaStyle::Creative)
        .with_trait(PersonaTrait::Creative);
    let _ = registry.register(creative);

    let teacher = Persona::new("teacher", "Teaching Assistant")
        .with_description("Educational, explains concepts clearly")
        .with_style(PersonaStyle::Educational)
        .with_trait(PersonaTrait::Patient)
        .with_trait(PersonaTrait::Helpful);
    let _ = registry.register(teacher);

    let technical = Persona::new("technical", "Technical Expert")
        .with_description("Precise technical language and detailed answers")
        .with_style(PersonaStyle::Technical)
        .with_trait(PersonaTrait::Accurate);
    let _ = registry.register(technical);

    registry
}

#[derive(Debug, Clone)]
struct ToolCallSpec {
    tool: String,
    args: serde_json::Value,
}

#[derive(Debug, Clone)]
struct ToolModeConfig {
    enabled: bool,
    auto_approve: bool,
    root: PathBuf,
    max_rounds: usize,
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}/{}", home.trim_end_matches('/'), rest);
        }
    }
    if path == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return home;
        }
    }
    path.to_string()
}

fn read_message_from_source(source: &str) -> Result<String> {
    use std::io::Read;

    if source == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        return Ok(buf);
    }

    let path = expand_tilde(source);
    std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("failed to read message file '{}': {}", path, e))
}

fn resolve_single_message(
    message: Option<String>,
    message_file: Option<String>,
) -> Result<Option<String>> {
    if message.is_some() && message_file.is_some() {
        return Err(anyhow::anyhow!(
            "Provide only one of -M/--message or --message-file."
        ));
    }

    if let Some(path) = message_file
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Ok(Some(read_message_from_source(path)?));
    }

    let Some(msg) = message else {
        return Ok(None);
    };
    let trimmed = msg.trim();
    if trimmed == "-" {
        return Ok(Some(read_message_from_source("-")?));
    }
    if let Some(rest) = trimmed.strip_prefix('@') {
        let rest = rest.trim();
        if !rest.is_empty() {
            return Ok(Some(read_message_from_source(rest)?));
        }
    }
    Ok(Some(msg))
}

fn canonicalize_root(root: &Path) -> Result<PathBuf> {
    root.canonicalize()
        .map_err(|e| anyhow::anyhow!("Failed to resolve root '{}': {}", root.display(), e))
}

fn resolve_path_under_root(root: &Path, path: &str, must_exist: bool) -> Result<PathBuf> {
    let expanded = expand_tilde(path);
    let input = Path::new(expanded.as_str());

    if input.is_absolute() {
        let canon = if must_exist {
            input.canonicalize()
        } else {
            // For writes, canonicalize the parent directory.
            let parent = input
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Invalid path: {}", input.display()))?;
            let parent_canon = parent.canonicalize().map_err(|e| {
                anyhow::anyhow!(
                    "Failed to resolve parent directory '{}': {}",
                    parent.display(),
                    e
                )
            })?;
            let name = input
                .file_name()
                .ok_or_else(|| anyhow::anyhow!("Invalid path: {}", input.display()))?;
            Ok(parent_canon.join(name))
        }?;

        if !canon.starts_with(root) {
            return Err(anyhow::anyhow!(
                "Path '{}' is outside tool root '{}'",
                canon.display(),
                root.display()
            ));
        }
        return Ok(canon);
    }

    let joined = root.join(input);
    if must_exist {
        let canon = joined
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("Failed to resolve path '{}': {}", joined.display(), e))?;
        if !canon.starts_with(root) {
            return Err(anyhow::anyhow!(
                "Path '{}' is outside tool root '{}'",
                canon.display(),
                root.display()
            ));
        }
        return Ok(canon);
    }

    // For writes, canonicalize parent directory (must exist).
    let parent = joined
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid path: {}", joined.display()))?;
    let parent_canon = parent.canonicalize().map_err(|e| {
        anyhow::anyhow!(
            "Failed to resolve parent directory '{}': {}",
            parent.display(),
            e
        )
    })?;
    if !parent_canon.starts_with(root) {
        return Err(anyhow::anyhow!(
            "Path '{}' is outside tool root '{}'",
            parent_canon.display(),
            root.display()
        ));
    }
    let name = joined
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Invalid path: {}", joined.display()))?;
    Ok(parent_canon.join(name))
}

fn build_agent_system_prompt(base: Option<String>, tool_root: &Path) -> String {
    let mut s = base.unwrap_or_else(|| "You are drbot, a helpful AI assistant.".to_string());
    s.push_str("\n\n");
    s.push_str("You are operating in a terminal with access to local tools.\n");
    s.push_str("When you need to run a tool, respond ONLY with a fenced code block with language drbot_tool containing JSON.\n");
    s.push_str("The JSON must be either a single object or an array of objects in this form:\n");
    s.push_str("{\"tool\":\"bash\",\"args\":{\"command\":\"git status\"}}\n");
    s.push_str("\nAvailable tools:\n");
    s.push_str("- bash: Run a shell command.\n");
    s.push_str("  args: { \"command\": string, \"cwd\"?: string }\n");
    s.push_str("  If cwd is provided, it must be a path under the tool root.\n");
    s.push_str("- read_file: Read a UTF-8 text file under the tool root.\n");
    s.push_str("  args: { \"path\": string }\n");
    s.push_str("- write_file: Write/replace a UTF-8 text file under the tool root.\n");
    s.push_str("  args: { \"path\": string, \"content\": string }\n");
    s.push_str("- list_dir / list_directory: List a directory under the tool root.\n");
    s.push_str("  args: { \"path\": string }\n");
    s.push_str("- search: Search for a pattern under a path (uses ripgrep when available).\n");
    s.push_str("  args: { \"pattern\": string, \"path\": string }\n");
    s.push_str("- apply_patch: Apply a unified diff patch to files under the tool root.\n");
    s.push_str("  args: { \"patch\": string }\n");
    s.push_str("\nRules:\n");
    s.push_str("- In tool mode, you are an autonomous coding agent: when the user asks you to create/modify/run something, do it with tools (don't ask the user to run commands).\n");
    s.push_str("- Use relative paths unless absolutely necessary.\n");
    s.push_str("- Each bash tool call does NOT preserve state between calls (including cd). Prefer args.cwd instead of `cd ... && ...`.\n");
    s.push_str("- Prefer safe, read-only commands (git status/diff, rg, cargo test, etc.).\n");
    s.push_str("- After a tool runs, you will receive a message starting with [Tool Result] or [Tool Denied]. Use it to continue.\n");
    s.push_str(&format!("\nTool root: {}\n", tool_root.display()));
    s
}

fn resolve_skill_url_timeout_secs() -> u64 {
    std::env::var("DRBOT_CHAT_SKILL_URL_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .or_else(|| {
            std::env::var("DRBOT_OPENCLAW_REMOTE_SKILLS_SYNC_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.trim().parse::<u64>().ok())
        })
        .filter(|v| *v >= 1)
        .unwrap_or(20)
}

fn resolve_skill_url_max_file_bytes() -> usize {
    std::env::var("DRBOT_CHAT_SKILL_URL_MAX_FILE_BYTES")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .or_else(|| {
            std::env::var("DRBOT_OPENCLAW_REMOTE_SKILLS_SYNC_MAX_FILE_BYTES")
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
        })
        .filter(|v| *v >= 1_024)
        .unwrap_or(2 * 1024 * 1024)
}

fn resolve_skill_url_max_relative_docs() -> usize {
    std::env::var("DRBOT_CHAT_SKILL_URL_MAX_RELATIVE_DOCS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .or_else(|| {
            std::env::var("DRBOT_OPENCLAW_REMOTE_SKILLS_SYNC_MAX_RELATIVE_DOCS")
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
        })
        .filter(|v| *v >= 1)
        .unwrap_or(32)
}

fn resolve_skill_url_max_total_bytes() -> usize {
    std::env::var("DRBOT_CHAT_SKILL_URL_MAX_TOTAL_BYTES")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v >= 1_024)
        .unwrap_or(400_000)
}

async fn fetch_url_text(client: &reqwest::Client, url: &str, max_bytes: usize) -> Result<String> {
    fn cache_path_for_url(url: &str) -> Option<PathBuf> {
        let dir = Config::config_dir()?.join("skill_url_cache");
        let key = Uuid::new_v5(&Uuid::NAMESPACE_URL, url.as_bytes()).to_string();
        Some(dir.join(format!("{}.json", key)))
    }

    fn load_cache(path: &Path) -> Option<(String, Option<String>, Option<String>)> {
        let txt = std::fs::read_to_string(path).ok()?;
        let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
        let body = v.get("body")?.as_str()?.to_string();
        let etag = v
            .get("etag")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        let last_modified = v
            .get("last_modified")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        Some((body, etag, last_modified))
    }

    fn store_cache(
        path: &Path,
        url: &str,
        body: &str,
        etag: Option<String>,
        last_modified: Option<String>,
    ) {
        let Some(parent) = path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let v = serde_json::json!({
            "url": url,
            "etag": etag,
            "last_modified": last_modified,
            "body": body,
        });
        if let Ok(txt) = serde_json::to_string(&v) {
            let _ = std::fs::write(path, txt);
        }
    }

    let cache_path = cache_path_for_url(url);
    let cached = cache_path.as_deref().and_then(|p| load_cache(p));

    let mut req = client.get(url);
    if let Some((_, etag, last_modified)) = &cached {
        if let Some(etag) = etag.as_deref() {
            req = req.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        if let Some(lm) = last_modified.as_deref() {
            req = req.header(reqwest::header::IF_MODIFIED_SINCE, lm);
        }
    }

    let res = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            if let Some((body, _, _)) = cached {
                warn!(error = %e, url = %url, "skill-url fetch failed; using cached body");
                return Ok(body);
            }
            return Err(anyhow::anyhow!("failed to fetch {}: {}", url, e));
        }
    };

    if res.status() == reqwest::StatusCode::NOT_MODIFIED {
        if let Some((body, _, _)) = cached {
            return Ok(body);
        }
        return Err(anyhow::anyhow!("http 304 but no cache for {}", url));
    }

    let status = res.status().as_u16();
    if status < 200 || status >= 300 {
        if let Some((body, _, _)) = cached {
            warn!(status, url = %url, "skill-url fetch returned error; using cached body");
            return Ok(body);
        }
        return Err(anyhow::anyhow!("http {} for {}", status, url));
    }

    if let Some(len) = res.content_length() {
        if len as usize > max_bytes {
            return Err(anyhow::anyhow!(
                "remote content too large ({} bytes) for {}",
                len,
                url
            ));
        }
    }

    let etag = res
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let last_modified = res
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let bytes = res.bytes().await?;
    if bytes.len() > max_bytes {
        return Err(anyhow::anyhow!(
            "remote content too large ({} bytes) for {}",
            bytes.len(),
            url
        ));
    }
    let body = String::from_utf8(bytes.to_vec())
        .map_err(|e| anyhow::anyhow!("invalid utf8 from {}: {}", url, e))?;

    if let Some(path) = cache_path.as_deref() {
        store_cache(path, url, &body, etag, last_modified);
    }

    Ok(body)
}

async fn fetch_skill_pack_from_url(url: &str) -> Result<String> {
    let base = reqwest::Url::parse(url)
        .map_err(|e| anyhow::anyhow!("invalid skill url: {} ({})", url, e))?;
    if base.scheme() != "http" && base.scheme() != "https" {
        return Err(anyhow::anyhow!(
            "unsupported skill url scheme: {}",
            base.scheme()
        ));
    }

    let timeout_secs = resolve_skill_url_timeout_secs();
    let max_file_bytes = resolve_skill_url_max_file_bytes();
    let max_relative_docs = resolve_skill_url_max_relative_docs();
    let max_total_bytes = resolve_skill_url_max_total_bytes();
    let ua = format!("drbot/{} (+skill-url)", env!("CARGO_PKG_VERSION"));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .user_agent(ua)
        .build()?;

    let dir_url = base
        .join(".")
        .map_err(|e| anyhow::anyhow!("failed to resolve skill base url: {}", e))?;
    let dir_prefix = dir_url.path().to_string();

    let main_raw = fetch_url_text(&client, base.as_str(), max_file_bytes).await?;
    let main_body = drbot_core::markdown::strip_frontmatter(&main_raw);
    let main_body = main_body.trim();
    if main_body.is_empty() {
        return Err(anyhow::anyhow!("empty skill document: {}", url));
    }
    if main_body.len() > max_total_bytes {
        return Err(anyhow::anyhow!(
            "skill document too large ({} bytes > max {})",
            main_body.len(),
            max_total_bytes
        ));
    }

    const SEP: &str = "\n\n---\n\n";
    let mut used_bytes = main_body.len();
    let mut docs: Vec<String> = Vec::new();
    let mut loaded_docs: Vec<String> = Vec::new();

    let mut seen = std::collections::HashSet::<PathBuf>::new();
    let mut docs_added = 0usize;
    let mut targets = Vec::new();
    targets.extend(drbot_core::markdown::extract_markdown_inline_link_targets(
        main_body,
    ));
    targets.extend(drbot_core::markdown::extract_markdown_reference_definition_targets(main_body));
    for target in targets {
        if docs_added >= max_relative_docs || used_bytes >= max_total_bytes {
            break;
        }
        let token = target.trim().split_whitespace().next().unwrap_or("").trim();
        let token = token.trim_start_matches('<').trim_end_matches('>');
        let Some(rel_path) = drbot_core::markdown::normalize_relative_doc_path_from_target(token)
        else {
            continue;
        };
        let leaf = rel_path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if leaf.eq_ignore_ascii_case("SKILL.md") || leaf.eq_ignore_ascii_case("HEARTBEAT.md") {
            continue;
        }
        let rel_display = rel_path.to_string_lossy().to_string();
        if !seen.insert(rel_path) {
            continue;
        }

        let resolved = match dir_url.join(token) {
            Ok(u) => u,
            Err(_) => continue,
        };
        if resolved.scheme() != base.scheme()
            || resolved.host_str() != base.host_str()
            || !resolved.path().starts_with(&dir_prefix)
        {
            continue;
        }

        let raw = match fetch_url_text(&client, resolved.as_str(), max_file_bytes).await {
            Ok(v) => v,
            Err(err) => {
                warn!(error = %err, url = %resolved.as_str(), "failed to fetch relative skill doc");
                continue;
            }
        };
        let body = drbot_core::markdown::strip_frontmatter(&raw);
        let body = body.trim();
        if body.is_empty() {
            continue;
        }
        if used_bytes
            .saturating_add(SEP.len())
            .saturating_add(body.len())
            > max_total_bytes
        {
            break;
        }
        docs.push(body.to_string());
        loaded_docs.push(rel_display);
        used_bytes = used_bytes
            .saturating_add(SEP.len())
            .saturating_add(body.len());
        docs_added = docs_added.saturating_add(1);
    }

    let mut prefix = String::new();
    prefix.push_str(&format!("[Skill URL] {}\n", url));
    if !loaded_docs.is_empty() {
        prefix.push_str(&format!(
            "[Loaded Docs] {} linked markdown file(s):\n",
            loaded_docs.len()
        ));
        for doc in &loaded_docs {
            prefix.push_str(&format!("- {}\n", doc));
        }
    } else {
        prefix.push_str("[Loaded Docs] (none)\n");
    }
    prefix.push_str("\n---\n\n");

    // Keep the skill pack within bounds; drop the verbose prefix if needed.
    if prefix.len().saturating_add(used_bytes) > max_total_bytes {
        prefix = format!("[Skill URL] {}\n\n---\n\n", url);
        if prefix.len().saturating_add(used_bytes) > max_total_bytes {
            prefix.clear();
        }
    }

    let mut out = String::new();
    out.push_str(&prefix);
    out.push_str(main_body);
    for body in docs {
        out.push_str(SEP);
        out.push_str(&body);
    }

    Ok(out)
}

fn extract_tool_calls(text: &str) -> Vec<ToolCallSpec> {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum BlockKind {
        ToolJson,
        BashCommand,
    }

    fn parse_fence_lang(trimmed: &str) -> Option<String> {
        if !trimmed.starts_with("```") {
            return None;
        }
        Some(
            trimmed
                .trim_start_matches("```")
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase(),
        )
    }

    fn normalize_bash_block(mut block: String) -> String {
        // Strip common prompt prefixes (`$ `) to improve execution success.
        let mut out = String::new();
        for line in block.lines() {
            let l = line.trim_end();
            let l = l.strip_prefix("$ ").unwrap_or(l);
            out.push_str(l);
            out.push('\n');
        }
        out.trim().to_string()
    }

    let mut calls: Vec<ToolCallSpec> = Vec::new();
    let mut in_block = false;
    let mut block = String::new();
    let mut kind: Option<BlockKind> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if !in_block {
            let Some(lang) = parse_fence_lang(trimmed) else {
                continue;
            };
            let block_kind = match lang.as_str() {
                "drbot_tool" | "json" => Some(BlockKind::ToolJson),
                "bash" | "sh" | "shell" | "zsh" => Some(BlockKind::BashCommand),
                _ => None,
            };
            if let Some(bk) = block_kind {
                in_block = true;
                kind = Some(bk);
                block.clear();
            }
            continue;
        }

        if trimmed.starts_with("```") {
            // End block -> parse
            match kind {
                Some(BlockKind::ToolJson) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&block) {
                        match value {
                            serde_json::Value::Array(items) => {
                                for item in items {
                                    if let Some(call) = parse_tool_call_value(&item) {
                                        calls.push(call);
                                    }
                                }
                            }
                            other => {
                                if let Some(call) = parse_tool_call_value(&other) {
                                    calls.push(call);
                                }
                            }
                        }
                    }
                }
                Some(BlockKind::BashCommand) => {
                    let command = normalize_bash_block(std::mem::take(&mut block));
                    if !command.is_empty() {
                        calls.push(ToolCallSpec {
                            tool: "bash".to_string(),
                            args: serde_json::json!({ "command": command }),
                        });
                    }
                }
                None => {}
            }

            in_block = false;
            kind = None;
            block.clear();
            continue;
        }

        block.push_str(line);
        block.push('\n');
    }

    if !calls.is_empty() {
        return calls;
    }

    // Allow lightweight patterns that local models often emit when "tool mode" prompts are ignored.
    // Example: `bash: cd app && pnpm test`
    for line in text.lines() {
        let trimmed = line.trim();
        let lowered = trimmed.to_ascii_lowercase();
        if let Some(rest) = lowered.strip_prefix("bash:") {
            let cmd = trimmed[trimmed.len() - rest.len()..].trim();
            if !cmd.is_empty() {
                return vec![ToolCallSpec {
                    tool: "bash".to_string(),
                    args: serde_json::json!({ "command": cmd }),
                }];
            }
        }
    }

    // Fallback: some models ignore the requested code-fence language and emit a raw JSON object/array.
    // Extract the first JSON value that looks like a tool call.
    fn extract_json_value_bounds(
        s: &str,
        start: usize,
        open: char,
        close: char,
    ) -> Option<(usize, usize)> {
        let slice = s.get(start..)?;
        let mut depth: i64 = 0;
        let mut in_string = false;
        let mut escape = false;

        for (off, ch) in slice.char_indices() {
            if in_string {
                if escape {
                    escape = false;
                    continue;
                }
                match ch {
                    '\\' => escape = true,
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }

            match ch {
                '"' => in_string = true,
                c if c == open => depth += 1,
                c if c == close => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((start, start + off + ch.len_utf8()));
                    }
                }
                _ => {}
            }
        }

        None
    }

    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        let (open, close) = match ch {
            '{' => ('{', '}'),
            '[' => ('[', ']'),
            _ => {
                i += 1;
                continue;
            }
        };

        let Some((start, end)) = extract_json_value_bounds(text, i, open, close) else {
            i += 1;
            continue;
        };
        let json_str = &text[start..end];
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
            match value {
                serde_json::Value::Array(items) => {
                    for item in items {
                        if let Some(call) = parse_tool_call_value(&item) {
                            calls.push(call);
                        }
                    }
                }
                other => {
                    if let Some(call) = parse_tool_call_value(&other) {
                        calls.push(call);
                    }
                }
            }
            if !calls.is_empty() {
                break;
            }
        }

        i = end;
    }

    calls
}

fn parse_tool_call_value(value: &serde_json::Value) -> Option<ToolCallSpec> {
    const SUPPORTED_TOOLS: &[&str] = &[
        "bash",
        "read_file",
        "write_file",
        "list_dir",
        "list_directory",
        "search",
        "apply_patch",
    ];

    let tool = value.get("tool")?.as_str()?.to_string();
    if !SUPPORTED_TOOLS.iter().any(|t| *t == tool) {
        return None;
    }
    let args = value
        .get("args")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    Some(ToolCallSpec { tool, args })
}

fn prompt_approve(prompt: &str) -> Result<bool> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let ans = input.trim().to_lowercase();
    Ok(ans == "y" || ans == "yes")
}

#[derive(Debug, Clone, Default)]
struct BashAutoApprovePolicy {
    allow_all: bool,
    extra_prefixes: Vec<String>,
    override_prefixes: Option<Vec<String>>,
}

fn bash_command_is_safe_for_auto_approve(command: &str, policy: &BashAutoApprovePolicy) -> bool {
    const SAFE_PREFIXES: &[&str] = &[
        "git", "cargo", "rg", "ls", "cat", "sed", "grep", "find", "head", "tail", "wc", "sort",
        "uniq", "pwd", "echo",
    ];
    const FORBIDDEN_COMMANDS: &[&str] = &["sudo", "rm", "mkfs", "dd", "shutdown", "reboot"];

    let cmd = command.trim();
    if cmd.is_empty() {
        return false;
    }

    // Extremely conservative: block clearly destructive commands even when auto-approving.
    //
    // Avoid substring matching like `"dd "` which would incorrectly block innocuous commands
    // containing that sequence (e.g. `pnpm add ...`).
    let normalized = cmd.replace("&&", ";").replace("||", ";").replace('\n', ";");
    for segment in normalized.split(';') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        for part in segment.split('|') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let first = part.split_whitespace().next().unwrap_or("");
            let first_lower = first.to_ascii_lowercase();
            let first_lower = first_lower.trim_start_matches('\\');
            let base = first_lower.rsplit('/').next().unwrap_or(first_lower);
            if FORBIDDEN_COMMANDS.contains(&base) {
                return false;
            }
        }
    }

    if policy.allow_all {
        return true;
    }

    let allowed: Vec<String> = if let Some(list) = &policy.override_prefixes {
        list.clone()
    } else if policy.extra_prefixes.is_empty() {
        SAFE_PREFIXES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    } else {
        let mut seen = std::collections::HashSet::<String>::new();
        let mut out: Vec<String> = Vec::new();
        for p in SAFE_PREFIXES.iter().map(|s| s.to_string()) {
            if seen.insert(p.clone()) {
                out.push(p);
            }
        }
        for p in &policy.extra_prefixes {
            if seen.insert(p.clone()) {
                out.push(p.clone());
            }
        }
        out
    };

    let first = cmd.split_whitespace().next().unwrap_or("");
    allowed
        .iter()
        .any(|p| first == p || first.ends_with(&format!("/{}", p)))
}

fn truncate_for_context(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{}...\n[truncated]", truncated)
}

async fn maybe_spool_tool_output(
    root: &Path,
    tool: &str,
    output: &str,
    max_chars: usize,
) -> Result<String> {
    let char_count = output.chars().count();
    if char_count <= max_chars {
        return Ok(output.to_string());
    }

    let dir = root.join(".drbot").join("tool-output");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| anyhow::anyhow!("failed to create tool-output dir: {}", e))?;

    let mut slug = tool
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        slug = "tool".to_string();
    }

    let path = dir.join(format!("{}-{}.txt", slug, Uuid::new_v4()));
    tokio::fs::write(&path, output.as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("failed to write tool output: {}", e))?;

    let rel = path
        .strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let truncated = truncate_for_context(output, max_chars);

    Ok(format!(
        "[output truncated: {} chars; full output saved to {}]\n{}",
        char_count, rel, truncated
    ))
}

async fn run_bash_tool(root: &Path, cwd: &Path, command: &str) -> Result<(String, bool)> {
    use tokio::process::Command;

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        Command::new("bash")
            .arg("-lc")
            .arg(command)
            .current_dir(cwd)
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("bash tool timed out"))?
    .map_err(|e| anyhow::anyhow!("Failed to run bash tool: {}", e))?;

    let code = output.status.code().unwrap_or(-1);
    let is_error = code != 0;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let mut out = String::new();
    out.push_str(&format!("exit_code: {}\n", code));
    if !stdout.trim().is_empty() {
        out.push_str("stdout:\n");
        out.push_str(&stdout);
        if !stdout.ends_with('\n') {
            out.push('\n');
        }
    }
    if !stderr.trim().is_empty() {
        out.push_str("stderr:\n");
        out.push_str(&stderr);
        if !stderr.ends_with('\n') {
            out.push('\n');
        }
    }

    let rendered = out.trim_end().to_string();
    let rendered = maybe_spool_tool_output(root, "bash", &rendered, 40_000).await?;
    Ok((rendered, is_error))
}

async fn run_read_file_tool(root: &Path, path: &str) -> Result<String> {
    let file = resolve_path_under_root(root, path, true)?;
    let bytes = tokio::fs::read(&file)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", file.display(), e))?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    Ok(truncate_for_context(&text, 120_000))
}

async fn run_write_file_tool(root: &Path, path: &str, content: &str) -> Result<String> {
    let file = resolve_path_under_root(root, path, false)?;
    tokio::fs::write(&file, content)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to write '{}': {}", file.display(), e))?;
    Ok(format!(
        "Wrote {} bytes to {}",
        content.len(),
        file.display()
    ))
}

async fn run_list_dir_tool(root: &Path, path: &str) -> Result<String> {
    let dir = resolve_path_under_root(root, path, true)?;
    let mut entries = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read dir '{}': {}", dir.display(), e))?;
    let mut items = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read dir entry: {}", e))?
    {
        let meta = entry.metadata().await.ok();
        let suffix = if meta.map(|m| m.is_dir()).unwrap_or(false) {
            "/"
        } else {
            ""
        };
        items.push(format!("{}{}", entry.file_name().to_string_lossy(), suffix));
    }
    items.sort();
    let rendered = items.join("\n");
    let rendered = maybe_spool_tool_output(root, "list_dir", &rendered, 40_000).await?;
    Ok(rendered)
}

async fn run_search_tool(root: &Path, pattern: &str, path: &str) -> Result<(String, bool)> {
    use tokio::process::Command;

    let target = resolve_path_under_root(root, path, true)?;

    // Prefer ripgrep; fall back to grep.
    let rg = Command::new("bash")
        .arg("-lc")
        .arg("command -v rg >/dev/null 2>&1")
        .current_dir(root)
        .output()
        .await;

    let (cmd, args): (&str, Vec<String>) = match rg {
        Ok(out) if out.status.success() => (
            "rg",
            vec![
                "-n".to_string(),
                "--hidden".to_string(),
                "--no-heading".to_string(),
                pattern.to_string(),
                target.to_string_lossy().to_string(),
            ],
        ),
        _ => (
            "grep",
            vec![
                "-R".to_string(),
                "-n".to_string(),
                pattern.to_string(),
                target.to_string_lossy().to_string(),
            ],
        ),
    };

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        Command::new(cmd).args(&args).current_dir(root).output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("search tool timed out"))?
    .map_err(|e| anyhow::anyhow!("Failed to run search tool: {}", e))?;

    let code = output.status.code().unwrap_or(-1);
    let is_error = code != 0 && code != 1; // grep/rg use 1 for "no matches"
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let mut out = String::new();
    out.push_str(&format!("exit_code: {}\n", code));
    if !stdout.trim().is_empty() {
        out.push_str("stdout:\n");
        out.push_str(&stdout);
        if !stdout.ends_with('\n') {
            out.push('\n');
        }
    }
    if !stderr.trim().is_empty() {
        out.push_str("stderr:\n");
        out.push_str(&stderr);
        if !stderr.ends_with('\n') {
            out.push('\n');
        }
    }

    let rendered = out.trim_end().to_string();
    let rendered = maybe_spool_tool_output(root, "search", &rendered, 40_000).await?;
    Ok((rendered, is_error))
}

#[derive(Debug, Clone)]
struct UnifiedDiffHunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    lines: Vec<(char, String)>,
}

#[derive(Debug, Clone)]
struct UnifiedDiffFile {
    old_path: String,
    new_path: String,
    hunks: Vec<UnifiedDiffHunk>,
}

fn strip_unified_diff_path(raw: &str) -> String {
    let token = raw.trim().trim_matches('"');
    if token == "/dev/null" {
        return token.to_string();
    }
    token
        .strip_prefix("a/")
        .or_else(|| token.strip_prefix("b/"))
        .unwrap_or(token)
        .to_string()
}

fn parse_unified_diff_hunk_header(line: &str) -> Result<(usize, usize, usize, usize)> {
    let trimmed = line.trim();
    if !trimmed.starts_with("@@") {
        return Err(anyhow::anyhow!("invalid hunk header: {}", line));
    }
    let Some(end) = trimmed[2..].find("@@").map(|i| i + 2) else {
        return Err(anyhow::anyhow!("invalid hunk header: {}", line));
    };
    let body = trimmed[2..end].trim();
    let mut parts = body.split_whitespace();
    let old = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid hunk header: {}", line))?;
    let new = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid hunk header: {}", line))?;

    fn parse_range(token: &str, sigil: char) -> Result<(usize, usize)> {
        let t = token
            .strip_prefix(sigil)
            .ok_or_else(|| anyhow::anyhow!("invalid hunk range: {}", token))?;
        let mut it = t.split(',');
        let start = it
            .next()
            .unwrap_or("")
            .parse::<usize>()
            .map_err(|_| anyhow::anyhow!("invalid hunk start: {}", token))?;
        let count = it
            .next()
            .map(|v| v.parse::<usize>())
            .transpose()
            .map_err(|_| anyhow::anyhow!("invalid hunk count: {}", token))?
            .unwrap_or(1);
        Ok((start, count))
    }

    let (old_start, old_count) = parse_range(old, '-')?;
    let (new_start, new_count) = parse_range(new, '+')?;
    Ok((old_start, old_count, new_start, new_count))
}

fn parse_unified_diff(patch: &str) -> Result<Vec<UnifiedDiffFile>> {
    let mut files: Vec<UnifiedDiffFile> = Vec::new();
    let mut cur_old: Option<String> = None;
    let mut cur_new: Option<String> = None;
    let mut cur_hunks: Vec<UnifiedDiffHunk> = Vec::new();
    let mut cur_hunk: Option<UnifiedDiffHunk> = None;

    fn finish_hunk(cur_hunks: &mut Vec<UnifiedDiffHunk>, cur_hunk: &mut Option<UnifiedDiffHunk>) {
        if let Some(h) = cur_hunk.take() {
            cur_hunks.push(h);
        }
    }

    fn finish_file(
        files: &mut Vec<UnifiedDiffFile>,
        cur_old: &mut Option<String>,
        cur_new: &mut Option<String>,
        cur_hunks: &mut Vec<UnifiedDiffHunk>,
        cur_hunk: &mut Option<UnifiedDiffHunk>,
    ) {
        finish_hunk(cur_hunks, cur_hunk);
        if let (Some(old_path), Some(new_path)) = (cur_old.take(), cur_new.take()) {
            files.push(UnifiedDiffFile {
                old_path,
                new_path,
                hunks: std::mem::take(cur_hunks),
            });
        } else {
            cur_old.take();
            cur_new.take();
            cur_hunks.clear();
        }
    }

    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("--- ") {
            finish_file(
                &mut files,
                &mut cur_old,
                &mut cur_new,
                &mut cur_hunks,
                &mut cur_hunk,
            );
            let token = rest.trim().split_whitespace().next().unwrap_or("");
            if token.is_empty() {
                return Err(anyhow::anyhow!("invalid --- line: {}", line));
            }
            cur_old = Some(strip_unified_diff_path(token));
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++ ") {
            let token = rest.trim().split_whitespace().next().unwrap_or("");
            if token.is_empty() {
                return Err(anyhow::anyhow!("invalid +++ line: {}", line));
            }
            cur_new = Some(strip_unified_diff_path(token));
            continue;
        }
        if line.trim_start().starts_with("@@") {
            finish_hunk(&mut cur_hunks, &mut cur_hunk);
            let (old_start, old_count, new_start, new_count) =
                parse_unified_diff_hunk_header(line)?;
            cur_hunk = Some(UnifiedDiffHunk {
                old_start,
                old_count,
                new_start,
                new_count,
                lines: Vec::new(),
            });
            continue;
        }
        if let Some(h) = cur_hunk.as_mut() {
            if line.starts_with('+') || line.starts_with('-') || line.starts_with(' ') {
                let kind = line.chars().next().unwrap();
                let text = line[1..].to_string();
                h.lines.push((kind, text));
            } else if line.starts_with('\\') {
                // "\ No newline at end of file" - ignore.
            }
        }
    }

    finish_file(
        &mut files,
        &mut cur_old,
        &mut cur_new,
        &mut cur_hunks,
        &mut cur_hunk,
    );
    Ok(files)
}

fn apply_unified_diff_to_text(original: &str, hunks: &[UnifiedDiffHunk]) -> Result<String> {
    let had_trailing_newline = original.ends_with('\n');
    let mut orig_lines: Vec<String> = original.split('\n').map(|s| s.to_string()).collect();
    if had_trailing_newline {
        if orig_lines.last().is_some_and(|l| l.is_empty()) {
            orig_lines.pop();
        }
    }

    let mut out: Vec<String> = Vec::new();
    let mut orig_idx: usize = 0;

    for hunk in hunks {
        let target = hunk.old_start.saturating_sub(1);
        if target < orig_idx {
            return Err(anyhow::anyhow!("overlapping or out-of-order hunks"));
        }
        if target > orig_lines.len() {
            return Err(anyhow::anyhow!("hunk starts past end of file"));
        }

        out.extend_from_slice(&orig_lines[orig_idx..target]);
        let mut pos = target;
        for (kind, text) in &hunk.lines {
            match *kind {
                ' ' => {
                    let cur = orig_lines.get(pos).ok_or_else(|| {
                        anyhow::anyhow!("context past end of file at line {}", pos + 1)
                    })?;
                    if cur != text {
                        return Err(anyhow::anyhow!(
                            "context mismatch at line {} (expected {:?}, found {:?})",
                            pos + 1,
                            text,
                            cur
                        ));
                    }
                    out.push(text.clone());
                    pos += 1;
                }
                '-' => {
                    let cur = orig_lines.get(pos).ok_or_else(|| {
                        anyhow::anyhow!("remove past end of file at line {}", pos + 1)
                    })?;
                    if cur != text {
                        return Err(anyhow::anyhow!(
                            "remove mismatch at line {} (expected {:?}, found {:?})",
                            pos + 1,
                            text,
                            cur
                        ));
                    }
                    pos += 1;
                }
                '+' => {
                    out.push(text.clone());
                }
                other => {
                    return Err(anyhow::anyhow!("unknown hunk line kind: {}", other));
                }
            }
        }
        orig_idx = pos;
    }

    out.extend_from_slice(&orig_lines[orig_idx..]);

    let mut rendered = out.join("\n");
    if had_trailing_newline {
        rendered.push('\n');
    }
    Ok(rendered)
}

async fn run_apply_patch_tool(root: &Path, patch: &str) -> Result<String> {
    let files = parse_unified_diff(patch)?;
    if files.is_empty() {
        return Err(anyhow::anyhow!("apply_patch: no file patches found"));
    }

    let mut out_lines: Vec<String> = Vec::new();
    for fp in files {
        let old_path = fp.old_path.clone();
        let new_path = fp.new_path.clone();

        if old_path != "/dev/null" && new_path != "/dev/null" && old_path != new_path {
            return Err(anyhow::anyhow!(
                "apply_patch: renames are not supported ({} -> {})",
                old_path,
                new_path
            ));
        }

        if new_path == "/dev/null" {
            let file = resolve_path_under_root(root, &old_path, true)?;
            tokio::fs::remove_file(&file)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to delete '{}': {}", file.display(), e))?;
            out_lines.push(format!("deleted {}", old_path));
            continue;
        }

        let target_path = new_path.clone();
        let original = if old_path == "/dev/null" {
            String::new()
        } else {
            let file = resolve_path_under_root(root, &target_path, true)?;
            let bytes = tokio::fs::read(&file)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", file.display(), e))?;
            String::from_utf8_lossy(&bytes).to_string()
        };

        let mut updated = apply_unified_diff_to_text(&original, &fp.hunks)?;
        if old_path == "/dev/null" && !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        let _ = run_write_file_tool(root, &target_path, &updated).await?;
        out_lines.push(format!("patched {}", target_path));
    }

    Ok(out_lines.join("\n"))
}

async fn execute_tool_call(
    tool_cfg: &ToolModeConfig,
    call: &ToolCallSpec,
) -> Result<(String, bool)> {
    match call.tool.as_str() {
        "bash" => {
            let command = call
                .args
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("bash tool requires args.command"))?;
            let cwd = call
                .args
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty());
            let cwd_path = if let Some(cwd) = cwd {
                let dir = resolve_path_under_root(&tool_cfg.root, cwd, true)?;
                if !dir.is_dir() {
                    return Err(anyhow::anyhow!("bash.cwd is not a directory: {}", cwd));
                }
                dir
            } else {
                tool_cfg.root.clone()
            };
            let (output, is_error) = run_bash_tool(&tool_cfg.root, &cwd_path, command).await?;
            Ok((output, is_error))
        }
        "read_file" => {
            let path = call
                .args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("read_file tool requires args.path"))?;
            let output = run_read_file_tool(&tool_cfg.root, path).await?;
            Ok((output, false))
        }
        "write_file" => {
            let path = call
                .args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("write_file tool requires args.path"))?;
            let content = call
                .args
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("write_file tool requires args.content"))?;
            let output = run_write_file_tool(&tool_cfg.root, path, content).await?;
            Ok((output, false))
        }
        "list_dir" | "list_directory" => {
            let path = call
                .args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let output = run_list_dir_tool(&tool_cfg.root, path).await?;
            Ok((output, false))
        }
        "search" => {
            let pattern = call
                .args
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("search tool requires args.pattern"))?;
            let path = call
                .args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let (output, is_error) = run_search_tool(&tool_cfg.root, pattern, path).await?;
            Ok((output, is_error))
        }
        "apply_patch" => {
            let patch = call
                .args
                .get("patch")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("apply_patch tool requires args.patch"))?;
            let output = run_apply_patch_tool(&tool_cfg.root, patch).await?;
            Ok((output, false))
        }
        other => Err(anyhow::anyhow!("Unknown tool: {}", other)),
    }
}

fn should_reprompt_for_tool_calls(user_text: &str, assistant_text: &str) -> bool {
    fn contains_any(haystack: &str, needles: &[&str]) -> bool {
        needles.iter().any(|n| haystack.contains(n))
    }

    let user = user_text.to_ascii_lowercase();
    let assistant = assistant_text.to_ascii_lowercase();

    // If the user is asking for actions/edits/runs, we strongly prefer tool calls.
    let user_intends_actions = contains_any(
        &user,
        &[
            "create",
            "scaffold",
            "build",
            "install",
            "run",
            "execute",
            "fix",
            "update",
            "edit",
            "write",
            "implement",
            "refactor",
            "add ",
            "remove",
            "generate",
            "test",
            "compile",
            "lint",
            "format",
            "apply",
            "patch",
        ],
    );

    // If the assistant is outputting command-like content without tool calls, reprompt.
    let assistant_looks_actionable = contains_any(
        &assistant,
        &[
            "```", // code fences (often `bash` without tool JSON)
            "$ ", "cd ", "pnpm ", "npm ", "npx ", "node ", "cargo ", "git ", "rg ", "cat <<",
        ],
    );

    user_intends_actions || assistant_looks_actionable
}

async fn run_chat(
    config: &Config,
    provider_name: Option<String>,
    model: Option<String>,
    system: Option<String>,
    skill_url: Option<String>,
    agent: bool,
    yes: bool,
    bash_auto_approve_prefixes: Option<String>,
    bash_auto_approve_allowlist: Option<String>,
    bash_auto_approve_all: bool,
    agent_strict: bool,
    root: Option<String>,
    max_tool_rounds: usize,
    single_message: Option<String>,
    message_file: Option<String>,
    stream: bool,
    session_id: Option<String>,
    new_session: bool,
    list_sessions: bool,
    title: Option<String>,
    persona_name: Option<String>,
    list_personas: bool,
    context_size: Option<usize>,
) -> Result<()> {
    // Initialize persona registry
    let persona_registry = init_persona_registry();

    // Handle --list-personas
    if list_personas {
        println!("Available Personas");
        println!("==================");
        println!();
        for persona in persona_registry.list() {
            println!("  {:12} - {}", persona.id, persona.description);
        }
        println!();
        println!("Use with: drbot chat --persona <name>");
        return Ok(());
    }

    // Initialize session store
    let store = get_session_store(config)?;

    // Handle --list-sessions
    if list_sessions {
        let sessions = store
            .list(ListOptions {
                limit: Some(20),
                ..Default::default()
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        if sessions.is_empty() {
            println!("No sessions found.");
        } else {
            println!("Recent Sessions");
            println!("===============");
            println!();
            for session in sessions {
                let title = session.title.as_deref().unwrap_or("Untitled");
                let msg_count = session.metadata.message_count;
                let updated = session.updated_at.format("%Y-%m-%d %H:%M");
                println!(
                    "  {} - {} ({} messages, {})",
                    &session.id.to_string()[..8],
                    title,
                    msg_count,
                    updated
                );
            }
            println!();
            println!("Resume with: drbot chat --session <id>");
        }
        return Ok(());
    }

    let single_message = resolve_single_message(single_message, message_file)?;

    // Determine which provider to use
    let provider_name = provider_name
        .or_else(|| config.providers.default_provider.clone())
        .unwrap_or_else(|| "auto".to_string());

    // Create the provider
    let mut provider = create_provider(config, &provider_name)?;
    let mut current_provider_name = provider.name().to_string();

    // Get or create session
    let user_id = Uuid::nil(); // CLI user - could be configurable
    let mut session = if let Some(sid) = session_id {
        // Resume specific session
        let uuid = if sid.len() >= 8 {
            // Try to find by prefix
            let sessions = store
                .list(ListOptions::default())
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            sessions
                .into_iter()
                .find(|s| s.id.to_string().starts_with(&sid))
                .map(|s| s.id)
        } else {
            None
        };

        let uuid = uuid
            .or_else(|| Uuid::parse_str(&sid).ok())
            .ok_or_else(|| anyhow::anyhow!("Invalid session ID: {}", sid))?;

        store
            .get(uuid)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?
            .ok_or_else(|| anyhow::anyhow!("Session not found: {}", sid))?
    } else if new_session {
        // Force new session
        let mut session = Session::new(user_id, "cli", "terminal");
        // `sessions` has a UNIQUE(channel_type, channel_id) constraint. For CLI we want multiple
        // sessions, so make the channel_id unique per session.
        session.channel_id = format!("terminal:{}", session.id);
        session.title = title.or_else(|| Some("CLI Chat".to_string()));
        session.model = model.clone();
        session.system_prompt = system.clone();
        store
            .create(&session)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        session
    } else {
        // Try to resume last CLI session or create new
        let sessions = store
            .list(ListOptions {
                channel_type: Some("cli".to_string()),
                limit: Some(1),
                ..Default::default()
            })
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        if let Some(mut last_session) = sessions.into_iter().next() {
            // Load messages
            last_session = store
                .get(last_session.id)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?
                .unwrap_or(last_session);
            last_session
        } else {
            let mut session = Session::new(user_id, "cli", "terminal");
            // See note above: ensure CLI sessions don't collide on UNIQUE(channel_type, channel_id).
            session.channel_id = format!("terminal:{}", session.id);
            session.title = title.or_else(|| Some("CLI Chat".to_string()));
            session.model = model.clone();
            session.system_prompt = system.clone();
            store
                .create(&session)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            session
        }
    };

    let tool_root = if agent {
        let root_path = root
            .map(|p| PathBuf::from(expand_tilde(&p)))
            .unwrap_or(std::env::current_dir().map_err(|e| anyhow::anyhow!("{}", e))?);
        canonicalize_root(&root_path)?
    } else {
        canonicalize_root(&std::env::current_dir().map_err(|e| anyhow::anyhow!("{}", e))?)?
    };

    let mut tool_cfg = ToolModeConfig {
        enabled: agent,
        auto_approve: yes,
        root: tool_root,
        max_rounds: max_tool_rounds.max(1),
    };

    let bash_policy = {
        fn parse_csv(raw: &str) -> Vec<String> {
            raw.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        }

        let mut override_prefixes: Option<Vec<String>> = bash_auto_approve_allowlist
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(parse_csv);

        if let Some(list) = &override_prefixes {
            if list.is_empty() {
                override_prefixes = None;
            }
        }

        let mut extra_prefixes: Vec<String> = bash_auto_approve_prefixes
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(parse_csv)
            .unwrap_or_default();

        let mut allow_all = bash_auto_approve_all;
        allow_all = allow_all
            || override_prefixes
                .as_ref()
                .map(|v| v.iter().any(|p| p == "*" || p.eq_ignore_ascii_case("all")))
                .unwrap_or(false)
            || extra_prefixes
                .iter()
                .any(|p| p == "*" || p.eq_ignore_ascii_case("all"));

        if allow_all {
            override_prefixes = None;
            extra_prefixes.clear();
        }

        BashAutoApprovePolicy {
            allow_all,
            extra_prefixes,
            override_prefixes,
        }
    };

    // Apply persona if specified
    let active_persona = persona_name
        .as_ref()
        .and_then(|name| persona_registry.get(name));
    let base_system = if let Some(persona) = &active_persona {
        // Persona system prompt takes precedence, but can be combined with user system prompt
        let persona_prompt = persona.build_system_prompt();
        if let Some(user_system) = system.or_else(|| session.system_prompt.clone()) {
            Some(format!("{}\n\n{}", persona_prompt, user_system))
        } else {
            Some(persona_prompt)
        }
    } else {
        system.or_else(|| session.system_prompt.clone())
    };

    let base_system = if let Some(skill_url) = skill_url
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let skill_pack = fetch_skill_pack_from_url(skill_url).await?;
        Some(match base_system {
            Some(existing) => format!("{}\n\n---\n\n{}", existing.trim(), skill_pack.trim()),
            None => skill_pack,
        })
    } else {
        base_system
    };

    let system_prompt = if tool_cfg.enabled {
        Some(build_agent_system_prompt(
            base_system.clone(),
            &tool_cfg.root,
        ))
    } else {
        base_system.clone()
    };

    // Build chat options (system prompt may change if tool mode toggles)
    let mut options = ChatOptions {
        model: model.clone(),
        max_tokens: Some(4096),
        temperature: if tool_cfg.enabled { Some(0.2) } else { None },
        top_p: None,
        stop_sequences: None,
        system_prompt: system_prompt.clone(),
        tools: None,
    };

    // Initialize context manager
    let context_config = ContextConfig {
        max_tokens: context_size.unwrap_or(100000),
        reserved_for_response: 4096,
        compression_threshold: 0.8,
        min_messages: 5,
        auto_summarize: true,
    };
    let mut context_manager = ContextManager::new(context_config);

    // Add system prompt to context if present
    if let Some(sys) = &system_prompt {
        let _ = context_manager.add_message(&Message::system(sys));
    }

    // Add existing session messages to context
    for msg in &session.messages {
        let _ = context_manager.add_message(msg);
    }

    // Single message mode
    if let Some(msg) = single_message {
        if tool_cfg.enabled && !tool_cfg.auto_approve {
            return Err(anyhow::anyhow!(
                "Tool mode requires approval. Use -y/--yes for non-interactive mode."
            ));
        }

        // Add user message to context and session
        let user_msg = Message::user(&msg);
        let _ = context_manager.add_message(&user_msg);
        session.add_message(user_msg);

        let mut strict_remaining = if agent_strict { 2usize } else { 0usize };
        let mut rounds = 0usize;
        loop {
            rounds += 1;
            if rounds > tool_cfg.max_rounds {
                return Err(anyhow::anyhow!(
                    "Max tool rounds exceeded ({}).",
                    tool_cfg.max_rounds
                ));
            }

            let messages_to_send = context_manager.build_messages();
            let response =
                send_chat(provider.as_ref(), &messages_to_send, &options, stream).await?;

            if !stream {
                println!("{}", response);
            }

            // Add assistant response to context and session
            let assistant_msg = Message::assistant(&response);
            let _ = context_manager.add_message(&assistant_msg);
            session.add_message(assistant_msg);

            if !tool_cfg.enabled {
                break;
            }

            let calls = extract_tool_calls(&response);
            if calls.is_empty() {
                if agent_strict
                    && strict_remaining > 0
                    && should_reprompt_for_tool_calls(&msg, &response)
                {
                    strict_remaining -= 1;
                    let reminder = Message::user(
                        "[Tool Mode Strict] Convert the previous response into tool calls. Reply ONLY with a `drbot_tool` code block containing JSON tool calls (object or array). No prose.",
                    );
                    let _ = context_manager.add_message(&reminder);
                    session.add_message(reminder);
                    continue;
                }
                break;
            }

            for call in calls {
                if call.tool == "bash" {
                    let command = call
                        .args
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !bash_command_is_safe_for_auto_approve(command, &bash_policy) {
                        let denied = Message::user(format!(
                            "[Tool Denied] tool=bash reason=unsafe_for_auto_approve\ncommand: {}",
                            command
                        ));
                        let _ = context_manager.add_message(&denied);
                        session.add_message(denied);
                        continue;
                    }
                }

                let (output, is_error) = match execute_tool_call(&tool_cfg, &call).await {
                    Ok((out, err)) => (out, err),
                    Err(e) => (format!("Error: {}", e), true),
                };

                let tool_result = Message::user(format!(
                    "[Tool Result] tool={}{}\n{}",
                    call.tool,
                    if is_error { " (error)" } else { "" },
                    output
                ));
                let _ = context_manager.add_message(&tool_result);
                session.add_message(tool_result);
            }
        }

        session.update_timestamp();
        let _ = store.update(&session).await;
        return Ok(());
    }

    // Interactive mode
    let session_info = format!("[Session: {}]", &session.id.to_string()[..8]);
    let persona_info = active_persona
        .as_ref()
        .map(|p| format!(" [Persona: {}]", p.name))
        .unwrap_or_default();
    println!(
        "drbot v{} - Interactive Chat ({}){}{}",
        env!("CARGO_PKG_VERSION"),
        provider.name(),
        session_info,
        persona_info
    );
    println!("Commands: /quit, /clear, /save, /info, /sessions, /new, /context, /tools, /approve, /agent, /model");
    if tool_cfg.enabled {
        println!(
            "Tool mode: ON (auto-approve: {})  Root: {}",
            if tool_cfg.auto_approve { "ON" } else { "OFF" },
            tool_cfg.root.display()
        );
    }

    if !session.messages.is_empty() {
        println!("Resuming session with {} messages.", session.messages.len());
    }
    println!();

    loop {
        // Prompt
        print!("You: ");
        io::stdout().flush()?;

        // Read input
        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 {
            // EOF - save and exit
            session.update_timestamp();
            let _ = store.update(&session).await;
            println!("\nSession saved.");
            break;
        }

        let input = input.trim();

        // Check for commands
        if input.is_empty() {
            continue;
        }

        if input == "/quit" || input == "quit" || input == "exit" {
            session.update_timestamp();
            store
                .update(&session)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Session saved. Goodbye!");
            break;
        }

        if input == "/clear" {
            session.clear_messages();
            context_manager.clear();
            if let Some(sys) = &options.system_prompt {
                let _ = context_manager.add_message(&Message::system(sys));
            }
            store
                .update(&session)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Conversation cleared.");
            println!();
            continue;
        }

        if input == "/save" {
            session.update_timestamp();
            store
                .update(&session)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Session saved.");
            println!();
            continue;
        }

        if input == "/info" {
            let state = context_manager.state();
            println!();
            println!("Session ID: {}", session.id);
            println!("Title: {}", session.title.as_deref().unwrap_or("Untitled"));
            println!("Provider: {}", current_provider_name);
            println!(
                "Model: {}",
                options.model.as_deref().unwrap_or("(provider default)")
            );
            println!("Messages: {}", session.messages.len());
            println!(
                "Context: {} tokens used, {} available",
                state.total_tokens, state.available_tokens
            );
            if state.needs_compression {
                println!("  (compression recommended)");
            }
            println!("Created: {}", session.created_at.format("%Y-%m-%d %H:%M"));
            println!("Updated: {}", session.updated_at.format("%Y-%m-%d %H:%M"));
            println!();
            continue;
        }

        if input == "/context" {
            let state = context_manager.state();
            println!();
            println!("Context Status");
            println!("--------------");
            println!("Total tokens: {}", state.total_tokens);
            println!("Available: {}", state.available_tokens);
            println!("Messages: {}", state.message_count);
            println!("Needs compression: {}", state.needs_compression);
            println!();
            continue;
        }

        if input.starts_with("/tools") {
            println!();
            println!("Tools");
            println!("-----");
            println!("bash       - run a shell command (args: command, cwd?)");
            println!("read_file  - read a file under root");
            println!("write_file - write a file under root");
            println!("list_dir / list_directory - list a directory under root");
            println!("search     - search for a pattern under root");
            println!("apply_patch - apply a unified diff patch under root");
            println!();
            println!("Tool mode: {}", if tool_cfg.enabled { "ON" } else { "OFF" });
            println!(
                "Auto-approve: {}",
                if tool_cfg.auto_approve { "ON" } else { "OFF" }
            );
            println!("Strict agent: {}", if agent_strict { "ON" } else { "OFF" });
            println!("Root: {}", tool_cfg.root.display());
            println!("Max tool rounds: {}", tool_cfg.max_rounds);
            println!();
            continue;
        }

        if input.starts_with("/approve") {
            let arg = input.split_whitespace().nth(1).unwrap_or("");
            match arg {
                "on" | "yes" | "true" => {
                    tool_cfg.auto_approve = true;
                    println!("\nAuto-approve: ON\n");
                }
                "off" | "no" | "false" => {
                    tool_cfg.auto_approve = false;
                    println!("\nAuto-approve: OFF\n");
                }
                _ => {
                    println!(
                        "\nAuto-approve: {} (use: /approve on|off)\n",
                        if tool_cfg.auto_approve { "ON" } else { "OFF" }
                    );
                }
            }
            continue;
        }

        if input.starts_with("/agent") {
            let arg = input.split_whitespace().nth(1).unwrap_or("");
            match arg {
                "on" | "yes" | "true" => {
                    tool_cfg.enabled = true;
                    options.system_prompt = Some(build_agent_system_prompt(
                        base_system.clone(),
                        &tool_cfg.root,
                    ));
                    options.temperature = Some(0.2);
                    println!("\nTool mode: ON\n");
                }
                "off" | "no" | "false" => {
                    tool_cfg.enabled = false;
                    options.system_prompt = base_system.clone();
                    options.temperature = None;
                    println!("\nTool mode: OFF\n");
                }
                _ => {
                    println!(
                        "\nTool mode: {} (use: /agent on|off)\n",
                        if tool_cfg.enabled { "ON" } else { "OFF" }
                    );
                }
            }
            continue;
        }

        if input == "/model" || input.starts_with("/model ") {
            let arg = input.strip_prefix("/model").unwrap().trim();
            if arg.is_empty() {
                // Show current provider/model
                let model_display = options.model.as_deref().unwrap_or("(provider default)");
                println!(
                    "\nProvider: {}\nModel: {}\n",
                    current_provider_name, model_display
                );
            } else if arg.contains('/') {
                // provider/model syntax
                let parts: Vec<&str> = arg.splitn(2, '/').collect();
                let prov = parts[0];
                let mdl = parts[1];
                match create_provider(config, prov) {
                    Ok(new_provider) => {
                        current_provider_name = new_provider.name().to_string();
                        provider = new_provider;
                        options.model = Some(mdl.to_string());
                        println!(
                            "\nSwitched to provider: {}, model: {}\n",
                            current_provider_name, mdl
                        );
                    }
                    Err(e) => {
                        println!("\nFailed to switch provider: {}\n", e);
                    }
                }
            } else {
                // Disambiguate: known provider name vs model name
                let known_providers = [
                    "auto",
                    "anthropic",
                    "claude",
                    "openai",
                    "gpt",
                    "ollama",
                    "local",
                    "claude-cli",
                    "claude-code",
                    "codex-cli",
                    "codex",
                ];
                if known_providers.contains(&arg.to_lowercase().as_str()) {
                    match create_provider(config, arg) {
                        Ok(new_provider) => {
                            current_provider_name = new_provider.name().to_string();
                            provider = new_provider;
                            options.model = None; // use provider default
                            println!(
                                "\nSwitched to provider: {} (default model)\n",
                                current_provider_name
                            );
                        }
                        Err(e) => {
                            println!("\nFailed to switch provider: {}\n", e);
                        }
                    }
                } else {
                    // Treat as model name on current provider
                    options.model = Some(arg.to_string());
                    println!(
                        "\nModel set to: {} (provider: {})\n",
                        arg, current_provider_name
                    );
                }
            }
            continue;
        }

        if input == "/sessions" {
            let sessions = store
                .list(ListOptions {
                    limit: Some(10),
                    ..Default::default()
                })
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            println!();
            for s in sessions {
                let marker = if s.id == session.id { " *" } else { "" };
                let title = s.title.as_deref().unwrap_or("Untitled");
                println!(
                    "  {} - {} ({} msgs){}",
                    &s.id.to_string()[..8],
                    title,
                    s.metadata.message_count,
                    marker
                );
            }
            println!();
            continue;
        }

        if input == "/new" {
            // Save current session
            session.update_timestamp();
            store
                .update(&session)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            // Create new session
            session = Session::new(user_id, "cli", "terminal");
            session.channel_id = format!("terminal:{}", session.id);
            session.title = Some("CLI Chat".to_string());
            session.model = options.model.clone();
            session.system_prompt = base_system.clone();
            store
                .create(&session)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            // Reset context manager
            context_manager.clear();
            if let Some(sys) = &options.system_prompt {
                let _ = context_manager.add_message(&Message::system(sys));
            }

            println!("New session started: {}", &session.id.to_string()[..8]);
            println!();
            continue;
        }

        // Add user message to context and session
        let user_msg = Message::user(input);
        let _ = context_manager.add_message(&user_msg);
        session.add_message(user_msg.clone());

        let mut strict_remaining = if agent_strict { 2usize } else { 0usize };
        let mut rounds = 0usize;
        loop {
            rounds += 1;
            if rounds > tool_cfg.max_rounds {
                eprintln!(
                    "\nError: Max tool rounds exceeded ({}).",
                    tool_cfg.max_rounds
                );
                break;
            }

            // Build messages from context manager (handles compression if needed)
            let messages_to_send = context_manager.build_messages();

            // Send and get response
            print!("\nAssistant: ");
            io::stdout().flush()?;

            let response =
                match send_chat(provider.as_ref(), &messages_to_send, &options, stream).await {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("\nError: {}", e);
                        // Remove the failed user message from session
                        session.messages.pop();
                        break;
                    }
                };

            if !stream {
                println!("{}", response);
            }
            println!();

            // Add assistant response to context and session
            let assistant_msg = Message::assistant(&response);
            let _ = context_manager.add_message(&assistant_msg);
            session.add_message(assistant_msg);

            if !tool_cfg.enabled {
                // Auto-save after each exchange
                session.update_timestamp();
                let _ = store.update(&session).await;
                break;
            }

            let calls = extract_tool_calls(&response);
            if calls.is_empty() {
                if agent_strict
                    && strict_remaining > 0
                    && should_reprompt_for_tool_calls(input, &response)
                {
                    strict_remaining -= 1;
                    let reminder = Message::user(
                        "[Tool Mode Strict] Convert the previous response into tool calls. Reply ONLY with a `drbot_tool` code block containing JSON tool calls (object or array). No prose.",
                    );
                    let _ = context_manager.add_message(&reminder);
                    session.add_message(reminder);
                    continue;
                }
                // Auto-save after each exchange
                session.update_timestamp();
                let _ = store.update(&session).await;
                break;
            }

            for call in calls {
                let mut approved = false;

                if tool_cfg.auto_approve {
                    approved = if call.tool == "bash" {
                        let command = call
                            .args
                            .get("command")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        bash_command_is_safe_for_auto_approve(command, &bash_policy)
                    } else {
                        true
                    };
                }

                if !approved {
                    match call.tool.as_str() {
                        "bash" => {
                            let command = call
                                .args
                                .get("command")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let cwd = call.args.get("cwd").and_then(|v| v.as_str()).unwrap_or("");
                            if cwd.trim().is_empty() {
                                println!("[Tool] bash\n  command: {}", command);
                            } else {
                                println!(
                                    "[Tool] bash\n  cwd: {}\n  command: {}",
                                    cwd.trim(),
                                    command
                                );
                            }
                        }
                        "read_file" => {
                            let path = call.args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            println!("[Tool] read_file\n  path: {}", path);
                        }
                        "write_file" => {
                            let path = call.args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            let bytes = call
                                .args
                                .get("content")
                                .and_then(|v| v.as_str())
                                .map(|s| s.len())
                                .unwrap_or(0);
                            println!("[Tool] write_file\n  path: {}\n  bytes: {}", path, bytes);
                        }
                        "list_dir" | "list_directory" => {
                            let path = call
                                .args
                                .get("path")
                                .and_then(|v| v.as_str())
                                .unwrap_or(".");
                            println!("[Tool] list_dir\n  path: {}", path);
                        }
                        "search" => {
                            let pattern = call
                                .args
                                .get("pattern")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let path = call
                                .args
                                .get("path")
                                .and_then(|v| v.as_str())
                                .unwrap_or(".");
                            println!("[Tool] search\n  pattern: {}\n  path: {}", pattern, path);
                        }
                        "apply_patch" => {
                            let bytes = call
                                .args
                                .get("patch")
                                .and_then(|v| v.as_str())
                                .map(|s| s.len())
                                .unwrap_or(0);
                            println!("[Tool] apply_patch\n  bytes: {}", bytes);
                        }
                        _ => {
                            println!("[Tool] {}", call.tool);
                        }
                    }

                    approved = prompt_approve("Approve? [y/N] ")? && tool_cfg.enabled;
                }

                if !approved {
                    let denied = Message::user(format!(
                        "[Tool Denied] tool={} reason=user_denied",
                        call.tool
                    ));
                    let _ = context_manager.add_message(&denied);
                    session.add_message(denied);
                    continue;
                }

                let (output, is_error) = match execute_tool_call(&tool_cfg, &call).await {
                    Ok((out, err)) => (out, err),
                    Err(e) => (format!("Error: {}", e), true),
                };

                println!(
                    "[Tool Result] {}{}\n",
                    call.tool,
                    if is_error { " (error)" } else { "" }
                );
                println!("{}", output);
                println!();

                let tool_result = Message::user(format!(
                    "[Tool Result] tool={}{}\n{}",
                    call.tool,
                    if is_error { " (error)" } else { "" },
                    output
                ));
                let _ = context_manager.add_message(&tool_result);
                session.add_message(tool_result);
            }

            // Auto-save after tool runs
            session.update_timestamp();
            let _ = store.update(&session).await;
        }
    }

    Ok(())
}

async fn send_chat(
    provider: &dyn Provider,
    messages: &[Message],
    options: &ChatOptions,
    stream: bool,
) -> Result<String> {
    if stream {
        let mut full_content = String::new();
        let mut stream_result = provider
            .stream(messages, options.clone())
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        while let Some(event) = stream_result.next().await {
            match event {
                StreamEvent::Delta { content } => {
                    print!("{}", content);
                    io::stdout().flush()?;
                    full_content.push_str(&content);
                }
                StreamEvent::Error { message } => {
                    return Err(anyhow::anyhow!("{}", message));
                }
                _ => {}
            }
        }
        Ok(full_content)
    } else {
        let response = provider
            .chat(messages, options.clone())
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        Ok(response.content)
    }
}

fn show_config(config: &Config) {
    println!("drbot Configuration");
    println!("===================");
    println!();
    println!("Gateway:");
    println!("  Host: {}", config.gateway.host);
    println!("  Port: {}", config.gateway.port);
    println!(
        "  Auth: {}",
        if config.gateway.auth_token.is_some() {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!();
    println!("Providers:");
    println!(
        "  Default: {}",
        config
            .providers
            .default_provider
            .as_deref()
            .unwrap_or("none")
    );
    if config.providers.anthropic.is_some() {
        println!("  Anthropic: configured");
    }
    if config.providers.openai.is_some() {
        println!("  OpenAI: configured");
    }
    if config.providers.ollama.is_some() {
        println!("  Ollama: configured");
    }
    println!();
    println!("Storage:");
    println!("  Database: {}", config.storage.database_path.display());
    println!("  Media: {}", config.storage.media_path.display());
}

#[cfg(test)]
mod tool_mode_tests {
    use super::*;

    #[test]
    fn extract_single_tool_call() {
        let text = r#"
hello
```drbot_tool
{"tool":"bash","args":{"command":"git status"}}
```
"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool, "bash");
        assert_eq!(
            calls[0].args.get("command").and_then(|v| v.as_str()),
            Some("git status")
        );
    }

    #[test]
    fn extract_multiple_tool_calls_array() {
        let text = r#"
```drbot_tool
[
  {"tool":"read_file","args":{"path":"src/main.rs"}},
  {"tool":"search","args":{"pattern":"run_chat","path":"src"}}
]
```
"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].tool, "read_file");
        assert_eq!(calls[1].tool, "search");
    }

    #[test]
    fn extract_bash_fence_as_tool_call() {
        let text = r#"
Here is the command:

```bash
cd app && pnpm test
```
"#;
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool, "bash");
        assert_eq!(
            calls[0].args.get("command").and_then(|v| v.as_str()),
            Some("cd app && pnpm test")
        );
    }

    #[test]
    fn extract_bash_colon_line_as_tool_call() {
        let text = "bash: cd app && pnpm build";
        let calls = extract_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool, "bash");
        assert_eq!(
            calls[0].args.get("command").and_then(|v| v.as_str()),
            Some("cd app && pnpm build")
        );
    }

    #[test]
    fn safe_bash_auto_approve() {
        let policy = BashAutoApprovePolicy::default();
        assert!(bash_command_is_safe_for_auto_approve("git status", &policy));
        assert!(bash_command_is_safe_for_auto_approve(
            "cargo test -q",
            &policy
        ));
        assert!(!bash_command_is_safe_for_auto_approve("rm -rf /", &policy));
        assert!(!bash_command_is_safe_for_auto_approve("sudo ls", &policy));
        assert!(!bash_command_is_safe_for_auto_approve(
            "dd if=/dev/zero of=/dev/null",
            &policy
        ));
        assert!(!bash_command_is_safe_for_auto_approve(
            "./script.sh",
            &policy
        ));
    }

    #[test]
    fn safe_bash_auto_approve_does_not_false_positive_on_add() {
        let policy = BashAutoApprovePolicy {
            allow_all: false,
            extra_prefixes: vec!["cd".to_string(), "pnpm".to_string()],
            override_prefixes: None,
        };
        assert!(bash_command_is_safe_for_auto_approve(
            "cd app && pnpm add @solana/client",
            &policy
        ));
    }
}

async fn run_doctor(config: &Config) -> Result<()> {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Severity {
        Ok,
        Info,
        Warning,
        Critical,
    }

    impl Severity {
        fn icon(self) -> &'static str {
            match self {
                Severity::Ok => "✓",
                Severity::Info => "i",
                Severity::Warning => "!",
                Severity::Critical => "✗",
            }
        }

        fn label(self) -> &'static str {
            match self {
                Severity::Ok => "OK",
                Severity::Info => "INFO",
                Severity::Warning => "WARN",
                Severity::Critical => "CRIT",
            }
        }
    }

    #[derive(Debug, Clone)]
    struct Finding {
        severity: Severity,
        title: String,
        details: Vec<String>,
    }

    fn env_truthy(key: &str) -> bool {
        matches!(
            std::env::var(key)
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "1" | "true" | "yes" | "on"
        )
    }

    fn is_loopback_host(host: &str) -> bool {
        let mut normalized = host.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return false;
        }
        if normalized == "localhost" {
            return true;
        }
        if normalized.starts_with('[') && normalized.ends_with(']') && normalized.len() >= 2 {
            normalized = normalized[1..normalized.len() - 1].to_string();
        }
        normalized
            .parse::<std::net::IpAddr>()
            .ok()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
    }

    fn normalize_channel_name(raw: &str) -> String {
        let lowered = raw.trim().to_ascii_lowercase();
        match lowered.as_str() {
            "imsg" => "imessage".to_string(),
            "gchat" | "google-chat" => "googlechat".to_string(),
            other => other.to_string(),
        }
    }

    println!("drbot Doctor");
    println!("============");
    println!();

    let mut findings: Vec<Finding> = Vec::new();

    // --------------------------------------------
    // Directories + state
    // --------------------------------------------

    if let Some(dir) = Config::config_dir() {
        if dir.exists() {
            findings.push(Finding {
                severity: Severity::Ok,
                title: "Config directory".to_string(),
                details: vec![dir.display().to_string()],
            });
        } else {
            findings.push(Finding {
                severity: Severity::Warning,
                title: "Config directory missing".to_string(),
                details: vec![
                    dir.display().to_string(),
                    "Create it or run `drbot config` / the wizard to initialize.".to_string(),
                ],
            });
        }
    } else {
        findings.push(Finding {
            severity: Severity::Warning,
            title: "Config directory unknown".to_string(),
            details: vec!["Unable to resolve OS config dir.".to_string()],
        });
    }

    if let Some(dir) = Config::data_dir() {
        if dir.exists() {
            findings.push(Finding {
                severity: Severity::Ok,
                title: "Data directory".to_string(),
                details: vec![dir.display().to_string()],
            });
        } else {
            findings.push(Finding {
                severity: Severity::Warning,
                title: "Data directory missing".to_string(),
                details: vec![
                    dir.display().to_string(),
                    "Create it (or start the gateway once) so drbot can persist state.".to_string(),
                ],
            });
        }
    }

    let openclaw_state_dir =
        drbot_gateway::openclaw_paths::resolve_openclaw_state_dir(config).unwrap_or_default();
    if openclaw_state_dir.as_os_str().is_empty() {
        findings.push(Finding {
            severity: Severity::Info,
            title: "OpenClaw state dir".to_string(),
            details: vec!["Not resolved (will fall back to defaults).".to_string()],
        });
    } else {
        findings.push(Finding {
            severity: if openclaw_state_dir.exists() {
                Severity::Ok
            } else {
                Severity::Info
            },
            title: "OpenClaw state dir".to_string(),
            details: vec![openclaw_state_dir.display().to_string()],
        });
    }

    // --------------------------------------------
    // Gateway exposure / auth
    // --------------------------------------------

    let host = config.gateway.host.trim();
    let auth_token = config.gateway.auth_token.as_deref().unwrap_or("").trim();
    let loopback = is_loopback_host(host);
    let tls = config.gateway.tls_enabled;

    if !loopback && auth_token.is_empty() {
        findings.push(Finding {
            severity: Severity::Critical,
            title: "Gateway exposed without auth token".to_string(),
            details: vec![
                format!("bind: {}:{}", host, config.gateway.port),
                "Anyone who can reach this port can fully control your agent.".to_string(),
                "Fix: set `gateway.auth_token` or bind to 127.0.0.1.".to_string(),
            ],
        });
    } else if !loopback {
        let weak = auth_token.len() < 16
            || matches!(
                auth_token.to_ascii_lowercase().as_str(),
                "changeme" | "change-me" | "password" | "token"
            );
        findings.push(Finding {
            severity: if weak {
                Severity::Warning
            } else {
                Severity::Info
            },
            title: "Gateway bound to network interface".to_string(),
            details: vec![
                format!("bind: {}:{}", host, config.gateway.port),
                "Ensure your auth token is strong and kept secret.".to_string(),
                if weak {
                    "Auth token looks weak; use a long random token.".to_string()
                } else {
                    "Auth token configured.".to_string()
                },
            ],
        });
        if !tls {
            findings.push(Finding {
                severity: Severity::Warning,
                title: "Gateway TLS disabled on network bind".to_string(),
                details: vec![
                    "Traffic will be plaintext unless you run behind a TLS-terminating proxy."
                        .to_string(),
                    "Fix: set `gateway.tls_enabled=true` (and configure cert/key) or bind to localhost."
                        .to_string(),
                ],
            });
        }
    } else {
        findings.push(Finding {
            severity: Severity::Ok,
            title: "Gateway bind policy".to_string(),
            details: vec![format!("bind: {}:{}", host, config.gateway.port)],
        });
    }

    // --------------------------------------------
    // SSRF / network fetch surfaces
    // --------------------------------------------

    let web_fetch_allows_private = env_truthy("DRBOT_OPENCLAW_WEB_FETCH_ALLOW_PRIVATE")
        || env_truthy("DRBOT_WEB_FETCH_ALLOW_PRIVATE");
    if web_fetch_allows_private {
        findings.push(Finding {
            severity: Severity::Warning,
            title: "SSRF policy: web_fetch allows private network".to_string(),
            details: vec![
                "This enables fetching internal URLs (e.g. 127.0.0.1 / RFC1918).".to_string(),
                "Fix: unset DRBOT_OPENCLAW_WEB_FETCH_ALLOW_PRIVATE / DRBOT_WEB_FETCH_ALLOW_PRIVATE."
                    .to_string(),
            ],
        });
    }

    let browser_allows_private = env_truthy("DRBOT_OPENCLAW_BROWSER_ALLOW_PRIVATE")
        || env_truthy("DRBOT_BROWSER_ALLOW_PRIVATE");
    if browser_allows_private {
        findings.push(Finding {
            severity: Severity::Warning,
            title: "SSRF policy: browser requests allow private network".to_string(),
            details: vec![
                "This enables screenshots/status checks against internal URLs.".to_string(),
                "Fix: unset DRBOT_OPENCLAW_BROWSER_ALLOW_PRIVATE / DRBOT_BROWSER_ALLOW_PRIVATE."
                    .to_string(),
            ],
        });
    }

    let skills_allows_private = env_truthy("DRBOT_OPENCLAW_SKILLS_ALLOW_PRIVATE")
        || env_truthy("DRBOT_OPENCLAW_REMOTE_SKILLS_ALLOW_PRIVATE")
        || env_truthy("DRBOT_OPENCLAW_SKILLS_INSTALL_ALLOW_PRIVATE");
    if skills_allows_private {
        findings.push(Finding {
            severity: Severity::Warning,
            title: "SSRF policy: remote skills allow private network".to_string(),
            details: vec![
                "Remote SKILL.md sync/install can fetch internal URLs.".to_string(),
                "Fix: unset DRBOT_OPENCLAW_SKILLS_ALLOW_PRIVATE / DRBOT_OPENCLAW_REMOTE_SKILLS_ALLOW_PRIVATE / DRBOT_OPENCLAW_SKILLS_INSTALL_ALLOW_PRIVATE."
                    .to_string(),
            ],
        });
    }

    // --------------------------------------------
    // Tool exposure / approvals bypasses
    // --------------------------------------------

    let bash_allow_all = env_truthy("DRBOT_OPENCLAW_AGENT_BASH_ALLOW_ALL")
        || env_truthy("DRBOT_AGENT_BASH_ALLOW_ALL");
    if bash_allow_all {
        findings.push(Finding {
            severity: Severity::Warning,
            title: "Agent bash tool: allow-all enabled".to_string(),
            details: vec![
                "This removes the command-prefix sandbox for agent bash runs.".to_string(),
                "If exec approvals are disabled for a session, this can be dangerous.".to_string(),
                "Fix: unset DRBOT_OPENCLAW_AGENT_BASH_ALLOW_ALL / DRBOT_AGENT_BASH_ALLOW_ALL."
                    .to_string(),
            ],
        });
    }

    let bash_allowlist_raw = std::env::var("DRBOT_OPENCLAW_AGENT_BASH_ALLOWLIST")
        .ok()
        .or_else(|| std::env::var("DRBOT_AGENT_BASH_ALLOWLIST").ok())
        .unwrap_or_default();
    if !bash_allowlist_raw.trim().is_empty() {
        let tokens = bash_allowlist_raw
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let has_all = tokens.iter().any(|s| s == "*" || s == "all");
        if has_all {
            findings.push(Finding {
                severity: Severity::Warning,
                title: "Agent bash tool: allowlist contains '*'".to_string(),
                details: vec![
                    "This effectively enables allow-all behavior.".to_string(),
                    "Fix: restrict DRBOT_OPENCLAW_AGENT_BASH_ALLOWLIST / DRBOT_AGENT_BASH_ALLOWLIST to a small set of prefixes."
                        .to_string(),
                ],
            });
        }
    }

    if env_truthy("DRBOT_OPENCLAW_ALLOW_EXTERNAL_RESTART") {
        findings.push(Finding {
            severity: Severity::Warning,
            title: "External restart enabled".to_string(),
            details: vec![
                "DRBOT_OPENCLAW_ALLOW_EXTERNAL_RESTART allows SIGUSR1 restarts without an in-process authorization window."
                    .to_string(),
                "Fix: unset DRBOT_OPENCLAW_ALLOW_EXTERNAL_RESTART unless you need it.".to_string(),
            ],
        });
    }

    if std::env::var("DRBOT_OPENCLAW_SEND_WRITE").ok().as_deref() == Some("1") {
        findings.push(Finding {
            severity: Severity::Warning,
            title: "Send approvals bypass enabled".to_string(),
            details: vec![
                "DRBOT_OPENCLAW_SEND_WRITE=1 bypasses send tool approvals when sendPolicy=ask."
                    .to_string(),
                "Fix: unset DRBOT_OPENCLAW_SEND_WRITE.".to_string(),
            ],
        });
    }

    // --------------------------------------------
    // Channels allowlists
    // --------------------------------------------

    let enabled = config
        .channels
        .enabled
        .iter()
        .map(|c| normalize_channel_name(c))
        .filter(|c| !c.is_empty())
        .collect::<std::collections::HashSet<_>>();

    let is_enabled = |name: &str| enabled.contains(&normalize_channel_name(name));
    if is_enabled("telegram") {
        match &config.channels.telegram {
            None => findings.push(Finding {
                severity: Severity::Warning,
                title: "Telegram enabled but not configured".to_string(),
                details: vec!["Set `channels.telegram.bot_token`.".to_string()],
            }),
            Some(cfg) => {
                if cfg.allowed_users.is_empty() && cfg.allowed_chats.is_empty() {
                    findings.push(Finding {
                        severity: Severity::Warning,
                        title: "Telegram allowlists empty".to_string(),
                        details: vec![
                            "Telegram will accept inbound messages from any user/chat by default."
                                .to_string(),
                            "Fix: set `channels.telegram.allowed_users` and/or `channels.telegram.allowed_chats`."
                                .to_string(),
                        ],
                    });
                }
            }
        }
    }

    if is_enabled("discord") {
        match &config.channels.discord {
            None => findings.push(Finding {
                severity: Severity::Warning,
                title: "Discord enabled but not configured".to_string(),
                details: vec![
                    "Set `channels.discord.bot_token` and `channels.discord.application_id`."
                        .to_string(),
                ],
            }),
            Some(cfg) => {
                if cfg.allowed_guilds.is_empty() {
                    findings.push(Finding {
                        severity: Severity::Warning,
                        title: "Discord allowlist empty".to_string(),
                        details: vec![
                            "`channels.discord.allowed_guilds` is empty (allows all guilds)."
                                .to_string(),
                            "Fix: set allowed guild ids.".to_string(),
                        ],
                    });
                }
            }
        }
    }

    if is_enabled("whatsapp") {
        match &config.channels.whatsapp {
            None => findings.push(Finding {
                severity: Severity::Warning,
                title: "WhatsApp enabled but not configured".to_string(),
                details: vec!["Set `channels.whatsapp.session_path` (and bridge url).".to_string()],
            }),
            Some(cfg) => {
                if cfg.allowed_numbers.is_empty() {
                    findings.push(Finding {
                        severity: Severity::Warning,
                        title: "WhatsApp allowlist empty".to_string(),
                        details: vec![
                            "`channels.whatsapp.allowed_numbers` is empty (allows all senders)."
                                .to_string(),
                            "Fix: set allowed phone numbers.".to_string(),
                        ],
                    });
                }
            }
        }
    }

    if is_enabled("signal") {
        match &config.channels.signal {
            None => findings.push(Finding {
                severity: Severity::Warning,
                title: "Signal enabled but not configured".to_string(),
                details: vec![
                    "Set `channels.signal.socket_path` and `channels.signal.phone_number`."
                        .to_string(),
                ],
            }),
            Some(cfg) => {
                if cfg.allowed_numbers.is_empty() {
                    findings.push(Finding {
                        severity: Severity::Warning,
                        title: "Signal allowlist empty".to_string(),
                        details: vec![
                            "`channels.signal.allowed_numbers` is empty (allows all senders)."
                                .to_string(),
                            "Fix: set allowed phone numbers.".to_string(),
                        ],
                    });
                }
            }
        }
    }

    if is_enabled("imessage") {
        match &config.channels.imessage {
            None => findings.push(Finding {
                severity: Severity::Warning,
                title: "iMessage enabled but not configured".to_string(),
                details: vec![
                    "Set `channels.imessage.allowed_contacts` (optional db path).".to_string(),
                ],
            }),
            Some(cfg) => {
                if cfg.allowed_contacts.is_empty() {
                    findings.push(Finding {
                        severity: Severity::Warning,
                        title: "iMessage allowlist empty".to_string(),
                        details: vec![
                            "`channels.imessage.allowed_contacts` is empty (allows all contacts)."
                                .to_string(),
                            "Fix: set allowed contacts.".to_string(),
                        ],
                    });
                }
            }
        }
    }

    if is_enabled("matrix") {
        match &config.channels.matrix {
            None => findings.push(Finding {
                severity: Severity::Warning,
                title: "Matrix enabled but not configured".to_string(),
                details: vec![
                    "Set `channels.matrix.homeserver_url`, `user_id`, `access_token`.".to_string(),
                ],
            }),
            Some(cfg) => {
                if cfg.allowed_rooms.is_empty() {
                    findings.push(Finding {
                        severity: Severity::Warning,
                        title: "Matrix allowlist empty".to_string(),
                        details: vec![
                            "`channels.matrix.allowed_rooms` is empty (allows all rooms)."
                                .to_string(),
                            "Fix: set allowed room ids.".to_string(),
                        ],
                    });
                }
            }
        }
    }

    if is_enabled("slack") && config.channels.slack.is_none() {
        findings.push(Finding {
            severity: Severity::Warning,
            title: "Slack enabled but not configured".to_string(),
            details: vec![
                "Set `channels.slack.bot_token`, `app_token`, and `signing_secret`.".to_string(),
            ],
        });
    }

    // --------------------------------------------
    // Providers (best-effort)
    // --------------------------------------------

    println!("Providers:");
    println!(
        "  Anthropic: {}",
        if config.providers.anthropic.is_some() {
            "configured"
        } else {
            "not configured"
        }
    );
    println!(
        "  OpenAI: {}",
        if config.providers.openai.is_some() {
            "configured"
        } else {
            "not configured"
        }
    );
    if let Some(ollama) = &config.providers.ollama {
        match check_ollama_health(&ollama.url).await {
            Ok(true) => println!("  Ollama: running ({})", ollama.url),
            Ok(false) => println!("  Ollama: configured but not responding ({})", ollama.url),
            Err(e) => println!("  Ollama: error checking ({}): {}", ollama.url, e),
        }
    } else {
        println!("  Ollama: not configured");
    }
    println!();

    // --------------------------------------------
    // Report
    // --------------------------------------------

    let mut crit = 0usize;
    let mut warn = 0usize;
    let mut info = 0usize;
    for f in &findings {
        match f.severity {
            Severity::Critical => crit += 1,
            Severity::Warning => warn += 1,
            Severity::Info => info += 1,
            Severity::Ok => {}
        }
    }

    println!("Checks:");
    for f in &findings {
        println!(
            "  {} [{}] {}",
            f.severity.icon(),
            f.severity.label(),
            f.title
        );
        for line in &f.details {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            println!("      {}", trimmed);
        }
    }

    println!();
    if crit == 0 && warn == 0 {
        println!("Summary: OK ({} info)", info);
    } else {
        println!(
            "Summary: {} critical, {} warnings, {} info",
            crit, warn, info
        );
    }

    Ok(())
}

/// Check if Ollama is running by hitting its health endpoint.
async fn check_ollama_health(url: &str) -> Result<bool> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    // Ollama's API endpoint - try to list models
    let api_url = format!("{}/api/tags", url.trim_end_matches('/'));

    match client.get(&api_url).send().await {
        Ok(response) => Ok(response.status().is_success()),
        Err(e) if e.is_timeout() => Ok(false),
        Err(e) if e.is_connect() => Ok(false),
        Err(e) => Err(anyhow::anyhow!("{}", e)),
    }
}

/// Interactive setup wizard.
async fn run_wizard() -> Result<()> {
    use drbot_core::config::{AnthropicConfig, OllamaConfig, OpenAIConfig};

    println!();
    println!("╔═══════════════════════════════════════╗");
    println!("║     drbot Setup Wizard                ║");
    println!("╚═══════════════════════════════════════╝");
    println!();
    println!("This wizard will help you configure drbot.");
    println!();

    let mut config = Config::default();

    // --- Provider Configuration ---
    println!("┌─ Provider Configuration ─────────────────┐");
    println!();

    // Anthropic
    print!("Configure Anthropic Claude? [Y/n]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let configure_anthropic =
        input.trim().is_empty() || input.trim().to_lowercase().starts_with('y');

    if configure_anthropic {
        // Check environment variable first
        let env_key = std::env::var("ANTHROPIC_API_KEY").ok();

        let api_key = if let Some(key) = env_key {
            println!("  Found ANTHROPIC_API_KEY in environment.");
            print!("  Use environment variable? [Y/n]: ");
            io::stdout().flush()?;
            input.clear();
            io::stdin().read_line(&mut input)?;
            if input.trim().is_empty() || input.trim().to_lowercase().starts_with('y') {
                key
            } else {
                print!("  Enter Anthropic API key: ");
                io::stdout().flush()?;
                input.clear();
                io::stdin().read_line(&mut input)?;
                input.trim().to_string()
            }
        } else {
            print!("  Enter Anthropic API key: ");
            io::stdout().flush()?;
            input.clear();
            io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        };

        if !api_key.is_empty() {
            print!("  Default model [claude-sonnet-4-20250514]: ");
            io::stdout().flush()?;
            input.clear();
            io::stdin().read_line(&mut input)?;
            let model = if input.trim().is_empty() {
                "claude-sonnet-4-20250514".to_string()
            } else {
                input.trim().to_string()
            };

            config.providers.anthropic = Some(AnthropicConfig {
                api_key,
                default_model: Some(model),
                base_url: None,
                headers: Default::default(),
                max_tokens: None,
            });
            println!("  Anthropic configured.");
        }
    }
    println!();

    // OpenAI
    print!("Configure OpenAI? [Y/n]: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    let configure_openai = input.trim().is_empty() || input.trim().to_lowercase().starts_with('y');

    if configure_openai {
        let env_key = std::env::var("OPENAI_API_KEY").ok();

        let api_key = if let Some(key) = env_key {
            println!("  Found OPENAI_API_KEY in environment.");
            print!("  Use environment variable? [Y/n]: ");
            io::stdout().flush()?;
            input.clear();
            io::stdin().read_line(&mut input)?;
            if input.trim().is_empty() || input.trim().to_lowercase().starts_with('y') {
                key
            } else {
                print!("  Enter OpenAI API key: ");
                io::stdout().flush()?;
                input.clear();
                io::stdin().read_line(&mut input)?;
                input.trim().to_string()
            }
        } else {
            print!("  Enter OpenAI API key: ");
            io::stdout().flush()?;
            input.clear();
            io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        };

        if !api_key.is_empty() {
            print!("  Default model [gpt-4o]: ");
            io::stdout().flush()?;
            input.clear();
            io::stdin().read_line(&mut input)?;
            let model = if input.trim().is_empty() {
                "gpt-4o".to_string()
            } else {
                input.trim().to_string()
            };

            config.providers.openai = Some(OpenAIConfig {
                api_key,
                default_model: Some(model),
                base_url: None,
                headers: Default::default(),
                organization: None,
            });
            println!("  OpenAI configured.");
        }
    }
    println!();

    // Ollama
    print!("Configure Ollama (local models)? [y/N]: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    let configure_ollama = input.trim().to_lowercase().starts_with('y');

    if configure_ollama {
        print!("  Ollama URL [http://localhost:11434]: ");
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        let url = if input.trim().is_empty() {
            "http://localhost:11434".to_string()
        } else {
            input.trim().to_string()
        };

        print!("  Default model [llama3.2]: ");
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        let model = if input.trim().is_empty() {
            "llama3.2".to_string()
        } else {
            input.trim().to_string()
        };

        config.providers.ollama = Some(OllamaConfig {
            url,
            default_model: Some(model),
        });
        println!("  Ollama configured.");
    }
    println!();

    // Default provider
    let mut providers_available = Vec::new();
    if config.providers.anthropic.is_some() {
        providers_available.push("anthropic");
    }
    if config.providers.openai.is_some() {
        providers_available.push("openai");
    }
    if config.providers.ollama.is_some() {
        providers_available.push("ollama");
    }

    if !providers_available.is_empty() {
        let default = providers_available[0];
        print!("Default provider [{}]: ", default);
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        let chosen = if input.trim().is_empty() {
            default.to_string()
        } else {
            input.trim().to_string()
        };
        config.providers.default_provider = Some(chosen);
    }

    println!();
    println!("└───────────────────────────────────────────┘");
    println!();

    // --- Gateway Configuration ---
    println!("┌─ Gateway Configuration ───────────────────┐");
    println!();

    print!("Gateway host [127.0.0.1]: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if !input.trim().is_empty() {
        config.gateway.host = input.trim().to_string();
    }

    print!("Gateway port [18789]: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if !input.trim().is_empty() {
        if let Ok(port) = input.trim().parse() {
            config.gateway.port = port;
        }
    }

    print!("Require authentication token? [y/N]: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase().starts_with('y') {
        print!("  Enter auth token: ");
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        if !input.trim().is_empty() {
            config.gateway.auth_token = Some(input.trim().to_string());
        }
    }

    println!();
    println!("└───────────────────────────────────────────┘");
    println!();

    // --- Save Configuration ---
    let config_path = Config::config_dir()
        .map(|d| d.join("config.toml"))
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    println!("Configuration will be saved to:");
    println!("  {}", config_path.display());
    println!();

    print!("Save configuration? [Y/n]: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;

    if input.trim().is_empty() || input.trim().to_lowercase().starts_with('y') {
        // Ensure config directory exists
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Serialize and save
        let toml_str = toml::to_string_pretty(&config)?;
        std::fs::write(&config_path, toml_str)?;

        println!();
        println!("Configuration saved!");
        println!();
        println!("You can now run:");
        println!("  drbot chat     - Start interactive chat");
        println!("  drbot tui      - Launch terminal UI");
        println!("  drbot gateway  - Start the gateway server");
        println!("  drbot doctor   - Verify configuration");
    } else {
        println!("Configuration not saved.");
    }

    println!();
    Ok(())
}

/// Manage channels.
async fn run_channels(
    config: &Config,
    action: ChannelsAction,
    config_path: Option<&str>,
) -> Result<()> {
    fn resolve_config_path(cli_config_path: Option<&str>) -> Result<std::path::PathBuf> {
        if let Some(path) = cli_config_path {
            return Ok(std::path::PathBuf::from(path));
        }
        Config::config_dir()
            .map(|d| d.join("config.toml"))
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))
    }

    fn save_config(path: &std::path::Path, config: &Config) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(config)?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }

    fn normalize_channel_name(name: &str) -> Option<&'static str> {
        match name {
            "whatsapp" => Some("whatsapp"),
            "telegram" => Some("telegram"),
            "discord" => Some("discord"),
            "slack" => Some("slack"),
            "signal" => Some("signal"),
            "imessage" => Some("imessage"),
            "matrix" => Some("matrix"),
            "webchat" => Some("webchat"),
            _ => None,
        }
    }

    match action {
        ChannelsAction::List => {
            println!("Configured Channels");
            println!("===================");
            println!();

            // List all possible channels with their status
            let channels = [
                ("whatsapp", config.channels.whatsapp.is_some()),
                ("telegram", config.channels.telegram.is_some()),
                ("discord", config.channels.discord.is_some()),
                ("slack", config.channels.slack.is_some()),
                ("signal", config.channels.signal.is_some()),
                ("imessage", config.channels.imessage.is_some()),
                ("matrix", config.channels.matrix.is_some()),
                ("webchat", config.channels.webchat.is_some()),
            ];

            for (name, configured) in channels {
                let enabled = config.channels.enabled.iter().any(|c| c == name);
                let status = match (configured, enabled) {
                    (true, true) => "configured, enabled",
                    (true, false) => "configured, disabled",
                    (false, true) => "not configured (but enabled)",
                    (false, false) => "not configured",
                };
                println!("  {:<12} {}", name, status);
            }
            println!();
        }
        ChannelsAction::Status { name } => {
            if let Some(channel_name) = name {
                let channel_name = normalize_channel_name(&channel_name)
                    .ok_or_else(|| anyhow::anyhow!("Unknown channel: {}", channel_name))?;
                let enabled = config.channels.enabled.iter().any(|c| c == channel_name);

                // Show status for specific channel
                let status = match channel_name {
                    "whatsapp" => {
                        if let Some(wa) = &config.channels.whatsapp {
                            format!(
                                "WhatsApp: configured, {} (session: {}, bridge: {})",
                                if enabled { "enabled" } else { "disabled" },
                                wa.session_path.display(),
                                wa.bridge_url
                            )
                        } else {
                            format!(
                                "WhatsApp: not configured{}",
                                if enabled { " (enabled)" } else { "" }
                            )
                        }
                    }
                    "telegram" => {
                        if let Some(_tg) = &config.channels.telegram {
                            format!(
                                "Telegram: configured, {}",
                                if enabled { "enabled" } else { "disabled" }
                            )
                        } else {
                            format!(
                                "Telegram: not configured{}",
                                if enabled { " (enabled)" } else { "" }
                            )
                        }
                    }
                    "discord" => {
                        if let Some(_dc) = &config.channels.discord {
                            format!(
                                "Discord: configured, {}",
                                if enabled { "enabled" } else { "disabled" }
                            )
                        } else {
                            format!(
                                "Discord: not configured{}",
                                if enabled { " (enabled)" } else { "" }
                            )
                        }
                    }
                    "slack" => {
                        if let Some(_sl) = &config.channels.slack {
                            format!(
                                "Slack: configured, {}",
                                if enabled { "enabled" } else { "disabled" }
                            )
                        } else {
                            format!(
                                "Slack: not configured{}",
                                if enabled { " (enabled)" } else { "" }
                            )
                        }
                    }
                    "webchat" => {
                        if let Some(wc) = &config.channels.webchat {
                            format!(
                                "WebChat: configured, {} (host: {}, port: {})",
                                if enabled { "enabled" } else { "disabled" },
                                wc.host,
                                wc.port
                            )
                        } else {
                            format!(
                                "WebChat: not configured{}",
                                if enabled { " (enabled)" } else { "" }
                            )
                        }
                    }
                    "matrix" => {
                        if let Some(mx) = &config.channels.matrix {
                            format!(
                                "Matrix: configured, {} (homeserver: {}, user: {}, allowed_rooms: {})",
                                if enabled { "enabled" } else { "disabled" },
                                mx.homeserver_url,
                                mx.user_id,
                                mx.allowed_rooms.len()
                            )
                        } else {
                            format!(
                                "Matrix: not configured{}",
                                if enabled { " (enabled)" } else { "" }
                            )
                        }
                    }
                    _ => format!("Unknown channel: {}", channel_name),
                };
                println!("{}", status);
            } else {
                // Show status for all channels
                println!("Channel Status");
                println!("==============");
                println!("Run 'drbot channels list' for config/enablement state.");
                println!("Connectivity checks are channel-specific and may require credentials.");
            }
        }
        ChannelsAction::Enable { name } => {
            let name = normalize_channel_name(&name)
                .ok_or_else(|| anyhow::anyhow!("Unknown channel: {}", name))?;

            let path = resolve_config_path(config_path)?;
            let mut cfg = if path.exists() {
                Config::from_file(&path)?
            } else {
                config.clone()
            };

            if !cfg.channels.enabled.iter().any(|c| c == name) {
                cfg.channels.enabled.push(name.to_string());
            }

            save_config(&path, &cfg)?;

            println!("Enabled '{}'.", name);
            println!("Config: {}", path.display());
            println!("Note: you still need the channel's config section (tokens/URLs) to actually connect.");
        }
        ChannelsAction::Disable { name } => {
            let name = normalize_channel_name(&name)
                .ok_or_else(|| anyhow::anyhow!("Unknown channel: {}", name))?;

            let path = resolve_config_path(config_path)?;
            let mut cfg = if path.exists() {
                Config::from_file(&path)?
            } else {
                config.clone()
            };

            cfg.channels.enabled.retain(|c| c != name);
            save_config(&path, &cfg)?;

            println!("Disabled '{}'.", name);
            println!("Config: {}", path.display());
        }
    }

    Ok(())
}
