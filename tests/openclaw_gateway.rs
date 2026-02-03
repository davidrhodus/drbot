//! OpenClaw Gateway v3 interoperability tests.

use drbot_core::Config;
use drbot_gateway::Gateway;
use drbot_protocol::openclaw::{GatewayFrame, RequestFrame};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

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
    // Keep auth disabled for these tests.
    config.gateway.auth_token = None;

    // Avoid writing into the user's real data dir during tests.
    let base = std::env::temp_dir().join(format!("drbot-openclaw-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&base).unwrap();
    config.storage.database_path = base.join("drbot.db");
    config.storage.media_path = base.join("media");
    config
}

async fn send_req(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    id: &str,
    method: &str,
    params: serde_json::Value,
) -> drbot_protocol::openclaw::ResponseFrame {
    let req = GatewayFrame::Req(RequestFrame {
        id: id.to_string(),
        method: method.to_string(),
        params: Some(params),
    });
    ws.send(Message::Text(serde_json::to_string(&req).unwrap().into()))
        .await
        .unwrap();

    for _ in 0..50 {
        match recv_frame(ws).await {
            GatewayFrame::Res(res) if res.id == id => return res,
            _ => {}
        }
    }
    panic!("did not receive response for {}", id);
}

#[tokio::test]
async fn openclaw_handshake_and_health() {
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

    let url = format!("ws://127.0.0.1:{}/openclaw/ws", port);
    let (mut ws, _resp) = tokio_tungstenite::connect_async(url).await.unwrap();

    // Server should send connect.challenge immediately.
    let frame = recv_frame(&mut ws).await;
    match frame {
        GatewayFrame::Event(evt) => {
            assert_eq!(evt.event, "connect.challenge");
            assert!(evt.payload.is_some());
        }
        other => panic!("expected connect.challenge event, got: {:?}", other),
    }

    // Send connect handshake request.
    let connect = GatewayFrame::Req(RequestFrame {
        id: "1".to_string(),
        method: "connect".to_string(),
        params: Some(json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": {
                "id": "test",
                "version": "0.0.0-test",
                "platform": "test",
                "mode": "test"
            }
        })),
    });
    ws.send(Message::Text(
        serde_json::to_string(&connect).unwrap().into(),
    ))
    .await
    .unwrap();

    let mut connect_res = None;
    for _ in 0..10 {
        match recv_frame(&mut ws).await {
            GatewayFrame::Res(res) if res.id == "1" => {
                connect_res = Some(res);
                break;
            }
            _ => {}
        }
    }
    let res = connect_res.expect("did not receive connect response");
    assert!(res.ok);
    let payload = res.payload.expect("missing hello-ok payload");
    assert_eq!(
        payload.get("type").and_then(|v| v.as_str()),
        Some("hello-ok")
    );
    assert_eq!(payload.get("protocol").and_then(|v| v.as_u64()), Some(3));

    // Call health.
    let health = GatewayFrame::Req(RequestFrame {
        id: "2".to_string(),
        method: "health".to_string(),
        params: Some(json!({})),
    });
    ws.send(Message::Text(
        serde_json::to_string(&health).unwrap().into(),
    ))
    .await
    .unwrap();

    let mut health_res = None;
    for _ in 0..10 {
        match recv_frame(&mut ws).await {
            GatewayFrame::Res(res) if res.id == "2" => {
                health_res = Some(res);
                break;
            }
            _ => {}
        }
    }
    let res = health_res.expect("did not receive health response");
    assert!(res.ok);
    let payload = res.payload.expect("missing health payload");
    assert_eq!(payload.get("status").and_then(|v| v.as_str()), Some("ok"));

    // Smoke: a few additional OpenClaw methods we advertise should be implemented.
    let res = send_req(&mut ws, "3", "tts.status", json!({})).await;
    assert!(res.ok);
    let payload = res.payload.expect("missing tts.status payload");
    assert!(payload.get("provider").is_some());

    let res = send_req(&mut ws, "4", "tts.providers", json!({})).await;
    assert!(res.ok);
    let payload = res.payload.expect("missing tts.providers payload");
    assert!(payload
        .get("providers")
        .and_then(|v| v.as_array())
        .is_some());

    let res = send_req(
        &mut ws,
        "5",
        "agent.wait",
        json!({ "runId": "missing-run", "timeoutMs": 0 }),
    )
    .await;
    assert!(res.ok, "agent.wait failed: {:?}", res.error);
    let payload = res.payload.expect("missing agent.wait payload");
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("timeout")
    );

    let res = send_req(
        &mut ws,
        "6",
        "browser.request",
        json!({ "method": "GET", "path": "/status" }),
    )
    .await;
    assert!(res.ok, "browser.request failed: {:?}", res.error);

    let res = send_req(
        &mut ws,
        "7",
        "send",
        json!({ "to": "nobody", "message": "hi", "idempotencyKey": "idem-1" }),
    )
    .await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().and_then(|e| Some(e.code.as_str())),
        Some("UNAVAILABLE")
    );

    // Heartbeats: enable and ensure last-heartbeat updates.
    let res = send_req(&mut ws, "8", "set-heartbeats", json!({ "enabled": true })).await;
    assert!(res.ok, "set-heartbeats failed: {:?}", res.error);

    let res = send_req(
        &mut ws,
        "9",
        "wake",
        json!({ "mode": "now", "text": "wake-test" }),
    )
    .await;
    assert!(res.ok);

    // last-heartbeat returns null until the heartbeat runner emits an event.
    let payload = {
        let mut found = None;
        for _ in 0..30 {
            let res = send_req(&mut ws, "10", "last-heartbeat", json!({})).await;
            assert!(res.ok);
            if let Some(p) = res.payload {
                found = Some(p);
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        found.expect("expected last-heartbeat payload after enabling")
    };
    assert!(payload.get("status").and_then(|v| v.as_str()).is_some());

    // Cron parity: status/add/run/runs/remove.
    let res = send_req(&mut ws, "11", "cron.status", json!({})).await;
    assert!(res.ok);
    let payload = res.payload.expect("missing cron.status payload");
    assert_eq!(payload.get("enabled").and_then(|v| v.as_bool()), Some(true));
    assert!(payload.get("storePath").and_then(|v| v.as_str()).is_some());

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let at_ms = now_ms + 3_600_000;

    let res = send_req(
        &mut ws,
        "12",
        "cron.add",
        json!({
            "name": "test-cron",
            "enabled": true,
            "schedule": { "kind": "at", "atMs": at_ms },
            "sessionTarget": "main",
            "wakeMode": "next-heartbeat",
            "payload": { "kind": "systemEvent", "text": "cron-test" }
        }),
    )
    .await;
    assert!(res.ok, "cron.add failed: {:?}", res.error);
    let payload = res.payload.expect("missing cron.add payload");
    let job_id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .expect("cron.add did not return id")
        .to_string();

    let res = send_req(
        &mut ws,
        "13",
        "cron.run",
        json!({ "id": job_id.clone(), "mode": "force" }),
    )
    .await;
    assert!(res.ok);
    let payload = res.payload.expect("missing cron.run payload");
    assert_eq!(payload.get("ran").and_then(|v| v.as_bool()), Some(true));

    // After running a one-shot job successfully, drbot disables it (OpenClaw behavior),
    // so default cron.list (includeDisabled=false) should be empty.
    let res = send_req(&mut ws, "14", "cron.list", json!({})).await;
    assert!(res.ok);
    let payload = res.payload.expect("missing cron.list payload");
    assert_eq!(
        payload
            .get("jobs")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(0)
    );

    let res = send_req(
        &mut ws,
        "15",
        "cron.runs",
        json!({ "jobId": job_id.clone(), "limit": 10 }),
    )
    .await;
    assert!(res.ok);
    let payload = res.payload.expect("missing cron.runs payload");
    let entries = payload
        .get("entries")
        .and_then(|v| v.as_array())
        .expect("cron.runs did not return entries array");
    assert!(!entries.is_empty());

    let res = send_req(&mut ws, "16", "cron.remove", json!({ "jobId": job_id })).await;
    assert!(res.ok);
    let payload = res.payload.expect("missing cron.remove payload");
    assert_eq!(payload.get("removed").and_then(|v| v.as_bool()), Some(true));

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}
