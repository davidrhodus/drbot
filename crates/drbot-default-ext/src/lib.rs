//! Default trait extensions for drbot.
//!
//! This crate provides:
//! - Default extensions
//! - Conditional defaults
//! - Custom default values

use thiserror::Error;

/// Default extension error types.
#[derive(Error, Debug, Clone)]
pub enum DefaultExtError {
    #[error("No default available")]
    NoDefault,
}

/// Result type for default operations.
pub type Result<T> = std::result::Result<T, DefaultExtError>;

/// Default extension trait.
pub trait DefaultExt: Default {
    /// Get default if condition is true.
    fn default_if(condition: bool) -> Option<Self> {
        if condition {
            Some(Self::default())
        } else {
            None
        }
    }

    /// Default or provided value.
    fn default_or(value: Self) -> Self {
        value
    }

    /// Default or computed value.
    fn default_or_else<F: FnOnce() -> Self>(f: F) -> Self {
        f()
    }

    /// Check if value is default.
    fn is_default(&self) -> bool
    where
        Self: PartialEq,
    {
        *self == Self::default()
    }

    /// Replace with default, returning old value.
    fn take_default(&mut self) -> Self {
        std::mem::take(self)
    }
}

impl<T: Default> DefaultExt for T {}

/// Or default trait.
pub trait OrDefault<T> {
    /// Get value or default.
    fn or_default(self) -> T
    where
        T: Default;
}

impl<T> OrDefault<T> for Option<T> {
    fn or_default(self) -> T
    where
        T: Default,
    {
        self.unwrap_or_default()
    }
}

/// With default builder.
pub struct WithDefault<T> {
    value: Option<T>,
}

impl<T: Default> WithDefault<T> {
    /// Create new.
    pub fn new() -> Self {
        Self { value: None }
    }

    /// Set value.
    pub fn set(mut self, value: T) -> Self {
        self.value = Some(value);
        self
    }

    /// Set if condition.
    pub fn set_if(mut self, condition: bool, value: T) -> Self {
        if condition {
            self.value = Some(value);
        }
        self
    }

    /// Build, using default if not set.
    pub fn build(self) -> T {
        self.value.unwrap_or_default()
    }

    /// Build with custom default.
    pub fn build_or(self, default: T) -> T {
        self.value.unwrap_or(default)
    }
}

impl<T: Default> Default for WithDefault<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Default registry.
#[derive(Debug)]
pub struct DefaultRegistry<K, V> {
    defaults: std::collections::HashMap<K, V>,
}

impl<K: std::hash::Hash + Eq, V: Clone> DefaultRegistry<K, V> {
    /// Create new.
    pub fn new() -> Self {
        Self {
            defaults: std::collections::HashMap::new(),
        }
    }

    /// Register default.
    pub fn register(&mut self, key: K, value: V) {
        self.defaults.insert(key, value);
    }

    /// Get default.
    pub fn get(&self, key: &K) -> Option<V> {
        self.defaults.get(key).cloned()
    }

    /// Get default or value.
    pub fn get_or(&self, key: &K, fallback: V) -> V {
        self.defaults.get(key).cloned().unwrap_or(fallback)
    }

    /// Has default.
    pub fn has(&self, key: &K) -> bool {
        self.defaults.contains_key(key)
    }
}

impl<K: std::hash::Hash + Eq, V: Clone> Default for DefaultRegistry<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

/// Defaultable wrapper.
#[derive(Debug, Clone)]
pub struct Defaultable<T> {
    value: Option<T>,
    default: T,
}

impl<T: Clone> Defaultable<T> {
    /// Create with default.
    pub fn new(default: T) -> Self {
        Self {
            value: None,
            default,
        }
    }

    /// Set value.
    pub fn set(&mut self, value: T) {
        self.value = Some(value);
    }

    /// Clear to default.
    pub fn clear(&mut self) {
        self.value = None;
    }

    /// Get value or default.
    pub fn get(&self) -> &T {
        self.value.as_ref().unwrap_or(&self.default)
    }

    /// Is using default.
    pub fn is_default(&self) -> bool {
        self.value.is_none()
    }

    /// Get the default.
    pub fn default_value(&self) -> &T {
        &self.default
    }
}

/// Lazy default.
pub struct LazyDefault<T, F: FnOnce() -> T> {
    value: Option<T>,
    initializer: Option<F>,
}

impl<T, F: FnOnce() -> T> LazyDefault<T, F> {
    /// Create with initializer.
    pub fn new(initializer: F) -> Self {
        Self {
            value: None,
            initializer: Some(initializer),
        }
    }

    /// Get value, initializing if needed.
    pub fn get(&mut self) -> &T {
        if self.value.is_none() {
            let init = self.initializer.take().expect("Already initialized");
            self.value = Some(init());
        }
        self.value.as_ref().unwrap()
    }

