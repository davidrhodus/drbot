//! Async barrier for drbot.
//!
//! This crate provides:
//! - Synchronization barrier
//! - Reusable barriers
//! - Leader election
//! - Phase synchronization

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{Barrier, Notify};

/// Barrier error types.
#[derive(Error, Debug)]
pub enum BarrierError {
    #[error("Barrier broken")]
    Broken,

    #[error("Timeout waiting for barrier")]
    Timeout,
}

/// Result type for barrier operations.
pub type Result<T> = std::result::Result<T, BarrierError>;

/// Async barrier for synchronizing tasks.
pub struct AsyncBarrier {
    inner: Barrier,
    count: usize,
}

impl AsyncBarrier {
    /// Create new barrier for N tasks.
    pub fn new(n: usize) -> Self {
        Self {
            inner: Barrier::new(n),
            count: n,
        }
    }

    /// Wait at barrier.
    pub async fn wait(&self) -> BarrierWaitResult {
        let result = self.inner.wait().await;
        BarrierWaitResult {
            is_leader: result.is_leader(),
        }
    }

    /// Get number of tasks this barrier synchronizes.
    pub fn count(&self) -> usize {
        self.count
    }
}

/// Result of waiting at a barrier.
pub struct BarrierWaitResult {
    is_leader: bool,
}

impl BarrierWaitResult {
    /// Check if this task is the leader.
    pub fn is_leader(&self) -> bool {
        self.is_leader
    }
}

/// Reusable barrier that can be used multiple times.
pub struct ReusableBarrier {
    count: usize,
    waiting: AtomicUsize,
    generation: AtomicUsize,
    notify: Notify,
}

impl ReusableBarrier {
    /// Create new reusable barrier.
    pub fn new(count: usize) -> Arc<Self> {
        Arc::new(Self {
            count,
            waiting: AtomicUsize::new(0),
            generation: AtomicUsize::new(0),
            notify: Notify::new(),
        })
    }

    /// Wait at barrier.
    pub async fn wait(&self) -> bool {
        let gen = self.generation.load(Ordering::Acquire);
        let waiting = self.waiting.fetch_add(1, Ordering::AcqRel) + 1;

        if waiting >= self.count {
            // Last one to arrive
            self.waiting.store(0, Ordering::Release);
            self.generation.fetch_add(1, Ordering::Release);
            self.notify.notify_waiters();
            return true; // Leader
        }

        // Wait for others
        loop {
            self.notify.notified().await;
            if self.generation.load(Ordering::Acquire) != gen {
                return false;
            }
        }
    }

    /// Get current count of waiting tasks.
    pub fn waiting_count(&self) -> usize {
        self.waiting.load(Ordering::Acquire)
    }
}

/// Phase barrier for multi-phase synchronization.
pub struct PhaseBarrier {
    barriers: Vec<Arc<ReusableBarrier>>,
    current_phase: AtomicUsize,
}

impl PhaseBarrier {
    /// Create new phase barrier with given phases.
    pub fn new(phases: usize, participants: usize) -> Self {
        let barriers = (0..phases)
            .map(|_| ReusableBarrier::new(participants))
            .collect();

        Self {
            barriers,
            current_phase: AtomicUsize::new(0),
        }
    }

    /// Complete current phase and advance.
    pub async fn advance(&self) -> usize {
        let phase = self.current_phase.fetch_add(1, Ordering::AcqRel);
        let idx = phase % self.barriers.len();
        self.barriers[idx].wait().await;
        phase + 1
    }

    /// Get current phase.
    pub fn current_phase(&self) -> usize {
        self.current_phase.load(Ordering::Acquire)
    }

    /// Get total phases.
    pub fn total_phases(&self) -> usize {
        self.barriers.len()
    }
}

/// Countdown latch (one-time barrier).
pub struct CountdownLatch {
    remaining: AtomicUsize,
    notify: Notify,
}

impl CountdownLatch {
    /// Create new countdown latch.
    pub fn new(count: usize) -> Arc<Self> {
        Arc::new(Self {
            remaining: AtomicUsize::new(count),
            notify: Notify::new(),
        })
    }

