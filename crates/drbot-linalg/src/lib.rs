//! Linear algebra utilities for drbot.
//!
//! This crate provides:
//! - Vector operations
//! - Matrix operations
//! - Basic linear algebra

use std::ops::{Add, Mul, Sub};
use thiserror::Error;

/// Linear algebra error types.
#[derive(Error, Debug)]
pub enum LinalgError {
    #[error("Dimension mismatch")]
    DimensionMismatch,

    #[error("Singular matrix")]
    SingularMatrix,

    #[error("Invalid dimensions")]
    InvalidDimensions,
}

/// Result type for linalg operations.
pub type Result<T> = std::result::Result<T, LinalgError>;

/// 2D Vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    /// Create new vector.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Zero vector.
    pub fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    /// Unit vector in x direction.
    pub fn unit_x() -> Self {
        Self { x: 1.0, y: 0.0 }
    }

    /// Unit vector in y direction.
    pub fn unit_y() -> Self {
        Self { x: 0.0, y: 1.0 }
    }

    /// Calculate length (magnitude).
    pub fn length(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    /// Calculate squared length.
    pub fn length_squared(&self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    /// Normalize to unit vector.
    pub fn normalize(&self) -> Self {
        let len = self.length();
        if len > 0.0 {
            Self {
                x: self.x / len,
                y: self.y / len,
            }
        } else {
            *self
        }
    }

    /// Dot product.
    pub fn dot(&self, other: &Self) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Cross product (returns scalar for 2D).
    pub fn cross(&self, other: &Self) -> f64 {
        self.x * other.y - self.y * other.x
    }

    /// Distance to another vector.
    pub fn distance(&self, other: &Self) -> f64 {
        (*self - *other).length()
    }

    /// Angle in radians.
    pub fn angle(&self) -> f64 {
        self.y.atan2(self.x)
    }

    /// Rotate by angle (radians).
    pub fn rotate(&self, angle: f64) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            x: self.x * cos - self.y * sin,
            y: self.x * sin + self.y * cos,
        }
    }

    /// Linear interpolation.
    pub fn lerp(&self, other: &Self, t: f64) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl Mul<f64> for Vec2 {
    type Output = Self;

    fn mul(self, scalar: f64) -> Self {
        Self {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }
}

/// 3D Vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    /// Create new vector.
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Zero vector.
    pub fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }

    /// Calculate length.
    pub fn length(&self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Normalize.
    pub fn normalize(&self) -> Self {
        let len = self.length();
        if len > 0.0 {
            Self {
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
            }
        } else {
            *self
        }
    }

    /// Dot product.
    pub fn dot(&self, other: &Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Cross product.
    pub fn cross(&self, other: &Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }
}

impl Mul<f64> for Vec3 {
    type Output = Self;

    fn mul(self, scalar: f64) -> Self {
        Self {
            x: self.x * scalar,
            y: self.y * scalar,
            z: self.z * scalar,
        }
    }
}

/// 2x2 Matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat2 {
    pub data: [[f64; 2]; 2],
}

impl Mat2 {
    /// Create from elements.
    pub fn new(m00: f64, m01: f64, m10: f64, m11: f64) -> Self {
        Self {
            data: [[m00, m01], [m10, m11]],
        }
    }

