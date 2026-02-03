//! Solana Agent CLI
//!
//! A command-line interface for AI agents to interact with Solana.
//! All commands output JSON for easy parsing.
//!
//! # Usage
//!
//! ```bash
//! solana-agent wallet create
//! solana-agent wallet balance
//! solana-agent swap quote SOL USDC 1000000000
//! solana-agent transfer <recipient> 0.1
//! solana-agent stake delegate jito 1.0
//! ```

use clap::{Parser, Subcommand};
use drbot_solana::{
    trading::JupiterClient,
    wallet::{BalanceChecker, FileKeypairManager, StakingManager, TransferManager},
    SolanaConfig,
};
use serde_json::json;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{native_token::LAMPORTS_PER_SOL, pubkey::Pubkey, signature::Signer};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

/// Solana Agent Kit - CLI for AI agents to interact with Solana
#[derive(Parser)]
#[command(name = "solana-agent")]
#[command(author = "drbot contributors")]
#[command(version)]
#[command(about = "CLI for AI agents to interact with Solana", long_about = None)]
struct Cli {
    /// Path to wallet JSON file
    #[arg(long, env = "SOLANA_WALLET_PATH")]
    wallet: Option<PathBuf>,

    /// Solana RPC URL
    #[arg(
        long,
        env = "SOLANA_RPC_URL",
        default_value = "https://api.mainnet-beta.solana.com"
    )]
    rpc_url: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Wallet operations
    Wallet {
        #[command(subcommand)]
        action: WalletCommands,
    },
    /// Token swap operations via Jupiter
    Swap {
        #[command(subcommand)]
        action: SwapCommands,
    },
    /// Get token price in USDC
    Price {
        /// Token symbol or mint address
        token: String,
    },
    /// Transfer SOL or tokens
    Transfer {
        /// Recipient address
        to: String,
        /// Amount to send (in SOL or token units)
        amount: f64,
        /// Token mint address (omit for SOL)
        #[arg(long)]
        token: Option<String>,
    },
    /// Native SOL staking operations
    Stake {
        #[command(subcommand)]
        action: StakeCommands,
    },
    /// List known token symbols
    Tokens,
}

#[derive(Subcommand)]
enum WalletCommands {
    /// Create a new wallet
    Create {
        /// Path to save wallet (default: ~/.config/solana/id.json)
        path: Option<PathBuf>,
    },
    /// Get SOL balance
    Balance,
    /// Get all token balances
    Tokens,
    /// Print wallet address
    Address,
}

#[derive(Subcommand)]
enum SwapCommands {
    /// Get a swap quote
    Quote {
        /// Input token (SOL, USDC, or mint address)
        from: String,
        /// Output token (SOL, USDC, or mint address)
        to: String,
        /// Amount in smallest units (lamports for SOL)
        amount: u64,
        /// Slippage in basis points (default: 50 = 0.5%)
        #[arg(long, default_value = "50")]
        slippage: u16,
    },
    /// Execute a swap
    Execute {
        /// Input token
        from: String,
        /// Output token
        to: String,
        /// Amount in smallest units
        amount: u64,
        /// Slippage in basis points
        #[arg(long, default_value = "50")]
        slippage: u16,
    },
}

#[derive(Subcommand)]
enum StakeCommands {
    /// Delegate SOL to a validator
    Delegate {
        /// Validator name (jito, marinade, etc.) or vote account address
        validator: String,
        /// Amount of SOL to stake
        amount: f64,
    },
    /// List stake accounts
    List,
    /// Deactivate a stake account (start unstaking)
    Unstake {
        /// Stake account address
        stake_account: String,
    },
    /// Withdraw from a deactivated stake account
    Withdraw {
        /// Stake account address
        stake_account: String,
    },
    /// List known validators
    Validators,
}

/// Common token mints
fn get_known_tokens() -> serde_json::Value {
    json!({
        "SOL": "So11111111111111111111111111111111111111112",
        "USDC": "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
        "USDT": "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
        "BONK": "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263",
        "JUP": "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN",
        "WIF": "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm",
        "PYTH": "HZ1JovNiVvGrGNiiYvEozEVgZ58xaU3RKwX8eACQBCt3",
        "RAY": "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R",
        "ORCA": "orcaEKTdK7LKz57vaAYr9QeNsVEPfiu6QeMU1kektZE"
    })
}

