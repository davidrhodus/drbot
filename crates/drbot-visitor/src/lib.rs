//! Visitor pattern utilities for drbot.
//!
//! This crate provides:
//! - Visitor trait
//! - Element visitation
//! - Composite visitors

use std::sync::Arc;
use thiserror::Error;

/// Visitor error types.
#[derive(Error, Debug)]
pub enum VisitorError {
    #[error("Visit failed: {0}")]
    VisitFailed(String),

    #[error("Element rejected")]
    Rejected,
}

/// Result type for visitor operations.
pub type Result<T> = std::result::Result<T, VisitorError>;

/// Visitor trait for visiting elements.
pub trait Visitor<E>: Send + Sync {
    /// Visit an element.
    fn visit(&self, element: &E);
}

/// Mutable visitor trait.
pub trait MutVisitor<E>: Send + Sync {
    /// Visit and potentially modify an element.
    fn visit_mut(&self, element: &mut E);
}

/// Element that can accept visitors.
pub trait Visitable<V> {
    /// Accept a visitor.
    fn accept(&self, visitor: &V);
}

/// Mutable visitable element.
pub trait MutVisitable<V> {
    /// Accept a mutable visitor.
    fn accept_mut(&mut self, visitor: &V);
}

/// Function-based visitor.
pub struct FnVisitor<E, F: Fn(&E) + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<E>,
}

impl<E, F: Fn(&E) + Send + Sync> FnVisitor<E, F> {
    /// Create new function visitor.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<E: Send + Sync, F: Fn(&E) + Send + Sync> Visitor<E> for FnVisitor<E, F> {
    fn visit(&self, element: &E) {
        (self.func)(element)
    }
}

/// Function-based mutable visitor.
pub struct FnMutVisitor<E, F: Fn(&mut E) + Send + Sync> {
    func: F,
    _marker: std::marker::PhantomData<E>,
}

impl<E, F: Fn(&mut E) + Send + Sync> FnMutVisitor<E, F> {
    /// Create new mutable function visitor.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<E: Send + Sync, F: Fn(&mut E) + Send + Sync> MutVisitor<E> for FnMutVisitor<E, F> {
    fn visit_mut(&self, element: &mut E) {
        (self.func)(element)
    }
}

/// Composite visitor that applies multiple visitors.
pub struct CompositeVisitor<E> {
    visitors: Vec<Arc<dyn Visitor<E>>>,
}

impl<E> CompositeVisitor<E> {
    /// Create new composite visitor.
    pub fn new() -> Self {
        Self {
            visitors: Vec::new(),
        }
    }

    /// Add visitor.
    pub fn add(&mut self, visitor: Arc<dyn Visitor<E>>) {
        self.visitors.push(visitor);
    }

    /// Add visitor (builder pattern).
    pub fn with(mut self, visitor: Arc<dyn Visitor<E>>) -> Self {
        self.add(visitor);
        self
    }
}

impl<E> Default for CompositeVisitor<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Visitor<E> for CompositeVisitor<E> {
    fn visit(&self, element: &E) {
        for visitor in &self.visitors {
            visitor.visit(element);
        }
    }
}

/// Collecting visitor that accumulates results.
pub struct CollectingVisitor<E, T> {
    collector: std::sync::Mutex<Vec<T>>,
    extractor: Box<dyn Fn(&E) -> T + Send + Sync>,
}

impl<E, T> CollectingVisitor<E, T> {
    /// Create new collecting visitor.
    pub fn new<F>(extractor: F) -> Self
    where
        F: Fn(&E) -> T + Send + Sync + 'static,
    {
        Self {
            collector: std::sync::Mutex::new(Vec::new()),
            extractor: Box::new(extractor),
        }
    }

    /// Get collected results.
    pub fn results(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.collector.lock().unwrap().clone()
    }

    /// Take collected results.
    pub fn take_results(&self) -> Vec<T> {
        std::mem::take(&mut *self.collector.lock().unwrap())
    }

    /// Clear collected results.
    pub fn clear(&self) {
        self.collector.lock().unwrap().clear();
    }
}

impl<E: Send + Sync, T: Send> Visitor<E> for CollectingVisitor<E, T> {
    fn visit(&self, element: &E) {
        let value = (self.extractor)(element);
        self.collector.lock().unwrap().push(value);
    }
}

