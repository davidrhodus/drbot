//! CLI command definitions.

use clap::{Parser, Subcommand};

/// drbot - Personal AI assistant
#[derive(Parser, Debug)]
#[command(name = "drbot")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Configuration file path
    #[arg(short, long, global = true)]
    pub config: Option<String>,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output format
    #[arg(long, global = true, default_value = "text")]
    pub format: OutputFormat,

    #[command(subcommand)]
    pub command: Commands,
}

/// Output format for commands.
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum OutputFormat {
    /// Human-readable text output
    #[default]
    Text,
    /// JSON output
    Json,
}

/// Available commands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the gateway server
    Gateway(GatewayArgs),

    /// Manage configuration
    Config(ConfigArgs),

    /// Manage channels
    Channels(ChannelsArgs),

    /// Run health checks
    Doctor(DoctorArgs),

    /// Show version information
    Version,
}

/// Arguments for gateway command.
#[derive(Parser, Debug)]
pub struct GatewayArgs {
    /// Host to bind to
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port to bind to
    #[arg(short, long, default_value = "18789")]
    pub port: u16,

    /// Run in foreground (don't daemonize)
    #[arg(short, long)]
    pub foreground: bool,
}

/// Arguments for config command.
#[derive(Parser, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

/// Config subcommands.
#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Show current configuration
    Show,

    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },

    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },

    /// Initialize configuration
    Init {
        /// Force overwrite existing config
        #[arg(short, long)]
        force: bool,
    },

    /// Show configuration file path
    Path,
}

/// Arguments for channels command.
#[derive(Parser, Debug)]
pub struct ChannelsArgs {
    #[command(subcommand)]
    pub action: ChannelsAction,
}

/// Channels subcommands.
#[derive(Subcommand, Debug)]
pub enum ChannelsAction {
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

/// Arguments for doctor command.
#[derive(Parser, Debug)]
pub struct DoctorArgs {
    /// Run all checks
    #[arg(short, long)]
    pub all: bool,

    /// Fix issues automatically where possible
    #[arg(short, long)]
    pub fix: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_parse() {
        // Verify CLI can be parsed
        Cli::command().debug_assert();
    }

    #[test]
    fn test_gateway_defaults() {
        let args = GatewayArgs {
            host: "127.0.0.1".to_string(),
            port: 18789,
            foreground: false,
        };
        assert_eq!(args.host, "127.0.0.1");
        assert_eq!(args.port, 18789);
    }
}
