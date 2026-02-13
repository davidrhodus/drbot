//! OpenClaw Gateway v3 interoperability tests.

use drbot_core::config::CliProviderConfig;
use drbot_core::Config;
use drbot_gateway::Gateway;
use drbot_protocol::openclaw::{GatewayFrame, RequestFrame};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

static ENV_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();

struct EnvVarGuard {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &std::path::Path) -> Self {
        let prev = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, prev }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.prev.as_ref() {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}

async fn env_permit() -> tokio::sync::OwnedSemaphorePermit {
    ENV_SEMAPHORE
        .get_or_init(|| Arc::new(Semaphore::new(1)))
        .clone()
        .acquire_owned()
        .await
        .expect("env semaphore closed")
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
    // Keep auth disabled for these tests.
    config.gateway.auth_token = None;

    // Use a deterministic provider for agent runs during tests (no network).
    config.providers.default_provider = Some("test-cli".to_string());
    config.providers.default_model = Some("test".to_string());
    config.providers.cli = vec![CliProviderConfig {
        name: "test-cli".to_string(),
        command: "echo".to_string(),
        args: vec!["ok".to_string()],
        model_flag: "--model".to_string(),
        default_model: Some("test".to_string()),
        system_flag: None,
        send_history: false,
        timeout_secs: Some(5),
    }];

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
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert!(payload.get("sessions").is_some());

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

    // Channel status should include UI metadata and per-channel snapshots.
    let res = send_req(&mut ws, "channels_status_1", "channels.status", json!({})).await;
    assert!(res.ok, "channels.status failed: {:?}", res.error);
    let payload = res.payload.expect("missing channels.status payload");
    assert!(payload
        .get("channelOrder")
        .and_then(|v| v.as_array())
        .is_some());
    assert!(payload
        .get("channelLabels")
        .and_then(|v| v.as_object())
        .is_some());
    assert!(payload
        .get("channelMeta")
        .and_then(|v| v.as_array())
        .is_some());
    let channels = payload
        .get("channels")
        .and_then(|v| v.as_object())
        .expect("channels.status missing channels object");
    let signal = channels
        .get("signal")
        .and_then(|v| v.as_object())
        .expect("channels.status missing signal summary");
    assert!(
        signal.get("baseUrl").and_then(|v| v.as_str()).is_some(),
        "expected signal.baseUrl to be present for Control UI decoding"
    );

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

    // Usage dashboards should return a sensible payload even when no provider is configured.
    let res = send_req(&mut ws, "usage_status_1", "usage.status", json!({})).await;
    assert!(res.ok, "usage.status failed: {:?}", res.error);
    let payload = res.payload.expect("missing usage.status payload");
    assert!(payload
        .get("providers")
        .and_then(|v| v.as_array())
        .is_some());

    let res = send_req(&mut ws, "usage_cost_1", "usage.cost", json!({ "days": 7 })).await;
    assert!(res.ok, "usage.cost failed: {:?}", res.error);
    let payload = res.payload.expect("missing usage.cost payload");
    assert_eq!(payload.get("days").and_then(|v| v.as_u64()), Some(7));
    assert!(payload.get("totals").is_some());

    let res = send_req(
        &mut ws,
        "6",
        "browser.request",
        json!({ "method": "GET", "path": "/status" }),
    )
    .await;
    assert!(res.ok, "browser.request failed: {:?}", res.error);

    // SSRF parity: browser screenshot should refuse private-network URL targets by default.
    let res = send_req(
        &mut ws,
        "browser_ssrf_1",
        "browser.request",
        json!({
            "method": "POST",
            "path": "/screenshot",
            "query": { "url": "http://127.0.0.1/" },
            "body": { "fullPage": false },
        }),
    )
    .await;
    assert!(!res.ok, "expected SSRF rejection, got ok response");
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("INVALID_REQUEST")
    );

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

    // OpenClaw parity: per-session sendPolicy should be enforced.
    let res = send_req(
        &mut ws,
        "send_policy_patch_1",
        "sessions.patch",
        json!({ "key": "main", "sendPolicy": "deny" }),
    )
    .await;
    assert!(res.ok, "sessions.patch sendPolicy failed: {:?}", res.error);

    let res = send_req(
        &mut ws,
        "send_policy_send_1",
        "send",
        json!({
            "to": "nobody",
            "message": "hi",
            "idempotencyKey": "idem-deny-1",
            "sessionKey": "main"
        }),
    )
    .await;
    assert!(!res.ok, "expected sendPolicy to block send request");
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("FORBIDDEN")
    );

    // Poll parity: sendPolicy should be enforced for group-aware session keys (slack:channel:*).
    let res = send_req(
        &mut ws,
        "send_policy_patch_slack_1",
        "sessions.patch",
        json!({ "key": "slack:channel:C123", "sendPolicy": "deny" }),
    )
    .await;
    assert!(
        res.ok,
        "sessions.patch slack sendPolicy failed: {:?}",
        res.error
    );

    let res = send_req(
        &mut ws,
        "send_policy_poll_1",
        "poll",
        json!({
            "channel": "slack",
            "to": "C123",
            "question": "Which?",
            "options": ["a", "b"],
            "idempotencyKey": "poll-deny-1"
        }),
    )
    .await;
    assert!(!res.ok, "expected sendPolicy to block poll request");
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("FORBIDDEN")
    );

    // Agent parity: delivery fallback should strip group/channel markers from sessionKey mapping.
    let agent_req_id = "agent_deliver_1";
    let agent_req = GatewayFrame::Req(RequestFrame {
        id: agent_req_id.to_string(),
        method: "agent".to_string(),
        params: Some(json!({
            "message": "deliver-test",
            "idempotencyKey": "agent-deliver-1",
            "sessionKey": "slack:channel:C123",
            "deliver": true
        })),
    });
    ws.send(Message::Text(
        serde_json::to_string(&agent_req).unwrap().into(),
    ))
    .await
    .unwrap();

    let mut saw_accepted = false;
    let mut saw_agent_runtime_shell = false;
    let mut final_agent_res = None;
    for _ in 0..200 {
        match recv_frame(&mut ws).await {
            GatewayFrame::Event(evt) if evt.event == "agent" => {
                if let Some(obj) = evt.payload.as_ref().and_then(|v| v.as_object()) {
                    if obj.contains_key("runtimeShell") {
                        saw_agent_runtime_shell = true;
                    }
                }
            }
            GatewayFrame::Res(res) if res.id == agent_req_id => {
                assert!(res.ok, "agent response was not ok: {:?}", res.error);
                let payload = res.payload.clone().expect("missing agent payload");
                match payload.get("status").and_then(|v| v.as_str()) {
                    Some("accepted") => saw_accepted = true,
                    Some("ok") => {
                        final_agent_res = Some(payload);
                        break;
                    }
                    other => panic!("unexpected agent status payload: {:?}", other),
                }
            }
            _ => {}
        }
    }
    assert!(saw_accepted, "expected agent accepted response");
    assert!(
        saw_agent_runtime_shell,
        "expected agent event envelopes to include runtimeShell"
    );
    let payload = final_agent_res.expect("expected final agent response");
    let delivery = payload
        .get("delivery")
        .and_then(|v| v.as_object())
        .expect("missing agent delivery report");
    assert_eq!(
        delivery.get("requested").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        delivery.get("channel").and_then(|v| v.as_str()),
        Some("slack")
    );
    assert_eq!(
        delivery.get("to").and_then(|v| v.as_str()),
        Some("C123"),
        "expected delivery to strip 'channel:' prefix"
    );
    assert_eq!(delivery.get("ok").and_then(|v| v.as_bool()), Some(false));

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
    assert_eq!(
        payload.get("aborted").and_then(|v| v.as_bool()),
        Some(false)
    );
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
        json!({ "sessionKey": "main", "message": "hello\nMEDIA: /tmp/secret.png\nworld", "label": "note" }),
    )
    .await;
    assert!(res.ok, "chat.inject failed: {:?}", res.error);
    let payload = res.payload.expect("missing chat.inject payload");
    assert!(payload.get("messageId").and_then(|v| v.as_str()).is_some());

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

    assert!(
        !text.contains("MEDIA:"),
        "expected MEDIA lines to be stripped from injected message, got: {:?}",
        text
    );
    assert!(
        !text.contains("/tmp/secret.png"),
        "expected local media path to be stripped from injected message, got: {:?}",
        text
    );

    // sessions.preview should resolve legacy keys to canonical OpenClaw sessions.
    let res = send_req(
        &mut ws,
        "sessions_preview_1",
        "sessions.preview",
        json!({ "keys": ["main"], "limit": 5, "maxChars": 120 }),
    )
    .await;
    assert!(res.ok, "sessions.preview failed: {:?}", res.error);
    let payload = res.payload.expect("missing sessions.preview payload");
    let previews = payload
        .get("previews")
        .and_then(|v| v.as_array())
        .expect("sessions.preview did not return previews array");
    assert_eq!(previews.len(), 1);
    let preview = previews.first().unwrap();
    assert_eq!(preview.get("key").and_then(|v| v.as_str()), Some("main"));
    assert_eq!(preview.get("status").and_then(|v| v.as_str()), Some("ok"));
    let items = preview
        .get("items")
        .and_then(|v| v.as_array())
        .expect("sessions.preview missing items array");
    assert!(!items.is_empty(), "expected preview items to be non-empty");

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
            "deleteAfterRun": false,
            "schedule": { "kind": "at", "atMs": at_ms },
            "sessionTarget": "main",
            "wakeMode": "next-heartbeat",
            "payload": { "kind": "systemEvent", "text": "cron-test" }
        }),
    )
    .await;
    assert!(res.ok, "cron.add failed: {:?}", res.error);
    let payload = res.payload.expect("missing cron.add payload");
    let schedule = payload
        .get("schedule")
        .expect("cron.add payload missing schedule");
    assert_eq!(schedule.get("kind").and_then(|v| v.as_str()), Some("at"));
    let at = schedule
        .get("at")
        .and_then(|v| v.as_str())
        .expect("cron.add payload missing schedule.at");
    assert!(
        at.ends_with('Z'),
        "expected cron schedule.at to be RFC3339 UTC, got: {}",
        at
    );
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
async fn openclaw_config_patch_hot_applies_provider() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

    let _permit = env_permit().await;

    // Pick a free port.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    // Keep OpenClaw config writes isolated from the repo + user machine.
    let base = std::env::temp_dir().join(format!("drbot-openclaw-config-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&base).unwrap();
    let config_path = base.join("drbot.toml");
    let _cfg_guard = EnvVarGuard::set("DRBOT_CONFIG_PATH", &config_path);

    let mut config = Config::default();
    config.gateway.host = "127.0.0.1".to_string();
    config.gateway.port = port;
    config.gateway.auth_token = None;

    // Ensure the gateway starts without a provider.
    config.providers.default_provider = None;
    config.providers.default_model = None;
    config.providers.openai = None;
    config.providers.anthropic = None;
    config.providers.ollama = None;
    config.providers.openai_compatible.clear();
    config.providers.cli.clear();

    // Avoid writing into the user's real data dir during tests.
    config.storage.database_path = base.join("drbot.db");
    config.storage.media_path = base.join("media");

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

    // connect
    let res = send_req(
        &mut ws,
        "connect_1",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": {
                "id": "test",
                "version": "0.0.0-test",
                "platform": "test",
                "mode": "test"
            }
        }),
    )
    .await;
    assert!(res.ok, "connect failed: {:?}", res.error);

    // chat.send should fail without a provider.
    let res = send_req(
        &mut ws,
        "chat_send_1",
        "chat.send",
        json!({ "sessionKey": "main", "message": "hi", "idempotencyKey": "run-1" }),
    )
    .await;
    assert!(!res.ok, "expected chat.send to fail without a provider");
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("UNAVAILABLE")
    );

    // config.patch should hot-apply the provider without requiring a restart.
    let patch_raw = serde_json::to_string_pretty(&json!({
        "providers": {
            "default_provider": "test-cli",
            "default_model": "test",
            "cli": [
                {
                    "name": "test-cli",
                    "command": "echo",
                    "args": ["ok"],
                    "model_flag": "--model",
                    "default_model": "test",
                    "system_flag": null,
                    "send_history": false,
                    "timeout_secs": 5
                }
            ]
        },
        "channels": {
            "enabled": ["telegram"]
        }
    }))
    .unwrap();

    let res = send_req(
        &mut ws,
        "config_patch_1",
        "config.patch",
        json!({ "raw": patch_raw }),
    )
    .await;
    assert!(res.ok, "config.patch failed: {:?}", res.error);

    let res = send_req(
        &mut ws,
        "chat_send_2",
        "chat.send",
        json!({ "sessionKey": "main", "message": "hi2", "idempotencyKey": "run-2" }),
    )
    .await;
    assert!(res.ok, "expected chat.send to succeed after config.patch");

    let res = send_req(&mut ws, "channels_status_2", "channels.status", json!({})).await;
    assert!(res.ok, "channels.status failed: {:?}", res.error);
    let payload = res.payload.expect("missing channels.status payload");
    let telegram_enabled = payload
        .get("channels")
        .and_then(|v| v.get("telegram"))
        .and_then(|v| v.get("enabled"))
        .and_then(|v| v.as_bool());
    assert_eq!(telegram_enabled, Some(true));

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_sessions_spawn_applies_subagent_thinking_defaults() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

    let _permit = env_permit().await;

    // Pick a free port.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    // Keep OpenClaw config writes isolated from the repo + user machine.
    let base = std::env::temp_dir().join(format!(
        "drbot-openclaw-subagent-thinking-test-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&base).unwrap();
    let config_path = base.join("drbot.toml");
    let _cfg_guard = EnvVarGuard::set("DRBOT_CONFIG_PATH", &config_path);

    let mut config = Config::default();
    config.gateway.host = "127.0.0.1".to_string();
    config.gateway.port = port;
    config.gateway.auth_token = None;

    // Ensure the gateway starts without a provider (we'll add it via config.patch).
    config.providers.default_provider = None;
    config.providers.default_model = None;
    config.providers.openai = None;
    config.providers.anthropic = None;
    config.providers.ollama = None;
    config.providers.openai_compatible.clear();
    config.providers.cli.clear();

    // Avoid writing into the user's real data dir during tests.
    config.storage.database_path = base.join("drbot.db");
    config.storage.media_path = base.join("media");

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

    // connect
    let res = send_req(
        &mut ws,
        "connect_1",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": {
                "id": "test",
                "version": "0.0.0-test",
                "platform": "test",
                "mode": "test"
            }
        }),
    )
    .await;
    assert!(res.ok, "connect failed: {:?}", res.error);

    // config.patch: set provider + agents subagent thinking defaults.
    let patch_raw = serde_json::to_string_pretty(&json!({
        "providers": {
            "default_provider": "test-cli",
            "default_model": "test",
            "cli": [
                {
                    "name": "test-cli",
                    "command": "echo",
                    "args": ["ok"],
                    "model_flag": "--model",
                    "default_model": "test",
                    "system_flag": null,
                    "send_history": false,
                    "timeout_secs": 5
                }
            ]
        },
        "agents": {
            "defaults": {
                "subagents": { "thinking": "high" }
            },
            "list": [
                { "id": "default", "subagents": { "thinking": "low" } }
            ]
        }
    }))
    .unwrap();

    let res = send_req(
        &mut ws,
        "config_patch_1",
        "config.patch",
        json!({ "raw": patch_raw }),
    )
    .await;
    assert!(res.ok, "config.patch failed: {:?}", res.error);

    let url = format!("http://127.0.0.1:{}/tools/invoke", port);
    let client = reqwest::Client::new();
    let res = client
        .post(&url)
        .json(&json!({
            "tool": "sessions_spawn",
            "args": {
                "task": "noop",
                "runTimeoutSeconds": 0
            },
            "sessionKey": "main"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = res.json().await.unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let details = payload
        .get("result")
        .and_then(|v| v.get("details"))
        .expect("tools.invoke sessions_spawn missing result.details");
    let child_key = details
        .get("childSessionKey")
        .and_then(|v| v.as_str())
        .expect("tools.invoke sessions_spawn missing childSessionKey")
        .to_string();

    let res = send_req(
        &mut ws,
        "sessions_list_1",
        "sessions.list",
        json!({ "includeGlobal": true, "includeUnknown": true }),
    )
    .await;
    assert!(res.ok, "sessions.list failed: {:?}", res.error);
    let payload = res.payload.expect("missing sessions.list payload");
    let sessions = payload
        .get("sessions")
        .and_then(|v| v.as_array())
        .expect("sessions.list payload missing sessions array");
    let child = sessions
        .iter()
        .find(|s| s.get("key").and_then(|v| v.as_str()) == Some(child_key.as_str()))
        .expect("sessions.list did not include the spawned session");
    assert_eq!(
        child.get("thinkingLevel").and_then(|v| v.as_str()),
        Some("low")
    );

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_agents_defaults_image_model_persists_in_config() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

    let _permit = env_permit().await;

    // Pick a free port.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let config = test_config(port);
    let state_dir = config
        .storage
        .database_path
        .parent()
        .expect("database_path missing parent")
        .to_path_buf();
    let config_path = state_dir.join("drbot.toml");
    let _cfg_guard = EnvVarGuard::set("DRBOT_CONFIG_PATH", &config_path);

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

    // connect
    let res = send_req(
        &mut ws,
        "connect_1",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": {
                "id": "test",
                "version": "0.0.0-test",
                "platform": "test",
                "mode": "test"
            }
        }),
    )
    .await;
    assert!(res.ok, "connect failed: {:?}", res.error);

    // config.patch: set agents.defaults.imageModel.
    let patch_raw = serde_json::to_string_pretty(&json!({
        "agents": {
            "defaults": {
                "imageModel": {
                    "primary": "openai/gpt-4o-mini",
                    "fallbacks": ["anthropic/claude-3-5-sonnet-latest"]
                }
            }
        }
    }))
    .unwrap();
    let res = send_req(
        &mut ws,
        "config_patch_1",
        "config.patch",
        json!({ "raw": patch_raw }),
    )
    .await;
    assert!(res.ok, "config.patch failed: {:?}", res.error);
    let payload = res.payload.expect("missing config.patch payload");
    let cfg = payload
        .get("config")
        .and_then(|v| v.as_object())
        .expect("config.patch payload missing config object");
    let image_model = cfg
        .get("agents")
        .and_then(|v| v.get("defaults"))
        .and_then(|v| v.get("imageModel"))
        .and_then(|v| v.as_object())
        .expect("config missing agents.defaults.imageModel");
    assert_eq!(
        image_model.get("primary").and_then(|v| v.as_str()),
        Some("openai/gpt-4o-mini")
    );
    assert_eq!(
        image_model
            .get("fallbacks")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>()),
        Some(vec!["anthropic/claude-3-5-sonnet-latest"])
    );

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_agent_image_model_override_roundtrips_via_agents_update() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

    let _permit = env_permit().await;

    // Pick a free port.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let config = test_config(port);
    let state_dir = config
        .storage
        .database_path
        .parent()
        .expect("database_path missing parent")
        .to_path_buf();
    let config_path = state_dir.join("drbot.toml");
    let _cfg_guard = EnvVarGuard::set("DRBOT_CONFIG_PATH", &config_path);

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

    // connect
    let res = send_req(
        &mut ws,
        "connect_1",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": {
                "id": "test",
                "version": "0.0.0-test",
                "platform": "test",
                "mode": "test"
            }
        }),
    )
    .await;
    assert!(res.ok, "connect failed: {:?}", res.error);

    // agents.update: set agents.default.imageModel override.
    let res = send_req(
        &mut ws,
        "agents_update_1",
        "agents.update",
        json!({
            "agentId": "default",
            "imageModel": {
                "primary": "openai/gpt-4o-mini",
                "fallbacks": ["anthropic/claude-3-5-sonnet-latest"]
            }
        }),
    )
    .await;
    assert!(res.ok, "agents.update failed: {:?}", res.error);

    // agents.list should include the stored override.
    let res = send_req(&mut ws, "agents_list_1", "agents.list", json!({})).await;
    assert!(res.ok, "agents.list failed: {:?}", res.error);
    let payload = res.payload.expect("missing agents.list payload");
    let agents = payload
        .get("agents")
        .and_then(|v| v.as_array())
        .expect("agents.list payload missing agents array");
    let default_agent = agents
        .iter()
        .find(|a| a.get("id").and_then(|v| v.as_str()) == Some("default"))
        .expect("agents.list missing default agent");
    let image_model = default_agent
        .get("imageModel")
        .and_then(|v| v.as_object())
        .expect("agents.list missing default.imageModel override");
    assert_eq!(
        image_model.get("primary").and_then(|v| v.as_str()),
        Some("openai/gpt-4o-mini")
    );
    assert_eq!(
        image_model
            .get("fallbacks")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>()),
        Some(vec!["anthropic/claude-3-5-sonnet-latest"])
    );

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_memory_backend_qmd_paths_best_effort() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

    let _permit = env_permit().await;

    // Pick a free port.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let config = test_config(port);
    let state_dir = config
        .storage
        .database_path
        .parent()
        .expect("database_path missing parent")
        .to_path_buf();
    let config_path = state_dir.join("drbot.toml");
    let _cfg_guard = EnvVarGuard::set("DRBOT_CONFIG_PATH", &config_path);
    let _qmd_guard = EnvVarGuard::set(
        "DRBOT_OPENCLAW_MEMORY_QMD_BIN",
        std::path::Path::new("qmd-missing"),
    );

    let workspace_dir = state_dir.join("agents").join("default");
    std::fs::create_dir_all(&workspace_dir).unwrap();
    std::fs::write(workspace_dir.join("MEMORY.md"), "internal memory\n").unwrap();
    std::fs::create_dir_all(workspace_dir.join("extras")).unwrap();
    std::fs::write(
        workspace_dir.join("extras").join("notes.md"),
        "external memory\n",
    )
    .unwrap();

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

    // connect
    let res = send_req(
        &mut ws,
        "connect_1",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": {
                "id": "test",
                "version": "0.0.0-test",
                "platform": "test",
                "mode": "test"
            }
        }),
    )
    .await;
    assert!(res.ok, "connect failed: {:?}", res.error);

    // config.patch: enable qmd backend + one extra md file (relative to workspace root).
    let patch_raw = serde_json::to_string_pretty(&json!({
        "memory": {
            "backend": "qmd",
            "qmd": { "paths": [{ "name": "notes", "path": "extras/notes.md" }] }
        }
    }))
    .unwrap();
    let res = send_req(
        &mut ws,
        "config_patch_1",
        "config.patch",
        json!({ "raw": patch_raw }),
    )
    .await;
    assert!(res.ok, "config.patch failed: {:?}", res.error);

    let url = format!("http://127.0.0.1:{}/tools/invoke", port);
    let client = reqwest::Client::new();

    let res = client
        .post(&url)
        .json(&json!({
            "tool": "memory_search",
            "args": { "query": "external", "maxResults": 10 },
            "sessionKey": "main"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = res.json().await.unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let details = payload
        .get("result")
        .and_then(|v| v.get("details"))
        .expect("tools.invoke memory_search missing result.details");
    assert_eq!(details.get("disabled").and_then(|v| v.as_bool()), Some(false));
    let provider = details.get("provider").and_then(|v| v.as_str()).unwrap_or("");
    assert!(provider == "qmd" || provider == "local");
    let results = details
        .get("results")
        .and_then(|v| v.as_array())
        .expect("memory_search result.details missing results array");
    assert!(results.iter().any(|r| {
        r.get("path").and_then(|v| v.as_str()) == Some("MEMORY.md")
    }));
    assert!(results.iter().any(|r| {
        r.get("path").and_then(|v| v.as_str()) == Some("qmd/notes/notes.md")
    }));

    let res = client
        .post(&url)
        .json(&json!({
            "tool": "memory_get",
            "args": { "path": "qmd/notes/notes.md", "from": 1, "lines": 10 },
            "sessionKey": "main"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = res.json().await.unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let details = payload
        .get("result")
        .and_then(|v| v.get("details"))
        .expect("tools.invoke memory_get missing result.details");
    assert_eq!(
        details.get("path").and_then(|v| v.as_str()),
        Some("qmd/notes/notes.md")
    );
    let text = details.get("text").and_then(|v| v.as_str()).unwrap_or("");
    assert!(text.contains("external memory"));

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_chat_send_response_prefix() {
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

    // connect as operator.
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

    let res = send_req(
        &mut ws,
        "chat_prefix_1",
        "chat.send",
        json!({
            "sessionKey": "main",
            "message": "hello",
            "idempotencyKey": "prefix-1",
            "responsePrefix": "PREFIX:"
        }),
    )
    .await;
    assert!(res.ok, "chat.send failed: {:?}", res.error);
    let payload = res.payload.expect("missing chat.send payload");
    let run_id = payload
        .get("runId")
        .and_then(|v| v.as_str())
        .expect("chat.send missing runId")
        .to_string();

    let mut found = None;
    for _ in 0..50 {
        let frame = recv_frame(&mut ws).await;
        let GatewayFrame::Event(evt) = frame else {
            continue;
        };
        if evt.event != "chat" {
            continue;
        }
        let Some(p) = evt.payload else {
            continue;
        };
        if p.get("runId").and_then(|v| v.as_str()) != Some(run_id.as_str()) {
            continue;
        }
        if p.get("state").and_then(|v| v.as_str()) != Some("final") {
            continue;
        }
        let text = p
            .get("message")
            .and_then(|v| v.get("content"))
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        found = Some(text);
        break;
    }

    let text = found.expect("did not observe chat final event for responsePrefix run");
    assert!(
        text.starts_with("PREFIX:"),
        "expected responsePrefix to be applied, got: {:?}",
        text
    );

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}



#[tokio::test]
async fn openclaw_chat_send_strips_media_path_lines() {
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

    let mut config = test_config(port);
    // Force a response that includes a local MEDIA line.
    config.providers.cli[0].command = "bash".to_string();
    config.providers.cli[0].args = vec![
        "-c".to_string(),
        "printf 'hello\\nMEDIA: /tmp/secret.png\\nworld\\n'".to_string(),
        "-".to_string(),
    ];

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

    // connect as operator.
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

    let res = send_req(
        &mut ws,
        "chat_media_1",
        "chat.send",
        json!({
            "sessionKey": "main",
            "message": "hello",
            "idempotencyKey": "media-1"
        }),
    )
    .await;
    assert!(res.ok, "chat.send failed: {:?}", res.error);
    let payload = res.payload.expect("missing chat.send payload");
    let run_id = payload
        .get("runId")
        .and_then(|v| v.as_str())
        .expect("chat.send missing runId")
        .to_string();

    let mut found = None;
    for _ in 0..50 {
        let frame = recv_frame(&mut ws).await;
        let GatewayFrame::Event(evt) = frame else {
            continue;
        };
        if evt.event != "chat" {
            continue;
        }
        let Some(p) = evt.payload else {
            continue;
        };
        if p.get("runId").and_then(|v| v.as_str()) != Some(run_id.as_str()) {
            continue;
        }
        if p.get("state").and_then(|v| v.as_str()) != Some("final") {
            continue;
        }
        let text = p
            .get("message")
            .and_then(|v| v.get("content"))
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        found = Some(text);
        break;
    }

    let text = found.expect("did not observe chat final event for media run");
    assert!(
        !text.contains("MEDIA:"),
        "expected MEDIA lines to be stripped from assistant output, got: {:?}",
        text
    );
    assert!(
        !text.contains("/tmp/secret.png"),
        "expected local media path to be stripped from assistant output, got: {:?}",
        text
    );
    assert!(text.contains("hello"));
    assert!(text.contains("world"));

    // Ensure history view also strips the media path.
    let res = send_req(
        &mut ws,
        "chat_media_history",
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
    assert!(!messages.is_empty(), "expected chat.history messages");
    let last = messages.last().unwrap();
    assert_eq!(last.get("role").and_then(|v| v.as_str()), Some("assistant"));
    let history_text = last
        .get("content")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(!history_text.contains("MEDIA:"));
    assert!(!history_text.contains("/tmp/secret.png"));

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}
#[tokio::test]
async fn openclaw_send_applies_response_prefix_cascade() {
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

    let mut config = test_config(port);
    config.messages.response_prefix = Some("GLOBAL:".to_string());
    config.channels.telegram = Some(drbot_core::config::TelegramConfig {
        bot_token: "tg-test-token".to_string(),
        response_prefix: Some("TG:".to_string()),
        accounts: std::collections::HashMap::new(),
        allowed_users: Vec::new(),
        allowed_chats: Vec::new(),
    });
    config.channels.slack = Some(drbot_core::config::SlackConfig {
        bot_token: "slack-bot-token".to_string(),
        app_token: "slack-app-token".to_string(),
        signing_secret: "slack-signing-secret".to_string(),
        response_prefix: Some("".to_string()),
        accounts: std::collections::HashMap::new(),
    });

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

    // connect as operator.
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

    async fn assert_last_history_text(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        req_id: &str,
        expected_prefix: Option<&str>,
    ) {
        let res = send_req(
            ws,
            req_id,
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
        assert!(!messages.is_empty(), "expected chat.history messages");
        let last = messages.last().unwrap();
        assert_eq!(last.get("role").and_then(|v| v.as_str()), Some("assistant"));
        let text = last
            .get("content")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match expected_prefix {
            Some(prefix) => assert!(
                text.starts_with(prefix),
                "expected history message to start with {:?}, got: {:?}",
                prefix,
                text
            ),
            None => assert_eq!(text, "hello", "expected no prefix in message"),
        }
    }

    // L2: channel-level prefix wins over global.
    let res = send_req(
        &mut ws,
        "send_prefix_telegram",
        "send",
        json!({
            "sessionKey": "main",
            "channel": "telegram",
            "to": "123",
            "message": "hello",
            "idempotencyKey": "send-prefix-telegram",
            "dryRun": true
        }),
    )
    .await;
    assert!(res.ok, "send telegram failed: {:?}", res.error);
    assert_last_history_text(&mut ws, "send_prefix_history_tg", Some("TG:")).await;

    // Empty string stops cascade (no global prefix applied).
    let res = send_req(
        &mut ws,
        "send_prefix_slack",
        "send",
        json!({
            "sessionKey": "main",
            "channel": "slack",
            "to": "C123",
            "message": "hello",
            "idempotencyKey": "send-prefix-slack",
            "dryRun": true
        }),
    )
    .await;
    assert!(res.ok, "send slack failed: {:?}", res.error);
    assert_last_history_text(&mut ws, "send_prefix_history_slack", None).await;

    // L4: global prefix applies when no channel override exists.
    let res = send_req(
        &mut ws,
        "send_prefix_discord",
        "send",
        json!({
            "sessionKey": "main",
            "channel": "discord",
            "to": "123",
            "message": "hello",
            "idempotencyKey": "send-prefix-discord",
            "dryRun": true
        }),
    )
    .await;
    assert!(res.ok, "send discord failed: {:?}", res.error);
    assert_last_history_text(&mut ws, "send_prefix_history_discord", Some("GLOBAL:")).await;

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}



#[tokio::test]
async fn openclaw_send_applies_response_prefix_account_override() {
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

    let mut config = test_config(port);
    config.messages.response_prefix = Some("GLOBAL:".to_string());

    let mut accounts = std::collections::HashMap::new();
    accounts.insert(
        "acct-1".to_string(),
        drbot_core::config::ChannelAccountConfig {
            response_prefix: Some("ACCT:".to_string()),
        },
    );
    config.channels.telegram = Some(drbot_core::config::TelegramConfig {
        bot_token: "tg-test-token".to_string(),
        response_prefix: Some("TG:".to_string()),
        accounts,
        allowed_users: Vec::new(),
        allowed_chats: Vec::new(),
    });

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

    // connect as operator.
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

    let res = send_req(
        &mut ws,
        "send_prefix_tg_account",
        "send",
        json!({
            "sessionKey": "main",
            "channel": "telegram",
            "accountId": "acct-1",
            "to": "123",
            "message": "hello",
            "idempotencyKey": "send-prefix-tg-account",
            "dryRun": true
        }),
    )
    .await;
    assert!(res.ok, "send telegram failed: {:?}", res.error);

    let res = send_req(
        &mut ws,
        "send_prefix_history_tg_account",
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
    assert!(!messages.is_empty(), "expected chat.history messages");
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
        text.starts_with("ACCT:"),
        "expected account responsePrefix override to be applied, got: {:?}",
        text
    );

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}
#[tokio::test]
async fn openclaw_tools_invoke_cron_and_gateway() {
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

    let url = format!("http://127.0.0.1:{}/tools/invoke", port);
    let client = reqwest::Client::new();

    let res = client
        .post(&url)
        .json(&json!({
            "tool": "cron",
            "action": "status",
            "args": {},
            "sessionKey": "main"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = res.json().await.unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let details = payload
        .get("result")
        .and_then(|v| v.get("details"))
        .expect("tools.invoke cron.status missing result.details");
    assert_eq!(details.get("enabled").and_then(|v| v.as_bool()), Some(true));

    let res = client
        .post(&url)
        .json(&json!({
            "tool": "gateway",
            "action": "config.get",
            "args": {},
            "sessionKey": "main"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = res.json().await.unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let details = payload
        .get("result")
        .and_then(|v| v.get("details"))
        .expect("tools.invoke gateway config.get missing result.details");
    assert_eq!(details.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert!(details
        .get("result")
        .and_then(|v| v.get("hash"))
        .and_then(|v| v.as_str())
        .is_some());

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn tools_invoke_message_broadcast_dry_run() {
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

    let mut config = test_config(port);
    config.channels.enabled = vec!["telegram".to_string()];
    config.channels.telegram = Some(drbot_core::config::TelegramConfig {
        bot_token: "test-token".to_string(),
        response_prefix: None,
        accounts: std::collections::HashMap::new(),
        allowed_users: Vec::new(),
        allowed_chats: Vec::new(),
    });

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

    let url = format!("http://127.0.0.1:{}/tools/invoke", port);
    let client = reqwest::Client::new();

    let res = client
        .post(&url)
        .json(&json!({
            "tool": "message",
            "args": {
                "action": "broadcast",
                "channel": "telegram",
                "targets": ["123", "456"],
                "message": "hi",
                "dryRun": true
            },
            "sessionKey": "main"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = res.json().await.unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let details = payload
        .get("result")
        .and_then(|v| v.get("details"))
        .expect("tools.invoke message missing result.details");
    assert_eq!(details.get("ok").and_then(|v| v.as_bool()), Some(true));
    let results = details
        .get("results")
        .and_then(|v| v.as_array())
        .expect("message broadcast missing results array");
    assert_eq!(results.len(), 2);
    for entry in results {
        assert_eq!(
            entry.get("channel").and_then(|v| v.as_str()),
            Some("telegram")
        );
        assert_eq!(entry.get("ok").and_then(|v| v.as_bool()), Some(true));
        let inner = entry
            .get("result")
            .and_then(|v| v.as_object())
            .expect("broadcast entry missing result object");
        assert_eq!(inner.get("dryRun").and_then(|v| v.as_bool()), Some(true));
    }

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn tools_invoke_process_resize_pty_session() {
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
    let state_dir = config
        .storage
        .database_path
        .parent()
        .expect("test_config did not set database_path parent")
        .to_path_buf();
    let sidecar_path = state_dir
        .join("agents")
        .join("default")
        .join("sessions")
        .join("sessions.json");
    std::fs::create_dir_all(
        sidecar_path
            .parent()
            .expect("sessions.json missing parent"),
    )
    .unwrap();
    std::fs::write(
        &sidecar_path,
        serde_json::to_string_pretty(&json!({
            "version": 1,
            "entries": {
                "agent:default:main": { "execAsk": "allow" }
            }
        }))
        .unwrap(),
    )
    .unwrap();

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

    let url = format!("http://127.0.0.1:{}/tools/invoke", port);
    let client = reqwest::Client::new();

    // Start a long-ish PTY process so we can resize it.
    let res = client
        .post(&url)
        .json(&json!({
            "tool": "exec",
            "args": {
                "command": "sleep 3",
                "pty": true,
                "background": true
            },
            "sessionKey": "main"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = res.json().await.unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let session_id = payload
        .get("result")
        .and_then(|v| v.get("details"))
        .and_then(|v| v.get("details"))
        .and_then(|v| v.get("sessionId"))
        .and_then(|v| v.as_str())
        .expect("exec missing details.sessionId")
        .to_string();

    // Resize the PTY.
    let res = client
        .post(&url)
        .json(&json!({
            "tool": "process",
            "args": {
                "action": "resize",
                "sessionId": session_id.as_str(),
                "rows": 40,
                "cols": 120
            },
            "sessionKey": "main"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = res.json().await.unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let details = payload
        .get("result")
        .and_then(|v| v.get("details"))
        .expect("resize missing result.details");
    assert_eq!(details.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        details
            .get("requested")
            .and_then(|v| v.get("rows"))
            .and_then(|v| v.as_u64()),
        Some(40)
    );
    assert_eq!(
        details
            .get("requested")
            .and_then(|v| v.get("cols"))
            .and_then(|v| v.as_u64()),
        Some(120)
    );

    // Best-effort cleanup: stop + remove the process session.
    let _ = client
        .post(&url)
        .json(&json!({
            "tool": "process",
            "args": { "action": "kill", "sessionId": session_id.as_str() },
            "sessionKey": "main"
        }))
        .send()
        .await;
    let _ = client
        .post(&url)
        .json(&json!({
            "tool": "process",
            "args": { "action": "remove", "sessionId": session_id.as_str() },
            "sessionKey": "main"
        }))
        .send()
        .await;

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn tools_invoke_exec_host_sandbox_uses_isolated_cwd_and_home() {
    if cfg!(windows) {
        return;
    }

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
    let state_dir = config
        .storage
        .database_path
        .parent()
        .expect("test_config did not set database_path parent")
        .to_path_buf();
    let sidecar_path = state_dir
        .join("agents")
        .join("default")
        .join("sessions")
        .join("sessions.json");
    std::fs::create_dir_all(
        sidecar_path
            .parent()
            .expect("sessions.json missing parent"),
    )
    .unwrap();
    std::fs::write(
        &sidecar_path,
        serde_json::to_string_pretty(&json!({
            "version": 1,
            "entries": {
                "agent:default:main": { "execAsk": "allow" }
            }
        }))
        .unwrap(),
    )
    .unwrap();

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

    let url = format!("http://127.0.0.1:{}/tools/invoke", port);
    let client = reqwest::Client::new();

    let res = client
        .post(&url)
        .json(&json!({
            "tool": "exec",
            "args": { "command": "pwd", "host": "sandbox" },
            "sessionKey": "main"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = res.json().await.unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let cwd = payload
        .get("result")
        .and_then(|v| v.get("details"))
        .and_then(|v| v.get("details"))
        .and_then(|v| v.get("cwd"))
        .and_then(|v| v.as_str())
        .expect("sandbox exec missing details.cwd");

    let cwd_path = std::path::PathBuf::from(cwd);
    let state_dir_canon = std::fs::canonicalize(&state_dir).unwrap_or(state_dir.clone());
    let expected_prefix = state_dir_canon.join("sandbox").join("exec").join("default");
    assert!(
        cwd_path.starts_with(&expected_prefix),
        "sandbox cwd did not start with {:?} (got {})",
        expected_prefix,
        cwd
    );
    assert!(
        !cwd_path.starts_with(state_dir_canon.join("agents").join("default")),
        "sandbox cwd should not reuse the agent workspace (got {})",
        cwd
    );

    let res = client
        .post(&url)
        .json(&json!({
            "tool": "exec",
            "args": { "command": "echo $HOME", "host": "sandbox" },
            "sessionKey": "main"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = res.json().await.unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let home = payload
        .get("result")
        .and_then(|v| v.get("details"))
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    assert_eq!(home, cwd);

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_agents_crud() {
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
    let state_dir = config
        .storage
        .database_path
        .parent()
        .expect("test_config did not set database_path parent")
        .to_path_buf();
    let workspace_dir = state_dir.join("workspace-test-agent");
    let workspace = workspace_dir.to_string_lossy().to_string();

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

    // connect as operator (scopes default to global in tests).
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

    // Create an agent workspace + config entry.
    let res = send_req(
        &mut ws,
        "agents_create_1",
        "agents.create",
        json!({
            "name": "Test Agent",
            "workspace": workspace,
            "emoji": "🧪",
            "avatar": "T"
        }),
    )
    .await;
    assert!(res.ok, "agents.create failed: {:?}", res.error);
    let payload = res.payload.expect("missing agents.create payload");
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    let agent_id = payload
        .get("agentId")
        .and_then(|v| v.as_str())
        .expect("agents.create missing agentId")
        .to_string();
    assert_eq!(agent_id, "test-agent");

    // Workspace should be bootstrapped.
    assert!(workspace_dir.join("AGENTS.md").exists());
    assert!(workspace_dir.join("IDENTITY.md").exists());
    assert!(workspace_dir.join("MEMORY.md").exists());

    // Identity should roundtrip.
    let res = send_req(
        &mut ws,
        "agent_identity_1",
        "agent.identity.get",
        json!({ "agentId": agent_id.clone() }),
    )
    .await;
    assert!(res.ok, "agent.identity.get failed: {:?}", res.error);
    let payload = res.payload.expect("missing agent.identity.get payload");
    assert_eq!(
        payload.get("name").and_then(|v| v.as_str()),
        Some("Test Agent")
    );
    assert_eq!(payload.get("avatar").and_then(|v| v.as_str()), Some("T"));
    assert_eq!(payload.get("emoji").and_then(|v| v.as_str()), Some("🧪"));

    // Update identity fields.
    let res = send_req(
        &mut ws,
        "agents_update_1",
        "agents.update",
        json!({ "agentId": agent_id.clone(), "name": "Renamed Agent", "avatar": "R" }),
    )
    .await;
    assert!(res.ok, "agents.update failed: {:?}", res.error);
    let payload = res.payload.expect("missing agents.update payload");
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));

    let res = send_req(
        &mut ws,
        "agent_identity_2",
        "agent.identity.get",
        json!({ "agentId": agent_id.clone() }),
    )
    .await;
    assert!(res.ok, "agent.identity.get failed: {:?}", res.error);
    let payload = res.payload.expect("missing agent.identity.get payload");
    assert_eq!(
        payload.get("name").and_then(|v| v.as_str()),
        Some("Renamed Agent")
    );
    assert_eq!(payload.get("avatar").and_then(|v| v.as_str()), Some("R"));

    // Delete agent + ensure files were removed or moved to trash.
    let res = send_req(
        &mut ws,
        "agents_delete_1",
        "agents.delete",
        json!({ "agentId": agent_id.clone(), "deleteFiles": true }),
    )
    .await;
    assert!(res.ok, "agents.delete failed: {:?}", res.error);
    let payload = res.payload.expect("missing agents.delete payload");
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        payload.get("removedBindings").and_then(|v| v.as_u64()),
        Some(0)
    );
    assert!(
        !workspace_dir.exists(),
        "expected workspace to be moved/removed"
    );

    let trash_base = state_dir.join("trash").join("agents");
    assert!(trash_base.exists(), "expected trash/agents dir to exist");
    let mut found_trash = false;
    for entry in std::fs::read_dir(&trash_base).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("test-agent-") {
            found_trash = true;
            assert!(
                entry.path().join("AGENTS.md").exists(),
                "expected trashed workspace to contain bootstrap files"
            );
        }
    }
    assert!(found_trash, "expected to find trashed agent workspace");

    // agents.list should no longer include it.
    let res = send_req(&mut ws, "agents_list_2", "agents.list", json!({})).await;
    assert!(res.ok, "agents.list failed: {:?}", res.error);
    let payload = res.payload.expect("missing agents.list payload");
    let agents = payload
        .get("agents")
        .and_then(|v| v.as_array())
        .expect("agents.list missing agents array");
    assert!(
        !agents
            .iter()
            .any(|a| a.get("id").and_then(|v| v.as_str()) == Some("test-agent")),
        "expected agents.list to omit deleted agent"
    );

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_channels_logout_telegram_clears_config_and_runtime() {
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

    // Serialize env var writes across tests.
    let _env_permit = env_permit().await;

    let mut config = test_config(port);
    config.channels.telegram = Some(drbot_core::config::TelegramConfig {
        bot_token: "tg-test-token".to_string(),
        response_prefix: None,
        accounts: std::collections::HashMap::new(),
        allowed_users: Vec::new(),
        allowed_chats: Vec::new(),
    });

    // Force gateway config operations (config.get/patch and channels.logout) to use a temp file.
    let cfg_dir = std::env::temp_dir().join(format!("drbot-openclaw-config-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&cfg_dir).unwrap();
    let cfg_path = cfg_dir.join("drbot.toml");
    std::fs::write(&cfg_path, toml::to_string_pretty(&config).unwrap()).unwrap();

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(prev) = self.prev.as_ref() {
                std::env::set_var(self.key, prev);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    let _env_guard = EnvGuard {
        key: "DRBOT_CONFIG_PATH",
        prev: std::env::var("DRBOT_CONFIG_PATH").ok(),
    };
    std::env::set_var("DRBOT_CONFIG_PATH", cfg_path.to_string_lossy().to_string());

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

    // Initial channels.status should show telegram configured.
    let res = send_req(
        &mut ws,
        "channels_status_before",
        "channels.status",
        json!({ "probe": true, "timeoutMs": 500 }),
    )
    .await;
    assert!(res.ok, "channels.status failed: {:?}", res.error);
    let payload = res.payload.expect("missing channels.status payload");
    let channels = payload
        .get("channels")
        .and_then(|v| v.as_object())
        .expect("channels.status missing channels object");
    let telegram = channels
        .get("telegram")
        .and_then(|v| v.as_object())
        .expect("channels.status missing telegram summary");
    assert_eq!(
        telegram.get("configured").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert!(telegram.get("probe").is_some());

    // Logout telegram and verify response shape.
    let res = send_req(
        &mut ws,
        "logout_tg_1",
        "channels.logout",
        json!({ "channel": "telegram" }),
    )
    .await;
    assert!(res.ok, "channels.logout failed: {:?}", res.error);
    let payload = res.payload.expect("missing channels.logout payload");
    assert_eq!(
        payload.get("channel").and_then(|v| v.as_str()),
        Some("telegram")
    );
    assert_eq!(payload.get("cleared").and_then(|v| v.as_bool()), Some(true));

    // channels.status should reflect runtime logout immediately.
    let res = send_req(
        &mut ws,
        "channels_status_after",
        "channels.status",
        json!({ "probe": false }),
    )
    .await;
    assert!(res.ok, "channels.status failed: {:?}", res.error);
    let payload = res.payload.expect("missing channels.status payload");
    let channels = payload
        .get("channels")
        .and_then(|v| v.as_object())
        .expect("channels.status missing channels object");
    let telegram = channels
        .get("telegram")
        .and_then(|v| v.as_object())
        .expect("channels.status missing telegram summary");
    assert_eq!(
        telegram.get("configured").and_then(|v| v.as_bool()),
        Some(false)
    );

    // Config file should have token cleared on disk.
    let raw = std::fs::read_to_string(&cfg_path).unwrap();
    let parsed: drbot_core::Config = toml::from_str(&raw).unwrap();
    assert_eq!(
        parsed
            .channels
            .telegram
            .as_ref()
            .map(|c| c.bot_token.clone()),
        Some("".to_string())
    );

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
                let id = payload
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
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
async fn openclaw_wizard_writes_gateway_settings() {
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

    // Serialize env var writes across tests.
    let _env_permit = env_permit().await;

    let config = test_config(port);

    // Force wizard config writes to use a temp file.
    let cfg_dir = std::env::temp_dir().join(format!("drbot-openclaw-wizard-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&cfg_dir).unwrap();
    let cfg_path = cfg_dir.join("drbot.toml");
    std::fs::write(&cfg_path, toml::to_string_pretty(&config).unwrap()).unwrap();

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(prev) = self.prev.as_ref() {
                std::env::set_var(self.key, prev);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
    let _env_guard = EnvGuard {
        key: "DRBOT_CONFIG_PATH",
        prev: std::env::var("DRBOT_CONFIG_PATH").ok(),
    };
    std::env::set_var("DRBOT_CONFIG_PATH", cfg_path.to_string_lossy().to_string());

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

    // connect
    let res = send_req(
        &mut ws,
        "1",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": {
                "id": "test",
                "version": "0.0.0-test",
                "platform": "test",
                "mode": "test"
            }
        }),
    )
    .await;
    assert!(res.ok, "connect failed: {:?}", res.error);

    // Start wizard.
    let res = send_req(
        &mut ws,
        "wiz_start",
        "wizard.start",
        json!({ "mode": "local" }),
    )
    .await;
    assert!(res.ok, "wizard.start failed: {:?}", res.error);
    let mut payload = res.payload.expect("missing wizard.start payload");
    let session_id = payload
        .get("sessionId")
        .and_then(|v| v.as_str())
        .expect("wizard.start missing sessionId")
        .to_string();

    let token_value = "wizard-test-token";
    let desired_port: u16 = 18001;

    for i in 0..32 {
        if payload.get("done").and_then(|v| v.as_bool()) == Some(true) {
            break;
        }
        let step = payload
            .get("step")
            .and_then(|v| v.as_object())
            .expect("wizard response missing step");
        let step_id = step
            .get("id")
            .and_then(|v| v.as_str())
            .expect("wizard step missing id");
        let step_type = step.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let step_message = step.get("message").and_then(|v| v.as_str()).unwrap_or("");

        let value = match step_type {
            "select" => {
                if step_message.to_ascii_lowercase().contains("authentication") {
                    json!("new")
                } else if step_message.to_ascii_lowercase().contains("bind host") {
                    json!("loopback")
                } else {
                    json!(null)
                }
            }
            "text" => {
                if step_message.to_ascii_lowercase().contains("token") {
                    json!(token_value)
                } else {
                    json!(desired_port)
                }
            }
            "confirm" => json!(true),
            _ => json!(null),
        };

        let res = send_req(
            &mut ws,
            &format!("wiz_next_{}", i),
            "wizard.next",
            json!({
                "sessionId": session_id,
                "answer": { "stepId": step_id, "value": value }
            }),
        )
        .await;
        assert!(res.ok, "wizard.next failed: {:?}", res.error);
        payload = res.payload.expect("missing wizard.next payload");
    }

    assert_eq!(payload.get("status").and_then(|v| v.as_str()), Some("done"));

    let cfg_raw = std::fs::read_to_string(&cfg_path).unwrap();
    let cfg_written: drbot_core::Config = toml::from_str(&cfg_raw).unwrap();
    assert_eq!(cfg_written.gateway.auth_token.as_deref(), Some(token_value));
    assert_eq!(cfg_written.gateway.host, "127.0.0.1");
    assert_eq!(cfg_written.gateway.port, desired_port);

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_wizard_can_configure_openai_provider() {
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

    // Serialize env var writes across tests.
    let _env_permit = env_permit().await;

    let config = test_config(port);

    // Force wizard config writes to use a temp file.
    let cfg_dir =
        std::env::temp_dir().join(format!("drbot-openclaw-wizard-provider-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&cfg_dir).unwrap();
    let cfg_path = cfg_dir.join("drbot.toml");
    std::fs::write(&cfg_path, toml::to_string_pretty(&config).unwrap()).unwrap();

    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(prev) = self.prev.as_ref() {
                std::env::set_var(self.key, prev);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
    let _env_guard = EnvGuard {
        key: "DRBOT_CONFIG_PATH",
        prev: std::env::var("DRBOT_CONFIG_PATH").ok(),
    };
    std::env::set_var("DRBOT_CONFIG_PATH", cfg_path.to_string_lossy().to_string());

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

    // connect
    let res = send_req(
        &mut ws,
        "1",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": {
                "id": "test",
                "version": "0.0.0-test",
                "platform": "test",
                "mode": "test"
            }
        }),
    )
    .await;
    assert!(res.ok, "connect failed: {:?}", res.error);

    // Start wizard.
    let res = send_req(
        &mut ws,
        "wiz_start",
        "wizard.start",
        json!({ "mode": "local" }),
    )
    .await;
    assert!(res.ok, "wizard.start failed: {:?}", res.error);
    let mut payload = res.payload.expect("missing wizard.start payload");
    let session_id = payload
        .get("sessionId")
        .and_then(|v| v.as_str())
        .expect("wizard.start missing sessionId")
        .to_string();

    let token_value = "wizard-test-token";
    let desired_port: u16 = 18002;
    let openai_key = "openai-test-key";
    let openai_model = "gpt-4o";

    for i in 0..32 {
        if payload.get("done").and_then(|v| v.as_bool()) == Some(true) {
            break;
        }
        let step = payload
            .get("step")
            .and_then(|v| v.as_object())
            .expect("wizard response missing step");
        let step_id = step
            .get("id")
            .and_then(|v| v.as_str())
            .expect("wizard step missing id");
        let step_type = step.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let step_message = step.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let msg_lc = step_message.to_ascii_lowercase();

        let value = match step_type {
            "select" => {
                if msg_lc.contains("authentication") {
                    json!("new")
                } else if msg_lc.contains("bind host") {
                    json!("loopback")
                } else if msg_lc.contains("default ai provider") {
                    json!("openai")
                } else {
                    json!(null)
                }
            }
            "text" => {
                if msg_lc.contains("api key") {
                    json!(openai_key)
                } else if msg_lc.contains("default model") {
                    json!(openai_model)
                } else if msg_lc.contains("token") {
                    json!(token_value)
                } else if msg_lc.contains("port") {
                    json!(desired_port)
                } else {
                    json!(null)
                }
            }
            "confirm" => json!(true),
            _ => json!(null),
        };

        let res = send_req(
            &mut ws,
            &format!("wiz_next_{}", i),
            "wizard.next",
            json!({
                "sessionId": session_id,
                "answer": { "stepId": step_id, "value": value }
            }),
        )
        .await;
        assert!(res.ok, "wizard.next failed: {:?}", res.error);
        payload = res.payload.expect("missing wizard.next payload");
    }

    assert_eq!(payload.get("status").and_then(|v| v.as_str()), Some("done"));

    let cfg_raw = std::fs::read_to_string(&cfg_path).unwrap();
    let cfg_written: drbot_core::Config = toml::from_str(&cfg_raw).unwrap();
    assert_eq!(cfg_written.gateway.auth_token.as_deref(), Some(token_value));
    assert_eq!(cfg_written.gateway.host, "127.0.0.1");
    assert_eq!(cfg_written.gateway.port, desired_port);
    assert_eq!(
        cfg_written.providers.default_provider.as_deref(),
        Some("openai")
    );
    assert_eq!(
        cfg_written
            .providers
            .openai
            .as_ref()
            .map(|v| v.api_key.as_str()),
        Some(openai_key)
    );
    assert_eq!(
        cfg_written.providers.default_model.as_deref(),
        Some(openai_model)
    );

    drop(ws);
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
        .filter_map(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
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

#[tokio::test]
async fn openclaw_moltbook_tools() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

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
    let _ = recv_frame(&mut ws).await;

    // connect handshake
    let _ = send_req(
        &mut ws,
        "connect",
        "connect",
        json!({
            "minProtocol": 3,
            "maxProtocol": 3,
            "client": { "id": "test", "version": "0.0.0-test", "platform": "test", "mode": "test" }
        }),
    )
    .await;

    // ---------------------------------------------------------------
    // 1. dryRun works without API key (validates the reordering fix).
    // ---------------------------------------------------------------

    // moltbook.post dryRun
    let res = send_req(
        &mut ws,
        "mb_post_dry",
        "moltbook.post",
        json!({ "title": "Hello", "content": "World", "submolt": "test", "dryRun": true }),
    )
    .await;
    assert!(res.ok, "moltbook.post dryRun failed: {:?}", res.error);
    let p = res.payload.expect("missing moltbook.post dryRun payload");
    assert_eq!(p.get("dryRun").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(p.get("method").and_then(|v| v.as_str()), Some("POST"));
    assert!(
        p.get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("/posts"),
        "expected url to contain /posts"
    );

    // moltbook.comment dryRun
    let res = send_req(
        &mut ws,
        "mb_comment_dry",
        "moltbook.comment",
        json!({ "postId": "abc123", "content": "Nice post!", "dryRun": true }),
    )
    .await;
    assert!(res.ok, "moltbook.comment dryRun failed: {:?}", res.error);
    let p = res
        .payload
        .expect("missing moltbook.comment dryRun payload");
    assert_eq!(p.get("dryRun").and_then(|v| v.as_bool()), Some(true));
    assert!(
        p.get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("/posts/abc123/comments"),
        "expected url to contain /posts/abc123/comments"
    );

    // moltbook.vote dryRun (down)
    let res = send_req(
        &mut ws,
        "mb_vote_dry",
        "moltbook.vote",
        json!({ "postId": "abc123", "direction": "down", "dryRun": true }),
    )
    .await;
    assert!(res.ok, "moltbook.vote dryRun failed: {:?}", res.error);
    let p = res.payload.expect("missing moltbook.vote dryRun payload");
    assert_eq!(p.get("dryRun").and_then(|v| v.as_bool()), Some(true));
    assert!(
        p.get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("/posts/abc123/downvote"),
        "expected url to contain downvote"
    );

    // moltbook.vote dryRun default direction (up)
    let res = send_req(
        &mut ws,
        "mb_vote_dry_up",
        "moltbook.vote",
        json!({ "postId": "xyz", "dryRun": true }),
    )
    .await;
    assert!(res.ok, "moltbook.vote dryRun (up) failed: {:?}", res.error);
    let p = res
        .payload
        .expect("missing moltbook.vote dryRun (up) payload");
    assert!(
        p.get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("/posts/xyz/upvote"),
        "expected url to contain upvote"
    );

    // moltbook.follow dryRun
    let res = send_req(
        &mut ws,
        "mb_follow_dry",
        "moltbook.follow",
        json!({ "agent": "coolbot", "dryRun": true }),
    )
    .await;
    assert!(res.ok, "moltbook.follow dryRun failed: {:?}", res.error);
    let p = res.payload.expect("missing moltbook.follow dryRun payload");
    assert_eq!(p.get("method").and_then(|v| v.as_str()), Some("POST"));
    assert!(p
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .contains("/agents/coolbot/follow"),);

    // moltbook.follow unfollow dryRun
    let res = send_req(
        &mut ws,
        "mb_unfollow_dry",
        "moltbook.follow",
        json!({ "agent": "coolbot", "unfollow": true, "dryRun": true }),
    )
    .await;
    assert!(
        res.ok,
        "moltbook.follow unfollow dryRun failed: {:?}",
        res.error
    );
    let p = res
        .payload
        .expect("missing moltbook.follow unfollow payload");
    assert_eq!(p.get("method").and_then(|v| v.as_str()), Some("DELETE"));

    // moltbook.subscribe dryRun
    let res = send_req(
        &mut ws,
        "mb_sub_dry",
        "moltbook.subscribe",
        json!({ "submolt": "agents", "dryRun": true }),
    )
    .await;
    assert!(res.ok, "moltbook.subscribe dryRun failed: {:?}", res.error);
    let p = res
        .payload
        .expect("missing moltbook.subscribe dryRun payload");
    assert_eq!(p.get("method").and_then(|v| v.as_str()), Some("POST"));
    assert!(p
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .contains("/submolts/agents/subscribe"),);

    // moltbook.subscribe unsubscribe dryRun
    let res = send_req(
        &mut ws,
        "mb_unsub_dry",
        "moltbook.subscribe",
        json!({ "submolt": "agents", "unsubscribe": true, "dryRun": true }),
    )
    .await;
    assert!(
        res.ok,
        "moltbook.subscribe unsub dryRun failed: {:?}",
        res.error
    );
    let p = res
        .payload
        .expect("missing moltbook.subscribe unsub payload");
    assert_eq!(p.get("method").and_then(|v| v.as_str()), Some("DELETE"));

    // moltbook.dm send dryRun
    let res = send_req(
        &mut ws,
        "mb_dm_dry",
        "moltbook.dm",
        json!({ "action": "send", "to": "friendbot", "message": "hey!", "dryRun": true }),
    )
    .await;
    assert!(res.ok, "moltbook.dm send dryRun failed: {:?}", res.error);
    let p = res
        .payload
        .expect("missing moltbook.dm send dryRun payload");
    assert_eq!(p.get("method").and_then(|v| v.as_str()), Some("POST"));
    assert!(p
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .contains("/agents/dm/send"),);

    // moltbook.identity token dryRun
    let res = send_req(
        &mut ws,
        "mb_identity_dry",
        "moltbook.identity",
        json!({ "action": "token", "dryRun": true }),
    )
    .await;
    assert!(
        res.ok,
        "moltbook.identity token dryRun failed: {:?}",
        res.error
    );
    let p = res
        .payload
        .expect("missing moltbook.identity dryRun payload");
    assert_eq!(p.get("method").and_then(|v| v.as_str()), Some("POST"));
    assert!(p
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .contains("/agents/me/identity-token"),);

    // moltbook.request dryRun (generic tool also benefits from reorder)
    let res = send_req(
        &mut ws,
        "mb_req_dry",
        "moltbook.request",
        json!({ "method": "GET", "path": "/feed", "dryRun": true }),
    )
    .await;
    assert!(res.ok, "moltbook.request dryRun failed: {:?}", res.error);
    let p = res
        .payload
        .expect("missing moltbook.request dryRun payload");
    assert_eq!(p.get("dryRun").and_then(|v| v.as_bool()), Some(true));

    // ---------------------------------------------------------------
    // 2. Validation errors (before API key / write checks).
    // ---------------------------------------------------------------

    // moltbook.post missing required fields
    let res = send_req(
        &mut ws,
        "mb_post_bad",
        "moltbook.post",
        json!({ "title": "", "content": "x", "submolt": "s" }),
    )
    .await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("INVALID_REQUEST")
    );

    // moltbook.comment missing postId
    let res = send_req(
        &mut ws,
        "mb_comment_bad",
        "moltbook.comment",
        json!({ "content": "hello" }),
    )
    .await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("INVALID_REQUEST")
    );

    // moltbook.vote missing postId
    let res = send_req(&mut ws, "mb_vote_bad", "moltbook.vote", json!({})).await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("INVALID_REQUEST")
    );

    // moltbook.search missing query
    let res = send_req(&mut ws, "mb_search_bad", "moltbook.search", json!({})).await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("INVALID_REQUEST")
    );

    // moltbook.follow missing agent
    let res = send_req(&mut ws, "mb_follow_bad", "moltbook.follow", json!({})).await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("INVALID_REQUEST")
    );

    // moltbook.subscribe missing submolt
    let res = send_req(&mut ws, "mb_sub_bad", "moltbook.subscribe", json!({})).await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("INVALID_REQUEST")
    );

    // ---------------------------------------------------------------
    // 3. Read-only tools return NOT_LINKED without API key.
    //    (Proves dispatch wiring works end-to-end.)
    // ---------------------------------------------------------------

    // moltbook.feed
    let res = send_req(
        &mut ws,
        "mb_feed_nokey",
        "moltbook.feed",
        json!({ "sort": "new", "limit": 5 }),
    )
    .await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("NOT_LINKED")
    );

    // moltbook.search
    let res = send_req(
        &mut ws,
        "mb_search_nokey",
        "moltbook.search",
        json!({ "query": "hello" }),
    )
    .await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("NOT_LINKED")
    );

    // moltbook.identity profile
    let res = send_req(
        &mut ws,
        "mb_identity_nokey",
        "moltbook.identity",
        json!({ "action": "profile" }),
    )
    .await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("NOT_LINKED")
    );

    // moltbook.dm check
    let res = send_req(
        &mut ws,
        "mb_dm_nokey",
        "moltbook.dm",
        json!({ "action": "check" }),
    )
    .await;
    assert!(!res.ok);
    assert_eq!(
        res.error.as_ref().map(|e| e.code.as_str()),
        Some("NOT_LINKED")
    );

    drop(ws);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn tools_invoke_web_fetch_registered_and_ssrf_blocked() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

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

    let url = format!("http://127.0.0.1:{}/tools/invoke", port);
    let client = reqwest::Client::new();
    let res = client
        .post(url)
        .json(&json!({
            "tool": "web_fetch",
            "args": { "url": "http://127.0.0.1/" },
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status().as_u16(), 400, "expected tool_error response");
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body.get("ok").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(
        body.get("error")
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str()),
        Some("tool_error")
    );
    let msg = body
        .get("error")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        msg.contains("blocked by SSRF policy"),
        "expected SSRF policy block, got: {}",
        msg
    );

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn tools_invoke_respects_session_tool_policy_deny() {
    let _permit = env_permit().await;

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let config = test_config(port);
    let base = config
        .storage
        .database_path
        .parent()
        .expect("db path missing parent")
        .to_path_buf();
    let sidecar_path = base
        .join("agents")
        .join("default")
        .join("sessions")
        .join("sessions.json");
    std::fs::create_dir_all(sidecar_path.parent().unwrap()).unwrap();
    std::fs::write(
        &sidecar_path,
        serde_json::to_string_pretty(&json!({
            "version": 1,
            "entries": {
                "agent:default:test-session": {
                    "toolPolicy": {
                        "web_fetch": "deny"
                    }
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

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

    let url = format!("http://127.0.0.1:{}/tools/invoke", port);
    let client = reqwest::Client::new();
    let res = client
        .post(url)
        .json(&json!({
            "tool": "web_fetch",
            "sessionKey": "test-session",
            "args": { "url": "http://127.0.0.1/" },
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status().as_u16(), 404, "expected not_found response");
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body.get("ok").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(
        body.get("error")
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str()),
        Some("not_found")
    );

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn tools_invoke_respects_group_tool_policy_rules() {
    let _permit = env_permit().await;

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "drbot_gateway=debug,drbot=info".into()),
        )
        .with_test_writer()
        .try_init();

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let config = test_config(port);
    let base = config
        .storage
        .database_path
        .parent()
        .expect("db path missing parent")
        .to_path_buf();
    let policy_path = base.join("tool-policy.json");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(
        &policy_path,
        serde_json::to_string_pretty(&json!({
            "version": 1,
            "rules": [
                {
                    "match": { "keyPrefix": "whatsapp:group:" },
                    "tools": { "allow": ["read"] }
                },
                {
                    "match": { "keyPrefix": "whatsapp:group:trusted" },
                    "tools": { "allow": ["read", "exec"] }
                },
                {
                    "match": { "keyPrefix": "telegram:group:123" },
                    "tools": { "allow": ["read"] }
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

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

    let url = format!("http://127.0.0.1:{}/tools/invoke", port);
    let client = reqwest::Client::new();

    // whatsapp wildcard (read-only): exec should be unavailable
    let res = client
        .post(&url)
        .json(&json!({
            "tool": "exec",
            "sessionKey": "whatsapp:group:unknown",
            "dryRun": true,
            "args": { "cmd": "echo hi" },
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 404, "expected not_found response");

    // whatsapp trusted override: exec should be available
    let res = client
        .post(&url)
        .json(&json!({
            "tool": "exec",
            "sessionKey": "whatsapp:group:trusted",
            "dryRun": true,
            "args": { "cmd": "echo hi" },
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let payload: serde_json::Value = res.json().await.unwrap();
    assert_eq!(payload.get("ok").and_then(|v| v.as_bool()), Some(true));

    // telegram topic sessions should match the group prefix rule
    let res = client
        .post(&url)
        .json(&json!({
            "tool": "exec",
            "sessionKey": "telegram:group:123:topic:456",
            "dryRun": true,
            "args": { "cmd": "echo hi" },
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 404, "expected not_found response");

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}
