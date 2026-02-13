//! Browser automation for drbot.
//!
//! This crate provides browser automation via Chrome DevTools Protocol.
//!
//! # Features
//!
//! - Launch or connect to Chrome/Chromium
//! - Navigate pages
//! - Take screenshots
//! - Evaluate JavaScript
//! - Click elements and type text
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_browser::{Browser, BrowserOptions};
//!
//! async fn example() -> drbot_core::Result<()> {
//!     // Launch a new browser
//!     let browser = Browser::launch(BrowserOptions::default()).await?;
//!
//!     // Create a new page
//!     let page = browser.new_page().await?;
//!
//!     // Navigate and take screenshot
//!     page.navigate("https://example.com").await?;
//!     page.wait_for_load().await?;
//!     let screenshot = page.screenshot(Default::default()).await?;
//!
//!     browser.close().await?;
//!     Ok(())
//! }
//! ```

mod browser;
mod cdp;
mod console;
mod network;
mod page;
mod session;

pub use browser::{
    Browser, BrowserDownload, BrowserNetworkRequest, BrowserOptions, BrowserPageError,
    BrowserSetCookie, TargetInfo, VersionInfo,
};
pub use console::{ConsoleConfig, ConsoleMessage, ConsoleMonitor, LogLevel};
pub use network::{
    HttpMethod, NetworkConfig, NetworkEntry, NetworkEvent, NetworkMonitor, NetworkRequest,
    NetworkResponse, ResourceType,
};
pub use page::{
    EvaluateResult, ExceptionDetails, NavigateOptions, Page, RemoteObject, ScreenshotFormat,
    ScreenshotOptions, Viewport, WaitUntil,
};
pub use session::{BrowserSession, Cookie, SessionData, SessionManager, StorageEntry};

/// High-level browser automation interface.
pub struct BrowserAutomation {
    browser: Browser,
}

impl BrowserAutomation {
    /// Create a new automation instance by launching a browser.
    pub async fn new() -> drbot_core::Result<Self> {
        let browser = Browser::launch(BrowserOptions::default()).await?;
        Ok(Self { browser })
    }

    /// Create with custom options.
    pub async fn with_options(options: BrowserOptions) -> drbot_core::Result<Self> {
        let browser = Browser::launch(options).await?;
        Ok(Self { browser })
    }

    /// Connect to an existing browser.
    pub async fn connect(ws_url: &str) -> drbot_core::Result<Self> {
        let browser = Browser::connect(ws_url).await?;
        Ok(Self { browser })
    }

    /// Take a screenshot of a URL.
    pub async fn screenshot_url(&self, url: &str) -> drbot_core::Result<Vec<u8>> {
        let page = self.browser.new_page().await?;
        page.navigate(url).await?;
        page.wait_for_load().await?;

        // Wait a bit more for rendering
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let screenshot = page.screenshot(ScreenshotOptions::default()).await?;

        self.browser.close_page(page.target_id()).await?;

        Ok(screenshot)
    }

    /// Take a full-page screenshot.
    pub async fn screenshot_full_page(&self, url: &str) -> drbot_core::Result<Vec<u8>> {
        let page = self.browser.new_page().await?;
        page.navigate(url).await?;
        page.wait_for_load().await?;

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let options = ScreenshotOptions {
            full_page: true,
            ..Default::default()
        };
        let screenshot = page.screenshot(options).await?;

        self.browser.close_page(page.target_id()).await?;

        Ok(screenshot)
    }

    /// Get page content (HTML).
    pub async fn get_content(&self, url: &str) -> drbot_core::Result<String> {
        let page = self.browser.new_page().await?;
        page.navigate(url).await?;
        page.wait_for_load().await?;

        let content = page.content().await?;

        self.browser.close_page(page.target_id()).await?;

        Ok(content)
    }

    /// Evaluate JavaScript on a page.
    pub async fn evaluate(&self, url: &str, script: &str) -> drbot_core::Result<serde_json::Value> {
        let page = self.browser.new_page().await?;
        page.navigate(url).await?;
        page.wait_for_load().await?;

        let result = page.evaluate(script).await?;

        self.browser.close_page(page.target_id()).await?;

        Ok(result)
    }

    /// Get the underlying browser.
    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    /// Close the automation and browser.
    pub async fn close(self) -> drbot_core::Result<()> {
        self.browser.close().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_options_default() {
        let opts = BrowserOptions::default();
        assert!(opts.headless);
    }

    #[test]
    fn test_screenshot_options_default() {
        let opts = ScreenshotOptions::default();
        assert!(!opts.full_page);
        assert!(opts.quality.is_none());
    }

    #[test]
    fn test_exports() {
        // Just verify types are exported correctly
        let _: fn() -> BrowserOptions = BrowserOptions::default;
        let _: fn() -> ScreenshotOptions = ScreenshotOptions::default;
    }
}
