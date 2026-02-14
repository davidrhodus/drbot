//! WebChat server implementation.

use async_trait::async_trait;
use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        ConnectInfo, Query, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use drbot_channels::Channel;
use drbot_core::config::WebChatConfig;
use drbot_core::message::{Content, IncomingMessage, MessageSender, OutgoingMessage};
use drbot_core::Config;
use drbot_core::Result;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
struct WebChatSecurity {
    require_auth: bool,
    auth_token: Option<String>,
}

#[derive(Debug)]
enum WsRejection {
    MissingToken,
    InvalidToken,
    MissingOrigin,
    InvalidOrigin,
}

impl WsRejection {
    fn status_code(&self) -> StatusCode {
        match self {
            WsRejection::MissingToken => StatusCode::UNAUTHORIZED,
            WsRejection::InvalidToken => StatusCode::FORBIDDEN,
            WsRejection::MissingOrigin | WsRejection::InvalidOrigin => StatusCode::FORBIDDEN,
        }
    }

    fn message(&self) -> &'static str {
        match self {
            WsRejection::MissingToken => "missing token",
            WsRejection::InvalidToken => "invalid token",
            WsRejection::MissingOrigin => "missing origin",
            WsRejection::InvalidOrigin => "invalid origin",
        }
    }
}

/// Connected client state.
struct ConnectedClient {
    /// Channel to send messages to this client.
    sender: mpsc::Sender<String>,
}

/// Shared state for the WebChat server.
struct WebChatState {
    /// Connected clients.
    clients: RwLock<HashMap<Uuid, ConnectedClient>>,
    /// Broadcast sender for incoming messages.
    incoming_tx: broadcast::Sender<IncomingMessage>,
    /// Security settings.
    security: WebChatSecurity,
}

/// WebChat channel implementation.
pub struct WebChatChannel {
    /// Configuration.
    config: WebChatConfig,
    /// Shared state.
    state: Arc<WebChatState>,
    /// Server task handle.
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl WebChatChannel {
    /// Create a new WebChat channel.
    pub fn new(config: WebChatConfig) -> Self {
        Self::new_with_gateway_auth_token(config, None)
    }

    /// Create a new WebChat channel, falling back to `gateway_auth_token` when `config.auth_token` is unset.
    pub fn new_with_gateway_auth_token(
        config: WebChatConfig,
        gateway_auth_token: Option<String>,
    ) -> Self {
        let (incoming_tx, _) = broadcast::channel(256);
        let require_auth = config.require_auth;
        let auth_token = normalize_token(config.auth_token.clone())
            .or_else(|| normalize_token(gateway_auth_token));
        Self {
            config,
            state: Arc::new(WebChatState {
                clients: RwLock::new(HashMap::new()),
                incoming_tx,
                security: WebChatSecurity {
                    require_auth,
                    auth_token,
                },
            }),
            server_handle: None,
        }
    }

    /// Create with default configuration.
    pub fn default_config() -> Self {
        Self::new(WebChatConfig::default())
    }

    /// Create from the main drbot config, falling back to `gateway.auth_token` when
    /// `channels.webchat.auth_token` is unset.
    pub fn from_core_config(config: &Config) -> Option<Self> {
        let webchat = config.channels.webchat.clone()?;
        Some(Self::new_with_gateway_auth_token(
            webchat,
            config.gateway.auth_token.clone(),
        ))
    }

    /// Get the server URL.
    pub fn url(&self) -> String {
        format!("http://{}:{}", self.config.host, self.config.port)
    }
}

#[async_trait]
impl Channel for WebChatChannel {
    async fn connect(&mut self) -> Result<()> {
        let bind_ip: IpAddr = self.config.host.parse().map_err(|e| {
            drbot_core::Error::Config(format!(
                "Invalid WebChat host '{}': {}",
                self.config.host, e
            ))
        })?;

        if !self.config.require_auth && !bind_ip.is_loopback() {
            return Err(drbot_core::Error::Config(format!(
                "Refusing to start WebChat without auth on non-loopback host '{}'. Set channels.webchat.require_auth=true (and an auth token) or bind to 127.0.0.1.",
                self.config.host
            )));
        }

        if self.config.require_auth && self.state.security.auth_token.is_none() {
            return Err(drbot_core::Error::Config(
                "WebChat auth is enabled but no token is configured. Set channels.webchat.auth_token or gateway.auth_token."
                    .to_string(),
            ));
        }

        let addr = SocketAddr::new(bind_ip, self.config.port);

        let state = self.state.clone();

        let app = Router::new()
            .route("/", get(serve_chat_html))
            .route("/ws", get(websocket_handler))
            .with_state(state);

        info!(address = %addr, "Starting WebChat server");

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| drbot_core::Error::Io(e))?;

        let handle = tokio::spawn(async move {
            if let Err(e) = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            {
                warn!(error = %e, "WebChat server error");
            }
        });

