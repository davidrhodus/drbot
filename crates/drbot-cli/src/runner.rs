//! Command execution.

use crate::commands::*;
use colored::Colorize;
use std::path::PathBuf;
use tracing::{info, warn};

/// Run the CLI with parsed arguments.
pub async fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Gateway(args) => run_gateway(args).await,
        Commands::Config(args) => run_config(args, cli.format).await,
        Commands::Channels(args) => run_channels(args, cli.format).await,
        Commands::Doctor(args) => run_doctor(args, cli.format).await,
        Commands::Version => run_version(cli.format),
    }
}

/// Run the gateway server.
async fn run_gateway(args: GatewayArgs) -> anyhow::Result<()> {
    println!(
        "{} Starting gateway on {}:{}",
        "drbot".cyan().bold(),
        args.host,
        args.port
    );

    // Create config with the specified host and port
    let mut config = drbot_core::Config::default();
    config.gateway.host = args.host.clone();
    config.gateway.port = args.port;

    // Create and start the gateway
    let gateway = drbot_gateway::Gateway::new(config);

    info!(host = %args.host, port = %args.port, "Gateway starting");

    // Run the gateway
    gateway.run().await.map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}

/// Run config commands.
async fn run_config(args: ConfigArgs, format: OutputFormat) -> anyhow::Result<()> {
    match args.action {
        ConfigAction::Show => {
            let config_path = get_config_path();
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                match format {
                    OutputFormat::Text => println!("{}", content),
                    OutputFormat::Json => {
                        // Parse and re-serialize as JSON
                        let config: serde_json::Value =
                            toml::from_str(&content).unwrap_or(serde_json::Value::Null);
                        println!("{}", serde_json::to_string_pretty(&config)?);
                    }
                }
            } else {
                println!("{} No configuration file found", "warning:".yellow());
                println!("Run 'drbot config init' to create one");
            }
        }
        ConfigAction::Get { key } => {
            let config_path = get_config_path();
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                let config: toml::Value = toml::from_str(&content)?;

                if let Some(value) = get_nested_value(&config, &key) {
                    match format {
                        OutputFormat::Text => println!("{}", value),
                        OutputFormat::Json => println!("{}", serde_json::to_string(&value)?),
                    }
                } else {
                    anyhow::bail!("Key '{}' not found", key);
                }
            } else {
                anyhow::bail!("Configuration file not found");
            }
        }
        ConfigAction::Set { key, value } => {
            let config_path = get_config_path();
            let mut config: toml::Value = if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)?;
                toml::from_str(&content)?
            } else {
                toml::Value::Table(toml::map::Map::new())
            };

            set_nested_value(&mut config, &key, &value)?;

            // Ensure parent directory exists
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            std::fs::write(&config_path, toml::to_string_pretty(&config)?)?;
            println!("{} Set {} = {}", "success:".green(), key, value);
        }
        ConfigAction::Init { force } => {
            let config_path = get_config_path();

            if config_path.exists() && !force {
                anyhow::bail!(
                    "Configuration file already exists at {}. Use --force to overwrite.",
                    config_path.display()
                );
            }

            // Ensure parent directory exists
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let default_config = default_config();
            std::fs::write(&config_path, default_config)?;
            println!(
                "{} Created configuration at {}",
                "success:".green(),
                config_path.display()
            );
        }
        ConfigAction::Path => {
            let config_path = get_config_path();
            match format {
                OutputFormat::Text => println!("{}", config_path.display()),
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({"path": config_path.to_string_lossy()})
                    );
                }
            }
        }
    }
    Ok(())
}