    /// Get mutable.
    pub fn get_mut(&mut self) -> &mut T {
        if self.value.is_none() {
            let init = self.initializer.take().expect("Already initialized");
            self.value = Some(init());
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
    fn test_default_ext() {
        assert_eq!(i32::default_if(true), Some(0));
        assert_eq!(i32::default_if(false), None);
    }

    #[test]
    fn test_is_default() {
        assert!(0i32.is_default());
        assert!(!42i32.is_default());
    }

    #[test]
    fn test_with_default() {
        let v: i32 = WithDefault::new().set_if(false, 42).build();
        assert_eq!(v, 0);

        let v: i32 = WithDefault::new().set_if(true, 42).build();
        assert_eq!(v, 42);
    }

    #[test]
    fn test_defaultable() {
        let mut d = Defaultable::new(10);
        assert_eq!(*d.get(), 10);
        assert!(d.is_default());

        d.set(20);
        assert_eq!(*d.get(), 20);
        assert!(!d.is_default());
    }

    #[test]
    fn test_lazy_default() {
        let mut lazy = LazyDefault::new(|| "initialized".to_string());
        assert!(!lazy.is_initialized());
        assert_eq!(lazy.get(), "initialized");
        assert!(lazy.is_initialized());
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // DefaultExt Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_default_if_true() {
        let result: Option<i32> = i32::default_if(true);
        kani::assert(result.is_some(), "default_if(true) returns Some");
        kani::assert(result == Some(0), "default_if(true) returns default value");
    }

    #[kani::proof]
    fn proof_default_if_false() {
        let result: Option<i32> = i32::default_if(false);
        kani::assert(result.is_none(), "default_if(false) returns None");
    }

    #[kani::proof]
    fn proof_default_or_returns_value() {
        let value: i8 = kani::any();
        let result = i8::default_or(value);
        kani::assert(result == value, "default_or returns provided value");
    }

    #[kani::proof]
    fn proof_is_default_zero() {
        let x: i32 = 0;
        kani::assert(x.is_default(), "0 is default for i32");
    }

    #[kani::proof]
    fn proof_is_default_nonzero() {
        let x: i8 = kani::any();
        kani::assume(x != 0);
        kani::assert(!x.is_default(), "nonzero is not default");
    }

    #[kani::proof]
    fn proof_take_default_returns_old() {
        let original: i8 = kani::any();
        let mut x = original;
        let old = x.take_default();

        kani::assert(old == original, "take_default returns old value");
        kani::assert(x == i8::default(), "take_default leaves default");
    }

    // ========================================================================
    // OrDefault Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_or_default_some() {
        let value: i8 = kani::any();
        let opt = Some(value);
        let result = opt.or_default();
        kani::assert(result == value, "Some.or_default returns inner value");
    }

    #[kani::proof]
    fn proof_or_default_none() {
        let opt: Option<i32> = None;
        let result = opt.or_default();
        kani::assert(result == 0, "None.or_default returns default");
    }

    // ========================================================================
    // WithDefault Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_with_default_new_builds_default() {
        let builder: WithDefault<i32> = WithDefault::new();
        let result = builder.build();
        kani::assert(result == 0, "new builder builds to default");
    }

    #[kani::proof]
    fn proof_with_default_default_builds_default() {
        let builder: WithDefault<i32> = WithDefault::default();
        let result = builder.build();
        kani::assert(result == 0, "default builder builds to default");
    }

    #[kani::proof]
    fn proof_with_default_set_value() {
        let value: i8 = kani::any();
        let result = WithDefault::new().set(value).build();
        kani::assert(result == value, "set value is returned");
    }

    #[kani::proof]
    fn proof_with_default_set_if_true() {
        let value: i8 = kani::any();
        kani::assume(value != 0);

        let result = WithDefault::new().set_if(true, value).build();
        kani::assert(result == value, "set_if(true) sets value");
    }

    #[kani::proof]
    fn proof_with_default_set_if_false() {
        let value: i8 = kani::any();
        kani::assume(value != 0);

        let result: i8 = WithDefault::new().set_if(false, value).build();
        kani::assert(result == 0, "set_if(false) keeps default");
    }

    #[kani::proof]
    fn proof_with_default_build_or() {
        let fallback: i8 = kani::any();
        let result = WithDefault::<i8>::new().build_or(fallback);
        kani::assert(result == fallback, "build_or uses custom fallback");
    }

    #[kani::proof]
    fn proof_with_default_set_overrides_build_or() {
        let value: i8 = kani::any();
        let fallback: i8 = kani::any();

        let result = WithDefault::new().set(value).build_or(fallback);
        kani::assert(result == value, "set value overrides build_or fallback");
    }

