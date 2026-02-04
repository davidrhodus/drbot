//! `drbot-a2a` over a real P2P overlay (libp2p).
//!
//! This crate bridges the in-process [`drbot_a2a::A2AHub`] to a libp2p swarm:
//! - Agent discovery is handled via gossipsub announcements.
//! - Direct agent-to-agent messages are delivered via request-response.
//! - Optional circuit-relay support enables NAT traversal.

pub use libp2p::{Multiaddr, PeerId};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use drbot_a2a::{A2AHub, A2AMessage, Agent};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, StreamExt};
use libp2p::core::Transport;
use libp2p::gossipsub::{self, MessageAuthenticity};
use libp2p::identify;
use libp2p::identity;
use libp2p::kad;
use libp2p::multiaddr::Protocol;
use libp2p::ping;
use libp2p::relay;
use libp2p::request_response;
use libp2p::swarm::behaviour::toggle::Toggle;
use libp2p::swarm::dial_opts::{DialOpts, PeerCondition};
use libp2p::swarm::{NetworkBehaviour, Swarm, SwarmEvent};
use libp2p::{dcutr, SwarmBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use uuid::Uuid;

const ANNOUNCE_TOPIC: &str = "drbot/a2a/announce/v1";
const MSG_PROTOCOL: &str = "/drbot/a2a/msg/1.0.0";
const DHT_SERVICE_KEY: &str = "drbot/a2a/service/v1";

/// P2P configuration for the A2A bridge.
#[derive(Debug, Clone)]
pub struct P2PConfig {
    /// Multiaddrs to listen on.
    pub listen_addrs: Vec<Multiaddr>,
    /// Multiaddrs to dial on startup (bootstrap).
    pub bootstrap_peers: Vec<Multiaddr>,
    /// Optional relay peers to reserve through (circuit relay v2).
    pub relay_peers: Vec<Multiaddr>,
    /// If true, run a relay server on this node.
    pub relay_server: bool,
    /// How often to broadcast agent announcements.
    pub announce_interval: Duration,
    /// Maximum inbound request size.
    pub max_message_bytes: usize,
    /// Optional path to persist the libp2p identity keypair (stable PeerId).
    pub identity_keypair_path: Option<PathBuf>,
    /// If true, automatically dial peers discovered via announcements.
    pub auto_dial: bool,
    /// How often to attempt auto-dialing discovered peers.
    pub auto_dial_interval: Duration,
    /// Max peers to dial per auto-dial tick.
    pub auto_dial_batch: usize,
    /// Optional path to persist a lightweight peer store (addresses learned from announcements).
    pub peer_store_path: Option<PathBuf>,
    /// How often to flush the peer store to disk.
    pub peer_store_flush_interval: Duration,
    /// Maximum number of peers to persist in the store.
    pub peer_store_max_peers: usize,
    /// Enable Kademlia DHT peer discovery.
    pub dht_enabled: bool,
    /// Kademlia "service key" used for provider records.
    pub dht_service_key: String,
    /// How often to (re)announce as a DHT provider.
    pub dht_provide_interval: Duration,
    /// How often to query the DHT for other providers.
    pub dht_query_interval: Duration,
    /// If true, dial peers discovered via DHT provider queries.
    pub dht_dial_providers: bool,
    /// How long to wait for a request-response ack before retrying.
    pub outbound_request_timeout: Duration,
    /// Maximum number of send attempts per outbound message.
    pub outbound_max_retries: u32,
    /// Base delay used for retrying after an immediate outbound failure.
    pub outbound_retry_base_delay: Duration,
}

impl Default for P2PConfig {
    fn default() -> Self {
        Self {
            listen_addrs: vec!["/ip4/0.0.0.0/tcp/0".parse().expect("valid multiaddr")],
            bootstrap_peers: vec![],
            relay_peers: vec![],
            relay_server: false,
            announce_interval: Duration::from_secs(30),
            max_message_bytes: 1024 * 1024,
            identity_keypair_path: None,
            auto_dial: true,
            auto_dial_interval: Duration::from_secs(2),
            auto_dial_batch: 8,
            peer_store_path: None,
            peer_store_flush_interval: Duration::from_secs(30),
            peer_store_max_peers: 2048,
            dht_enabled: true,
            dht_service_key: DHT_SERVICE_KEY.to_string(),
            dht_provide_interval: Duration::from_secs(60),
            dht_query_interval: Duration::from_secs(15),
            dht_dial_providers: true,
            outbound_request_timeout: Duration::from_secs(3),
            outbound_max_retries: 3,
            outbound_retry_base_delay: Duration::from_millis(250),
        }
    }
}

#[derive(Debug, Error)]
pub enum P2PError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("libp2p transport error: {0}")]
    Transport(String),
    #[error("Invalid identity keypair: {0}")]
    InvalidIdentity(String),
    #[error("Command channel closed")]
    CommandChannelClosed,
}

/// Handle to a running P2P bridge.
pub struct P2PBridge {
    pub peer_id: PeerId,
    listen_addrs: Arc<RwLock<Vec<Multiaddr>>>,
    connected_peers: Arc<RwLock<HashSet<PeerId>>>,
    cmd_tx: mpsc::Sender<Command>,
    task: JoinHandle<()>,
}

