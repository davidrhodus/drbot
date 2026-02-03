//! Pipe and pipeline utilities for drbot.
//!
//! This crate provides:
//! - Pipeline composition
//! - Transform stages
//! - Data flow utilities

use std::marker::PhantomData;
use thiserror::Error;

/// Pipe error types.
#[derive(Error, Debug, Clone)]
pub enum PipeError {
    #[error("Pipeline error: {0}")]
    Pipeline(String),

    #[error("Stage error at {stage}: {message}")]
    Stage { stage: String, message: String },

    #[error("Invalid input")]
    InvalidInput,
}

/// Result type for pipe operations.
pub type Result<T> = std::result::Result<T, PipeError>;

/// Transform trait for pipeline stages.
pub trait Transform<In, Out> {
    /// Transform input to output.
    fn transform(&self, input: In) -> Result<Out>;
}

/// Function-based transform.
pub struct FnTransform<F, In, Out> {
    func: F,
    _phantom: PhantomData<(In, Out)>,
}

impl<F, In, Out> FnTransform<F, In, Out>
where
    F: Fn(In) -> Result<Out>,
{
    /// Create new function transform.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _phantom: PhantomData,
        }
    }
}

impl<F, In, Out> Transform<In, Out> for FnTransform<F, In, Out>
where
    F: Fn(In) -> Result<Out>,
{
    fn transform(&self, input: In) -> Result<Out> {
        (self.func)(input)
    }
}

/// Identity transform.
pub struct Identity<T>(PhantomData<T>);

impl<T> Identity<T> {
    /// Create new identity transform.
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T> Default for Identity<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Transform<T, T> for Identity<T> {
    fn transform(&self, input: T) -> Result<T> {
        Ok(input)
    }
}

/// Map transform.
pub struct Map<F, T, U> {
    func: F,
    _phantom: PhantomData<(T, U)>,
}

impl<F, T, U> Map<F, T, U>
where
    F: Fn(T) -> U,
{
    /// Create new map transform.
    pub fn new(func: F) -> Self {
        Self {
            func,
            _phantom: PhantomData,
        }
    }
}

impl<F, T, U> Transform<T, U> for Map<F, T, U>
where
    F: Fn(T) -> U,
{
    fn transform(&self, input: T) -> Result<U> {
        Ok((self.func)(input))
    }
}

/// Filter transform.
pub struct Filter<F, T> {
    predicate: F,
    _phantom: PhantomData<T>,
}

impl<F, T> Filter<F, T>
where
    F: Fn(&T) -> bool,
{
    /// Create new filter transform.
    pub fn new(predicate: F) -> Self {
        Self {
            predicate,
            _phantom: PhantomData,
        }
    }
}

impl<F, T> Transform<T, Option<T>> for Filter<F, T>
where
    F: Fn(&T) -> bool,
{
    fn transform(&self, input: T) -> Result<Option<T>> {
        if (self.predicate)(&input) {
            Ok(Some(input))
        } else {
            Ok(None)
        }
    }
}

/// Composed pipeline of two transforms.
pub struct Compose<T1, T2, A, B, C> {
    first: T1,
    second: T2,
    _phantom: PhantomData<(A, B, C)>,
}

impl<T1, T2, A, B, C> Compose<T1, T2, A, B, C>
where
    T1: Transform<A, B>,
    T2: Transform<B, C>,
{
    /// Create new composed transform.
    pub fn new(first: T1, second: T2) -> Self {
        Self {
            first,
            second,
            _phantom: PhantomData,
        }
    }
}

impl<T1, T2, A, B, C> Transform<A, C> for Compose<T1, T2, A, B, C>
where
    T1: Transform<A, B>,
    T2: Transform<B, C>,
{
    fn transform(&self, input: A) -> Result<C> {
        let intermediate = self.first.transform(input)?;
        self.second.transform(intermediate)
    }
}

/// Pipeline builder.
pub struct Pipeline<T, In, Out> {
    transform: T,
    _phantom: PhantomData<(In, Out)>,
}

impl<In> Pipeline<Identity<In>, In, In> {
    /// Create new empty pipeline.
    pub fn new() -> Self {
        Self {
            transform: Identity::new(),
            _phantom: PhantomData,
        }
    }
}

impl<In> Default for Pipeline<Identity<In>, In, In> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, In, Out> Pipeline<T, In, Out>
where
    T: Transform<In, Out>,
{
    /// Add transform stage.
    pub fn then<T2, Next>(self, next: T2) -> Pipeline<Compose<T, T2, In, Out, Next>, In, Next>
    where
        T2: Transform<Out, Next>,
    {
        Pipeline {
            transform: Compose::new(self.transform, next),
            _phantom: PhantomData,
        }
    }

    /// Add map stage.
    pub fn map<F, Next>(
        self,
        func: F,
    ) -> Pipeline<Compose<T, Map<F, Out, Next>, In, Out, Next>, In, Next>
    where
        F: Fn(Out) -> Next,
    {
        self.then(Map::new(func))
    }

    /// Execute pipeline.
    pub fn execute(&self, input: In) -> Result<Out> {
        self.transform.transform(input)
    }
}

/// Simple data pipe between producer and consumer.
pub struct DataPipe<T> {
    buffer: Vec<T>,
    closed: bool,
}

impl<T> DataPipe<T> {
    /// Create new data pipe.
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            closed: false,
        }
    }

    /// Write data to pipe.
    pub fn write(&mut self, item: T) -> Result<()> {
        if self.closed {
            return Err(PipeError::Pipeline("Pipe closed".into()));
        }
        self.buffer.push(item);
        Ok(())
    }

    /// Read data from pipe.
    pub fn read(&mut self) -> Option<T> {
        if !self.buffer.is_empty() {
            Some(self.buffer.remove(0))
        } else {
            None
        }
    }

    /// Close pipe.
    pub fn close(&mut self) {
        self.closed = true;
    }

    /// Check if pipe is closed.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Check if pipe has data.
    pub fn has_data(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Drain all data.
    pub fn drain(&mut self) -> Vec<T> {
        std::mem::take(&mut self.buffer)
    }
}

