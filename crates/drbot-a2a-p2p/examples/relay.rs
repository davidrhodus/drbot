//! Run a libp2p circuit relay server for A2A.
//!
//! Example:
//!   cargo run -p drbot-a2a-p2p --example relay -- --listen /ip4/0.0.0.0/tcp/4001 --identity ./relay.key

use clap::Parser;
use drbot_a2a::{A2AConfig, A2AHub, Agent};
use drbot_a2a_p2p::{start_p2p_bridge, P2PConfig};
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::Duration;

#[derive(Debug, Parser)]
struct Args {
    /// Multiaddr(s) to listen on.
    #[arg(long = "listen", default_value = "/ip4/0.0.0.0/tcp/4001")]
    listen_addrs: Vec<String>,

    /// Keypair path (persist identity to keep the same PeerId).
    #[arg(long, default_value = "./a2a-relay.key")]
    identity: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    let listen_addrs: Vec<Multiaddr> = args
        .listen_addrs
        .iter()
        .map(|a| a.parse())
        .collect::<Result<_, _>>()?;

    let hub = Arc::new(A2AHub::new(
        A2AConfig::default(),
        Agent::new("a2a-relay", "a2a_p2p_relay"),
    ));

    // Give the hub a moment to register its local agent.
    tokio::time::sleep(Duration::from_millis(25)).await;

    let mut cfg = P2PConfig::default();
    cfg.listen_addrs = listen_addrs;
    cfg.relay_server = true;
    cfg.identity_keypair_path = Some(args.identity);

    let bridge = start_p2p_bridge(hub, cfg).await?;

    tokio::time::sleep(Duration::from_millis(250)).await;
    println!("peer_id: {}", bridge.peer_id);

    for addr in bridge.listen_addrs().await {
        let dial_addr = if addr.iter().any(|p| matches!(p, Protocol::P2p(_))) {
            addr.clone()
        } else {
            addr.clone().with(Protocol::P2p(bridge.peer_id.into()))
        };

        println!("listen_addr: {addr}");
        println!("dial_addr:   {dial_addr}");
    }

    println!("Press Ctrl-C to stop.");
    tokio::signal::ctrl_c().await?;
    bridge.shutdown().await;
    Ok(())
}
