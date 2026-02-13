//! OpenClaw-style /hooks/wake integration tests.

use drbot_core::config::CliProviderConfig;
use drbot_core::Config;
use drbot_gateway::Gateway;
use serde_json::{json, Value};
use std::time::Duration;
use uuid::Uuid;

fn test_config(port: u16) -> Config {
    let mut config = Config::default();
    config.gateway.host = "127.0.0.1".to_string();
    config.gateway.port = port;
    // Keep gateway auth disabled for these tests.
    config.gateway.auth_token = None;

    config.hooks.enabled = true;
    config.hooks.token = Some("secret".to_string());

    // Use a deterministic provider for background heartbeats during tests (no network).
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
    let base = std::env::temp_dir().join(format!(
        "drbot-openclaw-hooks-wake-test-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&base).unwrap();
    config.storage.database_path = base.join("drbot.db");
    config.storage.media_path = base.join("media");

    config
}

async fn post_wake(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> (reqwest::StatusCode, Value, Option<String>) {
    let resp = client
        .post(url)
        .header("authorization", format!("Bearer {}", token))
        .json(&json!({ "message": "ping" }))
        .send()
        .await
        .unwrap();

    let status = resp.status();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let body = resp.json::<Value>().await.unwrap();
    (status, body, retry_after)
}

#[tokio::test]
async fn openclaw_hooks_wake_auth_and_throttle() {
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

    let url = format!("http://127.0.0.1:{}/hooks/wake", port);
    let client = reqwest::Client::new();

    // A few failures should remain 401 and not block subsequent valid requests.
    for _ in 0..9 {
        let (status, body, retry_after) = post_wake(&client, &url, "wrong").await;
        assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
        assert_eq!(body.get("ok"), Some(&Value::Bool(false)));
        assert!(retry_after.is_none());
    }

    let (status, body, retry_after) = post_wake(&client, &url, "secret").await;
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(body.get("ok"), Some(&Value::Bool(true)));
    assert!(retry_after.is_none());

    // Repeated failures should eventually return 429 + Retry-After.
    for _ in 0..9 {
        let (status, body, retry_after) = post_wake(&client, &url, "wrong").await;
        assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
        assert_eq!(body.get("ok"), Some(&Value::Bool(false)));
        assert!(retry_after.is_none());
    }

    let (status, body, retry_after) = post_wake(&client, &url, "wrong").await;
    assert_eq!(status, reqwest::StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body.get("ok"), Some(&Value::Bool(false)));
    let retry_after = retry_after.expect("expected Retry-After header");
    assert!(retry_after.parse::<u64>().unwrap_or(0) >= 1);

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(3), server_handle)
        .await
        .expect("server did not shut down")
        .unwrap();
}
