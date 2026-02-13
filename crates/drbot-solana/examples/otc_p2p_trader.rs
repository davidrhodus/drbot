//! OTC trader over libp2p (true P2P overlay).
//!
//! Example:
//!   cargo run -p drbot-solana --example otc_p2p_trader -- \
//!     --listen /ip4/0.0.0.0/tcp/0 \
//!     --bootstrap /ip4/127.0.0.1/tcp/4001/p2p/<RELAY_PEER_ID> \
//!     --relay /ip4/127.0.0.1/tcp/4001/p2p/<RELAY_PEER_ID> \
//!     --direction buy --amount-sol 1 --timeout-ms 2000
//!
//! Settlement (optional, on Solana):
//!   cargo run -p drbot-solana --example otc_p2p_trader -- \
//!     --rpc-url https://api.mainnet-beta.solana.com \
//!     --wallet ~/.config/solana/trader.json \
//!     --escrow-program-id <PROGRAM_PUBKEY> \
//!     --direction buy --amount-sol 1

use chrono::Utc;
use clap::Parser;
use drbot_a2a::{A2AConfig, A2AHub, Agent};
use drbot_a2a_p2p::{start_p2p_bridge, P2PConfig};
use drbot_solana::otc::{
    build_escrow_params, make_sol_rfq, sign_otc_notification, spawn_otc_auto_cancel_watcher,
    wait_for_settled_notification, EscrowManager, EscrowParty, OTCMessage, OtcSettlementWatch,
    OtcSettlementWatchStore, OtcTraderClient, TradeDirection, OTC_CAPABILITY,
};
use drbot_solana::wallet::FileKeypairManager;
use serde_json::json;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::Signer;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

#[derive(Debug, Parser)]
struct Args {
    /// Trader name (also becomes the A2A agent name).
    #[arg(long, default_value = "trader-1")]
    name: String,

    /// Multiaddr to listen on (libp2p).
    #[arg(long, default_value = "/ip4/0.0.0.0/tcp/0")]
    listen: String,

    /// Bootstrap peers to dial (repeatable).
    #[arg(long)]
    bootstrap: Vec<String>,

    /// Relay servers to reserve through (repeatable).
    ///
    /// Provide the relay's reachable address including `/p2p/<peer_id>`.
    #[arg(long)]
    relay: Vec<String>,

    /// Persisted p2p identity key (stable PeerId).
    #[arg(long, default_value = "./otc-trader.key")]
    identity: PathBuf,

    /// If set, do not create a new RFQ; only watch/cancel existing state.
    #[arg(long, default_value_t = false)]
    watch_only: bool,

    /// Optional Solana keypair file (enables on-chain settlement when paired with --escrow-program-id).
    #[arg(long)]
    wallet: Option<PathBuf>,

    /// Optional Solana keypair to pay transaction fees (fee sponsorship).
    ///
    /// When set, the trader's `--wallet` still signs as the funder/rent-payer, but this key pays fees.
    #[arg(long)]
    fee_payer_wallet: Option<PathBuf>,

    /// OTC escrow program id (enables on-chain settlement when paired with --wallet).
    #[arg(long)]
    escrow_program_id: Option<Pubkey>,

    /// Optional peer store file (remember discovered peers across restarts).
    #[arg(long)]
    peer_store: Option<PathBuf>,

    /// Solana RPC URL (use "mock" for no-network demo mode).
    #[arg(long, default_value = "mock")]
    rpc_url: String,

    /// Persist settlement watch state to disk (crash/restart safety).
    #[arg(long, default_value = "./otc-trader-state.json")]
    state_file: PathBuf,

    /// Autosave flush interval (ms) for --state-file.
    #[arg(long, default_value_t = 1000)]
    state_flush_ms: u64,

    /// RFQ direction: buy or sell.
    #[arg(long, default_value = "buy")]
    direction: String,

    /// RFQ amount in SOL.
    #[arg(long, default_value_t = 1.0)]
    amount_sol: f64,

    /// RFQ expiry (seconds).
    #[arg(long, default_value_t = 120)]
    expires_secs: u64,

