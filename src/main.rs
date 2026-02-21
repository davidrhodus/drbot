//! drbot - A personal AI assistant
//!
//! This is the main entry point for the drbot binary.

mod gateway_client;

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
use drbot_tool_mode::{
    bash_command_is_safe_for_auto_approve, build_agent_system_prompt_with_policy,
    execute_tool_call, extract_tool_calls, resolve_tool_root_with_allowlist,
    should_reprompt_for_tool_calls, BashAutoApprovePolicy, ToolModeConfig,
};
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
        /// Provider to use (auto, anthropic/claude, openai/gpt, ollama/local, claude-cli, codex-cli, codex-oss)
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

    /// Manage a project-local knowledge base (.drbot/)
    Kb {
        #[command(subcommand)]
        action: KbAction,
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

/// Knowledge base subcommands.
#[derive(Subcommand)]
enum KbAction {
    /// Initialize a project-local knowledge base in .drbot/
    Init {
        /// Directory to initialize (defaults to current directory; uses nearest git root when available)
        #[arg(long)]
        dir: Option<String>,
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
        }) => {
            // Match the default `drbot` behavior: launch the TUI and ensure the gateway
            // is running (attach if another instance already bound the port).
            run_tui_with_gateway(config, provider, model, system).await
        }
        Some(Commands::Kb { action }) => run_kb(action).await,
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
            // Default: start TUI + gateway
            run_tui_with_gateway(config, None, None, None).await
        }
    }
}

async fn run_tui_with_gateway(
    mut config: Config,
    provider_name: Option<String>,
    model: Option<String>,
    system: Option<String>,
) -> Result<()> {
    'outer: loop {
        // If the gateway is already running (e.g., another drbot instance), just attach the TUI.
        if gateway_is_listening(&config).await {
            let action = run_tui(
                &config,
                provider_name.clone(),
                model.clone(),
                system.clone(),
                true,
            )
            .await?;
            match action {
                drbot_tui::ExitAction::Quit => return Ok(()),
                drbot_tui::ExitAction::LaunchWizard => {
                    run_wizard().await?;
                    config = Config::load().unwrap_or_default();
                    continue;
                }
            }
        }

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let gateway_config = config.clone();

        let gateway_task = tokio::spawn(async move {
            run_gateway_with_external_shutdown(gateway_config, async move {
                let _ = shutdown_rx.await;
            })
            .await
        });

        // Wait briefly for the gateway to start accepting connections.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if gateway_is_listening(&config).await {
                break;
            }
            if gateway_task.is_finished() {
                let res = gateway_task.await;
                if gateway_is_listening(&config).await {
                    // Another process won the race and bound the port. Treat as "gateway already running".
                    let action = run_tui(
                        &config,
                        provider_name.clone(),
                        model.clone(),
                        system.clone(),
                        true,
                    )
                    .await?;
                    match action {
                        drbot_tui::ExitAction::Quit => return Ok(()),
                        drbot_tui::ExitAction::LaunchWizard => {
                            run_wizard().await?;
                            config = Config::load().unwrap_or_default();
                            continue 'outer;
                        }
                    }
                }
                let err = match res {
                    Ok(Ok(())) => anyhow::anyhow!("Gateway exited before TUI started"),
                    Ok(Err(e)) => e,
                    Err(e) => anyhow::anyhow!("Gateway task failed: {}", e),
                };
                return Err(err);
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        let action = run_tui(
            &config,
            provider_name.clone(),
            model.clone(),
            system.clone(),
            true,
        )
        .await;

        let _ = shutdown_tx.send(());

        let gateway_result = match gateway_task.await {
            Ok(res) => res,
            Err(e) => Err(anyhow::anyhow!("Gateway task failed: {}", e)),
        };

        let action = action?;
        let _ = gateway_result?;

        match action {
            drbot_tui::ExitAction::Quit => return Ok(()),
            drbot_tui::ExitAction::LaunchWizard => {
                run_wizard().await?;
                config = Config::load().unwrap_or_default();
                continue;
            }
        }
    }
}

async fn run_tui(
    config: &Config,
    provider_name: Option<String>,
    model: Option<String>,
    system: Option<String>,
    gateway_running: bool,
) -> Result<drbot_tui::ExitAction> {
    // NOTE: The TUI is gateway-backed. Provider/model selection happens via the gateway
    // (Ctrl+P, Ctrl+M, /provider, /model), including CLI providers like `claude-cli`/`codex-cli`.
    // Keep the CLI args for compatibility, but don't hard-fail on unknown providers.
    let _ = provider_name;

    let tui_config = AppConfig {
        provider_type: drbot_tui::ProviderType::default(),
        api_key: None,
        base_url: None,
        model,
        system_prompt: system,
        gateway_url: Some(gateway_ws_url(config)),
        gateway_auth_token: gateway_login_token(config),
        gateway_running,
        max_history: 100,
    };

    drbot_tui::run(tui_config)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))
}

fn gateway_ws_url(config: &Config) -> String {
    let scheme = if config.gateway.tls_enabled {
        "wss"
    } else {
        "ws"
    };
    // If the gateway is bound to a wildcard address, clients should connect via localhost.
    let host = config.gateway.host.trim();
    let host = match host {
        "" | "0.0.0.0" | "::" => "127.0.0.1",
        other => other,
    };
    format!("{}://{}:{}/ws", scheme, host, config.gateway.port)
}

/// Token to use for `auth.login` when connecting to the gateway.
///
/// If `gateway.auth_token` is configured, use it (required for auth-required gateways).
/// Otherwise, use a stable local token so sessions persist across reconnects even when
/// the gateway doesn't require auth.
fn gateway_login_token(config: &Config) -> Option<String> {
    if let Some(token) = config
        .gateway
        .auth_token
        .as_deref()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
    {
        return Some(token.to_string());
    }

    let Some(dir) = Config::config_dir() else {
        return None;
    };
    let path = dir.join("gateway-client-token");

    if let Ok(existing) = std::fs::read_to_string(&path) {
        let token = existing.trim().to_string();
        if !token.is_empty() {
            return Some(token);
        }
    }

    let token = uuid::Uuid::new_v4().to_string();
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(&path, format!("{}\n", token));
    }
    Some(token)
}

async fn gateway_is_listening(config: &Config) -> bool {
    let host = config.gateway.host.trim();
    let host = match host {
        "" | "0.0.0.0" | "::" => "127.0.0.1",
        other => other,
    };
    let addr = format!("{}:{}", host, config.gateway.port);
    tokio::time::timeout(
        std::time::Duration::from_millis(150),
        tokio::net::TcpStream::connect(addr),
    )
    .await
    .is_ok_and(|r| r.is_ok())
}

async fn run_gateway(config: Config) -> Result<()> {
    run_gateway_with_external_shutdown(config, std::future::pending::<()>()).await
}

