//! Browser page operations.

use crate::cdp::CdpConnection;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

/// Screenshot format.
#[derive(Debug, Clone, Copy, Default)]
pub enum ScreenshotFormat {
    #[default]
    Png,
    Jpeg,
    Webp,
}

impl ScreenshotFormat {
    fn as_str(&self) -> &str {
        match self {
            ScreenshotFormat::Png => "png",
            ScreenshotFormat::Jpeg => "jpeg",
            ScreenshotFormat::Webp => "webp",
        }
    }
}

/// Screenshot options.
#[derive(Debug, Clone, Default)]
pub struct ScreenshotOptions {
    /// Image format.
    pub format: ScreenshotFormat,
    /// Quality (1-100, only for JPEG/WebP).
    pub quality: Option<u8>,
    /// Capture full page.
    pub full_page: bool,
    /// Clip area.
    pub clip: Option<Viewport>,
}

/// Viewport/clip area.
#[derive(Debug, Clone, Serialize)]
pub struct Viewport {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
}

/// Navigation options.
#[derive(Debug, Clone, Default)]
pub struct NavigateOptions {
    /// Timeout in milliseconds.
    pub timeout: Option<u64>,
    /// Wait until event.
    pub wait_until: Option<WaitUntil>,
}

/// Wait until condition.
#[derive(Debug, Clone, Copy, Default)]
pub enum WaitUntil {
    /// DOMContentLoaded event.
    #[default]
    DomContentLoaded,
    /// Load event.
    Load,
    /// Network idle.
    NetworkIdle,
}

/// Evaluate result.
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluateResult {
    /// Result value.
    pub result: RemoteObject,
    /// Exception details if any.
    #[serde(rename = "exceptionDetails")]
    pub exception_details: Option<ExceptionDetails>,
}

/// Remote object from evaluation.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteObject {
    /// Object type.
    #[serde(rename = "type")]
    pub object_type: String,
    /// Primitive value.
    pub value: Option<serde_json::Value>,
    /// Description for non-primitive types.
    pub description: Option<String>,
}

/// Exception details.
#[derive(Debug, Clone, Deserialize)]
pub struct ExceptionDetails {
    /// Exception ID.
    #[serde(rename = "exceptionId")]
    pub exception_id: u64,
    /// Exception text.
    pub text: String,
    /// Line number.
    #[serde(rename = "lineNumber")]
    pub line_number: u32,
    /// Column number.
    #[serde(rename = "columnNumber")]
    pub column_number: u32,
}

/// A browser page.
pub struct Page {
    /// CDP connection.
    cdp: Arc<CdpConnection>,
    /// Target ID.
    target_id: String,
    /// Session ID.
    session_id: Option<String>,
}

impl Page {
    /// Create a new page handle.
    pub(crate) fn new(
        cdp: Arc<CdpConnection>,
        target_id: String,
        session_id: Option<String>,
    ) -> Self {
        Self {
            cdp,
            target_id,
            session_id,
        }
    }

    /// Navigate to a URL.
    pub async fn navigate(&self, url: &str) -> drbot_core::Result<String> {
        debug!("Navigating to: {}", url);

        let result = self
            .cdp
            .send_with_session(
                "Page.navigate",
                Some(serde_json::json!({ "url": url })),
                self.session_id.as_deref(),
            )
            .await?;

        let frame_id = result
            .get("frameId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(frame_id)
    }

    /// Wait for navigation to complete.
    pub async fn wait_for_load(&self) -> drbot_core::Result<()> {
        // Simple approach: wait for loadEventFired
        // In a real implementation, we'd listen for the event
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        Ok(())
    }

    /// Take a screenshot.
    pub async fn screenshot(&self, options: ScreenshotOptions) -> drbot_core::Result<Vec<u8>> {
        debug!("Taking screenshot");

        let mut params = serde_json::json!({
            "format": options.format.as_str(),
        });

        if let Some(quality) = options.quality {
            params["quality"] = serde_json::json!(quality);
        }

        if options.full_page {
            params["captureBeyondViewport"] = serde_json::json!(true);
        }

        if let Some(clip) = options.clip {
            params["clip"] = serde_json::to_value(clip)
                .map_err(|e| drbot_core::Error::Internal(e.to_string()))?;
        }

        let result = self
            .cdp
            .send_with_session(
                "Page.captureScreenshot",
                Some(params),
                self.session_id.as_deref(),
            )
            .await?;

        let data = result
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| drbot_core::Error::Internal("No screenshot data".to_string()))?;

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| drbot_core::Error::Internal(format!("Base64 decode failed: {}", e)))?;

