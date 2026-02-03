//! Background task queue for drbot.
//!
//! This crate provides:
//! - Priority-based task queuing
//! - Task scheduling
//! - Worker pools
//! - Task persistence

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};
use uuid::Uuid;

/// Task queue error types.
#[derive(Error, Debug)]
pub enum TaskQueueError {
    #[error("Queue full")]
    QueueFull,

    #[error("Task not found: {0}")]
    NotFound(Uuid),

    #[error("Task failed: {0}")]
    TaskFailed(String),

    #[error("Queue closed")]
    Closed,

    #[error("Timeout")]
    Timeout,
}

/// Result type for task queue operations.
pub type Result<T> = std::result::Result<T, TaskQueueError>;

/// Task priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Task status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Task metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMeta {
    /// Task ID.
    pub id: Uuid,
    /// Task name.
    pub name: String,
    /// Task priority.
    pub priority: Priority,
    /// Task status.
    pub status: TaskStatus,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Started at.
    pub started_at: Option<DateTime<Utc>>,
    /// Completed at.
    pub completed_at: Option<DateTime<Utc>>,
    /// Retry count.
    pub retry_count: u32,
    /// Max retries.
    pub max_retries: u32,
    /// Error message if failed.
    pub error: Option<String>,
    /// Scheduled execution time.
    pub scheduled_at: Option<DateTime<Utc>>,
}

impl TaskMeta {
    /// Create new task metadata.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            priority: Priority::Normal,
            status: TaskStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            retry_count: 0,
            max_retries: 3,
            error: None,
            scheduled_at: None,
        }
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Set max retries.
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Schedule for later.
    pub fn schedule_at(mut self, time: DateTime<Utc>) -> Self {
        self.scheduled_at = Some(time);
        self
    }

    /// Check if ready to run.
    pub fn is_ready(&self) -> bool {
        if self.status != TaskStatus::Pending {
            return false;
        }
        if let Some(scheduled) = self.scheduled_at {
            Utc::now() >= scheduled
        } else {
            true
        }
    }
}

/// Task trait.
#[async_trait]
pub trait Task: Send + Sync {
    /// Get task metadata.
    fn meta(&self) -> &TaskMeta;

    /// Get mutable metadata.
    fn meta_mut(&mut self) -> &mut TaskMeta;

    /// Execute the task.
    async fn execute(&mut self) -> Result<()>;

    /// Called on task completion.
    async fn on_complete(&mut self) {}

    /// Called on task failure.
    async fn on_failure(&mut self, _error: &str) {}
}

/// Boxed task.
pub type BoxedTask = Box<dyn Task>;

/// Priority queue entry.
struct PriorityEntry {
    priority: Priority,
    created_at: DateTime<Utc>,
    task_id: Uuid,
}

impl PartialEq for PriorityEntry {
    fn eq(&self, other: &Self) -> bool {
        self.task_id == other.task_id
    }
}

impl Eq for PriorityEntry {}

impl PartialOrd for PriorityEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first, then older tasks first
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => other.created_at.cmp(&self.created_at),
            ord => ord,
        }
    }
}

/// Simple in-memory task queue.
pub struct TaskQueue {
    tasks: RwLock<HashMap<Uuid, BoxedTask>>,
    priority_queue: Mutex<BinaryHeap<PriorityEntry>>,
    max_size: usize,
    pending: AtomicUsize,
    running: AtomicUsize,
    completed: AtomicU64,
    failed: AtomicU64,
    closed: std::sync::atomic::AtomicBool,
}

impl TaskQueue {
    /// Create a new task queue.
    pub fn new(max_size: usize) -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            priority_queue: Mutex::new(BinaryHeap::new()),
            max_size,
            pending: AtomicUsize::new(0),
            running: AtomicUsize::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            closed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Submit a task.
    pub async fn submit(&self, task: BoxedTask) -> Result<Uuid> {
        if self.closed.load(AtomicOrdering::Relaxed) {
            return Err(TaskQueueError::Closed);
        }

        let pending = self.pending.load(AtomicOrdering::Relaxed);
        if pending >= self.max_size {
            return Err(TaskQueueError::QueueFull);
        }

        let id = task.meta().id;
        let priority = task.meta().priority;
        let created_at = task.meta().created_at;

        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(id, task);
        }