    /// Wait for desks to appear (ms).
    #[arg(long, default_value_t = 5000)]
    discover_wait_ms: u64,

    /// Quote collection timeout (ms).
    #[arg(long, default_value_t = 2000)]
    timeout_ms: u64,

    /// Settlement wait timeout (ms).
    #[arg(long, default_value_t = 15000)]
    settle_timeout_ms: u64,

    /// Max quotes to collect.
    #[arg(long, default_value_t = 5)]
    max_quotes: usize,

    /// Announcement interval seconds.
    #[arg(long, default_value_t = 5)]
    announce_interval_secs: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    let rpc = if args.rpc_url == "mock" {
        Arc::new(RpcClient::new_mock("succeeds".to_string()))
    } else {
        Arc::new(RpcClient::new(args.rpc_url.clone()))
    };

    let watch_store = Arc::new(OtcSettlementWatchStore::new());
    let _watch_persistence_task = watch_store
        .enable_persistence(&args.state_file, Duration::from_millis(args.state_flush_ms))
        .await?;

    if args.wallet.is_some() ^ args.escrow_program_id.is_some() {
        return Err("Provide both --wallet and --escrow-program-id to enable settlement".into());
    }
    if args.wallet.is_some() && args.rpc_url == "mock" {
        return Err("Settlement requires a real --rpc-url (not mock)".into());
    }
    if args.fee_payer_wallet.is_some() && args.wallet.is_none() {
        return Err("--fee-payer-wallet requires --wallet/--escrow-program-id".into());
    }

