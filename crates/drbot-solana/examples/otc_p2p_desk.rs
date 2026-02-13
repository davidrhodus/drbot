//! OTC desk over libp2p (true P2P overlay).
//!
//! Example:
//!   cargo run -p drbot-solana --example otc_p2p_desk -- \
//!     --listen /ip4/0.0.0.0/tcp/0 \
//!     --bootstrap /ip4/127.0.0.1/tcp/4001/p2p/<RELAY_PEER_ID> \
//!     --relay /ip4/127.0.0.1/tcp/4001/p2p/<RELAY_PEER_ID> \
//!     --mid 150 --spread-bps 80
//!
//! Settlement (optional, on Solana):
//!   cargo run -p drbot-solana --example otc_p2p_desk -- \
//!     --rpc-url https://api.mainnet-beta.solana.com \
//!     --wallet ~/.config/solana/desk.json \
//!     --escrow-program-id <PROGRAM_PUBKEY> \
//!     --listen /ip4/0.0.0.0/tcp/0 --bootstrap ... --relay ...

use clap::Parser;
use drbot_a2a::{A2AConfig, A2AHub, Agent};
use drbot_a2a_p2p::{start_p2p_bridge, P2PConfig};
use drbot_solana::otc::{spawn_desk_settlement_service, OtcDeskAgent};
use drbot_solana::wallet::FileKeypairManager;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Parser)]
struct Args {
    /// Human-readable desk name (also becomes the A2A agent name).
    #[arg(long, default_value = "desk-1")]
    name: String,

    /// Mid price (USDC per SOL).
    #[arg(long, default_value_t = 150.0)]
    mid: f64,

    /// Total spread in basis points (e.g. 80 = 0.80%).
    #[arg(long, default_value_t = 80)]
    spread_bps: u16,

    /// Settlement USDC mint (defaults to mainnet USDC).
    ///
    /// Override this for devnet/testing where the mainnet mint does not exist.
    #[arg(long, default_value = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v")]
    usdc_mint: Pubkey,

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
    #[arg(long, default_value = "./otc-desk.key")]
    identity: PathBuf,

    /// Optional Solana keypair file (enables on-chain settlement when paired with --escrow-program-id).
    #[arg(long)]
    wallet: Option<PathBuf>,

    /// Optional Solana keypair to pay transaction fees (fee sponsorship).
    ///
    /// When set, the desk's `--wallet` still signs as the funder/rent-payer, but this key pays fees.
    #[arg(long)]
    fee_payer_wallet: Option<PathBuf>,

    /// If set, the desk will create the escrow account if it's missing (desk pays rent/fees).
    ///
    /// For an open network, leaving this off is safer: require the initiator (Party A) to create.
    #[arg(long, default_value_t = false)]
    create_escrow: bool,

    /// OTC escrow program id (enables on-chain settlement when paired with --wallet).
    #[arg(long)]
    escrow_program_id: Option<Pubkey>,

    /// Optional peer store file (remember discovered peers across restarts).
    #[arg(long)]
    peer_store: Option<PathBuf>,

    /// Solana RPC URL (use "mock" for no-network demo mode).
    #[arg(long, default_value = "mock")]
    rpc_url: String,

    /// Persist negotiation state to disk (crash/restart safety).
    #[arg(long, default_value = "./otc-desk-state.json")]
    state_file: PathBuf,

    /// Autosave flush interval (ms) for --state-file.
    #[arg(long, default_value_t = 1000)]
    state_flush_ms: u64,

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

    if args.wallet.is_some() ^ args.escrow_program_id.is_some() {
        return Err("Provide both --wallet and --escrow-program-id to enable settlement".into());
    }
    if args.wallet.is_some() && args.rpc_url == "mock" {
        return Err("Settlement requires a real --rpc-url (not mock)".into());
    }
    if args.fee_payer_wallet.is_some() && args.wallet.is_none() {
        return Err("--fee-payer-wallet requires --wallet/--escrow-program-id".into());
    }

    let hub = Arc::new(A2AHub::new(
        A2AConfig::default(),
        Agent::new("otc-node", "otc_p2p_node"),
    ));

    // Register desk agent first so the first announcement includes it.
    let (desk, desk_keypair) = if let Some(path) = &args.wallet {
        let manager = FileKeypairManager::from_file(path)?;
        let signer = manager.keypair().insecure_clone();
        (
            OtcDeskAgent::new_sol_usdc_spread_with_wallet_and_mint(
                rpc.clone(),
                &args.name,
                args.mid,
                args.spread_bps,
                manager.pubkey(),
                args.usdc_mint,
            )
            .with_signing_keypair(signer),
            Some(manager),
        )
    } else {
        (
            OtcDeskAgent::new_sol_usdc_spread_with_mint(
                rpc.clone(),
                &args.name,
                args.mid,
                args.spread_bps,
                args.usdc_mint,
            ),
            None,
        )
    };

    let negotiation_manager = desk.negotiation_manager();
    let _persistence_task = negotiation_manager
        .enable_persistence(&args.state_file, Duration::from_millis(args.state_flush_ms))
        .await?;
    let desk_agent_id = desk.agent.id;
    let _desk_task = desk.spawn(hub.clone()).await;

    if let (Some(keypair_manager), Some(program_id)) = (desk_keypair, args.escrow_program_id) {
        let fee_payer = if let Some(path) = &args.fee_payer_wallet {
            Some(
                FileKeypairManager::from_file(path)?
                    .keypair()
                    .insecure_clone(),
            )
        } else {
            None
        };

        let _settlement_task = spawn_desk_settlement_service(
            hub.clone(),
            negotiation_manager.clone(),
            rpc.clone(),
            program_id,
            desk_agent_id,
            keypair_manager.keypair().insecure_clone(),
            fee_payer,
            args.create_escrow,
        );
    }

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

    let bridge = start_p2p_bridge(hub, cfg).await?;

    tokio::time::sleep(Duration::from_millis(250)).await;
    println!("peer_id: {}", bridge.peer_id);
    println!("desk_agent_id: {desk_agent_id}");
    for addr in bridge.listen_addrs().await {
        println!("addr: {addr}");
    }

    println!("Press Ctrl-C to stop.");
    tokio::signal::ctrl_c().await?;
    bridge.shutdown().await;
    Ok(())
}
