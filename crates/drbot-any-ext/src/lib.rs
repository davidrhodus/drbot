//! Any type extensions for drbot.
//!
//! This crate provides:
//! - Extended Any trait
//! - Type-safe casting
//! - Any value containers

use std::any::{Any, TypeId};
use thiserror::Error;

/// Any extension error types.
#[derive(Error, Debug, Clone)]
pub enum AnyError {
    #[error("Cast failed: expected {expected}, found {found}")]
    CastFailed { expected: String, found: String },

    #[error("Value not set")]
    NotSet,
}

/// Result type for any operations.
pub type Result<T> = std::result::Result<T, AnyError>;

/// Extension trait for Any.
pub trait AnyExt: Any {
    /// Get type name.
    fn type_name(&self) -> &'static str;

    /// Check if is type.
    fn is_type<T: 'static>(&self) -> bool;

    /// Try to downcast reference.
    fn try_ref<T: 'static>(&self) -> Option<&T>;

    /// Try to downcast mutable reference.
    fn try_mut<T: 'static>(&mut self) -> Option<&mut T>;
}

impl<A: Any> AnyExt for A {
    fn type_name(&self) -> &'static str {
        std::any::type_name::<A>()
    }

    fn is_type<T: 'static>(&self) -> bool {
        TypeId::of::<A>() == TypeId::of::<T>()
    }

    fn try_ref<T: 'static>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref()
    }

    fn try_mut<T: 'static>(&mut self) -> Option<&mut T> {
        (self as &mut dyn Any).downcast_mut()
    }
}

/// Boxed any value.
pub struct AnyBox {
    value: Box<dyn Any + Send + Sync>,
    type_name: &'static str,
}

impl AnyBox {
    /// Create new any box.
    pub fn new<T: Any + Send + Sync + 'static>(value: T) -> Self {
        Self {
            value: Box::new(value),
            type_name: std::any::type_name::<T>(),
        }
    }

    /// Get type name.
    pub fn type_name(&self) -> &'static str {
        self.type_name
    }

    /// Check if is type.
    pub fn is<T: 'static>(&self) -> bool {
        self.value.is::<T>()
    }

    /// Try to downcast reference.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.value.downcast_ref()
    }

    /// Try to downcast mutable reference.
    pub fn downcast_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.value.downcast_mut()
    }

    /// Try to downcast and take ownership.
    pub fn downcast<T: 'static>(self) -> std::result::Result<T, Self> {
        match self.value.downcast::<T>() {
            Ok(v) => Ok(*v),
            Err(value) => Err(Self {
                value,
                type_name: self.type_name,
            }),
        }
    }

    /// Get reference or error.
    pub fn get_ref<T: 'static>(&self) -> Result<&T> {
        self.downcast_ref().ok_or_else(|| AnyError::CastFailed {
            expected: std::any::type_name::<T>().to_string(),
            found: self.type_name.to_string(),
        })
    }

    /// Get mutable reference or error.
    pub fn get_mut<T: 'static>(&mut self) -> Result<&mut T> {
        let type_name = self.type_name;
        self.downcast_mut().ok_or_else(|| AnyError::CastFailed {
            expected: std::any::type_name::<T>().to_string(),
            found: type_name.to_string(),
        })
    }
}

impl std::fmt::Debug for AnyBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnyBox")
            .field("type_name", &self.type_name)
            .finish()
    }
}

/// Optional any value.
pub struct AnyOption {
    value: Option<AnyBox>,
}

impl AnyOption {
    /// Create empty.
    pub fn none() -> Self {
        Self { value: None }
    }