impl P2PBridge {
    /// Current listen addresses observed by the swarm.
    pub async fn listen_addrs(&self) -> Vec<Multiaddr> {
        self.listen_addrs.read().await.clone()
    }

    /// Currently connected peer IDs observed by the swarm.
    pub async fn connected_peers(&self) -> Vec<PeerId> {
        self.connected_peers.read().await.iter().copied().collect()
    }

    /// Dial an additional peer address.
    pub async fn dial(&self, addr: Multiaddr) -> Result<(), P2PError> {
        self.cmd_tx
            .send(Command::Dial(addr))
            .await
            .map_err(|_| P2PError::CommandChannelClosed)
    }

    /// Stop the bridge task.
    pub async fn shutdown(self) {
        let _ = self.cmd_tx.send(Command::Shutdown).await;
        let _ = self.task.await;
    }
}

/// Start a libp2p-backed A2A bridge for a local hub.
pub async fn start_p2p_bridge(hub: Arc<A2AHub>, config: P2PConfig) -> Result<P2PBridge, P2PError> {
    let keypair = load_or_create_identity(config.identity_keypair_path.as_deref())?;
    let peer_id = PeerId::from(keypair.public());

    let listen_addrs = Arc::new(RwLock::new(Vec::new()));
    let connected_peers = Arc::new(RwLock::new(HashSet::new()));
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>(128);

    let task = {
        let hub = hub.clone();
        let listen_addrs = listen_addrs.clone();
        let connected_peers = connected_peers.clone();
        tokio::spawn(async move {
            if let Err(e) =
                run_swarm(hub, keypair, config, cmd_rx, listen_addrs, connected_peers).await
            {
                warn!(error = %e, "A2A P2P bridge exited with error");
            }
        })
    };

    Ok(P2PBridge {
        peer_id,
        listen_addrs,
        connected_peers,
        cmd_tx,
        task,
    })
}

#[derive(Debug)]
enum Command {
    Dial(Multiaddr),
    Shutdown,
}

