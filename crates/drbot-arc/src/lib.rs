//! Arc utilities for drbot.
//!
//! This crate provides:
//! - Arc helper functions
//! - Arc swap utilities
//! - Arc collections

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};
use thiserror::Error;

/// Arc error types.
#[derive(Error, Debug, Clone)]
pub enum ArcError {
    #[error("No strong references remain")]
    NoStrongRefs,

    #[error("Arc is not unique")]
    NotUnique,
}

/// Result type for Arc operations.
pub type Result<T> = std::result::Result<T, ArcError>;

/// Arc with usage tracking.
pub struct TrackedArc<T> {
    inner: Arc<TrackedInner<T>>,
}

struct TrackedInner<T> {
    value: T,
    clone_count: AtomicUsize,
}

impl<T> TrackedArc<T> {
    /// Create new tracked Arc.
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(TrackedInner {
                value,
                clone_count: AtomicUsize::new(0),
            }),
        }
    }

    /// Get strong reference count.
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }

    /// Get weak reference count.
    pub fn weak_count(&self) -> usize {
        Arc::weak_count(&self.inner)
    }

    /// Get total times cloned.
    pub fn clone_count(&self) -> usize {
        self.inner.clone_count.load(Ordering::Relaxed)
    }

    /// Create weak reference.
    pub fn downgrade(&self) -> WeakTracked<T> {
        WeakTracked {
            inner: Arc::downgrade(&self.inner),
        }
    }

    /// Check if unique.
    pub fn is_unique(&self) -> bool {
        self.strong_count() == 1 && self.weak_count() == 0
    }

    /// Try to unwrap.
    pub fn try_unwrap(this: Self) -> std::result::Result<T, Self> {
        match Arc::try_unwrap(this.inner) {
            Ok(inner) => Ok(inner.value),
            Err(inner) => Err(Self { inner }),
        }
    }

    /// Get mutable if unique.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        Arc::get_mut(&mut self.inner).map(|inner| &mut inner.value)
    }

    /// Make mutable (clone if needed).
    pub fn make_mut(&mut self) -> &mut T
    where
        T: Clone,
    {
        if Arc::strong_count(&self.inner) != 1 || Arc::weak_count(&self.inner) != 0 {
            let value = self.inner.value.clone();
            *self = Self::new(value);
        }
        self.get_mut().unwrap()
    }

    /// Get as Arc.
    pub fn as_arc(&self) -> Arc<T>
    where
        T: Clone,
    {
        Arc::new(self.inner.value.clone())
    }
}

impl<T> Clone for TrackedArc<T> {
    fn clone(&self) -> Self {
        self.inner.clone_count.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> std::ops::Deref for TrackedArc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner.value
    }
}

// SAFETY: TrackedArc uses Arc which is Send + Sync
unsafe impl<T: Send + Sync> Send for TrackedArc<T> {}
unsafe impl<T: Send + Sync> Sync for TrackedArc<T> {}

/// Weak reference to TrackedArc.
pub struct WeakTracked<T> {
    inner: Weak<TrackedInner<T>>,
}

impl<T> WeakTracked<T> {
    /// Upgrade to strong reference.
    pub fn upgrade(&self) -> Option<TrackedArc<T>> {
        self.inner.upgrade().map(|inner| TrackedArc { inner })
    }

    /// Get strong count.
    pub fn strong_count(&self) -> usize {
        self.inner.strong_count()
    }

    /// Get weak count.
    pub fn weak_count(&self) -> usize {
        self.inner.weak_count()
    }
}

impl<T> Clone for WeakTracked<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// A swappable Arc.
pub struct ArcSwap<T> {
    ptr: std::sync::atomic::AtomicPtr<Arc<T>>,
}

impl<T> ArcSwap<T> {
    /// Create new ArcSwap.
    pub fn new(value: Arc<T>) -> Self {
        Self {
            ptr: std::sync::atomic::AtomicPtr::new(Box::into_raw(Box::new(value))),
        }
    }

    /// Load current Arc.
    pub fn load(&self) -> Arc<T> {
        let ptr = self.ptr.load(Ordering::Acquire);
        // SAFETY: ptr is always valid
        unsafe { (*ptr).clone() }
    }