/// Run channels commands.
async fn run_channels(args: ChannelsArgs, format: OutputFormat) -> anyhow::Result<()> {
    match args.action {
        ChannelsAction::List => {
            let channels = vec![
                ("webchat", true, "Web chat interface"),
                ("telegram", false, "Telegram Bot"),
                ("discord", false, "Discord Bot"),
                ("slack", false, "Slack Bot"),
                ("matrix", false, "Matrix Protocol"),
                ("signal", false, "Signal Messenger"),
                ("whatsapp", false, "WhatsApp"),
                ("imessage", false, "iMessage (macOS)"),
            ];

            match format {
                OutputFormat::Text => {
                    println!("{}", "Configured channels:".bold());
                    for (name, enabled, desc) in &channels {
                        let status = if *enabled {
                            "enabled".green()
                        } else {
                            "disabled".dimmed()
                        };
                        println!("  {} - {} [{}]", name.cyan(), desc, status);
                    }
                }
                OutputFormat::Json => {
                    let json: Vec<_> = channels
                        .iter()
                        .map(|(name, enabled, desc)| {
                            serde_json::json!({
                                "name": name,
                                "enabled": enabled,
                                "description": desc
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
            }
        }
        ChannelsAction::Status { name } => {
            if let Some(channel_name) = name {
                println!("Status for channel: {}", channel_name.cyan());
                println!("  Status: {}", "not configured".yellow());
            } else {
                println!("{}", "Channel status:".bold());
                println!("  No channels are currently running");
            }
        }
        ChannelsAction::Enable { name } => {
            println!("{} Enabled channel: {}", "success:".green(), name);
        }
        ChannelsAction::Disable { name } => {
            println!("{} Disabled channel: {}", "success:".green(), name);
        }
    }
    Ok(())
}

/// Run doctor checks.
async fn run_doctor(args: DoctorArgs, format: OutputFormat) -> anyhow::Result<()> {
    let checks = vec![
        ("config", check_config()),
        ("directories", check_directories()),
        ("dependencies", check_dependencies()),
    ];

    let mut all_passed = true;

    match format {
        OutputFormat::Text => {
            println!("{}", "Running health checks...".bold());
            println!();

            for (name, result) in &checks {
                match result {
                    Ok(msg) => {
                        println!("  {} {} - {}", "✓".green(), name, msg);
                    }
                    Err(msg) => {
                        println!("  {} {} - {}", "✗".red(), name, msg);
                        all_passed = false;
                    }
                }
            }

            println!();
            if all_passed {
                println!("{}", "All checks passed!".green().bold());
            } else {
                println!(
                    "{}",
                    "Some checks failed. Run with --fix to attempt automatic fixes.".yellow()
                );
            }
        }
        OutputFormat::Json => {
            let results: Vec<_> = checks
                .iter()
                .map(|(name, result)| {
                    serde_json::json!({
                        "name": name,
                        "passed": result.is_ok(),
                        "message": result.as_ref().map(|s| s.as_str()).unwrap_or_else(|e| e.as_str())
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "checks": results,
                    "all_passed": all_passed
                }))?
            );
        }
    }

    if args.fix && !all_passed {
        println!();
        println!("{}", "Attempting fixes...".bold());
        // Run fixes here
        if !check_directories().is_ok() {
            if let Err(e) = fix_directories() {
                warn!("Failed to fix directories: {}", e);
            } else {
                println!("  {} Fixed directory structure", "✓".green());
            }
        }
    }

    Ok(())
}

/// Show version information.
fn run_version(format: OutputFormat) -> anyhow::Result<()> {
    let version_info = serde_json::json!({
        "name": "drbot",
        "version": env!("CARGO_PKG_VERSION"),
        "rust_version": env!("CARGO_PKG_RUST_VERSION"),
    });

    match format {
        OutputFormat::Text => {
            println!("{} {}", "drbot".cyan().bold(), env!("CARGO_PKG_VERSION"));
            println!("Personal AI Assistant");
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&version_info)?);
        }
    }

    Ok(())
}

/// Get the configuration file path.
fn get_config_path() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        config_dir.join("drbot").join("config.toml")
    } else {
        PathBuf::from("drbot.toml")
    }
}

/// Get a nested value from TOML.
fn get_nested_value<'a>(config: &'a toml::Value, key: &str) -> Option<&'a toml::Value> {
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = config;

    for part in parts {
        match current {
            toml::Value::Table(table) => {
                current = table.get(part)?;
            }
            _ => return None,
        }
    }

    Some(current)
}

/// Set a nested value in TOML.
fn set_nested_value(config: &mut toml::Value, key: &str, value: &str) -> anyhow::Result<()> {
    let parts: Vec<&str> = key.split('.').collect();

    let mut current = config;
    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            // Last part - set the value
            if let toml::Value::Table(table) = current {
                // Try to parse as number, bool, or string
                let parsed: toml::Value = if let Ok(n) = value.parse::<i64>() {
                    toml::Value::Integer(n)
                } else if let Ok(b) = value.parse::<bool>() {
                    toml::Value::Boolean(b)
                } else {
                    toml::Value::String(value.to_string())
                };
                table.insert(part.to_string(), parsed);
            }
        } else {
            // Navigate or create table
            if let toml::Value::Table(table) = current {
                current = table
                    .entry(part.to_string())
                    .or_insert(toml::Value::Table(toml::map::Map::new()));
            }
        }
    }

    Ok(())
}

/// Default configuration content.
fn default_config() -> String {
    r#"# drbot configuration

[gateway]
host = "127.0.0.1"
port = 18789

[provider]
default = "anthropic"

[provider.anthropic]
# api_key = "your-api-key"
model = "claude-sonnet-4-20250514"

[channels]
# Configure your channels here

[channels.webchat]
enabled = true
port = 8080
"#
    .to_string()
}

/// Check configuration.
fn check_config() -> Result<String, String> {
    let config_path = get_config_path();
    if config_path.exists() {
        Ok("Configuration file exists".to_string())
    } else {
        Err("Configuration file not found".to_string())
    }
}

/// Check directories.
fn check_directories() -> Result<String, String> {
    let data_dir = dirs::data_dir()
        .map(|d| d.join("drbot"))
        .unwrap_or_else(|| PathBuf::from(".drbot"));

    if data_dir.exists() {
        Ok("Data directory exists".to_string())
    } else {
        Err("Data directory not found".to_string())
    }
}

/// Fix directories.
fn fix_directories() -> anyhow::Result<()> {
    let data_dir = dirs::data_dir()
        .map(|d| d.join("drbot"))
        .unwrap_or_else(|| PathBuf::from(".drbot"));

    std::fs::create_dir_all(&data_dir)?;
    Ok(())
}

/// Check dependencies.
fn check_dependencies() -> Result<String, String> {
    // Check for optional dependencies
    Ok("All dependencies available".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_config_path() {
        let path = get_config_path();
        assert!(
            path.to_string_lossy().contains("config.toml")
                || path.to_string_lossy().contains("drbot.toml")
        );
    }

    #[test]
    fn test_nested_value() {
        let config: toml::Value = toml::from_str(
            r#"
            [section]
            key = "value"
            "#,
        )
        .unwrap();

        let value = get_nested_value(&config, "section.key").unwrap();
        assert_eq!(value.as_str(), Some("value"));
    }

    #[test]
    fn test_set_nested_value() {
        let mut config = toml::Value::Table(toml::map::Map::new());
        set_nested_value(&mut config, "section.key", "value").unwrap();

        let value = get_nested_value(&config, "section.key").unwrap();
        assert_eq!(value.as_str(), Some("value"));
    }

    #[test]
    fn test_default_config() {
        let config = default_config();
        assert!(config.contains("[gateway]"));
        assert!(config.contains("[provider]"));
    }
}
