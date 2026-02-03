//! Math utilities for drbot.
//!
//! This crate provides:
//! - Common math functions
//! - Number utilities
//! - Interpolation
//! - Numerical methods

use thiserror::Error;

/// Math error types.
#[derive(Error, Debug)]
pub enum MathError {
    #[error("Division by zero")]
    DivisionByZero,

    #[error("Domain error: {0}")]
    DomainError(String),

    #[error("Overflow")]
    Overflow,

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}

/// Result type for math operations.
pub type Result<T> = std::result::Result<T, MathError>;

/// Clamp value to range.
pub fn clamp<T: PartialOrd>(value: T, min: T, max: T) -> T {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Linear interpolation.
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Inverse linear interpolation.
pub fn inverse_lerp(a: f64, b: f64, value: f64) -> f64 {
    if (b - a).abs() < f64::EPSILON {
        0.0
    } else {
        (value - a) / (b - a)
    }
}

/// Map value from one range to another.
pub fn map_range(value: f64, in_min: f64, in_max: f64, out_min: f64, out_max: f64) -> f64 {
    let t = inverse_lerp(in_min, in_max, value);
    lerp(out_min, out_max, t)
}

/// Smooth step interpolation.
pub fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smoother step interpolation (Ken Perlin).
pub fn smootherstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Calculate factorial.
pub fn factorial(n: u64) -> Result<u64> {
    if n > 20 {
        return Err(MathError::Overflow);
    }

    let mut result = 1u64;
    for i in 2..=n {
        result = result.checked_mul(i).ok_or(MathError::Overflow)?;
    }
    Ok(result)
}

/// Calculate binomial coefficient (n choose k).
pub fn binomial(n: u64, k: u64) -> Result<u64> {
    if k > n {
        return Ok(0);
    }
    if k == 0 || k == n {
        return Ok(1);
    }

    let k = k.min(n - k); // Optimization
    let mut result = 1u64;

    for i in 0..k {
        result = result
            .checked_mul(n - i)
            .ok_or(MathError::Overflow)?
            .checked_div(i + 1)
            .ok_or(MathError::DivisionByZero)?;
    }

    Ok(result)
}

/// Calculate greatest common divisor.
pub fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Calculate least common multiple.
pub fn lcm(a: u64, b: u64) -> u64 {
    if a == 0 || b == 0 {
        0
    } else {
        a / gcd(a, b) * b
    }
}

/// Check if number is prime.
pub fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }

    let sqrt_n = (n as f64).sqrt() as u64;
    for i in (3..=sqrt_n).step_by(2) {
        if n % i == 0 {
            return false;
        }
    }
    true
}

/// Calculate power with modulo.
pub fn mod_pow(base: u64, exp: u64, modulo: u64) -> u64 {
    if modulo == 1 {
        return 0;
    }

    let mut result = 1u64;
    let mut base = base % modulo;
    let mut exp = exp;

    while exp > 0 {
        if exp % 2 == 1 {
            result = result * base % modulo;
        }
        exp /= 2;
        base = base * base % modulo;
    }

    result
}

/// Calculate nth Fibonacci number.
pub fn fibonacci(n: u64) -> Result<u64> {
    if n > 93 {
        return Err(MathError::Overflow);
    }

    if n <= 1 {
        return Ok(n);
    }

    let mut a = 0u64;
    let mut b = 1u64;

    for _ in 2..=n {
        let temp = a.checked_add(b).ok_or(MathError::Overflow)?;
        a = b;
        b = temp;
    }

    Ok(b)
}

/// Round to specified decimal places.
pub fn round_to(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
}

/// Floor to specified decimal places.
pub fn floor_to(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).floor() / factor
}

/// Ceil to specified decimal places.
pub fn ceil_to(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).ceil() / factor
}

/// Convert degrees to radians.
pub fn deg_to_rad(degrees: f64) -> f64 {
    degrees * std::f64::consts::PI / 180.0
}

/// Convert radians to degrees.
pub fn rad_to_deg(radians: f64) -> f64 {
    radians * 180.0 / std::f64::consts::PI
}

/// Calculate hypotenuse (Euclidean distance).
pub fn hypot(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}

/// Calculate distance between two points.
pub fn distance(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    hypot(x2 - x1, y2 - y1)
}

/// Wrap angle to [0, 2π).
pub fn wrap_angle(angle: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    ((angle % two_pi) + two_pi) % two_pi
}

