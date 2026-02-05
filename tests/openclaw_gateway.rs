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
    // Enable tracing logs for debugging interop issues when running tests with `--nocapture`.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

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

    // OpenClaw clients rely on these defaults to choose the initial session key.
    let session_defaults = payload
        .get("snapshot")
        .and_then(|v| v.get("sessionDefaults"))
        .expect("missing snapshot.sessionDefaults");
    assert_eq!(
        session_defaults
            .get("defaultAgentId")
            .and_then(|v| v.as_str()),
        Some("default")
    );
    assert_eq!(
        session_defaults.get("mainKey").and_then(|v| v.as_str()),
        Some("main")
    );
    assert_eq!(
        session_defaults
            .get("mainSessionKey")
            .and_then(|v| v.as_str()),
        Some("agent:default:main")
    );
    assert_eq!(
        session_defaults.get("scope").and_then(|v| v.as_str()),
        Some("per-sender")
    );

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

    // Logs tail should include at least a few lines (connect.challenge + connect handshake).
    let res = send_req(
        &mut ws,
        "logs1",
        "logs.tail",
        json!({ "limit": 50, "maxBytes": 200_000 }),
    )
    .await;
    assert!(res.ok, "logs.tail failed: {:?}", res.error);
    let payload = res.payload.expect("missing logs.tail payload");
    let lines = payload
        .get("lines")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(!lines.is_empty(), "expected logs.tail to return lines");

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

    // chat.abort should return the OpenClaw payload shape even when nothing is in-flight.
    let res = send_req(
        &mut ws,
        "chat_abort_1",
        "chat.abort",
        json!({ "sessionKey": "main" }),
    )
    .await;
    assert!(res.ok, "chat.abort failed: {:?}", res.error);
    let payload = res.payload.expect("missing chat.abort payload");
    assert_eq!(payload.get("aborted").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(
        payload
            .get("runIds")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(0)
    );

    // chat.send stop command should act like abort and not require a provider.
    let res = send_req(
        &mut ws,
        "chat_stop_1",
        "chat.send",
        json!({ "sessionKey": "main", "message": "/stop", "idempotencyKey": "stop-1" }),
    )
    .await;
    assert!(res.ok, "chat.send /stop failed: {:?}", res.error);
    let payload = res.payload.expect("missing chat.send /stop payload");
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));

    // chat.inject should append an assistant message that shows up in chat.history.
    let res = send_req(
        &mut ws,
        "chat_inject_1",
        "chat.inject",
        json!({ "sessionKey": "main", "message": "hello", "label": "note" }),
    )
    .await;
    assert!(res.ok, "chat.inject failed: {:?}", res.error);
    let payload = res.payload.expect("missing chat.inject payload");
    assert!(payload
        .get("messageId")
        .and_then(|v| v.as_str())
        .is_some());

    let res = send_req(
        &mut ws,
        "chat_history_1",
        "chat.history",
        json!({ "sessionKey": "main", "limit": 10 }),
    )
    .await;
    assert!(res.ok, "chat.history failed: {:?}", res.error);
    let payload = res.payload.expect("missing chat.history payload");
    let messages = payload
        .get("messages")
        .and_then(|v| v.as_array())
        .expect("chat.history did not return messages array");
    assert!(!messages.is_empty());
    let last = messages.last().unwrap();
    assert_eq!(last.get("role").and_then(|v| v.as_str()), Some("assistant"));
    let text = last
        .get("content")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        text.contains("[note]"),
        "expected injected label prefix in assistant message, got: {:?}",
        text
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

