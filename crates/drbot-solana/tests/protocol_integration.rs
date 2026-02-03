//! Integration tests for DeFi protocol modules.

use drbot_solana::protocols::{
    drift::{DriftClient, DriftMarket, OpenPositionParams, OrderSide, OrderType, DRIFT_PROGRAM_ID},
    meteora::{LiquidityStrategy, MeteoraClient, MeteoraPool, METEORA_DLMM_PROGRAM_ID},
    nft::{
        Marketplace, NftClient, NftCollection, NftListing, MAGIC_EDEN_PROGRAM_ID,
        TENSOR_SWAP_PROGRAM_ID,
    },
    pyth::{PriceData, PythClient, PythFeedIds},
    raydium::{RaydiumClient, RaydiumPool, RAYDIUM_AMM_PROGRAM_ID, RAYDIUM_CLMM_PROGRAM_ID},
};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

// ===== Pyth Tests =====

#[test]
fn test_pyth_feed_ids_sol() {
    let feed = PythFeedIds::get("SOL");
    assert!(feed.is_some());
    assert_eq!(feed.unwrap(), PythFeedIds::SOL_USD);
}

#[test]
fn test_pyth_feed_ids_case_insensitive() {
    assert_eq!(PythFeedIds::get("sol"), PythFeedIds::get("SOL"));
    assert_eq!(PythFeedIds::get("btc"), PythFeedIds::get("BTC"));
    assert_eq!(PythFeedIds::get("eth"), PythFeedIds::get("ETH"));
}

#[test]
fn test_pyth_feed_ids_all() {
    let all = PythFeedIds::all();
    assert!(all.contains_key("SOL"));
    assert!(all.contains_key("BTC"));
    assert!(all.contains_key("ETH"));
    assert!(all.contains_key("USDC"));
    assert!(all.contains_key("BONK"));
    assert!(all.contains_key("JUP"));
}

#[test]
fn test_pyth_supported_symbols() {
    let symbols = PythClient::supported_symbols();
    assert!(symbols.contains(&"SOL"));
    assert!(symbols.contains(&"BTC"));
    assert!(symbols.contains(&"ETH"));
}

#[test]
fn test_pyth_client_default() {
    let client = PythClient::default();
    // Just verify it can be created
    assert!(true);
    drop(client);
}

#[test]
fn test_price_data_serialization() {
    let price_data = PriceData {
        symbol: "SOL".to_string(),
        price: 100.50,
        confidence: 0.05,
        timestamp: 1704067200,
        exponent: -8,
    };

    let json = serde_json::to_string(&price_data).unwrap();
    assert!(json.contains("\"symbol\":\"SOL\""));
    assert!(json.contains("\"price\":100.5"));
}

// ===== Drift Tests =====

#[test]
fn test_drift_program_id_valid() {
    let pubkey: Pubkey = DRIFT_PROGRAM_ID.parse().unwrap();
    assert_eq!(pubkey.to_string(), DRIFT_PROGRAM_ID);
}

#[test]
fn test_drift_order_side_display() {
    assert_eq!(OrderSide::Long.to_string(), "long");
    assert_eq!(OrderSide::Short.to_string(), "short");
}

#[test]
fn test_drift_order_type_serialization() {
    let order_type = OrderType::Market;
    let json = serde_json::to_string(&order_type).unwrap();
    assert_eq!(json, "\"market\"");

    let limit = OrderType::Limit;
    let json = serde_json::to_string(&limit).unwrap();
    assert_eq!(json, "\"limit\"");
}

#[test]
fn test_drift_open_position_params_default() {
    let params = OpenPositionParams::default();
    assert_eq!(params.leverage, 1.0);
    assert_eq!(params.side, OrderSide::Long);
    assert_eq!(params.order_type, OrderType::Market);
    assert!(params.limit_price.is_none());
    assert!(!params.reduce_only);
}

#[test]
fn test_drift_available_markets() {
    let markets = DriftClient::available_markets();
    assert!(markets.contains(&"SOL-PERP"));
    assert!(markets.contains(&"BTC-PERP"));
    assert!(markets.contains(&"ETH-PERP"));
}

