//! Value inspection utilities for drbot.
//!
//! This crate provides:
//! - Value inspection
//! - Type inspection
//! - Memory inspection

use std::any::{type_name, TypeId};
use std::fmt::Debug;
use std::mem;
use thiserror::Error;

/// Inspect error types.
#[derive(Error, Debug, Clone)]
pub enum InspectError {
    #[error("Cannot inspect: {0}")]
    CannotInspect(String),
}

/// Result type for inspect operations.
pub type Result<T> = std::result::Result<T, InspectError>;

/// Type info.
#[derive(Debug, Clone)]
pub struct TypeInfo {
    /// Type name.
    pub name: String,
    /// Type ID.
    pub id: TypeId,
    /// Size in bytes.
    pub size: usize,
    /// Alignment.
    pub align: usize,
}

impl TypeInfo {
    /// Get type info for T.
    pub fn of<T: 'static>() -> Self {
        Self {
            name: type_name::<T>().to_string(),
            id: TypeId::of::<T>(),
            size: mem::size_of::<T>(),
            align: mem::align_of::<T>(),
        }
    }

    /// Short type name.
    pub fn short_name(&self) -> &str {
        self.name.rsplit("::").next().unwrap_or(&self.name)
    }
}

/// Get type info.
pub fn type_info<T: 'static>() -> TypeInfo {
    TypeInfo::of::<T>()
}

/// Get size of type.
pub fn size_of<T>() -> usize {
    mem::size_of::<T>()
}

/// Get size of value.
pub fn size_of_val<T: ?Sized>(value: &T) -> usize {
    mem::size_of_val(value)
}

/// Get alignment of type.
pub fn align_of<T>() -> usize {
    mem::align_of::<T>()
}

/// Value inspector.
pub struct Inspector<'a, T: ?Sized> {
    value: &'a T,
}

impl<'a, T: 'static> Inspector<'a, T> {
    /// Create new inspector.
    pub fn new(value: &'a T) -> Self {
        Self { value }
    }

    /// Get type info.
    pub fn type_info(&self) -> TypeInfo {
        TypeInfo::of::<T>()
    }

    /// Get size.
    pub fn size(&self) -> usize {
        mem::size_of_val(self.value)
    }

    /// Get value reference.
    pub fn value(&self) -> &T {
        self.value
    }

    /// Get address.
    pub fn address(&self) -> usize {
        self.value as *const T as usize
    }
}

impl<'a, T: Debug + 'static> Inspector<'a, T> {
    /// Get debug string.
    pub fn debug(&self) -> String {
        format!("{:?}", self.value)
    }

    /// Get pretty debug string.
    pub fn debug_pretty(&self) -> String {
        format!("{:#?}", self.value)
    }
}

/// Inspect a value.
pub fn inspect<T: 'static>(value: &T) -> Inspector<'_, T> {
    Inspector::new(value)
}

/// Memory layout info.
#[derive(Debug, Clone)]
pub struct LayoutInfo {
    /// Size.
    pub size: usize,
    /// Alignment.
    pub align: usize,
}

impl LayoutInfo {
    /// Get layout for type.
    pub fn of<T>() -> Self {
        Self {
            size: mem::size_of::<T>(),
            align: mem::align_of::<T>(),
        }
    }
}

/// Get layout info.
pub fn layout<T>() -> LayoutInfo {
    LayoutInfo::of::<T>()
}

/// Inspectable trait.
pub trait Inspectable: Sized + 'static {
    /// Get type info.
    fn type_info(&self) -> TypeInfo {
        TypeInfo::of::<Self>()
    }

    /// Get size.
    fn inspect_size(&self) -> usize {
        mem::size_of_val(self)
    }

    /// Get address.
    fn inspect_address(&self) -> usize {
        self as *const Self as usize
    }
}

impl<T: Sized + 'static> Inspectable for T {}

/// Field info.
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// Field name.
    pub name: String,
    /// Field type.
    pub type_name: String,
    /// Offset in parent.
    pub offset: usize,
    /// Size.
    pub size: usize,
}

/// Struct inspector builder.
pub struct StructInspector {
    name: String,
    fields: Vec<FieldInfo>,
    total_size: usize,
}

impl StructInspector {
    /// Create new.
    pub fn new<T: 'static>() -> Self {
        Self {
            name: type_name::<T>().to_string(),
            fields: Vec::new(),
            total_size: mem::size_of::<T>(),
        }
    }

    /// Add field.
    pub fn field<T: 'static>(mut self, name: &str, offset: usize) -> Self {
        self.fields.push(FieldInfo {
            name: name.to_string(),
            type_name: type_name::<T>().to_string(),
            offset,
            size: mem::size_of::<T>(),
        });
        self
    }

    /// Get struct name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get fields.
    pub fn fields(&self) -> &[FieldInfo] {
        &self.fields
    }

    /// Get total size.
    pub fn total_size(&self) -> usize {
        self.total_size
    }

    /// Get padding size.
    pub fn padding_size(&self) -> usize {
        let fields_size: usize = self.fields.iter().map(|f| f.size).sum();
        self.total_size.saturating_sub(fields_size)
    }
}

/// Check if type is zero-sized.
pub fn is_zst<T>() -> bool {
    mem::size_of::<T>() == 0
}

/// Check if type needs drop.
pub fn needs_drop<T>() -> bool {
    mem::needs_drop::<T>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_info() {
        let info = TypeInfo::of::<i32>();
        assert_eq!(info.size, 4);
        assert!(info.name.contains("i32"));
    }

    #[test]
    fn test_inspector() {
        let value = 42i32;
        let inspector = inspect(&value);
        assert_eq!(inspector.size(), 4);
        assert_eq!(*inspector.value(), 42);
    }

    #[test]
    fn test_layout() {
        let layout = layout::<u64>();
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 8);
    }

    #[test]
    fn test_is_zst() {
        assert!(is_zst::<()>());
        assert!(!is_zst::<i32>());
    }

    #[test]
    fn test_struct_inspector() {
        let inspector = StructInspector::new::<(i32, i32)>()
            .field::<i32>("0", 0)
            .field::<i32>("1", 4);
        assert_eq!(inspector.fields().len(), 2);
    }
}
