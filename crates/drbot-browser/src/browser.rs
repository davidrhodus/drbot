//! Browser instance management.

use crate::cdp::CdpConnection;
use crate::page::Page;
use serde::Deserialize;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Browser launch options.
#[derive(Debug, Clone)]
pub struct BrowserOptions {
    /// Path to Chrome/Chromium executable.
    pub executable: Option<String>,
    /// Run headless.
    pub headless: bool,
    /// Additional arguments.
    pub args: Vec<String>,
    /// User data directory.
    pub user_data_dir: Option<String>,
    /// Remote debugging port.
    pub port: u16,
}

impl Default for BrowserOptions {
    fn default() -> Self {
        Self {
            executable: None,
            headless: true,
            args: vec![],
            user_data_dir: None,
            port: 0, // Random port
        }
    }
}

/// Chrome DevTools JSON endpoint response.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionInfo {
    /// Browser name.
    #[serde(rename = "Browser")]
    pub browser: String,
    /// Protocol version.
    #[serde(rename = "Protocol-Version")]
    pub protocol_version: String,
    /// V8 version.
    #[serde(rename = "V8-Version")]
    pub v8_version: Option<String>,
    /// WebKit version.
    #[serde(rename = "WebKit-Version")]
    pub webkit_version: Option<String>,
    /// WebSocket debugger URL.
    #[serde(rename = "webSocketDebuggerUrl")]
    pub websocket_debugger_url: Option<String>,
}

/// Target info from CDP.
#[derive(Debug, Clone, Deserialize)]
pub struct TargetInfo {
    /// Target ID.
    #[serde(rename = "targetId")]
    pub target_id: String,
    /// Target type.
    #[serde(rename = "type")]
    pub target_type: String,
    /// Title.
    pub title: String,
    /// URL.
    pub url: String,
    /// Whether attached.
    pub attached: Option<bool>,
}

/// Browser instance.
pub struct Browser {
    /// CDP connection.
    cdp: Arc<CdpConnection>,
    /// Event receiver.
    _events: mpsc::Receiver<crate::cdp::CdpEvent>,
    /// Child process (if launched).
    process: Option<Child>,
    /// WebSocket URL.
    ws_url: String,
}

impl Browser {
    /// Connect to an existing browser instance.
    pub async fn connect(ws_url: &str) -> drbot_core::Result<Self> {
        info!("Connecting to browser at: {}", ws_url);

        let (cdp, events) = CdpConnection::connect(ws_url).await?;
        let cdp = Arc::new(cdp);

        // Enable necessary domains
        cdp.send(
            "Target.setDiscoverTargets",
            Some(serde_json::json!({"discover": true})),
        )
        .await?;

        Ok(Self {
            cdp,
            _events: events,
            process: None,
            ws_url: ws_url.to_string(),
        })
    }

    /// Launch a new browser instance.
    pub async fn launch(options: BrowserOptions) -> drbot_core::Result<Self> {
        let executable = options
            .executable
            .or_else(find_chrome_executable)
            .ok_or_else(|| {
                drbot_core::Error::NotFound("Chrome executable not found".to_string())
            })?;

        let port = if options.port == 0 {
            // Find an available port
            let listener = std::net::TcpListener::bind("127.0.0.1:0")
                .map_err(|e| drbot_core::Error::Internal(format!("Failed to bind port: {}", e)))?;
            listener.local_addr().unwrap().port()
        } else {
            options.port
        };

        let mut args = vec![
            format!("--remote-debugging-port={}", port),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
        ];

        if options.headless {
            args.push("--headless=new".to_string());
        }

        if let Some(ref user_data_dir) = options.user_data_dir {
            args.push(format!("--user-data-dir={}", user_data_dir));
        }

        args.extend(options.args);

        info!("Launching browser: {} {:?}", executable, args);

        let process = Command::new(&executable)
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| drbot_core::Error::Internal(format!("Failed to launch browser: {}", e)))?;

        // Wait for browser to start and get WebSocket URL
        let ws_url = wait_for_browser(port).await?;

        let (cdp, events) = CdpConnection::connect(&ws_url).await?;
        let cdp = Arc::new(cdp);

        cdp.send(
            "Target.setDiscoverTargets",
            Some(serde_json::json!({"discover": true})),
        )
        .await?;