#[test]
fn test_drift_market_serialization() {
    let market = DriftMarket {
        symbol: "SOL-PERP".to_string(),
        market_index: 0,
        mark_price: 100.0,
        volume_24h: 1000000.0,
        open_interest: 50000.0,
        funding_rate: 0.0001,
    };

    let json = serde_json::to_string(&market).unwrap();
    assert!(json.contains("\"symbol\":\"SOL-PERP\""));
    assert!(json.contains("\"market_index\":0"));
}

// ===== Raydium Tests =====

#[test]
fn test_raydium_amm_program_id_valid() {
    let pubkey: Pubkey = RAYDIUM_AMM_PROGRAM_ID.parse().unwrap();
    assert_eq!(pubkey.to_string(), RAYDIUM_AMM_PROGRAM_ID);
}

#[test]
fn test_raydium_clmm_program_id_valid() {
    let pubkey: Pubkey = RAYDIUM_CLMM_PROGRAM_ID.parse().unwrap();
    assert_eq!(pubkey.to_string(), RAYDIUM_CLMM_PROGRAM_ID);
}

#[test]
fn test_raydium_pool_serialization() {
    let pool = RaydiumPool {
        id: "test_pool_id".to_string(),
        name: "SOL-USDC".to_string(),
        token_a: "So11111111111111111111111111111111111111112".to_string(),
        token_a_symbol: "SOL".to_string(),
        token_b: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
        token_b_symbol: "USDC".to_string(),
        tvl: 5000000.0,
        apy: 15.5,
        volume_24h: 1000000.0,
        fee_rate: 0.25,
    };

    let json = serde_json::to_string(&pool).unwrap();
    assert!(json.contains("\"name\":\"SOL-USDC\""));
    assert!(json.contains("\"tvl\":5000000"));
}

// ===== Meteora Tests =====

#[test]
fn test_meteora_program_id_valid() {
    let pubkey: Pubkey = METEORA_DLMM_PROGRAM_ID.parse().unwrap();
    assert_eq!(pubkey.to_string(), METEORA_DLMM_PROGRAM_ID);
}

#[test]
fn test_meteora_liquidity_strategy_default() {
    let strategy = LiquidityStrategy::default();
    assert_eq!(strategy, LiquidityStrategy::Uniform);
}

#[test]
fn test_meteora_liquidity_strategy_serialization() {
    let uniform = LiquidityStrategy::Uniform;
    let json = serde_json::to_string(&uniform).unwrap();
    assert_eq!(json, "\"uniform\"");

    let spot = LiquidityStrategy::Spot;
    let json = serde_json::to_string(&spot).unwrap();
    assert_eq!(json, "\"spot\"");
}

#[test]
fn test_meteora_pool_serialization() {
    let pool = MeteoraPool {
        address: "test_address".to_string(),
        name: "SOL-USDC".to_string(),
        token_x: "So11111111111111111111111111111111111111112".to_string(),
        token_x_symbol: "SOL".to_string(),
        token_y: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
        token_y_symbol: "USDC".to_string(),
        tvl: 2000000.0,
        apy: 25.0,
        fee_rate: 0.25,
        bin_step: 10,
        active_bin_id: 8388608,
    };

    let json = serde_json::to_string(&pool).unwrap();
    assert!(json.contains("\"name\":\"SOL-USDC\""));
    assert!(json.contains("\"bin_step\":10"));
}

// ===== NFT Tests =====

#[test]
fn test_tensor_program_id_valid() {
    let pubkey: Pubkey = TENSOR_SWAP_PROGRAM_ID.parse().unwrap();
    assert_eq!(pubkey.to_string(), TENSOR_SWAP_PROGRAM_ID);
}

#[test]
fn test_magic_eden_program_id_valid() {
    let pubkey: Pubkey = MAGIC_EDEN_PROGRAM_ID.parse().unwrap();
    assert_eq!(pubkey.to_string(), MAGIC_EDEN_PROGRAM_ID);
}