/// Counting visitor.
pub struct CountingVisitor {
    count: std::sync::atomic::AtomicUsize,
}

impl CountingVisitor {
    /// Create new counting visitor.
    pub fn new() -> Self {
        Self {
            count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Get count.
    pub fn count(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Reset count.
    pub fn reset(&self) {
        self.count.store(0, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Default for CountingVisitor {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Visitor<E> for CountingVisitor {
    fn visit(&self, _element: &E) {
        self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Filtering visitor that only visits matching elements.
pub struct FilteringVisitor<E, P: Fn(&E) -> bool + Send + Sync> {
    predicate: P,
    inner: Arc<dyn Visitor<E>>,
}

impl<E, P: Fn(&E) -> bool + Send + Sync> FilteringVisitor<E, P> {
    /// Create new filtering visitor.
    pub fn new(predicate: P, inner: Arc<dyn Visitor<E>>) -> Self {
        Self { predicate, inner }
    }
}

impl<E, P: Fn(&E) -> bool + Send + Sync> Visitor<E> for FilteringVisitor<E, P> {
    fn visit(&self, element: &E) {
        if (self.predicate)(element) {
            self.inner.visit(element);
        }
    }
}

/// Element walker for traversing structures.
pub struct Walker<E> {
    visitor: Arc<dyn Visitor<E>>,
}

impl<E> Walker<E> {
    /// Create new walker.
    pub fn new(visitor: Arc<dyn Visitor<E>>) -> Self {
        Self { visitor }
    }

    /// Walk a single element.
    pub fn walk(&self, element: &E) {
        self.visitor.visit(element);
    }

    /// Walk multiple elements.
    pub fn walk_all<'a>(&self, elements: impl IntoIterator<Item = &'a E>)
    where
        E: 'a,
    {
        for element in elements {
            self.visitor.visit(element);
        }
    }
}

/// Helper to create visitor.
pub fn visitor<E: Send + Sync + 'static, F>(func: F) -> Arc<dyn Visitor<E>>
where
    F: Fn(&E) + Send + Sync + 'static,
{
    Arc::new(FnVisitor::new(func))
}

/// Helper to create mutable visitor.
pub fn mut_visitor<E: Send + Sync + 'static, F>(func: F) -> Arc<dyn MutVisitor<E>>
where
    F: Fn(&mut E) + Send + Sync + 'static,
{
    Arc::new(FnMutVisitor::new(func))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fn_visitor() {
        let sum = std::sync::atomic::AtomicI32::new(0);
        let visitor = FnVisitor::new(|n: &i32| {
            sum.fetch_add(*n, std::sync::atomic::Ordering::SeqCst);
        });

        visitor.visit(&10);
        visitor.visit(&20);
        visitor.visit(&12);

        assert_eq!(sum.load(std::sync::atomic::Ordering::SeqCst), 42);
    }

    #[test]
    fn test_composite_visitor() {
        let counter = Arc::new(CountingVisitor::new());
        let collector: Arc<CollectingVisitor<i32, i32>> =
            Arc::new(CollectingVisitor::new(|n| *n * 2));

        let composite = CompositeVisitor::new()
            .with(counter.clone() as Arc<dyn Visitor<i32>>)
            .with(collector.clone() as Arc<dyn Visitor<i32>>);

        composite.visit(&5);
        composite.visit(&10);

        assert_eq!(counter.count(), 2);
        assert_eq!(collector.results(), vec![10, 20]);
    }

    #[test]
    fn test_filtering_visitor() {
        let counter = Arc::new(CountingVisitor::new());
        let filtering =
            FilteringVisitor::new(|n: &i32| *n > 10, counter.clone() as Arc<dyn Visitor<i32>>);

        filtering.visit(&5);
        filtering.visit(&15);
        filtering.visit(&8);
        filtering.visit(&20);

        assert_eq!(counter.count(), 2); // Only 15 and 20
    }

    #[test]
    fn test_walker() {
        let collector: Arc<CollectingVisitor<i32, i32>> = Arc::new(CollectingVisitor::new(|n| *n));
        let walker = Walker::new(collector.clone() as Arc<dyn Visitor<i32>>);

        walker.walk_all(&[1, 2, 3, 4, 5]);

        assert_eq!(collector.results(), vec![1, 2, 3, 4, 5]);
    }
}