fn transport_error<E>(context: &'static str, error: E) -> P2PError
where
    E: std::fmt::Display + std::fmt::Debug,
{
    let display = error.to_string();
    if display.trim().is_empty() {
        return P2PError::Transport(format!("{context}: {error:?}"));
    }
    P2PError::Transport(format!("{context}: {display}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentAnnouncement {
    peer_id: String,
    addrs: Vec<String>,
    sent_at: DateTime<Utc>,
    agents: Vec<Agent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct A2ARequest {
    message: A2AMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct A2AResponse {
    accepted: bool,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct KnownPeer {
    addrs: HashSet<Multiaddr>,
    last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct PendingOutbound {
    peer: PeerId,
    message: A2AMessage,
    attempts: u32,
    last_request_id: request_response::OutboundRequestId,
    last_sent_at: tokio::time::Instant,
    next_retry_at: tokio::time::Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeerStoreFile {
    peers: Vec<PeerStorePeer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeerStorePeer {
    peer_id: String,
    addrs: Vec<String>,
    last_seen: DateTime<Utc>,
}

#[derive(Clone)]
struct A2ACodec {
    max_message_bytes: usize,
}

#[derive(Clone)]
struct A2AProtocol;

impl AsRef<str> for A2AProtocol {
    fn as_ref(&self) -> &str {
        MSG_PROTOCOL
    }
}

#[async_trait]
impl request_response::Codec for A2ACodec {
    type Protocol = A2AProtocol;
    type Request = A2ARequest;
    type Response = A2AResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let bytes = read_framed(io, self.max_message_bytes).await?;
        let req = serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(req)
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let bytes = read_framed(io, self.max_message_bytes).await?;
        let resp = serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(resp)
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> std::io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let bytes = serde_json::to_vec(&req)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        write_framed(io, &bytes).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        resp: Self::Response,
    ) -> std::io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let bytes = serde_json::to_vec(&resp)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        write_framed(io, &bytes).await
    }
}

async fn read_framed<T: AsyncRead + Unpin + Send>(
    io: &mut T,
    max_len: usize,
) -> std::io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > max_len {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame_too_large: {len} > {max_len}"),
        ));
    }
    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_framed<T: AsyncWrite + Unpin + Send>(
    io: &mut T,
    bytes: &[u8],
) -> std::io::Result<()> {
    let len = u32::try_from(bytes.len()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "frame_too_large_u32")
    })?;
    io.write_all(&len.to_be_bytes()).await?;
    io.write_all(bytes).await?;
    io.flush().await?;
    Ok(())
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "BehaviourEvent")]
struct Behaviour {
    gossipsub: gossipsub::Behaviour,
    request_response: request_response::Behaviour<A2ACodec>,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    kademlia: Toggle<kad::Behaviour<kad::store::MemoryStore>>,
    relay_client: relay::client::Behaviour,
    relay_server: Toggle<relay::Behaviour>,
    dcutr: dcutr::Behaviour,
}

#[derive(Debug)]
enum BehaviourEvent {
    Gossipsub(gossipsub::Event),
    RequestResponse(request_response::Event<A2ARequest, A2AResponse>),
    Identify(identify::Event),
    Ping(ping::Event),
    Kademlia(kad::Event),
    RelayClient(relay::client::Event),
    RelayServer(relay::Event),
    Dcutr(dcutr::Event),
}

impl From<gossipsub::Event> for BehaviourEvent {
    fn from(value: gossipsub::Event) -> Self {
        Self::Gossipsub(value)
    }
}

impl From<request_response::Event<A2ARequest, A2AResponse>> for BehaviourEvent {
    fn from(value: request_response::Event<A2ARequest, A2AResponse>) -> Self {
        Self::RequestResponse(value)
    }
}

impl From<identify::Event> for BehaviourEvent {
    fn from(value: identify::Event) -> Self {
        Self::Identify(value)
    }
}

impl From<ping::Event> for BehaviourEvent {
    fn from(value: ping::Event) -> Self {
        Self::Ping(value)
    }
}

impl From<kad::Event> for BehaviourEvent {
    fn from(value: kad::Event) -> Self {
        Self::Kademlia(value)
    }
}

impl From<relay::client::Event> for BehaviourEvent {
    fn from(value: relay::client::Event) -> Self {
        Self::RelayClient(value)
    }
}

impl From<relay::Event> for BehaviourEvent {
    fn from(value: relay::Event) -> Self {
        Self::RelayServer(value)
    }
}

impl From<dcutr::Event> for BehaviourEvent {
    fn from(value: dcutr::Event) -> Self {
        Self::Dcutr(value)
    }
}

fn load_or_create_identity(path: Option<&Path>) -> Result<identity::Keypair, P2PError> {
    let Some(path) = path else {
        return Ok(identity::Keypair::generate_ed25519());
    };

    if let Ok(bytes) = std::fs::read(path) {
        return identity::Keypair::from_protobuf_encoding(&bytes)
            .map_err(|e| P2PError::InvalidIdentity(e.to_string()));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let keypair = identity::Keypair::generate_ed25519();
    let bytes = keypair
        .to_protobuf_encoding()
        .map_err(|e| P2PError::InvalidIdentity(e.to_string()))?;
    std::fs::write(path, &bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(keypair)
}

fn is_dialable_addr(addr: &Multiaddr) -> bool {
    for p in addr.iter() {
        match p {
            Protocol::Ip4(ip) if ip.is_unspecified() => return false,
            Protocol::Ip6(ip) if ip.is_unspecified() => return false,
            _ => {}
        }
    }
    true
}

fn strip_p2p(addr: &Multiaddr) -> Multiaddr {
    let mut out = Multiaddr::empty();
    for p in addr.iter() {
        if matches!(p, Protocol::P2p(_)) {
            break;
        }
        out.push(p);
    }
    out
}

fn split_peer_id(addr: &Multiaddr) -> Option<(PeerId, Multiaddr)> {
    let mut out = Multiaddr::empty();
    for p in addr.iter() {
        match p {
            Protocol::P2p(peer_id) => return Some((peer_id, out)),
            _ => out.push(p),
        }
    }
    None
}

async fn load_peer_store(path: &Path) -> Result<HashMap<PeerId, KnownPeer>, P2PError> {
    let bytes = match tokio::fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(P2PError::Io(e)),
    };

    let store: PeerStoreFile = serde_json::from_slice(&bytes)?;
    let mut peers = HashMap::new();

    for entry in store.peers {
        let peer_id = match entry.peer_id.parse::<PeerId>() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let mut addrs = HashSet::new();
        for addr in entry.addrs {
            let Ok(maddr) = addr.parse::<Multiaddr>() else {
                continue;
            };
            if is_dialable_addr(&maddr) {
                addrs.insert(maddr);
            }
        }

        peers.insert(
            peer_id,
            KnownPeer {
                addrs,
                last_seen: entry.last_seen,
            },
        );
    }

    Ok(peers)
}

async fn save_peer_store(
    path: &Path,
    peers: &HashMap<PeerId, KnownPeer>,
    max_peers: usize,
) -> Result<(), P2PError> {
    let mut entries = peers
        .iter()
        .filter(|(_, p)| !p.addrs.is_empty())
        .map(|(peer_id, p)| PeerStorePeer {
            peer_id: peer_id.to_string(),
            addrs: p.addrs.iter().map(|a| a.to_string()).collect(),
            last_seen: p.last_seen,
        })
        .collect::<Vec<_>>();

    entries.sort_by_key(|e| std::cmp::Reverse(e.last_seen));
    entries.truncate(max_peers);

    for e in &mut entries {
        e.addrs.sort();
        e.addrs.dedup();
    }

    let store = PeerStoreFile { peers: entries };
    let bytes = serde_json::to_vec_pretty(&store)?;

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, bytes).await?;
    Ok(())
}

async fn run_swarm(
    hub: Arc<A2AHub>,
    keypair: identity::Keypair,
    config: P2PConfig,
    mut cmd_rx: mpsc::Receiver<Command>,
    listen_addrs: Arc<RwLock<Vec<Multiaddr>>>,
    connected_peers: Arc<RwLock<HashSet<PeerId>>>,
) -> Result<(), P2PError> {
    let peer_id = PeerId::from(keypair.public());

    let max_message_bytes = config.max_message_bytes;
    let relay_server_enabled = config.relay_server;
    let dht_enabled = config.dht_enabled;
    let dht_service_key = config.dht_service_key.clone();
    let dht_dial_providers = config.dht_dial_providers;

    let listen_addrs_cfg = config.listen_addrs.clone();
    let bootstrap_peers_cfg = config.bootstrap_peers.clone();
    let relay_peers_cfg = config.relay_peers.clone();

    let mut swarm = SwarmBuilder::with_existing_identity(keypair.clone())
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )
        .map_err(|e| transport_error("with_tcp", e))?
        // Include an in-memory transport so we can run fully socket-free integration tests
        // (e.g. in sandboxes that forbid binding/listening on TCP sockets).
        .with_other_transport(|keypair| {
            let noise = libp2p::noise::Config::new(keypair)?;
            Ok(libp2p::core::transport::MemoryTransport::default()
                .upgrade(libp2p::core::upgrade::Version::V1Lazy)
                .authenticate(noise)
                .multiplex(libp2p::yamux::Config::default()))
        })
        .map_err(|e| transport_error("with_other_transport(memory)", e))?
        .with_dns()
        .map_err(|e| transport_error("with_dns", e))?
        .with_relay_client(libp2p::noise::Config::new, libp2p::yamux::Config::default)
        .map_err(|e| transport_error("with_relay_client", e))?
        .with_behaviour(
            move |keypair,
                  relay_client|
                  -> Result<Behaviour, Box<dyn std::error::Error + Send + Sync>> {
                let peer_id = PeerId::from(keypair.public());

                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .validation_mode(gossipsub::ValidationMode::Strict)
                    .build()?;

                let mut gossipsub = gossipsub::Behaviour::new(
                    MessageAuthenticity::Signed(keypair.clone()),
                    gossipsub_config,
                )?;

                let announce_topic = gossipsub::IdentTopic::new(ANNOUNCE_TOPIC);
                gossipsub.subscribe(&announce_topic)?;

                let protocols =
                    std::iter::once((A2AProtocol, request_response::ProtocolSupport::Full));
                let request_response = request_response::Behaviour::with_codec(
                    A2ACodec { max_message_bytes },
                    protocols,
                    request_response::Config::default(),
                );

                let identify = identify::Behaviour::new(identify::Config::new(
                    "drbot-a2a/1.0.0".to_string(),
                    keypair.public(),
                ));

                let ping = ping::Behaviour::new(
                    ping::Config::new().with_interval(Duration::from_secs(15)),
                );

                let kademlia = if dht_enabled {
                    let store = kad::store::MemoryStore::new(peer_id);
                    let mut kad_config = kad::Config::default();
                    kad_config.set_query_timeout(Duration::from_secs(10));
                    let mut kademlia = kad::Behaviour::with_config(peer_id, store, kad_config);
                    kademlia.set_mode(Some(kad::Mode::Server));
                    Toggle::from(Some(kademlia))
                } else {
                    Toggle::from(None)
                };

                let relay_server = if relay_server_enabled {
                    Toggle::from(Some(relay::Behaviour::new(
                        peer_id,
                        relay::Config::default(),
                    )))
                } else {
                    Toggle::from(None)
                };

                let dcutr = dcutr::Behaviour::new(peer_id);

                Ok(Behaviour {
                    gossipsub,
                    request_response,
                    identify,
                    ping,
                    kademlia,
                    relay_client,
                    relay_server,
                    dcutr,
                })
            },
        )
        .map_err(|e| transport_error("with_behaviour", e))?
        .build();

    let announce_topic = gossipsub::IdentTopic::new(ANNOUNCE_TOPIC);

    // Seed Kademlia with known bootstrap / relay peer addresses (when provided with `/p2p/<peer_id>`).
    if dht_enabled {
        if let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() {
            for addr in bootstrap_peers_cfg.iter().chain(relay_peers_cfg.iter()) {
                if let Some((peer, base)) = split_peer_id(addr) {
                    kad.add_address(&peer, base);
                }
            }

            // Start participating immediately.
            let _ = kad.bootstrap();
            let key = kad::RecordKey::new(&dht_service_key);
            if let Err(e) = kad.start_providing(key.clone()) {
                debug!(error = %e, "DHT start_providing failed");
            }
            kad.get_providers(key);
        }
    }

    for addr in listen_addrs_cfg {
        swarm
            .listen_on(addr)
            .map_err(|e| transport_error("listen_on", e))?;
    }

    for addr in &bootstrap_peers_cfg {
        if let Err(e) = swarm.dial(addr.clone()) {
            warn!(error = %e, addr = %addr, "Failed to dial bootstrap peer");
        }
    }

    for addr in &relay_peers_cfg {
        // Reserve an inbound slot on the relay by "listening" on its /p2p-circuit address.
        let relay_listen = addr.clone().with(Protocol::P2pCircuit);
        if let Err(e) = swarm.listen_on(relay_listen) {
            warn!(error = %e, addr = %addr, "Failed to listen via relay");
        }

        // Also dial the relay directly (helps establish / keep the reservation).
        if let Err(e) = swarm.dial(addr.clone()) {
            warn!(error = %e, addr = %addr, "Failed to dial relay peer");
        }
    }

    let mut hub_rx = hub.subscribe();
    let mut known_routes: HashMap<Uuid, PeerId> = HashMap::new();
    let mut connected: HashSet<PeerId> = HashSet::new();
    let mut known_peers: HashMap<PeerId, KnownPeer> = HashMap::new();

    let announce_auto_dial = config.auto_dial;
    let dialer_enabled = announce_auto_dial || (dht_enabled && dht_dial_providers);
    let auto_dial_batch = config.auto_dial_batch;
    let mut auto_dial_tick = tokio::time::interval(config.auto_dial_interval);

    let peer_store_path = config.peer_store_path.clone();
    let peer_store_enabled = peer_store_path.is_some();
    let peer_store_max_peers = config.peer_store_max_peers;
    let mut peer_store_tick = tokio::time::interval(config.peer_store_flush_interval);

    let mut dht_provide_tick = tokio::time::interval(config.dht_provide_interval);
    let mut dht_query_tick = tokio::time::interval(config.dht_query_interval);

    let mut dial_queue: VecDeque<PeerId> = VecDeque::new();
    let mut dial_set: HashSet<PeerId> = HashSet::new();

    if let Some(path) = peer_store_path.as_deref() {
        match load_peer_store(path).await {
            Ok(loaded) => {
                known_peers.extend(loaded);
                if dialer_enabled {
                    let peers: Vec<PeerId> = known_peers.keys().copied().collect();
                    for p in peers {
                        if dial_set.insert(p) {
                            dial_queue.push_back(p);
                        }
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, path = %path.display(), "Failed to load peer store");
            }
        }
    }

    // Feed persisted peers into Kademlia's address book.
    if dht_enabled {
        if let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() {
            for (peer, info) in &known_peers {
                for addr in &info.addrs {
                    kad.add_address(peer, addr.clone());
                }
            }
        }
    }

    // Simple de-dup for request IDs we've recently published (avoid loops).
    let mut recent_msgs: VecDeque<Uuid> = VecDeque::new();
    let mut recent_set: HashSet<Uuid> = HashSet::new();
    let recent_cap = 2048usize;

    // Outbound reliability: track pending requests and retry on failure/timeout.
    let outbound_timeout = config.outbound_request_timeout;
    let outbound_max_retries = config.outbound_max_retries;
    let outbound_retry_base_delay = config.outbound_retry_base_delay;
    let mut outbound_retry_tick = tokio::time::interval(outbound_retry_base_delay);
    let mut pending_by_msg: HashMap<Uuid, PendingOutbound> = HashMap::new();
    let mut pending_by_req: HashMap<request_response::OutboundRequestId, Uuid> = HashMap::new();

    let mut announce_tick = tokio::time::interval(config.announce_interval);

    info!(%peer_id, "A2A P2P swarm started");

    loop {
        tokio::select! {
            _ = announce_tick.tick() => {
                if let Err(e) = announce_local_agents(&hub, &mut swarm, &announce_topic).await {
                    warn!(error = %e, "Failed to announce agents");
                }
            }
            _ = auto_dial_tick.tick(), if dialer_enabled => {
                let mut dialed = 0usize;
                while dialed < auto_dial_batch {
                    let Some(peer) = dial_queue.pop_front() else {
                        break;
                    };
                    dial_set.remove(&peer);

                    if peer == peer_id || connected.contains(&peer) {
                        continue;
                    }

                    let addrs = known_peers
                        .get(&peer)
                        .map(|i| i.addrs.iter().cloned().collect::<Vec<_>>())
                        .unwrap_or_default();

                    let opts = if addrs.is_empty() {
                        DialOpts::peer_id(peer)
                            .condition(PeerCondition::DisconnectedAndNotDialing)
                            .build()
                    } else {
                        DialOpts::peer_id(peer)
                            .condition(PeerCondition::DisconnectedAndNotDialing)
                            .addresses(addrs)
                            .build()
                    };
                    if let Err(e) = swarm.dial(opts) {
                        debug!(error = %e, to_peer = %peer, "Auto-dial failed");
                    } else {
                        dialed += 1;
                    }
                }
            }
            _ = dht_provide_tick.tick(), if dht_enabled => {
                if let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() {
                    let key = kad::RecordKey::new(&dht_service_key);
                    if let Err(e) = kad.start_providing(key) {
                        debug!(error = %e, "DHT start_providing failed");
                    }
                }
            }
            _ = dht_query_tick.tick(), if dht_enabled => {
                if let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() {
                    let key = kad::RecordKey::new(&dht_service_key);
                    kad.get_providers(key);
                }
            }
            _ = peer_store_tick.tick(), if peer_store_enabled => {
                if let Some(path) = peer_store_path.as_deref() {
                    if let Err(e) = save_peer_store(path, &known_peers, peer_store_max_peers).await {
                        warn!(error = %e, path = %path.display(), "Failed to save peer store");
                    }
                }
            }
            _ = outbound_retry_tick.tick(), if !pending_by_msg.is_empty() => {
                let now = tokio::time::Instant::now();
                let mut to_retry: Vec<Uuid> = Vec::new();
                let mut to_drop: Vec<Uuid> = Vec::new();

                for (id, pending) in pending_by_msg.iter() {
                    if now < pending.next_retry_at {
                        continue;
                    }
                    if pending.attempts >= outbound_max_retries {
                        to_drop.push(*id);
                    } else {
                        to_retry.push(*id);
                    }
                }

                for id in to_drop {
                    if let Some(p) = pending_by_msg.remove(&id) {
                        pending_by_req.remove(&p.last_request_id);
                        debug!(msg_id = %id, to_peer = %p.peer, attempts = p.attempts, "Dropping outbound message after retries");
                    }
                }

                for id in to_retry {
                    let Some(pending) = pending_by_msg.get_mut(&id) else {
                        continue;
                    };

                    pending_by_req.remove(&pending.last_request_id);
                    let req_id = swarm
                        .behaviour_mut()
                        .request_response
                        .send_request(&pending.peer, A2ARequest { message: pending.message.clone() });

                    pending.attempts = pending.attempts.saturating_add(1);
                    pending.last_request_id = req_id;
                    pending.last_sent_at = now;
                    pending.next_retry_at = now + outbound_timeout;
                    pending_by_req.insert(req_id, id);

                    debug!(msg_id = %id, to_peer = %pending.peer, attempt = pending.attempts, "Retrying outbound message");
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(Command::Dial(addr)) => {
                        if let Err(e) = swarm.dial(addr.clone()) {
                            warn!(error = %e, addr = %addr, "Dial failed");
                        }
                    }
                    Some(Command::Shutdown) | None => break,
                }
            }
            msg = hub_rx.recv() => {
                let msg = match msg {
                    Ok(m) => m,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };

                // Only forward if we have a known remote route for the recipient agent.
                let Some(peer) = known_routes.get(&msg.to).copied() else {
                    continue;
                };

                // Avoid re-sending messages we recently handled (best-effort dedup).
                if recent_set.contains(&msg.id) {
                    continue;
                }
                push_recent(msg.id, recent_cap, &mut recent_msgs, &mut recent_set);

                if !connected.contains(&peer) {
                    // If not directly connected yet, request-response will connect on-demand if possible.
                    // We rely on bootstrap/relay connectivity; explicit dial can be added by callers.
                    debug!(to_peer = %peer, "Sending to possibly-not-connected peer");
                }

                // If already pending (e.g. local duplicates), do not enqueue another send.
                if pending_by_msg.contains_key(&msg.id) {
                    continue;
                }

                let now = tokio::time::Instant::now();
                let req_id = swarm
                    .behaviour_mut()
                    .request_response
                    .send_request(&peer, A2ARequest { message: msg.clone() });

                pending_by_req.insert(req_id, msg.id);
                pending_by_msg.insert(
                    msg.id,
                    PendingOutbound {
                        peer,
                        message: msg,
                        attempts: 1,
                        last_request_id: req_id,
                        last_sent_at: now,
                        next_retry_at: now + outbound_timeout,
                    },
                );
            }
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!(addr = %address, "Listening");
                        listen_addrs.write().await.push(address);
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        connected.insert(peer_id);
                        connected_peers.write().await.insert(peer_id);
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        connected.remove(&peer_id);
                        connected_peers.write().await.remove(&peer_id);
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(ev)) => {
                        handle_gossipsub_event(
                            &hub,
                            &mut swarm,
                            peer_id,
                            ev,
                            &mut known_routes,
                            &mut known_peers,
                            &mut dial_queue,
                            &mut dial_set,
                            announce_auto_dial,
                        )
                        .await;
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(ev)) => {
                        handle_request_response_event(
                            &hub,
                            &mut swarm,
                            ev,
                            &mut known_routes,
                            &mut recent_msgs,
                            &mut recent_set,
                            recent_cap,
                            &mut pending_by_msg,
                            &mut pending_by_req,
                            outbound_retry_base_delay,
                        )
                        .await;
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Identify(ev)) => {
                        handle_identify_event(
                            &mut swarm,
                            peer_id,
                            ev,
                            &mut known_peers,
                            &mut dial_queue,
                            &mut dial_set,
                            dialer_enabled,
                        )
                        .await;
                    }
                    SwarmEvent::Behaviour(BehaviourEvent::Kademlia(ev)) => {
                        handle_kademlia_event(
                            &mut swarm,
                            peer_id,
                            ev,
                            &mut known_peers,
                            &mut dial_queue,
                            &mut dial_set,
                            dht_dial_providers,
                        )
                        .await;
                    }
                    _ => {}
                }
            }
        }
    }

    // Best-effort flush on shutdown.
    if let Some(path) = peer_store_path.as_deref() {
        if let Err(e) = save_peer_store(path, &known_peers, peer_store_max_peers).await {
            warn!(error = %e, path = %path.display(), "Failed to save peer store on shutdown");
        }
    }

    Ok(())
}

