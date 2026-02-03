//! Unit type utilities for drbot.
//!
//! This crate provides:
//! - Unit type wrappers
//! - Phantom data utilities
//! - Marker types

use std::marker::PhantomData;
use thiserror::Error;

/// Unit error types.
#[derive(Error, Debug, Clone)]
pub enum UnitError {
    #[error("Unit operation failed")]
    Failed,
}

/// Result type for unit operations.
pub type Result<T> = std::result::Result<T, UnitError>;

/// A unit type with a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NamedUnit {
    name: &'static str,
}

impl NamedUnit {
    /// Create new named unit.
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }

    /// Get the name.
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

impl Default for NamedUnit {
    fn default() -> Self {
        Self::new("unit")
    }
}

/// A phantom marker type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Phantom<T>(PhantomData<T>);

impl<T> Phantom<T> {
    /// Create new phantom.
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

/// A phantom marker with a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tagged<T, Tag> {
    value: T,
    _tag: PhantomData<Tag>,
}

impl<T, Tag> Tagged<T, Tag> {
    /// Create new tagged value.
    pub const fn new(value: T) -> Self {
        Self {
            value,
            _tag: PhantomData,
        }
    }

    /// Get inner value.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Get reference to inner value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get mutable reference to inner value.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Map inner value.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Tagged<U, Tag> {
        Tagged::new(f(self.value))
    }

    /// Change tag type.
    pub fn retag<NewTag>(self) -> Tagged<T, NewTag> {
        Tagged::new(self.value)
    }
}

impl<T: Default, Tag> Default for Tagged<T, Tag> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Marker for "nothing" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Nothing;

/// Marker for "something" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Something;

/// Marker for "pending" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Pending;

/// Marker for "completed" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Completed;

/// Marker for "failed" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Failed;

/// Marker for "initialized" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Initialized;

/// Marker for "uninitialized" state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Uninitialized;

/// A type-level boolean.
pub trait TypeBool {
    /// The boolean value.
    const VALUE: bool;
}

/// Type-level true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct True;

impl TypeBool for True {
    const VALUE: bool = true;
}

/// Type-level false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct False;

impl TypeBool for False {
    const VALUE: bool = false;
}

/// A void type (no values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Void {}

impl Void {
    /// Absurd function - can prove anything from Void.
    pub fn absurd<T>(self) -> T {
        match self {}
    }
}

/// An infallible conversion marker.
pub struct Infallible(Void);

impl Infallible {
    /// Convert to any type.
    pub fn into<T>(self) -> T {
        self.0.absurd()
    }
}

/// Zero-sized type with alignment.
#[repr(C, align(1))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Align1;

/// Zero-sized type with 2-byte alignment.
#[repr(C, align(2))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Align2;

/// Zero-sized type with 4-byte alignment.
#[repr(C, align(4))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Align4;

/// Zero-sized type with 8-byte alignment.
#[repr(C, align(8))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Align8;

/// A wrapper that ignores its content for comparison.
#[derive(Debug, Clone, Default)]
pub struct Ignore<T>(pub T);

impl<T> PartialEq for Ignore<T> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl<T> Eq for Ignore<T> {}

impl<T> std::hash::Hash for Ignore<T> {
    fn hash<H: std::hash::Hasher>(&self, _state: &mut H) {
        // Hash nothing
    }
}

impl<T> Ignore<T> {
    /// Create new ignore wrapper.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Get inner value.
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Get reference.
    pub fn get(&self) -> &T {
        &self.0
    }

    /// Get mutable reference.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

/// Type-level natural number zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Zero;

/// Type-level successor (adds one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Succ<N>(PhantomData<N>);

/// Type alias for one.
pub type One = Succ<Zero>;
/// Type alias for two.
pub type Two = Succ<One>;
/// Type alias for three.
pub type Three = Succ<Two>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_named_unit() {
        let unit = NamedUnit::new("my_unit");
        assert_eq!(unit.name(), "my_unit");
    }

