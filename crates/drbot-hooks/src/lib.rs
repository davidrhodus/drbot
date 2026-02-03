//! Hook system for drbot.
//!
//! This crate provides a flexible hook system for processing messages and events.
//!
//! # Features
//!
//! - Pre/post hook timing
//! - Multiple event types
//! - Priority-based execution order
//! - Built-in hooks (logging, filtering, rate limiting, transforms)
//!
//! # Example
//!
//! ```rust,no_run
//! use drbot_hooks::{HookManager, LoggingHook, FilterHook, HookContext, HookEvent};
//!
//! async fn example() {
//!     let manager = HookManager::new();
//!
//!     // Register a logging hook
//!     manager.register(LoggingHook::new("logger")).await;
//!
//!     // Register a filter hook
//!     manager.register(
//!         FilterHook::new("profanity-filter")
//!             .block_pattern("badword")
//!     ).await;
//!
//!     // Execute hooks
//!     let context = HookContext::new(HookEvent::MessageReceived)
//!         .with_message("hello world");
//!     let result = manager.execute_pre(&HookEvent::MessageReceived, &context).await;
//!
//!     if result.continue_processing {
//!         // Process the message
//!     }
//! }
//! ```

mod builtin;
mod manager;
mod types;

pub use builtin::{FilterHook, LoggingHook, RateLimitHook, TransformHook};
pub use manager::HookManager;
pub use types::{BoxedHook, Hook, HookConfig, HookContext, HookEvent, HookResult, HookTiming};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exports() {
        // Verify types are exported
        let _: fn() -> HookManager = HookManager::new;
        let _: fn(HookEvent) -> HookContext = HookContext::new;
    }

    #[tokio::test]
    async fn test_integration() {
        let manager = HookManager::new();

        // Register hooks
        manager.register(LoggingHook::new("logger")).await;
        manager
            .register(FilterHook::new("filter").block_pattern("spam"))
            .await;
        manager
            .register(TransformHook::new("transform").replace("hello", "hi"))
            .await;

        // Test allowed message
        let context = HookContext::new(HookEvent::MessageReceived).with_message("hello world");
        let result = manager
            .execute_pre(&HookEvent::MessageReceived, &context)
            .await;

        assert!(result.continue_processing);
        assert_eq!(result.modified_message, Some("hi world".to_string()));

        // Test blocked message
        let context = HookContext::new(HookEvent::MessageReceived).with_message("this is spam");
        let result = manager
            .execute_pre(&HookEvent::MessageReceived, &context)
            .await;

        assert!(!result.continue_processing);
    }
}