    /// Store new Arc.
    pub fn store(&self, value: Arc<T>) {
        let new_ptr = Box::into_raw(Box::new(value));
        let old_ptr = self.ptr.swap(new_ptr, Ordering::AcqRel);
        // SAFETY: old_ptr was valid
        unsafe {
            drop(Box::from_raw(old_ptr));
        }
    }

    /// Swap and return old value.
    pub fn swap(&self, value: Arc<T>) -> Arc<T> {
        let new_ptr = Box::into_raw(Box::new(value));
        let old_ptr = self.ptr.swap(new_ptr, Ordering::AcqRel);
        // SAFETY: old_ptr was valid
        unsafe { *Box::from_raw(old_ptr) }
    }

    /// Update using a function.
    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&T) -> T,
    {
        let current = self.load();
        let new_value = f(&*current);
        self.store(Arc::new(new_value));
    }
}

impl<T> Drop for ArcSwap<T> {
    fn drop(&mut self) {
        let ptr = self.ptr.load(Ordering::Relaxed);
        // SAFETY: ptr is valid
        unsafe {
            drop(Box::from_raw(ptr));
        }
    }
}

// SAFETY: ArcSwap uses atomic operations
unsafe impl<T: Send + Sync> Send for ArcSwap<T> {}
unsafe impl<T: Send + Sync> Sync for ArcSwap<T> {}

/// Arc list - a list of Arc values.
pub struct ArcList<T> {
    items: Vec<Arc<T>>,
}

impl<T> ArcList<T> {
    /// Create empty list.
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Add item.
    pub fn push(&mut self, item: Arc<T>) {
        self.items.push(item);
    }