    #[test]
    fn test_tagged() {
        struct Meters;
        struct Feet;

        let distance: Tagged<f64, Meters> = Tagged::new(100.0);
        assert_eq!(*distance.get(), 100.0);

        let converted: Tagged<f64, Feet> = distance.map(|m| m * 3.28084).retag();
        assert!(*converted.get() > 300.0);
    }

    #[test]
    fn test_type_bool() {
        assert!(True::VALUE);
        assert!(!False::VALUE);
    }

    #[test]
    fn test_ignore() {
        let a = Ignore::new(42);
        let b = Ignore::new(100);
        assert_eq!(a, b); // Always equal
    }

    #[test]
    fn test_phantom() {
        let _p: Phantom<String> = Phantom::new();
        assert_eq!(std::mem::size_of::<Phantom<String>>(), 0);
    }

    #[test]
    fn test_alignments() {
        assert_eq!(std::mem::align_of::<Align1>(), 1);
        assert_eq!(std::mem::align_of::<Align2>(), 2);
        assert_eq!(std::mem::align_of::<Align4>(), 4);
        assert_eq!(std::mem::align_of::<Align8>(), 8);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // NamedUnit Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_named_unit_new() {
        let unit = NamedUnit::new("test");
        kani::assert(unit.name() == "test", "name returns correct value");
    }

    #[kani::proof]
    fn proof_named_unit_default() {
        let unit = NamedUnit::default();
        kani::assert(unit.name() == "unit", "default name is 'unit'");
    }

    #[kani::proof]
    fn proof_named_unit_equality() {
        let u1 = NamedUnit::new("test");
        let u2 = NamedUnit::new("test");
        let u3 = NamedUnit::new("other");

        kani::assert(u1 == u2, "same name equal");
        kani::assert(u1 != u3, "different name not equal");
    }