        self.server_handle = Some(handle);

        info!(url = %self.url(), "WebChat server started");
        Ok(())
    }

    async fn send(&self, to: &str, message: OutgoingMessage) -> Result<()> {
        let client_id: Uuid = to
            .parse()
            .map_err(|_| drbot_core::Error::InvalidInput(format!("Invalid client ID: {}", to)))?;

        let text = message
            .content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        let clients = self.state.clients.read().await;
        if let Some(client) = clients.get(&client_id) {
            let ws_msg = serde_json::json!({
                "type": "message",
                "data": {
                    "id": Uuid::new_v4().to_string(),
                    "content": text,
                    "role": "assistant"
                }
            });

            let _ = client
                .sender
                .send(serde_json::to_string(&ws_msg).unwrap())
                .await;
            Ok(())
        } else {
            Err(drbot_core::Error::NotFound(format!(
                "Client not found: {}",
                client_id
            )))
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.state.incoming_tx.subscribe()
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }
        info!("WebChat server stopped");
        Ok(())
    }

    fn channel_type(&self) -> &str {
        "webchat"
    }
}

async fn serve_chat_html() -> impl IntoResponse {
    Html(crate::CHAT_HTML)
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<WebChatState>>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let token = query.get("token").map(|t| t.as_str());
    match authorize_websocket(&state.security, remote_addr, &headers, token) {
        Ok(()) => ws.on_upgrade(move |socket| handle_websocket(socket, state)),
        Err(rejection) => {
            let origin = headers
                .get(header::ORIGIN)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("<missing>");
            let host = headers
                .get(header::HOST)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("<missing>");
            warn!(
                remote_addr = %remote_addr,
                origin = %origin,
                host = %host,
                reason = %rejection.message(),
                "Rejected WebChat WebSocket connection"
            );

            (rejection.status_code(), rejection.message()).into_response()
        }
    }
}

