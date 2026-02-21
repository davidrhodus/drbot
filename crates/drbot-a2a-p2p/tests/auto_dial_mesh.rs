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
async fn auto_dial_forms_mesh_beyond_bootstrap() {
    // Bootstrap node B (only address known to A and C).
    let hub_b = Arc::new(A2AHub::new(
        A2AConfig::default(),
        Agent::new("node-b", "bootstrap"),
    ));
    let mut cfg_b = P2PConfig::default();
    cfg_b.listen_addrs = vec!["/memory/0".parse().unwrap()];
    cfg_b.announce_interval = Duration::from_millis(100);
    cfg_b.auto_dial = false;

    let bridge_b = start_p2p_bridge(hub_b, cfg_b).await.unwrap();
    let addr_b = wait_for_listen_addr(&bridge_b).await;
    let dial_b = addr_b.with(Protocol::P2p(bridge_b.peer_id.into()));

    // Node A and C only bootstrap to B, then should auto-dial each other once they
    // learn each other's addresses via announcements routed through B.
    let hub_a = Arc::new(A2AHub::new(
        A2AConfig::default(),
        Agent::new("node-a", "node"),
    ));
    let mut cfg_a = P2PConfig::default();
    cfg_a.listen_addrs = vec!["/memory/0".parse().unwrap()];
    cfg_a.bootstrap_peers = vec![dial_b.clone()];
    cfg_a.announce_interval = Duration::from_millis(100);
    cfg_a.auto_dial = true;
    cfg_a.auto_dial_interval = Duration::from_millis(50);
    cfg_a.auto_dial_batch = 16;

    let hub_c = Arc::new(A2AHub::new(
        A2AConfig::default(),
        Agent::new("node-c", "node"),
    ));
    let mut cfg_c = P2PConfig::default();
    cfg_c.listen_addrs = vec!["/memory/0".parse().unwrap()];
    cfg_c.bootstrap_peers = vec![dial_b];
    cfg_c.announce_interval = Duration::from_millis(100);
    cfg_c.auto_dial = true;
    cfg_c.auto_dial_interval = Duration::from_millis(50);
    cfg_c.auto_dial_batch = 16;

    let bridge_a = start_p2p_bridge(hub_a, cfg_a).await.unwrap();
    let bridge_c = start_p2p_bridge(hub_c, cfg_c).await.unwrap();

    // Ensure both nodes have reached the bootstrap peer before asserting the mesh.
    let bootstrap_deadline = Instant::now() + Duration::from_secs(3);
    timeout(Duration::from_secs(4), async {
        tokio::join!(
            wait_connected(&bridge_a, bridge_b.peer_id, bootstrap_deadline),
            wait_connected(&bridge_c, bridge_b.peer_id, bootstrap_deadline),
        );
    })
    .await
    .expect("bootstrap connect timeout");

    let deadline = Instant::now() + Duration::from_secs(5);
    timeout(Duration::from_secs(6), async {
        tokio::join!(
            wait_connected(&bridge_a, bridge_c.peer_id, deadline),
            wait_connected(&bridge_c, bridge_a.peer_id, deadline),
        );
    })
    .await
    .expect("auto-dial mesh timeout");

    bridge_a.shutdown().await;
    bridge_c.shutdown().await;
    bridge_b.shutdown().await;
}