    // ========================================================================
    // Phantom Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_phantom_new() {
        let _p: Phantom<i32> = Phantom::new();
        // Just verify it creates without panic
    }

    #[kani::proof]
    fn proof_phantom_equality() {
        let p1: Phantom<i32> = Phantom::new();
        let p2: Phantom<i32> = Phantom::new();

        kani::assert(p1 == p2, "Phantoms of same type are equal");
    }

    #[kani::proof]
    fn proof_phantom_default() {
        let p: Phantom<i32> = Phantom::default();
        let p2: Phantom<i32> = Phantom::new();

        kani::assert(p == p2, "default equals new");
    }

    // ========================================================================
    // Tagged Proofs
    // ========================================================================

    struct Tag1;
    struct Tag2;

    #[kani::proof]
    fn proof_tagged_new() {
        let value: i8 = kani::any();
        let tagged: Tagged<i8, Tag1> = Tagged::new(value);

        kani::assert(*tagged.get() == value, "get returns value");
    }

    #[kani::proof]
    fn proof_tagged_into_inner() {
        let value: i8 = kani::any();
        let tagged: Tagged<i8, Tag1> = Tagged::new(value);

        kani::assert(tagged.into_inner() == value, "into_inner returns value");
    }

    #[kani::proof]
    fn proof_tagged_get_mut() {
        let value: i8 = kani::any();
        let mut tagged: Tagged<i8, Tag1> = Tagged::new(value);

        *tagged.get_mut() = 42;
        kani::assert(*tagged.get() == 42, "get_mut allows modification");
    }

    #[kani::proof]
    fn proof_tagged_map() {
        let value: i8 = kani::any();
        kani::assume(value < 100);

        let tagged: Tagged<i8, Tag1> = Tagged::new(value);
        let mapped = tagged.map(|v| v + 1);

        kani::assert(*mapped.get() == value + 1, "map transforms value");
    }

    #[kani::proof]
    fn proof_tagged_retag() {
        let value: i8 = kani::any();
        let tagged: Tagged<i8, Tag1> = Tagged::new(value);
        let retagged: Tagged<i8, Tag2> = tagged.retag();

        kani::assert(*retagged.get() == value, "retag preserves value");
    }

    #[kani::proof]
    fn proof_tagged_default() {
        let tagged: Tagged<i8, Tag1> = Tagged::default();
        kani::assert(*tagged.get() == 0, "default uses T::default()");
    }

    #[kani::proof]
    fn proof_tagged_equality() {
        let v1: i8 = kani::any();
        let v2: i8 = kani::any();

        let t1: Tagged<i8, Tag1> = Tagged::new(v1);
        let t2: Tagged<i8, Tag1> = Tagged::new(v2);

        if v1 == v2 {
            kani::assert(t1 == t2, "same value same tag equal");
        } else {
            kani::assert(t1 != t2, "different value not equal");
        }
    }

    // ========================================================================
    // Marker Types Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_nothing_default() {
        let _n: Nothing = Nothing::default();
    }

    #[kani::proof]
    fn proof_something_default() {
        let _s: Something = Something::default();
    }

    #[kani::proof]
    fn proof_pending_default() {
        let _p: Pending = Pending::default();
    }

    #[kani::proof]
    fn proof_completed_default() {
        let _c: Completed = Completed::default();
    }

    #[kani::proof]
    fn proof_failed_default() {
        let _f: Failed = Failed::default();
    }

    #[kani::proof]
    fn proof_initialized_default() {
        let _i: Initialized = Initialized::default();
    }

    #[kani::proof]
    fn proof_uninitialized_default() {
        let _u: Uninitialized = Uninitialized::default();
    }

    // ========================================================================
    // TypeBool Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_type_bool_true() {
        kani::assert(True::VALUE, "True::VALUE is true");
    }

    #[kani::proof]
    fn proof_type_bool_false() {
        kani::assert(!False::VALUE, "False::VALUE is false");
    }

    #[kani::proof]
    fn proof_true_default() {
        let _t: True = True::default();
    }

    #[kani::proof]
    fn proof_false_default() {
        let _f: False = False::default();
    }

    // ========================================================================
    // Ignore Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_ignore_always_equal() {
        let a: i8 = kani::any();
        let b: i8 = kani::any();

        let i1 = Ignore::new(a);
        let i2 = Ignore::new(b);

        kani::assert(i1 == i2, "Ignore always equals");
    }

    #[kani::proof]
    fn proof_ignore_into_inner() {
        let value: i8 = kani::any();
        let ignore = Ignore::new(value);

        kani::assert(ignore.into_inner() == value, "into_inner returns value");
    }

    #[kani::proof]
    fn proof_ignore_get() {
        let value: i8 = kani::any();
        let ignore = Ignore::new(value);

        kani::assert(*ignore.get() == value, "get returns reference");
    }

    #[kani::proof]
    fn proof_ignore_get_mut() {
        let value: i8 = kani::any();
        let mut ignore = Ignore::new(value);

        *ignore.get_mut() = 42;
        kani::assert(*ignore.get() == 42, "get_mut allows modification");
    }

    #[kani::proof]
    fn proof_ignore_default() {
        let ignore: Ignore<i32> = Ignore::default();
        kani::assert(*ignore.get() == 0, "default uses T::default()");
    }

    // ========================================================================
    // Alignment Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_align1_size_zero() {
        kani::assert(std::mem::size_of::<Align1>() == 0, "Align1 is ZST");
    }

    #[kani::proof]
    fn proof_align2_size_zero() {
        kani::assert(std::mem::size_of::<Align2>() == 0, "Align2 is ZST");
    }

    #[kani::proof]
    fn proof_align4_size_zero() {
        kani::assert(std::mem::size_of::<Align4>() == 0, "Align4 is ZST");
    }

    #[kani::proof]
    fn proof_align8_size_zero() {
        kani::assert(std::mem::size_of::<Align8>() == 0, "Align8 is ZST");
    }

    // ========================================================================
    // Zero/Succ Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_zero_default() {
        let _z: Zero = Zero::default();
    }

    #[kani::proof]
    fn proof_succ_default() {
        let _s: Succ<Zero> = Succ::default();
    }

    #[kani::proof]
    fn proof_type_aliases() {
        let _one: One = Succ::default();
        let _two: Two = Succ::default();
        let _three: Three = Succ::default();
    }
}
