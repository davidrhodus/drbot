//! Box type extensions for drbot.
//!
//! This crate provides:
//! - Box extensions
//! - Box utilities
//! - Heap allocation helpers

use thiserror::Error;

/// Box error types.
#[derive(Error, Debug, Clone)]
pub enum BoxError {
    #[error("Allocation failed")]
    AllocationFailed,
}

/// Result type for box operations.
pub type Result<T> = std::result::Result<T, BoxError>;

/// Box extension trait.
pub trait BoxExt<T: ?Sized> {
    /// Into raw pointer.
    fn into_raw_ptr(self) -> *mut T;
}

impl<T: ?Sized> BoxExt<T> for Box<T> {
    fn into_raw_ptr(self) -> *mut T {
        Box::into_raw(self)
    }
}

/// Create boxed value.
pub fn boxed<T>(value: T) -> Box<T> {
    Box::new(value)
}

/// Box from raw pointer.
pub unsafe fn from_raw<T>(ptr: *mut T) -> Box<T> {
    Box::from_raw(ptr)
}

/// Leak box.
pub fn leak<T>(b: Box<T>) -> &'static mut T {
    Box::leak(b)
}

/// Try box (returns None on allocation failure in no_std).
pub fn try_box<T>(value: T) -> Option<Box<T>> {
    Some(Box::new(value))
}

/// Boxed trait object helper.
pub fn box_trait<T: ?Sized>(value: Box<T>) -> Box<T> {
    value
}

/// Clone boxed value.
pub fn clone_box<T: Clone>(b: &Box<T>) -> Box<T> {
    b.clone()
}

/// Map boxed value.
pub fn map_box<T, U, F: FnOnce(T) -> U>(b: Box<T>, f: F) -> Box<U> {
    Box::new(f(*b))
}

/// Flatten nested boxes.
pub fn flatten_box<T>(b: Box<Box<T>>) -> Box<T> {
    *b
}

/// Sized box wrapper.
#[derive(Debug)]
pub struct SizedBox<T> {
    inner: Box<T>,
    size: usize,
}

impl<T> SizedBox<T> {
    /// Create new.
    pub fn new(value: T) -> Self {
        Self {
            inner: Box::new(value),
            size: std::mem::size_of::<T>(),
        }
    }

    /// Get size.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.inner
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Into inner.
    pub fn into_inner(self) -> T {
        *self.inner
    }
}

impl<T> std::ops::Deref for SizedBox<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> std::ops::DerefMut for SizedBox<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Box with metadata.
#[derive(Debug)]
pub struct MetaBox<T, M> {
    value: Box<T>,
    metadata: M,
}

impl<T, M> MetaBox<T, M> {
    /// Create new.
    pub fn new(value: T, metadata: M) -> Self {
        Self {
            value: Box::new(value),
            metadata,
        }
    }

    /// Get value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable value.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Get metadata.
    pub fn metadata(&self) -> &M {
        &self.metadata
    }

    /// Get mutable metadata.
    pub fn metadata_mut(&mut self) -> &mut M {
        &mut self.metadata
    }

    /// Into parts.
    pub fn into_parts(self) -> (T, M) {
        (*self.value, self.metadata)
    }
}

impl<T, M> std::ops::Deref for MetaBox<T, M> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T, M> std::ops::DerefMut for MetaBox<T, M> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Lazy boxed value.
pub struct LazyBox<T, F = fn() -> T> {
    value: Option<Box<T>>,
    init: Option<F>,
}

impl<T, F: FnOnce() -> T> LazyBox<T, F> {
    /// Create new.
    pub fn new(init: F) -> Self {
        Self {
            value: None,
            init: Some(init),
        }
    }

    /// Get reference.
    pub fn get(&mut self) -> &T {
        if self.value.is_none() {
            let init = self.init.take().expect("Already initialized");
            self.value = Some(Box::new(init()));
        }
        self.value.as_ref().unwrap()
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        if self.value.is_none() {
            let init = self.init.take().expect("Already initialized");
            self.value = Some(Box::new(init()));
        }
        self.value.as_mut().unwrap()
    }

