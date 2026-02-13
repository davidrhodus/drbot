//! Demo: Agent-to-agent OTC desk (fixed/spread) for SOL/USDC.
//!
//! Run:
//!   cargo run -p drbot-solana --example otc_a2a_sol_usdc_spread

use drbot_a2a::{A2AConfig, A2AHub, Agent};
use drbot_solana::otc::{make_sol_rfq, OtcDeskAgent, OtcTraderClient, TradeDirection};
use serde_json::json;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::sync::Arc;
use tokio::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Negotiation manager requires an RPC client even though this demo does not hit the network.
    let rpc = Arc::new(RpcClient::new_mock("succeeds".to_string()));

    // In-process A2A hub (local broadcast bus).
    let hub = Arc::new(A2AHub::new(
        A2AConfig::default(),
        Agent::new("hub", "coordinator"),
    ));

    // Spawn a SOL/USDC spread desk (mid=150 USDC/SOL, spread=0.80%).
    let desk = OtcDeskAgent::new_sol_usdc_spread(rpc, "desk-1", 150.0, 80);
    let _desk_task = desk.spawn(hub.clone()).await;

    // Create trader client and broadcast an RFQ to any desks.
    let trader = OtcTraderClient::new(hub.clone(), "trader-1").await;
    let rfq = make_sol_rfq(
        TradeDirection::Buy,
        1_000_000_000,
        120,
        solana_sdk::pubkey::Pubkey::default(),
    );
    let quotes = trader.broadcast_rfq(rfq, 5, Duration::from_secs(1)).await?;

    let output: Vec<_> = quotes
        .into_iter()
        .map(|(from, env)| json!({ "from": from, "envelope": env }))
        .collect();

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