#[test]
fn test_marketplace_display() {
    assert_eq!(Marketplace::Tensor.to_string(), "Tensor");
    assert_eq!(Marketplace::MagicEden.to_string(), "Magic Eden");
}

#[test]
fn test_marketplace_serialization() {
    let tensor = Marketplace::Tensor;
    let json = serde_json::to_string(&tensor).unwrap();
    assert_eq!(json, "\"tensor\"");

    let me = Marketplace::MagicEden;
    let json = serde_json::to_string(&me).unwrap();
    assert_eq!(json, "\"magiceden\"");
}

#[test]
fn test_nft_collection_serialization() {
    let collection = NftCollection {
        symbol: "degods".to_string(),
        name: "DeGods".to_string(),
        floor_price: 5.5,
        volume_24h: 100.0,
        listed_count: 500,
        total_supply: Some(10000),
        image_url: Some("https://example.com/image.png".to_string()),
    };

    let json = serde_json::to_string(&collection).unwrap();
    assert!(json.contains("\"symbol\":\"degods\""));
    assert!(json.contains("\"floor_price\":5.5"));
}

#[test]
fn test_nft_listing_serialization() {
    let listing = NftListing {
        mint: "mint_address".to_string(),
        name: "DeGod #1234".to_string(),
        collection: "degods".to_string(),
        price: 6.0,
        seller: "seller_address".to_string(),
        marketplace: "MagicEden".to_string(),
        image_url: None,
        rarity_rank: Some(100),
    };

    let json = serde_json::to_string(&listing).unwrap();
    assert!(json.contains("\"mint\":\"mint_address\""));
    assert!(json.contains("\"price\":6"));
}

// ===== Client Creation Tests =====

#[tokio::test]
async fn test_drift_client_creation() {
    let rpc_client = Arc::new(RpcClient::new(
        "https://api.mainnet-beta.solana.com".to_string(),
    ));
    let client = DriftClient::new(rpc_client);

    let program_id = client.program_id();
    assert_eq!(program_id.to_string(), DRIFT_PROGRAM_ID);
}

#[tokio::test]
async fn test_raydium_client_creation() {
    let rpc_client = Arc::new(RpcClient::new(
        "https://api.mainnet-beta.solana.com".to_string(),
    ));
    let client = RaydiumClient::new(rpc_client);

    let amm_id = client.amm_program_id();
    let clmm_id = client.clmm_program_id();
    assert_eq!(amm_id.to_string(), RAYDIUM_AMM_PROGRAM_ID);
    assert_eq!(clmm_id.to_string(), RAYDIUM_CLMM_PROGRAM_ID);
}

#[tokio::test]
async fn test_meteora_client_creation() {
    let rpc_client = Arc::new(RpcClient::new(
        "https://api.mainnet-beta.solana.com".to_string(),
    ));
    let client = MeteoraClient::new(rpc_client);

    let program_id = client.program_id();
    assert_eq!(program_id.to_string(), METEORA_DLMM_PROGRAM_ID);
}

#[tokio::test]
async fn test_nft_client_creation() {
    let rpc_client = Arc::new(RpcClient::new(
        "https://api.mainnet-beta.solana.com".to_string(),
    ));
    let client = NftClient::new(rpc_client);

    let tensor_id = client.tensor_program_id();
    let me_id = client.magic_eden_program_id();
    assert_eq!(tensor_id.to_string(), TENSOR_SWAP_PROGRAM_ID);
    assert_eq!(me_id.to_string(), MAGIC_EDEN_PROGRAM_ID);
}

#[tokio::test]
async fn test_drift_get_markets() {
    let rpc_client = Arc::new(RpcClient::new(
        "https://api.mainnet-beta.solana.com".to_string(),
    ));
    let client = DriftClient::new(rpc_client);

    let markets = client.get_markets().await.unwrap();
    assert!(!markets.is_empty());
    assert!(markets.iter().any(|m| m.symbol == "SOL-PERP"));
}
