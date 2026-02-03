//! Built-in hooks.

use crate::types::{Hook, HookContext, HookEvent, HookResult, HookTiming};
use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;

/// Logging hook that logs all events.
pub struct LoggingHook {
    name: String,
    events: Vec<HookEvent>,
    timing: HookTiming,
    enabled: AtomicBool,
}

impl LoggingHook {
    /// Create a new logging hook for all events.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            events: vec![
                HookEvent::MessageReceived,
                HookEvent::MessageToProvider,
                HookEvent::ProviderResponse,
                HookEvent::MessageToChannel,
            ],
            timing: HookTiming::Pre,
            enabled: AtomicBool::new(true),
        }
    }

    /// Set specific events to log.
    pub fn with_events(mut self, events: Vec<HookEvent>) -> Self {
        self.events = events;
        self
    }

    /// Set timing.
    pub fn with_timing(mut self, timing: HookTiming) -> Self {
        self.timing = timing;
        self
    }

    /// Enable or disable the hook.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }
}

#[async_trait]
impl Hook for LoggingHook {
    fn name(&self) -> &str {
        &self.name
    }

    fn events(&self) -> Vec<HookEvent> {
        self.events.clone()
    }

    fn timing(&self) -> HookTiming {
        self.timing
    }

    fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    async fn execute(&self, context: &HookContext) -> HookResult {
        info!(
            hook = %self.name,
            event = ?context.event,
            session = ?context.session_id,
            channel = ?context.channel_id,
            user = ?context.user_id,
            message = ?context.message.as_ref().map(|m| if m.len() > 50 {
                format!("{}...", &m[..50])
            } else {
                m.clone()
            }),
            "Hook event"
        );
        HookResult::ok()
    }
}

/// Filter hook that can block messages based on patterns.
pub struct FilterHook {
    name: String,
    blocked_patterns: Vec<String>,
    enabled: AtomicBool,
}

impl FilterHook {
    /// Create a new filter hook.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            blocked_patterns: vec![],
            enabled: AtomicBool::new(true),
        }
    }

    /// Add a blocked pattern.
    pub fn block_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.blocked_patterns.push(pattern.into());
        self
    }

    /// Add multiple blocked patterns.
    pub fn block_patterns(mut self, patterns: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.blocked_patterns
            .extend(patterns.into_iter().map(|p| p.into()));
        self
    }

    /// Enable or disable.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }
}

#[async_trait]
impl Hook for FilterHook {
    fn name(&self) -> &str {
        &self.name
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::MessageReceived]
    }

    fn timing(&self) -> HookTiming {
        HookTiming::Pre
    }

    fn priority(&self) -> i32 {
        -100 // Run early to filter before other hooks
    }

    fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    async fn execute(&self, context: &HookContext) -> HookResult {
        let Some(message) = &context.message else {
            return HookResult::ok();
        };

        let message_lower = message.to_lowercase();

        for pattern in &self.blocked_patterns {
            if message_lower.contains(&pattern.to_lowercase()) {
                info!(
                    hook = %self.name,
                    pattern = %pattern,
                    "Message blocked by filter"
                );
                return HookResult::stop();
            }
        }

        HookResult::ok()
    }
}

/// Transform hook that modifies messages.
pub struct TransformHook {
    name: String,
    replacements: Vec<(String, String)>,
    prefix: Option<String>,
    suffix: Option<String>,
    enabled: AtomicBool,
}

impl TransformHook {
    /// Create a new transform hook.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            replacements: vec![],
            prefix: None,
            suffix: None,
            enabled: AtomicBool::new(true),
        }
    }

    /// Add a replacement.
    pub fn replace(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.replacements.push((from.into(), to.into()));
        self
    }

    /// Add a prefix to all messages.
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Add a suffix to all messages.
    pub fn with_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = Some(suffix.into());
        self
    }

    /// Enable or disable.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }
}

