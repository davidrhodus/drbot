//! Triple utilities for drbot.
//!
//! This crate provides:
//! - Triple type and operations
//! - RGB-like triples
//! - Named triples

use thiserror::Error;

/// Triple error types.
#[derive(Error, Debug, Clone)]
pub enum TripleError {
    #[error("Invalid triple index: {0}")]
    InvalidIndex(usize),
}

/// Result type for triple operations.
pub type Result<T> = std::result::Result<T, TripleError>;

/// A generic triple type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Triple<A, B, C> {
    /// First element.
    pub first: A,
    /// Second element.
    pub second: B,
    /// Third element.
    pub third: C,
}

impl<A, B, C> Triple<A, B, C> {
    /// Create new triple.
    pub fn new(first: A, second: B, third: C) -> Self {
        Self {
            first,
            second,
            third,
        }
    }

    /// Map first element.
    pub fn map_first<D, F: FnOnce(A) -> D>(self, f: F) -> Triple<D, B, C> {
        Triple {
            first: f(self.first),
            second: self.second,
            third: self.third,
        }
    }

    /// Map second element.
    pub fn map_second<D, F: FnOnce(B) -> D>(self, f: F) -> Triple<A, D, C> {
        Triple {
            first: self.first,
            second: f(self.second),
            third: self.third,
        }
    }

    /// Map third element.
    pub fn map_third<D, F: FnOnce(C) -> D>(self, f: F) -> Triple<A, B, D> {
        Triple {
            first: self.first,
            second: self.second,
            third: f(self.third),
        }
    }

    /// Convert to tuple.
    pub fn into_tuple(self) -> (A, B, C) {
        (self.first, self.second, self.third)
    }

    /// Get first two elements as pair.
    pub fn first_pair(self) -> (A, B) {
        (self.first, self.second)
    }

    /// Get last two elements as pair.
    pub fn last_pair(self) -> (B, C) {
        (self.second, self.third)
    }
}

impl<A, B, C> From<(A, B, C)> for Triple<A, B, C> {
    fn from((first, second, third): (A, B, C)) -> Self {
        Self {
            first,
            second,
            third,
        }
    }
}

impl<A, B, C> From<Triple<A, B, C>> for (A, B, C) {
    fn from(t: Triple<A, B, C>) -> Self {
        (t.first, t.second, t.third)
    }
}

/// A homogeneous triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Triple3<T> {
    /// Elements.
    pub data: [T; 3],
}

impl<T> Triple3<T> {
    /// Create new triple.
    pub fn new(a: T, b: T, c: T) -> Self {
        Self { data: [a, b, c] }
    }

    /// Get first element.
    pub fn first(&self) -> &T {
        &self.data[0]
    }

    /// Get second element.
    pub fn second(&self) -> &T {
        &self.data[1]
    }

    /// Get third element.
    pub fn third(&self) -> &T {
        &self.data[2]
    }

    /// Get element by index.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }

    /// Map all elements.
    pub fn map<U, F: Fn(&T) -> U>(&self, f: F) -> Triple3<U> {
        Triple3 {
            data: [f(&self.data[0]), f(&self.data[1]), f(&self.data[2])],
        }
    }

    /// Convert to array.
    pub fn into_array(self) -> [T; 3] {
        self.data
    }

    /// Iterate over elements.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }
}

impl<T: Copy> Triple3<T> {
    /// Fold over elements.
    pub fn fold<U, F: Fn(U, T) -> U>(&self, init: U, f: F) -> U {
        let acc = f(init, self.data[0]);
        let acc = f(acc, self.data[1]);
        f(acc, self.data[2])
    }

    /// Sum elements.
    pub fn sum(&self) -> T
    where
        T: std::ops::Add<Output = T>,
    {
        self.data[0] + self.data[1] + self.data[2]
    }

    /// Product of elements.
    pub fn product(&self) -> T
    where
        T: std::ops::Mul<Output = T>,
    {
        self.data[0] * self.data[1] * self.data[2]
    }
}

impl<T: Ord + Copy> Triple3<T> {
    /// Get minimum element.
    pub fn min(&self) -> T {
        self.data[0].min(self.data[1]).min(self.data[2])
    }

    /// Get maximum element.
    pub fn max(&self) -> T {
        self.data[0].max(self.data[1]).max(self.data[2])
    }
}

