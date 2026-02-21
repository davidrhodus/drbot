use drbot_a2a::{A2AConfig, A2AHub, Agent};
use drbot_a2a_p2p::{start_p2p_bridge, P2PConfig};
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{timeout, Instant};

async fn wait_for_listen_addr(bridge: &drbot_a2a_p2p::P2PBridge) -> Multiaddr {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(addr) = bridge.listen_addrs().await.into_iter().next() {
            return addr;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for listen addr");
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_connected(
    bridge: &drbot_a2a_p2p::P2PBridge,
    peer_id: drbot_a2a_p2p::PeerId,
    deadline: Instant,
) {
    loop {
        if bridge
            .connected_peers()
            .await
            .into_iter()
            .any(|p| p == peer_id)
        {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for connection to {peer_id}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn dht_provider_discovery_connects_peers_without_announce_autodial() {
    // Bootstrap node B.
    let hub_b = Arc::new(A2AHub::new(
        A2AConfig::default(),
        Agent::new("node-b", "bootstrap"),
    ));
    let mut cfg_b = P2PConfig::default();
    cfg_b.listen_addrs = vec!["/memory/0".parse().unwrap()];
    cfg_b.announce_interval = Duration::from_secs(60);
    cfg_b.auto_dial = false;
    cfg_b.dht_enabled = true;
    cfg_b.dht_dial_providers = false;
    cfg_b.dht_provide_interval = Duration::from_millis(100);
    cfg_b.dht_query_interval = Duration::from_millis(100);

    let bridge_b = start_p2p_bridge(hub_b, cfg_b).await.unwrap();
    let addr_b = wait_for_listen_addr(&bridge_b).await;
    let dial_b = addr_b.with(Protocol::P2p(bridge_b.peer_id));

    // Node A and C only bootstrap to B; announcements don't auto-dial; DHT provider discovery must connect them.
    let mk_cfg = |name: &str| {
        let hub = Arc::new(A2AHub::new(A2AConfig::default(), Agent::new(name, "node")));
        let mut cfg = P2PConfig::default();
        cfg.listen_addrs = vec!["/memory/0".parse().unwrap()];
        cfg.bootstrap_peers = vec![dial_b.clone()];
        cfg.announce_interval = Duration::from_secs(60);
        cfg.auto_dial = false; // announcements won't create the mesh
        cfg.auto_dial_interval = Duration::from_millis(50); // dial loop for DHT discoveries
        cfg.auto_dial_batch = 16;
        cfg.dht_enabled = true;
        cfg.dht_dial_providers = true;
        cfg.dht_provide_interval = Duration::from_millis(100);
        cfg.dht_query_interval = Duration::from_millis(100);
        (hub, cfg)
    };

    let (hub_a, cfg_a) = mk_cfg("node-a");
    let (hub_c, cfg_c) = mk_cfg("node-c");

    let bridge_a = start_p2p_bridge(hub_a, cfg_a).await.unwrap();
    let bridge_c = start_p2p_bridge(hub_c, cfg_c).await.unwrap();

    // Ensure both nodes have reached the bootstrap peer before asserting provider discovery.
    let bootstrap_deadline = Instant::now() + Duration::from_secs(5);
    timeout(Duration::from_secs(6), async {
        tokio::join!(
            wait_connected(&bridge_a, bridge_b.peer_id, bootstrap_deadline),
            wait_connected(&bridge_c, bridge_b.peer_id, bootstrap_deadline),
        );
    })
    .await
    .expect("bootstrap connect timeout");

    let deadline = Instant::now() + Duration::from_secs(8);
    timeout(Duration::from_secs(10), async {
        tokio::join!(
            wait_connected(&bridge_a, bridge_c.peer_id, deadline),
            wait_connected(&bridge_c, bridge_a.peer_id, deadline),
        );
    })
    .await
    .expect("DHT discovery mesh timeout");

    bridge_a.shutdown().await;
    bridge_c.shutdown().await;
    bridge_b.shutdown().await;
}
