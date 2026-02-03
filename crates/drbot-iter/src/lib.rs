//! Iterator utilities for drbot.
//!
//! This crate provides:
//! - Iterator adapters
//! - Aggregation utilities
//! - Windowing functions

/// Iterator extension trait.
pub trait IteratorExt: Iterator + Sized {
    /// Enumerate starting from n.
    fn enumerate_from(self, start: usize) -> EnumerateFrom<Self> {
        EnumerateFrom {
            iter: self,
            index: start,
        }
    }

    /// Take while including the first failing element.
    fn take_while_inclusive<P>(self, predicate: P) -> TakeWhileInclusive<Self, P>
    where
        P: FnMut(&Self::Item) -> bool,
    {
        TakeWhileInclusive {
            iter: self,
            predicate,
            done: false,
        }
    }

    /// Skip until predicate is true (including that element).
    fn skip_until<P>(self, predicate: P) -> SkipUntil<Self, P>
    where
        P: FnMut(&Self::Item) -> bool,
    {
        SkipUntil {
            iter: self,
            predicate,
            found: false,
        }
    }

    /// Interleave with another iterator.
    fn interleave<J>(self, other: J) -> Interleave<Self, J::IntoIter>
    where
        J: IntoIterator<Item = Self::Item>,
    {
        Interleave {
            a: self,
            b: other.into_iter(),
            flag: false,
        }
    }

    /// Sliding window.
    fn sliding_window(self, size: usize) -> SlidingWindow<Self>
    where
        Self::Item: Clone,
    {
        SlidingWindow {
            iter: self,
            window: Vec::with_capacity(size),
            size,
        }
    }

    /// Pair with next element.
    fn pairs(self) -> Pairs<Self>
    where
        Self::Item: Clone,
    {
        Pairs {
            iter: self,
            prev: None,
        }
    }

    /// Unique elements (first occurrence).
    fn unique(self) -> Unique<Self>
    where
        Self::Item: Clone + std::hash::Hash + Eq,
    {
        Unique {
            iter: self,
            seen: std::collections::HashSet::new(),
        }
    }

    /// Duplicate elements only.
    fn duplicates(self) -> Duplicates<Self>
    where
        Self::Item: Clone + std::hash::Hash + Eq,
    {
        Duplicates {
            iter: self,
            seen: std::collections::HashSet::new(),
            yielded: std::collections::HashSet::new(),
        }
    }

    /// Collect into chunks.
    fn chunks_vec(self, size: usize) -> Vec<Vec<Self::Item>> {
        let mut result = Vec::new();
        let mut current = Vec::with_capacity(size);

        for item in self {
            current.push(item);
            if current.len() == size {
                result.push(current);
                current = Vec::with_capacity(size);
            }
        }

        if !current.is_empty() {
            result.push(current);
        }

        result
    }

    /// Collect into groups by key.
    fn group_by_key<K, F>(self, key_fn: F) -> std::collections::HashMap<K, Vec<Self::Item>>
    where
        K: std::hash::Hash + Eq,
        F: Fn(&Self::Item) -> K,
    {
        let mut groups: std::collections::HashMap<K, Vec<Self::Item>> =
            std::collections::HashMap::new();

        for item in self {
            let key = key_fn(&item);
            groups.entry(key).or_default().push(item);
        }

        groups
    }

    /// Find min and max in one pass.
    fn min_max(self) -> Option<(Self::Item, Self::Item)>
    where
        Self::Item: Ord + Clone,
    {
        let mut iter = self;
        let first = iter.next()?;
        let mut min = first.clone();
        let mut max = first;

        for item in iter {
            if item < min {
                min = item.clone();
            }
            if item > max {
                max = item;
            } else {
                // Only clone if we haven't already
                let _ = item;
            }
        }

        Some((min, max))
    }
}

impl<I: Iterator> IteratorExt for I {}

/// Enumerate starting from n.
pub struct EnumerateFrom<I> {
    iter: I,
    index: usize,
}

impl<I: Iterator> Iterator for EnumerateFrom<I> {
    type Item = (usize, I::Item);

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next()?;
        let index = self.index;
        self.index += 1;
        Some((index, item))
    }
}

/// Take while inclusive.
pub struct TakeWhileInclusive<I, P> {
    iter: I,
    predicate: P,
    done: bool,
}

impl<I: Iterator, P: FnMut(&I::Item) -> bool> Iterator for TakeWhileInclusive<I, P> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let item = self.iter.next()?;
        if !(self.predicate)(&item) {
            self.done = true;
        }
        Some(item)
    }
}

/// Skip until.
pub struct SkipUntil<I, P> {
    iter: I,
    predicate: P,
    found: bool,
}

impl<I: Iterator, P: FnMut(&I::Item) -> bool> Iterator for SkipUntil<I, P> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.found {
            return self.iter.next();
        }

        loop {
            let item = self.iter.next()?;
            if (self.predicate)(&item) {
                self.found = true;
                return Some(item);
            }
        }
    }
}

/// Interleave iterator.
pub struct Interleave<A, B> {
    a: A,
    b: B,
    flag: bool,
}

impl<A, B> Iterator for Interleave<A, B>
where
    A: Iterator,
    B: Iterator<Item = A::Item>,
{
    type Item = A::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.flag = !self.flag;
        if self.flag {
            self.a.next().or_else(|| self.b.next())
        } else {
            self.b.next().or_else(|| self.a.next())
        }
    }
}

/// Sliding window iterator.
pub struct SlidingWindow<I: Iterator> {
    iter: I,
    window: Vec<I::Item>,
    size: usize,
}