    /// Get item at index.
    pub fn get(&self, index: usize) -> Option<Arc<T>> {
        self.items.get(index).cloned()
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Iterate over items.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<T>> {
        self.items.iter()
    }

    /// Remove and return last item.
    pub fn pop(&mut self) -> Option<Arc<T>> {
        self.items.pop()
    }

    /// Clear list.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Retain items matching predicate.
    pub fn retain<F: FnMut(&Arc<T>) -> bool>(&mut self, f: F) {
        self.items.retain(f);
    }
}

impl<T> Default for ArcList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for ArcList<T> {
    fn clone(&self) -> Self {
        Self {
            items: self.items.clone(),
        }
    }
}

/// Convert helpers.
pub fn arc_from_box<T>(boxed: Box<T>) -> Arc<T> {
    Arc::from(boxed)
}

/// Clone inner value from Arc.
pub fn arc_clone_inner<T: Clone>(arc: &Arc<T>) -> T {
    (**arc).clone()
}

/// Check if two Arcs point to same allocation.
pub fn arc_ptr_eq<T>(a: &Arc<T>, b: &Arc<T>) -> bool {
    Arc::ptr_eq(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracked_arc() {
        let arc = TrackedArc::new(42);
        assert_eq!(*arc, 42);
        assert_eq!(arc.strong_count(), 1);
        assert_eq!(arc.clone_count(), 0);

        let arc2 = arc.clone();
        assert_eq!(arc.strong_count(), 2);
        assert_eq!(arc.clone_count(), 1);

        drop(arc2);
        assert_eq!(arc.strong_count(), 1);
    }

    #[test]
    fn test_weak_tracked() {
        let arc = TrackedArc::new(42);
        let weak = arc.downgrade();

        {
            let upgraded = weak.upgrade();
            assert!(upgraded.is_some());
        }

        drop(arc);
        let upgraded = weak.upgrade();
        assert!(upgraded.is_none());
    }

    #[test]
    fn test_arc_swap() {
        let swap = ArcSwap::new(Arc::new(42));
        assert_eq!(*swap.load(), 42);

        swap.store(Arc::new(100));
        assert_eq!(*swap.load(), 100);

        let old = swap.swap(Arc::new(200));
        assert_eq!(*old, 100);
        assert_eq!(*swap.load(), 200);
    }

    #[test]
    fn test_arc_list() {
        let mut list = ArcList::new();
        list.push(Arc::new(1));
        list.push(Arc::new(2));
        list.push(Arc::new(3));

        assert_eq!(list.len(), 3);
        assert_eq!(*list.get(1).unwrap(), 2);

        list.retain(|a| **a != 2);
        assert_eq!(list.len(), 2);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // TrackedArc Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_tracked_arc_new() {
        let value: i8 = kani::any();
        let arc = TrackedArc::new(value);

        kani::assert(*arc == value, "deref returns value");
        kani::assert(arc.strong_count() == 1, "initial strong count is 1");
        kani::assert(arc.weak_count() == 0, "initial weak count is 0");
        kani::assert(arc.clone_count() == 0, "initial clone count is 0");
    }

    #[kani::proof]
    fn proof_tracked_arc_clone() {
        let arc = TrackedArc::new(42i32);
        let arc2 = arc.clone();

        kani::assert(arc.strong_count() == 2, "strong count is 2 after clone");
        kani::assert(arc.clone_count() == 1, "clone count is 1 after one clone");
        kani::assert(*arc == *arc2, "cloned arcs have same value");
    }

    #[kani::proof]
    fn proof_tracked_arc_multiple_clones() {
        let arc = TrackedArc::new(42i32);
        let _arc2 = arc.clone();
        let _arc3 = arc.clone();

        kani::assert(arc.strong_count() == 3, "strong count is 3");
        kani::assert(arc.clone_count() == 2, "clone count is 2");
    }

    #[kani::proof]
    fn proof_tracked_arc_is_unique() {
        let arc = TrackedArc::new(42i32);

        kani::assert(arc.is_unique(), "single arc is unique");

        let arc2 = arc.clone();
        kani::assert(!arc.is_unique(), "cloned arc is not unique");

        drop(arc2);
        kani::assert(arc.is_unique(), "unique again after drop");
    }

    #[kani::proof]
    fn proof_tracked_arc_downgrade() {
        let arc = TrackedArc::new(42i32);
        let weak = arc.downgrade();

        kani::assert(arc.weak_count() == 1, "weak count is 1 after downgrade");
        kani::assert(weak.strong_count() == 1, "weak sees strong count");
    }

    #[kani::proof]
    fn proof_tracked_arc_get_mut_unique() {
        let mut arc = TrackedArc::new(42i32);

        if let Some(val) = arc.get_mut() {
            *val = 100;
        }

        kani::assert(*arc == 100, "get_mut allows modification");
    }

    #[kani::proof]
    fn proof_tracked_arc_get_mut_not_unique() {
        let mut arc = TrackedArc::new(42i32);
        let _arc2 = arc.clone();

        let result = arc.get_mut();
        kani::assert(result.is_none(), "get_mut returns None when not unique");
    }

    #[kani::proof]
    fn proof_tracked_arc_try_unwrap_unique() {
        let arc = TrackedArc::new(42i32);
        let result = TrackedArc::try_unwrap(arc);

        kani::assert(result.is_ok(), "try_unwrap succeeds when unique");
        kani::assert(result.unwrap() == 42, "unwrapped value is correct");
    }

    #[kani::proof]
    fn proof_tracked_arc_try_unwrap_not_unique() {
        let arc = TrackedArc::new(42i32);
        let _arc2 = arc.clone();

        let result = TrackedArc::try_unwrap(arc);
        kani::assert(result.is_err(), "try_unwrap fails when not unique");
    }

    // ========================================================================
    // WeakTracked Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_weak_tracked_upgrade_success() {
        let arc = TrackedArc::new(42i32);
        let weak = arc.downgrade();

        let upgraded = weak.upgrade();
        kani::assert(upgraded.is_some(), "upgrade succeeds when strong exists");
        kani::assert(*upgraded.unwrap() == 42, "upgraded value is correct");
    }

    #[kani::proof]
    fn proof_weak_tracked_upgrade_fail() {
        let weak = {
            let arc = TrackedArc::new(42i32);
            arc.downgrade()
        };

        let upgraded = weak.upgrade();
        kani::assert(upgraded.is_none(), "upgrade fails when no strong refs");
    }

    #[kani::proof]
    fn proof_weak_tracked_clone() {
        let arc = TrackedArc::new(42i32);
        let weak1 = arc.downgrade();
        let weak2 = weak1.clone();

        kani::assert(weak1.strong_count() == 1, "weak1 sees strong count");
        kani::assert(weak2.strong_count() == 1, "weak2 sees strong count");
        kani::assert(arc.weak_count() == 2, "arc has 2 weak refs");
    }

    // ========================================================================
    // ArcSwap Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_arc_swap_new_load() {
        let swap = ArcSwap::new(Arc::new(42i32));
        let loaded = swap.load();

        kani::assert(*loaded == 42, "load returns initial value");
    }

