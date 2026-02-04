//! OpenClaw device-auth handshake interoperability tests.

use drbot_core::Config;
use drbot_gateway::Gateway;
use drbot_protocol::openclaw::{GatewayFrame, RequestFrame};
use futures::{SinkExt, StreamExt};
use ring::digest;
use ring::rand::SystemRandom;
use ring::signature::Ed25519KeyPair;
use ring::signature::KeyPair;
use serde_json::json;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

async fn recv_frame(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> GatewayFrame {
    let msg = tokio::time::timeout(Duration::from_secs(3), ws.next())
        .await
        .expect("timed out waiting for ws frame")
        .expect("ws stream closed")
        .expect("ws recv error");

    let text = match msg {
        Message::Text(t) => t.to_string(),
        Message::Binary(b) => String::from_utf8(b.to_vec()).expect("binary frame not utf8"),
        other => panic!("unexpected ws message: {:?}", other),
    };

    serde_json::from_str::<GatewayFrame>(&text).expect("invalid OpenClaw frame JSON")
}

fn test_config(port: u16) -> Config {
    let mut config = Config::default();
    config.gateway.host = "127.0.0.1".to_string();
    config.gateway.port = port;
    config.gateway.auth_token = Some("shared-secret".to_string());

    // Avoid writing into the user's real data dir during tests.
    let base = std::env::temp_dir().join(format!("drbot-openclaw-device-auth-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&base).unwrap();
    config.storage.database_path = base.join("drbot.db");
    config.storage.media_path = base.join("media");
    config
}

fn build_device_auth_payload(
    device_id: &str,
    client_id: &str,
    client_mode: &str,
    role: &str,
    scopes: &[String],
    signed_at_ms: u64,
    token: &str,
    nonce: &str,
) -> String {
    let version = if nonce.trim().is_empty() { "v1" } else { "v2" };
    let scopes = scopes.join(",");
    let mut parts = vec![
        version.to_string(),
        device_id.trim().to_string(),
        client_id.trim().to_string(),
        client_mode.trim().to_string(),
        role.trim().to_string(),
        scopes,
        signed_at_ms.to_string(),
        token.trim().to_string(),
    ];
    if version == "v2" {
        parts.push(nonce.trim().to_string());
    }
    parts.join("|")
}

fn b64url_no_pad(raw: &[u8]) -> String {
    drbot_base64_util::encode_config(raw, drbot_base64_util::Base64Config::URL_SAFE_NO_PAD)
}

fn sha256_hex(raw: &[u8]) -> String {
    let d = digest::digest(&digest::SHA256, raw);
    drbot_hex_util::encode(d.as_ref())
}

async fn connect_with_device(
    port: u16,
    keypair: &Ed25519KeyPair,
    device_id: &str,
    public_key_b64: &str,
    token: &str,
) -> String {
    let url = format!("ws://127.0.0.1:{}/openclaw/ws", port);
    let (mut ws, _resp) = tokio_tungstenite::connect_async(url).await.unwrap();

    // Server should send connect.challenge immediately.
    let mut nonce = None;
    for _ in 0..10 {
        match recv_frame(&mut ws).await {
            GatewayFrame::Event(evt) if evt.event == "connect.challenge" => {
                nonce = evt
                    .payload
                    .as_ref()
                    .and_then(|p| p.get("nonce"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                break;
            }
            _ => {}
        }
    }
    let nonce = nonce.expect("did not receive connect.challenge nonce");

    let signed_at_ms = now_ms();
    let payload = build_device_auth_payload(
        device_id,
        "test-device",
        "test",
        "operator",
        &vec!["global".to_string()],
        signed_at_ms,
        token,
        &nonce,
    );
    let sig = keypair.sign(payload.as_bytes());
    let sig_b64 = b64url_no_pad(sig.as_ref());

    // Send connect handshake request.
    let connect = GatewayFrame::Req(RequestFrame {
        id: "1".to_string(),
        method: "connect".to_string(),
        params: Some(json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": {
                "id": "test-device",
                "version": "0.0.0-test",
                "platform": "test",
                "mode": "test"
            },
            "role": "operator",
            "scopes": ["global"],
            "auth": { "token": token },
            "device": {
                "id": device_id,
                "publicKey": public_key_b64,
                "signature": sig_b64,
                "signedAt": signed_at_ms,
                "nonce": nonce,
            }
        })),
    });
    ws.send(Message::Text(
        serde_json::to_string(&connect).unwrap().into(),
    ))
    .await
    .unwrap();

    let mut connect_res = None;
    for _ in 0..20 {
        match recv_frame(&mut ws).await {
            GatewayFrame::Res(res) if res.id == "1" => {
                connect_res = Some(res);
                break;
            }
            _ => {}
        }
    }
    let res = connect_res.expect("did not receive connect response");
    assert!(res.ok, "connect failed: {:?}", res.error);
    let payload = res.payload.expect("missing hello-ok payload");

    let device_token = payload
        .get("auth")
        .and_then(|v| v.get("deviceToken"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    assert!(!device_token.trim().is_empty(), "missing deviceToken");

    drop(ws);
    device_token
}

#[tokio::test]
async fn openclaw_device_auth_token_fallback() {
    // Pick a free port.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let config = test_config(port);

    let gateway = Gateway::new(config);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server_handle = tokio::spawn(async move {
        gateway
            .run_with_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    // Wait a moment for the server to bind.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Generate a device identity.
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let public_key_raw = keypair.public_key().as_ref().to_vec();
    let device_id = sha256_hex(&public_key_raw);
    let public_key_b64 = b64url_no_pad(&public_key_raw);

    // First connect with the shared auth token (issues device token).
    let device_token_1 = connect_with_device(port, &keypair, &device_id, &public_key_b64, "shared-secret").await;

    // Second connect with the device token (shared token is wrong, but device-token fallback should succeed).
    let device_token_2 =
        connect_with_device(port, &keypair, &device_id, &public_key_b64, &device_token_1).await;

    assert_eq!(device_token_2, device_token_1);

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}