        {
            let mut pq = self.priority_queue.lock().await;
            pq.push(PriorityEntry {
                priority,
                created_at,
                task_id: id,
            });
        }

        self.pending.fetch_add(1, AtomicOrdering::Relaxed);
        Ok(id)
    }

    /// Get next ready task.
    pub async fn next(&self) -> Option<Uuid> {
        let mut pq = self.priority_queue.lock().await;

        while let Some(entry) = pq.pop() {
            let tasks = self.tasks.read().await;
            if let Some(task) = tasks.get(&entry.task_id) {
                if task.meta().is_ready() {
                    return Some(entry.task_id);
                }
                // Not ready, re-queue
                drop(tasks);
                pq.push(entry);
                return None;
            }
        }

        None
    }

    /// Get a task by ID.
    pub async fn get(&self, id: Uuid) -> Option<TaskStatus> {
        let tasks = self.tasks.read().await;
        tasks.get(&id).map(|t| t.meta().status)
    }

    /// Cancel a task.
    pub async fn cancel(&self, id: Uuid) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(&id) {
            if task.meta().status == TaskStatus::Pending {
                task.meta_mut().status = TaskStatus::Cancelled;
                self.pending.fetch_sub(1, AtomicOrdering::Relaxed);
                return Ok(());
            }
        }
        Err(TaskQueueError::NotFound(id))
    }

    /// Get queue statistics.
    pub fn stats(&self) -> QueueStats {
        QueueStats {
            pending: self.pending.load(AtomicOrdering::Relaxed),
            running: self.running.load(AtomicOrdering::Relaxed),
            completed: self.completed.load(AtomicOrdering::Relaxed),
            failed: self.failed.load(AtomicOrdering::Relaxed),
            max_size: self.max_size,
        }
    }

    /// Close the queue.
    pub fn close(&self) {
        self.closed.store(true, AtomicOrdering::SeqCst);
    }

    /// Check if closed.
    pub fn is_closed(&self) -> bool {
        self.closed.load(AtomicOrdering::Relaxed)
    }
}

/// Queue statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub pending: usize,
    pub running: usize,
    pub completed: u64,
    pub failed: u64,
    pub max_size: usize,
}