    #[kani::proof]
    fn proof_arc_swap_store() {
        let swap = ArcSwap::new(Arc::new(42i32));
        swap.store(Arc::new(100));
        let loaded = swap.load();

        kani::assert(*loaded == 100, "store updates value");
    }

    #[kani::proof]
    fn proof_arc_swap_swap() {
        let swap = ArcSwap::new(Arc::new(42i32));
        let old = swap.swap(Arc::new(100));

        kani::assert(*old == 42, "swap returns old value");
        kani::assert(*swap.load() == 100, "swap stores new value");
    }

    #[kani::proof]
    fn proof_arc_swap_update() {
        let swap = ArcSwap::new(Arc::new(10i32));
        swap.update(|v| v + 5);

        kani::assert(*swap.load() == 15, "update applies function");
    }

    // ========================================================================
    // ArcList Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_arc_list_new_empty() {
        let list: ArcList<i32> = ArcList::new();

        kani::assert(list.is_empty(), "new list is empty");
        kani::assert(list.len() == 0, "new list has len 0");
    }

    #[kani::proof]
    fn proof_arc_list_default_empty() {
        let list: ArcList<i32> = ArcList::default();

        kani::assert(list.is_empty(), "default list is empty");
    }

    #[kani::proof]
    fn proof_arc_list_push() {
        let mut list: ArcList<i32> = ArcList::new();
        list.push(Arc::new(42));

        kani::assert(!list.is_empty(), "list not empty after push");
        kani::assert(list.len() == 1, "len is 1 after push");
    }

    #[kani::proof]
    fn proof_arc_list_get() {
        let mut list: ArcList<i32> = ArcList::new();
        list.push(Arc::new(10));
        list.push(Arc::new(20));
        list.push(Arc::new(30));

        kani::assert(*list.get(0).unwrap() == 10, "get(0) returns first");
        kani::assert(*list.get(1).unwrap() == 20, "get(1) returns second");
        kani::assert(*list.get(2).unwrap() == 30, "get(2) returns third");
        kani::assert(list.get(3).is_none(), "get(3) returns None");
    }

    #[kani::proof]
    fn proof_arc_list_pop() {
        let mut list: ArcList<i32> = ArcList::new();
        list.push(Arc::new(1));
        list.push(Arc::new(2));

        let popped = list.pop();
        kani::assert(popped.is_some(), "pop returns Some");
        kani::assert(*popped.unwrap() == 2, "pop returns last");
        kani::assert(list.len() == 1, "len is 1 after pop");
    }

    #[kani::proof]
    fn proof_arc_list_pop_empty() {
        let mut list: ArcList<i32> = ArcList::new();
        let popped = list.pop();

        kani::assert(popped.is_none(), "pop on empty returns None");
    }

    #[kani::proof]
    fn proof_arc_list_clear() {
        let mut list: ArcList<i32> = ArcList::new();
        list.push(Arc::new(1));
        list.push(Arc::new(2));
        list.clear();

        kani::assert(list.is_empty(), "list empty after clear");
        kani::assert(list.len() == 0, "len is 0 after clear");
    }

    #[kani::proof]
    fn proof_arc_list_clone() {
        let mut list: ArcList<i32> = ArcList::new();
        list.push(Arc::new(42));

        let list2 = list.clone();
        kani::assert(list2.len() == 1, "cloned list has same len");
        kani::assert(*list2.get(0).unwrap() == 42, "cloned list has same value");
    }

    // ========================================================================
    // Helper Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_arc_from_box() {
        let boxed = Box::new(42i32);
        let arc = arc_from_box(boxed);

        kani::assert(*arc == 42, "arc_from_box preserves value");
    }

    #[kani::proof]
    fn proof_arc_clone_inner() {
        let arc = Arc::new(42i32);
        let cloned = arc_clone_inner(&arc);

        kani::assert(cloned == 42, "arc_clone_inner returns value");
    }

    #[kani::proof]
    fn proof_arc_ptr_eq_same() {
        let arc1 = Arc::new(42i32);
        let arc2 = arc1.clone();

        kani::assert(arc_ptr_eq(&arc1, &arc2), "clones point to same allocation");
    }

    #[kani::proof]
    fn proof_arc_ptr_eq_different() {
        let arc1 = Arc::new(42i32);
        let arc2 = Arc::new(42i32);

        kani::assert(!arc_ptr_eq(&arc1, &arc2), "different allocations not equal");
    }
}