#[async_trait]
impl Hook for TransformHook {
    fn name(&self) -> &str {
        &self.name
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::MessageReceived]
    }

    fn timing(&self) -> HookTiming {
        HookTiming::Pre
    }

    fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    async fn execute(&self, context: &HookContext) -> HookResult {
        let Some(message) = &context.message else {
            return HookResult::ok();
        };

        let mut modified = message.clone();

        // Apply replacements
        for (from, to) in &self.replacements {
            modified = modified.replace(from, to);
        }

        // Apply prefix/suffix
        if let Some(ref prefix) = self.prefix {
            modified = format!("{}{}", prefix, modified);
        }
        if let Some(ref suffix) = self.suffix {
            modified = format!("{}{}", modified, suffix);
        }

        if &modified != message {
            HookResult::with_message(modified)
        } else {
            HookResult::ok()
        }
    }
}

/// Rate limiting hook.
pub struct RateLimitHook {
    name: String,
    max_per_minute: u32,
    counts: std::sync::RwLock<std::collections::HashMap<String, (u32, std::time::Instant)>>,
    enabled: AtomicBool,
}

impl RateLimitHook {
    /// Create a new rate limit hook.
    pub fn new(name: impl Into<String>, max_per_minute: u32) -> Self {
        Self {
            name: name.into(),
            max_per_minute,
            counts: std::sync::RwLock::new(std::collections::HashMap::new()),
            enabled: AtomicBool::new(true),
        }
    }

    /// Enable or disable.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }
}

#[async_trait]
impl Hook for RateLimitHook {
    fn name(&self) -> &str {
        &self.name
    }

    fn events(&self) -> Vec<HookEvent> {
        vec![HookEvent::MessageReceived]
    }

    fn timing(&self) -> HookTiming {
        HookTiming::Pre
    }

    fn priority(&self) -> i32 {
        -50 // Run early
    }

    fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    async fn execute(&self, context: &HookContext) -> HookResult {
        let user_id = context
            .user_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let now = std::time::Instant::now();

        let mut counts = self.counts.write().unwrap();

        let (count, started) = counts.entry(user_id.clone()).or_insert((0, now));

        // Reset if minute has passed
        if now.duration_since(*started).as_secs() >= 60 {
            *count = 0;
            *started = now;
        }

        *count += 1;

        if *count > self.max_per_minute {
            info!(
                hook = %self.name,
                user = %user_id,
                count = %count,
                limit = %self.max_per_minute,
                "Rate limit exceeded"
            );
            return HookResult::error("Rate limit exceeded");
        }

        HookResult::ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_logging_hook() {
        let hook = LoggingHook::new("test-logger");
        assert_eq!(hook.name(), "test-logger");
        assert!(!hook.events().is_empty());

        let context = HookContext::new(HookEvent::MessageReceived).with_message("test message");
        let result = hook.execute(&context).await;
        assert!(result.continue_processing);
    }

    #[tokio::test]
    async fn test_filter_hook_allows() {
        let hook = FilterHook::new("test-filter").block_pattern("blocked");

        let context = HookContext::new(HookEvent::MessageReceived).with_message("hello world");
        let result = hook.execute(&context).await;
        assert!(result.continue_processing);
    }

    #[tokio::test]
    async fn test_filter_hook_blocks() {
        let hook = FilterHook::new("test-filter").block_pattern("blocked");

        let context =
            HookContext::new(HookEvent::MessageReceived).with_message("this is blocked content");
        let result = hook.execute(&context).await;
        assert!(!result.continue_processing);
    }

    #[tokio::test]
    async fn test_transform_hook() {
        let hook = TransformHook::new("test-transform")
            .replace("hello", "hi")
            .with_prefix("[BOT] ");

        let context = HookContext::new(HookEvent::MessageReceived).with_message("hello world");
        let result = hook.execute(&context).await;

        assert!(result.continue_processing);
        assert_eq!(result.modified_message, Some("[BOT] hi world".to_string()));
    }

    #[tokio::test]
    async fn test_rate_limit_hook() {
        let hook = RateLimitHook::new("test-rate", 2);

        let context = HookContext::new(HookEvent::MessageReceived).with_user("user-1");

        // First two should pass
        let result = hook.execute(&context).await;
        assert!(result.continue_processing);

        let result = hook.execute(&context).await;
        assert!(result.continue_processing);

        // Third should be rate limited
        let result = hook.execute(&context).await;
        assert!(!result.continue_processing);
        assert!(result.error.is_some());
    }
}