impl<T> From<[T; 3]> for Triple3<T> {
    fn from(data: [T; 3]) -> Self {
        Self { data }
    }
}

/// RGB triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb<T> {
    /// Red component.
    pub r: T,
    /// Green component.
    pub g: T,
    /// Blue component.
    pub b: T,
}

impl<T> Rgb<T> {
    /// Create new RGB.
    pub fn new(r: T, g: T, b: T) -> Self {
        Self { r, g, b }
    }

    /// Map all components.
    pub fn map<U, F: Fn(T) -> U>(self, f: F) -> Rgb<U> {
        Rgb {
            r: f(self.r),
            g: f(self.g),
            b: f(self.b),
        }
    }

    /// Convert to array.
    pub fn into_array(self) -> [T; 3] {
        [self.r, self.g, self.b]
    }
}

impl Rgb<u8> {
    /// Create from hex value.
    pub fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as u8,
            g: ((hex >> 8) & 0xFF) as u8,
            b: (hex & 0xFF) as u8,
        }
    }

    /// Convert to hex value.
    pub fn to_hex(&self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    /// Convert to normalized floats.
    pub fn to_float(&self) -> Rgb<f32> {
        Rgb {
            r: self.r as f32 / 255.0,
            g: self.g as f32 / 255.0,
            b: self.b as f32 / 255.0,
        }
    }
}

impl Rgb<f32> {
    /// Convert to u8 values.
    pub fn to_u8(&self) -> Rgb<u8> {
        Rgb {
            r: (self.r.clamp(0.0, 1.0) * 255.0) as u8,
            g: (self.g.clamp(0.0, 1.0) * 255.0) as u8,
            b: (self.b.clamp(0.0, 1.0) * 255.0) as u8,
        }
    }

    /// Clamp values to 0-1 range.
    pub fn clamp(&self) -> Self {
        Self {
            r: self.r.clamp(0.0, 1.0),
            g: self.g.clamp(0.0, 1.0),
            b: self.b.clamp(0.0, 1.0),
        }
    }
}

/// XYZ coordinate triple.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Xyz<T> {
    /// X coordinate.
    pub x: T,
    /// Y coordinate.
    pub y: T,
    /// Z coordinate.
    pub z: T,
}

impl<T> Xyz<T> {
    /// Create new XYZ.
    pub fn new(x: T, y: T, z: T) -> Self {
        Self { x, y, z }
    }

    /// Map all components.
    pub fn map<U, F: Fn(T) -> U>(self, f: F) -> Xyz<U> {
        Xyz {
            x: f(self.x),
            y: f(self.y),
            z: f(self.z),
        }
    }

    /// Convert to array.
    pub fn into_array(self) -> [T; 3] {
        [self.x, self.y, self.z]
    }
}