async fn run_gateway_with_external_shutdown(
    config: Config,
    external_shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<()> {
    info!("drbot v{} starting...", env!("CARGO_PKG_VERSION"));

    let gateway = Gateway::new(config);
    let state_for_shutdown = gateway.state();

    const ACTION_STOP: u8 = 1;
    const ACTION_RESTART: u8 = 2;
    let shutdown_action = Arc::new(std::sync::atomic::AtomicU8::new(0));

    // Set up graceful shutdown
    let shutdown_action_for_shutdown = shutdown_action.clone();
    let shutdown = async move {
        tokio::pin!(external_shutdown);
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sigterm =
                signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");
            let mut sigusr1 =
                signal(SignalKind::user_defined1()).expect("Failed to install SIGUSR1 handler");

            drbot_gateway::openclaw_restart::enable_sigusr1_self_restart();

            loop {
                tokio::select! {
                    _ = &mut external_shutdown => {
                        info!("shutdown requested (external)");
                        shutdown_action_for_shutdown.store(ACTION_STOP, std::sync::atomic::Ordering::Relaxed);
                        break;
                    }
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
            tokio::select! {
                _ = &mut external_shutdown => {
                    info!("shutdown requested (external)");
                    shutdown_action_for_shutdown.store(ACTION_STOP, std::sync::atomic::Ordering::Relaxed);
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Received shutdown signal");
                    shutdown_action_for_shutdown.store(ACTION_STOP, std::sync::atomic::Ordering::Relaxed);
                }
            }
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
async fn create_provider(config: &Config, provider_name: &str) -> Result<Arc<dyn Provider>> {
    use drbot_ollama::OllamaProvider;
    use drbot_openai::OpenAIProvider;

    fn env_flag_enabled(name: &str) -> bool {
        let Ok(v) = std::env::var(name) else {
            return false;
        };
        let v = v.trim().to_ascii_lowercase();
        matches!(v.as_str(), "1" | "true" | "yes" | "on")
    }

    fn normalize_http_url(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Ollama commonly uses OLLAMA_HOST like "127.0.0.1:11434".
        if trimmed.contains("://") {
            Some(trimmed.to_string())
        } else {
            Some(format!("http://{}", trimmed))
        }
    }

    fn ollama_base_url_best_effort(config: &Config) -> String {
        if let Some(ollama) = &config.providers.ollama {
            let u = ollama.url.trim();
            if !u.is_empty() {
                return u.to_string();
            }
        }

        if let Ok(v) = std::env::var("DRBOT_OLLAMA_URL") {
            if let Some(u) = normalize_http_url(&v) {
                return u;
            }
        }
        if let Ok(v) = std::env::var("OLLAMA_HOST") {
            if let Some(u) = normalize_http_url(&v) {
                return u;
            }
        }

        drbot_ollama::DEFAULT_BASE_URL.to_string()
    }

    match provider_name {
        "auto" => {
            // Auto-select prefers cost-savers first:
            // - CLI tools (claude-cli / codex-cli) if installed on PATH
            // - Ollama if configured OR running (even without config)
            // - API providers as fallback
            //
            // You can disable CLI auto-detect with: DRBOT_AUTO_DISABLE_CLI_PRESETS=1
            let cli_presets_disabled = env_flag_enabled("DRBOT_AUTO_DISABLE_CLI_PRESETS");

            if !cli_presets_disabled {
                let p = CliProvider::claude_cli();
                if p.check_command_exists().is_ok() {
                    info!("Auto-selected provider: claude-cli");
                    return Ok(Arc::new(p));
                }

                let p = CliProvider::codex_cli();
                if p.check_command_exists().is_ok() {
                    info!("Auto-selected provider: codex-cli");
                    return Ok(Arc::new(p));
                }
            }

            let base_url = ollama_base_url_best_effort(config);
            let mut allow_ollama = config.providers.ollama.is_some();
            if !allow_ollama {
                allow_ollama = check_ollama_health_with_timeout(
                    &base_url,
                    std::time::Duration::from_millis(350),
                )
                .await
                .unwrap_or(false);
            }

            if allow_ollama {
                let mut p = OllamaProvider::new().with_base_url(&base_url);
                let default_model = config
                    .providers
                    .ollama
                    .as_ref()
                    .and_then(|c| c.default_model.clone())
                    .or_else(|| config.providers.default_model.clone());
                if let Some(default_model) = default_model {
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
                let default_model = anthropic_config
                    .default_model
                    .clone()
                    .or_else(|| config.providers.default_model.clone());
                if let Some(default_model) = default_model {
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
                let default_model = openai_config
                    .default_model
                    .clone()
                    .or_else(|| config.providers.default_model.clone());
                if let Some(default_model) = default_model {
                    p = p.with_default_model(default_model);
                }
                info!("Auto-selected provider: openai");
                return Ok(Arc::new(p));
            }

            for cfg in config.providers.openai_compatible.iter() {
                let mut p = OpenAIProvider::new(&cfg.api_key)
                    .with_provider_name(cfg.name.clone())
                    .with_base_url(cfg.base_url.clone())
                    .with_extra_headers(cfg.headers.clone());
                let default_model = cfg
                    .default_model
                    .clone()
                    .or_else(|| config.providers.default_model.clone());
                if let Some(default_model) = default_model {
                    p = p.with_default_model(default_model);
                }
                info!(provider = %cfg.name, "Auto-selected provider: openai-compatible");
                return Ok(Arc::new(p));
            }

            for cfg in config.providers.cli.iter() {
                let p = CliProvider::from_config(cfg);
                if p.check_command_exists().is_ok() {
                    info!(provider = %cfg.name, "Auto-selected provider: cli");
                    return Ok(Arc::new(p));
                }
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
            let default_model = anthropic_config
                .default_model
                .clone()
                .or_else(|| config.providers.default_model.clone());
            if let Some(default_model) = default_model {
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
            let default_model = openai_config
                .default_model
                .clone()
                .or_else(|| config.providers.default_model.clone());
            if let Some(default_model) = default_model {
                p = p.with_default_model(default_model);
            }
            Ok(Arc::new(p))
        }
        "ollama" | "local" => {
            let base_url = ollama_base_url_best_effort(config);
            let ok =
                check_ollama_health_with_timeout(&base_url, std::time::Duration::from_millis(350))
                    .await
                    .unwrap_or(false);
            if !ok {
                return Err(anyhow::anyhow!(
                    "Ollama not reachable at {} (start Ollama or run 'drbot wizard')",
                    base_url.trim()
                ));
            }

            let mut p = OllamaProvider::new().with_base_url(&base_url);
            let default_model = config
                .providers
                .ollama
                .as_ref()
                .and_then(|c| c.default_model.clone())
                .or_else(|| config.providers.default_model.clone());
            if let Some(default_model) = default_model {
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
        "codex-oss" | "codex-local" => {
            let p = CliProvider::codex_oss_ollama();
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

            // Check OpenAI-compatible providers from config (OpenRouter, xAI, etc).
            if let Some(cfg) = config
                .providers
                .openai_compatible
                .iter()
                .find(|c| c.name == other)
            {
                let mut p = OpenAIProvider::new(&cfg.api_key)
                    .with_provider_name(cfg.name.clone())
                    .with_base_url(cfg.base_url.clone())
                    .with_extra_headers(cfg.headers.clone());
                let default_model = cfg
                    .default_model
                    .clone()
                    .or_else(|| config.providers.default_model.clone());
                if let Some(default_model) = default_model {
                    p = p.with_default_model(default_model);
                }
                return Ok(Arc::new(p));
            }
            Err(anyhow::anyhow!(
                "Unknown provider: {}. Supported: auto, anthropic/claude, openai/gpt, ollama/local, claude-cli, codex-cli, codex-oss",
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

fn resolve_default_chat_workspace_dir(config: &Config) -> Option<PathBuf> {
    use drbot_gateway::openclaw_paths;

    let state_dir = openclaw_paths::resolve_openclaw_state_dir(config)?;
    let agent_id = openclaw_paths::DEFAULT_AGENT_ID;

    // Honor `agents.json` workspace overrides (OpenClaw parity).
    let agents_path = state_dir.join("agents.json");
    if let Ok(raw) = std::fs::read_to_string(&agents_path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(agents) = value.get("agents").and_then(|v| v.as_array()) {
                for agent in agents {
                    let raw_id = agent.get("agentId").and_then(|v| v.as_str()).unwrap_or("");
                    if openclaw_paths::normalize_agent_id(raw_id) != agent_id {
                        continue;
                    }
                    let workspace = agent
                        .get("workspace")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim())
                        .unwrap_or("");
                    if workspace.is_empty() {
                        continue;
                    }
                    return Some(openclaw_paths::resolve_user_path(workspace));
                }
            }
        }
    }

    Some(state_dir.join("agents").join(agent_id))
}

fn ensure_chat_workspace_bootstrap(workspace_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(workspace_dir)?;

    let identity_path = workspace_dir.join("IDENTITY.md");
    if !identity_path.exists() {
        let _ = std::fs::write(&identity_path, "# drbot\n");
    }

    let user_path = workspace_dir.join("USER.md");
    if !user_path.exists() {
        let _ = std::fs::write(
            &user_path,
            r#"# User

Stable facts and preferences that should carry across sessions.

- Name:
- Timezone:
- Preferred tone/style:
- Formatting preferences (e.g., bullets, terse/verbose):
- Defaults (units, currency, locale):
- Avoid / don't do:
"#,
        );
    }

    let memory_path = workspace_dir.join("MEMORY.md");
    let memory_alt_path = workspace_dir.join("memory.md");
    if !memory_path.exists() && !memory_alt_path.exists() {
        let _ = std::fs::write(
            &memory_path,
            r#"# Memory

Long-term notes for the assistant.

- Keep *always-relevant* items short and stable.
- Put longer docs/notes in `memory/` as separate Markdown files (easier to search).

## Pinned

## Preferences

## Knowledge base
"#,
        );
    }

    let memory_dir = workspace_dir.join("memory");
    std::fs::create_dir_all(&memory_dir)?;
    let readme = memory_dir.join("README.md");
    if !readme.exists() {
        let _ = std::fs::write(
            &readme,
            r#"# Knowledge Base

Put longer notes/docs here as Markdown files (the assistant can search them when needed).

Suggested files:
- `projects.md` (current work + status)
- `people.md` (names + relationships)
- `procedures.md` (how you like things done)
- `preferences.md` (style, tools, defaults)
"#,
        );
    }

    // Best-effort personalization: auto-fill local timezone if the field is still blank.
    let _ = drbot_gateway::workspace_autosave::ensure_user_timezone_best_effort(workspace_dir);

    Ok(())
}

async fn run_kb(action: KbAction) -> Result<()> {
    match action {
        KbAction::Init { dir } => {
            let start = match dir.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                Some(raw) => PathBuf::from(expand_tilde(raw)),
                None => std::env::current_dir().map_err(|e| anyhow::anyhow!("{}", e))?,
            };
            let start = start.canonicalize().unwrap_or(start);
            let project_root =
                drbot_tool_mode::find_git_root_best_effort(&start).unwrap_or(start);
            let project_drbot_dir = project_root.join(".drbot");

            drbot_tool_mode::ensure_project_kb_bootstrap(&project_drbot_dir).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to initialize {}: {}",
                    project_drbot_dir.display(),
                    e
                )
            })?;

            println!("Initialized project KB: {}", project_drbot_dir.display());
            Ok(())
        }
    }
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

fn prompt_approve(prompt: &str) -> Result<bool> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let ans = input.trim().to_lowercase();
    Ok(ans == "y" || ans == "yes")
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
    let force_direct = match std::env::var("DRBOT_CHAT_DIRECT") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    };

    if !force_direct {
        match run_chat_gateway(
            config,
            provider_name.clone(),
            model.clone(),
            system.clone(),
            skill_url.clone(),
            agent,
            yes,
            bash_auto_approve_prefixes.clone(),
            bash_auto_approve_allowlist.clone(),
            bash_auto_approve_all,
            agent_strict,
            root.clone(),
            max_tool_rounds,
            single_message.clone(),
            message_file.clone(),
            stream,
            session_id.clone(),
            new_session,
            list_sessions,
            title.clone(),
            persona_name.clone(),
            list_personas,
            context_size,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(e) => {
                warn!(
                    error = %e,
                    "Gateway-backed chat failed; falling back to direct provider mode (set DRBOT_CHAT_DIRECT=1 to force direct)"
                );
            }
        }
    }

    run_chat_direct(
        config,
        provider_name,
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
        single_message,
        message_file,
        stream,
        session_id,
        new_session,
        list_sessions,
        title,
        persona_name,
        list_personas,
        context_size,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_chat_gateway(
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
    use crate::gateway_client::GatewayClient;
    use drbot_protocol::{
        event::{chat as chat_events, provider as provider_events},
        event_types, AuthLoginParams, ChatOptions as GatewayChatOptions, ChatSendParams,
        ProviderListParams, ProviderListResult, ProviderModelsParams, ProviderModelsResult,
        ProviderSelectParams, ProviderSelectResult, Request, Response, SessionClearParams,
        SessionCreateParams, SessionCreateResult, SessionGetParams, SessionGetResult,
        SessionListParams, SessionListResult, SessionUpdateParams,
    };
    use std::time::Duration;
    use tokio::sync::oneshot;
    use uuid::Uuid;

    let persona_registry = init_persona_registry();

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

    let single_message = resolve_single_message(single_message, message_file)?;

    // Best-effort: start/attach gateway (like the default `drbot` TUI flow).
    let mut shutdown_tx: Option<oneshot::Sender<()>> = None;
    let mut gateway_task: Option<tokio::task::JoinHandle<Result<()>>> = None;

    if !gateway_is_listening(config).await {
        let (tx, rx) = oneshot::channel::<()>();
        shutdown_tx = Some(tx);
        let gateway_config = config.clone();
        gateway_task = Some(tokio::spawn(async move {
            run_gateway_with_external_shutdown(gateway_config, async move {
                let _ = rx.await;
            })
            .await
        }));

        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            if gateway_is_listening(config).await {
                break;
            }
            if gateway_task.as_ref().is_some_and(|t| t.is_finished()) {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        if !gateway_is_listening(config).await {
            let err = match gateway_task.take() {
                Some(t) => match t.await {
                    Ok(Ok(())) => anyhow::anyhow!("Gateway exited before chat started"),
                    Ok(Err(e)) => e,
                    Err(e) => anyhow::anyhow!("Gateway task failed: {}", e),
                },
                None => anyhow::anyhow!("Gateway did not start listening"),
            };
            return Err(err);
        }
    }

    let result: Result<()> = async {
        let url = gateway_ws_url(config);
        let (client, mut event_rx, mut response_rx) = GatewayClient::connect(&url).await?;

        if let Some(token) = gateway_login_token(config) {
            let resp = client
                .request("auth.login", AuthLoginParams { token })
                .await?;
            if let Some(err) = resp.error {
                return Err(anyhow::anyhow!("Gateway auth failed: {}", err.message));
            }
        }

        async fn expect_ok(resp: Response, method: &str) -> Result<serde_json::Value> {
            if let Some(err) = resp.error {
                return Err(anyhow::anyhow!("{} error: {}", method, err.message));
            }
            resp.result
                .ok_or_else(|| anyhow::anyhow!("{} returned no result", method))
        }

        // Sessions: list + exit (doesn't require a provider).
        if list_sessions {
            let resp = client
                .request(
                    "session.list",
                    SessionListParams {
                        limit: Some(20),
                        offset: None,
                        state: Some("all".to_string()),
                    },
                )
                .await?;
            let result = expect_ok(resp, "session.list").await?;
            let parsed: SessionListResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;

            if parsed.sessions.is_empty() {
                println!("No sessions found.");
                return Ok(());
            }

            println!("Recent Sessions");
            println!("===============");
            println!();
            for s in parsed.sessions.iter().take(20) {
                let title = s.title.as_deref().unwrap_or("Untitled");
                let updated = s.updated_at.format("%Y-%m-%d %H:%M");
                println!(
                    "  {} - {} ({} messages, {})",
                    &s.id.to_string()[..8],
                    title,
                    s.message_count,
                    updated
                );
            }
            println!();
            println!("Resume with: drbot chat --session <id>");
            return Ok(());
        }

        // Tool/agent mode config (runs tools locally while chatting via the gateway).
        let tool_root = if agent {
            let root_path = root
                .as_deref()
                .map(|p| PathBuf::from(expand_tilde(p)))
                .unwrap_or(std::env::current_dir().map_err(|e| anyhow::anyhow!("{}", e))?);
            let (resolved, used_default) =
                resolve_tool_root_with_allowlist(root_path, &config.assistant.workspace_allowlist)?;
            if used_default && (agent || root.is_some()) {
                warn!(
                    root = %resolved.display(),
                    "tool root not in allowlist; using allowed workspace root"
                );
                eprintln!(
                    "Warning: tool root not in allowlist; using {}",
                    resolved.display()
                );
            }
            resolved
        } else {
            let root_path =
                std::env::current_dir().map_err(|e| anyhow::anyhow!("{}", e))?;
            let (resolved, used_default) =
                resolve_tool_root_with_allowlist(root_path, &config.assistant.workspace_allowlist)?;
            if used_default && root.is_some() {
                warn!(
                    root = %resolved.display(),
                    "tool root not in allowlist; using allowed workspace root"
                );
            }
            resolved
        };

        let mut tool_cfg = ToolModeConfig {
            enabled: agent,
            auto_approve: yes,
            root: tool_root,
            max_rounds: max_tool_rounds.max(1),
            autonomy_mode: config.assistant.autonomy_mode,
            tool_allowlist: config.assistant.tool_allowlist.clone(),
            tool_denylist: config.assistant.tool_denylist.clone(),
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

        let skill_pack = if let Some(url) = skill_url
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            Some(fetch_skill_pack_from_url(url).await?)
        } else {
            None
        };

        // Provider: select explicit provider if requested; otherwise ensure something is active.
        let mut active_provider: Option<String>;
        if let Some(requested) = provider_name
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            let resp = client
                .request(
                    "provider.select",
                    ProviderSelectParams {
                        provider: requested.to_string(),
                    },
                )
                .await?;
            let result = expect_ok(resp, "provider.select").await?;
            let parsed: ProviderSelectResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;
            active_provider = Some(parsed.provider.name);
        } else {
            let resp = client
                .request("provider.list", ProviderListParams::default())
                .await?;
            let result = expect_ok(resp, "provider.list").await?;
            let parsed: ProviderListResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;
            active_provider = parsed
                .providers
                .iter()
                .find(|p| p.status.starts_with("active"))
                .map(|p| p.name.clone());

            if active_provider.is_none() {
                let resp = client
                    .request(
                        "provider.select",
                        ProviderSelectParams {
                            provider: "auto".to_string(),
                        },
                    )
                    .await?;
                let result = expect_ok(resp, "provider.select").await?;
                let parsed: ProviderSelectResult =
                    serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;
                active_provider = Some(parsed.provider.name);
            }
        }

        // Helper to resolve session ID by UUID or prefix.
        async fn resolve_session_id_prefix(
            client: &GatewayClient,
            raw: &str,
        ) -> Result<Uuid> {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(anyhow::anyhow!("session id is empty"));
            }
            if let Ok(uuid) = Uuid::parse_str(trimmed) {
                return Ok(uuid);
            }

            let resp = client
                .request(
                    "session.list",
                    SessionListParams {
                        limit: Some(200),
                        offset: None,
                        state: Some("all".to_string()),
                    },
                )
                .await?;
            let result = expect_ok(resp, "session.list").await?;
            let parsed: SessionListResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;

            let mut matches = parsed
                .sessions
                .into_iter()
                .filter(|s| s.id.to_string().starts_with(trimmed))
                .collect::<Vec<_>>();

            if matches.is_empty() {
                return Err(anyhow::anyhow!("Session not found: {}", trimmed));
            }
            if matches.len() > 1 {
                matches.sort_by_key(|s| s.updated_at);
                matches.reverse();
                let suggestions = matches
                    .into_iter()
                    .take(5)
                    .map(|s| s.id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(anyhow::anyhow!(
                    "Session prefix is ambiguous: {} (matches: {})",
                    trimmed,
                    suggestions
                ));
            }
            Ok(matches.remove(0).id)
        }

        let git_root = drbot_tool_mode::find_git_root_best_effort(&tool_cfg.root);
        let in_git_project = git_root.is_some();
        let project_key_base = git_root
            .unwrap_or_else(|| tool_cfg.root.clone())
            .to_string_lossy()
            .to_string();
        let project_drbot_dir = drbot_tool_mode::resolve_project_drbot_dir_best_effort(&tool_cfg.root);
        if in_git_project && drbot_tool_mode::project_kb_auto_init_enabled() {
            drbot_tool_mode::ensure_project_kb_bootstrap_best_effort(&project_drbot_dir);
        }
        let session_map_key = if tool_cfg.enabled {
            format!("{}#agent", project_key_base)
        } else {
            format!("{}#chat", project_key_base)
        };
        let mut session_prefs = GatewayChatPrefs::load_best_effort();
        let mapped_session_uuid = session_prefs
            .last_session_by_project
            .get(&session_map_key)
            .and_then(|s| Uuid::parse_str(s).ok());

        // Avoid polluting a "normal chat" session with tool-mode prompts when using --agent.
        // Users can still resume a specific session via --session.
        let user_requested_new_session = new_session;
        let session_id_requested = session_id
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        let new_session =
            user_requested_new_session || (agent && !session_id_requested && mapped_session_uuid.is_none());
        let default_session_title = if agent { "Gateway Agent" } else { "Gateway Chat" };

        // Establish a session (resume by id, resume most recent, or create).
        let mut session_uuid_maybe_stale = false;
        let mut session_uuid: Uuid = if let Some(raw) = session_id
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            resolve_session_id_prefix(&client, raw).await?
        } else if new_session {
            let resp = client
                .request(
                    "session.create",
                    SessionCreateParams {
                        title: title
                            .clone()
                            .or_else(|| Some(default_session_title.to_string())),
                        provider: active_provider.clone(),
                        model: model.clone(),
                        system_prompt: system.clone(),
                    },
                )
                .await?;
            let result = expect_ok(resp, "session.create").await?;
            let parsed: SessionCreateResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;
            parsed.session_id
        } else if let Some(mapped) = mapped_session_uuid {
            session_uuid_maybe_stale = true;
            mapped
        } else if in_git_project {
            let resp = client
                .request(
                    "session.create",
                    SessionCreateParams {
                        title: title
                            .clone()
                            .or_else(|| Some(default_session_title.to_string())),
                        provider: active_provider.clone(),
                        model: model.clone(),
                        system_prompt: system.clone(),
                    },
                )
                .await?;
            let result = expect_ok(resp, "session.create").await?;
            let parsed: SessionCreateResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;
            parsed.session_id
        } else {
            let resp = client
                .request(
                    "session.list",
                    SessionListParams {
                        limit: Some(1),
                        offset: None,
                        state: Some("active".to_string()),
                    },
                )
                .await?;
            let result = expect_ok(resp, "session.list").await?;
            let parsed: SessionListResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;

            if let Some(first) = parsed.sessions.first() {
                session_uuid_maybe_stale = true;
                first.id
            } else {
                let resp = client
                    .request(
                        "session.create",
                        SessionCreateParams {
                            title: title
                                .clone()
                                .or_else(|| Some(default_session_title.to_string())),
                            provider: active_provider.clone(),
                            model: model.clone(),
                            system_prompt: system.clone(),
                        },
                    )
                    .await?;
                let result = expect_ok(resp, "session.create").await?;
                let parsed: SessionCreateResult =
                    serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;
                parsed.session_id
            }
        };

        session_prefs
            .last_session_by_project
            .insert(session_map_key.clone(), session_uuid.to_string());
        session_prefs.save_best_effort();

        let mut remember_session = |id: Uuid| {
            session_prefs
                .last_session_by_project
                .insert(session_map_key.clone(), id.to_string());
            session_prefs.save_best_effort();
        };

        // Load session details (and align provider/model header state).
        let resp = client
            .request("session.get", SessionGetParams { session_id: session_uuid })
            .await?;
        let result = match expect_ok(resp, "session.get").await {
            Ok(r) => r,
            Err(e) => {
                if session_id_requested || !session_uuid_maybe_stale {
                    return Err(e);
                }

                let resp = client
                    .request(
                        "session.create",
                        SessionCreateParams {
                            title: title
                                .clone()
                                .or_else(|| Some(default_session_title.to_string())),
                            provider: active_provider.clone(),
                            model: model.clone(),
                            system_prompt: system.clone(),
                        },
                    )
                    .await?;
                let result = expect_ok(resp, "session.create").await?;
                let parsed: SessionCreateResult =
                    serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;
                session_uuid = parsed.session_id;
                remember_session(session_uuid);

                let resp = client
                    .request("session.get", SessionGetParams { session_id: session_uuid })
                    .await?;
                expect_ok(resp, "session.get").await?
            }
        };
        let mut parsed: SessionGetResult =
            serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;

        if let Some(p) = parsed
            .session
            .provider
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            // Best-effort: keep active provider aligned with the session.
            let resp = client
                .request(
                    "provider.select",
                    ProviderSelectParams {
                        provider: p.to_string(),
                    },
                )
                .await;
            if let Ok(resp) = resp {
                if resp.error.is_none() {
                    active_provider = Some(p.to_string());
                }
            }
        }

        let mut model_override: Option<String> = parsed.session.model.clone();
        let mut session_system_prompt: Option<String> = parsed.system_prompt.clone();

        // Apply persona/system/model overrides as a session.update before chatting.
        let active_persona = persona_name
            .as_ref()
            .and_then(|name| persona_registry.get(name));
        let system_override = system
            .as_deref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let system_clear_requested = system
            .as_deref()
            .is_some_and(|s| s.trim().is_empty());

        let desired_base_system_prompt: Option<String> = {
            let mut base = if let Some(persona) = active_persona.as_ref() {
                let persona_prompt = persona.build_system_prompt();
                let underlying = if system_clear_requested {
                    system_override.clone()
                } else {
                    system_override.clone().or_else(|| session_system_prompt.clone())
                };

                match underlying {
                    Some(user_system) => {
                        let persona_trim = persona_prompt.trim();
                        let user_trim = user_system.trim();
                        if user_trim.contains(persona_trim) {
                            Some(user_trim.to_string())
                        } else {
                            Some(format!("{}\n\n{}", persona_trim, user_trim))
                        }
                    }
                    None => Some(persona_prompt),
                }
            } else if system_clear_requested {
                system_override.clone()
            } else {
                system_override.clone().or_else(|| session_system_prompt.clone())
            };

            if let Some(pack) = skill_pack.as_ref() {
                let pack_trim = pack.trim();
                if !pack_trim.is_empty() {
                    base = Some(match base {
                        Some(existing) => {
                            let existing_trim = existing.trim();
                            if existing_trim.contains(pack_trim) {
                                existing_trim.to_string()
                            } else {
                                format!("{}\n\n---\n\n{}", existing_trim, pack_trim)
                            }
                        }
                        None => pack_trim.to_string(),
                    });
                }
            }

            base
        };

        let mut update = SessionUpdateParams {
            session_id: session_uuid,
            ..Default::default()
        };
        let mut need_update = false;

        if let Some(mdl) = model
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            update.model = Some(mdl.to_string());
            model_override = Some(mdl.to_string());
            need_update = true;
        }

        let persist_base_prompt = active_persona.is_some()
            || system_override.is_some()
            || system_clear_requested
            || skill_pack.is_some();
        if persist_base_prompt {
            if system_clear_requested && desired_base_system_prompt.is_none() {
                update.clear_system_prompt = true;
                session_system_prompt = None;
                need_update = true;
            } else if let Some(sys) = desired_base_system_prompt
                .as_deref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                update.system_prompt = Some(sys.to_string());
                session_system_prompt = Some(sys.to_string());
                need_update = true;
            }
        }

        // If an explicit provider was requested, persist it to the session as well (and clear the model).
        if let Some(p) = provider_name
            .as_deref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            update.provider = Some(p.to_string());
            update.clear_model = model_override.is_none();
            model_override = if update.clear_model {
                None
            } else {
                model_override
            };
            need_update = true;
        }

        if need_update {
            let resp = client.request("session.update", update).await?;
            // Don't hard fail; chat.send will still work even if persistence fails.
            if let Some(err) = resp.error {
                eprintln!("Warning: session.update failed: {}", err.message);
            } else {
                // Refresh session for accurate header state.
                let resp = client
                    .request(
                        "session.get",
                        SessionGetParams {
                            session_id: session_uuid,
                        },
                    )
                    .await?;
                if let Ok(result) = expect_ok(resp, "session.get").await {
                    if let Ok(new_parsed) = serde_json::from_value::<SessionGetResult>(result) {
                        parsed = new_parsed;
                        model_override = parsed.session.model.clone();
                        session_system_prompt = parsed.system_prompt.clone();
                    }
                }
            }
        }

        // Gateway chat doesn't use the local ContextManager yet; keep flag to avoid unused warnings.
        let _ = context_size;

        let tool_root_for_prompt = tool_cfg.root.clone();
        let build_effective_system_prompt =
            |tool_enabled: bool, base_system: &Option<String>| -> Option<String> {
                if tool_enabled {
                    Some(build_agent_system_prompt_with_policy(
                        base_system.clone(),
                        &tool_root_for_prompt,
                        &tool_cfg.tool_allowlist,
                        &tool_cfg.tool_denylist,
                    ))
                } else {
                    base_system.clone()
                }
            };

        let session_prefix = &session_uuid.to_string()[..8];
        let provider_label = active_provider.as_deref().unwrap_or("auto");
        let persona_label = active_persona
            .as_ref()
            .map(|p| format!(" [Persona: {}]", p.name))
            .unwrap_or_default();

        let has_single_message = single_message
            .as_deref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        if !has_single_message {
            let chat_mode = if tool_cfg.enabled {
                "Gateway Agent"
            } else {
                "Gateway Chat"
            };
            println!(
                "drbot v{} - {} ({}) [Session: {}]{}",
                env!("CARGO_PKG_VERSION"),
                chat_mode,
                provider_label,
                session_prefix,
                persona_label
            );
            println!("Commands: /help, /quit, /provider, /model, /sessions, /session, /new, /clear, /info, /agent, /approve, /tools");
            if tool_cfg.enabled {
                println!(
                    "Tool mode: ON (auto-approve: {})  Autonomy: {:?}  Root: {}  Max rounds: {}",
                    if tool_cfg.auto_approve { "ON" } else { "OFF" },
                    tool_cfg.autonomy_mode,
                    tool_cfg.root.display(),
                    tool_cfg.max_rounds
                );
            }
            if !parsed.messages.is_empty() {
                println!("Resuming session with {} messages.", parsed.messages.len());
            }
            println!();
        }

        async fn print_provider_list(client: &GatewayClient) -> Result<()> {
            let resp = client
                .request("provider.list", ProviderListParams::default())
                .await?;
            let result = expect_ok(resp, "provider.list").await?;
            let parsed: ProviderListResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Providers:");
            for p in parsed.providers {
                println!("  - {} ({})  models: {}", p.name, p.status, p.models.len());
            }
            Ok(())
        }

        async fn print_model_list(client: &GatewayClient, cur: Option<&str>) -> Result<()> {
            let resp = client
                .request("provider.models", ProviderModelsParams { provider: None })
                .await?;
            let result = expect_ok(resp, "provider.models").await?;
            let parsed: ProviderModelsResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;

            println!("Current model: {}", cur.unwrap_or("(default)"));
            println!("Available models:");
            for m in parsed.models {
                println!("  - {}  {}", m.id, m.name);
            }
            Ok(())
        }

        async fn print_sessions(client: &GatewayClient) -> Result<()> {
            let resp = client
                .request(
                    "session.list",
                    SessionListParams {
                        limit: Some(20),
                        offset: None,
                        state: Some("all".to_string()),
                    },
                )
                .await?;
            let result = expect_ok(resp, "session.list").await?;
            let parsed: SessionListResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;

            if parsed.sessions.is_empty() {
                println!("No sessions found.");
                return Ok(());
            }
            println!("Sessions:");
            for s in parsed.sessions.iter().take(20) {
                let provider = s.provider.as_deref().unwrap_or("(default)");
                let model = s.model.as_deref().unwrap_or("(default)");
                let title = s.title.as_deref().unwrap_or("Untitled");
                println!(
                    "  - {}  {}  provider:{}  model:{}  {}",
                    s.id, s.state, provider, model, title
                );
            }
            Ok(())
        }

        async fn print_session_info(client: &GatewayClient, id: Uuid) -> Result<()> {
            let resp = client
                .request("session.get", SessionGetParams { session_id: id })
                .await?;
            let result = expect_ok(resp, "session.get").await?;
            let parsed: SessionGetResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;

            println!("Session: {}", parsed.session.id);
            println!("State: {}", parsed.session.state);
            println!(
                "Title: {}",
                parsed.session.title.as_deref().unwrap_or("(untitled)")
            );
            println!(
                "Provider: {}",
                parsed.session.provider.as_deref().unwrap_or("(default)")
            );
            println!(
                "Model: {}",
                parsed.session.model.as_deref().unwrap_or("(default)")
            );
            println!(
                "System prompt: {}",
                parsed
                    .system_prompt
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("(none)")
            );
            println!("Messages: {}", parsed.messages.len());
            Ok(())
        }

        async fn send_chat_streaming(
            client: &GatewayClient,
            event_rx: &mut tokio::sync::mpsc::Receiver<drbot_protocol::Event>,
            response_rx: &mut tokio::sync::mpsc::Receiver<Response>,
            session_id: Uuid,
            message: &str,
            model_override: &mut Option<String>,
            opts: GatewayChatOptions,
        ) -> Result<String> {
            let params = ChatSendParams {
                session_id: Some(session_id),
                message: message.to_string(),
                model: None,
                stream: true,
                options: Some(opts),
            };
            let req = Request::create("chat.send", params);
            let request_id = req.id;

            client.send_request(req).await?;

            // Print streaming deltas until complete, while also watching for request-level errors.
            let mut complete = false;
            let mut response_ok = false;
            let mut last_provider_changed: Option<String> = None;
            let mut out = String::new();

            loop {
                tokio::select! {
                    maybe_event = event_rx.recv() => {
                        let Some(event) = maybe_event else {
                            return Err(anyhow::anyhow!("Gateway event channel closed"));
                        };

                        match event.event_type.as_str() {
                            event_types::CHAT_STREAM_START => {
                                if let Ok(ev) = serde_json::from_value::<chat_events::StreamStartEvent>(event.data) {
                                    if ev.request_id == request_id {
                                        // Session ids should match, but keep it robust.
                                        if ev.provider.is_some() {
                                            // no-op (header is printed outside)
                                        }
                                    }
                                }
                            }
                            event_types::CHAT_STREAM_DELTA => {
                                if let Ok(ev) = serde_json::from_value::<chat_events::StreamDeltaEvent>(event.data) {
                                    if ev.request_id == request_id {
                                        out.push_str(&ev.delta);
                                        print!("{}", ev.delta);
                                        let _ = io::stdout().flush();
                                    }
                                }
                            }
                            event_types::CHAT_STREAM_COMPLETE => {
                                if let Ok(ev) = serde_json::from_value::<chat_events::StreamCompleteEvent>(event.data) {
                                    if ev.request_id == request_id {
                                        complete = true;
                                        if let Some(_usage) = ev.usage {
                                            // Keep output simple; could be added to /info later.
                                        }
                                        println!();
                                        if response_ok {
                                            break;
                                        }
                                    }
                                }
                            }
                            event_types::CHAT_STREAM_ERROR => {
                                if let Ok(ev) = serde_json::from_value::<chat_events::StreamErrorEvent>(event.data) {
                                    if ev.request_id == request_id {
                                        println!();
                                        return Err(anyhow::anyhow!("{}", ev.error));
                                    }
                                }
                            }
                            event_types::PROVIDER_CHANGED => {
                                if let Ok(ev) = serde_json::from_value::<provider_events::ChangedEvent>(event.data) {
                                    last_provider_changed = Some(ev.provider.clone());
                                }
                            }
                            "system.disconnected" => {
                                return Err(anyhow::anyhow!("Gateway disconnected"));
                            }
                            _ => {}
                        }
                    }
                    maybe_resp = response_rx.recv() => {
                        let Some(resp) = maybe_resp else {
                            return Err(anyhow::anyhow!("Gateway response channel closed"));
                        };
                        if resp.id != request_id {
                            continue;
                        }
                        if let Some(err) = resp.error {
                            println!();
                            return Err(anyhow::anyhow!("{}", err.message));
                        }
                        response_ok = true;
                        // If the provider changed during this request (fallback), the gateway clears
                        // any persisted model override to avoid cross-provider mismatch.
                        if last_provider_changed.is_some() {
                            *model_override = None;
                        }
                        if complete {
                            break;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(120)) => {
                        return Err(anyhow::anyhow!("chat.send timed out"));
                    }
                }

                // Keep looping until we've observed both:
                // - the request-level response (errors are delivered here)
                // - the stream completion event (signals end-of-output)
            }

            Ok(out)
        }

        async fn send_chat_non_streaming(
            client: &GatewayClient,
            session_id: Uuid,
            message: &str,
            opts: GatewayChatOptions,
        ) -> Result<String> {
            let resp = client
                .request(
                    "chat.send",
                    ChatSendParams {
                        session_id: Some(session_id),
                        message: message.to_string(),
                        model: None,
                        stream: false,
                        options: Some(opts),
                    },
                )
                .await?;
            let result = expect_ok(resp, "chat.send").await?;
            let parsed: drbot_protocol::ChatSendResult =
                serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;
            Ok(parsed.content.unwrap_or_default())
        }

        fn parse_kb_query_best_effort(message: &str) -> Option<String> {
            let msg_trimmed = message.trim_start();
            let msg_lower = msg_trimmed.to_ascii_lowercase();

            let is_kb_cmd = msg_lower == "/kb"
                || msg_lower.starts_with("/kb ")
                || msg_lower.starts_with("/kb:")
                || msg_lower.starts_with("kb:")
                || msg_lower == "/notes"
                || msg_lower.starts_with("/notes ")
                || msg_lower.starts_with("/notes:")
                || msg_lower.starts_with("notes:");

            if !is_kb_cmd {
                return None;
            }

            let query = if msg_lower == "/kb"
                || msg_lower.starts_with("/kb ")
                || msg_lower.starts_with("/kb:")
            {
                msg_trimmed[3..]
                    .trim_start_matches(&[' ', ':'][..])
                    .trim()
                    .to_string()
            } else if msg_lower.starts_with("kb:") {
                msg_trimmed[3..].trim().to_string()
            } else if msg_lower == "/notes"
                || msg_lower.starts_with("/notes ")
                || msg_lower.starts_with("/notes:")
            {
                msg_trimmed[6..]
                    .trim_start_matches(&[' ', ':'][..])
                    .trim()
                    .to_string()
            } else if msg_lower.starts_with("notes:") {
                msg_trimmed[6..].trim().to_string()
            } else {
                String::new()
            };

            if query.trim().is_empty() {
                None
            } else {
                Some(query)
            }
        }

        // Single message mode (non-interactive).
        if let Some(msg) = single_message.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if drbot_tool_mode::is_project_remember_command(msg)
                || drbot_tool_mode::is_project_forget_command(msg)
            {
                let reply = if drbot_tool_mode::is_project_remember_command(msg) {
                    match drbot_tool_mode::parse_project_remember_note(msg) {
                        Some(note) => match drbot_tool_mode::remember_project_kb(&project_drbot_dir, &note) {
                            Ok(updates) => {
                                if updates.rejected {
                                    "Nothing saved (refused to store sensitive/invalid content)."
                                        .to_string()
                                } else if updates.applied {
                                    if updates.updates.is_empty() {
                                        "Saved to project memory.".to_string()
                                    } else {
                                        let mut out = String::new();
                                        out.push_str("Saved to project memory:\n");
                                        for u in updates.updates.iter().take(12) {
                                            out.push_str("- ");
                                            out.push_str(u);
                                            out.push('\n');
                                        }
                                        out.trim_end().to_string()
                                    }
                                } else {
                                    "Nothing saved.".to_string()
                                }
                            }
                            Err(e) => format!("Project remember error: {}", e),
                        },
                        None => "Usage: /remember project <note>".to_string(),
                    }
                } else {
                    match drbot_tool_mode::parse_project_forget_arg(msg) {
                        Some(arg) => match drbot_tool_mode::forget_project_kb(&project_drbot_dir, &arg) {
                            Ok(updates) => {
                                if updates.applied {
                                    if updates.updates.is_empty() {
                                        "Forgot from project memory.".to_string()
                                    } else {
                                        let mut out = String::new();
                                        out.push_str("Forgot from project memory:\n");
                                        for u in updates.updates.iter().take(12) {
                                            out.push_str("- ");
                                            out.push_str(u);
                                            out.push('\n');
                                        }
                                        out.trim_end().to_string()
                                    }
                                } else {
                                    "Nothing forgotten (no matching items).".to_string()
                                }
                            }
                            Err(e) => format!("Project forget error: {}", e),
                        },
                        None => {
                            "Usage: /forget project <all|pinned|conventions|runbooks|kb|text>"
                                .to_string()
                        }
                    }
                };
                println!("{}\n", reply);
                return Ok(());
            }

            let mut mem_parts = msg.split_whitespace();
            let mem_cmd = mem_parts.next().unwrap_or("");
            let mem_arg = mem_parts.next().unwrap_or("");
            if (mem_cmd == "/memory" || mem_cmd == "/mem") && mem_arg.eq_ignore_ascii_case("project")
            {
                let reply = drbot_tool_mode::build_project_memory_overview(&project_drbot_dir);
                println!("{}\n", reply);
                return Ok(());
            }

            let effective_system_prompt =
                build_effective_system_prompt(tool_cfg.enabled, &session_system_prompt);

            if in_git_project {
                let msg_trimmed = msg.trim_start();
                let msg_lower = msg_trimmed.to_ascii_lowercase();
                let is_internal_tool_message = msg_trimmed.starts_with("[Tool Result]")
                    || msg_trimmed.starts_with("[Tool Denied]")
                    || msg_trimmed.starts_with("[Tool Mode Strict]");
                let is_local_cmd = msg_lower.starts_with("/remember")
                    || msg_lower.starts_with("remember:")
                    || msg_lower.starts_with("/forget")
                    || msg_lower.starts_with("forget:")
                    || msg_lower == "/memory"
                    || msg_lower == "/mem"
                    || msg_lower == "/profile"
                    || msg_lower == "/kb"
                    || msg_lower.starts_with("/kb ")
                    || msg_lower.starts_with("/kb:")
                    || msg_lower.starts_with("kb:")
                    || msg_lower == "/notes"
                    || msg_lower.starts_with("/notes ")
                    || msg_lower.starts_with("/notes:")
                    || msg_lower.starts_with("notes:");
                if !is_internal_tool_message && !is_local_cmd {
                    drbot_tool_mode::autosave_project_kb_best_effort(&project_drbot_dir, msg);
                }
            }

            let mut strict_remaining = if agent_strict { 2usize } else { 0usize };
            let mut rounds = 0usize;
            let mut next_message = msg.to_string();
            loop {
                rounds += 1;
                if tool_cfg.enabled && rounds > tool_cfg.max_rounds {
                    return Err(anyhow::anyhow!(
                        "Max tool rounds exceeded ({}).",
                        tool_cfg.max_rounds
                    ));
                }

                if stream {
                    print!("Assistant: ");
                    io::stdout().flush()?;
                }

                let opts = GatewayChatOptions {
                    max_tokens: Some(4096),
                    temperature: if tool_cfg.enabled { Some(0.2) } else { None },
                    system_prompt: {
                        let msg_trimmed = next_message.trim_start();
                        let msg_lower = msg_trimmed.to_ascii_lowercase();
                        let is_internal_tool_message = msg_trimmed.starts_with("[Tool Result]")
                            || msg_trimmed.starts_with("[Tool Denied]")
                            || msg_trimmed.starts_with("[Tool Mode Strict]");
                        let is_local_cmd = msg_lower.starts_with("/remember")
                            || msg_lower.starts_with("remember:")
                            || msg_lower.starts_with("/forget")
                            || msg_lower.starts_with("forget:")
                            || msg_lower == "/memory"
                            || msg_lower == "/mem"
                            || msg_lower == "/profile"
                            || msg_lower == "/kb"
                            || msg_lower.starts_with("/kb ")
                            || msg_lower.starts_with("/kb:")
                            || msg_lower.starts_with("kb:")
                            || msg_lower == "/notes"
                            || msg_lower.starts_with("/notes ")
                            || msg_lower.starts_with("/notes:")
                            || msg_lower.starts_with("notes:");

                        let project_notes = if is_internal_tool_message || is_local_cmd {
                            None
                        } else {
                            drbot_gateway::workspace_notes_recall::recall_project_notes_prompt(
                                &project_drbot_dir,
                                &next_message,
                            )
                            .await
                        };

                        match (effective_system_prompt.as_deref(), project_notes.as_deref()) {
                            (Some(core), Some(notes)) => {
                                Some(format!("{}\n\n---\n\n{}", core.trim(), notes.trim()))
                            }
                            (None, Some(notes)) => Some(notes.to_string()),
                            (core, None) => core.map(|s| s.to_string()),
                        }
                    },
                    ..Default::default()
                };
                let response = if stream {
                    send_chat_streaming(
                        &client,
                        &mut event_rx,
                        &mut response_rx,
                        session_uuid,
                        &next_message,
                        &mut model_override,
                        opts,
                    )
                    .await?
                } else {
                    send_chat_non_streaming(&client, session_uuid, &next_message, opts).await?
                };

                if !stream {
                    println!("{}", response);
                }

                if rounds == 1 {
                    if in_git_project {
                        if let Some(query) = parse_kb_query_best_effort(msg) {
                            let project_reply =
                                drbot_gateway::workspace_notes_recall::recall_project_notes_prompt_explicit(
                                    &project_drbot_dir,
                                    &query,
                                )
                                .await;

                            println!();
                            println!("Project KB (.drbot):");
                            if let Some(reply) = project_reply
                                .as_deref()
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                            {
                                println!("{}", reply);
                            } else {
                                println!("No relevant notes found.");
                            }
                            println!();
                        }
                    }

                    let msg_lower = msg.trim_start().to_ascii_lowercase();
                    let is_memory_cmd = msg_lower == "/memory" || msg_lower == "/mem";
                    if is_memory_cmd && (in_git_project || project_drbot_dir.is_dir()) {
                        println!();
                        println!(
                            "{}",
                            drbot_tool_mode::build_project_memory_overview(&project_drbot_dir)
                        );
                        println!();
                    }
                }

                if !tool_cfg.enabled {
                    break;
                }

                let calls = extract_tool_calls(&response);
                if calls.is_empty() {
                    if agent_strict
                        && strict_remaining > 0
                        && should_reprompt_for_tool_calls(msg, &response)
                    {
                        strict_remaining -= 1;
                        next_message = "[Tool Mode Strict] Convert the previous response into tool calls. Reply ONLY with a `drbot_tool` code block containing JSON tool calls (object or array). No prose.".to_string();
                        continue;
                    }
                    break;
                }

                let mut tool_updates: Vec<String> = Vec::new();
                for call in calls {
                    let approved = match call.tool.as_str() {
                        "read_file" | "list_dir" | "list_directory" | "search" => true,
                        "bash" => {
                            let command = call
                                .args
                                .get("command")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            bash_command_is_safe_for_auto_approve(command, &bash_policy)
                        }
                        _ => tool_cfg.auto_approve,
                    };

                    if !approved {
                        match call.tool.as_str() {
                            "bash" => {
                                let command = call
                                    .args
                                    .get("command")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                println!("\n[Tool Denied] bash (unsafe_for_auto_approve)");
                                println!("command: {}", command);
                                println!();
                                tool_updates.push(format!(
                                    "[Tool Denied] tool=bash reason=unsafe_for_auto_approve\ncommand: {}",
                                    command
                                ));
                            }
                            "write_file" => {
                                let path = call
                                    .args
                                    .get("path")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");
                                let bytes = call
                                    .args
                                    .get("content")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.len())
                                    .unwrap_or(0);
                                println!("\n[Tool Denied] write_file (approval_required)");
                                println!("path: {}  bytes: {}", path, bytes);
                                println!("hint: re-run with -y/--yes (or enable /approve in interactive mode)");
                                println!();
                                tool_updates.push(format!(
                                    "[Tool Denied] tool=write_file reason=approval_required\npath: {}\nbytes: {}",
                                    path, bytes
                                ));
                            }
                            "apply_patch" => {
                                let bytes = call
                                    .args
                                    .get("patch")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.len())
                                    .unwrap_or(0);
                                println!("\n[Tool Denied] apply_patch (approval_required)");
                                println!("bytes: {}", bytes);
                                println!("hint: re-run with -y/--yes (or enable /approve in interactive mode)");
                                println!();
                                tool_updates.push(format!(
                                    "[Tool Denied] tool=apply_patch reason=approval_required\nbytes: {}",
                                    bytes
                                ));
                            }
                            other => {
                                println!("\n[Tool Denied] {} (approval_required)", other);
                                println!("hint: re-run with -y/--yes (or enable /approve in interactive mode)");
                                println!();
                                tool_updates.push(format!(
                                    "[Tool Denied] tool={} reason=approval_required",
                                    other
                                ));
                            }
                        }
                        continue;
                    }

                    let (output, is_error) = match execute_tool_call(&tool_cfg, &call).await {
                        Ok((out, err)) => (out, err),
                        Err(e) => (format!("Error: {}", e), true),
                    };

                    println!(
                        "\n[Tool Result] {}{}\n",
                        call.tool,
                        if is_error { " (error)" } else { "" }
                    );
                    println!("{}", output);
                    println!();

                    tool_updates.push(format!(
                        "[Tool Result] tool={}{}\n{}",
                        call.tool,
                        if is_error { " (error)" } else { "" },
                        output
                    ));
                }

                next_message = tool_updates.join("\n\n");
            }

            return Ok(());
        }

        // Interactive loop.
        loop {
            print!("You: ");
            io::stdout().flush()?;

            let mut input = String::new();
            if io::stdin().read_line(&mut input)? == 0 {
                println!("\nGoodbye!");
                break;
            }
            let input = input.trim();
            if input.is_empty() {
                continue;
            }

            if input == "/quit" || input == "quit" || input == "exit" {
                println!("Goodbye!");
                break;
            }

            let mut mem_parts = input.split_whitespace();
            let mem_cmd = mem_parts.next().unwrap_or("");
            let mem_arg = mem_parts.next().unwrap_or("");
            if (mem_cmd == "/memory" || mem_cmd == "/mem") && mem_arg.eq_ignore_ascii_case("project")
            {
                let reply = drbot_tool_mode::build_project_memory_overview(&project_drbot_dir);
                println!("\n{}\n", reply);
                continue;
            }

            if drbot_tool_mode::is_project_remember_command(input)
                || drbot_tool_mode::is_project_forget_command(input)
            {
                let reply = if drbot_tool_mode::is_project_remember_command(input) {
                    match drbot_tool_mode::parse_project_remember_note(input) {
                        Some(note) => match drbot_tool_mode::remember_project_kb(&project_drbot_dir, &note) {
                            Ok(updates) => {
                                if updates.rejected {
                                    "Nothing saved (refused to store sensitive/invalid content)."
                                        .to_string()
                                } else if updates.applied {
                                    if updates.updates.is_empty() {
                                        "Saved to project memory.".to_string()
                                    } else {
                                        let mut out = String::new();
                                        out.push_str("Saved to project memory:\n");
                                        for u in updates.updates.iter().take(12) {
                                            out.push_str("- ");
                                            out.push_str(u);
                                            out.push('\n');
                                        }
                                        out.trim_end().to_string()
                                    }
                                } else {
                                    "Nothing saved.".to_string()
                                }
                            }
                            Err(e) => format!("Project remember error: {}", e),
                        },
                        None => "Usage: /remember project <note>".to_string(),
                    }
                } else {
                    match drbot_tool_mode::parse_project_forget_arg(input) {
                        Some(arg) => match drbot_tool_mode::forget_project_kb(&project_drbot_dir, &arg) {
                            Ok(updates) => {
                                if updates.applied {
                                    if updates.updates.is_empty() {
                                        "Forgot from project memory.".to_string()
                                    } else {
                                        let mut out = String::new();
                                        out.push_str("Forgot from project memory:\n");
                                        for u in updates.updates.iter().take(12) {
                                            out.push_str("- ");
                                            out.push_str(u);
                                            out.push('\n');
                                        }
                                        out.trim_end().to_string()
                                    }
                                } else {
                                    "Nothing forgotten (no matching items).".to_string()
                                }
                            }
                            Err(e) => format!("Project forget error: {}", e),
                        },
                        None => {
                            "Usage: /forget project <all|pinned|conventions|runbooks|kb|text>"
                                .to_string()
                        }
                    }
                };
                println!("\n{}\n", reply);
                continue;
            }

            if input == "/help" {
                println!(
                    "Commands:\n  /help - Show this help\n  /quit - Exit\n\n  /remember <note> - Save to memory (workspace)\n  /forget <name|timezone|style|all|text> - Forget from memory (workspace)\n  /remember project <note> - Save to project memory (.drbot)\n  /forget project <all|pinned|conventions|runbooks|kb|text> - Forget from project memory (.drbot)\n  /profile - Show USER.md profile (workspace)\n  /memory, /mem - Show memory overview (workspace + project)\n  /memory project - Show project memory only (.drbot)\n  /kb <query>, /notes <query> - Search notes (workspace + .drbot)\n\n  /provider list - List providers\n  /provider <name> - Select provider\n  /model list - List models\n  /model <id> - Set model override\n  /model clear - Use provider default\n\n  /sessions - List sessions\n  /session <uuid|prefix> - Open a session\n  /new - Start a new session\n  /clear - Clear current session history\n  /info - Show current session info\n\n  /agent on|off - Toggle tool mode\n  /approve on|off - Toggle auto-approve\n  /tools - Show tool status"
                );
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
                println!(
                    "Tool mode: {} (autonomy: {:?})",
                    if tool_cfg.enabled { "ON" } else { "OFF" },
                    tool_cfg.autonomy_mode
                );
                println!(
                    "Auto-approve: {}",
                    if tool_cfg.auto_approve { "ON" } else { "OFF" }
                );
                println!("Strict agent: {}", if agent_strict { "ON" } else { "OFF" });
                println!("Root: {}", tool_cfg.root.display());
                println!("Max tool rounds: {}", tool_cfg.max_rounds);
                let allowlist = if tool_cfg.tool_allowlist.is_empty() {
                    "all".to_string()
                } else {
                    tool_cfg.tool_allowlist.join(", ")
                };
                let denylist = if tool_cfg.tool_denylist.is_empty() {
                    "(none)".to_string()
                } else {
                    tool_cfg.tool_denylist.join(", ")
                };
                println!("Tool allowlist: {}", allowlist);
                println!("Tool denylist: {}", denylist);
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
                        println!("\nTool mode: ON (autonomy: {:?})\n", tool_cfg.autonomy_mode);
                    }
                    "off" | "no" | "false" => {
                        tool_cfg.enabled = false;
                        println!("\nTool mode: OFF (autonomy: {:?})\n", tool_cfg.autonomy_mode);
                    }
                    _ => {
                        println!(
                            "\nTool mode: {} (autonomy: {:?}) (use: /agent on|off)\n",
                            if tool_cfg.enabled { "ON" } else { "OFF" },
                            tool_cfg.autonomy_mode
                        );
                    }
                }
                continue;
            }

            if input == "/provider" || input == "/provider list" {
                print_provider_list(&client).await?;
                println!();
                continue;
            }
            if let Some(rest) = input.strip_prefix("/provider ") {
                let name = rest.trim();
                if name.is_empty() {
                    continue;
                }
                let resp = client
                    .request(
                        "provider.select",
                        ProviderSelectParams {
                            provider: name.to_string(),
                        },
                    )
                    .await?;
                if let Some(err) = resp.error {
                    eprintln!("provider.select error: {}", err.message);
                    continue;
                }
                let name = resp
                    .result
                    .and_then(|v| serde_json::from_value::<ProviderSelectResult>(v).ok())
                    .map(|r| r.provider.name)
                    .unwrap_or_else(|| name.to_string());
                active_provider = Some(name.clone());
                model_override = None;

                // Persist provider to the session (and clear model to avoid mismatch).
                let resp = client
                    .request(
                        "session.update",
                        SessionUpdateParams {
                            session_id: session_uuid,
                            provider: Some(name.clone()),
                            clear_model: true,
                            ..Default::default()
                        },
                    )
                    .await?;
                if let Some(err) = resp.error {
                    eprintln!("Warning: session.update failed: {}", err.message);
                }

                println!("Provider set to {}. Model reset to provider default.\n", name);
                continue;
            }

            if input == "/model" || input == "/model list" {
                // provider.models requires an active provider; if missing, try auto-select.
                if active_provider.is_none() {
                    let _ = client
                        .request(
                            "provider.select",
                            ProviderSelectParams {
                                provider: "auto".to_string(),
                            },
                        )
                        .await;
                }
                print_model_list(&client, model_override.as_deref()).await?;
                println!();
                continue;
            }
            if let Some(rest) = input.strip_prefix("/model ") {
                let arg = rest.trim();
                if arg.is_empty() {
                    continue;
                }
                if matches!(arg, "clear" | "default") {
                    model_override = None;
                    let resp = client
                        .request(
                            "session.update",
                            SessionUpdateParams {
                                session_id: session_uuid,
                                clear_model: true,
                                ..Default::default()
                            },
                        )
                        .await?;
                    if let Some(err) = resp.error {
                        eprintln!("Warning: session.update failed: {}", err.message);
                    }
                    println!("Model: (default)\n");
                    continue;
                }

                model_override = Some(arg.to_string());
                let resp = client
                    .request(
                        "session.update",
                        SessionUpdateParams {
                            session_id: session_uuid,
                            model: Some(arg.to_string()),
                            ..Default::default()
                        },
                    )
                    .await?;
                if let Some(err) = resp.error {
                    eprintln!("Warning: session.update failed: {}", err.message);
                }
                println!("Model set: {}\n", arg);
                continue;
            }

            if input == "/sessions" || input == "/sessions list" {
                print_sessions(&client).await?;
                println!();
                continue;
            }

            if input == "/info" || input == "/session" || input == "/session show" {
                print_session_info(&client, session_uuid).await?;
                println!();
                continue;
            }

            if let Some(rest) = input.strip_prefix("/session ") {
                let raw = rest.trim();
                if raw.is_empty() {
                    continue;
                }
                let resolved = resolve_session_id_prefix(&client, raw).await?;
                session_uuid = resolved;
                remember_session(session_uuid);

                let resp = client
                    .request("session.get", SessionGetParams { session_id: session_uuid })
                    .await?;
                let result = expect_ok(resp, "session.get").await?;
                let parsed: SessionGetResult =
                    serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;

                // Align provider/model local state with the session.
                model_override = parsed.session.model.clone();
                session_system_prompt = parsed.system_prompt.clone();

                if let Some(p) = parsed
                    .session
                    .provider
                    .as_deref()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                {
                    let _ = client
                        .request(
                            "provider.select",
                            ProviderSelectParams {
                                provider: p.to_string(),
                            },
                        )
                        .await;
                    active_provider = Some(p.to_string());
                }

                println!(
                    "Opened session: {} (messages: {})\n",
                    &session_uuid.to_string()[..8],
                    parsed.messages.len()
                );
                continue;
            }

            if input == "/new" {
                let resp = client
                    .request(
                        "session.create",
                        SessionCreateParams {
                            title: Some(
                                if tool_cfg.enabled {
                                    "Gateway Agent"
                                } else {
                                    "Gateway Chat"
                                }
                                .to_string(),
                            ),
                            provider: active_provider.clone(),
                            model: model_override.clone(),
                            system_prompt: session_system_prompt.clone(),
                        },
                    )
                    .await?;
                let result = expect_ok(resp, "session.create").await?;
                let parsed: SessionCreateResult =
                    serde_json::from_value(result).map_err(|e| anyhow::anyhow!("{}", e))?;
                session_uuid = parsed.session_id;
                remember_session(session_uuid);
                println!("New session started: {}\n", &session_uuid.to_string()[..8]);
                continue;
            }

            if input == "/clear" {
                let resp = client
                    .request(
                        "session.clear",
                        SessionClearParams {
                            session_id: session_uuid,
                        },
                    )
                    .await?;
                if let Some(err) = resp.error {
                    eprintln!("session.clear error: {}", err.message);
                } else {
                    println!("Session cleared.\n");
                }
                continue;
            }

            // Normal chat message.
            let effective_system_prompt =
                build_effective_system_prompt(tool_cfg.enabled, &session_system_prompt);

            let user_text = input.to_string();
            if in_git_project {
                let msg_trimmed = user_text.trim_start();
                let msg_lower = msg_trimmed.to_ascii_lowercase();
                let is_internal_tool_message = msg_trimmed.starts_with("[Tool Result]")
                    || msg_trimmed.starts_with("[Tool Denied]")
                    || msg_trimmed.starts_with("[Tool Mode Strict]");
                let is_local_cmd = msg_lower.starts_with("/remember")
                    || msg_lower.starts_with("remember:")
                    || msg_lower.starts_with("/forget")
                    || msg_lower.starts_with("forget:")
                    || msg_lower == "/memory"
                    || msg_lower == "/mem"
                    || msg_lower == "/profile"
                    || msg_lower == "/kb"
                    || msg_lower.starts_with("/kb ")
                    || msg_lower.starts_with("/kb:")
                    || msg_lower.starts_with("kb:")
                    || msg_lower == "/notes"
                    || msg_lower.starts_with("/notes ")
                    || msg_lower.starts_with("/notes:")
                    || msg_lower.starts_with("notes:");
                if !is_internal_tool_message && !is_local_cmd {
                    drbot_tool_mode::autosave_project_kb_best_effort(&project_drbot_dir, &user_text);
                }
            }
            let mut strict_remaining = if agent_strict { 2usize } else { 0usize };
            let mut rounds = 0usize;
            let mut next_message = user_text.clone();
            loop {
                rounds += 1;
                if tool_cfg.enabled && rounds > tool_cfg.max_rounds {
                    eprintln!("\nError: Max tool rounds exceeded ({}).", tool_cfg.max_rounds);
                    break;
                }

                print!("\nAssistant: ");
                io::stdout().flush()?;
                let opts = GatewayChatOptions {
                    max_tokens: Some(4096),
                    temperature: if tool_cfg.enabled { Some(0.2) } else { None },
                    system_prompt: {
                        let msg_trimmed = next_message.trim_start();
                        let msg_lower = msg_trimmed.to_ascii_lowercase();
                        let is_internal_tool_message = msg_trimmed.starts_with("[Tool Result]")
                            || msg_trimmed.starts_with("[Tool Denied]")
                            || msg_trimmed.starts_with("[Tool Mode Strict]");
                        let is_local_cmd = msg_lower.starts_with("/remember")
                            || msg_lower.starts_with("remember:")
                            || msg_lower.starts_with("/forget")
                            || msg_lower.starts_with("forget:")
                            || msg_lower == "/memory"
                            || msg_lower == "/mem"
                            || msg_lower == "/profile"
                            || msg_lower == "/kb"
                            || msg_lower.starts_with("/kb ")
                            || msg_lower.starts_with("/kb:")
                            || msg_lower.starts_with("kb:")
                            || msg_lower == "/notes"
                            || msg_lower.starts_with("/notes ")
                            || msg_lower.starts_with("/notes:")
                            || msg_lower.starts_with("notes:");

                        let project_notes = if is_internal_tool_message || is_local_cmd {
                            None
                        } else {
                            drbot_gateway::workspace_notes_recall::recall_project_notes_prompt(
                                &project_drbot_dir,
                                &next_message,
                            )
                            .await
                        };

                        match (effective_system_prompt.as_deref(), project_notes.as_deref()) {
                            (Some(core), Some(notes)) => {
                                Some(format!("{}\n\n---\n\n{}", core.trim(), notes.trim()))
                            }
                            (None, Some(notes)) => Some(notes.to_string()),
                            (core, None) => core.map(|s| s.to_string()),
                        }
                    },
                    ..Default::default()
                };

                let response = if stream {
                    match send_chat_streaming(
                        &client,
                        &mut event_rx,
                        &mut response_rx,
                        session_uuid,
                        &next_message,
                        &mut model_override,
                        opts,
                    )
                    .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("\nError: {}", e);
                            break;
                        }
                    }
                } else {
                    match send_chat_non_streaming(&client, session_uuid, &next_message, opts).await {
                        Ok(out) => {
                            println!("{}", out);
                            out
                        }
                        Err(e) => {
                            eprintln!("\nError: {}", e);
                            break;
                        }
                    }
                };
                println!();

                if rounds == 1 {
                    if in_git_project {
                        if let Some(query) = parse_kb_query_best_effort(&user_text) {
                            let project_reply =
                                drbot_gateway::workspace_notes_recall::recall_project_notes_prompt_explicit(
                                    &project_drbot_dir,
                                    &query,
                                )
                                .await;

                            println!("Project KB (.drbot):");
                            if let Some(reply) = project_reply
                                .as_deref()
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                            {
                                println!("{}", reply);
                            } else {
                                println!("No relevant notes found.");
                            }
                            println!();
                        }
                    }

                    let msg_lower = user_text.trim_start().to_ascii_lowercase();
                    let is_memory_cmd = msg_lower == "/memory" || msg_lower == "/mem";
                    if is_memory_cmd && (in_git_project || project_drbot_dir.is_dir()) {
                        println!(
                            "{}",
                            drbot_tool_mode::build_project_memory_overview(&project_drbot_dir)
                        );
                        println!();
                    }
                }

                if !tool_cfg.enabled {
                    break;
                }

                let calls = extract_tool_calls(&response);
                if calls.is_empty() {
                    if agent_strict
                        && strict_remaining > 0
                        && should_reprompt_for_tool_calls(&user_text, &response)
                    {
                        strict_remaining -= 1;
                        next_message = "[Tool Mode Strict] Convert the previous response into tool calls. Reply ONLY with a `drbot_tool` code block containing JSON tool calls (object or array). No prose.".to_string();
                        continue;
                    }
                    break;
                }

                let mut tool_updates: Vec<String> = Vec::new();
                for call in calls {
                    // Default to "automated in place":
                    // - Read-only tools run without prompting.
                    // - Safe bash commands run without prompting.
                    // - Writes require explicit approval unless auto-approve is enabled.
                    let mut approved = match call.tool.as_str() {
                        "read_file" | "list_dir" | "list_directory" | "search" => true,
                        "bash" => {
                            let command = call
                                .args
                                .get("command")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            bash_command_is_safe_for_auto_approve(command, &bash_policy)
                        }
                        _ => tool_cfg.auto_approve,
                    };

                    // Print tool summary (even when auto-approved) for transparency.
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
                            let path = call.args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                            println!("[Tool] list_dir\n  path: {}", path);
                        }
                        "search" => {
                            let pattern =
                                call.args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                            let path = call.args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
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

                    if !approved {
                        approved = prompt_approve("Approve? [y/N] ")? && tool_cfg.enabled;
                    }

                    if !approved {
                        tool_updates.push(format!(
                            "[Tool Denied] tool={} reason=user_denied",
                            call.tool
                        ));
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

                    tool_updates.push(format!(
                        "[Tool Result] tool={}{}\n{}",
                        call.tool,
                        if is_error { " (error)" } else { "" },
                        output
                    ));
                }

                next_message = tool_updates.join("\n\n");
            }
        }

        Ok(())
    }
    .await;

    // If we spawned a gateway in-process, shut it down on exit (best effort).
    if let Some(tx) = shutdown_tx.take() {
        let _ = tx.send(());
    }
    if let Some(task) = gateway_task.take() {
        if let Err(e) = task.await {
            warn!(error = %e, "Gateway task join failed");
        }
    }

    result
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CliChatPrefs {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    last_session_by_project: BTreeMap<String, String>,
}

impl Default for CliChatPrefs {
    fn default() -> Self {
        Self {
            last_session_by_project: BTreeMap::new(),
        }
    }
}

impl CliChatPrefs {
    fn path() -> Option<PathBuf> {
        Config::config_dir().map(|dir| dir.join("cli-chat-prefs.json"))
    }

    fn write_atomic_best_effort(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let tmp = path.with_extension(format!(
            "tmp.{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")
        ));
        if std::fs::write(&tmp, content).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return;
        }

        if std::fs::rename(&tmp, path).is_err() {
            // Windows doesn't allow renaming over an existing file.
            let _ = std::fs::remove_file(path);
            let _ = std::fs::rename(&tmp, path);
        }
        let _ = std::fs::remove_file(&tmp);
    }

    fn load_best_effort() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    fn save_best_effort(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        let Ok(raw) = serde_json::to_string_pretty(self) else {
            return;
        };
        Self::write_atomic_best_effort(&path, &raw);
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct GatewayChatPrefs {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    last_session_by_project: BTreeMap<String, String>,
}

impl Default for GatewayChatPrefs {
    fn default() -> Self {
        Self {
            last_session_by_project: BTreeMap::new(),
        }
    }
}

impl GatewayChatPrefs {
    fn path() -> Option<PathBuf> {
        Config::config_dir().map(|dir| dir.join("gateway-chat-prefs.json"))
    }

    fn load_best_effort() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    fn save_best_effort(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        let Ok(raw) = serde_json::to_string_pretty(self) else {
            return;
        };
        CliChatPrefs::write_atomic_best_effort(&path, &raw);
    }
}

async fn run_chat_direct(
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

    let project_start_dir = if agent {
        root.as_deref()
            .map(|p| PathBuf::from(expand_tilde(p)))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    };
    let project_start_dir = project_start_dir
        .canonicalize()
        .unwrap_or_else(|_| project_start_dir.clone());
    let git_root = drbot_tool_mode::find_git_root_best_effort(&project_start_dir);
    let in_git_project = git_root.is_some();
    let project_root = git_root.unwrap_or(project_start_dir);
    let project_key = project_root.to_string_lossy().to_string();
    let session_map_key = if agent {
        format!("{}#agent", project_key)
    } else {
        format!("{}#chat", project_key)
    };

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
    let mut provider = create_provider(config, &provider_name).await?;
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
        // Try to resume last session for this project (best effort), else resume last CLI session.
        let mut prefs = CliChatPrefs::load_best_effort();
        let mapped_session = prefs
            .last_session_by_project
            .get(&session_map_key)
            .and_then(|s| Uuid::parse_str(s).ok());

        let new_for_project = || async {
            let mut session = Session::new(user_id, "cli", "terminal");
            // See note above: ensure CLI sessions don't collide on UNIQUE(channel_type, channel_id).
            session.channel_id = format!("terminal:{}", session.id);
            session.title = title.clone().or_else(|| Some("CLI Chat".to_string()));
            session.model = model.clone();
            session.system_prompt = system.clone();
            store
                .create(&session)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            Ok::<Session, anyhow::Error>(session)
        };

        let resume_global_last = || async {
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
                Ok::<Session, anyhow::Error>(last_session)
            } else {
                new_for_project().await
            }
        };

        if let Some(uuid) = mapped_session {
            if let Ok(Some(session)) = store.get(uuid).await {
                if session.channel_type == "cli" && session.is_active() {
                    session
                } else {
                    prefs.last_session_by_project.remove(&session_map_key);
                    prefs.save_best_effort();
                    if in_git_project {
                        new_for_project().await?
                    } else {
                        resume_global_last().await?
                    }
                }
            } else {
                prefs.last_session_by_project.remove(&session_map_key);
                prefs.save_best_effort();
                if in_git_project {
                    new_for_project().await?
                } else {
                    resume_global_last().await?
                }
            }
        } else if in_git_project {
            new_for_project().await?
        } else {
            resume_global_last().await?
        }
    };

    if session.channel_type == "cli" && session.is_active() {
        let mut prefs = CliChatPrefs::load_best_effort();
        prefs.last_session_by_project
            .insert(session_map_key.clone(), session.id.to_string());
        prefs.save_best_effort();
    }

    let root_provided = root.is_some();
    let tool_root = if agent {
        let root_path = root
            .map(|p| PathBuf::from(expand_tilde(&p)))
            .unwrap_or(std::env::current_dir().map_err(|e| anyhow::anyhow!("{}", e))?);
        let (resolved, used_default) =
            resolve_tool_root_with_allowlist(root_path, &config.assistant.workspace_allowlist)?;
        if used_default && (agent || root_provided) {
            warn!(
                root = %resolved.display(),
                "tool root not in allowlist; using allowed workspace root"
            );
            eprintln!(
                "Warning: tool root not in allowlist; using {}",
                resolved.display()
            );
        }
        resolved
    } else {
        let root_path =
            std::env::current_dir().map_err(|e| anyhow::anyhow!("{}", e))?;
        let (resolved, used_default) =
            resolve_tool_root_with_allowlist(root_path, &config.assistant.workspace_allowlist)?;
        if used_default && root_provided {
            warn!(
                root = %resolved.display(),
                "tool root not in allowlist; using allowed workspace root"
            );
        }
        resolved
    };

    let mut tool_cfg = ToolModeConfig {
        enabled: agent,
        auto_approve: yes,
        root: tool_root,
        max_rounds: max_tool_rounds.max(1),
        autonomy_mode: config.assistant.autonomy_mode,
        tool_allowlist: config.assistant.tool_allowlist.clone(),
        tool_denylist: config.assistant.tool_denylist.clone(),
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

    let workspace_dir = resolve_default_chat_workspace_dir(config);
    if let Some(dir) = workspace_dir.as_deref() {
        if let Err(err) = ensure_chat_workspace_bootstrap(dir) {
            warn!(
                error = %err,
                workspace = %dir.to_string_lossy(),
                "Failed to bootstrap workspace memory files"
            );
        }
    }

    // Project-local KB (repo-scoped): `.drbot/memory/*.md` under the nearest git root.
    let project_drbot_dir = drbot_tool_mode::resolve_project_drbot_dir_best_effort(&tool_cfg.root);
    let in_git_project_for_notes = drbot_tool_mode::find_git_root_best_effort(&tool_cfg.root).is_some();
    if in_git_project_for_notes && drbot_tool_mode::project_kb_auto_init_enabled() {
        drbot_tool_mode::ensure_project_kb_bootstrap_best_effort(&project_drbot_dir);
    }

    let base_system_prompt_tool_on = Some(build_agent_system_prompt_with_policy(
        base_system.clone(),
        &tool_cfg.root,
        &tool_cfg.tool_allowlist,
        &tool_cfg.tool_denylist,
    ));
    let base_system_prompt_tool_off = base_system.clone();

    let build_effective_system_prompt =
        |tool_enabled: bool, notes: Option<&str>| -> Option<String> {
            let core = if tool_enabled {
                base_system_prompt_tool_on.as_deref()
            } else {
                base_system_prompt_tool_off.as_deref()
            };

            let mut sections: Vec<String> = Vec::new();
            if let Some(core) = core {
                let trimmed = core.trim();
                if !trimmed.is_empty() {
                    sections.push(trimmed.to_string());
                }
            }

            if let Some(dir) = workspace_dir.as_deref() {
                let ctx =
                    drbot_gateway::workspace_chat_context::build_chat_workspace_context_prompt(dir);
                let trimmed = ctx.trim();
                if !trimmed.is_empty() {
                    sections.push(trimmed.to_string());
                }
            }

            if let Some(notes) = notes {
                let trimmed = notes.trim();
                if !trimmed.is_empty() {
                    sections.push(trimmed.to_string());
                }
            }

            if sections.is_empty() {
                None
            } else {
                Some(sections.join("\n\n---\n\n"))
            }
        };

    let base_effective_system_prompt = build_effective_system_prompt(tool_cfg.enabled, None);

    // Build chat options (system prompt may change if tool mode toggles)
    let mut options = ChatOptions {
        model: model.clone(),
        max_tokens: Some(4096),
        temperature: if tool_cfg.enabled { Some(0.2) } else { None },
        top_p: None,
        stop_sequences: None,
        system_prompt: base_effective_system_prompt.clone(),
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
    if let Some(sys) = &base_effective_system_prompt {
        let _ = context_manager.add_message(&Message::system(sys));
    }

    // Add existing session messages to context
    for msg in &session.messages {
        let _ = context_manager.add_message(msg);
    }

    // Single message mode
    if let Some(msg) = single_message {
        let msg_trimmed = msg.trim_start();
        if drbot_tool_mode::is_project_remember_command(msg_trimmed)
            || drbot_tool_mode::is_project_forget_command(msg_trimmed)
        {
            let reply = if drbot_tool_mode::is_project_remember_command(msg_trimmed) {
                match drbot_tool_mode::parse_project_remember_note(msg_trimmed) {
                    Some(note) => match drbot_tool_mode::remember_project_kb(&project_drbot_dir, &note) {
                        Ok(updates) => {
                            if updates.rejected {
                                "Nothing saved (refused to store sensitive/invalid content)."
                                    .to_string()
                            } else if updates.applied {
                                if updates.updates.is_empty() {
                                    "Saved to project memory.".to_string()
                                } else {
                                    let mut out = String::new();
                                    out.push_str("Saved to project memory:\n");
                                    for u in updates.updates.iter().take(12) {
                                        out.push_str("- ");
                                        out.push_str(u);
                                        out.push('\n');
                                    }
                                    out.trim_end().to_string()
                                }
                            } else {
                                "Nothing saved.".to_string()
                            }
                        }
                        Err(e) => format!("Project remember error: {}", e),
                    },
                    None => "Usage: /remember project <note>".to_string(),
                }
            } else {
                match drbot_tool_mode::parse_project_forget_arg(msg_trimmed) {
                    Some(arg) => match drbot_tool_mode::forget_project_kb(&project_drbot_dir, &arg) {
                        Ok(updates) => {
                            if updates.applied {
                                if updates.updates.is_empty() {
                                    "Forgot from project memory.".to_string()
                                } else {
                                    let mut out = String::new();
                                    out.push_str("Forgot from project memory:\n");
                                    for u in updates.updates.iter().take(12) {
                                        out.push_str("- ");
                                        out.push_str(u);
                                        out.push('\n');
                                    }
                                    out.trim_end().to_string()
                                }
                            } else {
                                "Nothing forgotten (no matching items).".to_string()
                            }
                        }
                        Err(e) => format!("Project forget error: {}", e),
                    },
                    None => {
                        "Usage: /forget project <all|pinned|conventions|runbooks|kb|text>"
                            .to_string()
                    }
                }
            };

            session.add_message(Message::user(&msg));
            session.add_message(Message::assistant(&reply));
            session.update_timestamp();
            let _ = store.update(&session).await;

            println!("{}\n", reply);
            return Ok(());
        }

        let mut mem_parts = msg_trimmed.split_whitespace();
        let mem_cmd = mem_parts.next().unwrap_or("");
        let mem_arg = mem_parts.next().unwrap_or("");
        if (mem_cmd == "/memory" || mem_cmd == "/mem") && mem_arg.eq_ignore_ascii_case("project") {
            let reply = drbot_tool_mode::build_project_memory_overview(&project_drbot_dir);
            println!("{}\n", reply);
            return Ok(());
        }

        let msg_lower = msg_trimmed.to_ascii_lowercase();
        let is_remember_cmd =
            msg_lower.starts_with("/remember") || msg_lower.starts_with("remember:");
        let is_forget_cmd = msg_lower.starts_with("/forget") || msg_lower.starts_with("forget:");
        let is_memory_cmd = msg_lower == "/memory" || msg_lower == "/mem";
        let is_profile_cmd = msg_lower == "/profile";
        let is_kb_cmd = msg_lower == "/kb"
            || msg_lower.starts_with("/kb ")
            || msg_lower.starts_with("/kb:")
            || msg_lower.starts_with("kb:")
            || msg_lower == "/notes"
            || msg_lower.starts_with("/notes ")
            || msg_lower.starts_with("/notes:")
            || msg_lower.starts_with("notes:");
        let is_local_cmd =
            is_remember_cmd || is_forget_cmd || is_memory_cmd || is_profile_cmd || is_kb_cmd;

        if is_local_cmd {
            let Some(dir) = workspace_dir.as_deref() else {
                return Err(anyhow::anyhow!("Workspace directory is unavailable"));
            };

            let should_persist = is_remember_cmd || is_forget_cmd;
            let reply = if is_remember_cmd || is_forget_cmd {
                let updates = if is_remember_cmd {
                    drbot_gateway::workspace_autosave::autosave_workspace_best_effort(dir, &msg)
                } else {
                    drbot_gateway::workspace_autosave::forget_workspace_best_effort(dir, &msg)
                };

                if updates.applied {
                    if updates.updates.is_empty() {
                        if is_remember_cmd {
                            "Saved to memory.".to_string()
                        } else {
                            "Forgot.".to_string()
                        }
                    } else {
                        let mut out = String::new();
                        out.push_str(if is_remember_cmd {
                            "Saved to memory:\n"
                        } else {
                            "Forgot:\n"
                        });
                        for u in updates.updates.iter().take(12) {
                            out.push_str("- ");
                            out.push_str(u);
                            out.push('\n');
                        }
                        out.trim_end().to_string()
                    }
                } else if is_remember_cmd {
                    if drbot_gateway::workspace_autosave::parse_remember_command(&msg).is_some() {
                        "Nothing saved (refused to store sensitive/invalid content).".to_string()
                    } else {
                        "Usage: /remember <note>".to_string()
                    }
                } else if drbot_gateway::workspace_autosave::parse_forget_command(&msg).is_some() {
                    "Nothing forgotten (no matching items).".to_string()
                } else {
                    "Usage: /forget <name|timezone|style|all|text>".to_string()
                }
            } else if is_profile_cmd {
                drbot_gateway::workspace_memory_view::build_workspace_profile_overview(dir)
            } else if is_memory_cmd {
                let workspace = drbot_gateway::workspace_memory_view::build_workspace_memory_overview(dir);
                let project = if in_git_project_for_notes || project_drbot_dir.is_dir() {
                    drbot_tool_mode::build_project_memory_overview(&project_drbot_dir)
                } else {
                    String::new()
                };
                if project.trim().is_empty() {
                    workspace
                } else {
                    format!("{}\n\n{}", workspace.trim(), project.trim())
                }
            } else if is_kb_cmd {
                let query = if msg_lower == "/kb"
                    || msg_lower.starts_with("/kb ")
                    || msg_lower.starts_with("/kb:")
                {
                    msg_trimmed[3..].trim_start_matches(&[' ', ':'][..]).trim()
                } else if msg_lower.starts_with("kb:") {
                    msg_trimmed[3..].trim()
                } else if msg_lower == "/notes"
                    || msg_lower.starts_with("/notes ")
                    || msg_lower.starts_with("/notes:")
                {
                    msg_trimmed[6..].trim_start_matches(&[' ', ':'][..]).trim()
                } else if msg_lower.starts_with("notes:") {
                    msg_trimmed[6..].trim()
                } else {
                    ""
                };

                if query.trim().is_empty() {
                    "Usage: /kb <query>".to_string()
                } else {
                    drbot_gateway::workspace_notes_recall::recall_workspace_notes_prompt_explicit_with_project(
                        dir,
                        Some(&project_drbot_dir),
                        query,
                    )
                    .await
                    .unwrap_or_else(|| "No relevant notes found.".to_string())
                }
            } else {
                "Unknown local command.".to_string()
            };

            if should_persist {
                // Persist to local session transcript.
                session.add_message(Message::user(&msg));
                session.add_message(Message::assistant(&reply));
                session.update_timestamp();
                let _ = store.update(&session).await;
            }

            println!("{}\n", reply);
            return Ok(());
        }

        // Add user message to context and session
        let user_msg = Message::user(&msg);
        let _ = context_manager.add_message(&user_msg);
        session.add_message(user_msg);

        if let Some(dir) = workspace_dir.as_deref() {
            drbot_gateway::workspace_autosave::autosave_workspace_best_effort(dir, &msg);
        }
        if in_git_project_for_notes {
            let msg_trimmed = msg.trim_start();
            let is_internal_tool_message = msg_trimmed.starts_with("[Tool Result]")
                || msg_trimmed.starts_with("[Tool Denied]")
                || msg_trimmed.starts_with("[Tool Mode Strict]");
            if !is_internal_tool_message {
                drbot_tool_mode::autosave_project_kb_best_effort(&project_drbot_dir, &msg);
            }
        }

        let workspace_notes = match workspace_dir.as_deref() {
            Some(dir) => {
                drbot_gateway::workspace_notes_recall::recall_workspace_notes_prompt_with_project(
                    dir,
                    Some(&project_drbot_dir),
                    &msg,
                )
                .await
            }
            None => {
                drbot_gateway::workspace_notes_recall::recall_project_notes_prompt(
                    &project_drbot_dir,
                    &msg,
                )
                .await
            }
        };
        let mut send_options = options.clone();
        send_options.system_prompt =
            build_effective_system_prompt(tool_cfg.enabled, workspace_notes.as_deref());

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
                send_chat(provider.as_ref(), &messages_to_send, &send_options, stream).await?;

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
                let approved = match call.tool.as_str() {
                    "read_file" | "list_dir" | "list_directory" | "search" => true,
                    "bash" => {
                        let command = call
                            .args
                            .get("command")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        bash_command_is_safe_for_auto_approve(command, &bash_policy)
                    }
                    _ => tool_cfg.auto_approve,
                };

                if !approved {
                    match call.tool.as_str() {
                        "bash" => {
                            let command = call
                                .args
                                .get("command")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            println!("\n[Tool Denied] bash (unsafe_for_auto_approve)");
                            println!("command: {}", command);
                            println!();
                            let denied = Message::user(format!(
                                "[Tool Denied] tool=bash reason=unsafe_for_auto_approve\ncommand: {}",
                                command
                            ));
                            let _ = context_manager.add_message(&denied);
                            session.add_message(denied);
                        }
                        "write_file" => {
                            let path = call.args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            let bytes = call
                                .args
                                .get("content")
                                .and_then(|v| v.as_str())
                                .map(|s| s.len())
                                .unwrap_or(0);
                            println!("\n[Tool Denied] write_file (approval_required)");
                            println!("path: {}  bytes: {}", path, bytes);
                            println!(
                                "hint: re-run with -y/--yes (or use interactive mode and approve)"
                            );
                            println!();
                            let denied = Message::user(format!(
                                "[Tool Denied] tool=write_file reason=approval_required\npath: {}\nbytes: {}",
                                path, bytes
                            ));
                            let _ = context_manager.add_message(&denied);
                            session.add_message(denied);
                        }
                        "apply_patch" => {
                            let bytes = call
                                .args
                                .get("patch")
                                .and_then(|v| v.as_str())
                                .map(|s| s.len())
                                .unwrap_or(0);
                            println!("\n[Tool Denied] apply_patch (approval_required)");
                            println!("bytes: {}", bytes);
                            println!(
                                "hint: re-run with -y/--yes (or use interactive mode and approve)"
                            );
                            println!();
                            let denied = Message::user(format!(
                                "[Tool Denied] tool=apply_patch reason=approval_required\nbytes: {}",
                                bytes
                            ));
                            let _ = context_manager.add_message(&denied);
                            session.add_message(denied);
                        }
                        other => {
                            println!("\n[Tool Denied] {} (approval_required)", other);
                            println!(
                                "hint: re-run with -y/--yes (or use interactive mode and approve)"
                            );
                            println!();
                            let denied = Message::user(format!(
                                "[Tool Denied] tool={} reason=approval_required",
                                other
                            ));
                            let _ = context_manager.add_message(&denied);
                            session.add_message(denied);
                        }
                    }
                    continue;
                }

                let (output, is_error) = match execute_tool_call(&tool_cfg, &call).await {
                    Ok((out, err)) => (out, err),
                    Err(e) => (format!("Error: {}", e), true),
                };

                println!(
                    "\n[Tool Result] {}{}\n",
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
    println!("Commands: /quit, /clear, /save, /info, /sessions, /new, /context, /tools, /approve, /agent, /provider, /model, /remember, /forget, /remember project, /forget project, /memory, /memory project, /profile, /kb");
    if tool_cfg.enabled {
        println!(
            "Tool mode: ON (auto-approve: {})  Autonomy: {:?}  Root: {}",
            if tool_cfg.auto_approve { "ON" } else { "OFF" },
            tool_cfg.autonomy_mode,
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

        let input_lower = input.to_ascii_lowercase();
        let mut mem_parts = input.split_whitespace();
        let mem_cmd = mem_parts.next().unwrap_or("");
        let mem_arg = mem_parts.next().unwrap_or("");
        if (mem_cmd == "/memory" || mem_cmd == "/mem") && mem_arg.eq_ignore_ascii_case("project") {
            let reply = drbot_tool_mode::build_project_memory_overview(&project_drbot_dir);
            println!("\n{}\n", reply);
            continue;
        }
        if drbot_tool_mode::is_project_remember_command(input)
            || drbot_tool_mode::is_project_forget_command(input)
        {
            let reply = if drbot_tool_mode::is_project_remember_command(input) {
                match drbot_tool_mode::parse_project_remember_note(input) {
                    Some(note) => match drbot_tool_mode::remember_project_kb(&project_drbot_dir, &note) {
                        Ok(updates) => {
                            if updates.rejected {
                                "Nothing saved (refused to store sensitive/invalid content)."
                                    .to_string()
                            } else if updates.applied {
                                if updates.updates.is_empty() {
                                    "Saved to project memory.".to_string()
                                } else {
                                    let mut out = String::new();
                                    out.push_str("Saved to project memory:\n");
                                    for u in updates.updates.iter().take(12) {
                                        out.push_str("- ");
                                        out.push_str(u);
                                        out.push('\n');
                                    }
                                    out.trim_end().to_string()
                                }
                            } else {
                                "Nothing saved.".to_string()
                            }
                        }
                        Err(e) => format!("Project remember error: {}", e),
                    },
                    None => "Usage: /remember project <note>".to_string(),
                }
            } else {
                match drbot_tool_mode::parse_project_forget_arg(input) {
                    Some(arg) => match drbot_tool_mode::forget_project_kb(&project_drbot_dir, &arg) {
                        Ok(updates) => {
                            if updates.applied {
                                if updates.updates.is_empty() {
                                    "Forgot from project memory.".to_string()
                                } else {
                                    let mut out = String::new();
                                    out.push_str("Forgot from project memory:\n");
                                    for u in updates.updates.iter().take(12) {
                                        out.push_str("- ");
                                        out.push_str(u);
                                        out.push('\n');
                                    }
                                    out.trim_end().to_string()
                                }
                            } else {
                                "Nothing forgotten (no matching items).".to_string()
                            }
                        }
                        Err(e) => format!("Project forget error: {}", e),
                    },
                    None => {
                        "Usage: /forget project <all|pinned|conventions|runbooks|kb|text>"
                            .to_string()
                    }
                }
            };

            session.add_message(Message::user(input));
            session.add_message(Message::assistant(&reply));
            session.update_timestamp();
            let _ = store.update(&session).await;

            println!("\n{}\n", reply);
            continue;
        }
        let is_remember_cmd =
            input_lower.starts_with("/remember") || input_lower.starts_with("remember:");
        let is_forget_cmd =
            input_lower.starts_with("/forget") || input_lower.starts_with("forget:");
        if is_remember_cmd || is_forget_cmd {
            if let Some(dir) = workspace_dir.as_deref() {
                let updates = if is_remember_cmd {
                    drbot_gateway::workspace_autosave::autosave_workspace_best_effort(dir, input)
                } else {
                    drbot_gateway::workspace_autosave::forget_workspace_best_effort(dir, input)
                };
                let reply = if updates.applied {
                    if updates.updates.is_empty() {
                        if is_remember_cmd {
                            "Saved to memory.".to_string()
                        } else {
                            "Forgot.".to_string()
                        }
                    } else {
                        let mut out = String::new();
                        out.push_str(if is_remember_cmd {
                            "Saved to memory:\n"
                        } else {
                            "Forgot:\n"
                        });
                        for u in updates.updates.iter().take(12) {
                            out.push_str("- ");
                            out.push_str(u);
                            out.push('\n');
                        }
                        out.trim_end().to_string()
                    }
                } else if is_remember_cmd {
                    if drbot_gateway::workspace_autosave::parse_remember_command(input).is_some() {
                        "Nothing saved (refused to store sensitive/invalid content).".to_string()
                    } else {
                        "Usage: /remember <note>".to_string()
                    }
                } else if drbot_gateway::workspace_autosave::parse_forget_command(input).is_some() {
                    "Nothing forgotten (no matching items).".to_string()
                } else {
                    "Usage: /forget <name|timezone|style|all|text>".to_string()
                };

                // Persist locally (but don't send to provider).
                session.add_message(Message::user(input));
                session.add_message(Message::assistant(&reply));
                session.update_timestamp();
                let _ = store.update(&session).await;

                println!("\n{}\n", reply);
                continue;
            } else {
                println!("\nWorkspace directory is unavailable.\n");
                continue;
            }
        }

        let is_memory_cmd = input_lower == "/memory" || input_lower == "/mem";
        let is_profile_cmd = input_lower == "/profile";
        let is_kb_cmd = input_lower == "/kb"
            || input_lower.starts_with("/kb ")
            || input_lower.starts_with("/kb:")
            || input_lower.starts_with("kb:")
            || input_lower == "/notes"
            || input_lower.starts_with("/notes ")
            || input_lower.starts_with("/notes:")
            || input_lower.starts_with("notes:");
        if is_memory_cmd || is_profile_cmd || is_kb_cmd {
            if let Some(dir) = workspace_dir.as_deref() {
                let reply = if is_profile_cmd {
                    drbot_gateway::workspace_memory_view::build_workspace_profile_overview(dir)
                } else if is_memory_cmd {
                    let workspace =
                        drbot_gateway::workspace_memory_view::build_workspace_memory_overview(dir);
                    let project = if in_git_project_for_notes || project_drbot_dir.is_dir() {
                        drbot_tool_mode::build_project_memory_overview(&project_drbot_dir)
                    } else {
                        String::new()
                    };
                    if project.trim().is_empty() {
                        workspace
                    } else {
                        format!("{}\n\n{}", workspace.trim(), project.trim())
                    }
                } else {
                    let query = if input_lower == "/kb"
                        || input_lower.starts_with("/kb ")
                        || input_lower.starts_with("/kb:")
                    {
                        input[3..].trim_start_matches(&[' ', ':'][..]).trim()
                    } else if input_lower.starts_with("kb:") {
                        input[3..].trim()
                    } else if input_lower == "/notes"
                        || input_lower.starts_with("/notes ")
                        || input_lower.starts_with("/notes:")
                    {
                        input[6..].trim_start_matches(&[' ', ':'][..]).trim()
                    } else if input_lower.starts_with("notes:") {
                        input[6..].trim()
                    } else {
                        ""
                    };

                    if query.trim().is_empty() {
                        "Usage: /kb <query>".to_string()
                    } else {
                        drbot_gateway::workspace_notes_recall::recall_workspace_notes_prompt_explicit_with_project(
                            dir,
                            Some(&project_drbot_dir),
                            query,
                        )
                        .await
                        .unwrap_or_else(|| "No relevant notes found.".to_string())
                    }
                };

                println!("\n{}\n", reply);
            } else {
                println!("\nWorkspace directory is unavailable.\n");
            }
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
            println!(
                "Tool mode: {} (autonomy: {:?})",
                if tool_cfg.enabled { "ON" } else { "OFF" },
                tool_cfg.autonomy_mode
            );
            println!(
                "Auto-approve: {}",
                if tool_cfg.auto_approve { "ON" } else { "OFF" }
            );
            println!("Strict agent: {}", if agent_strict { "ON" } else { "OFF" });
            println!("Root: {}", tool_cfg.root.display());
            println!("Max tool rounds: {}", tool_cfg.max_rounds);
            let allowlist = if tool_cfg.tool_allowlist.is_empty() {
                "all".to_string()
            } else {
                tool_cfg.tool_allowlist.join(", ")
            };
            let denylist = if tool_cfg.tool_denylist.is_empty() {
                "(none)".to_string()
            } else {
                tool_cfg.tool_denylist.join(", ")
            };
            println!("Tool allowlist: {}", allowlist);
            println!("Tool denylist: {}", denylist);
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
                    options.system_prompt = build_effective_system_prompt(true, None);
                    options.temperature = Some(0.2);
                    println!("\nTool mode: ON (autonomy: {:?})\n", tool_cfg.autonomy_mode);
                }
                "off" | "no" | "false" => {
                    tool_cfg.enabled = false;
                    options.system_prompt = build_effective_system_prompt(false, None);
                    options.temperature = None;
                    println!(
                        "\nTool mode: OFF (autonomy: {:?})\n",
                        tool_cfg.autonomy_mode
                    );
                }
                _ => {
                    println!(
                        "\nTool mode: {} (autonomy: {:?}) (use: /agent on|off)\n",
                        if tool_cfg.enabled { "ON" } else { "OFF" },
                        tool_cfg.autonomy_mode
                    );
                }
            }
            continue;
        }

        if input == "/provider" || input.starts_with("/provider ") {
            let arg = input.strip_prefix("/provider").unwrap().trim();
            if arg.is_empty() {
                println!(
                    "\nProvider: {}\n(use: /provider list | /provider <name>)\n",
                    current_provider_name
                );
                continue;
            }

            if arg == "list" {
                fn normalize_http_url(raw: &str) -> Option<String> {
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    if trimmed.contains("://") {
                        Some(trimmed.to_string())
                    } else {
                        Some(format!("http://{}", trimmed))
                    }
                }

                let ollama_url: String = if let Some(ollama) = &config.providers.ollama {
                    ollama.url.trim().to_string()
                } else if let Ok(v) = std::env::var("DRBOT_OLLAMA_URL") {
                    normalize_http_url(&v)
                        .unwrap_or_else(|| drbot_ollama::DEFAULT_BASE_URL.to_string())
                } else if let Ok(v) = std::env::var("OLLAMA_HOST") {
                    normalize_http_url(&v)
                        .unwrap_or_else(|| drbot_ollama::DEFAULT_BASE_URL.to_string())
                } else {
                    drbot_ollama::DEFAULT_BASE_URL.to_string()
                };

                let claude_ok = CliProvider::claude_cli().check_command_exists().is_ok();
                let codex_ok = CliProvider::codex_cli().check_command_exists().is_ok();
                let ollama_ok = check_ollama_health_with_timeout(
                    &ollama_url,
                    std::time::Duration::from_millis(350),
                )
                .await
                .unwrap_or(false);
                let codex_oss_ok = CliProvider::codex_oss_ollama()
                    .check_command_exists()
                    .is_ok()
                    && ollama_ok;

                let mut any_available = false;
                if claude_ok
                    || codex_ok
                    || codex_oss_ok
                    || ollama_ok
                    || config.providers.anthropic.is_some()
                    || config.providers.openai.is_some()
                    || !config.providers.openai_compatible.is_empty()
                    || !config.providers.cli.is_empty()
                {
                    any_available = true;
                }

                println!("\nProviders:");
                println!(
                    "  - auto ({})",
                    if any_available {
                        "available"
                    } else {
                        "unavailable: no providers available (try drbot wizard)"
                    }
                );
                println!(
                    "  - claude-cli ({})",
                    if claude_ok {
                        "available"
                    } else {
                        "unavailable"
                    }
                );
                println!(
                    "  - codex-cli ({})",
                    if codex_ok { "available" } else { "unavailable" }
                );
                println!(
                    "  - codex-oss ({})",
                    if codex_oss_ok {
                        "available"
                    } else if !ollama_ok {
                        "unavailable: ollama not running"
                    } else {
                        "unavailable: codex CLI missing"
                    }
                );
                println!(
                    "  - ollama ({})  {}",
                    if ollama_ok {
                        "available"
                    } else {
                        "unavailable"
                    },
                    ollama_url
                );
                println!(
                    "  - anthropic ({})",
                    if config.providers.anthropic.is_some() {
                        "configured"
                    } else {
                        "unconfigured"
                    }
                );
                println!(
                    "  - openai ({})",
                    if config.providers.openai.is_some() {
                        "configured"
                    } else {
                        "unconfigured"
                    }
                );

                for cfg in config.providers.openai_compatible.iter() {
                    println!("  - {} (openai-compatible) (configured)", cfg.name.trim());
                }

                for cfg in config.providers.cli.iter() {
                    let p = CliProvider::from_config(cfg);
                    let ok = p.check_command_exists().is_ok();
                    println!(
                        "  - {} (cli) ({})",
                        cfg.name.trim(),
                        if ok { "available" } else { "unavailable" }
                    );
                }

                println!();
                continue;
            }

            match create_provider(config, arg).await {
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
                match create_provider(config, prov).await {
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
                    "codex-oss",
                    "codex-local",
                ];
                if known_providers.contains(&arg.to_lowercase().as_str()) {
                    match create_provider(config, arg).await {
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

        if let Some(dir) = workspace_dir.as_deref() {
            drbot_gateway::workspace_autosave::autosave_workspace_best_effort(dir, input);
        }
        if in_git_project_for_notes {
            let msg_trimmed = input.trim_start();
            let is_internal_tool_message = msg_trimmed.starts_with("[Tool Result]")
                || msg_trimmed.starts_with("[Tool Denied]")
                || msg_trimmed.starts_with("[Tool Mode Strict]");
            if !is_internal_tool_message {
                drbot_tool_mode::autosave_project_kb_best_effort(&project_drbot_dir, input);
            }
        }

        let workspace_notes = match workspace_dir.as_deref() {
            Some(dir) => {
                drbot_gateway::workspace_notes_recall::recall_workspace_notes_prompt_with_project(
                    dir,
                    Some(&project_drbot_dir),
                    input,
                )
                .await
            }
            None => {
                drbot_gateway::workspace_notes_recall::recall_project_notes_prompt(
                    &project_drbot_dir,
                    input,
                )
                .await
            }
        };
        let mut send_options = options.clone();
        send_options.system_prompt =
            build_effective_system_prompt(tool_cfg.enabled, workspace_notes.as_deref());

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

            let response = match send_chat(
                provider.as_ref(),
                &messages_to_send,
                &send_options,
                stream,
            )
            .await
            {
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
                // Default to "automated in place":
                // - Read-only tools run without prompting.
                // - Safe bash commands run without prompting.
                // - Writes require explicit approval unless auto-approve is enabled.
                let mut approved = match call.tool.as_str() {
                    "read_file" | "list_dir" | "list_directory" | "search" => true,
                    "bash" => {
                        let command = call
                            .args
                            .get("command")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        bash_command_is_safe_for_auto_approve(command, &bash_policy)
                    }
                    _ => tool_cfg.auto_approve,
                };

                // Print tool summary (even when auto-approved) for transparency.
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
                            println!("[Tool] bash\n  cwd: {}\n  command: {}", cwd.trim(), command);
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

                if !approved {
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
    println!(
        "  Pairing required: {}",
        if config.gateway.pairing_required {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "  Pairing allow local: {}",
        if config.gateway.pairing_allow_local {
            "yes"
        } else {
            "no"
        }
    );
    println!();
    println!("Assistant:");
    println!("  Autonomy: {:?}", config.assistant.autonomy_mode);
    if config.assistant.workspace_allowlist.is_empty() {
        println!("  Workspace allowlist: (none)");
    } else {
        println!("  Workspace allowlist:");
        for path in &config.assistant.workspace_allowlist {
            println!("    - {}", path.display());
        }
    }
    if config.assistant.tool_allowlist.is_empty() {
        println!("  Tool allowlist: (all)");
    } else {
        println!("  Tool allowlist: {}", config.assistant.tool_allowlist.join(", "));
    }
    if config.assistant.tool_denylist.is_empty() {
        println!("  Tool denylist: (none)");
    } else {
        println!("  Tool denylist: {}", config.assistant.tool_denylist.join(", "));
    }
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
        if !config.gateway.pairing_required {
            findings.push(Finding {
                severity: Severity::Warning,
                title: "Pairing not required for remote operators".to_string(),
                details: vec![
                    "OpenClaw operator connections can attach without device pairing."
                        .to_string(),
                    "Fix: set `gateway.pairing_required=true` (and keep auth enabled).".to_string(),
                ],
            });
        }
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
        if auth_token.is_empty() {
            findings.push(Finding {
                severity: Severity::Warning,
                title: "Gateway on loopback without auth token".to_string(),
                details: vec![
                    format!("bind: {}:{}", host, config.gateway.port),
                    "This is reachable by any local process and may be reachable by websites via localhost WebSockets.".to_string(),
                    "Fix: set `gateway.auth_token` (recommended even on loopback).".to_string(),
                ],
            });
        } else {
            let weak = auth_token.len() < 16
                || matches!(
                    auth_token.to_ascii_lowercase().as_str(),
                    "changeme" | "change-me" | "password" | "token"
                );
            findings.push(Finding {
                severity: if weak { Severity::Warning } else { Severity::Ok },
                title: "Gateway bind policy".to_string(),
                details: vec![
                    format!("bind: {}:{}", host, config.gateway.port),
                    if weak {
                        "Auth token looks weak; use a long random token.".to_string()
                    } else {
                        "Auth token configured.".to_string()
                    },
                ],
            });
        }
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
    match config.assistant.autonomy_mode {
        drbot_core::config::AutonomyMode::ReadOnly => {
            findings.push(Finding {
                severity: Severity::Info,
                title: "Assistant autonomy: read-only".to_string(),
                details: vec![
                    "Tool writes and exec are disabled by default.".to_string(),
                ],
            });
        }
        drbot_core::config::AutonomyMode::Full => {
            findings.push(Finding {
                severity: Severity::Warning,
                title: "Assistant autonomy: full".to_string(),
                details: vec![
                    "Full autonomy enables tool runs without extra supervision.".to_string(),
                    "Fix: consider `assistant.autonomy_mode=supervised` for safer defaults."
                        .to_string(),
                ],
            });
        }
        drbot_core::config::AutonomyMode::Supervised => {}
    }
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
    let claude_cli = CliProvider::claude_cli();
    println!(
        "  Claude CLI: {}",
        if claude_cli.check_command_exists().is_ok() {
            "available"
        } else {
            "not found"
        }
    );
    let codex_cli = CliProvider::codex_cli();
    let codex_ok = codex_cli.check_command_exists().is_ok();
    println!(
        "  Codex CLI: {}",
        if codex_ok { "available" } else { "not found" }
    );

    fn normalize_http_url(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Ollama commonly uses OLLAMA_HOST like "127.0.0.1:11434".
        if trimmed.contains("://") {
            Some(trimmed.to_string())
        } else {
            Some(format!("http://{}", trimmed))
        }
    }

    let ollama_source;
    let ollama_url: String = if let Some(ollama) = &config.providers.ollama {
        ollama_source = "configured";
        ollama.url.clone()
    } else if let Ok(v) = std::env::var("DRBOT_OLLAMA_URL") {
        if let Some(u) = normalize_http_url(&v) {
            ollama_source = "env";
            u
        } else {
            ollama_source = "default";
            drbot_ollama::DEFAULT_BASE_URL.to_string()
        }
    } else if let Ok(v) = std::env::var("OLLAMA_HOST") {
        if let Some(u) = normalize_http_url(&v) {
            ollama_source = "env";
            u
        } else {
            ollama_source = "default";
            drbot_ollama::DEFAULT_BASE_URL.to_string()
        }
    } else {
        ollama_source = "default";
        drbot_ollama::DEFAULT_BASE_URL.to_string()
    };

    let ollama_ok = match check_ollama_health(&ollama_url).await {
        Ok(true) => {
            if ollama_source == "configured" {
                println!("  Ollama: running ({})", ollama_url);
            } else {
                println!("  Ollama: running (auto-detected at {})", ollama_url);
            }
            true
        }
        Ok(false) => {
            if ollama_source == "configured" {
                println!("  Ollama: configured but not responding ({})", ollama_url);
            } else {
                println!("  Ollama: not running ({})", ollama_url);
            }
            false
        }
        Err(e) => {
            if ollama_source == "configured" {
                println!("  Ollama: error checking ({}): {}", ollama_url, e);
            } else {
                println!("  Ollama: error checking ({}): {}", ollama_url, e);
            }
            false
        }
    };

    println!(
        "  Codex OSS (Ollama): {}",
        if codex_ok && ollama_ok {
            "available"
        } else if !codex_ok {
            "not available (codex CLI missing)"
        } else {
            "not available (Ollama not running)"
        }
    );
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
async fn check_ollama_health_with_timeout(url: &str, timeout: std::time::Duration) -> Result<bool> {
    let client = reqwest::Client::builder().timeout(timeout).build()?;

    // Ollama's API endpoint - try to list models
    let api_url = format!("{}/api/tags", url.trim_end_matches('/'));

    match client.get(&api_url).send().await {
        Ok(response) => Ok(response.status().is_success()),
        Err(e) if e.is_timeout() => Ok(false),
        Err(e) if e.is_connect() => Ok(false),
        Err(e) => Err(anyhow::anyhow!("{}", e)),
    }
}

/// Check if Ollama is running by hitting its health endpoint.
async fn check_ollama_health(url: &str) -> Result<bool> {
    check_ollama_health_with_timeout(url, std::time::Duration::from_secs(5)).await
}

/// Interactive setup wizard.
async fn run_wizard() -> Result<()> {
    use drbot_core::config::{AnthropicConfig, AutonomyMode, OllamaConfig, OpenAIConfig};

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

    let mut input = String::new();
    fn parse_yes_no(raw: &str, default_yes: bool) -> bool {
        let t = raw.trim().to_ascii_lowercase();
        if t.is_empty() {
            default_yes
        } else {
            t.starts_with('y')
        }
    }

    fn normalize_http_url(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Ollama commonly uses OLLAMA_HOST like "127.0.0.1:11434".
        if trimmed.contains("://") {
            Some(trimmed.to_string())
        } else {
            Some(format!("http://{}", trimmed))
        }
    }

    fn parse_autonomy_mode(raw: &str) -> Option<AutonomyMode> {
        let t = raw.trim().to_ascii_lowercase();
        if t.is_empty() {
            return None;
        }
        match t.as_str() {
            "read" | "readonly" | "read-only" | "read_only" => Some(AutonomyMode::ReadOnly),
            "supervised" | "supervise" | "super" => Some(AutonomyMode::Supervised),
            "full" => Some(AutonomyMode::Full),
            _ => None,
        }
    }

    fn parse_csv_list(raw: &str) -> Vec<String> {
        raw.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    }

    // Detect cost-savers first (no API cost).
    let claude_ok = CliProvider::claude_cli().check_command_exists().is_ok();
    let codex_ok = CliProvider::codex_cli().check_command_exists().is_ok();

    let ollama_url: String = if let Ok(v) = std::env::var("DRBOT_OLLAMA_URL") {
        normalize_http_url(&v).unwrap_or_else(|| drbot_ollama::DEFAULT_BASE_URL.to_string())
    } else if let Ok(v) = std::env::var("OLLAMA_HOST") {
        normalize_http_url(&v).unwrap_or_else(|| drbot_ollama::DEFAULT_BASE_URL.to_string())
    } else {
        drbot_ollama::DEFAULT_BASE_URL.to_string()
    };

    let ollama_ok = match check_ollama_health(&ollama_url).await {
        Ok(v) => v,
        Err(_) => false,
    };

    println!("Local cost-savers (no API cost):");
    println!(
        "  Claude CLI (provider: claude-cli): {}",
        if claude_ok { "available" } else { "not found" }
    );
    println!(
        "  Codex CLI (provider: codex-cli): {}",
        if codex_ok { "available" } else { "not found" }
    );
    println!(
        "  Ollama (provider: ollama): {} ({})",
        if ollama_ok { "running" } else { "not running" },
        ollama_url
    );
    println!();
    println!("Tip: If you set default provider to 'auto', drbot prefers:");
    println!("  claude-cli/codex-cli → ollama → APIs");
    println!();

    // Ollama (optional). If it's already running and no CLI providers are available, default to yes.
    let ollama_default_yes = ollama_ok && !(claude_ok || codex_ok);
    print!(
        "Configure Ollama settings (optional; auto-detected if running)? [{}]: ",
        if ollama_default_yes { "Y/n" } else { "y/N" }
    );
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    let configure_ollama = parse_yes_no(&input, ollama_default_yes);

    if configure_ollama {
        print!("  Ollama URL [{}]: ", ollama_url);
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        let url = if input.trim().is_empty() {
            ollama_url.clone()
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

    // API providers (optional). If no cost-savers were detected, default to yes.
    let api_default_yes = !(claude_ok || codex_ok || ollama_ok);

    // Anthropic
    print!(
        "Configure Anthropic Claude (API)? [{}]: ",
        if api_default_yes { "Y/n" } else { "y/N" }
    );
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    let configure_anthropic = parse_yes_no(&input, api_default_yes);

    if configure_anthropic {
        // Check environment variable first
        let env_key = std::env::var("ANTHROPIC_API_KEY").ok();

        let api_key = if let Some(key) = env_key {
            println!("  Found ANTHROPIC_API_KEY in environment.");
            print!("  Use environment variable? [Y/n]: ");
            io::stdout().flush()?;
            input.clear();
            io::stdin().read_line(&mut input)?;
            if parse_yes_no(&input, true) {
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
    print!(
        "Configure OpenAI (API)? [{}]: ",
        if api_default_yes { "Y/n" } else { "y/N" }
    );
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    let configure_openai = parse_yes_no(&input, api_default_yes);

    if configure_openai {
        let env_key = std::env::var("OPENAI_API_KEY").ok();

        let api_key = if let Some(key) = env_key {
            println!("  Found OPENAI_API_KEY in environment.");
            print!("  Use environment variable? [Y/n]: ");
            io::stdout().flush()?;
            input.clear();
            io::stdin().read_line(&mut input)?;
            if parse_yes_no(&input, true) {
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

    // Default provider: always offer "auto" (cost-savers first when available).
    println!("Default provider controls what drbot picks when you don't pass --provider.");
    println!("Recommended: auto (claude-cli/codex-cli → ollama → APIs).");
    print!("Default provider [auto]: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    let chosen = if input.trim().is_empty() {
        "auto".to_string()
    } else {
        input.trim().to_string()
    };
    config.providers.default_provider = Some(chosen);

    let any_provider = claude_ok
        || codex_ok
        || ollama_ok
        || config.providers.ollama.is_some()
        || config.providers.anthropic.is_some()
        || config.providers.openai.is_some();
    if !any_provider {
        println!();
        println!(
            "Warning: No providers configured or detected. Install claude/codex CLI, start Ollama, or configure an API key."
        );
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

    // --- Assistant Policy ---
    println!("┌─ Assistant Policy ────────────────────────┐");
    println!();
    println!("Autonomy controls default tool behavior for agents.");
    print!("Autonomy mode [supervised]: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if let Some(mode) = parse_autonomy_mode(&input) {
        config.assistant.autonomy_mode = mode;
    }

    print!("Workspace allowlist (comma-separated paths, blank = allow any): ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if !input.trim().is_empty() {
        config.assistant.workspace_allowlist = parse_csv_list(&input)
            .into_iter()
            .map(PathBuf::from)
            .collect();
    }

    print!("Tool allowlist (comma-separated, blank = allow all): ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if !input.trim().is_empty() {
        config.assistant.tool_allowlist = parse_csv_list(&input);
    }

    print!("Tool denylist (comma-separated, blank = deny none): ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if !input.trim().is_empty() {
        config.assistant.tool_denylist = parse_csv_list(&input);
    }

    print!("Require pairing for non-local operator connections? [y/N]: ");
    io::stdout().flush()?;
    input.clear();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase().starts_with('y') {
        config.gateway.pairing_required = true;
        print!("  Allow local loopback without pairing? [Y/n]: ");
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        config.gateway.pairing_allow_local = !input.trim().to_lowercase().starts_with('n');
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