async fn announce_local_agents(
    hub: &A2AHub,
    swarm: &mut Swarm<Behaviour>,
    topic: &gossipsub::IdentTopic,
) -> Result<(), P2PError> {
    let peer_id = *swarm.local_peer_id();

    // Agents lacking the p2p peer id metadata are considered "local" to this process.
    let agents = hub
        .list_agents()
        .await
        .into_iter()
        .filter(|a| !a.metadata.contains_key("p2p_peer_id"))
        .collect::<Vec<_>>();

    let mut addrs = HashSet::new();
    for addr in swarm.listeners() {
        if is_dialable_addr(addr) {
            addrs.insert(addr.to_string());
        }
    }
    for addr in swarm.external_addresses() {
        if is_dialable_addr(addr) {
            addrs.insert(addr.to_string());
        }
    }
    let mut addrs: Vec<String> = addrs.into_iter().collect();
    addrs.sort();

    let announcement = AgentAnnouncement {
        peer_id: peer_id.to_string(),
        addrs,
        sent_at: Utc::now(),
        agents,
    };

    let bytes = serde_json::to_vec(&announcement)?;
    let _ = swarm
        .behaviour_mut()
        .gossipsub
        .publish(topic.clone(), bytes)
        .map_err(|e| transport_error("gossipsub.publish", e))?;

    Ok(())
}

