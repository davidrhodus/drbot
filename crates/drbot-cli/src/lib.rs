//! CLI interface for drbot.
//!
//! This crate provides the command-line interface for drbot.
//!
//! # Commands
//!
//! - `drbot gateway` - Start the WebSocket gateway server
//! - `drbot config` - Manage configuration
//! - `drbot channels` - Manage messaging channels
//! - `drbot doctor` - Run health checks
//! - `drbot version` - Show version information
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_cli::{Cli, run};
//! use clap::Parser;
//!
//! #[tokio::main]
//! async fn main() {
//!     let cli = Cli::parse();
//!     if let Err(e) = run(cli).await {
//!         eprintln!("Error: {}", e);
//!         std::process::exit(1);
//!     }
//! }
//! ```

pub mod commands;
pub mod runner;

pub use commands::{Cli, Commands, OutputFormat};
pub use runner::run;

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_version() {
        // Test that version command parses
        let result = Cli::try_parse_from(["drbot", "version"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cli_gateway() {
        let result = Cli::try_parse_from(["drbot", "gateway", "--port", "9000"]);
        assert!(result.is_ok());
        if let Ok(cli) = result {
            if let Commands::Gateway(args) = cli.command {
                assert_eq!(args.port, 9000);
            }
        }
    }

    #[test]
    fn test_cli_config_path() {
        let result = Cli::try_parse_from(["drbot", "config", "path"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_cli_doctor() {
        let result = Cli::try_parse_from(["drbot", "doctor", "--all"]);
        assert!(result.is_ok());
    }
}
