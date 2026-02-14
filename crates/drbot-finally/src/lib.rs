//! Finally/cleanup handlers for drbot.
//!
//! This crate provides:
//! - Scope guards (RAII cleanup)
//! - Deferred execution
//! - Finally blocks

use std::mem::ManuallyDrop;

/// Scope guard that runs cleanup on drop.
pub struct ScopeGuard<F: FnOnce()> {
    cleanup: Option<F>,
}

impl<F: FnOnce()> ScopeGuard<F> {
    /// Create new scope guard.
    pub fn new(cleanup: F) -> Self {
        Self {
            cleanup: Some(cleanup),
        }
    }

    /// Cancel the cleanup.
    pub fn cancel(&mut self) {
        self.cleanup = None;
    }

    /// Run cleanup early.
    pub fn run_now(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

impl<F: FnOnce()> Drop for ScopeGuard<F> {
    fn drop(&mut self) {
        if let Some(cleanup) = self.cleanup.take() {
            cleanup();
        }
    }
}

/// Create scope guard with closure.
pub fn finally<F: FnOnce()>(cleanup: F) -> ScopeGuard<F> {
    ScopeGuard::new(cleanup)
}

/// Deferred action that can be scheduled and cancelled.
pub struct Deferred<F: FnOnce()> {
    action: ManuallyDrop<Option<F>>,
    should_run: bool,
}

impl<F: FnOnce()> Deferred<F> {
    /// Create new deferred action.
    pub fn new(action: F) -> Self {
        Self {
            action: ManuallyDrop::new(Some(action)),
            should_run: true,
        }
    }

    /// Cancel deferred action.
    pub fn cancel(&mut self) {
        self.should_run = false;
    }

    /// Re-enable deferred action.
    pub fn enable(&mut self) {
        self.should_run = true;
    }

    /// Run action now.
    pub fn run(mut self) {
        if let Some(action) = self.action.take() {
            action();
        }
        self.should_run = false;
    }

    /// Check if action will run.
    pub fn will_run(&self) -> bool {
        self.should_run && self.action.is_some()
    }
}

impl<F: FnOnce()> Drop for Deferred<F> {
    fn drop(&mut self) {
        if self.should_run {
            // Safety: We only take once
            if let Some(action) = unsafe { ManuallyDrop::take(&mut self.action) } {
                action();
            }
        }
    }
}

/// Create deferred action.
pub fn defer<F: FnOnce()>(action: F) -> Deferred<F> {
    Deferred::new(action)
}

/// Execute code with guaranteed cleanup.
pub fn with_finally<T, F, C>(code: F, cleanup: C) -> T
where
    F: FnOnce() -> T,
    C: FnOnce(),
{
    let _guard = finally(cleanup);
    code()
}

/// Stack of cleanup actions.
pub struct CleanupStack {
    actions: Vec<Box<dyn FnOnce()>>,
}

impl CleanupStack {
    /// Create new cleanup stack.
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    /// Push cleanup action.
    pub fn push<F: FnOnce() + 'static>(&mut self, action: F) {
        self.actions.push(Box::new(action));
    }

    /// Pop and run last action.
    pub fn pop_run(&mut self) {
        if let Some(action) = self.actions.pop() {
            action();
        }
    }

    /// Run all actions in reverse order.
    pub fn run_all(&mut self) {
        while let Some(action) = self.actions.pop() {
            action();
        }
    }

    /// Clear without running.
    pub fn clear(&mut self) {
        self.actions.clear();
    }

    /// Get number of pending actions.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

impl Default for CleanupStack {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CleanupStack {
    fn drop(&mut self) {
        self.run_all();
    }
}

/// On-success guard - only runs if explicitly triggered.
pub struct OnSuccess<F: FnOnce()> {
    action: Option<F>,
}

impl<F: FnOnce()> OnSuccess<F> {
    /// Create new on-success guard.
    pub fn new(action: F) -> Self {
        Self {
            action: Some(action),
        }
    }

    /// Mark as successful and run action.
    pub fn success(mut self) {
        if let Some(action) = self.action.take() {
            action();
        }
    }
}

impl<F: FnOnce()> Drop for OnSuccess<F> {
    fn drop(&mut self) {
        // Don't run on normal drop
    }
}

/// On-failure guard - only runs on panic or early return.
pub struct OnFailure<F: FnOnce()> {
    action: Option<F>,
    succeeded: bool,
}

impl<F: FnOnce()> OnFailure<F> {
    /// Create new on-failure guard.
    pub fn new(action: F) -> Self {
        Self {
            action: Some(action),
            succeeded: false,
        }
    }

    /// Mark as successful - action won't run.
    pub fn success(&mut self) {
        self.succeeded = true;
    }
}

impl<F: FnOnce()> Drop for OnFailure<F> {
    fn drop(&mut self) {
        if !self.succeeded {
            if let Some(action) = self.action.take() {
                action();
            }
        }
    }
}

/// Create on-failure guard.
pub fn on_failure<F: FnOnce()>(action: F) -> OnFailure<F> {
    OnFailure::new(action)
}

/// Resource with cleanup.
pub struct Resource<T, F: FnOnce(T)> {
    value: Option<T>,
    cleanup: Option<F>,
}

impl<T, F: FnOnce(T)> Resource<T, F> {
    /// Create new resource with cleanup.
    pub fn new(value: T, cleanup: F) -> Self {
        Self {
            value: Some(value),
            cleanup: Some(cleanup),
        }
    }

    /// Get reference to value.
    pub fn get(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// Get mutable reference to value.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.value.as_mut()
    }

    /// Take value without cleanup.
    pub fn take(mut self) -> Option<T> {
        self.cleanup = None;
        self.value.take()
    }
}

impl<T, F: FnOnce(T)> Drop for Resource<T, F> {
    fn drop(&mut self) {
        if let (Some(value), Some(cleanup)) = (self.value.take(), self.cleanup.take()) {
            cleanup(value);
        }
    }
}

/// Create resource with cleanup.
pub fn resource<T, F: FnOnce(T)>(value: T, cleanup: F) -> Resource<T, F> {
    Resource::new(value, cleanup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn test_scope_guard() {
        let called = Cell::new(false);
        {
            let _guard = finally(|| called.set(true));
        }
        assert!(called.get());
    }

    #[test]
    fn test_scope_guard_cancel() {
        let called = Cell::new(false);
        {
            let mut guard = finally(|| called.set(true));
            guard.cancel();
        }
        assert!(!called.get());
    }

    #[test]
    fn test_deferred() {
        let called = Cell::new(false);
        {
            let _d = defer(|| called.set(true));
        }
        assert!(called.get());
    }

    #[test]
    fn test_with_finally() {
        let called = Cell::new(false);
        let result = with_finally(|| 42, || called.set(true));
        assert_eq!(result, 42);
        assert!(called.get());
    }

    #[test]
    fn test_cleanup_stack() {
        let order = std::rc::Rc::new(Cell::new(Vec::new()));
        {
            let mut stack = CleanupStack::new();

            let order1 = order.clone();
            stack.push(move || {
                let mut v = order1.take();
                v.push(1);
                order1.set(v);
            });

            let order2 = order.clone();
            stack.push(move || {
                let mut v = order2.take();
                v.push(2);
                order2.set(v);
            });
        }
        // Should run in reverse order
        assert_eq!(order.take(), vec![2, 1]);
    }

    #[test]
    fn test_on_failure() {
        let called = Cell::new(false);
        {
            let mut guard = on_failure(|| called.set(true));
            guard.success();
        }
        assert!(!called.get());

        let called2 = Cell::new(false);
        {
            let _guard = on_failure(|| called2.set(true));
            // No success() called
        }
        assert!(called2.get());
    }

    #[test]
    fn test_resource() {
        let cleaned = Cell::new(false);
        {
            let _r = resource(42, |_| cleaned.set(true));
        }
        assert!(cleaned.get());
    }
}