impl<I: Iterator> Iterator for SlidingWindow<I>
where
    I::Item: Clone,
{
    type Item = Vec<I::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.window.len() < self.size {
            while self.window.len() < self.size {
                self.window.push(self.iter.next()?);
            }
            return Some(self.window.clone());
        }

        let next = self.iter.next()?;
        self.window.remove(0);
        self.window.push(next);
        Some(self.window.clone())
    }
}

/// Pairs iterator.
pub struct Pairs<I: Iterator> {
    iter: I,
    prev: Option<I::Item>,
}

impl<I: Iterator> Iterator for Pairs<I>
where
    I::Item: Clone,
{
    type Item = (I::Item, I::Item);

    fn next(&mut self) -> Option<Self::Item> {
        if self.prev.is_none() {
            self.prev = self.iter.next();
        }

        let prev = self.prev.take()?;
        let next = self.iter.next()?;
        self.prev = Some(next.clone());
        Some((prev, next))
    }
}

/// Unique iterator.
pub struct Unique<I: Iterator>
where
    I::Item: std::hash::Hash + Eq,
{
    iter: I,
    seen: std::collections::HashSet<I::Item>,
}

impl<I: Iterator> Iterator for Unique<I>
where
    I::Item: Clone + std::hash::Hash + Eq,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iter.next()?;
            if self.seen.insert(item.clone()) {
                return Some(item);
            }
        }
    }
}

/// Duplicates iterator.
pub struct Duplicates<I: Iterator>
where
    I::Item: std::hash::Hash + Eq,
{
    iter: I,
    seen: std::collections::HashSet<I::Item>,
    yielded: std::collections::HashSet<I::Item>,
}

impl<I: Iterator> Iterator for Duplicates<I>
where
    I::Item: Clone + std::hash::Hash + Eq,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iter.next()?;
            if !self.seen.insert(item.clone()) && self.yielded.insert(item.clone()) {
                return Some(item);
            }
        }
    }
}

/// Generate range iterator.
pub fn range_inclusive(start: i64, end: i64) -> impl Iterator<Item = i64> {
    start..=end
}

/// Generate stepped range.
pub fn range_step(start: i64, end: i64, step: i64) -> impl Iterator<Item = i64> {
    std::iter::successors(Some(start), move |&n| {
        let next = n + step;
        if (step > 0 && next <= end) || (step < 0 && next >= end) {
            Some(next)
        } else {
            None
        }
    })
}

/// Repeat element n times.
pub fn repeat_n<T: Clone>(item: T, n: usize) -> impl Iterator<Item = T> {
    std::iter::repeat(item).take(n)
}

/// Cycle through items n times.
pub fn cycle_n<T: Clone>(items: Vec<T>, n: usize) -> impl Iterator<Item = T> {
    items.into_iter().cycle().take(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enumerate_from() {
        let v = vec!["a", "b", "c"];
        let result: Vec<_> = v.iter().enumerate_from(10).collect();
        assert_eq!(result, vec![(10, &"a"), (11, &"b"), (12, &"c")]);
    }

    #[test]
    fn test_take_while_inclusive() {
        let v = vec![1, 2, 3, 4, 5];
        let result: Vec<_> = v.into_iter().take_while_inclusive(|&x| x < 3).collect();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_interleave() {
        let a = vec![1, 3, 5];
        let b = vec![2, 4, 6];
        let result: Vec<_> = a.into_iter().interleave(b).collect();
        assert_eq!(result, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_sliding_window() {
        let v = vec![1, 2, 3, 4, 5];
        let windows: Vec<_> = v.into_iter().sliding_window(3).collect();
        assert_eq!(windows, vec![vec![1, 2, 3], vec![2, 3, 4], vec![3, 4, 5]]);
    }

    #[test]
    fn test_pairs() {
        let v = vec![1, 2, 3, 4];
        let pairs: Vec<_> = v.into_iter().pairs().collect();
        assert_eq!(pairs, vec![(1, 2), (2, 3), (3, 4)]);
    }

    #[test]
    fn test_unique() {
        let v = vec![1, 2, 2, 3, 1, 4];
        let unique: Vec<_> = v.into_iter().unique().collect();
        assert_eq!(unique, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_duplicates() {
        let v = vec![1, 2, 2, 3, 1, 4, 2];
        let dups: Vec<_> = v.into_iter().duplicates().collect();
        assert_eq!(dups, vec![2, 1]);
    }

    #[test]
    fn test_chunks_vec() {
        let v = vec![1, 2, 3, 4, 5];
        let chunks: Vec<Vec<_>> = v.into_iter().chunks_vec(2);
        assert_eq!(chunks, vec![vec![1, 2], vec![3, 4], vec![5]]);
    }

    #[test]
    fn test_group_by_key() {
        let v = vec![1, 2, 3, 4, 5, 6];
        let groups = v.into_iter().group_by_key(|&x| x % 2);
        assert_eq!(groups[&0], vec![2, 4, 6]);
        assert_eq!(groups[&1], vec![1, 3, 5]);
    }

    #[test]
    fn test_min_max() {
        let v = vec![3, 1, 4, 1, 5, 9, 2, 6];
        let (min, max) = v.into_iter().min_max().unwrap();
        assert_eq!(min, 1);
        assert_eq!(max, 9);
    }

    #[test]
    fn test_range_step() {
        let result: Vec<_> = range_step(0, 10, 2).collect();
        assert_eq!(result, vec![0, 2, 4, 6, 8, 10]);
    }
}