/// Worker pool for processing tasks.
pub struct WorkerPool {
    queue: Arc<TaskQueue>,
    workers: usize,
    semaphore: Arc<Semaphore>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl WorkerPool {
    /// Create a new worker pool.
    pub fn new(queue: Arc<TaskQueue>, workers: usize) -> Self {
        Self {
            queue,
            workers,
            semaphore: Arc::new(Semaphore::new(workers)),
            shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Start the worker pool.
    pub async fn start(&self) {
        let mut handles = Vec::new();

        for worker_id in 0..self.workers {
            let queue = self.queue.clone();
            let semaphore = self.semaphore.clone();
            let shutdown = self.shutdown.clone();

            let handle = tokio::spawn(async move {
                loop {
                    if shutdown.load(AtomicOrdering::Relaxed) {
                        break;
                    }

                    // Acquire worker slot
                    let _permit = semaphore.acquire().await.unwrap();

                    // Get next task
                    if let Some(task_id) = queue.next().await {
                        // Process task
                        let result = Self::process_task(&queue, task_id).await;

                        if let Err(e) = result {
                            eprintln!("Worker {} task {} failed: {}", worker_id, task_id, e);
                        }
                    } else {
                        // No tasks, wait a bit
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            });

            handles.push(handle);
        }
    }

    async fn process_task(queue: &TaskQueue, task_id: Uuid) -> Result<()> {
        // Mark as running
        {
            let mut tasks = queue.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.meta_mut().status = TaskStatus::Running;
                task.meta_mut().started_at = Some(Utc::now());
            } else {
                return Err(TaskQueueError::NotFound(task_id));
            }
        }

        queue.pending.fetch_sub(1, AtomicOrdering::Relaxed);
        queue.running.fetch_add(1, AtomicOrdering::Relaxed);

        // Execute task
        let result = {
            let mut tasks = queue.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.execute().await
            } else {
                return Err(TaskQueueError::NotFound(task_id));
            }
        };

        queue.running.fetch_sub(1, AtomicOrdering::Relaxed);

        // Update status
        {
            let mut tasks = queue.tasks.write().await;
            if let Some(task) = tasks.get_mut(&task_id) {
                task.meta_mut().completed_at = Some(Utc::now());

                match result {
                    Ok(()) => {
                        task.meta_mut().status = TaskStatus::Completed;
                        task.on_complete().await;
                        queue.completed.fetch_add(1, AtomicOrdering::Relaxed);
                    }
                    Err(ref e) => {
                        let error_msg = e.to_string();
                        task.meta_mut().retry_count += 1;

                        if task.meta().retry_count < task.meta().max_retries {
                            // Retry
                            task.meta_mut().status = TaskStatus::Pending;
                            task.meta_mut().started_at = None;
                            task.meta_mut().completed_at = None;
                            queue.pending.fetch_add(1, AtomicOrdering::Relaxed);
                        } else {
                            // Failed permanently
                            task.meta_mut().status = TaskStatus::Failed;
                            task.meta_mut().error = Some(error_msg.clone());
                            task.on_failure(&error_msg).await;
                            queue.failed.fetch_add(1, AtomicOrdering::Relaxed);
                        }
                    }
                }
            }
        }

        result
    }

    /// Shutdown the worker pool.
    pub fn shutdown(&self) {
        self.shutdown.store(true, AtomicOrdering::SeqCst);
    }

    /// Check if shutdown.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(AtomicOrdering::Relaxed)
    }
}

/// Simple task implementation.
pub struct SimpleTask<F>
where
    F: FnMut() -> Result<()> + Send + Sync,
{
    meta: TaskMeta,
    func: F,
}

impl<F> SimpleTask<F>
where
    F: FnMut() -> Result<()> + Send + Sync,
{
    /// Create a new simple task.
    pub fn new(name: impl Into<String>, func: F) -> Self {
        Self {
            meta: TaskMeta::new(name),
            func,
        }
    }

    /// Set priority.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.meta = self.meta.with_priority(priority);
        self
    }
}

#[async_trait]
impl<F> Task for SimpleTask<F>
where
    F: FnMut() -> Result<()> + Send + Sync,
{
    fn meta(&self) -> &TaskMeta {
        &self.meta
    }

    fn meta_mut(&mut self) -> &mut TaskMeta {
        &mut self.meta
    }

    async fn execute(&mut self) -> Result<()> {
        (self.func)()
    }
}

/// Delayed task queue.
pub struct DelayedQueue {
    tasks: Mutex<VecDeque<(DateTime<Utc>, BoxedTask)>>,
}

impl DelayedQueue {
    /// Create a new delayed queue.
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(VecDeque::new()),
        }
    }

    /// Schedule a task.
    pub async fn schedule(&self, task: BoxedTask, delay: Duration) {
        let run_at = Utc::now() + chrono::Duration::from_std(delay).unwrap_or_default();
        let mut tasks = self.tasks.lock().await;
        tasks.push_back((run_at, task));
        // Sort by run time
        tasks.make_contiguous().sort_by_key(|(t, _)| *t);
    }

    /// Get next ready task.
    pub async fn next_ready(&self) -> Option<BoxedTask> {
        let mut tasks = self.tasks.lock().await;
        let now = Utc::now();

        if let Some((run_at, _)) = tasks.front() {
            if *run_at <= now {
                return tasks.pop_front().map(|(_, t)| t);
            }
        }

        None
    }

    /// Get pending count.
    pub async fn pending(&self) -> usize {
        self.tasks.lock().await.len()
    }
}

