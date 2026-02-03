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