/// Normalize angle to [-π, π).
pub fn normalize_angle(angle: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let two_pi = 2.0 * pi;
    let mut a = ((angle % two_pi) + two_pi) % two_pi;
    if a >= pi {
        a -= two_pi;
    }
    a
}

/// Sign function (-1, 0, or 1).
pub fn sign(x: f64) -> i32 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

/// Check if two floats are approximately equal.
pub fn approx_eq(a: f64, b: f64, epsilon: f64) -> bool {
    (a - b).abs() < epsilon
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clamp() {
        assert_eq!(clamp(5, 0, 10), 5);
        assert_eq!(clamp(-5, 0, 10), 0);
        assert_eq!(clamp(15, 0, 10), 10);
    }

    #[test]
    fn test_lerp() {
        assert!((lerp(0.0, 10.0, 0.5) - 5.0).abs() < 1e-10);
        assert!((lerp(0.0, 10.0, 0.0) - 0.0).abs() < 1e-10);
        assert!((lerp(0.0, 10.0, 1.0) - 10.0).abs() < 1e-10);
    }

    #[test]
    fn test_factorial() {
        assert_eq!(factorial(0).unwrap(), 1);
        assert_eq!(factorial(5).unwrap(), 120);
        assert_eq!(factorial(10).unwrap(), 3628800);
    }

    #[test]
    fn test_binomial() {
        assert_eq!(binomial(5, 2).unwrap(), 10);
        assert_eq!(binomial(10, 5).unwrap(), 252);
    }

    #[test]
    fn test_gcd_lcm() {
        assert_eq!(gcd(12, 8), 4);
        assert_eq!(lcm(12, 8), 24);
    }

    #[test]
    fn test_is_prime() {
        assert!(!is_prime(0));
        assert!(!is_prime(1));
        assert!(is_prime(2));
        assert!(is_prime(17));
        assert!(!is_prime(18));
    }

    #[test]
    fn test_fibonacci() {
        assert_eq!(fibonacci(0).unwrap(), 0);
        assert_eq!(fibonacci(1).unwrap(), 1);
        assert_eq!(fibonacci(10).unwrap(), 55);
    }

    #[test]
    fn test_round_to() {
        assert!((round_to(3.14159, 2) - 3.14).abs() < 1e-10);
        assert!((round_to(3.14159, 4) - 3.1416).abs() < 1e-10);
    }

    #[test]
    fn test_angle_conversions() {
        assert!((deg_to_rad(180.0) - std::f64::consts::PI).abs() < 1e-10);
        assert!((rad_to_deg(std::f64::consts::PI) - 180.0).abs() < 1e-10);
    }
}