impl Default for DelayedQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestTask {
        meta: TaskMeta,
        executed: Arc<std::sync::atomic::AtomicBool>,
    }

    impl TestTask {
        fn new(name: &str, executed: Arc<std::sync::atomic::AtomicBool>) -> Self {
            Self {
                meta: TaskMeta::new(name),
                executed,
            }
        }
    }

    #[async_trait]
    impl Task for TestTask {
        fn meta(&self) -> &TaskMeta {
            &self.meta
        }

        fn meta_mut(&mut self) -> &mut TaskMeta {
            &mut self.meta
        }

        async fn execute(&mut self) -> Result<()> {
            self.executed.store(true, AtomicOrdering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn test_task_meta() {
        let meta = TaskMeta::new("test")
            .with_priority(Priority::High)
            .with_max_retries(5);

        assert_eq!(meta.name, "test");
        assert_eq!(meta.priority, Priority::High);
        assert_eq!(meta.max_retries, 5);
        assert!(meta.is_ready());
    }

    #[test]
    fn test_scheduled_task() {
        let future = Utc::now() + chrono::Duration::hours(1);
        let meta = TaskMeta::new("test").schedule_at(future);

        assert!(!meta.is_ready());
    }

    #[tokio::test]
    async fn test_task_queue_submit() {
        let queue = TaskQueue::new(100);
        let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task = Box::new(TestTask::new("test", executed));

        let id = queue.submit(task).await.unwrap();

        let stats = queue.stats();
        assert_eq!(stats.pending, 1);

        let status = queue.get(id).await;
        assert_eq!(status, Some(TaskStatus::Pending));
    }

    #[tokio::test]
    async fn test_task_queue_priority() {
        let queue = TaskQueue::new(100);

        let exec1 = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut task1 = TestTask::new("low", exec1);
        task1.meta = task1.meta.with_priority(Priority::Low);

        let exec2 = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut task2 = TestTask::new("high", exec2);
        task2.meta = task2.meta.with_priority(Priority::High);

        queue.submit(Box::new(task1)).await.unwrap();
        let high_id = queue.submit(Box::new(task2)).await.unwrap();

        // High priority should come first
        let next = queue.next().await;
        assert_eq!(next, Some(high_id));
    }

    #[tokio::test]
    async fn test_task_queue_cancel() {
        let queue = TaskQueue::new(100);
        let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task = Box::new(TestTask::new("test", executed));

        let id = queue.submit(task).await.unwrap();
        queue.cancel(id).await.unwrap();

        let status = queue.get(id).await;
        assert_eq!(status, Some(TaskStatus::Cancelled));
    }

    #[tokio::test]
    async fn test_queue_full() {
        let queue = TaskQueue::new(1);

        let exec1 = Arc::new(std::sync::atomic::AtomicBool::new(false));
        queue
            .submit(Box::new(TestTask::new("t1", exec1)))
            .await
            .unwrap();

        let exec2 = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let result = queue.submit(Box::new(TestTask::new("t2", exec2))).await;

        assert!(matches!(result, Err(TaskQueueError::QueueFull)));
    }

    #[tokio::test]
    async fn test_delayed_queue() {
        let queue = DelayedQueue::new();

        let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task = Box::new(TestTask::new("test", executed));

        queue.schedule(task, Duration::from_millis(50)).await;

        // Not ready yet
        assert!(queue.next_ready().await.is_none());

        tokio::time::sleep(Duration::from_millis(60)).await;

        // Now ready
        let task = queue.next_ready().await;
        assert!(task.is_some());
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Low < Priority::Normal);
        assert!(Priority::Normal < Priority::High);
        assert!(Priority::High < Priority::Critical);
    }
}