    // ========================================================================
    // DefaultRegistry Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_default_registry_new_empty() {
        let registry: DefaultRegistry<i32, i32> = DefaultRegistry::new();
        let key: i8 = kani::any();
        kani::assert(
            registry.get(&(key as i32)).is_none(),
            "new registry is empty",
        );
    }

    #[kani::proof]
    fn proof_default_registry_default_empty() {
        let registry: DefaultRegistry<i32, i32> = DefaultRegistry::default();
        let key: i8 = kani::any();
        kani::assert(
            registry.get(&(key as i32)).is_none(),
            "default registry is empty",
        );
    }

    #[kani::proof]
    fn proof_default_registry_register_get() {
        let mut registry: DefaultRegistry<i32, i32> = DefaultRegistry::new();
        let key: i8 = kani::any();
        let value: i8 = kani::any();

        registry.register(key as i32, value as i32);
        let result = registry.get(&(key as i32));

        kani::assert(
            result == Some(value as i32),
            "registered value can be retrieved",
        );
    }

    #[kani::proof]
    fn proof_default_registry_has() {
        let mut registry: DefaultRegistry<i32, i32> = DefaultRegistry::new();
        let key: i8 = kani::any();
        let other_key: i8 = kani::any();
        kani::assume(key != other_key);

        registry.register(key as i32, 42);

        kani::assert(
            registry.has(&(key as i32)),
            "has returns true for registered",
        );
        kani::assert(
            !registry.has(&(other_key as i32)),
            "has returns false for unregistered",
        );
    }

    #[kani::proof]
    fn proof_default_registry_get_or() {
        let registry: DefaultRegistry<i32, i32> = DefaultRegistry::new();
        let key: i8 = kani::any();
        let fallback: i8 = kani::any();

        let result = registry.get_or(&(key as i32), fallback as i32);
        kani::assert(
            result == fallback as i32,
            "get_or returns fallback for missing",
        );
    }

    #[kani::proof]
    fn proof_default_registry_get_or_with_value() {
        let mut registry: DefaultRegistry<i32, i32> = DefaultRegistry::new();
        let key: i8 = kani::any();
        let value: i8 = kani::any();
        let fallback: i8 = kani::any();

        registry.register(key as i32, value as i32);
        let result = registry.get_or(&(key as i32), fallback as i32);

        kani::assert(result == value as i32, "get_or returns value when present");
    }

    // ========================================================================
    // Defaultable Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_defaultable_new_is_default() {
        let default_val: i8 = kani::any();
        let d = Defaultable::new(default_val);

        kani::assert(d.is_default(), "new Defaultable is_default");
        kani::assert(*d.get() == default_val, "new Defaultable returns default");
    }

    #[kani::proof]
    fn proof_defaultable_set_not_default() {
        let default_val: i8 = kani::any();
        let value: i8 = kani::any();

        let mut d = Defaultable::new(default_val);
        d.set(value);

        kani::assert(!d.is_default(), "set Defaultable not is_default");
        kani::assert(*d.get() == value, "set Defaultable returns value");
    }

    #[kani::proof]
    fn proof_defaultable_clear_restores_default() {
        let default_val: i8 = kani::any();
        let value: i8 = kani::any();

        let mut d = Defaultable::new(default_val);
        d.set(value);
        d.clear();

        kani::assert(d.is_default(), "cleared Defaultable is_default");
        kani::assert(*d.get() == default_val, "cleared returns default");
    }

    #[kani::proof]
    fn proof_defaultable_default_value() {
        let default_val: i8 = kani::any();
        let value: i8 = kani::any();

        let mut d = Defaultable::new(default_val);
        d.set(value);

        // default_value should still return original default
        kani::assert(
            *d.default_value() == default_val,
            "default_value unchanged after set",
        );
    }

    // ========================================================================
    // LazyDefault Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_lazy_default_not_initialized() {
        let lazy = LazyDefault::new(|| 42i32);
        kani::assert(!lazy.is_initialized(), "new LazyDefault not initialized");
    }

    #[kani::proof]
    fn proof_lazy_default_get_initializes() {
        let value: i8 = kani::any();
        let mut lazy = LazyDefault::new(move || value);

        let result = *lazy.get();

        kani::assert(lazy.is_initialized(), "get initializes LazyDefault");
        kani::assert(result == value, "get returns initialized value");
    }

    #[kani::proof]
    fn proof_lazy_default_get_mut_initializes() {
        let value: i8 = kani::any();
        let mut lazy = LazyDefault::new(move || value);

        let result = *lazy.get_mut();

        kani::assert(lazy.is_initialized(), "get_mut initializes LazyDefault");
        kani::assert(result == value, "get_mut returns initialized value");
    }

    #[kani::proof]
    fn proof_lazy_default_get_idempotent() {
        let mut lazy = LazyDefault::new(|| 42i32);

        let first = *lazy.get();
        let second = *lazy.get();

        kani::assert(first == second, "multiple get calls return same value");
    }
}