    if args.watch_only {
        let wallet = args
            .wallet
            .as_ref()
            .ok_or("--watch-only requires --wallet/--escrow-program-id")?;
        let program_id = args
            .escrow_program_id
            .ok_or("--watch-only requires --wallet/--escrow-program-id")?;
        if args.rpc_url == "mock" {
            return Err("--watch-only requires a real --rpc-url (not mock)".into());
        }

        let keypair_manager = FileKeypairManager::from_file(wallet)?;
        let fee_payer = args
            .fee_payer_wallet
            .as_ref()
            .map(|p| FileKeypairManager::from_file(p).map(|m| m.keypair().insecure_clone()))
            .transpose()?;

        let escrow_manager = Arc::new(EscrowManager::new(rpc.clone()).with_program_id(program_id));
        let _watcher_task = spawn_otc_auto_cancel_watcher(
            watch_store.clone(),
            escrow_manager,
            keypair_manager.keypair().insecure_clone(),
            fee_payer,
            None,
        );

        let watches = watch_store.list().await;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "watch_only": true,
                "state_file": args.state_file.to_string_lossy(),
                "watch_count": watches.len(),
            }))?
        );

        println!("Press Ctrl-C to stop.");
        tokio::signal::ctrl_c().await?;
        return Ok(());
    }

    let hub = Arc::new(A2AHub::new(
        A2AConfig::default(),
        Agent::new("otc-node", "otc_p2p_node"),
    ));

    let trader = OtcTraderClient::new(hub.clone(), &args.name).await;
    let trader_agent_id = trader.agent.id;

    let mut cfg = P2PConfig::default();
    cfg.listen_addrs = vec![args.listen.parse()?];
    cfg.bootstrap_peers = args
        .bootstrap
        .iter()
        .map(|a| a.parse())
        .collect::<Result<_, _>>()?;
    cfg.relay_peers = args
        .relay
        .iter()
        .map(|a| a.parse())
        .collect::<Result<_, _>>()?;
    cfg.relay_server = false;
    cfg.announce_interval = Duration::from_secs(args.announce_interval_secs);
    cfg.identity_keypair_path = Some(args.identity);
    cfg.peer_store_path = args.peer_store;

    let bridge = start_p2p_bridge(hub.clone(), cfg).await?;

    // Wait briefly for any desks to be discovered.
    let deadline = Instant::now() + Duration::from_millis(args.discover_wait_ms);
    loop {
        let desks = hub.discover(OTC_CAPABILITY).await;
        let found = desks.iter().any(|a| a.id != trader_agent_id);
        if found || Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let direction = match args.direction.as_str() {
        "sell" => TradeDirection::Sell,
        _ => TradeDirection::Buy,
    };
    let amount_lamports = (args.amount_sol * 1e9).round() as u64;
    let keypair_manager = if let Some(path) = &args.wallet {
        Some(FileKeypairManager::from_file(path)?)
    } else {
        None
    };
    let fee_payer_manager = if let Some(path) = &args.fee_payer_wallet {
        Some(FileKeypairManager::from_file(path)?)
    } else {
        None
    };

    let initiator_wallet = keypair_manager
        .as_ref()
        .map(|k| k.pubkey())
        .unwrap_or_else(Pubkey::default);

    let rfq = make_sol_rfq(
        direction,
        amount_lamports,
        args.expires_secs,
        initiator_wallet,
    );

    let quotes = if let Some(kp) = keypair_manager.as_ref() {
        trader
            .broadcast_rfq_signed(
                rfq.clone(),
                kp.keypair(),
                args.max_quotes,
                Duration::from_millis(args.timeout_ms),
            )
            .await?
    } else {
        trader
            .broadcast_rfq(
                rfq.clone(),
                args.max_quotes,
                Duration::from_millis(args.timeout_ms),
            )
            .await?
    };

    let output: Vec<_> = quotes
        .iter()
        .map(|(from, env)| json!({ "from": from, "envelope": env }))
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "peer_id": bridge.peer_id.to_string(),
            "quotes": output
        }))?
    );

    if let (Some(keypair_manager), Some(program_id)) = (keypair_manager, args.escrow_program_id) {
        let fee_payer = fee_payer_manager
            .as_ref()
            .map(|m| m.keypair().insecure_clone());

        let escrow_manager = Arc::new(EscrowManager::new(rpc.clone()).with_program_id(program_id));
        let _watcher_task = spawn_otc_auto_cancel_watcher(
            watch_store.clone(),
            escrow_manager.clone(),
            keypair_manager.keypair().insecure_clone(),
            fee_payer_manager
                .as_ref()
                .map(|m| m.keypair().insecure_clone()),
            None,
        );

        // Pick best quote (lowest ask for buy; highest bid for sell).
        let best = quotes
            .iter()
            .filter(|(_, env)| env.verify_signature().ok() == Some(true))
            .filter_map(|(from, env)| match &env.message {
                OTCMessage::Quote {
                    price,
                    quantity,
                    settlement_amount,
                    settlement_asset,
                    ..
                } => {
                    if *quantity == 0 {
                        return None;
                    }

                    // For SOL/USDC, trust the on-chain amount (settlement_amount) over the float.
                    let effective_price = if settlement_asset.eq_ignore_ascii_case("USDC") {
                        (*settlement_amount as f64) * 1000.0 / (*quantity as f64)
                    } else {
                        *price
                    };

                    Some((*from, env.clone(), effective_price))
                }
                _ => None,
            })
            .reduce(|a, b| match direction {
                TradeDirection::Buy => {
                    if b.2 < a.2 {
                        b
                    } else {
                        a
                    }
                }
                TradeDirection::Sell => {
                    if b.2 > a.2 {
                        b
                    } else {
                        a
                    }
                }
            });

        let Some((desk_agent_id, best_env, _price)) = best else {
            return Err("No quotes received".into());
        };

        let (quote_id, quote_msg) = match &best_env.message {
            OTCMessage::Quote { id, .. } => (*id, best_env.message.clone()),
            _ => return Err("Selected message is not a quote".into()),
        };

        // Send accept to the desk.
        let accept = OTCMessage::Accept {
            quote_id,
            accepting_wallet: initiator_wallet,
        };
        hub.send(sign_otc_notification(
            trader_agent_id,
            desk_agent_id,
            keypair_manager.keypair(),
            accept,
        )?)
        .await?;

        // Create escrow and fund Party A.
        let params = build_escrow_params(&rfq, &quote_msg)?;
        let keypair = keypair_manager.keypair();
        let (escrow_address, _) =
            escrow_manager.derive_address(params.negotiation_id, params.party_a, params.party_b)?;

        // Persist a watch entry before funding so we can safely resume after crashes.
        watch_store
            .upsert(OtcSettlementWatch::new(
                params.negotiation_id,
                params.party_a,
                params.party_b,
                escrow_address,
                params.expiry_unix_ts,
            ))
            .await;
        watch_store.flush().await?;

        if escrow_manager
            .try_get_escrow(&escrow_address)
            .await?
            .is_none()
        {
            match fee_payer.as_ref() {
                Some(fee_payer) => {
                    let _ = escrow_manager
                        .create_escrow_with_fee_payer(fee_payer, keypair, params.clone())
                        .await?;
                }
                None => {
                    let _ = escrow_manager
                        .create_escrow(keypair, params.clone())
                        .await?;
                }
            }
        }

        let token_source = if params.a_owes.kind == drbot_otc_escrow_program::LegKind::SplToken {
            Some(EscrowManager::associated_token_address(
                &keypair.pubkey(),
                &params.a_owes.mint,
            ))
        } else {
            None
        };

        let sig = match fee_payer.as_ref() {
            Some(fee_payer) => {
                escrow_manager
                    .fund_party_a_with_fee_payer(fee_payer, keypair, escrow_address, token_source)
                    .await?
            }
            None => {
                escrow_manager
                    .fund_party_a(keypair, escrow_address, token_source)
                    .await?
            }
        };

        let settled = escrow_manager
            .await_escrow_closed(
                &escrow_address,
                Duration::from_secs(2),
                Duration::from_millis(250),
            )
            .await
            .unwrap_or(false);

        let notify_msg = if settled {
            let (final_price, final_quantity) = match &quote_msg {
                OTCMessage::Quote {
                    price, quantity, ..
                } => (*price, *quantity),
                _ => (0.0, 0),
            };
            OTCMessage::Settled {
                negotiation_id: params.negotiation_id,
                signature: sig.to_string(),
                final_price,
                final_quantity,
                reporting_wallet: keypair.pubkey(),
            }
        } else {
            OTCMessage::EscrowFunded {
                negotiation_id: params.negotiation_id,
                escrow_address,
                signature: sig.to_string(),
                funded_by: EscrowParty::PartyA,
                reporting_wallet: keypair.pubkey(),
            }
        };

        hub.send(sign_otc_notification(
            trader_agent_id,
            desk_agent_id,
            keypair_manager.keypair(),
            notify_msg,
        )?)
        .await?;

        if !settled {
            // Fast path: wait for a signed A2A "Settled" notification.
            let now_ts = Utc::now().timestamp();
            let remaining_ms = ((params.expiry_unix_ts - now_ts).max(0) as u64) * 1000;
            let a2a_wait_ms = std::cmp::min(args.settle_timeout_ms, remaining_ms);
            if a2a_wait_ms > 0 {
                if let Some((_from, env)) = wait_for_settled_notification(
                    hub.clone(),
                    trader_agent_id,
                    params.negotiation_id,
                    Duration::from_millis(a2a_wait_ms),
                )
                .await?
                {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "settled": env,
                        }))?
                    );
                    watch_store.remove(params.negotiation_id).await;
                    watch_store.flush().await?;
                    bridge.shutdown().await;
                    return Ok(());
                }
            }

            // Wait for on-chain resolution. The background watcher will auto-cancel after expiry.
            let poll_interval = Duration::from_millis(500);
            loop {
                if let Some(status) = escrow_manager
                    .get_receipt_status(params.negotiation_id, params.party_a, params.party_b)
                    .await?
                {
                    match status {
                        drbot_otc_escrow_program::ReceiptStatus::Settled => {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&json!({ "status": "settled" }))?
                            );
                            watch_store.remove(params.negotiation_id).await;
                            watch_store.flush().await?;
                            break;
                        }
                        drbot_otc_escrow_program::ReceiptStatus::Cancelled => {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&json!({ "status": "cancelled" }))?
                            );
                            watch_store.remove(params.negotiation_id).await;
                            watch_store.flush().await?;
                            break;
                        }
                        drbot_otc_escrow_program::ReceiptStatus::Open => {}
                    }
                }

                tokio::time::sleep(poll_interval).await;
            }
        }
    }

    bridge.shutdown().await;
    Ok(())
}