        Ok(Self {
            cdp,
            _events: events,
            process: Some(process),
            ws_url,
        })
    }

    /// Create a new page (tab).
    pub async fn new_page(&self) -> drbot_core::Result<Page> {
        let result = self
            .cdp
            .send(
                "Target.createTarget",
                Some(serde_json::json!({"url": "about:blank"})),
            )
            .await?;

        let target_id = result
            .get("targetId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| drbot_core::Error::Internal("No targetId in response".to_string()))?
            .to_string();

        // Attach to target
        let attach_result = self
            .cdp
            .send(
                "Target.attachToTarget",
                Some(serde_json::json!({
                    "targetId": target_id,
                    "flatten": true,
                })),
            )
            .await?;

        let session_id = attach_result
            .get("sessionId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Enable Page domain
        self.cdp.send("Page.enable", None).await?;

        Ok(Page::new(self.cdp.clone(), target_id, session_id))
    }

    /// Get all pages (tabs).
    pub async fn pages(&self) -> drbot_core::Result<Vec<TargetInfo>> {
        let result = self.cdp.send("Target.getTargets", None).await?;

        let targets: Vec<TargetInfo> = result
            .get("targetInfos")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        Ok(targets
            .into_iter()
            .filter(|t| t.target_type == "page")
            .collect())
    }

    /// Close a page by target ID.
    pub async fn close_page(&self, target_id: &str) -> drbot_core::Result<()> {
        self.cdp
            .send(
                "Target.closeTarget",
                Some(serde_json::json!({"targetId": target_id})),
            )
            .await?;
        Ok(())
    }

    /// Get browser version info.
    pub async fn version(&self) -> drbot_core::Result<serde_json::Value> {
        self.cdp.send("Browser.getVersion", None).await
    }

    /// Close the browser.
    pub async fn close(mut self) -> drbot_core::Result<()> {
        info!("Closing browser");

        // Try to close gracefully via CDP
        let _ = self.cdp.send("Browser.close", None).await;

        // Kill process if we launched it
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
        }

        Ok(())
    }

    /// Get WebSocket URL.
    pub fn ws_url(&self) -> &str {
        &self.ws_url
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
        }
    }
}

/// Find Chrome executable on the system.
fn find_chrome_executable() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let paths = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ];
        for path in paths {
            if std::path::Path::new(path).exists() {
                return Some(path.to_string());
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let paths = [
            "/usr/bin/google-chrome",
            "/usr/bin/chromium-browser",
            "/usr/bin/chromium",
            "/usr/bin/microsoft-edge",
        ];
        for path in paths {
            if std::path::Path::new(path).exists() {
                return Some(path.to_string());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let paths = [
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        ];
        for path in paths {
            if std::path::Path::new(path).exists() {
                return Some(path.to_string());
            }
        }
    }

    None
}

/// Wait for browser to be ready and return WebSocket URL.
async fn wait_for_browser(port: u16) -> drbot_core::Result<String> {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/json/version", port);

    for attempt in 0..50 {
        match client.get(&url).send().await {
            Ok(resp) => {
                if let Ok(info) = resp.json::<VersionInfo>().await {
                    if let Some(ws_url) = info.websocket_debugger_url {
                        debug!("Browser ready after {} attempts", attempt + 1);
                        return Ok(ws_url);
                    }
                }
            }
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }

    Err(drbot_core::Error::Timeout(
        "Browser failed to start".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_options_default() {
        let opts = BrowserOptions::default();
        assert!(opts.headless);
        assert!(opts.executable.is_none());
        assert_eq!(opts.port, 0);
    }

    #[test]
    fn test_version_info_deserialize() {
        let json = r#"{
            "Browser": "Chrome/120.0.0.0",
            "Protocol-Version": "1.3",
            "webSocketDebuggerUrl": "ws://127.0.0.1:9222/devtools/browser/abc123"
        }"#;
        let info: VersionInfo = serde_json::from_str(json).unwrap();
        assert!(info.browser.contains("Chrome"));
        assert!(info.websocket_debugger_url.is_some());
    }

    #[test]
    fn test_target_info_deserialize() {
        let json = r#"{
            "targetId": "ABC123",
            "type": "page",
            "title": "Test Page",
            "url": "https://example.com",
            "attached": false
        }"#;
        let info: TargetInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.target_id, "ABC123");
        assert_eq!(info.target_type, "page");
    }

    #[test]
    fn test_find_chrome_executable() {
        // This test just ensures the function doesn't panic
        let _ = find_chrome_executable();
    }
}