        Ok(bytes)
    }

    /// Render the current page to PDF.
    pub async fn pdf(&self) -> drbot_core::Result<Vec<u8>> {
        debug!("Printing to PDF");

        let result = self
            .cdp
            .send_with_session(
                "Page.printToPDF",
                Some(serde_json::json!({
                    "printBackground": true,
                    "transferMode": "ReturnAsBase64",
                })),
                self.session_id.as_deref(),
            )
            .await?;

        let data = result
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| drbot_core::Error::Internal("No pdf data".to_string()))?;

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| drbot_core::Error::Internal(format!("Base64 decode failed: {}", e)))?;

        Ok(bytes)
    }

    /// Get page content (HTML).
    pub async fn content(&self) -> drbot_core::Result<String> {
        let result = self
            .cdp
            .send_with_session(
                "Runtime.evaluate",
                Some(serde_json::json!({
                    "expression": "document.documentElement.outerHTML",
                    "returnByValue": true,
                })),
                self.session_id.as_deref(),
            )
            .await?;

        let eval_result: EvaluateResult = serde_json::from_value(result)
            .map_err(|e| drbot_core::Error::Internal(format!("Parse result failed: {}", e)))?;

        if let Some(exception) = eval_result.exception_details {
            return Err(drbot_core::Error::Internal(format!(
                "JavaScript error: {}",
                exception.text
            )));
        }

        eval_result
            .result
            .value
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| drbot_core::Error::Internal("No content returned".to_string()))
    }

    /// Evaluate JavaScript.
    pub async fn evaluate(&self, expression: &str) -> drbot_core::Result<serde_json::Value> {
        debug!("Evaluating: {}", expression);

        let result = self
            .cdp
            .send_with_session(
                "Runtime.evaluate",
                Some(serde_json::json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
                self.session_id.as_deref(),
            )
            .await?;

        let eval_result: EvaluateResult = serde_json::from_value(result)
            .map_err(|e| drbot_core::Error::Internal(format!("Parse result failed: {}", e)))?;

        if let Some(exception) = eval_result.exception_details {
            return Err(drbot_core::Error::Internal(format!(
                "JavaScript error: {}",
                exception.text
            )));
        }

        Ok(eval_result.result.value.unwrap_or(serde_json::Value::Null))
    }

    /// Click an element by selector.
    pub async fn click(&self, selector: &str) -> drbot_core::Result<()> {
        debug!("Clicking: {}", selector);

        let script = format!(
            r#"
            (() => {{
                const el = document.querySelector({});
                if (!el) throw new Error('Element not found: {}');
                el.click();
                return true;
            }})()
            "#,
            serde_json::to_string(selector).unwrap(),
            selector
        );

        self.evaluate(&script).await?;
        Ok(())
    }

    /// Type text into an element.
    pub async fn type_text(&self, selector: &str, text: &str) -> drbot_core::Result<()> {
        debug!("Typing into: {}", selector);

        let script = format!(
            r#"
            (() => {{
                const el = document.querySelector({});
                if (!el) throw new Error('Element not found: {}');
                el.focus();
                el.value = {};
                el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                return true;
            }})()
            "#,
            serde_json::to_string(selector).unwrap(),
            selector,
            serde_json::to_string(text).unwrap(),
        );

        self.evaluate(&script).await?;
        Ok(())
    }

    /// Get the page title.
    pub async fn title(&self) -> drbot_core::Result<String> {
        let result = self.evaluate("document.title").await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    /// Get the page URL.
    pub async fn url(&self) -> drbot_core::Result<String> {
        let result = self.evaluate("window.location.href").await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    /// Set viewport size.
    pub async fn set_viewport(&self, width: u32, height: u32) -> drbot_core::Result<()> {
        self.cdp
            .send_with_session(
                "Emulation.setDeviceMetricsOverride",
                Some(serde_json::json!({
                    "width": width,
                    "height": height,
                    "deviceScaleFactor": 1,
                    "mobile": false,
                })),
                self.session_id.as_deref(),
            )
            .await?;
        Ok(())
    }

    /// Get target ID.
    pub fn target_id(&self) -> &str {
        &self.target_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screenshot_format() {
        assert_eq!(ScreenshotFormat::Png.as_str(), "png");
        assert_eq!(ScreenshotFormat::Jpeg.as_str(), "jpeg");
        assert_eq!(ScreenshotFormat::Webp.as_str(), "webp");
    }

    #[test]
    fn test_viewport_serialize() {
        let vp = Viewport {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
            scale: Some(2.0),
        };
        let json = serde_json::to_string(&vp).unwrap();
        assert!(json.contains("1920"));
        assert!(json.contains("scale"));
    }

    #[test]
    fn test_evaluate_result_deserialize() {
        let json = r#"{
            "result": {"type": "string", "value": "hello"},
            "exceptionDetails": null
        }"#;
        let result: EvaluateResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.result.object_type, "string");
        assert_eq!(result.result.value, Some(serde_json::json!("hello")));
    }
}
