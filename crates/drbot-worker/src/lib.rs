//! Worker thread utilities for drbot.
//!
//! This crate provides:
//! - Worker thread pool
//! - Job queue
//! - Work stealing

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use thiserror::Error;

/// Worker error types.
#[derive(Error, Debug)]
pub enum WorkerError {
    #[error("Worker pool shutdown")]
    Shutdown,

    #[error("Job failed: {0}")]
    JobFailed(String),

    #[error("Queue full")]
    QueueFull,
}

/// Result type for worker operations.
pub type Result<T> = std::result::Result<T, WorkerError>;

/// Job to be executed.
pub type Job = Box<dyn FnOnce() + Send + 'static>;

/// Simple thread pool.
pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<Arc<JobQueue>>,
}

impl ThreadPool {
    /// Create new thread pool.
    pub fn new(size: usize) -> Self {
        let queue = Arc::new(JobQueue::new());
        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id, queue.clone()));
        }

        Self {
            workers,
            sender: Some(queue),
        }
    }

    /// Execute a job.
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if let Some(ref queue) = self.sender {
            queue.push(Box::new(f));
        }
    }

    /// Get number of workers.
    pub fn size(&self) -> usize {
        self.workers.len()
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        if let Some(queue) = self.sender.take() {
            queue.shutdown();
        }

        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

/// Worker thread.
struct Worker {
    id: usize,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, queue: Arc<JobQueue>) -> Self {
        let thread = thread::spawn(move || loop {
            match queue.pop() {
                Some(job) => job(),
                None => break, // Shutdown signal
            }
        });

        Self {
            id,
            thread: Some(thread),
        }
    }
}

/// Job queue.
pub struct JobQueue {
    jobs: Mutex<VecDeque<Job>>,
    cond: Condvar,
    shutdown: Mutex<bool>,
}

impl JobQueue {
    /// Create new job queue.
    pub fn new() -> Self {
        Self {
            jobs: Mutex::new(VecDeque::new()),
            cond: Condvar::new(),
            shutdown: Mutex::new(false),
        }
    }

    /// Push job to queue.
    pub fn push(&self, job: Job) {
        let mut jobs = self.jobs.lock().unwrap();
        jobs.push_back(job);
        self.cond.notify_one();
    }

    /// Pop job from queue (blocking).
    pub fn pop(&self) -> Option<Job> {
        let mut jobs = self.jobs.lock().unwrap();
        loop {
            if *self.shutdown.lock().unwrap() && jobs.is_empty() {
                return None;
            }
            if let Some(job) = jobs.pop_front() {
                return Some(job);
            }
            jobs = self.cond.wait(jobs).unwrap();
        }
    }

    /// Try to pop job (non-blocking).
    pub fn try_pop(&self) -> Option<Job> {
        self.jobs.lock().unwrap().pop_front()
    }

    /// Signal shutdown.
    pub fn shutdown(&self) {
        *self.shutdown.lock().unwrap() = true;
        self.cond.notify_all();
    }

    /// Get queue length.
    pub fn len(&self) -> usize {
        self.jobs.lock().unwrap().len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.jobs.lock().unwrap().is_empty()
    }
}

impl Default for JobQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Worker with result collection.
pub struct ResultWorker<T: Send + 'static> {
    results: Arc<Mutex<Vec<T>>>,
    pool: ThreadPool,
}

impl<T: Send + 'static> ResultWorker<T> {
    /// Create new result worker.
    pub fn new(pool_size: usize) -> Self {
        Self {
            results: Arc::new(Mutex::new(Vec::new())),
            pool: ThreadPool::new(pool_size),
        }
    }

    /// Submit job that produces result.
    pub fn submit<F>(&self, f: F)
    where
        F: FnOnce() -> T + Send + 'static,
    {
        let results = self.results.clone();
        self.pool.execute(move || {
            let result = f();
            results.lock().unwrap().push(result);
        });
    }

    /// Get collected results (clones the results).
    pub fn results(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.results.lock().unwrap().clone()
    }

    /// Take results.
    pub fn take_results(&self) -> Vec<T> {
        std::mem::take(&mut *self.results.lock().unwrap())
    }
}

/// Background worker for periodic tasks.
pub struct BackgroundWorker {
    running: Arc<Mutex<bool>>,
    thread: Option<JoinHandle<()>>,
}

impl BackgroundWorker {
    /// Create and start background worker.
    pub fn start<F>(mut task: F, interval: std::time::Duration) -> Self
    where
        F: FnMut() + Send + 'static,
    {
        let running = Arc::new(Mutex::new(true));
        let running_clone = running.clone();

        let thread = thread::spawn(move || {
            while *running_clone.lock().unwrap() {
                task();
                thread::sleep(interval);
            }
        });

        Self {
            running,
            thread: Some(thread),
        }
    }

    /// Stop the worker.
    pub fn stop(&mut self) {
        *self.running.lock().unwrap() = false;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    /// Check if running.
    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }
}

impl Drop for BackgroundWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn test_thread_pool() {
        let counter = Arc::new(AtomicI32::new(0));
        let pool = ThreadPool::new(2);

        for _ in 0..10 {
            let c = counter.clone();
            pool.execute(move || {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }

        drop(pool); // Wait for completion
        assert_eq!(counter.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn test_job_queue() {
        let queue = JobQueue::new();

        let counter = Arc::new(AtomicI32::new(0));
        let c = counter.clone();
        queue.push(Box::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));

        let job = queue.try_pop().unwrap();
        job();

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
