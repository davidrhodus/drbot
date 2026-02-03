//! Mobile device protocol definitions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request to a mobile device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileRequest {
    /// Request ID.
    pub id: Uuid,
    /// Request type.
    #[serde(rename = "type")]
    pub request_type: String,
    /// Request parameters.
    #[serde(default)]
    pub params: serde_json::Value,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl MobileRequest {
    /// Create a new request.
    pub fn new(request_type: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            request_type: request_type.to_string(),
            params: serde_json::json!({}),
            timestamp: Utc::now(),
        }
    }

    /// Create with parameters.
    pub fn with_params(request_type: &str, params: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            request_type: request_type.to_string(),
            params,
            timestamp: Utc::now(),
        }
    }

    /// Take a photo request.
    pub fn take_photo(camera: Option<&str>) -> Self {
        Self::with_params(
            "take_photo",
            serde_json::json!({
                "camera": camera.unwrap_or("back")
            }),
        )
    }

    /// Take a screenshot request.
    pub fn screenshot() -> Self {
        Self::new("screenshot")
    }

    /// Get notifications request.
    pub fn get_notifications(since: Option<DateTime<Utc>>) -> Self {
        Self::with_params(
            "get_notifications",
            serde_json::json!({
                "since": since.map(|t| t.to_rfc3339())
            }),
        )
    }

    /// Start screen mirroring request.
    pub fn start_screen_mirror() -> Self {
        Self::new("start_screen_mirror")
    }

    /// Stop screen mirroring request.
    pub fn stop_screen_mirror() -> Self {
        Self::new("stop_screen_mirror")
    }

    /// Get device info request.
    pub fn get_device_info() -> Self {
        Self::new("get_device_info")
    }

    /// Ping request.
    pub fn ping() -> Self {
        Self::new("ping")
    }
}

/// Response from a mobile device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MobileResponse {
    /// Request ID this is responding to.
    pub id: Uuid,
    /// Whether the request succeeded.
    pub success: bool,
    /// Response data.
    #[serde(default)]
    pub data: serde_json::Value,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

impl MobileResponse {
    /// Create a success response.
    pub fn success(id: Uuid, data: serde_json::Value) -> Self {
        Self {
            id,
            success: true,
            data,
            error: None,
            timestamp: Utc::now(),
        }
    }

    /// Create an error response.
    pub fn error(id: Uuid, error: &str) -> Self {
        Self {
            id,
            success: false,
            data: serde_json::json!({}),
            error: Some(error.to_string()),
            timestamp: Utc::now(),
        }
    }
}

/// Event from a mobile device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MobileEvent {
    /// Device connected.
    Connected {
        device_id: String,
        device_name: String,
    },
    /// Device disconnected.
    Disconnected {
        device_id: String,
        reason: Option<String>,
    },
    /// Photo captured.
    Photo {
        device_id: String,
        /// Base64-encoded image data.
        data: String,
        format: String,
        width: u32,
        height: u32,
    },
    /// Screenshot captured.
    Screenshot {
        device_id: String,
        /// Base64-encoded image data.
        data: String,
        format: String,
        width: u32,
        height: u32,
    },
    /// Screen frame (for mirroring).
    ScreenFrame {
        device_id: String,
        /// Base64-encoded frame data.
        data: String,
        format: String,
        sequence: u64,
    },
    /// New notification received.
    Notification {
        device_id: String,
        app: String,
        title: String,
        body: Option<String>,
        timestamp: DateTime<Utc>,
    },
    /// Error occurred.
    Error { device_id: String, error: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mobile_request() {
        let request = MobileRequest::take_photo(Some("front"));
        assert_eq!(request.request_type, "take_photo");
        assert_eq!(request.params["camera"], "front");
    }

    #[test]
    fn test_mobile_response() {
        let id = Uuid::new_v4();
        let response = MobileResponse::success(id, serde_json::json!({"key": "value"}));
        assert!(response.success);
        assert_eq!(response.id, id);
    }

    #[test]
    fn test_mobile_event_serialization() {
        let event = MobileEvent::Connected {
            device_id: "test".to_string(),
            device_name: "Test Device".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("connected"));
    }
}