async fn handle_gossipsub_event(
    hub: &Arc<A2AHub>,
    swarm: &mut Swarm<Behaviour>,
    local_peer: PeerId,
    event: gossipsub::Event,
    routes: &mut HashMap<Uuid, PeerId>,
    peers: &mut HashMap<PeerId, KnownPeer>,
    dial_queue: &mut VecDeque<PeerId>,
    dial_set: &mut HashSet<PeerId>,
    auto_dial: bool,
) {
    let gossipsub::Event::Message {
        propagation_source: _propagation_source,
        message_id: _,
        message,
    } = event
    else {
        return;
    };

    let Some(publisher) = message.source else {
        return;
    };

    if publisher == local_peer {
        return;
    }

    let announcement: AgentAnnouncement = match serde_json::from_slice(&message.data) {
        Ok(a) => a,
        Err(e) => {
            debug!(error = %e, "Invalid announcement payload");
            return;
        }
    };

    // Best-effort: if the payload contains a peer id, ensure it matches the signed publisher.
    if let Ok(payload_peer) = announcement.peer_id.parse::<PeerId>() {
        if payload_peer != publisher {
            debug!(payload_peer_id = %payload_peer, publisher_peer_id = %publisher, "Ignoring spoofed peer id in announcement payload");
        }
    }

    let now = Utc::now();
    let peer_entry = peers.entry(publisher).or_insert_with(|| KnownPeer {
        addrs: HashSet::new(),
        last_seen: now,
    });
    peer_entry.last_seen = now;

    // Store any advertised addresses to enable dialing for request-response.
    for addr in &announcement.addrs {
        if let Ok(maddr) = addr.parse::<Multiaddr>() {
            let maddr = strip_p2p(&maddr);
            if !is_dialable_addr(&maddr) {
                continue;
            }
            swarm.add_peer_address(publisher, maddr.clone());
            if let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() {
                kad.add_address(&publisher, maddr.clone());
            }
            peer_entry.addrs.insert(maddr);
        }
    }

    if auto_dial && !peer_entry.addrs.is_empty() && dial_set.insert(publisher) {
        dial_queue.push_back(publisher);
    }

    // Register advertised agents into the local hub and track routes.
    for mut agent in announcement.agents {
        routes.insert(agent.id, publisher);

        agent.endpoint = Some(format!("p2p:{publisher}"));
        agent.last_seen = Utc::now();
        agent.status = drbot_a2a::AgentStatus::Online;
        agent.metadata.insert(
            "p2p_peer_id".to_string(),
            serde_json::Value::String(publisher.to_string()),
        );
        agent.metadata.insert(
            "p2p_addrs".to_string(),
            serde_json::Value::Array(
                announcement
                    .addrs
                    .iter()
                    .map(|a| serde_json::Value::String(a.clone()))
                    .collect(),
            ),
        );

        hub.register_agent(agent).await;
    }
}