    /// Decrement the count.
    pub fn count_down(&self) {
        let remaining = self.remaining.fetch_sub(1, Ordering::AcqRel);
        if remaining == 1 {
            self.notify.notify_waiters();
        }
    }

    /// Wait until count reaches zero.
    pub async fn wait(&self) {
        loop {
            if self.remaining.load(Ordering::Acquire) == 0 {
                return;
            }
            self.notify.notified().await;
        }
    }

    /// Get remaining count.
    pub fn remaining(&self) -> usize {
        self.remaining.load(Ordering::Acquire)
    }

    /// Check if done.
    pub fn is_done(&self) -> bool {
        self.remaining.load(Ordering::Acquire) == 0
    }
}

/// Two-phase barrier for double-buffering patterns.
pub struct DoubleBarrier {
    enter: Arc<ReusableBarrier>,
    exit: Arc<ReusableBarrier>,
}

impl DoubleBarrier {
    /// Create new double barrier.
    pub fn new(count: usize) -> Self {
        Self {
            enter: ReusableBarrier::new(count),
            exit: ReusableBarrier::new(count),
        }
    }

    /// Enter the critical section.
    pub async fn enter(&self) -> bool {
        self.enter.wait().await
    }

    /// Exit the critical section.
    pub async fn leave(&self) -> bool {
        self.exit.wait().await
    }
}

impl Clone for DoubleBarrier {
    fn clone(&self) -> Self {
        Self {
            enter: Arc::clone(&self.enter),
            exit: Arc::clone(&self.exit),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI32;

    #[tokio::test]
    async fn test_basic_barrier() {
        let barrier = Arc::new(AsyncBarrier::new(3));
        let counter = Arc::new(AtomicI32::new(0));

        let mut handles = vec![];
        for _ in 0..3 {
            let b = Arc::clone(&barrier);
            let c = Arc::clone(&counter);
            handles.push(tokio::spawn(async move {
                c.fetch_add(1, Ordering::SeqCst);
                b.wait().await;
                c.load(Ordering::SeqCst)
            }));
        }

        for handle in handles {
            let count = handle.await.unwrap();
            assert_eq!(count, 3); // All should see 3
        }
    }

    #[tokio::test]
    async fn test_leader_election() {
        let barrier = Arc::new(AsyncBarrier::new(3));
        let mut handles = vec![];

        for _ in 0..3 {
            let b = Arc::clone(&barrier);
            handles.push(tokio::spawn(async move { b.wait().await.is_leader() }));
        }

        let mut leader_count = 0;
        for handle in handles {
            if handle.await.unwrap() {
                leader_count += 1;
            }
        }

        assert_eq!(leader_count, 1); // Exactly one leader
    }

    #[tokio::test]
    async fn test_reusable_barrier() {
        let barrier = ReusableBarrier::new(2);

        let b1 = Arc::clone(&barrier);
        let b2 = Arc::clone(&barrier);

        // First round
        let h1 = tokio::spawn(async move {
            b1.wait().await;
            b1.wait().await;
        });

        let h2 = tokio::spawn(async move {
            b2.wait().await;
            b2.wait().await;
        });

        h1.await.unwrap();
        h2.await.unwrap();
    }

    #[tokio::test]
    async fn test_countdown_latch() {
        let latch = CountdownLatch::new(3);

        let l1 = Arc::clone(&latch);
        let l2 = Arc::clone(&latch);

        // Waiter
        let waiter = tokio::spawn(async move {
            l1.wait().await;
            true
        });

        // Count down
        l2.count_down();
        l2.count_down();
        l2.count_down();

        assert!(waiter.await.unwrap());
        assert!(latch.is_done());
    }

    #[tokio::test]
    async fn test_double_barrier() {
        let barrier = DoubleBarrier::new(2);
        let b1 = barrier.clone();
        let b2 = barrier;

        let h1 = tokio::spawn(async move {
            b1.enter().await;
            // Critical section
            b1.leave().await
        });

        let h2 = tokio::spawn(async move {
            b2.enter().await;
            // Critical section
            b2.leave().await
        });

        h1.await.unwrap();
        h2.await.unwrap();
    }
}