    /// Is initialized.
    pub fn is_initialized(&self) -> bool {
        self.value.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boxed() {
        let b = boxed(42);
        assert_eq!(*b, 42);
    }

    #[test]
    fn test_map_box() {
        let b = boxed(21);
        let b2 = map_box(b, |x| x * 2);
        assert_eq!(*b2, 42);
    }

    #[test]
    fn test_sized_box() {
        let sb = SizedBox::new(42i32);
        assert_eq!(sb.size(), 4);
        assert_eq!(*sb.get(), 42);
    }

    #[test]
    fn test_meta_box() {
        let mb = MetaBox::new(42, "metadata");
        assert_eq!(*mb.get(), 42);
        assert_eq!(*mb.metadata(), "metadata");
    }

    #[test]
    fn test_lazy_box() {
        let mut lb = LazyBox::new(|| "computed".to_string());
        assert!(!lb.is_initialized());
        assert_eq!(lb.get(), "computed");
        assert!(lb.is_initialized());
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // boxed() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_boxed_stores_value() {
        let value: u8 = kani::any();
        let b = boxed(value);
        kani::assert(*b == value, "boxed must store value");
    }

    // ========================================================================
    // try_box() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_try_box_succeeds() {
        let value: u8 = kani::any();
        let result = try_box(value);
        kani::assert(result.is_some(), "try_box must succeed");
        kani::assert(*result.unwrap() == value, "try_box must store value");
    }

