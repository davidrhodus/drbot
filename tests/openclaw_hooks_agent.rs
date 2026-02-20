//! OpenClaw-style /hooks/agent integration tests.

use drbot_core::config::CliProviderConfig;
use drbot_core::Config;
use drbot_gateway::Gateway;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

fn base_test_config(port: u16) -> Config {
    // Keep tests deterministic even if claude/codex CLIs are installed locally.
    std::env::set_var("DRBOT_AUTO_DISABLE_CLI_PRESETS", "1");

    let mut config = Config::default();
    config.gateway.host = "127.0.0.1".to_string();
    config.gateway.port = port;
    // Keep gateway auth disabled for these tests.
    config.gateway.auth_token = None;

    config.hooks.enabled = true;
    config.hooks.token = Some("secret".to_string());

    // Avoid writing into the user's real data dir during tests.
    let base = std::env::temp_dir().join(format!(
        "drbot-openclaw-hooks-agent-test-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&base).unwrap();
    config.storage.database_path = base.join("drbot.db");
    config.storage.media_path = base.join("media");

    config
}

fn config_with_provider(port: u16) -> Config {
    let mut config = base_test_config(port);

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

    config
}

async fn spawn_gateway(
    config: Config,
) -> (
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
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
    (shutdown_tx, server_handle)
}

#[tokio::test]
async fn openclaw_hooks_agent_success_returns_result_text() {
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

    let config = config_with_provider(port);
    let (shutdown_tx, server_handle) = spawn_gateway(config).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("http://127.0.0.1:{}/hooks/agent", port);
    let client = reqwest::Client::new();

    let resp = client
        .post(&url)
        .header("authorization", "Bearer secret")
        .json(&json!({
            "id": "hook-agent-1",
            "agentId": "default",
            "message": "hello",
            "timeoutMs": 5000
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body = resp.json::<Value>().await.unwrap();

    assert_eq!(body.get("ok"), Some(&Value::Bool(true)));
    assert_eq!(
        body.get("agentId").and_then(|v| v.as_str()),
        Some("default")
    );
    assert_eq!(
        body.get("sessionKey").and_then(|v| v.as_str()),
        Some("agent:default:main")
    );
    assert_eq!(
        body.get("messageId").and_then(|v| v.as_str()),
        Some("hook-agent-1")
    );
    assert_eq!(body.get("stream"), Some(&Value::Bool(false)));

    let text = body
        .get("result")
        .and_then(|v| v.get("text"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(text.contains("ok"), "unexpected result text: {:?}", text);

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}

#[tokio::test]
async fn openclaw_hooks_agent_without_provider_returns_error_shape() {
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

    let config = base_test_config(port);
    let (shutdown_tx, server_handle) = spawn_gateway(config).await;

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("http://127.0.0.1:{}/hooks/agent", port);
    let client = reqwest::Client::new();

    let resp = client
        .post(&url)
        .header("authorization", "Bearer secret")
        .json(&json!({
            "id": "hook-agent-2",
            "agentId": "default",
            "message": "hello",
            "timeoutMs": 5000
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let body = resp.json::<Value>().await.unwrap();

    assert_eq!(body.get("ok"), Some(&Value::Bool(false)));
    assert_eq!(
        body.get("error")
            .and_then(|v| v.get("code"))
            .and_then(|v| v.as_str()),
        Some("UNAVAILABLE")
    );
    let msg = body
        .get("error")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        msg.contains("provider"),
        "expected provider-related error, got: {:?}",
        msg
    );

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}