    /// Create with value.
    pub fn some<T: Any + Send + Sync + 'static>(value: T) -> Self {
        Self {
            value: Some(AnyBox::new(value)),
        }
    }

    /// Check if has value.
    pub fn is_some(&self) -> bool {
        self.value.is_some()
    }

    /// Check if empty.
    pub fn is_none(&self) -> bool {
        self.value.is_none()
    }

    /// Set value.
    pub fn set<T: Any + Send + Sync + 'static>(&mut self, value: T) {
        self.value = Some(AnyBox::new(value));
    }

    /// Clear value.
    pub fn clear(&mut self) {
        self.value = None;
    }

    /// Get reference.
    pub fn get_ref<T: 'static>(&self) -> Result<&T> {
        self.value.as_ref().ok_or(AnyError::NotSet)?.get_ref()
    }

    /// Get mutable reference.
    pub fn get_mut<T: 'static>(&mut self) -> Result<&mut T> {
        self.value.as_mut().ok_or(AnyError::NotSet)?.get_mut()
    }

    /// Take value.
    pub fn take<T: 'static>(&mut self) -> Result<T> {
        let boxed = self.value.take().ok_or(AnyError::NotSet)?;
        boxed.downcast().map_err(|b| {
            self.value = Some(b);
            AnyError::CastFailed {
                expected: std::any::type_name::<T>().to_string(),
                found: self.value.as_ref().unwrap().type_name().to_string(),
            }
        })
    }
}

impl Default for AnyOption {
    fn default() -> Self {
        Self::none()
    }
}

impl std::fmt::Debug for AnyOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnyOption")
            .field("is_some", &self.is_some())
            .finish()
    }
}

/// Cast reference to type.
pub fn cast_ref<T: 'static>(value: &dyn Any) -> Option<&T> {
    value.downcast_ref()
}

/// Cast mutable reference to type.
pub fn cast_mut<T: 'static>(value: &mut dyn Any) -> Option<&mut T> {
    value.downcast_mut()
}