// ============================================================================
// Kani Formal Verification Proofs
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    // ========================================================================
    // clamp() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_clamp_within_bounds() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let result = clamp(value, min, max);
        kani::assert(result >= min && result <= max, "clamp within bounds");
    }

    #[kani::proof]
    fn proof_clamp_preserves_in_range() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max && value >= min && value <= max);

        let result = clamp(value, min, max);
        kani::assert(result == value, "clamp preserves in-range value");
    }

    #[kani::proof]
    fn proof_clamp_below_min() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max && value < min);

        let result = clamp(value, min, max);
        kani::assert(result == min, "clamp below min returns min");
    }

    #[kani::proof]
    fn proof_clamp_above_max() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max && value > max);

        let result = clamp(value, min, max);
        kani::assert(result == max, "clamp above max returns max");
    }

    #[kani::proof]
    fn proof_clamp_idempotent() {
        let value: i8 = kani::any();
        let min: i8 = kani::any();
        let max: i8 = kani::any();
        kani::assume(min <= max);

        let result = clamp(clamp(value, min, max), min, max);
        kani::assert(result == clamp(value, min, max), "clamp is idempotent");
    }

    // ========================================================================
    // gcd() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_gcd_zero_b() {
        let a: u8 = kani::any();
        kani::assume(a > 0);

        let result = gcd(a as u64, 0);
        kani::assert(result == a as u64, "gcd(a, 0) == a");
    }

    #[kani::proof]
    fn proof_gcd_commutative() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a > 0 && b > 0 && a < 100 && b < 100);

        kani::assert(
            gcd(a as u64, b as u64) == gcd(b as u64, a as u64),
            "gcd is commutative",
        );
    }

    #[kani::proof]
    fn proof_gcd_divides_both() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a > 0 && b > 0 && a < 50 && b < 50);

        let g = gcd(a as u64, b as u64);
        kani::assert(a as u64 % g == 0 && b as u64 % g == 0, "gcd divides both");
    }

    #[kani::proof]
    fn proof_gcd_same_value() {
        let a: u8 = kani::any();
        kani::assume(a > 0);

        let result = gcd(a as u64, a as u64);
        kani::assert(result == a as u64, "gcd(a, a) == a");
    }

    // ========================================================================
    // lcm() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_lcm_zero() {
        let a: u8 = kani::any();
        kani::assert(lcm(a as u64, 0) == 0, "lcm(a, 0) == 0");
        kani::assert(lcm(0, a as u64) == 0, "lcm(0, a) == 0");
    }

    #[kani::proof]
    fn proof_lcm_commutative() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a > 0 && b > 0 && a < 50 && b < 50);

        kani::assert(
            lcm(a as u64, b as u64) == lcm(b as u64, a as u64),
            "lcm is commutative",
        );
    }

    #[kani::proof]
    fn proof_lcm_divisible_by_both() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a > 0 && b > 0 && a < 30 && b < 30);

        let l = lcm(a as u64, b as u64);
        kani::assert(
            l % (a as u64) == 0 && l % (b as u64) == 0,
            "lcm divisible by both",
        );
    }

    #[kani::proof]
    fn proof_lcm_same_value() {
        let a: u8 = kani::any();
        kani::assume(a > 0);

        let result = lcm(a as u64, a as u64);
        kani::assert(result == a as u64, "lcm(a, a) == a");
    }

    // ========================================================================
    // gcd and lcm relationship
    // ========================================================================

    #[kani::proof]
    fn proof_gcd_lcm_product() {
        let a: u8 = kani::any();
        let b: u8 = kani::any();
        kani::assume(a > 0 && b > 0 && a < 20 && b < 20);

        let g = gcd(a as u64, b as u64);
        let l = lcm(a as u64, b as u64);

        // a * b == gcd(a, b) * lcm(a, b)
        kani::assert((a as u64) * (b as u64) == g * l, "a*b == gcd*lcm");
    }

    // ========================================================================
    // factorial() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_factorial_zero() {
        let result = factorial(0);
        kani::assert(result.is_ok() && result.unwrap() == 1, "0! == 1");
    }

    #[kani::proof]
    fn proof_factorial_one() {
        let result = factorial(1);
        kani::assert(result.is_ok() && result.unwrap() == 1, "1! == 1");
    }

    #[kani::proof]
    fn proof_factorial_small() {
        let result = factorial(5);
        kani::assert(result.is_ok() && result.unwrap() == 120, "5! == 120");
    }

    #[kani::proof]
    fn proof_factorial_overflow_limit() {
        let result = factorial(21);
        kani::assert(result.is_err(), "21! overflows");
    }

    // ========================================================================
    // binomial() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_binomial_k_greater_n() {
        let n: u8 = kani::any();
        let k: u8 = kani::any();
        kani::assume(k > n && n < 50);

        let result = binomial(n as u64, k as u64);
        kani::assert(
            result.is_ok() && result.unwrap() == 0,
            "C(n,k) == 0 when k > n",
        );
    }

    #[kani::proof]
    fn proof_binomial_k_zero() {
        let n: u8 = kani::any();
        kani::assume(n < 50);

        let result = binomial(n as u64, 0);
        kani::assert(result.is_ok() && result.unwrap() == 1, "C(n,0) == 1");
    }

    #[kani::proof]
    fn proof_binomial_k_equals_n() {
        let n: u8 = kani::any();
        kani::assume(n < 50);

        let result = binomial(n as u64, n as u64);
        kani::assert(result.is_ok() && result.unwrap() == 1, "C(n,n) == 1");
    }

    #[kani::proof]
    fn proof_binomial_symmetry() {
        let n: u8 = kani::any();
        let k: u8 = kani::any();
        kani::assume(n > 0 && k <= n && n < 20);

        let result1 = binomial(n as u64, k as u64);
        let result2 = binomial(n as u64, (n - k) as u64);

        kani::assert(result1.is_ok() && result2.is_ok(), "both must succeed");
        kani::assert(result1.unwrap() == result2.unwrap(), "C(n,k) == C(n,n-k)");
    }

    // ========================================================================
    // is_prime() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_is_prime_zero_one() {
        kani::assert(!is_prime(0), "0 is not prime");
        kani::assert(!is_prime(1), "1 is not prime");
    }

    #[kani::proof]
    fn proof_is_prime_two() {
        kani::assert(is_prime(2), "2 is prime");
    }

    #[kani::proof]
    fn proof_is_prime_small_primes() {
        kani::assert(is_prime(3), "3 is prime");
        kani::assert(is_prime(5), "5 is prime");
        kani::assert(is_prime(7), "7 is prime");
        kani::assert(is_prime(11), "11 is prime");
        kani::assert(is_prime(13), "13 is prime");
    }

    #[kani::proof]
    fn proof_is_prime_composites() {
        kani::assert(!is_prime(4), "4 is not prime");
        kani::assert(!is_prime(6), "6 is not prime");
        kani::assert(!is_prime(9), "9 is not prime");
        kani::assert(!is_prime(15), "15 is not prime");
    }

    // ========================================================================
    // fibonacci() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_fibonacci_base_cases() {
        let f0 = fibonacci(0);
        let f1 = fibonacci(1);

        kani::assert(f0.is_ok() && f0.unwrap() == 0, "F(0) == 0");
        kani::assert(f1.is_ok() && f1.unwrap() == 1, "F(1) == 1");
    }

    #[kani::proof]
    fn proof_fibonacci_small() {
        let f10 = fibonacci(10);
        kani::assert(f10.is_ok() && f10.unwrap() == 55, "F(10) == 55");
    }

    #[kani::proof]
    fn proof_fibonacci_overflow_limit() {
        let result = fibonacci(94);
        kani::assert(result.is_err(), "F(94) overflows");
    }

    // ========================================================================
    // mod_pow() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_mod_pow_modulo_one() {
        let base: u8 = kani::any();
        let exp: u8 = kani::any();

        let result = mod_pow(base as u64, exp as u64, 1);
        kani::assert(result == 0, "x^n mod 1 == 0");
    }

    #[kani::proof]
    fn proof_mod_pow_exp_zero() {
        let base: u8 = kani::any();
        let modulo: u8 = kani::any();
        kani::assume(modulo > 1);

        let result = mod_pow(base as u64, 0, modulo as u64);
        kani::assert(result == 1, "x^0 mod m == 1");
    }

    #[kani::proof]
    fn proof_mod_pow_exp_one() {
        let base: u8 = kani::any();
        let modulo: u8 = kani::any();
        kani::assume(modulo > 1);

        let result = mod_pow(base as u64, 1, modulo as u64);
        kani::assert(
            result == (base as u64) % (modulo as u64),
            "x^1 mod m == x mod m",
        );
    }

    #[kani::proof]
    fn proof_mod_pow_result_bounded() {
        let base: u8 = kani::any();
        let exp: u8 = kani::any();
        let modulo: u8 = kani::any();
        kani::assume(modulo > 1);

        let result = mod_pow(base as u64, exp as u64, modulo as u64);
        kani::assert(result < modulo as u64, "result < modulo");
    }

    // ========================================================================
    // sign() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_sign_positive() {
        kani::assert(sign(1.0) == 1, "sign(positive) == 1");
        kani::assert(sign(100.0) == 1, "sign(100) == 1");
    }

    #[kani::proof]
    fn proof_sign_negative() {
        kani::assert(sign(-1.0) == -1, "sign(negative) == -1");
        kani::assert(sign(-100.0) == -1, "sign(-100) == -1");
    }

    #[kani::proof]
    fn proof_sign_zero() {
        kani::assert(sign(0.0) == 0, "sign(0) == 0");
    }

    // ========================================================================
    // approx_eq() Function Proofs
    // ========================================================================

    #[kani::proof]
    fn proof_approx_eq_same() {
        kani::assert(approx_eq(1.0, 1.0, 0.001), "same values are approx_eq");
    }

    #[kani::proof]
    fn proof_approx_eq_within_epsilon() {
        kani::assert(
            approx_eq(1.0, 1.0001, 0.001),
            "within epsilon are approx_eq",
        );
    }

    #[kani::proof]
    fn proof_approx_eq_outside_epsilon() {
        kani::assert(
            !approx_eq(1.0, 1.01, 0.001),
            "outside epsilon not approx_eq",
        );
    }

    #[kani::proof]
    fn proof_approx_eq_symmetric() {
        let a = 1.0;
        let b = 1.0001;
        let eps = 0.001;

        kani::assert(
            approx_eq(a, b, eps) == approx_eq(b, a, eps),
            "approx_eq is symmetric",
        );
    }
}