async fn handle_identify_event(
    swarm: &mut Swarm<Behaviour>,
    local_peer: PeerId,
    event: identify::Event,
    peers: &mut HashMap<PeerId, KnownPeer>,
    dial_queue: &mut VecDeque<PeerId>,
    dial_set: &mut HashSet<PeerId>,
    dialer_enabled: bool,
) {
    let (peer, info) = match event {
        identify::Event::Received { peer_id, info } => (peer_id, info),
        identify::Event::Pushed { peer_id, info } => (peer_id, info),
        identify::Event::Error { peer_id, error } => {
            debug!(to_peer = %peer_id, error = %error, "Identify error");
            return;
        }
        _ => return,
    };

    if peer == local_peer {
        return;
    }

    let now = Utc::now();
    let entry = peers.entry(peer).or_insert_with(|| KnownPeer {
        addrs: HashSet::new(),
        last_seen: now,
    });
    entry.last_seen = now;

    for addr in info.listen_addrs {
        let addr = strip_p2p(&addr);
        if !is_dialable_addr(&addr) {
            continue;
        }
        swarm.add_peer_address(peer, addr.clone());
        if let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() {
            kad.add_address(&peer, addr.clone());
        }
        entry.addrs.insert(addr);
    }

    // If this was the first time we learned dialable addrs, enqueue a dial attempt.
    if dialer_enabled && !entry.addrs.is_empty() && dial_set.insert(peer) {
        dial_queue.push_back(peer);
    }
}

