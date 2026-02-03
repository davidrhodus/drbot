//! Destructor utilities for drbot.
//!
//! This crate provides:
//! - Destructor wrappers
//! - Cleanup scheduling
//! - Destructor ordering

use thiserror::Error;

/// Destructor error types.
#[derive(Error, Debug, Clone)]
pub enum DestructorError {
    #[error("Destructor failed: {0}")]
    Failed(String),

    #[error("Already destructed")]
    AlreadyDestructed,
}

/// Result type for destructor operations.
pub type Result<T> = std::result::Result<T, DestructorError>;

/// Destructor callback.
pub type DestructorFn = Box<dyn FnOnce() + Send>;

/// Destructor queue.
#[derive(Default)]
pub struct DestructorQueue {
    callbacks: Vec<DestructorFn>,
}

impl DestructorQueue {
    /// Create new queue.
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
        }
    }

    /// Add destructor.
    pub fn add<F: FnOnce() + Send + 'static>(&mut self, f: F) {
        self.callbacks.push(Box::new(f));
    }

    /// Run all destructors.
    pub fn run_all(&mut self) {
        while let Some(f) = self.callbacks.pop() {
            f();
        }
    }

    /// Count pending.
    pub fn pending(&self) -> usize {
        self.callbacks.len()
    }

    /// Clear without running.
    pub fn clear(&mut self) {
        self.callbacks.clear();
    }
}

impl Drop for DestructorQueue {
    fn drop(&mut self) {
        self.run_all();
    }
}

/// Ordered destructor.
pub struct OrderedDestructor<T> {
    value: T,
    order: i32,
}

impl<T> OrderedDestructor<T> {
    /// Create with order (lower runs first).
    pub fn new(value: T, order: i32) -> Self {
        Self { value, order }
    }

    /// Get order.
    pub fn order(&self) -> i32 {
        self.order
    }

    /// Get value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable value.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> std::ops::Deref for OrderedDestructor<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

/// Destructor group.
pub struct DestructorGroup {
    destructors: Vec<(i32, DestructorFn)>,
}

impl DestructorGroup {
    /// Create new group.
    pub fn new() -> Self {
        Self {
            destructors: Vec::new(),
        }
    }

    /// Add destructor with order.
    pub fn add<F: FnOnce() + Send + 'static>(&mut self, order: i32, f: F) {
        self.destructors.push((order, Box::new(f)));
    }

    /// Run destructors in order.
    pub fn run_ordered(&mut self) {
        self.destructors.sort_by_key(|(order, _)| *order);
        for (_, f) in self.destructors.drain(..) {
            f();
        }
    }

    /// Count pending.
    pub fn pending(&self) -> usize {
        self.destructors.len()
    }
}

impl Default for DestructorGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DestructorGroup {
    fn drop(&mut self) {
        self.run_ordered();
    }
}

/// Fallible destructor.
pub struct FallibleDestructor<F: FnOnce() -> Result<()>> {
    func: Option<F>,
}

impl<F: FnOnce() -> Result<()>> FallibleDestructor<F> {
    /// Create new.
    pub fn new(f: F) -> Self {
        Self { func: Some(f) }
    }

    /// Run destructor.
    pub fn run(mut self) -> Result<()> {
        if let Some(f) = self.func.take() {
            f()
        } else {
            Err(DestructorError::AlreadyDestructed)
        }
    }

    /// Disarm.
    pub fn disarm(mut self) {
        self.func = None;
    }
}

impl<F: FnOnce() -> Result<()>> Drop for FallibleDestructor<F> {
    fn drop(&mut self) {
        if let Some(f) = self.func.take() {
            let _ = f(); // Ignore error in drop
        }
    }
}

/// Destructor chain.
pub struct DestructorChain {
    chain: Vec<DestructorFn>,
}

impl DestructorChain {
    /// Create new chain.
    pub fn new() -> Self {
        Self { chain: Vec::new() }
    }

    /// Add to chain (runs in reverse order).
    pub fn push<F: FnOnce() + Send + 'static>(&mut self, f: F) {
        self.chain.push(Box::new(f));
    }

    /// Run chain.
    pub fn run(&mut self) {
        while let Some(f) = self.chain.pop() {
            f();
        }
    }

    /// Length.
    pub fn len(&self) -> usize {
        self.chain.len()
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.chain.is_empty()
    }
}

impl Default for DestructorChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DestructorChain {
    fn drop(&mut self) {
        self.run();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_destructor_queue() {
        let order = Arc::new(Mutex::new(Vec::new()));

        {
            let mut queue = DestructorQueue::new();
            let o1 = order.clone();
            queue.add(move || o1.lock().unwrap().push(1));
            let o2 = order.clone();
            queue.add(move || o2.lock().unwrap().push(2));
        }

        // Runs in reverse order (stack)
        assert_eq!(*order.lock().unwrap(), vec![2, 1]);
    }

    #[test]
    fn test_destructor_group() {
        let order = Arc::new(Mutex::new(Vec::new()));

        {
            let mut group = DestructorGroup::new();
            let o1 = order.clone();
            group.add(2, move || o1.lock().unwrap().push(2));
            let o2 = order.clone();
            group.add(1, move || o2.lock().unwrap().push(1));
            let o3 = order.clone();
            group.add(3, move || o3.lock().unwrap().push(3));
        }

        // Runs in order
        assert_eq!(*order.lock().unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn test_destructor_chain() {
        let order = Arc::new(Mutex::new(Vec::new()));

        {
            let mut chain = DestructorChain::new();
            let o1 = order.clone();
            chain.push(move || o1.lock().unwrap().push(1));
            let o2 = order.clone();
            chain.push(move || o2.lock().unwrap().push(2));
        }

        // Runs in reverse (LIFO)
        assert_eq!(*order.lock().unwrap(), vec![2, 1]);
    }
}