fn normalize_token(token: Option<String>) -> Option<String> {
    token.and_then(|t| {
        let trimmed = t.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn bearer_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let value = value.trim();
    let token = value.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn parse_origin(origin: &str) -> Option<(String, u16)> {
    if origin.eq_ignore_ascii_case("null") {
        return None;
    }

    let uri: axum::http::Uri = origin.parse().ok()?;
    let authority = uri.authority()?;
    let host = authority.host().to_ascii_lowercase();
    let port = match authority.port_u16() {
        Some(p) => p,
        None => match uri.scheme_str()? {
            "https" => 443,
            "http" => 80,
            _ => return None,
        },
    };

    Some((host, port))
}

fn parse_host_header(host: &str) -> Option<(String, Option<u16>)> {
    let authority: axum::http::uri::Authority = host.parse().ok()?;
    Some((authority.host().to_ascii_lowercase(), authority.port_u16()))
}

fn origin_matches_host_header(origin: &str, host_header: &str) -> bool {
    let Some((origin_host, origin_port)) = parse_origin(origin) else {
        return false;
    };
    let Some((host, host_port)) = parse_host_header(host_header) else {
        return false;
    };

    if origin_host != host {
        return false;
    }

    match host_port {
        Some(p) => p == origin_port,
        None => true,
    }
}

fn authorize_websocket(
    security: &WebChatSecurity,
    remote_addr: SocketAddr,
    headers: &HeaderMap,
    query_token: Option<&str>,
) -> std::result::Result<(), WsRejection> {
    if security.require_auth {
        let provided = query_token
            .map(|t| t.trim().to_string())
            .and_then(|t| if t.is_empty() { None } else { Some(t) })
            .or_else(|| bearer_token_from_headers(headers));

        let expected = security.auth_token.as_deref().unwrap_or("");
        if expected.is_empty() {
            return Err(WsRejection::MissingToken);
        }

        match provided {
            Some(t) if t == expected => {}
            Some(_) => return Err(WsRejection::InvalidToken),
            None => return Err(WsRejection::MissingToken),
        }
    }

    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    match origin {
        Some(origin) => {
            let host_header = headers
                .get(header::HOST)
                .and_then(|v| v.to_str().ok())
                .ok_or(WsRejection::InvalidOrigin)?;

            if !origin_matches_host_header(origin, host_header) {
                return Err(WsRejection::InvalidOrigin);
            }
        }
        None => {
            // Browser clients always send Origin; allow missing Origin only:
            // - for local non-browser clients when auth is off, or
            // - for token-authenticated connections (handled above).
            if !security.require_auth && !remote_addr.ip().is_loopback() {
                return Err(WsRejection::MissingOrigin);
            }
        }
    }

    Ok(())
}

async fn handle_websocket(socket: WebSocket, state: Arc<WebChatState>) {
    use futures::{SinkExt, StreamExt};

    let client_id = Uuid::new_v4();
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<String>(32);

    info!(client_id = %client_id, "WebChat client connected");

    // Register client
    {
        let mut clients = state.clients.write().await;
        clients.insert(client_id, ConnectedClient { sender: tx });
    }

    // Send connected event
    let connected_event = serde_json::json!({
        "type": "event",
        "event": {
            "event_type": "system.connected",
            "data": {
                "client_id": client_id.to_string()
            }
        }
    });
    let _ = ws_sender
        .send(axum::extract::ws::Message::Text(
            serde_json::to_string(&connected_event).unwrap().into(),
        ))
        .await;

    // Task to forward messages from channel to websocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender
                .send(axum::extract::ws::Message::Text(msg.into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Process incoming messages
    let incoming_tx = state.incoming_tx.clone();
    while let Some(msg_result) = ws_receiver.next().await {
        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                debug!(error = %e, "WebSocket error");
                break;
            }
        };

        let text = match msg {
            axum::extract::ws::Message::Text(t) => t.to_string(),
            axum::extract::ws::Message::Close(_) => break,
            _ => continue,
        };

        // Parse the message
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(content) = parsed
                .get("message")
                .or_else(|| parsed.get("content"))
                .and_then(|v| v.as_str())
            {
                let incoming = IncomingMessage {
                    id: Uuid::new_v4(),
                    channel_type: "webchat".to_string(),
                    channel_id: client_id.to_string(),
                    sender: MessageSender {
                        id: client_id.to_string(),
                        name: None,
                        username: None,
                    },
                    content: vec![Content::Text {
                        text: content.to_string(),
                    }],
                    received_at: chrono::Utc::now(),
                    raw: Some(parsed),
                    reply_to: None,
                };

                let _ = incoming_tx.send(incoming);
            }
        }
    }

    // Cleanup
    {
        let mut clients = state.clients.write().await;
        clients.remove(&client_id);
    }

    send_task.abort();
    info!(client_id = %client_id, "WebChat client disconnected");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_config_default() {
        let config = WebChatConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert!(!config.require_auth);
        assert!(config.auth_token.is_none());
    }

    #[test]
    fn test_channel_creation() {
        let channel = WebChatChannel::default_config();
        assert_eq!(channel.channel_type(), "webchat");
        assert_eq!(channel.url(), "http://127.0.0.1:8080");
    }

    #[test]
    fn test_origin_matches_host_header() {
        assert!(origin_matches_host_header(
            "http://localhost:8080",
            "localhost:8080"
        ));
        assert!(origin_matches_host_header(
            "http://127.0.0.1:8080",
            "127.0.0.1:8080"
        ));
        assert!(!origin_matches_host_header(
            "http://evil.com:8080",
            "localhost:8080"
        ));
        assert!(!origin_matches_host_header("null", "localhost:8080"));
    }

    #[test]
    fn test_authorize_websocket_origin_only_loopback() {
        let security = WebChatSecurity {
            require_auth: false,
            auth_token: None,
        };

        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("localhost:8080"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:8080"),
        );

        let remote = SocketAddr::from(([127, 0, 0, 1], 50000));
        assert!(authorize_websocket(&security, remote, &headers, None).is_ok());

        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://evil.com:8080"),
        );
        assert!(matches!(
            authorize_websocket(&security, remote, &headers, None),
            Err(WsRejection::InvalidOrigin)
        ));
    }

    #[test]
    fn test_authorize_websocket_requires_token() {
        let security = WebChatSecurity {
            require_auth: true,
            auth_token: Some("secret".to_string()),
        };

        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("localhost:8080"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:8080"),
        );

        let remote = SocketAddr::from(([10, 0, 0, 2], 50000));
        assert!(matches!(
            authorize_websocket(&security, remote, &headers, None),
            Err(WsRejection::MissingToken)
        ));
        assert!(matches!(
            authorize_websocket(&security, remote, &headers, Some("wrong")),
            Err(WsRejection::InvalidToken)
        ));
        assert!(authorize_websocket(&security, remote, &headers, Some("secret")).is_ok());
    }

    #[tokio::test]
    async fn test_connect_refuses_non_loopback_without_auth() {
        let config = WebChatConfig {
            host: "0.0.0.0".to_string(),
            port: 0,
            require_auth: false,
            auth_token: None,
            response_prefix: None,
            accounts: std::collections::HashMap::new(),
        };

        let mut channel = WebChatChannel::new(config);
        let err = drbot_channels::Channel::connect(&mut channel)
            .await
            .unwrap_err();
        assert!(matches!(err, drbot_core::Error::Config(_)));
    }

    #[tokio::test]
    async fn test_connect_requires_token_when_auth_enabled() {
        let config = WebChatConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            require_auth: true,
            auth_token: None,
            response_prefix: None,
            accounts: std::collections::HashMap::new(),
        };

        let mut channel = WebChatChannel::new(config);
        let err = drbot_channels::Channel::connect(&mut channel)
            .await
            .unwrap_err();
        assert!(matches!(err, drbot_core::Error::Config(_)));
    }
}
