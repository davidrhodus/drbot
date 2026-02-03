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
use drbot_solana::otc::{
    build_escrow_params, otc_notification, parse_otc_envelope, EscrowManager, EscrowParty,
    OTCEnvelope, OTCMessage, OtcDeskAgent,
};
use drbot_solana::wallet::FileKeypairManager;
use drbot_solana::SolanaError;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signer::Signer;
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

    /// OTC escrow program id (enables on-chain settlement when paired with --wallet).
    #[arg(long)]
    escrow_program_id: Option<Pubkey>,

    /// Optional peer store file (remember discovered peers across restarts).
    #[arg(long)]
    peer_store: Option<PathBuf>,

    /// Solana RPC URL (use "mock" for no-network demo mode).
    #[arg(long, default_value = "mock")]
    rpc_url: String,

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

    let hub = Arc::new(A2AHub::new(
        A2AConfig::default(),
        Agent::new("otc-node", "otc_p2p_node"),
    ));

    // Register desk agent first so the first announcement includes it.
    let (desk, desk_keypair) = if let Some(path) = &args.wallet {
        let manager = FileKeypairManager::from_file(path)?;
        let signer = manager.keypair().insecure_clone();
        (
            OtcDeskAgent::new_sol_usdc_spread_with_wallet(
                rpc.clone(),
                &args.name,
                args.mid,
                args.spread_bps,
                manager.pubkey(),
            )
            .with_signing_keypair(signer),
            Some(manager),
        )
    } else {
        (
            OtcDeskAgent::new_sol_usdc_spread(rpc.clone(), &args.name, args.mid, args.spread_bps),
            None,
        )
    };

    let negotiation_manager = desk.negotiation_manager();
    let desk_agent_id = desk.agent.id;
    let _desk_task = desk.spawn(hub.clone()).await;

    if let (Some(keypair_manager), Some(program_id)) = (desk_keypair, args.escrow_program_id) {
        let escrow_manager = EscrowManager::new(rpc.clone()).with_program_id(program_id);
        let hub = hub.clone();
        tokio::spawn(async move {
            let mut rx = hub.subscribe();

            loop {
                let msg = match rx.recv().await {
                    Ok(m) => m,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };

                if msg.to != desk_agent_id {
                    continue;
                }

                let Some(envelope) = parse_otc_envelope(&msg) else {
                    continue;
                };

                if envelope.verify_signature().ok() != Some(true) {
                    continue;
                }

                let OTCMessage::Accept {
                    quote_id,
                    accepting_wallet,
                } = envelope.message
                else {
                    continue;
                };

                let ctx = match negotiation_manager.quote_context(quote_id).await {
                    Ok(ctx) => ctx,
                    Err(e) => {
                        tracing::warn!(error = %e, %quote_id, "Failed to resolve accepted quote");
                        continue;
                    }
                };

                let params = match build_escrow_params(&ctx.rfq, &ctx.quote) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(error = %e, %quote_id, "Failed to build escrow params");
                        continue;
                    }
                };

                let expected_party_a = match &ctx.rfq {
                    OTCMessage::Rfq {
                        initiator_wallet, ..
                    } => *initiator_wallet,
                    _ => Pubkey::default(),
                };
                if accepting_wallet != expected_party_a {
                    tracing::warn!(
                        %accepting_wallet,
                        %expected_party_a,
                        "Accepting wallet does not match RFQ initiator"
                    );
                    continue;
                }

                let keypair = keypair_manager.keypair();

                let (escrow_address, _) = match escrow_manager.create_escrow(keypair, params.clone()).await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(error = %e, "Create escrow failed");
                        continue;
                    }
                };

                let token_source = if params.b_owes.kind == drbot_otc_escrow_program::LegKind::SplToken {
                    Some(EscrowManager::associated_token_address(&keypair.pubkey(), &params.b_owes.mint))
                } else {
                    None
                };

                let sig = match escrow_manager.fund_party_b(keypair, escrow_address, token_source).await {
                    Ok(sig) => sig,
                    Err(e) => {
                        tracing::warn!(error = %e, "Fund party B failed");
                        continue;
                    }
                };

                let settled = match escrow_manager.get_escrow(&escrow_address).await {
                    Ok(_) => false,
                    Err(SolanaError::RpcError(msg)) => {
                        msg.contains("AccountNotFound")
                            || msg.contains("could not find account")
                            || msg.contains("could not find")
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to confirm escrow state after funding");
                        false
                    }
                };

                let reply_msg = if settled {
                    let (final_price, final_quantity) = match &ctx.quote {
                        OTCMessage::Quote { price, quantity, .. } => (*price, *quantity),
                        _ => (0.0, 0),
                    };
                    OTCMessage::Settled {
                        negotiation_id: ctx.negotiation_id,
                        signature: sig.to_string(),
                        final_price,
                        final_quantity,
                        reporting_wallet: keypair.pubkey(),
                    }
                } else {
                    OTCMessage::EscrowFunded {
                        negotiation_id: ctx.negotiation_id,
                        escrow_address,
                        signature: sig.to_string(),
                        funded_by: EscrowParty::PartyB,
                        reporting_wallet: keypair.pubkey(),
                    }
                };

                let reply_env = match OTCEnvelope::new(desk_agent_id.to_string(), reply_msg)
                    .sign_with(keypair)
                {
                    Ok(env) => env,
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to sign settlement update");
                        continue;
                    }
                };
                let reply = otc_notification(desk_agent_id, msg.from, reply_env);
                if let Err(e) = hub.send(reply).await {
                    tracing::warn!(error = %e, "Failed to send settlement update");
                }
            }
        });
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