    // ========================================================================
    // clone_box() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_clone_box_clones_value() {
        let value: u8 = kani::any();
        let b = boxed(value);
        let cloned = clone_box(&b);
        kani::assert(*cloned == value, "clone_box must clone value");
        kani::assert(*b == *cloned, "original and clone must be equal");
    }

    // ========================================================================
    // map_box() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_map_box_applies_function() {
        let value: u8 = kani::any();
        let b = boxed(value);
        let mapped = map_box(b, |x| x.wrapping_add(1));
        kani::assert(
            *mapped == value.wrapping_add(1),
            "map_box must apply function",
        );
    }

    #[kani::proof]
    fn proof_map_box_identity() {
        let value: u8 = kani::any();
        let b = boxed(value);
        let mapped = map_box(b, |x| x);
        kani::assert(*mapped == value, "map_box identity must preserve value");
    }

    // ========================================================================
    // flatten_box() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_flatten_box() {
        let value: u8 = kani::any();
        let nested = boxed(boxed(value));
        let flat = flatten_box(nested);
        kani::assert(*flat == value, "flatten_box must unwrap nested box");
    }

    // ========================================================================
    // SizedBox Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_sized_box_new() {
        let value: u8 = kani::any();
        let sb = SizedBox::new(value);
        kani::assert(*sb.get() == value, "SizedBox::new must store value");
    }

    #[kani::proof]
    fn proof_sized_box_size_u8() {
        let value: u8 = kani::any();
        let sb = SizedBox::new(value);
        kani::assert(sb.size() == 1, "SizedBox size of u8 must be 1");
    }

    #[kani::proof]
    fn proof_sized_box_size_u32() {
        let value: u32 = kani::any();
        let sb = SizedBox::new(value);
        kani::assert(sb.size() == 4, "SizedBox size of u32 must be 4");
    }

    #[kani::proof]
    fn proof_sized_box_size_u64() {
        let value: u64 = kani::any();
        let sb = SizedBox::new(value);
        kani::assert(sb.size() == 8, "SizedBox size of u64 must be 8");
    }

    #[kani::proof]
    fn proof_sized_box_get() {
        let value: u8 = kani::any();
        let sb = SizedBox::new(value);
        kani::assert(*sb.get() == value, "get must return stored value");
    }

    #[kani::proof]
    fn proof_sized_box_get_mut() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut sb = SizedBox::new(value);
        *sb.get_mut() = new_value;
        kani::assert(*sb.get() == new_value, "get_mut must allow modification");
    }

    #[kani::proof]
    fn proof_sized_box_into_inner() {
        let value: u8 = kani::any();
        let sb = SizedBox::new(value);
        let inner = sb.into_inner();
        kani::assert(inner == value, "into_inner must return original value");
    }

    #[kani::proof]
    fn proof_sized_box_deref() {
        let value: u8 = kani::any();
        let sb = SizedBox::new(value);
        kani::assert(*sb == value, "deref must return value");
    }

    #[kani::proof]
    fn proof_sized_box_deref_mut() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut sb = SizedBox::new(value);
        *sb = new_value;
        kani::assert(*sb == new_value, "deref_mut must allow modification");
    }

    // ========================================================================
    // MetaBox Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_meta_box_new() {
        let value: u8 = kani::any();
        let meta: u8 = kani::any();
        let mb = MetaBox::new(value, meta);
        kani::assert(*mb.get() == value, "MetaBox::new must store value");
        kani::assert(*mb.metadata() == meta, "MetaBox::new must store metadata");
    }

    #[kani::proof]
    fn proof_meta_box_get() {
        let value: u8 = kani::any();
        let meta: u8 = kani::any();
        let mb = MetaBox::new(value, meta);
        kani::assert(*mb.get() == value, "get must return value");
    }

    #[kani::proof]
    fn proof_meta_box_get_mut() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let meta: u8 = kani::any();
        let mut mb = MetaBox::new(value, meta);
        *mb.get_mut() = new_value;
        kani::assert(*mb.get() == new_value, "get_mut must allow modification");
    }

    #[kani::proof]
    fn proof_meta_box_metadata() {
        let value: u8 = kani::any();
        let meta: u8 = kani::any();
        let mb = MetaBox::new(value, meta);
        kani::assert(*mb.metadata() == meta, "metadata must return metadata");
    }

    #[kani::proof]
    fn proof_meta_box_metadata_mut() {
        let value: u8 = kani::any();
        let meta: u8 = kani::any();
        let new_meta: u8 = kani::any();
        let mut mb = MetaBox::new(value, meta);
        *mb.metadata_mut() = new_meta;
        kani::assert(
            *mb.metadata() == new_meta,
            "metadata_mut must allow modification",
        );
    }

    #[kani::proof]
    fn proof_meta_box_into_parts() {
        let value: u8 = kani::any();
        let meta: u8 = kani::any();
        let mb = MetaBox::new(value, meta);
        let (v, m) = mb.into_parts();
        kani::assert(v == value, "into_parts must return value");
        kani::assert(m == meta, "into_parts must return metadata");
    }

    #[kani::proof]
    fn proof_meta_box_deref() {
        let value: u8 = kani::any();
        let meta: u8 = kani::any();
        let mb = MetaBox::new(value, meta);
        kani::assert(*mb == value, "deref must return value");
    }

    #[kani::proof]
    fn proof_meta_box_deref_mut() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let meta: u8 = kani::any();
        let mut mb = MetaBox::new(value, meta);
        *mb = new_value;
        kani::assert(*mb == new_value, "deref_mut must allow modification");
    }

    // ========================================================================
    // LazyBox Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_lazy_box_not_initialized_initially() {
        let value: u8 = kani::any();
        let lb = LazyBox::new(move || value);
        kani::assert(
            !lb.is_initialized(),
            "LazyBox must not be initialized initially",
        );
    }

    #[kani::proof]
    fn proof_lazy_box_initialized_after_get() {
        let value: u8 = kani::any();
        let mut lb = LazyBox::new(move || value);
        let _ = lb.get();
        kani::assert(lb.is_initialized(), "LazyBox must be initialized after get");
    }

    #[kani::proof]
    fn proof_lazy_box_get_returns_value() {
        let value: u8 = kani::any();
        let mut lb = LazyBox::new(move || value);
        kani::assert(*lb.get() == value, "get must return computed value");
    }

    #[kani::proof]
    fn proof_lazy_box_get_mut_returns_value() {
        let value: u8 = kani::any();
        let mut lb = LazyBox::new(move || value);
        kani::assert(*lb.get_mut() == value, "get_mut must return computed value");
    }

    #[kani::proof]
    fn proof_lazy_box_get_mut_modifiable() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut lb = LazyBox::new(move || value);
        *lb.get_mut() = new_value;
        kani::assert(*lb.get() == new_value, "get_mut must allow modification");
    }

    #[kani::proof]
    fn proof_lazy_box_initialized_after_get_mut() {
        let value: u8 = kani::any();
        let mut lb = LazyBox::new(move || value);
        let _ = lb.get_mut();
        kani::assert(
            lb.is_initialized(),
            "LazyBox must be initialized after get_mut",
        );
    }
}
