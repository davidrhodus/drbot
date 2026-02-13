use drbot_a2a::{A2AConfig, A2AHub, A2AMessage, Agent, Capability, MessageType};
use drbot_a2a_p2p::{start_p2p_bridge, P2PConfig};
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use serde_json::json;
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

async fn wait_for_agent(hub: &A2AHub, agent_id: uuid::Uuid) -> drbot_a2a::Agent {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(agent) = hub
            .list_agents()
            .await
            .into_iter()
            .find(|a| a.id == agent_id)
        {
            return agent;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for agent {agent_id}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn p2p_memory_transport_discovers_and_forwards_messages() {
    let agent_a = Agent::new("node-a", "test_node")
        .with_capability(Capability::new("test_cap", "test capability"));
    let agent_b = Agent::new("node-b", "test_node")
        .with_capability(Capability::new("test_cap", "test capability"));

    let hub_a = Arc::new(A2AHub::new(A2AConfig::default(), agent_a.clone()));
    let hub_b = Arc::new(A2AHub::new(A2AConfig::default(), agent_b.clone()));

    let mut cfg_a = P2PConfig::default();
    cfg_a.listen_addrs = vec!["/memory/0".parse().unwrap()];
    cfg_a.announce_interval = Duration::from_millis(150);

    let mut cfg_b = P2PConfig::default();
    cfg_b.listen_addrs = vec!["/memory/0".parse().unwrap()];
    cfg_b.announce_interval = Duration::from_millis(150);

    let bridge_a = start_p2p_bridge(hub_a.clone(), cfg_a).await.unwrap();
    let bridge_b = start_p2p_bridge(hub_b.clone(), cfg_b).await.unwrap();

    let addr_a = wait_for_listen_addr(&bridge_a).await;
    let addr_b = wait_for_listen_addr(&bridge_b).await;

    let dial_a = addr_a.with(Protocol::P2p(bridge_a.peer_id.into()));
    let dial_b = addr_b.with(Protocol::P2p(bridge_b.peer_id.into()));

    bridge_a.dial(dial_b).await.unwrap();
    bridge_b.dial(dial_a).await.unwrap();

    // Wait for discovery in both directions.
    let a_sees_b = wait_for_agent(&hub_a, agent_b.id).await;
    let b_sees_a = wait_for_agent(&hub_b, agent_a.id).await;

    assert_eq!(a_sees_b.endpoint, Some(format!("p2p:{}", bridge_b.peer_id)));
    assert_eq!(b_sees_a.endpoint, Some(format!("p2p:{}", bridge_a.peer_id)));

    // Validate direct delivery (request-response).
    let mut rx_b = hub_b.subscribe();
    let msg = A2AMessage::new(
        agent_a.id,
        agent_b.id,
        MessageType::Notification,
        json!({"kind":"test","pair":"SOL/USDC","side":"buy","amount":1}),
    );
    hub_a.send(msg.clone()).await.unwrap();

    let got = timeout(Duration::from_secs(5), async {
        loop {
            match rx_b.recv().await {
                Ok(m) => break m,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(e) => panic!("hub receiver error: {e}"),
            }
        }
    })
    .await
    .expect("message receive timeout");

    assert_eq!(got.id, msg.id);
    assert_eq!(got.from, msg.from);
    assert_eq!(got.to, msg.to);
    assert_eq!(got.message_type, msg.message_type);
    assert_eq!(got.payload, msg.payload);

    bridge_a.shutdown().await;
    bridge_b.shutdown().await;
}

#[tokio::test]
async fn p2p_retries_outbound_messages_across_peer_restart() {
    let agent_a = Agent::new("node-a", "test_node")
        .with_capability(Capability::new("test_cap", "test capability"));
    let agent_b = Agent::new("node-b", "test_node")
        .with_capability(Capability::new("test_cap", "test capability"));

    let hub_a = Arc::new(A2AHub::new(A2AConfig::default(), agent_a.clone()));
    let hub_b = Arc::new(A2AHub::new(A2AConfig::default(), agent_b.clone()));

    let identity_path =
        std::env::temp_dir().join(format!("drbot-a2a-p2p-test-{}.key", uuid::Uuid::new_v4()));

    let mut cfg_a = P2PConfig::default();
    cfg_a.listen_addrs = vec!["/memory/0".parse().unwrap()];
    cfg_a.announce_interval = Duration::from_millis(100);
    cfg_a.outbound_request_timeout = Duration::from_millis(150);
    cfg_a.outbound_max_retries = 20;
    cfg_a.outbound_retry_base_delay = Duration::from_millis(50);

    let mut cfg_b_base = P2PConfig::default();
    cfg_b_base.listen_addrs = vec!["/memory/0".parse().unwrap()];
    cfg_b_base.announce_interval = Duration::from_millis(100);
    cfg_b_base.identity_keypair_path = Some(identity_path.clone());

    let bridge_a = start_p2p_bridge(hub_a.clone(), cfg_a).await.unwrap();
    let bridge_b = start_p2p_bridge(hub_b.clone(), cfg_b_base.clone())
        .await
        .unwrap();

    let addr_a = wait_for_listen_addr(&bridge_a).await;
    let addr_b = wait_for_listen_addr(&bridge_b).await;
    let peer_b = bridge_b.peer_id;

    let dial_a = addr_a.with(Protocol::P2p(bridge_a.peer_id.into()));
    let dial_b = addr_b.clone().with(Protocol::P2p(peer_b.into()));

    bridge_a.dial(dial_b).await.unwrap();
    bridge_b.dial(dial_a).await.unwrap();

    wait_for_agent(&hub_a, agent_b.id).await;
    wait_for_agent(&hub_b, agent_a.id).await;

    // Simulate a crash/restart of node B.
    bridge_b.shutdown().await;

    let mut rx_b = hub_b.subscribe();

    // Send while B is offline; A should retry until B comes back.
    let msg = A2AMessage::new(
        agent_a.id,
        agent_b.id,
        MessageType::Notification,
        json!({"kind":"retry-test","pair":"SOL/USDC","n":1}),
    );
    hub_a.send(msg.clone()).await.unwrap();

    tokio::time::sleep(Duration::from_millis(250)).await;

    let bridge_b2 = start_p2p_bridge(hub_b.clone(), cfg_b_base).await.unwrap();
    assert_eq!(bridge_b2.peer_id, peer_b);

    let addr_b2 = wait_for_listen_addr(&bridge_b2).await;

    // Ensure we reconnect quickly (helps deterministic delivery in CI).
    let dial_b2 = addr_b2.with(Protocol::P2p(peer_b.into()));
    bridge_a.dial(dial_b2).await.unwrap();

    let got = timeout(Duration::from_secs(5), async {
        loop {
            match rx_b.recv().await {
                Ok(m) => break m,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(e) => panic!("hub receiver error: {e}"),
            }
        }
    })
    .await
    .expect("message receive timeout");

    assert_eq!(got.id, msg.id);
    assert_eq!(got.from, msg.from);
    assert_eq!(got.to, msg.to);
    assert_eq!(got.message_type, msg.message_type);
    assert_eq!(got.payload, msg.payload);

    bridge_a.shutdown().await;
    bridge_b2.shutdown().await;
    let _ = tokio::fs::remove_file(&identity_path).await;
}
