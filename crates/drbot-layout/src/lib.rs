//! Memory layout utilities for drbot.
//!
//! This crate provides:
//! - Layout computation
//! - Struct layout helpers
//! - Field offset utilities

use std::alloc::Layout;
use thiserror::Error;

/// Layout error types.
#[derive(Error, Debug, Clone)]
pub enum LayoutError {
    #[error("Invalid size")]
    InvalidSize,

    #[error("Invalid alignment")]
    InvalidAlignment,

    #[error("Overflow")]
    Overflow,
}

/// Result type for layout operations.
pub type Result<T> = std::result::Result<T, LayoutError>;

/// Create layout from size and alignment.
pub fn from_size_align(size: usize, align: usize) -> Result<Layout> {
    Layout::from_size_align(size, align).map_err(|_| LayoutError::InvalidAlignment)
}

/// Layout for type.
pub fn layout_for<T>() -> Layout {
    Layout::new::<T>()
}

/// Layout for array of type.
pub fn layout_for_array<T>(n: usize) -> Result<Layout> {
    Layout::array::<T>(n).map_err(|_| LayoutError::Overflow)
}

/// Size of type.
pub const fn size_of<T>() -> usize {
    std::mem::size_of::<T>()
}

/// Alignment of type.
pub const fn align_of<T>() -> usize {
    std::mem::align_of::<T>()
}

/// Pad to alignment.
pub fn pad_to_align(offset: usize, align: usize) -> usize {
    let rem = offset % align;
    if rem == 0 {
        offset
    } else {
        offset + (align - rem)
    }
}

/// Layout builder for structs.
#[derive(Debug, Clone)]
pub struct LayoutBuilder {
    size: usize,
    align: usize,
}

impl LayoutBuilder {
    /// Create new builder.
    pub fn new() -> Self {
        Self { size: 0, align: 1 }
    }

    /// Add a field.
    pub fn add_field<T>(&mut self) -> usize {
        let field_align = align_of::<T>();
        let field_size = size_of::<T>();

        self.align = self.align.max(field_align);
        let offset = pad_to_align(self.size, field_align);
        self.size = offset + field_size;

        offset
    }

    /// Add field with explicit size and alignment.
    pub fn add_field_raw(&mut self, size: usize, align: usize) -> usize {
        self.align = self.align.max(align);
        let offset = pad_to_align(self.size, align);
        self.size = offset + size;
        offset
    }

    /// Add padding.
    pub fn add_padding(&mut self, bytes: usize) {
        self.size += bytes;
    }

    /// Current size.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Current alignment.
    pub fn alignment(&self) -> usize {
        self.align
    }

    /// Build layout.
    pub fn build(self) -> Result<Layout> {
        let padded_size = pad_to_align(self.size, self.align);
        from_size_align(padded_size, self.align)
    }
}

impl Default for LayoutBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Field info.
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// Field name.
    pub name: &'static str,
    /// Field offset.
    pub offset: usize,
    /// Field size.
    pub size: usize,
    /// Field alignment.
    pub align: usize,
}

/// Struct layout info.
#[derive(Debug, Clone)]
pub struct StructLayout {
    /// Struct name.
    pub name: &'static str,
    /// Fields.
    pub fields: Vec<FieldInfo>,
    /// Total size.
    pub size: usize,
    /// Alignment.
    pub align: usize,
}

impl StructLayout {
    /// Create new.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            fields: Vec::new(),
            size: 0,
            align: 1,
        }
    }

    /// Add field.
    pub fn add_field(&mut self, name: &'static str, size: usize, align: usize) -> usize {
        self.align = self.align.max(align);
        let offset = pad_to_align(self.size, align);

        self.fields.push(FieldInfo {
            name,
            offset,
            size,
            align,
        });

        self.size = offset + size;
        offset
    }

    /// Finalize layout.
    pub fn finalize(&mut self) {
        self.size = pad_to_align(self.size, self.align);
    }

    /// Get field by name.
    pub fn field(&self, name: &str) -> Option<&FieldInfo> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Get offset of field.
    pub fn offset_of(&self, name: &str) -> Option<usize> {
        self.field(name).map(|f| f.offset)
    }
}

/// Extend layout with another.
pub fn extend_layout(base: Layout, ext: Layout) -> Result<(Layout, usize)> {
    base.extend(ext).map_err(|_| LayoutError::Overflow)
}

/// Repeat layout n times.
pub fn repeat_layout(layout: Layout, n: usize) -> Result<(Layout, usize)> {
    let padded_size = layout.pad_to_align().size();
    let total = padded_size.checked_mul(n).ok_or(LayoutError::Overflow)?;
    let new_layout = from_size_align(total, layout.align())?;
    Ok((new_layout, padded_size))
}

/// Align layout up.
pub fn align_to(layout: Layout, align: usize) -> Result<Layout> {
    layout
        .align_to(align)
        .map_err(|_| LayoutError::InvalidAlignment)
}

/// Pad layout to alignment.
pub fn pad_to(layout: Layout) -> Layout {
    layout.pad_to_align()
}

/// Check if alignment is valid.
pub fn is_valid_align(align: usize) -> bool {
    align.is_power_of_two()
}

/// Round up to alignment.
pub fn round_up(size: usize, align: usize) -> Option<usize> {
    if !is_valid_align(align) {
        return None;
    }
    let mask = align - 1;
    size.checked_add(mask).map(|s| s & !mask)
}

/// Calculate padding needed.
pub fn padding_needed(offset: usize, align: usize) -> usize {
    let rem = offset % align;
    if rem == 0 {
        0
    } else {
        align - rem
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_builder() {
        let mut builder = LayoutBuilder::new();
        let offset_a = builder.add_field::<u8>();
        let offset_b = builder.add_field::<u32>();
        let offset_c = builder.add_field::<u8>();

        assert_eq!(offset_a, 0);
        assert_eq!(offset_b, 4); // Padded for alignment
        assert_eq!(offset_c, 8);
    }

    #[test]
    fn test_pad_to_align() {
        assert_eq!(pad_to_align(1, 4), 4);
        assert_eq!(pad_to_align(4, 4), 4);
        assert_eq!(pad_to_align(5, 4), 8);
    }

    #[test]
    fn test_struct_layout() {
        let mut layout = StructLayout::new("Test");
        layout.add_field("a", 1, 1);
        layout.add_field("b", 4, 4);
        layout.add_field("c", 1, 1);
        layout.finalize();

        assert_eq!(layout.offset_of("a"), Some(0));
        assert_eq!(layout.offset_of("b"), Some(4));
        assert_eq!(layout.offset_of("c"), Some(8));
    }

    #[test]
    fn test_round_up() {
        assert_eq!(round_up(1, 4), Some(4));
        assert_eq!(round_up(4, 4), Some(4));
        assert_eq!(round_up(5, 4), Some(8));
    }
}