impl<T: std::ops::Add<Output = T> + std::ops::Mul<Output = T> + Copy> Xyz<T> {
    /// Calculate squared magnitude.
    pub fn magnitude_squared(&self) -> T {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Dot product.
    pub fn dot(&self, other: &Self) -> T {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
}

impl Xyz<f32> {
    /// Calculate magnitude.
    pub fn magnitude(&self) -> f32 {
        self.magnitude_squared().sqrt()
    }

    /// Normalize to unit vector.
    pub fn normalize(&self) -> Self {
        let mag = self.magnitude();
        if mag > 0.0 {
            Self {
                x: self.x / mag,
                y: self.y / mag,
                z: self.z / mag,
            }
        } else {
            *self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triple() {
        let t = Triple::new(1, "hello", 3.14);
        assert_eq!(t.first, 1);
        assert_eq!(t.second, "hello");

        let tuple = t.into_tuple();
        assert_eq!(tuple, (1, "hello", 3.14));
    }

    #[test]
    fn test_triple3() {
        let t = Triple3::new(1, 2, 3);
        assert_eq!(t.sum(), 6);
        assert_eq!(t.product(), 6);
        assert_eq!(t.min(), 1);
        assert_eq!(t.max(), 3);
    }

    #[test]
    fn test_rgb() {
        let rgb = Rgb::from_hex(0xFF8040);
        assert_eq!(rgb.r, 255);
        assert_eq!(rgb.g, 128);
        assert_eq!(rgb.b, 64);

        assert_eq!(rgb.to_hex(), 0xFF8040);
    }

    #[test]
    fn test_xyz() {
        let v = Xyz::new(3.0f32, 4.0, 0.0);
        assert_eq!(v.magnitude(), 5.0);

        let normalized = v.normalize();
        assert!((normalized.magnitude() - 1.0).abs() < 0.0001);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // Triple<A, B, C> Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_triple_new_stores_values() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple::new(a, b, c);

        kani::assert(t.first == a, "first element must match");
        kani::assert(t.second == b, "second element must match");
        kani::assert(t.third == c, "third element must match");
    }

    #[kani::proof]
    fn proof_triple_into_tuple_preserves_values() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple::new(a, b, c);
        let (x, y, z) = t.into_tuple();

        kani::assert(x == a, "tuple first must match");
        kani::assert(y == b, "tuple second must match");
        kani::assert(z == c, "tuple third must match");
    }

    #[kani::proof]
    fn proof_triple_from_tuple_roundtrip() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let tuple = (a, b, c);
        let t: Triple<u8, u8, u8> = Triple::from(tuple);
        let back: (u8, u8, u8) = t.into();

        kani::assert(back.0 == a, "roundtrip first must match");
        kani::assert(back.1 == b, "roundtrip second must match");
        kani::assert(back.2 == c, "roundtrip third must match");
    }

    #[kani::proof]
    fn proof_triple_map_first() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple::new(a, b, c);
        let mapped = t.map_first(|x| x.wrapping_add(1));

        kani::assert(
            mapped.first == a.wrapping_add(1),
            "mapped first must be incremented",
        );
        kani::assert(mapped.second == b, "second unchanged");
        kani::assert(mapped.third == c, "third unchanged");
    }

    #[kani::proof]
    fn proof_triple_map_second() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple::new(a, b, c);
        let mapped = t.map_second(|x| x.wrapping_add(1));

        kani::assert(mapped.first == a, "first unchanged");
        kani::assert(
            mapped.second == b.wrapping_add(1),
            "mapped second must be incremented",
        );
        kani::assert(mapped.third == c, "third unchanged");
    }

    #[kani::proof]
    fn proof_triple_map_third() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple::new(a, b, c);
        let mapped = t.map_third(|x| x.wrapping_add(1));