async fn handle_kademlia_event(
    swarm: &mut Swarm<Behaviour>,
    local_peer: PeerId,
    event: kad::Event,
    peers: &mut HashMap<PeerId, KnownPeer>,
    dial_queue: &mut VecDeque<PeerId>,
    dial_set: &mut HashSet<PeerId>,
    dial_providers: bool,
) {
    match event {
        kad::Event::RoutingUpdated {
            peer, addresses, ..
        } => {
            if peer == local_peer {
                return;
            }

            let now = Utc::now();
            let entry = peers.entry(peer).or_insert_with(|| KnownPeer {
                addrs: HashSet::new(),
                last_seen: now,
            });
            entry.last_seen = now;

            for addr in addresses.into_vec() {
                let addr = strip_p2p(&addr);
                if !is_dialable_addr(&addr) {
                    continue;
                }
                swarm.add_peer_address(peer, addr.clone());
                if let Some(kad) = swarm.behaviour_mut().kademlia.as_mut() {
                    kad.add_address(&peer, addr.clone());
                }
                entry.addrs.insert(addr);
            }
        }
        kad::Event::OutboundQueryProgressed { result, .. } => match result {
            kad::QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders {
                providers,
                ..
            })) => {
                for provider in providers {
                    if provider == local_peer {
                        continue;
                    }

                    let now = Utc::now();
                    peers.entry(provider).or_insert_with(|| KnownPeer {
                        addrs: HashSet::new(),
                        last_seen: now,
                    });

                    if dial_providers && dial_set.insert(provider) {
                        dial_queue.push_back(provider);
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
}

async fn handle_request_response_event(
    hub: &Arc<A2AHub>,
    swarm: &mut Swarm<Behaviour>,
    event: request_response::Event<A2ARequest, A2AResponse>,
    routes: &mut HashMap<Uuid, PeerId>,
    recent_msgs: &mut VecDeque<Uuid>,
    recent_set: &mut HashSet<Uuid>,
    recent_cap: usize,
    pending_by_msg: &mut HashMap<Uuid, PendingOutbound>,
    pending_by_req: &mut HashMap<request_response::OutboundRequestId, Uuid>,
    outbound_retry_base_delay: Duration,
) {
    match event {
        request_response::Event::Message { peer, message } => match message {
            request_response::Message::Request {
                request, channel, ..
            } => {
                let msg = request.message;
                let msg_id = msg.id;
                // Opportunistically learn a route for the sender (helps reply before announcements arrive).
                routes.insert(msg.from, peer);

                // Dedup: if we've already delivered this message to the local hub, ack it again.
                // This covers the case where the sender retried due to a lost response.
                if recent_set.contains(&msg_id) {
                    if swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(
                            channel,
                            A2AResponse {
                                accepted: true,
                                error: None,
                            },
                        )
                        .is_err()
                    {
                        debug!(from_peer = %peer, "Failed to send A2A response");
                    }
                    return;
                }

                let deliver = hub.send(msg).await;
                let resp = match deliver {
                    Ok(()) => {
                        // Mark as delivered only after hub acceptance so retries can work.
                        push_recent(msg_id, recent_cap, recent_msgs, recent_set);
                        A2AResponse {
                            accepted: true,
                            error: None,
                        }
                    }
                    Err(e) => A2AResponse {
                        accepted: false,
                        error: Some(e.to_string()),
                    },
                };
                if swarm
                    .behaviour_mut()
                    .request_response
                    .send_response(channel, resp)
                    .is_err()
                {
                    debug!(from_peer = %peer, "Failed to send A2A response");
                }
            }
            request_response::Message::Response {
                request_id,
                response,
            } => {
                if let Some(msg_id) = pending_by_req.remove(&request_id) {
                    if response.accepted {
                        pending_by_msg.remove(&msg_id);
                    } else if let Some(pending) = pending_by_msg.get_mut(&msg_id) {
                        let now = tokio::time::Instant::now();
                        pending.next_retry_at = now + outbound_retry_base_delay;
                    }
                }

                debug!(
                    from_peer = %peer,
                    accepted = response.accepted,
                    error = ?response.error,
                    "A2A response"
                );
            }
        },
        request_response::Event::OutboundFailure {
            peer,
            request_id,
            error,
        } => {
            if let Some(msg_id) = pending_by_req.remove(&request_id) {
                if let Some(pending) = pending_by_msg.get_mut(&msg_id) {
                    let now = tokio::time::Instant::now();
                    pending.next_retry_at = now + outbound_retry_base_delay;
                }
            }
            debug!(to_peer = %peer, error = %error, "A2A outbound failure");
        }
        request_response::Event::InboundFailure { peer, error, .. } => {
            debug!(from_peer = %peer, error = %error, "A2A inbound failure");
        }
        request_response::Event::ResponseSent { peer, .. } => {
            debug!(to_peer = %peer, "A2A response sent");
        }
    }
}

fn push_recent(id: Uuid, cap: usize, q: &mut VecDeque<Uuid>, set: &mut HashSet<Uuid>) {
    if set.insert(id) {
        q.push_back(id);
        while q.len() > cap {
            if let Some(evicted) = q.pop_front() {
                set.remove(&evicted);
            }
        }
    }
}
