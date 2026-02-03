//! Fuzzing utilities for drbot testing.
//!
//! This crate provides:
//! - Random data generation
//! - Property-based testing support
//! - Fuzzing strategies
//! - Shrinking support

use std::ops::Range;
use thiserror::Error;

/// Fuzzer error types.
#[derive(Error, Debug)]
pub enum FuzzerError {
    #[error("Generation failed: {0}")]
    GenerationFailed(String),

    #[error("Shrink failed")]
    ShrinkFailed,

    #[error("Invalid range")]
    InvalidRange,
}

/// Result type for fuzzer operations.
pub type Result<T> = std::result::Result<T, FuzzerError>;

/// Random number generator using xorshift.
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Create new RNG with seed.
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    /// Create from current time.
    pub fn from_time() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        Self::new(seed)
    }

    /// Get next u64.
    pub fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// Get next u32.
    pub fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    /// Get next f64 in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Get random bool.
    pub fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    /// Get random in range.
    pub fn range(&mut self, range: Range<i64>) -> i64 {
        if range.is_empty() {
            return range.start;
        }
        let len = (range.end - range.start) as u64;
        range.start + (self.next_u64() % len) as i64
    }

    /// Get random usize in range.
    pub fn range_usize(&mut self, range: Range<usize>) -> usize {
        if range.is_empty() {
            return range.start;
        }
        let len = range.end - range.start;
        range.start + (self.next_u64() as usize % len)
    }

    /// Choose random element.
    pub fn choose<'a, T>(&mut self, slice: &'a [T]) -> Option<&'a T> {
        if slice.is_empty() {
            None
        } else {
            Some(&slice[self.range_usize(0..slice.len())])
        }
    }

    /// Shuffle slice.
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        for i in (1..slice.len()).rev() {
            let j = self.range_usize(0..i + 1);
            slice.swap(i, j);
        }
    }
}

impl Default for Rng {
    fn default() -> Self {
        Self::from_time()
    }
}

/// Generator trait.
pub trait Generator {
    /// Generated type.
    type Output;

    /// Generate a value.
    fn generate(&self, rng: &mut Rng) -> Self::Output;

    /// Generate multiple values.
    fn generate_many(&self, rng: &mut Rng, count: usize) -> Vec<Self::Output> {
        (0..count).map(|_| self.generate(rng)).collect()
    }
}

/// Integer generator.
pub struct IntGen {
    min: i64,
    max: i64,
}

impl IntGen {
    /// Create new integer generator.
    pub fn new(min: i64, max: i64) -> Self {
        Self { min, max }
    }

    /// Create for full range.
    pub fn any() -> Self {
        Self::new(i64::MIN, i64::MAX)
    }

    /// Create for positive.
    pub fn positive() -> Self {
        Self::new(1, i64::MAX)
    }

    /// Create for non-negative.
    pub fn non_negative() -> Self {
        Self::new(0, i64::MAX)
    }
}

impl Generator for IntGen {
    type Output = i64;

    fn generate(&self, rng: &mut Rng) -> i64 {
        rng.range(self.min..self.max.saturating_add(1))
    }
}

/// Float generator.
pub struct FloatGen {
    min: f64,
    max: f64,
}

impl FloatGen {
    /// Create new float generator.
    pub fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }

    /// Create for unit interval.
    pub fn unit() -> Self {
        Self::new(0.0, 1.0)
    }
}

impl Generator for FloatGen {
    type Output = f64;

    fn generate(&self, rng: &mut Rng) -> f64 {
        self.min + rng.next_f64() * (self.max - self.min)
    }
}

/// String generator.
pub struct StringGen {
    min_len: usize,
    max_len: usize,
    charset: Vec<char>,
}

impl StringGen {
    /// Create new string generator.
    pub fn new(min_len: usize, max_len: usize) -> Self {
        Self {
            min_len,
            max_len,
            charset: ('a'..='z').collect(),
        }
    }

    /// Set charset.
    pub fn charset(mut self, chars: impl IntoIterator<Item = char>) -> Self {
        self.charset = chars.into_iter().collect();
        self
    }

    /// Alphanumeric charset.
    pub fn alphanumeric(self) -> Self {
        self.charset(('a'..='z').chain('A'..='Z').chain('0'..='9'))
    }

    /// ASCII charset.
    pub fn ascii(self) -> Self {
        self.charset((32u8..127).map(|b| b as char))
    }
}

impl Generator for StringGen {
    type Output = String;

    fn generate(&self, rng: &mut Rng) -> String {
        let len = rng.range_usize(self.min_len..self.max_len + 1);
        (0..len)
            .map(|_| *rng.choose(&self.charset).unwrap_or(&'a'))
            .collect()
    }
}

/// Boolean generator.
pub struct BoolGen;

impl Generator for BoolGen {
    type Output = bool;

    fn generate(&self, rng: &mut Rng) -> bool {
        rng.next_bool()
    }
}

/// Optional generator.
pub struct OptionGen<G> {
    inner: G,
    none_probability: f64,
}

impl<G: Generator> OptionGen<G> {
    /// Create new option generator.
    pub fn new(inner: G) -> Self {
        Self {
            inner,
            none_probability: 0.1,
        }
    }

    /// Set None probability.
    pub fn none_probability(mut self, prob: f64) -> Self {
        self.none_probability = prob;
        self
    }
}

impl<G: Generator> Generator for OptionGen<G> {
    type Output = Option<G::Output>;