        kani::assert(mapped.first == a, "first unchanged");
        kani::assert(mapped.second == b, "second unchanged");
        kani::assert(
            mapped.third == c.wrapping_add(1),
            "mapped third must be incremented",
        );
    }

    #[kani::proof]
    fn proof_triple_first_pair() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple::new(a, b, c);
        let (x, y) = t.first_pair();

        kani::assert(x == a, "first_pair.0 must be first");
        kani::assert(y == b, "first_pair.1 must be second");
    }

    #[kani::proof]
    fn proof_triple_last_pair() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple::new(a, b, c);
        let (x, y) = t.last_pair();

        kani::assert(x == b, "last_pair.0 must be second");
        kani::assert(y == c, "last_pair.1 must be third");
    }

    // ========================================================================
    // Triple3<T> Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_triple3_new_stores_values() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple3::new(a, b, c);

        kani::assert(t.data[0] == a, "data[0] must match");
        kani::assert(t.data[1] == b, "data[1] must match");
        kani::assert(t.data[2] == c, "data[2] must match");
    }

    #[kani::proof]
    fn proof_triple3_accessors() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple3::new(a, b, c);

        kani::assert(*t.first() == a, "first() must return data[0]");
        kani::assert(*t.second() == b, "second() must return data[1]");
        kani::assert(*t.third() == c, "third() must return data[2]");
    }

    #[kani::proof]
    fn proof_triple3_get_valid_indices() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple3::new(a, b, c);

        kani::assert(t.get(0) == Some(&a), "get(0) must return first");
        kani::assert(t.get(1) == Some(&b), "get(1) must return second");
        kani::assert(t.get(2) == Some(&c), "get(2) must return third");
    }

    #[kani::proof]
    fn proof_triple3_get_invalid_index() {
        let t = Triple3::new(1u8, 2u8, 3u8);
        let idx: usize = kani::any();
        kani::assume(idx >= 3);

        kani::assert(t.get(idx).is_none(), "out of bounds must return None");
    }

    #[kani::proof]
    fn proof_triple3_into_array() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple3::new(a, b, c);
        let arr = t.into_array();

        kani::assert(arr[0] == a, "array[0] must match");
        kani::assert(arr[1] == b, "array[1] must match");
        kani::assert(arr[2] == c, "array[2] must match");
    }

    #[kani::proof]
    fn proof_triple3_from_array() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let arr = [a, b, c];
        let t = Triple3::from(arr);

        kani::assert(t.data[0] == a, "from array data[0] must match");
        kani::assert(t.data[1] == b, "from array data[1] must match");
        kani::assert(t.data[2] == c, "from array data[2] must match");
    }

    #[kani::proof]
    fn proof_triple3_sum() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        // Avoid overflow
        kani::assume(a <= 80);
        kani::assume(b <= 80);
        kani::assume(c <= 80);

        let t = Triple3::new(a, b, c);
        let sum = t.sum();

        kani::assert(sum == a + b + c, "sum must equal a + b + c");
    }

    #[kani::proof]
    fn proof_triple3_product() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        // Avoid overflow
        kani::assume(a <= 6);
        kani::assume(b <= 6);
        kani::assume(c <= 6);

        let t = Triple3::new(a, b, c);
        let product = t.product();

        kani::assert(product == a * b * c, "product must equal a * b * c");
    }

    #[kani::proof]
    fn proof_triple3_min() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple3::new(a, b, c);
        let min = t.min();

        kani::assert(min <= a, "min must be <= a");
        kani::assert(min <= b, "min must be <= b");
        kani::assert(min <= c, "min must be <= c");
        kani::assert(
            min == a || min == b || min == c,
            "min must be one of the elements",
        );
    }

    #[kani::proof]
    fn proof_triple3_max() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        let t = Triple3::new(a, b, c);
        let max = t.max();

        kani::assert(max >= a, "max must be >= a");
        kani::assert(max >= b, "max must be >= b");
        kani::assert(max >= c, "max must be >= c");
        kani::assert(
            max == a || max == b || max == c,
            "max must be one of the elements",
        );
    }

    #[kani::proof]
    fn proof_triple3_fold() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        let c: u8 = kani::any();

        // Avoid overflow
        kani::assume(a <= 80);
        kani::assume(b <= 80);
        kani::assume(c <= 80);

        let t = Triple3::new(a, b, c);
        let sum = t.fold(0u8, |acc, x| acc + x);

        kani::assert(sum == a + b + c, "fold with add must equal sum");
    }

    // ========================================================================
    // Rgb<T> Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_rgb_new_stores_values() {
        let r: u8 = kani::any();
        let g: u8 = kani::any();
        let b: u8 = kani::any();

        let rgb = Rgb::new(r, g, b);

        kani::assert(rgb.r == r, "r must match");
        kani::assert(rgb.g == g, "g must match");
        kani::assert(rgb.b == b, "b must match");
    }

    #[kani::proof]
    fn proof_rgb_into_array() {
        let r: u8 = kani::any();
        let g: u8 = kani::any();
        let b: u8 = kani::any();

        let rgb = Rgb::new(r, g, b);
        let arr = rgb.into_array();

        kani::assert(arr[0] == r, "array[0] must be r");
        kani::assert(arr[1] == g, "array[1] must be g");
        kani::assert(arr[2] == b, "array[2] must be b");
    }

    #[kani::proof]
    fn proof_rgb_from_hex_to_hex_roundtrip() {
        let hex: u32 = kani::any();
        kani::assume(hex <= 0xFFFFFF); // Valid RGB hex range

        let rgb = Rgb::from_hex(hex);
        let back = rgb.to_hex();

        kani::assert(back == hex, "from_hex/to_hex roundtrip must preserve value");
    }

    #[kani::proof]
    fn proof_rgb_from_hex_components() {
        let hex: u32 = kani::any();
        kani::assume(hex <= 0xFFFFFF);

        let rgb = Rgb::from_hex(hex);

        let expected_r = ((hex >> 16) & 0xFF) as u8;
        let expected_g = ((hex >> 8) & 0xFF) as u8;
        let expected_b = (hex & 0xFF) as u8;

        kani::assert(rgb.r == expected_r, "r component must match");
        kani::assert(rgb.g == expected_g, "g component must match");
        kani::assert(rgb.b == expected_b, "b component must match");
    }

    #[kani::proof]
    fn proof_rgb_map() {
        let r: u8 = kani::any();
        let g: u8 = kani::any();
        let b: u8 = kani::any();

        let rgb = Rgb::new(r, g, b);
        let mapped = rgb.map(|x| x.wrapping_add(1));

        kani::assert(
            mapped.r == r.wrapping_add(1),
            "mapped r must be incremented",
        );
        kani::assert(
            mapped.g == g.wrapping_add(1),
            "mapped g must be incremented",
        );
        kani::assert(
            mapped.b == b.wrapping_add(1),
            "mapped b must be incremented",
        );
    }

    // ========================================================================
    // Xyz<T> Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_xyz_new_stores_values() {
        let x: i8 = kani::any();
        let y: i8 = kani::any();
        let z: i8 = kani::any();

        let xyz = Xyz::new(x, y, z);

        kani::assert(xyz.x == x, "x must match");
        kani::assert(xyz.y == y, "y must match");
        kani::assert(xyz.z == z, "z must match");
    }

    #[kani::proof]
    fn proof_xyz_into_array() {
        let x: i8 = kani::any();
        let y: i8 = kani::any();
        let z: i8 = kani::any();

        let xyz = Xyz::new(x, y, z);
        let arr = xyz.into_array();

        kani::assert(arr[0] == x, "array[0] must be x");
        kani::assert(arr[1] == y, "array[1] must be y");
        kani::assert(arr[2] == z, "array[2] must be z");
    }

    #[kani::proof]
    fn proof_xyz_map() {
        let x: i8 = kani::any();
        let y: i8 = kani::any();
        let z: i8 = kani::any();

        let xyz = Xyz::new(x, y, z);
        let mapped = xyz.map(|v| v.wrapping_add(1));

        kani::assert(
            mapped.x == x.wrapping_add(1),
            "mapped x must be incremented",
        );
        kani::assert(
            mapped.y == y.wrapping_add(1),
            "mapped y must be incremented",
        );
        kani::assert(
            mapped.z == z.wrapping_add(1),
            "mapped z must be incremented",
        );
    }

    #[kani::proof]
    fn proof_xyz_magnitude_squared_non_negative() {
        let x: i8 = kani::any();
        let y: i8 = kani::any();
        let z: i8 = kani::any();

        // Avoid overflow by using small values
        kani::assume(x >= -10 && x <= 10);
        kani::assume(y >= -10 && y <= 10);
        kani::assume(z >= -10 && z <= 10);

        let xyz = Xyz::new(x as i32, y as i32, z as i32);
        let mag_sq = xyz.magnitude_squared();

        kani::assert(mag_sq >= 0, "magnitude squared must be non-negative");
    }

    #[kani::proof]
    fn proof_xyz_dot_product_commutative() {
        let x1: i8 = kani::any();
        let y1: i8 = kani::any();
        let z1: i8 = kani::any();
        let x2: i8 = kani::any();
        let y2: i8 = kani::any();
        let z2: i8 = kani::any();

        // Avoid overflow
        kani::assume(x1 >= -5 && x1 <= 5);
        kani::assume(y1 >= -5 && y1 <= 5);
        kani::assume(z1 >= -5 && z1 <= 5);
        kani::assume(x2 >= -5 && x2 <= 5);
        kani::assume(y2 >= -5 && y2 <= 5);
        kani::assume(z2 >= -5 && z2 <= 5);

        let a = Xyz::new(x1 as i32, y1 as i32, z1 as i32);
        let b = Xyz::new(x2 as i32, y2 as i32, z2 as i32);

        kani::assert(a.dot(&b) == b.dot(&a), "dot product must be commutative");
    }

    #[kani::proof]
    fn proof_xyz_dot_with_self_equals_magnitude_squared() {
        let x: i8 = kani::any();
        let y: i8 = kani::any();
        let z: i8 = kani::any();

        // Avoid overflow
        kani::assume(x >= -10 && x <= 10);
        kani::assume(y >= -10 && y <= 10);
        kani::assume(z >= -10 && z <= 10);

        let xyz = Xyz::new(x as i32, y as i32, z as i32);

        kani::assert(
            xyz.dot(&xyz) == xyz.magnitude_squared(),
            "dot with self must equal magnitude squared",
        );
    }
}
