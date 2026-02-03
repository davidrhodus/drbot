//! Hook manager for registering and executing hooks.

use crate::types::{BoxedHook, Hook, HookContext, HookEvent, HookResult, HookTiming};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Registered hook entry.
struct HookEntry {
    hook: BoxedHook,
    priority: i32,
}

/// Hook manager for registering and executing hooks.
pub struct HookManager {
    /// Hooks indexed by event type.
    hooks: Arc<RwLock<HashMap<HookEvent, Vec<HookEntry>>>>,
}

impl HookManager {
    /// Create a new hook manager.
    pub fn new() -> Self {
        Self {
            hooks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a hook.
    pub async fn register(&self, hook: impl Hook + 'static) {
        let hook = Arc::new(hook);
        self.register_boxed(hook).await;
    }

    /// Register a boxed hook.
    pub async fn register_boxed(&self, hook: BoxedHook) {
        let name = hook.name().to_string();
        let events = hook.events();
        let priority = hook.priority();

        let mut hooks = self.hooks.write().await;

        for event in events {
            let entries = hooks.entry(event.clone()).or_insert_with(Vec::new);
            entries.push(HookEntry {
                hook: hook.clone(),
                priority,
            });
            // Sort by priority (lower first)
            entries.sort_by_key(|e| e.priority);

            debug!(
                hook = %name,
                event = ?event,
                priority = priority,
                "Registered hook"
            );
        }

        info!(hook = %name, "Hook registered");
    }

    /// Unregister a hook by name.
    pub async fn unregister(&self, name: &str) {
        let mut hooks = self.hooks.write().await;

        for entries in hooks.values_mut() {
            entries.retain(|e| e.hook.name() != name);
        }

        info!(hook = %name, "Hook unregistered");
    }

    /// Execute hooks for an event.
    pub async fn execute(
        &self,
        event: &HookEvent,
        timing: HookTiming,
        context: &HookContext,
    ) -> HookResult {
        let hooks = self.hooks.read().await;

        let Some(entries) = hooks.get(event) else {
            return HookResult::ok();
        };

        let mut current_context = context.clone();
        let mut final_result = HookResult::ok();

        for entry in entries {
            // Skip disabled hooks
            if !entry.hook.enabled() {
                continue;
            }

            // Skip hooks with different timing
            if entry.hook.timing() != timing {
                continue;
            }

            debug!(
                hook = %entry.hook.name(),
                event = ?event,
                timing = ?timing,
                "Executing hook"
            );

            let result = entry.hook.execute(&current_context).await;

            // Apply modifications
            if let Some(ref modified_message) = result.modified_message {
                current_context.message = Some(modified_message.clone());
                final_result.modified_message = Some(modified_message.clone());
            }

            if let Some(ref modified_metadata) = result.modified_metadata {
                current_context.metadata = modified_metadata.clone();
                final_result.modified_metadata = Some(modified_metadata.clone());
            }

            // Check for errors or stop
            if let Some(ref error) = result.error {
                error!(
                    hook = %entry.hook.name(),
                    error = %error,
                    "Hook returned error"
                );
                return result;
            }

            if !result.continue_processing {
                warn!(
                    hook = %entry.hook.name(),
                    "Hook stopped processing"
                );
                return result;
            }
        }

        final_result
    }

    /// Execute pre-hooks for an event.
    pub async fn execute_pre(&self, event: &HookEvent, context: &HookContext) -> HookResult {
        self.execute(event, HookTiming::Pre, context).await
    }

    /// Execute post-hooks for an event.
    pub async fn execute_post(&self, event: &HookEvent, context: &HookContext) -> HookResult {
        self.execute(event, HookTiming::Post, context).await
    }

    /// Get all registered hook names.
    pub async fn list_hooks(&self) -> Vec<String> {
        let hooks = self.hooks.read().await;
        let mut names: Vec<String> = hooks
            .values()
            .flat_map(|entries| entries.iter().map(|e| e.hook.name().to_string()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Check if a hook is registered.
    pub async fn is_registered(&self, name: &str) -> bool {
        let hooks = self.hooks.read().await;
        hooks
            .values()
            .any(|entries| entries.iter().any(|e| e.hook.name() == name))
    }
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct TestHook {
        name: String,
        events: Vec<HookEvent>,
        timing: HookTiming,
        priority: i32,
    }

    #[async_trait]
    impl Hook for TestHook {
        fn name(&self) -> &str {
            &self.name
        }

        fn events(&self) -> Vec<HookEvent> {
            self.events.clone()
        }

        fn timing(&self) -> HookTiming {
            self.timing
        }

        fn priority(&self) -> i32 {
            self.priority
        }

        async fn execute(&self, _context: &HookContext) -> HookResult {
            HookResult::ok()
        }
    }

    #[tokio::test]
    async fn test_register_and_list() {
        let manager = HookManager::new();

        manager
            .register(TestHook {
                name: "test-hook".to_string(),
                events: vec![HookEvent::MessageReceived],
                timing: HookTiming::Pre,
                priority: 0,
            })
            .await;

        let hooks = manager.list_hooks().await;
        assert!(hooks.contains(&"test-hook".to_string()));
    }

    #[tokio::test]
    async fn test_unregister() {
        let manager = HookManager::new();

        manager
            .register(TestHook {
                name: "test-hook".to_string(),
                events: vec![HookEvent::MessageReceived],
                timing: HookTiming::Pre,
                priority: 0,
            })
            .await;

        assert!(manager.is_registered("test-hook").await);

        manager.unregister("test-hook").await;

        assert!(!manager.is_registered("test-hook").await);
    }

    #[tokio::test]
    async fn test_execute_pre() {
        let manager = HookManager::new();

        manager
            .register(TestHook {
                name: "pre-hook".to_string(),
                events: vec![HookEvent::MessageReceived],
                timing: HookTiming::Pre,
                priority: 0,
            })
            .await;

        let context = HookContext::new(HookEvent::MessageReceived);
        let result = manager
            .execute_pre(&HookEvent::MessageReceived, &context)
            .await;

        assert!(result.continue_processing);
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        use std::sync::atomic::{AtomicU32, Ordering};

        static COUNTER: AtomicU32 = AtomicU32::new(0);
        static FIRST_ORDER: AtomicU32 = AtomicU32::new(0);
        static SECOND_ORDER: AtomicU32 = AtomicU32::new(0);

        struct OrderedHook {
            name: String,
            priority: i32,
            order_var: &'static AtomicU32,
        }

        #[async_trait]
        impl Hook for OrderedHook {
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
                self.priority
            }

            async fn execute(&self, _context: &HookContext) -> HookResult {
                let order = COUNTER.fetch_add(1, Ordering::SeqCst);
                self.order_var.store(order, Ordering::SeqCst);
                HookResult::ok()
            }
        }

        let manager = HookManager::new();

        // Register second priority first
        manager
            .register(OrderedHook {
                name: "second".to_string(),
                priority: 10,
                order_var: &SECOND_ORDER,
            })
            .await;

        // Register first priority second
        manager
            .register(OrderedHook {
                name: "first".to_string(),
                priority: 1,
                order_var: &FIRST_ORDER,
            })
            .await;

        COUNTER.store(0, Ordering::SeqCst);

        let context = HookContext::new(HookEvent::MessageReceived);
        manager
            .execute_pre(&HookEvent::MessageReceived, &context)
            .await;

        // First should run before second
        assert!(FIRST_ORDER.load(Ordering::SeqCst) < SECOND_ORDER.load(Ordering::SeqCst));
    }
}
