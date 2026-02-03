//! Solana Agent HTTP Server
//!
//! A REST API for AI agents to interact with Solana.
//!
//! # Endpoints
//!
//! - `GET  /health` - Health check
//! - `GET  /wallet/address` - Get wallet address
//! - `GET  /wallet/balance` - Get SOL balance
//! - `GET  /wallet/tokens` - Get all token balances
//! - `POST /swap/quote` - Get swap quote
//! - `POST /swap/execute` - Execute swap
//! - `GET  /price?token=SOL` - Get token price
//! - `POST /transfer/sol` - Send SOL
//! - `POST /transfer/token` - Send SPL token
//! - `POST /stake/delegate` - Stake SOL
//! - `GET  /stake/list` - List stake accounts
//! - `POST /stake/unstake` - Start unstaking
//! - `POST /stake/withdraw` - Withdraw unstaked

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use drbot_solana::{
    trading::JupiterClient,
    wallet::{BalanceChecker, FileKeypairManager, StakingManager, TransferManager},
    SolanaConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{native_token::LAMPORTS_PER_SOL, pubkey::Pubkey, signature::Signer};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

/// Solana Agent HTTP Server
#[derive(Parser)]
#[command(name = "solana-agent-server")]
#[command(about = "HTTP server for AI agents to interact with Solana")]
struct Args {
    /// Port to listen on
    #[arg(short, long, env = "SOLANA_AGENT_PORT", default_value = "3030")]
    port: u16,

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
}

/// Shared application state
struct AppState {
    rpc_client: Arc<RpcClient>,
    keypair_manager: Option<FileKeypairManager>,
    jupiter: JupiterClient,
}

/// Common token mints
fn get_known_tokens() -> HashMap<String, String> {
    let mut tokens = HashMap::new();
    tokens.insert(
        "SOL".to_string(),
        "So11111111111111111111111111111111111111112".to_string(),
    );
    tokens.insert(
        "USDC".to_string(),
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
    );
    tokens.insert(
        "USDT".to_string(),
        "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB".to_string(),
    );
    tokens.insert(
        "BONK".to_string(),
        "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263".to_string(),
    );
    tokens.insert(
        "JUP".to_string(),
        "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN".to_string(),
    );
    tokens.insert(
        "WIF".to_string(),
        "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm".to_string(),
    );
    tokens.insert(
        "PYTH".to_string(),
        "HZ1JovNiVvGrGNiiYvEozEVgZ58xaU3RKwX8eACQBCt3".to_string(),
    );
    tokens
}

fn resolve_token(token: &str) -> String {
    let tokens = get_known_tokens();
    tokens
        .get(&token.to_uppercase())
        .cloned()
        .unwrap_or_else(|| token.to_string())
}

fn default_wallet_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("solana")
        .join("id.json")
}

// Response types

#[derive(Serialize)]
struct ErrorResponse {
    success: bool,
    error: String,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    wallet: Option<String>,
    version: String,
}

// Request types

#[derive(Deserialize)]
struct SwapRequest {
    from: String,
    to: String,
    amount: u64,
    #[serde(default = "default_slippage")]
    slippage: u16,
}

fn default_slippage() -> u16 {
    50
}

#[derive(Deserialize)]
struct TransferSolRequest {
    to: String,
    amount: f64,
}

#[derive(Deserialize)]
struct TransferTokenRequest {
    to: String,
    amount: u64,
    mint: String,
}

#[derive(Deserialize)]
struct StakeRequest {
    validator: String,
    amount: f64,
}

#[derive(Deserialize)]
struct StakeAccountRequest {
    stake_account: String,
}

#[derive(Deserialize)]
struct PriceQuery {
    token: String,
}

// Error handling

fn error_response(message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "success": false,
            "error": message
        })),
    )
}

fn server_error(message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "success": false,
            "error": message
        })),
    )
}

// Handlers