    fn generate(&self, rng: &mut Rng) -> Option<G::Output> {
        if rng.next_f64() < self.none_probability {
            None
        } else {
            Some(self.inner.generate(rng))
        }
    }
}

/// Vector generator.
pub struct VecGen<G> {
    inner: G,
    min_len: usize,
    max_len: usize,
}

impl<G: Generator> VecGen<G> {
    /// Create new vector generator.
    pub fn new(inner: G, min_len: usize, max_len: usize) -> Self {
        Self {
            inner,
            min_len,
            max_len,
        }
    }
}

impl<G: Generator> Generator for VecGen<G> {
    type Output = Vec<G::Output>;

    fn generate(&self, rng: &mut Rng) -> Vec<G::Output> {
        let len = rng.range_usize(self.min_len..self.max_len + 1);
        (0..len).map(|_| self.inner.generate(rng)).collect()
    }
}

/// One-of generator.
pub struct OneOf<T> {
    values: Vec<T>,
}

impl<T: Clone> OneOf<T> {
    /// Create new one-of generator.
    pub fn new(values: Vec<T>) -> Self {
        Self { values }
    }
}

impl<T: Clone> Generator for OneOf<T> {
    type Output = T;

    fn generate(&self, rng: &mut Rng) -> T {
        rng.choose(&self.values).unwrap().clone()
    }
}

/// Shrinkable trait.
pub trait Shrinkable: Sized {
    /// Generate shrink candidates.
    fn shrink(&self) -> Box<dyn Iterator<Item = Self>>;
}

impl Shrinkable for i64 {
    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let value = *self;
        if value == 0 {
            Box::new(std::iter::empty())
        } else {
            Box::new(
                std::iter::once(0)
                    .chain(std::iter::once(value / 2))
                    .chain(std::iter::once(value - value.signum()))
                    .filter(move |&v| v.abs() < value.abs()),
            )
        }
    }
}

impl Shrinkable for String {
    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let s = self.clone();
        if s.is_empty() {
            Box::new(std::iter::empty())
        } else {
            Box::new(
                std::iter::once(String::new()).chain((0..s.len()).map(move |i| {
                    let mut new_s = s.clone();
                    new_s.remove(i);
                    new_s
                })),
            )
        }
    }
}

impl<T: Clone + Shrinkable + 'static> Shrinkable for Vec<T> {
    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        let v = self.clone();
        if v.is_empty() {
            Box::new(std::iter::empty())
        } else {
            Box::new(std::iter::once(vec![]).chain((0..v.len()).map(move |i| {
                let mut new_v = v.clone();
                new_v.remove(i);
                new_v
            })))
        }
    }
}

/// Property test runner.
pub struct PropertyTest<G: Generator> {
    generator: G,
    iterations: usize,
    seed: Option<u64>,
}

impl<G: Generator> PropertyTest<G> {
    /// Create new property test.
    pub fn new(generator: G) -> Self {
        Self {
            generator,
            iterations: 100,
            seed: None,
        }
    }

    /// Set iterations.
    pub fn iterations(mut self, n: usize) -> Self {
        self.iterations = n;
        self
    }

    /// Set seed.
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Run property test.
    pub fn check<F>(self, property: F) -> PropertyResult<G::Output>
    where
        F: Fn(&G::Output) -> bool,
        G::Output: Clone + std::fmt::Debug,
    {
        let mut rng = match self.seed {
            Some(s) => Rng::new(s),
            None => Rng::from_time(),
        };

        for i in 0..self.iterations {
            let value = self.generator.generate(&mut rng);
            if !property(&value) {
                return PropertyResult::Failed {
                    iteration: i,
                    counterexample: value,
                };
            }
        }

        PropertyResult::Passed {
            iterations: self.iterations,
        }
    }
}

/// Property test result.
#[derive(Debug)]
pub enum PropertyResult<T> {
    /// Test passed.
    Passed { iterations: usize },
    /// Test failed.
    Failed { iteration: usize, counterexample: T },
}

impl<T> PropertyResult<T> {
    /// Check if passed.
    pub fn is_passed(&self) -> bool {
        matches!(self, PropertyResult::Passed { .. })
    }

    /// Check if failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, PropertyResult::Failed { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rng() {
        let mut rng = Rng::new(42);
        let a = rng.next_u64();
        let b = rng.next_u64();
        assert_ne!(a, b);
    }

    #[test]
    fn test_int_gen() {
        let gen = IntGen::new(0, 100);
        let mut rng = Rng::new(42);

        for _ in 0..100 {
            let v = gen.generate(&mut rng);
            assert!(v >= 0 && v <= 100);
        }
    }

    #[test]
    fn test_string_gen() {
        let gen = StringGen::new(5, 10);
        let mut rng = Rng::new(42);

        for _ in 0..100 {
            let s = gen.generate(&mut rng);
            assert!(s.len() >= 5 && s.len() <= 10);
        }
    }

    #[test]
    fn test_property_pass() {
        let result = PropertyTest::new(IntGen::new(0, 1000))
            .seed(42)
            .iterations(100)
            .check(|&x| x >= 0);

        assert!(result.is_passed());
    }

    #[test]
    fn test_property_fail() {
        let result = PropertyTest::new(IntGen::new(-10, 10))
            .seed(42)
            .iterations(100)
            .check(|&x| x > 0);

        assert!(result.is_failed());
    }

    #[test]
    fn test_shrink_i64() {
        let value = 100i64;
        let shrinks: Vec<_> = value.shrink().collect();
        assert!(shrinks.contains(&0));
        assert!(shrinks.contains(&50));
    }
}