#[tokio::test]
async fn openclaw_node_role_authorization() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

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

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://127.0.0.1:{}/openclaw/ws", port);
    let (mut ws, _resp) = tokio_tungstenite::connect_async(url).await.unwrap();

    // connect.challenge
    let frame = recv_frame(&mut ws).await;
    assert!(matches!(frame, GatewayFrame::Event(_)));

    // connect as a node
    let connect = GatewayFrame::Req(RequestFrame {
        id: "1".to_string(),
        method: "connect".to_string(),
        params: Some(json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "role": "node",
            "client": {
                "id": "test-node",
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
    assert!(res.ok, "node connect failed: {:?}", res.error);

    // Node role should be blocked from operator-only methods.
    let res = send_req(&mut ws, "2", "config.get", json!({})).await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("FORBIDDEN")
    );

    // ...but should be able to request pairing.
    let res = send_req(
        &mut ws,
        "3",
        "node.pair.request",
        json!({ "nodeId": "node-test-1" }),
    )
    .await;
    assert!(res.ok, "node.pair.request failed: {:?}", res.error);

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_sessions_patch_and_resolve_label() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

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

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://127.0.0.1:{}/openclaw/ws", port);
    let (mut ws, _resp) = tokio_tungstenite::connect_async(url).await.unwrap();

    // connect.challenge
    let frame = recv_frame(&mut ws).await;
    assert!(matches!(frame, GatewayFrame::Event(_)));

    // connect as operator
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

    for _ in 0..10 {
        if matches!(recv_frame(&mut ws).await, GatewayFrame::Res(res) if res.id == "1" && res.ok) {
            break;
        }
    }

    // Patch session label + metadata (should create session if missing).
    let res = send_req(
        &mut ws,
        "patch1",
        "sessions.patch",
        json!({
            "key": "test-session",
            "label": "my-test-worker",
            "thinkingLevel": "low",
            "model": "test-model"
        }),
    )
    .await;
    assert!(res.ok, "sessions.patch failed: {:?}", res.error);
    let payload = res.payload.expect("missing sessions.patch payload");
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let entry = payload
        .get("entry")
        .and_then(|v| v.as_object())
        .expect("sessions.patch did not return entry object");
    assert_eq!(
        entry.get("label").and_then(|v| v.as_str()),
        Some("my-test-worker")
    );
    assert_eq!(
        entry.get("thinkingLevel").and_then(|v| v.as_str()),
        Some("low")
    );
    assert_eq!(
        entry.get("model").and_then(|v| v.as_str()),
        Some("test-model")
    );

    // Resolve by label.
    let res = send_req(
        &mut ws,
        "resolve1",
        "sessions.resolve",
        json!({ "label": "my-test-worker" }),
    )
    .await;
    assert!(res.ok, "sessions.resolve failed: {:?}", res.error);
    let payload = res.payload.expect("missing sessions.resolve payload");
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload.get("key").and_then(|v| v.as_str()),
        Some("agent:default:test-session")
    );

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_node_command_allowlist_enforced() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

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

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://127.0.0.1:{}/openclaw/ws", port);

    // Operator connection.
    let (mut op_ws, _resp) = tokio_tungstenite::connect_async(url.clone()).await.unwrap();
    let _ = recv_frame(&mut op_ws).await; // connect.challenge
    let _ = send_req(
        &mut op_ws,
        "op_connect",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": { "id": "op", "version": "0.0.0-test", "platform": "test", "mode": "test" }
        }),
    )
    .await;

    // Node connection (linux platform => system-only commands allowlisted).
    let (mut node_ws, _resp) = tokio_tungstenite::connect_async(url).await.unwrap();
    let _ = recv_frame(&mut node_ws).await; // connect.challenge
    let _ = send_req(
        &mut node_ws,
        "node_connect",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "role": "node",
            "commands": ["canvas.present", "system.run"],
            "client": { "id": "node", "version": "0.0.0-test", "platform": "linux", "mode": "test" }
        }),
    )
    .await;

    // Operator should see node and its filtered command list.
    let res = send_req(&mut op_ws, "node_list_1", "node.list", json!({})).await;
    assert!(res.ok, "node.list failed: {:?}", res.error);
    let payload = res.payload.expect("missing node.list payload");
    let nodes = payload
        .get("nodes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let first = nodes
        .iter()
        .find(|n| n.get("connected").and_then(|v| v.as_bool()) == Some(true))
        .expect("expected a connected node");
    let node_id = first
        .get("nodeId")
        .and_then(|v| v.as_str())
        .expect("nodeId missing")
        .to_string();
    let commands = first
        .get("commands")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let commands_str = commands
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>();
    assert!(
        commands_str.contains(&"system.run"),
        "expected system.run to be allowlisted"
    );
    assert!(
        !commands_str.contains(&"canvas.present"),
        "expected canvas.present to be filtered out for linux nodes"
    );

    // Operator should be blocked from invoking a non-allowlisted command.
    let res = send_req(
        &mut op_ws,
        "node_invoke_1",
        "node.invoke",
        json!({
            "nodeId": node_id,
            "command": "canvas.present",
            "idempotencyKey": "idem-1",
            "timeoutMs": 10
        }),
    )
    .await;
    assert!(!res.ok, "expected node.invoke to fail");
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("INVALID_REQUEST")
    );
    assert_eq!(
        res.error.as_ref().map(|e| e.message.as_str()),
        Some("node command not allowed")
    );
    let details = res.error.as_ref().and_then(|e| e.details.as_ref());
    assert_eq!(
        details
            .and_then(|d| d.get("reason"))
            .and_then(|v| v.as_str()),
        Some("command not allowlisted")
    );

    drop(op_ws);
    drop(node_ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_node_invoke_result_accepts_payloadjson_object() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

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

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://127.0.0.1:{}/openclaw/ws", port);

    // Operator connection.
    let (mut op_ws, _resp) = tokio_tungstenite::connect_async(url.clone()).await.unwrap();
    let _ = recv_frame(&mut op_ws).await; // connect.challenge
    let _ = send_req(
        &mut op_ws,
        "connect",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": { "id": "op", "version": "0.0.0-test", "platform": "test", "mode": "test" }
        }),
    )
    .await;

    // Node connection (linux platform => system-only commands allowlisted).
    let (mut node_ws, _resp) = tokio_tungstenite::connect_async(url).await.unwrap();
    let _ = recv_frame(&mut node_ws).await; // connect.challenge
    let _ = send_req(
        &mut node_ws,
        "node_connect",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "role": "node",
            "commands": ["system.run"],
            "client": { "id": "node", "version": "0.0.0-test", "platform": "linux", "mode": "test" }
        }),
    )
    .await;

    // Operator discovers nodeId.
    let res = send_req(&mut op_ws, "node_list_1", "node.list", json!({})).await;
    assert!(res.ok, "node.list failed: {:?}", res.error);
    let payload = res.payload.expect("missing node.list payload");
    let nodes = payload
        .get("nodes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let first = nodes
        .iter()
        .find(|n| n.get("connected").and_then(|v| v.as_bool()) == Some(true))
        .expect("expected a connected node");
    let node_id = first
        .get("nodeId")
        .and_then(|v| v.as_str())
        .expect("nodeId missing")
        .to_string();

    let invoke_node_id = node_id.clone();
    let invoke_handle = tokio::spawn(async move {
        send_req(
            &mut op_ws,
            "node_invoke_ok",
            "node.invoke",
            json!({
                "nodeId": invoke_node_id,
                "command": "system.run",
                "params": { "cmd": "echo ok" },
                "idempotencyKey": "idem-ok",
                "timeoutMs": 2_000
            }),
        )
        .await
    });

    // Node receives invoke request event.
    let invoke_id = loop {
        let frame = recv_frame(&mut node_ws).await;
        match frame {
            GatewayFrame::Event(evt) if evt.event == "node.invoke.request" => {
                let payload = evt.payload.unwrap_or_else(|| json!({}));
                let id = payload.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let node = payload
                    .get("nodeId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                assert_eq!(node, node_id, "node.invoke.request nodeId mismatch");
                assert!(!id.is_empty(), "node.invoke.request missing id");
                break id;
            }
            _ => {}
        }
    };

    // Respond with payloadJSON as a literal object (older node quirk).
    let res = send_req(
        &mut node_ws,
        "invoke_result",
        "node.invoke.result",
        json!({
            "id": invoke_id,
            "nodeId": node_id,
            "ok": true,
            "payloadJSON": { "answer": 42 }
        }),
    )
    .await;
    assert!(res.ok, "node.invoke.result failed: {:?}", res.error);

    let invoke_res = invoke_handle.await.unwrap();
    assert!(invoke_res.ok, "node.invoke failed: {:?}", invoke_res.error);
    let payload = invoke_res.payload.expect("missing node.invoke payload");
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload
            .get("payload")
            .and_then(|v| v.get("answer"))
            .and_then(|v| v.as_i64()),
        Some(42)
    );
    assert_eq!(
        payload.get("payloadJSON").and_then(|v| v.as_str()),
        None,
        "expected payloadJSON to be omitted or null"
    );

    drop(node_ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_node_event_parses_payloadjson_for_voicewake() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

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

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://127.0.0.1:{}/openclaw/ws", port);

    // Operator connection.
    let (mut op_ws, _resp) = tokio_tungstenite::connect_async(url.clone()).await.unwrap();
    let _ = recv_frame(&mut op_ws).await; // connect.challenge
    let _ = send_req(
        &mut op_ws,
        "connect",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": { "id": "op", "version": "0.0.0-test", "platform": "test", "mode": "test" }
        }),
    )
    .await;

    // Node connection.
    let (mut node_ws, _resp) = tokio_tungstenite::connect_async(url).await.unwrap();
    let _ = recv_frame(&mut node_ws).await; // connect.challenge
    let _ = send_req(
        &mut node_ws,
        "node_connect",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "role": "node",
            "commands": ["system.run"],
            "client": { "id": "node", "version": "0.0.0-test", "platform": "linux", "mode": "test" }
        }),
    )
    .await;

    // Emit voicewake.changed with payloadJSON only.
    let res = send_req(
        &mut node_ws,
        "node_event_1",
        "node.event",
        json!({
            "event": "voicewake.changed",
            "payloadJSON": "{\"triggers\":[\"alpha\",\"beta\"]}"
        }),
    )
    .await;
    assert!(res.ok, "node.event failed: {:?}", res.error);

    // Operator reads back triggers.
    let res = send_req(&mut op_ws, "vw_get", "voicewake.get", json!({})).await;
    assert!(res.ok, "voicewake.get failed: {:?}", res.error);
    let payload = res.payload.expect("missing voicewake.get payload");
    let triggers = payload
        .get("triggers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let got = triggers
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();
    assert_eq!(got, vec!["alpha".to_string(), "beta".to_string()]);

    drop(op_ws);
    drop(node_ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_agents_files_list_and_skills_bins_shapes() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

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

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://127.0.0.1:{}/openclaw/ws", port);

    let (mut ws, _resp) = tokio_tungstenite::connect_async(url).await.unwrap();
    let _ = recv_frame(&mut ws).await; // connect.challenge
    let _ = send_req(
        &mut ws,
        "connect",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": { "id": "op", "version": "0.0.0-test", "platform": "test", "mode": "test" }
        }),
    )
    .await;

    let res = send_req(
        &mut ws,
        "agents_files_list",
        "agents.files.list",
        json!({ "agentId": "default" }),
    )
    .await;
    assert!(res.ok, "agents.files.list failed: {:?}", res.error);
    let payload = res.payload.expect("missing agents.files.list payload");
    let files = payload
        .get("files")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let names: Vec<String> = files
        .iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
        .collect();

    for required in [
        "AGENTS.md",
        "SOUL.md",
        "TOOLS.md",
        "IDENTITY.md",
        "USER.md",
        "HEARTBEAT.md",
        "BOOTSTRAP.md",
    ] {
        assert!(
            names.iter().any(|n| n == required),
            "agents.files.list missing {}",
            required
        );
    }
    assert!(
        names.iter().any(|n| n == "MEMORY.md" || n == "memory.md"),
        "agents.files.list missing MEMORY.md/memory.md"
    );
    for entry in &files {
        assert!(
            entry.get("missing").and_then(|v| v.as_bool()).is_some(),
            "agents.files.list entry missing boolean: {:?}",
            entry
        );
    }

    let res = send_req(&mut ws, "skills_bins", "skills.bins", json!({})).await;
    assert!(res.ok, "skills.bins failed: {:?}", res.error);
    let payload = res.payload.expect("missing skills.bins payload");
    assert!(
        payload.get("bins").and_then(|v| v.as_array()).is_some(),
        "skills.bins bins should be array"
    );

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}