/// Resolve token symbol to mint address
fn resolve_token(token: &str) -> String {
    let tokens = get_known_tokens();
    if let Some(mint) = tokens.get(token.to_uppercase().as_str()) {
        mint.as_str().unwrap().to_string()
    } else {
        token.to_string()
    }
}

fn default_wallet_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("solana")
        .join("id.json")
}

fn output_json(value: serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(&value).unwrap());
}

fn output_error(message: &str) {
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "success": false,
            "error": message
        }))
        .unwrap()
    );
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let wallet_path = cli.wallet.unwrap_or_else(default_wallet_path);
    let rpc_client = Arc::new(RpcClient::new(cli.rpc_url.clone()));

    match cli.command {
        Commands::Wallet { action } => match action {
            WalletCommands::Create { path } => {
                let save_path = path.unwrap_or_else(default_wallet_path);

                // Create parent directories if needed
                if let Some(parent) = save_path.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        output_error(&format!("Failed to create directory: {}", e));
                    }
                }

                let keypair = solana_sdk::signature::Keypair::new();
                let secret_key = keypair.to_bytes();

                // Save as JSON array (solana-keygen format)
                let json_data = serde_json::to_string(&secret_key.to_vec()).unwrap();
                if let Err(e) = std::fs::write(&save_path, &json_data) {
                    output_error(&format!("Failed to save wallet: {}", e));
                }

                // Set file permissions to 600 on Unix
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(
                        &save_path,
                        std::fs::Permissions::from_mode(0o600),
                    );
                }

                output_json(json!({
                    "success": true,
                    "address": keypair.pubkey().to_string(),
                    "path": save_path.to_string_lossy(),
                    "warning": "Backup your wallet file! Loss means loss of funds."
                }));
            }

            WalletCommands::Balance => {
                if !wallet_path.exists() {
                    output_error(&format!(
                        "Wallet not found at {}. Create one with: solana-agent wallet create",
                        wallet_path.display()
                    ));
                }

                let manager = match FileKeypairManager::from_file(&wallet_path) {
                    Ok(m) => m,
                    Err(e) => {
                        output_error(&format!("Failed to load wallet: {}", e));
                        return;
                    }
                };

                let checker = BalanceChecker::new(rpc_client);
                match checker.get_sol_balance(&manager.pubkey()).await {
                    Ok(balance) => {
                        output_json(json!({
                            "address": manager.pubkey().to_string(),
                            "balance": balance,
                            "unit": "SOL"
                        }));
                    }
                    Err(e) => output_error(&format!("Failed to get balance: {}", e)),
                }
            }

            WalletCommands::Tokens => {
                if !wallet_path.exists() {
                    output_error(&format!("Wallet not found at {}", wallet_path.display()));
                }

                let manager = match FileKeypairManager::from_file(&wallet_path) {
                    Ok(m) => m,
                    Err(e) => {
                        output_error(&format!("Failed to load wallet: {}", e));
                        return;
                    }
                };

                let checker = BalanceChecker::new(rpc_client);
                match checker.get_all_token_balances(&manager.pubkey()).await {
                    Ok(tokens) => {
                        output_json(json!({
                            "address": manager.pubkey().to_string(),
                            "tokens": tokens
                        }));
                    }
                    Err(e) => output_error(&format!("Failed to get token balances: {}", e)),
                }
            }

            WalletCommands::Address => {
                if !wallet_path.exists() {
                    output_error(&format!("Wallet not found at {}", wallet_path.display()));
                }

                let manager = match FileKeypairManager::from_file(&wallet_path) {
                    Ok(m) => m,
                    Err(e) => {
                        output_error(&format!("Failed to load wallet: {}", e));
                        return;
                    }
                };

                println!("{}", manager.pubkey());
            }
        },

        Commands::Swap { action } => {
            let jupiter = JupiterClient::new(SolanaConfig::default().jupiter_api_url);

            match action {
                SwapCommands::Quote {
                    from,
                    to,
                    amount,
                    slippage,
                } => {
                    let input_mint = resolve_token(&from);
                    let output_mint = resolve_token(&to);

                    match jupiter
                        .get_quote(&input_mint, &output_mint, amount, slippage)
                        .await
                    {
                        Ok(quote) => {
                            output_json(json!({
                                "success": true,
                                "from": from,
                                "to": to,
                                "inputMint": input_mint,
                                "outputMint": output_mint,
                                "inputAmount": quote.in_amount,
                                "outputAmount": quote.out_amount,
                                "priceImpact": quote.price_impact_pct
                            }));
                        }
                        Err(e) => output_error(&format!("Quote failed: {}", e)),
                    }
                }

                SwapCommands::Execute {
                    from,
                    to,
                    amount,
                    slippage,
                } => {
                    if !wallet_path.exists() {
                        output_error(&format!("Wallet not found at {}", wallet_path.display()));
                    }

                    let manager = match FileKeypairManager::from_file(&wallet_path) {
                        Ok(m) => m,
                        Err(e) => {
                            output_error(&format!("Failed to load wallet: {}", e));
                            return;
                        }
                    };

                    let input_mint = resolve_token(&from);
                    let output_mint = resolve_token(&to);

                    eprintln!("Swapping {} {} -> {}...", amount, from, to);

                    match jupiter
                        .swap(
                            rpc_client.clone(),
                            manager.keypair(),
                            &input_mint,
                            &output_mint,
                            amount,
                            slippage,
                        )
                        .await
                    {
                        Ok(result) => {
                            output_json(json!({
                                "success": true,
                                "signature": result.signature,
                                "inputAmount": result.input_amount,
                                "outputAmount": result.output_amount,
                                "explorer": format!("https://solscan.io/tx/{}", result.signature)
                            }));
                        }
                        Err(e) => output_error(&format!("Swap failed: {}", e)),
                    }
                }
            }
        }

        Commands::Price { token } => {
            let jupiter = JupiterClient::new(SolanaConfig::default().jupiter_api_url);
            let mint = resolve_token(&token);
            let usdc = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

            // Get quote for 1 token worth
            let amount = if mint == "So11111111111111111111111111111111111111112" {
                LAMPORTS_PER_SOL
            } else {
                1_000_000 // Assume 6 decimals for most tokens
            };

            match jupiter.get_quote(&mint, usdc, amount, 50).await {
                Ok(quote) => {
                    let price = quote.out_amount as f64 / 1_000_000.0; // USDC has 6 decimals
                    output_json(json!({
                        "token": token,
                        "mint": mint,
                        "price": price,
                        "unit": "USDC"
                    }));
                }
                Err(e) => output_error(&format!("Failed to get price: {}", e)),
            }
        }

        Commands::Transfer { to, amount, token } => {
            if !wallet_path.exists() {
                output_error(&format!("Wallet not found at {}", wallet_path.display()));
            }

            let manager = match FileKeypairManager::from_file(&wallet_path) {
                Ok(m) => m,
                Err(e) => {
                    output_error(&format!("Failed to load wallet: {}", e));
                    return;
                }
            };

            let to_pubkey = match Pubkey::from_str(&to) {
                Ok(p) => p,
                Err(_) => {
                    output_error("Invalid recipient address");
                    return;
                }
            };

            let transfer_manager = TransferManager::new(rpc_client);

            match token {
                Some(mint_str) => {
                    let mint = resolve_token(&mint_str);
                    let mint_pubkey = match Pubkey::from_str(&mint) {
                        Ok(p) => p,
                        Err(_) => {
                            output_error("Invalid token mint address");
                            return;
                        }
                    };

                    eprintln!("Transferring {} {} to {}...", amount, mint_str, to);

                    match transfer_manager
                        .send_token(manager.keypair(), &to_pubkey, &mint_pubkey, amount as u64)
                        .await
                    {
                        Ok(result) => {
                            output_json(json!({
                                "success": true,
                                "signature": result.signature,
                                "to": to,
                                "amount": amount,
                                "token": mint,
                                "explorer": format!("https://solscan.io/tx/{}", result.signature)
                            }));
                        }
                        Err(e) => output_error(&format!("Transfer failed: {}", e)),
                    }
                }
                None => {
                    eprintln!("Transferring {} SOL to {}...", amount, to);

                    match transfer_manager
                        .send_sol(manager.keypair(), &to_pubkey, amount)
                        .await
                    {
                        Ok(result) => {
                            output_json(json!({
                                "success": true,
                                "signature": result.signature,
                                "to": to,
                                "amount": amount,
                                "token": "SOL",
                                "explorer": format!("https://solscan.io/tx/{}", result.signature)
                            }));
                        }
                        Err(e) => output_error(&format!("Transfer failed: {}", e)),
                    }
                }
            }
        }

        Commands::Stake { action } => {
            let staking = StakingManager::new(rpc_client.clone());

            match action {
                StakeCommands::Delegate { validator, amount } => {
                    if !wallet_path.exists() {
                        output_error(&format!("Wallet not found at {}", wallet_path.display()));
                    }

                    let manager = match FileKeypairManager::from_file(&wallet_path) {
                        Ok(m) => m,
                        Err(e) => {
                            output_error(&format!("Failed to load wallet: {}", e));
                            return;
                        }
                    };

                    eprintln!("Staking {} SOL to {}...", amount, validator);

                    match staking.stake(manager.keypair(), &validator, amount).await {
                        Ok(result) => {
                            output_json(json!({
                                "success": true,
                                "signature": result.signature,
                                "stakeAccount": result.stake_account.to_string(),
                                "validator": result.validator.map(|v| v.to_string()),
                                "amount": result.amount_sol,
                                "explorer": result.explorer_url
                            }));
                        }
                        Err(e) => output_error(&format!("Staking failed: {}", e)),
                    }
                }

                StakeCommands::List => {
                    if !wallet_path.exists() {
                        output_error(&format!("Wallet not found at {}", wallet_path.display()));
                    }

                    let manager = match FileKeypairManager::from_file(&wallet_path) {
                        Ok(m) => m,
                        Err(e) => {
                            output_error(&format!("Failed to load wallet: {}", e));
                            return;
                        }
                    };

                    match staking.get_stake_accounts(&manager.pubkey()).await {
                        Ok(accounts) => {
                            let total: f64 = accounts.iter().map(|a| a.sol).sum();
                            output_json(json!({
                                "address": manager.pubkey().to_string(),
                                "stakeAccounts": accounts,
                                "totalStaked": total
                            }));
                        }
                        Err(e) => output_error(&format!("Failed to list stake accounts: {}", e)),
                    }
                }

                StakeCommands::Unstake { stake_account } => {
                    if !wallet_path.exists() {
                        output_error(&format!("Wallet not found at {}", wallet_path.display()));
                    }

                    let manager = match FileKeypairManager::from_file(&wallet_path) {
                        Ok(m) => m,
                        Err(e) => {
                            output_error(&format!("Failed to load wallet: {}", e));
                            return;
                        }
                    };

                    let stake_pubkey = match Pubkey::from_str(&stake_account) {
                        Ok(p) => p,
                        Err(_) => {
                            output_error("Invalid stake account address");
                            return;
                        }
                    };

                    eprintln!("Deactivating stake account {}...", stake_account);

                    match staking.unstake(manager.keypair(), &stake_pubkey).await {
                        Ok(result) => {
                            output_json(json!({
                                "success": true,
                                "signature": result.signature,
                                "stakeAccount": stake_account,
                                "status": "deactivating",
                                "note": "Stake will be withdrawable after the current epoch ends",
                                "explorer": result.explorer_url
                            }));
                        }
                        Err(e) => output_error(&format!("Unstake failed: {}", e)),
                    }
                }

                StakeCommands::Withdraw { stake_account } => {
                    if !wallet_path.exists() {
                        output_error(&format!("Wallet not found at {}", wallet_path.display()));
                    }

                    let manager = match FileKeypairManager::from_file(&wallet_path) {
                        Ok(m) => m,
                        Err(e) => {
                            output_error(&format!("Failed to load wallet: {}", e));
                            return;
                        }
                    };

                    let stake_pubkey = match Pubkey::from_str(&stake_account) {
                        Ok(p) => p,
                        Err(_) => {
                            output_error("Invalid stake account address");
                            return;
                        }
                    };

                    eprintln!("Withdrawing from {}...", stake_account);

                    match staking.withdraw(manager.keypair(), &stake_pubkey).await {
                        Ok(result) => {
                            output_json(json!({
                                "success": true,
                                "signature": result.signature,
                                "stakeAccount": stake_account,
                                "withdrawn": result.amount_sol,
                                "explorer": result.explorer_url
                            }));
                        }
                        Err(e) => output_error(&format!("Withdraw failed: {}", e)),
                    }
                }

                StakeCommands::Validators => {
                    let validators = staking.known_validators();
                    output_json(json!(validators
                        .all()
                        .iter()
                        .map(|(k, v)| (k.clone(), v.to_string()))
                        .collect::<std::collections::HashMap<_, _>>()));
                }
            }
        }

        Commands::Tokens => {
            output_json(get_known_tokens());
        }
    }
}