    /// Identity matrix.
    pub fn identity() -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0)
    }

    /// Zero matrix.
    pub fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }

    /// Calculate determinant.
    pub fn determinant(&self) -> f64 {
        self.data[0][0] * self.data[1][1] - self.data[0][1] * self.data[1][0]
    }

    /// Calculate inverse.
    pub fn inverse(&self) -> Result<Self> {
        let det = self.determinant();
        if det.abs() < 1e-10 {
            return Err(LinalgError::SingularMatrix);
        }

        let inv_det = 1.0 / det;
        Ok(Self::new(
            self.data[1][1] * inv_det,
            -self.data[0][1] * inv_det,
            -self.data[1][0] * inv_det,
            self.data[0][0] * inv_det,
        ))
    }

    /// Transpose.
    pub fn transpose(&self) -> Self {
        Self::new(
            self.data[0][0],
            self.data[1][0],
            self.data[0][1],
            self.data[1][1],
        )
    }

    /// Multiply by vector.
    pub fn mul_vec(&self, v: Vec2) -> Vec2 {
        Vec2::new(
            self.data[0][0] * v.x + self.data[0][1] * v.y,
            self.data[1][0] * v.x + self.data[1][1] * v.y,
        )
    }

    /// Rotation matrix.
    pub fn rotation(angle: f64) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self::new(cos, -sin, sin, cos)
    }

    /// Scale matrix.
    pub fn scale(sx: f64, sy: f64) -> Self {
        Self::new(sx, 0.0, 0.0, sy)
    }
}

impl Mul for Mat2 {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self::new(
            self.data[0][0] * other.data[0][0] + self.data[0][1] * other.data[1][0],
            self.data[0][0] * other.data[0][1] + self.data[0][1] * other.data[1][1],
            self.data[1][0] * other.data[0][0] + self.data[1][1] * other.data[1][0],
            self.data[1][0] * other.data[0][1] + self.data[1][1] * other.data[1][1],
        )
    }
}

/// Dynamic vector.
#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    pub data: Vec<f64>,
}

impl Vector {
    /// Create from data.
    pub fn new(data: Vec<f64>) -> Self {
        Self { data }
    }

    /// Create zeros.
    pub fn zeros(n: usize) -> Self {
        Self { data: vec![0.0; n] }
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Dot product.
    pub fn dot(&self, other: &Self) -> Result<f64> {
        if self.len() != other.len() {
            return Err(LinalgError::DimensionMismatch);
        }
        Ok(self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(a, b)| a * b)
            .sum())
    }

    /// Magnitude.
    pub fn magnitude(&self) -> f64 {
        self.data.iter().map(|x| x * x).sum::<f64>().sqrt()
    }

    /// Normalize.
    pub fn normalize(&self) -> Self {
        let mag = self.magnitude();
        if mag > 0.0 {
            Self {
                data: self.data.iter().map(|x| x / mag).collect(),
            }
        } else {
            self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec2_operations() {
        let v1 = Vec2::new(3.0, 4.0);
        assert!((v1.length() - 5.0).abs() < 1e-10);

        let v2 = Vec2::new(1.0, 0.0);
        assert!((v1.dot(&v2) - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_vec2_normalize() {
        let v = Vec2::new(3.0, 4.0);
        let n = v.normalize();
        assert!((n.length() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_vec3_cross() {
        let x = Vec3::new(1.0, 0.0, 0.0);
        let y = Vec3::new(0.0, 1.0, 0.0);
        let z = x.cross(&y);
        assert!((z.x - 0.0).abs() < 1e-10);
        assert!((z.y - 0.0).abs() < 1e-10);
        assert!((z.z - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_mat2_identity() {
        let m = Mat2::identity();
        let v = Vec2::new(3.0, 4.0);
        let result = m.mul_vec(v);
        assert!((result.x - v.x).abs() < 1e-10);
        assert!((result.y - v.y).abs() < 1e-10);
    }

    #[test]
    fn test_mat2_inverse() {
        let m = Mat2::new(1.0, 2.0, 3.0, 4.0);
        let inv = m.inverse().unwrap();
        let product = m * inv;

        // Should be approximately identity
        assert!((product.data[0][0] - 1.0).abs() < 1e-10);
        assert!((product.data[1][1] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_vector_dot() {
        let v1 = Vector::new(vec![1.0, 2.0, 3.0]);
        let v2 = Vector::new(vec![4.0, 5.0, 6.0]);
        assert!((v1.dot(&v2).unwrap() - 32.0).abs() < 1e-10);
    }
}
