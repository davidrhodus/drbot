//! Plugin runtime for drbot.
//!
//! This crate provides a plugin system for extending drbot functionality.
//!
//! # Features
//!
//! - Plugin loading and lifecycle management
//! - Event dispatch to plugins
//! - Plugin configuration
//! - Capability-based permissions
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_plugins::{Plugin, PluginManager, PluginMetadata, PluginContext, PluginEvent, PluginResponse};
//! use async_trait::async_trait;
//!
//! struct MyPlugin {
//!     metadata: PluginMetadata,
//! }
//!
//! impl MyPlugin {
//!     fn new() -> Self {
//!         Self {
//!             metadata: PluginMetadata::new("my-plugin", "1.0.0")
//!                 .with_description("My custom plugin"),
//!         }
//!     }
//! }
//!
//! #[async_trait]
//! impl Plugin for MyPlugin {
//!     fn metadata(&self) -> &PluginMetadata {
//!         &self.metadata
//!     }
//!
//!     async fn handle_event(
//!         &self,
//!         event: &PluginEvent,
//!         context: &PluginContext,
//!     ) -> drbot_core::Result<PluginResponse> {
//!         // Handle events here
//!         Ok(PluginResponse::unhandled())
//!     }
//! }
//!
//! async fn example() {
//!     let manager = PluginManager::new();
//!     manager.load(Box::new(MyPlugin::new())).await.unwrap();
//!     manager.start("my-plugin").await.unwrap();
//! }
//! ```

mod manager;
mod types;
pub mod wasm;

pub use manager::PluginManager;
pub use types::{
    BoxedPlugin, Plugin, PluginCapability, PluginContext, PluginEvent, PluginMetadata,
    PluginResponse, PluginState,
};
pub use wasm::{WasmError, WasmPlugin, WasmPluginConfig, WasmPluginManifest, WasmRuntime};

/// Re-export async_trait for plugin implementations.
pub use async_trait::async_trait;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exports() {
        // Verify types are exported
        let _ = PluginManager::new();
        let _ = PluginMetadata::new("test", "1.0.0");
    }

    #[tokio::test]
    async fn test_integration() {
        struct EchoPlugin {
            metadata: PluginMetadata,
        }

        impl EchoPlugin {
            fn new() -> Self {
                Self {
                    metadata: PluginMetadata::new("echo", "1.0.0").with_description("Echo plugin"),
                }
            }
        }

        #[async_trait]
        impl Plugin for EchoPlugin {
            fn metadata(&self) -> &PluginMetadata {
                &self.metadata
            }

            async fn handle_event(
                &self,
                event: &PluginEvent,
                _context: &PluginContext,
            ) -> drbot_core::Result<PluginResponse> {
                match event {
                    PluginEvent::Message { content, .. } => {
                        Ok(PluginResponse::with_message(format!("Echo: {}", content)))
                    }
                    _ => Ok(PluginResponse::unhandled()),
                }
            }
        }

        let manager = PluginManager::new();
        manager.load(Box::new(EchoPlugin::new())).await.unwrap();
        manager.start("echo").await.unwrap();

        let event = PluginEvent::Message {
            session_id: "s1".to_string(),
            user_id: "u1".to_string(),
            content: "Hello".to_string(),
        };

        let responses = manager.dispatch(&event).await;
        assert_eq!(responses.len(), 1);
        assert!(responses[0].handled);
        assert_eq!(responses[0].message, Some("Echo: Hello".to_string()));

        manager.stop("echo").await.unwrap();
    }
}