/// Check if any value is type.
pub fn is_type<T: 'static>(value: &dyn Any) -> bool {
    value.is::<T>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_any_ext() {
        let value = 42i32;

        assert!(value.is_type::<i32>());
        assert!(!value.is_type::<String>());
        assert_eq!(value.try_ref::<i32>(), Some(&42));
    }

    #[test]
    fn test_any_box() {
        let boxed = AnyBox::new(42i32);

        assert!(boxed.is::<i32>());
        assert_eq!(boxed.downcast_ref::<i32>(), Some(&42));
        assert_eq!(boxed.downcast::<i32>().ok(), Some(42));
    }

    #[test]
    fn test_any_box_failed_cast() {
        let boxed = AnyBox::new(42i32);

        assert!(boxed.downcast_ref::<String>().is_none());
        assert!(boxed.get_ref::<String>().is_err());
    }

    #[test]
    fn test_any_option() {
        let mut opt = AnyOption::none();
        assert!(opt.is_none());

        opt.set(42i32);
        assert!(opt.is_some());
        assert_eq!(opt.get_ref::<i32>().ok(), Some(&42));

        let taken = opt.take::<i32>();
        assert!(taken.is_ok());
        assert!(opt.is_none());
    }

    #[test]
    fn test_cast_functions() {
        let value: Box<dyn Any> = Box::new(42i32);

        assert!(is_type::<i32>(&*value));
        assert_eq!(cast_ref::<i32>(&*value), Some(&42));
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // AnyExt Trait Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_any_ext_is_type_same_type() {
        let value: u8 = kani::any();
        kani::assert(value.is_type::<u8>(), "value must be its own type");
    }

    #[kani::proof]
    fn proof_any_ext_is_type_different_type() {
        let value: u8 = kani::any();
        kani::assert(!value.is_type::<u16>(), "u8 must not be u16");
        kani::assert(!value.is_type::<i8>(), "u8 must not be i8");
    }

    #[kani::proof]
    fn proof_any_ext_try_ref_same_type() {
        let value: u8 = kani::any();
        let result = value.try_ref::<u8>();
        kani::assert(result.is_some(), "try_ref same type must succeed");
        kani::assert(*result.unwrap() == value, "must return correct value");
    }

    #[kani::proof]
    fn proof_any_ext_try_ref_different_type() {
        let value: u8 = kani::any();
        let result = value.try_ref::<u16>();
        kani::assert(result.is_none(), "try_ref different type must fail");
    }

    #[kani::proof]
    fn proof_any_ext_try_mut_same_type() {
        let mut value: u8 = kani::any();
        let original = value;
        let result = value.try_mut::<u8>();
        kani::assert(result.is_some(), "try_mut same type must succeed");
        kani::assert(*result.unwrap() == original, "must return correct value");
    }

    #[kani::proof]
    fn proof_any_ext_try_mut_modifiable() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut stored = value;

        if let Some(r) = stored.try_mut::<u8>() {
            *r = new_value;
        }

        kani::assert(stored == new_value, "try_mut must allow modification");
    }

    // ========================================================================
    // AnyBox Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_any_box_new_stores_value() {
        let value: u8 = kani::any();
        let boxed = AnyBox::new(value);

        kani::assert(boxed.is::<u8>(), "must be correct type");
        kani::assert(
            *boxed.downcast_ref::<u8>().unwrap() == value,
            "must store value",
        );
    }

    #[kani::proof]
    fn proof_any_box_is_correct_type() {
        let value: u8 = kani::any();
        let boxed = AnyBox::new(value);

        kani::assert(boxed.is::<u8>(), "is must return true for correct type");
        kani::assert(!boxed.is::<u16>(), "is must return false for wrong type");
    }

    #[kani::proof]
    fn proof_any_box_downcast_ref_correct_type() {
        let value: u8 = kani::any();
        let boxed = AnyBox::new(value);

        let result = boxed.downcast_ref::<u8>();
        kani::assert(
            result.is_some(),
            "downcast_ref must succeed for correct type",
        );
        kani::assert(*result.unwrap() == value, "must return correct value");
    }

    #[kani::proof]
    fn proof_any_box_downcast_ref_wrong_type() {
        let value: u8 = kani::any();
        let boxed = AnyBox::new(value);

        let result = boxed.downcast_ref::<u16>();
        kani::assert(result.is_none(), "downcast_ref must fail for wrong type");
    }

    #[kani::proof]
    fn proof_any_box_downcast_mut_modifiable() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut boxed = AnyBox::new(value);

        if let Some(r) = boxed.downcast_mut::<u8>() {
            *r = new_value;
        }

        kani::assert(
            *boxed.downcast_ref::<u8>().unwrap() == new_value,
            "must be modified",
        );
    }

    #[kani::proof]
    fn proof_any_box_downcast_success() {
        let value: u8 = kani::any();
        let boxed = AnyBox::new(value);

        let result = boxed.downcast::<u8>();
        kani::assert(result.is_ok(), "downcast must succeed for correct type");
        kani::assert(result.unwrap() == value, "must return correct value");
    }

    #[kani::proof]
    fn proof_any_box_downcast_failure() {
        let value: u8 = kani::any();
        let boxed = AnyBox::new(value);

        let result = boxed.downcast::<u16>();
        kani::assert(result.is_err(), "downcast must fail for wrong type");
    }

    #[kani::proof]
    fn proof_any_box_get_ref_success() {
        let value: u8 = kani::any();
        let boxed = AnyBox::new(value);

        let result = boxed.get_ref::<u8>();
        kani::assert(result.is_ok(), "get_ref must succeed for correct type");
        kani::assert(*result.unwrap() == value, "must return correct value");
    }

    #[kani::proof]
    fn proof_any_box_get_ref_failure() {
        let value: u8 = kani::any();
        let boxed = AnyBox::new(value);

        let result = boxed.get_ref::<u16>();
        kani::assert(result.is_err(), "get_ref must fail for wrong type");
    }

    #[kani::proof]
    fn proof_any_box_get_mut_success() {
        let value: u8 = kani::any();
        let mut boxed = AnyBox::new(value);

        let result = boxed.get_mut::<u8>();
        kani::assert(result.is_ok(), "get_mut must succeed for correct type");
    }

    #[kani::proof]
    fn proof_any_box_get_mut_failure() {
        let value: u8 = kani::any();
        let mut boxed = AnyBox::new(value);

        let result = boxed.get_mut::<u16>();
        kani::assert(result.is_err(), "get_mut must fail for wrong type");
    }

    // ========================================================================
    // AnyOption Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_any_option_none_is_none() {
        let opt = AnyOption::none();
        kani::assert(opt.is_none(), "none must return is_none true");
        kani::assert(!opt.is_some(), "none must return is_some false");
    }

    #[kani::proof]
    fn proof_any_option_some_is_some() {
        let value: u8 = kani::any();
        let opt = AnyOption::some(value);
        kani::assert(opt.is_some(), "some must return is_some true");
        kani::assert(!opt.is_none(), "some must return is_none false");
    }

    #[kani::proof]
    fn proof_any_option_default_is_none() {
        let opt = AnyOption::default();
        kani::assert(opt.is_none(), "default must be none");
    }

    #[kani::proof]
    fn proof_any_option_set_makes_some() {
        let value: u8 = kani::any();
        let mut opt = AnyOption::none();
        opt.set(value);
        kani::assert(opt.is_some(), "set must make some");
    }

    #[kani::proof]
    fn proof_any_option_clear_makes_none() {
        let value: u8 = kani::any();
        let mut opt = AnyOption::some(value);
        opt.clear();
        kani::assert(opt.is_none(), "clear must make none");
    }

    #[kani::proof]
    fn proof_any_option_get_ref_some() {
        let value: u8 = kani::any();
        let opt = AnyOption::some(value);
        let result = opt.get_ref::<u8>();
        kani::assert(result.is_ok(), "get_ref on some must succeed");
        kani::assert(*result.unwrap() == value, "must return correct value");
    }

    #[kani::proof]
    fn proof_any_option_get_ref_none() {
        let opt = AnyOption::none();
        let result = opt.get_ref::<u8>();
        kani::assert(result.is_err(), "get_ref on none must fail");
    }

    #[kani::proof]
    fn proof_any_option_get_ref_wrong_type() {
        let value: u8 = kani::any();
        let opt = AnyOption::some(value);
        let result = opt.get_ref::<u16>();
        kani::assert(result.is_err(), "get_ref wrong type must fail");
    }

    #[kani::proof]
    fn proof_any_option_take_success() {
        let value: u8 = kani::any();
        let mut opt = AnyOption::some(value);

        let result = opt.take::<u8>();
        kani::assert(result.is_ok(), "take must succeed");
        kani::assert(result.unwrap() == value, "must return correct value");
        kani::assert(opt.is_none(), "must be none after take");
    }

    #[kani::proof]
    fn proof_any_option_take_none() {
        let mut opt = AnyOption::none();
        let result = opt.take::<u8>();
        kani::assert(result.is_err(), "take on none must fail");
    }

    // ========================================================================
    // Helper Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_is_type_correct() {
        let value: u8 = kani::any();
        let boxed: Box<dyn Any> = Box::new(value);

        kani::assert(
            is_type::<u8>(&*boxed),
            "is_type must return true for correct type",
        );
        kani::assert(
            !is_type::<u16>(&*boxed),
            "is_type must return false for wrong type",
        );
    }

    #[kani::proof]
    fn proof_cast_ref_correct() {
        let value: u8 = kani::any();
        let boxed: Box<dyn Any> = Box::new(value);

        let result = cast_ref::<u8>(&*boxed);
        kani::assert(result.is_some(), "cast_ref must succeed for correct type");
        kani::assert(*result.unwrap() == value, "must return correct value");
    }

    #[kani::proof]
    fn proof_cast_ref_wrong() {
        let value: u8 = kani::any();
        let boxed: Box<dyn Any> = Box::new(value);

        let result = cast_ref::<u16>(&*boxed);
        kani::assert(result.is_none(), "cast_ref must fail for wrong type");
    }

    #[kani::proof]
    fn proof_cast_mut_modifiable() {
        let value: u8 = kani::any();
        let new_value: u8 = kani::any();
        let mut boxed: Box<dyn Any> = Box::new(value);

        if let Some(r) = cast_mut::<u8>(&mut *boxed) {
            *r = new_value;
        }

        kani::assert(
            *cast_ref::<u8>(&*boxed).unwrap() == new_value,
            "must be modified",
        );
    }
}