async fn health(State(state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    let state = state.read().await;
    Json(HealthResponse {
        status: "ok".to_string(),
        wallet: state
            .keypair_manager
            .as_ref()
            .map(|m| m.pubkey().to_string()),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn wallet_address(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;
    let manager = state
        .keypair_manager
        .as_ref()
        .ok_or_else(|| error_response("Wallet not loaded"))?;

    Ok(Json(json!({
        "address": manager.pubkey().to_string()
    })))
}

async fn wallet_balance(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;
    let manager = state
        .keypair_manager
        .as_ref()
        .ok_or_else(|| error_response("Wallet not loaded"))?;

    let checker = BalanceChecker::new(state.rpc_client.clone());
    let balance = checker
        .get_sol_balance(&manager.pubkey())
        .await
        .map_err(|e| server_error(&e.to_string()))?;

    Ok(Json(json!({
        "address": manager.pubkey().to_string(),
        "balance": balance,
        "unit": "SOL"
    })))
}

async fn wallet_tokens(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;
    let manager = state
        .keypair_manager
        .as_ref()
        .ok_or_else(|| error_response("Wallet not loaded"))?;

    let checker = BalanceChecker::new(state.rpc_client.clone());
    let tokens = checker
        .get_all_token_balances(&manager.pubkey())
        .await
        .map_err(|e| server_error(&e.to_string()))?;

    Ok(Json(json!({
        "address": manager.pubkey().to_string(),
        "tokens": tokens
    })))
}

async fn swap_quote(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(req): Json<SwapRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;

    let input_mint = resolve_token(&req.from);
    let output_mint = resolve_token(&req.to);

    let quote = state
        .jupiter
        .get_quote(&input_mint, &output_mint, req.amount, req.slippage)
        .await
        .map_err(|e| server_error(&e.to_string()))?;

    Ok(Json(json!({
        "success": true,
        "quote": {
            "inputMint": input_mint,
            "outputMint": output_mint,
            "inputAmount": quote.in_amount,
            "outputAmount": quote.out_amount,
            "priceImpact": quote.price_impact_pct
        }
    })))
}

async fn swap_execute(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(req): Json<SwapRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;
    let manager = state
        .keypair_manager
        .as_ref()
        .ok_or_else(|| error_response("Wallet not loaded"))?;

    let input_mint = resolve_token(&req.from);
    let output_mint = resolve_token(&req.to);

    let result = state
        .jupiter
        .swap(
            state.rpc_client.clone(),
            manager.keypair(),
            &input_mint,
            &output_mint,
            req.amount,
            req.slippage,
        )
        .await
        .map_err(|e| server_error(&e.to_string()))?;

    Ok(Json(json!({
        "success": true,
        "signature": result.signature,
        "inputAmount": result.input_amount,
        "outputAmount": result.output_amount,
        "explorer": format!("https://solscan.io/tx/{}", result.signature)
    })))
}

async fn get_price(
    State(state): State<Arc<RwLock<AppState>>>,
    Query(query): Query<PriceQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;
    let mint = resolve_token(&query.token);
    let usdc = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

    let amount = if mint == "So11111111111111111111111111111111111111112" {
        LAMPORTS_PER_SOL
    } else {
        1_000_000
    };

    let quote = state
        .jupiter
        .get_quote(&mint, usdc, amount, 50)
        .await
        .map_err(|e| server_error(&e.to_string()))?;

    let price = quote.out_amount as f64 / 1_000_000.0;

    Ok(Json(json!({
        "token": query.token,
        "mint": mint,
        "price": price,
        "unit": "USDC"
    })))
}

async fn transfer_sol(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(req): Json<TransferSolRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;
    let manager = state
        .keypair_manager
        .as_ref()
        .ok_or_else(|| error_response("Wallet not loaded"))?;

    let to_pubkey =
        Pubkey::from_str(&req.to).map_err(|_| error_response("Invalid recipient address"))?;

    let transfer_manager = TransferManager::new(state.rpc_client.clone());
    let result = transfer_manager
        .send_sol(manager.keypair(), &to_pubkey, req.amount)
        .await
        .map_err(|e| server_error(&e.to_string()))?;

    Ok(Json(json!({
        "success": true,
        "signature": result.signature,
        "to": req.to,
        "amount": req.amount,
        "explorer": format!("https://solscan.io/tx/{}", result.signature)
    })))
}

async fn transfer_token(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(req): Json<TransferTokenRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;
    let manager = state
        .keypair_manager
        .as_ref()
        .ok_or_else(|| error_response("Wallet not loaded"))?;

    let to_pubkey =
        Pubkey::from_str(&req.to).map_err(|_| error_response("Invalid recipient address"))?;
    let mint = resolve_token(&req.mint);
    let mint_pubkey = Pubkey::from_str(&mint).map_err(|_| error_response("Invalid token mint"))?;

    let transfer_manager = TransferManager::new(state.rpc_client.clone());
    let result = transfer_manager
        .send_token(manager.keypair(), &to_pubkey, &mint_pubkey, req.amount)
        .await
        .map_err(|e| server_error(&e.to_string()))?;

    Ok(Json(json!({
        "success": true,
        "signature": result.signature,
        "to": req.to,
        "amount": req.amount,
        "mint": mint,
        "explorer": format!("https://solscan.io/tx/{}", result.signature)
    })))
}

async fn stake_delegate(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(req): Json<StakeRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;
    let manager = state
        .keypair_manager
        .as_ref()
        .ok_or_else(|| error_response("Wallet not loaded"))?;

    let staking = StakingManager::new(state.rpc_client.clone());
    let result = staking
        .stake(manager.keypair(), &req.validator, req.amount)
        .await
        .map_err(|e| server_error(&e.to_string()))?;

    Ok(Json(json!({
        "success": true,
        "signature": result.signature,
        "stakeAccount": result.stake_account.to_string(),
        "validator": result.validator.map(|v| v.to_string()),
        "amount": result.amount_sol,
        "explorer": result.explorer_url
    })))
}

async fn stake_list(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;
    let manager = state
        .keypair_manager
        .as_ref()
        .ok_or_else(|| error_response("Wallet not loaded"))?;

    let staking = StakingManager::new(state.rpc_client.clone());
    let accounts = staking
        .get_stake_accounts(&manager.pubkey())
        .await
        .map_err(|e| server_error(&e.to_string()))?;

    let total: f64 = accounts.iter().map(|a| a.sol).sum();

    Ok(Json(json!({
        "address": manager.pubkey().to_string(),
        "stakeAccounts": accounts,
        "totalStaked": total
    })))
}

async fn stake_unstake(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(req): Json<StakeAccountRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;
    let manager = state
        .keypair_manager
        .as_ref()
        .ok_or_else(|| error_response("Wallet not loaded"))?;

    let stake_pubkey = Pubkey::from_str(&req.stake_account)
        .map_err(|_| error_response("Invalid stake account address"))?;

    let staking = StakingManager::new(state.rpc_client.clone());
    let result = staking
        .unstake(manager.keypair(), &stake_pubkey)
        .await
        .map_err(|e| server_error(&e.to_string()))?;

    Ok(Json(json!({
        "success": true,
        "signature": result.signature,
        "stakeAccount": req.stake_account,
        "status": "deactivating",
        "note": "Stake will be withdrawable after the current epoch ends",
        "explorer": result.explorer_url
    })))
}

async fn stake_withdraw(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(req): Json<StakeAccountRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let state = state.read().await;
    let manager = state
        .keypair_manager
        .as_ref()
        .ok_or_else(|| error_response("Wallet not loaded"))?;

    let stake_pubkey = Pubkey::from_str(&req.stake_account)
        .map_err(|_| error_response("Invalid stake account address"))?;

    let staking = StakingManager::new(state.rpc_client.clone());
    let result = staking
        .withdraw(manager.keypair(), &stake_pubkey)
        .await
        .map_err(|e| server_error(&e.to_string()))?;

    Ok(Json(json!({
        "success": true,
        "signature": result.signature,
        "stakeAccount": req.stake_account,
        "withdrawn": result.amount_sol,
        "explorer": result.explorer_url
    })))
}

async fn get_tokens() -> impl IntoResponse {
    Json(get_known_tokens())
}

async fn get_validators(State(state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    let staking = StakingManager::new(state.read().await.rpc_client.clone());
    let validators: HashMap<String, String> = staking
        .known_validators()
        .all()
        .iter()
        .map(|(k, v)| (k.clone(), v.to_string()))
        .collect();
    Json(validators)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let wallet_path = args.wallet.unwrap_or_else(default_wallet_path);

    // Load wallet if it exists
    let keypair_manager = if wallet_path.exists() {
        match FileKeypairManager::from_file(&wallet_path) {
            Ok(m) => {
                println!("Wallet loaded: {}", m.pubkey());
                Some(m)
            }
            Err(e) => {
                eprintln!("Warning: Failed to load wallet: {}", e);
                None
            }
        }
    } else {
        eprintln!("Warning: Wallet not found at {}", wallet_path.display());
        eprintln!("Server will start but wallet operations will fail.");
        None
    };

    let state = Arc::new(RwLock::new(AppState {
        rpc_client: Arc::new(RpcClient::new(args.rpc_url)),
        keypair_manager,
        jupiter: JupiterClient::new(SolanaConfig::default().jupiter_api_url),
    }));

    let app = Router::new()
        .route("/health", get(health))
        .route("/wallet/address", get(wallet_address))
        .route("/wallet/balance", get(wallet_balance))
        .route("/wallet/tokens", get(wallet_tokens))
        .route("/swap/quote", post(swap_quote))
        .route("/swap/execute", post(swap_execute))
        .route("/price", get(get_price))
        .route("/transfer/sol", post(transfer_sol))
        .route("/transfer/token", post(transfer_token))
        .route("/stake/delegate", post(stake_delegate))
        .route("/stake/list", get(stake_list))
        .route("/stake/unstake", post(stake_unstake))
        .route("/stake/withdraw", post(stake_withdraw))
        .route("/tokens", get(get_tokens))
        .route("/validators", get(get_validators))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", args.port);
    println!("Solana Agent Kit server running on http://{}", addr);
    println!();
    println!("Endpoints:");
    println!("  GET  /health              Health check");
    println!("  GET  /wallet/address      Get wallet address");
    println!("  GET  /wallet/balance      Get SOL balance");
    println!("  GET  /wallet/tokens       Get all token balances");
    println!("  POST /swap/quote          Get swap quote");
    println!("  POST /swap/execute        Execute swap");
    println!("  GET  /price?token=SOL     Get token price");
    println!("  POST /transfer/sol        Send SOL");
    println!("  POST /transfer/token      Send SPL token");
    println!("  POST /stake/delegate      Stake SOL");
    println!("  GET  /stake/list          List stake accounts");
    println!("  POST /stake/unstake       Start unstaking");
    println!("  POST /stake/withdraw      Withdraw unstaked");
    println!("  GET  /tokens              List known tokens");
    println!("  GET  /validators          List known validators");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
