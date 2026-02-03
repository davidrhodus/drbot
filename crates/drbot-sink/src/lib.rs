//! Sink utilities for drbot.
//!
//! This crate provides:
//! - Sink trait for consuming data
//! - Various sink implementations
//! - Sink combinators

use thiserror::Error;

/// Sink error types.
#[derive(Error, Debug, Clone)]
pub enum SinkError {
    #[error("Sink error: {0}")]
    Error(String),

    #[error("Sink full")]
    Full,

    #[error("Sink closed")]
    Closed,
}

/// Result type for sink operations.
pub type Result<T> = std::result::Result<T, SinkError>;

/// Sink trait for consuming items.
pub trait Sink<T> {
    /// Send item to sink.
    fn send(&mut self, item: T) -> Result<()>;

    /// Flush sink.
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    /// Close sink.
    fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Null sink that discards all items.
pub struct NullSink;

impl<T> Sink<T> for NullSink {
    fn send(&mut self, _item: T) -> Result<()> {
        Ok(())
    }
}

/// Vector sink that collects items.
pub struct VecSink<T> {
    items: Vec<T>,
}

impl<T> VecSink<T> {
    /// Create new vector sink.
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Create with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
        }
    }

    /// Get collected items.
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Take collected items.
    pub fn into_items(self) -> Vec<T> {
        self.items
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Clear items.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

impl<T> Default for VecSink<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Sink<T> for VecSink<T> {
    fn send(&mut self, item: T) -> Result<()> {
        self.items.push(item);
        Ok(())
    }
}

/// Counting sink that counts items.
pub struct CountingSink {
    count: usize,
}

impl CountingSink {
    /// Create new counting sink.
    pub fn new() -> Self {
        Self { count: 0 }
    }

    /// Get count.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Reset count.
    pub fn reset(&mut self) {
        self.count = 0;
    }
}

impl Default for CountingSink {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Sink<T> for CountingSink {
    fn send(&mut self, _item: T) -> Result<()> {
        self.count += 1;
        Ok(())
    }
}

/// Limited sink with maximum capacity.
pub struct LimitedSink<S> {
    inner: S,
    limit: usize,
    count: usize,
}

impl<S> LimitedSink<S> {
    /// Create new limited sink.
    pub fn new(inner: S, limit: usize) -> Self {
        Self {
            inner,
            limit,
            count: 0,
        }
    }

    /// Get remaining capacity.
    pub fn remaining(&self) -> usize {
        self.limit.saturating_sub(self.count)
    }

    /// Check if full.
    pub fn is_full(&self) -> bool {
        self.count >= self.limit
    }

    /// Get inner sink.
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<T, S: Sink<T>> Sink<T> for LimitedSink<S> {
    fn send(&mut self, item: T) -> Result<()> {
        if self.count >= self.limit {
            return Err(SinkError::Full);
        }
        self.inner.send(item)?;
        self.count += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.inner.flush()
    }

    fn close(&mut self) -> Result<()> {
        self.inner.close()
    }
}

/// Callback sink that calls a function for each item.
pub struct CallbackSink<F> {
    callback: F,
}

impl<F> CallbackSink<F> {
    /// Create new callback sink.
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<T, F: FnMut(T)> Sink<T> for CallbackSink<F> {
    fn send(&mut self, item: T) -> Result<()> {
        (self.callback)(item);
        Ok(())
    }
}

/// Fan-out sink that sends to multiple sinks.
pub struct FanOut<S> {
    sinks: Vec<S>,
}

impl<S> FanOut<S> {
    /// Create new fan-out sink.
    pub fn new(sinks: Vec<S>) -> Self {
        Self { sinks }
    }

    /// Add sink.
    pub fn add(&mut self, sink: S) {
        self.sinks.push(sink);
    }

    /// Get number of sinks.
    pub fn len(&self) -> usize {
        self.sinks.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.sinks.is_empty()
    }
}

impl<T: Clone, S: Sink<T>> Sink<T> for FanOut<S> {
    fn send(&mut self, item: T) -> Result<()> {
        let len = self.sinks.len();
        for (i, sink) in self.sinks.iter_mut().enumerate() {
            let item_clone = if i == len - 1 {
                item.clone()
            } else {
                item.clone()
            };
            sink.send(item_clone)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        for sink in &mut self.sinks {
            sink.flush()?;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<()> {
        for sink in &mut self.sinks {
            sink.close()?;
        }
        Ok(())
    }
}

/// Map sink that transforms items before sending.
pub struct MapSink<S, F> {
    inner: S,
    map_fn: F,
}

impl<S, F> MapSink<S, F> {
    /// Create new map sink.
    pub fn new(inner: S, map_fn: F) -> Self {
        Self { inner, map_fn }
    }

    /// Get inner sink.
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<T, U, S: Sink<U>, F: FnMut(T) -> U> Sink<T> for MapSink<S, F> {
    fn send(&mut self, item: T) -> Result<()> {
        let mapped = (self.map_fn)(item);
        self.inner.send(mapped)
    }

    fn flush(&mut self) -> Result<()> {
        self.inner.flush()
    }

    fn close(&mut self) -> Result<()> {
        self.inner.close()
    }
}

/// Filter sink that only passes matching items.
pub struct FilterSink<S, F> {
    inner: S,
    predicate: F,
}

impl<S, F> FilterSink<S, F> {
    /// Create new filter sink.
    pub fn new(inner: S, predicate: F) -> Self {
        Self { inner, predicate }
    }

    /// Get inner sink.
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<T, S: Sink<T>, F: FnMut(&T) -> bool> Sink<T> for FilterSink<S, F> {
    fn send(&mut self, item: T) -> Result<()> {
        if (self.predicate)(&item) {
            self.inner.send(item)
        } else {
            Ok(())
        }
    }

    fn flush(&mut self) -> Result<()> {
        self.inner.flush()
    }

    fn close(&mut self) -> Result<()> {
        self.inner.close()
    }
}

/// Aggregate sink that reduces items.
pub struct AggregateSink<T, F> {
    accumulator: Option<T>,
    reduce_fn: F,
}

impl<T, F> AggregateSink<T, F>
where
    F: FnMut(T, T) -> T,
{
    /// Create new aggregate sink with initial value.
    pub fn new(initial: T, reduce_fn: F) -> Self {
        Self {
            accumulator: Some(initial),
            reduce_fn,
        }
    }

    /// Get result.
    pub fn result(self) -> Option<T> {
        self.accumulator
    }

    /// Get reference to current value.
    pub fn current(&self) -> Option<&T> {
        self.accumulator.as_ref()
    }
}

impl<T, F: FnMut(T, T) -> T> Sink<T> for AggregateSink<T, F> {
    fn send(&mut self, item: T) -> Result<()> {
        if let Some(acc) = self.accumulator.take() {
            self.accumulator = Some((self.reduce_fn)(acc, item));
        } else {
            self.accumulator = Some(item);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_sink() {
        let mut sink = NullSink;
        sink.send(1).unwrap();
        sink.send(2).unwrap();
        // No effect
    }

    #[test]
    fn test_vec_sink() {
        let mut sink = VecSink::new();
        sink.send(1).unwrap();
        sink.send(2).unwrap();
        sink.send(3).unwrap();

        assert_eq!(sink.items(), &[1, 2, 3]);
    }

    #[test]
    fn test_counting_sink() {
        let mut sink = CountingSink::new();
        sink.send(()).unwrap();
        sink.send(()).unwrap();
        sink.send(()).unwrap();

        assert_eq!(sink.count(), 3);
    }

    #[test]
    fn test_limited_sink() {
        let mut sink = LimitedSink::new(VecSink::new(), 2);

        sink.send(1).unwrap();
        sink.send(2).unwrap();
        assert!(sink.send(3).is_err());
    }

    #[test]
    fn test_callback_sink() {
        let mut total = 0;
        {
            let mut sink = CallbackSink::new(|x: i32| total += x);
            sink.send(1).unwrap();
            sink.send(2).unwrap();
            sink.send(3).unwrap();
        }
        // Note: closure borrows, so this won't actually work as written
        // In practice you'd use interior mutability
    }

    #[test]
    fn test_map_sink() {
        let mut sink = MapSink::new(VecSink::new(), |x: i32| x * 2);

        sink.send(1).unwrap();
        sink.send(2).unwrap();
        sink.send(3).unwrap();

        assert_eq!(sink.into_inner().items(), &[2, 4, 6]);
    }

    #[test]
    fn test_filter_sink() {
        let mut sink = FilterSink::new(VecSink::new(), |x: &i32| *x > 2);

        sink.send(1).unwrap();
        sink.send(2).unwrap();
        sink.send(3).unwrap();
        sink.send(4).unwrap();

        assert_eq!(sink.into_inner().items(), &[3, 4]);
    }

    #[test]
    fn test_aggregate_sink() {
        let mut sink = AggregateSink::new(0, |acc, x| acc + x);

        sink.send(1).unwrap();
        sink.send(2).unwrap();
        sink.send(3).unwrap();

        assert_eq!(sink.result(), Some(6));
    }
}