impl<T> Default for DataPipe<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Chain multiple operations.
pub fn chain<T, U, V, F1, F2>(input: T, f1: F1, f2: F2) -> Result<V>
where
    F1: FnOnce(T) -> Result<U>,
    F2: FnOnce(U) -> Result<V>,
{
    let intermediate = f1(input)?;
    f2(intermediate)
}

/// Pipe value through function.
pub fn pipe<T, U, F>(value: T, func: F) -> U
where
    F: FnOnce(T) -> U,
{
    func(value)
}

/// Pipe value through multiple functions.
pub fn pipe2<T, U, V, F1, F2>(value: T, f1: F1, f2: F2) -> V
where
    F1: FnOnce(T) -> U,
    F2: FnOnce(U) -> V,
{
    f2(f1(value))
}

/// Pipe value through multiple functions.
pub fn pipe3<T, U, V, W, F1, F2, F3>(value: T, f1: F1, f2: F2, f3: F3) -> W
where
    F1: FnOnce(T) -> U,
    F2: FnOnce(U) -> V,
    F3: FnOnce(V) -> W,
{
    f3(f2(f1(value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity() {
        let id = Identity::<i32>::new();
        assert_eq!(id.transform(42).unwrap(), 42);
    }

    #[test]
    fn test_map() {
        let map = Map::new(|x: i32| x * 2);
        assert_eq!(map.transform(21).unwrap(), 42);
    }

    #[test]
    fn test_filter() {
        let filter = Filter::new(|x: &i32| *x > 0);
        assert_eq!(filter.transform(5).unwrap(), Some(5));
        assert_eq!(filter.transform(-5).unwrap(), None);
    }

    #[test]
    fn test_compose() {
        let double = Map::new(|x: i32| x * 2);
        let add_one = Map::new(|x: i32| x + 1);
        let composed = Compose::new(double, add_one);

        assert_eq!(composed.transform(5).unwrap(), 11); // (5 * 2) + 1
    }

    #[test]
    fn test_pipeline() {
        let result = Pipeline::new()
            .map(|x: i32| x * 2)
            .map(|x| x + 1)
            .map(|x| x.to_string())
            .execute(5)
            .unwrap();

        assert_eq!(result, "11");
    }

    #[test]
    fn test_data_pipe() {
        let mut pipe = DataPipe::new();

        pipe.write(1).unwrap();
        pipe.write(2).unwrap();

        assert_eq!(pipe.read(), Some(1));
        assert_eq!(pipe.read(), Some(2));
        assert_eq!(pipe.read(), None);
    }

    #[test]
    fn test_pipe_functions() {
        let result = pipe(5, |x| x * 2);
        assert_eq!(result, 10);

        let result = pipe2(5, |x| x * 2, |x| x + 1);
        assert_eq!(result, 11);

        let result = pipe3(5, |x| x * 2, |x| x + 1, |x: i32| x.to_string());
        assert_eq!(result, "11");
    }
}
