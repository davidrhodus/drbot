//! Job scheduler for drbot.
//!
//! This crate provides:
//! - Cron-based scheduling
//! - One-time scheduled jobs
//! - Recurring jobs
//! - Job persistence

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Scheduler error types.
#[derive(Error, Debug)]
pub enum SchedulerError {
    #[error("Job not found: {0}")]
    JobNotFound(Uuid),

    #[error("Invalid schedule: {0}")]
    InvalidSchedule(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Storage error: {0}")]
    StorageError(String),
}

/// Result type for scheduler operations.
pub type Result<T> = std::result::Result<T, SchedulerError>;

/// Job status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    /// Job is scheduled.
    Scheduled,
    /// Job is running.
    Running,
    /// Job completed successfully.
    Completed,
    /// Job failed.
    Failed,
    /// Job was cancelled.
    Cancelled,
    /// Job is paused.
    Paused,
}

/// Schedule type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Schedule {
    /// One-time execution at a specific time.
    Once(DateTime<Utc>),
    /// Recurring interval.
    Interval(std::time::Duration),
    /// Cron expression.
    Cron(String),
    /// Fixed delay after completion.
    FixedDelay(std::time::Duration),
}

impl Schedule {
    /// Calculate next run time from now.
    pub fn next_run(&self, from: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Schedule::Once(at) => {
                if *at > from {
                    Some(*at)
                } else {
                    None
                }
            }
            Schedule::Interval(interval) => {
                Some(from + Duration::from_std(*interval).unwrap_or(Duration::zero()))
            }
            Schedule::Cron(expr) => {
                // Simplified cron parsing - in real impl would use a cron parser
                // For now, just return 1 minute from now
                Some(from + Duration::minutes(1))
            }
            Schedule::FixedDelay(delay) => {
                Some(from + Duration::from_std(*delay).unwrap_or(Duration::zero()))
            }
        }
    }
}

/// A scheduled job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique job ID.
    pub id: Uuid,
    /// Job name.
    pub name: String,
    /// Job description.
    pub description: Option<String>,
    /// Schedule.
    pub schedule: Schedule,
    /// Current status.
    pub status: JobStatus,
    /// Job payload/configuration.
    pub payload: serde_json::Value,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Next run time.
    pub next_run_at: Option<DateTime<Utc>>,
    /// Last run time.
    pub last_run_at: Option<DateTime<Utc>>,
    /// Last run result.
    pub last_result: Option<JobResult>,
    /// Run count.
    pub run_count: u64,
    /// Max retries on failure.
    pub max_retries: u32,
    /// Current retry count.
    pub retry_count: u32,
    /// Tags for grouping.
    pub tags: Vec<String>,
}

impl Job {
    /// Create a new job.
    pub fn new(name: impl Into<String>, schedule: Schedule) -> Self {
        let now = Utc::now();
        let next_run_at = match &schedule {
            Schedule::Once(at) => Some(*at),
            _ => schedule.next_run(now),
        };

        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: None,
            schedule,
            status: JobStatus::Scheduled,
            payload: serde_json::Value::Null,
            created_at: now,
            next_run_at,
            last_run_at: None,
            last_result: None,
            run_count: 0,
            max_retries: 3,
            retry_count: 0,
            tags: Vec::new(),
        }
    }

    /// Set description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set payload.
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }

    /// Set max retries.
    pub fn with_max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Check if job should run.
    pub fn should_run(&self, now: DateTime<Utc>) -> bool {
        self.status == JobStatus::Scheduled && self.next_run_at.map(|t| t <= now).unwrap_or(false)
    }
}

/// Job execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    /// Execution ID.
    pub execution_id: Uuid,
    /// Start time.
    pub started_at: DateTime<Utc>,
    /// End time.
    pub ended_at: DateTime<Utc>,
    /// Success.
    pub success: bool,
    /// Result data.
    pub data: Option<serde_json::Value>,
    /// Error message.
    pub error: Option<String>,
}

impl JobResult {
    /// Create a success result.
    pub fn success(
        execution_id: Uuid,
        started_at: DateTime<Utc>,
        data: Option<serde_json::Value>,
    ) -> Self {
        Self {
            execution_id,
            started_at,
            ended_at: Utc::now(),
            success: true,
            data,
            error: None,
        }
    }

    /// Create a failure result.
    pub fn failure(
        execution_id: Uuid,
        started_at: DateTime<Utc>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            execution_id,
            started_at,
            ended_at: Utc::now(),
            success: false,
            data: None,
            error: Some(error.into()),
        }
    }

    /// Duration.
    pub fn duration(&self) -> Duration {
        self.ended_at - self.started_at
    }
}

/// Job executor trait.
#[async_trait]
pub trait JobExecutor: Send + Sync {
    /// Execute a job.
    async fn execute(&self, job: &Job) -> Result<serde_json::Value>;
}

/// Job storage trait.
#[async_trait]
pub trait JobStorage: Send + Sync {
    /// Save a job.
    async fn save(&self, job: Job) -> Result<()>;

    /// Get a job by ID.
    async fn get(&self, id: Uuid) -> Result<Option<Job>>;

    /// Get jobs due for execution.
    async fn get_due_jobs(&self, limit: usize) -> Result<Vec<Job>>;

    /// Update job status.
    async fn update_status(&self, id: Uuid, status: JobStatus) -> Result<()>;

    /// Record execution result.
    async fn record_result(&self, id: Uuid, result: JobResult) -> Result<()>;

    /// Delete a job.
    async fn delete(&self, id: Uuid) -> Result<()>;

    /// List all jobs.
    async fn list(&self) -> Result<Vec<Job>>;
}

/// In-memory job storage.
pub struct InMemoryJobStorage {
    jobs: RwLock<HashMap<Uuid, Job>>,
}

impl InMemoryJobStorage {
    /// Create new storage.
    pub fn new() -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryJobStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl JobStorage for InMemoryJobStorage {
    async fn save(&self, job: Job) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        jobs.insert(job.id, job);
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<Job>> {
        let jobs = self.jobs.read().await;
        Ok(jobs.get(&id).cloned())
    }

    async fn get_due_jobs(&self, limit: usize) -> Result<Vec<Job>> {
        let jobs = self.jobs.read().await;
        let now = Utc::now();

        let due: Vec<_> = jobs
            .values()
            .filter(|j| j.should_run(now))
            .take(limit)
            .cloned()
            .collect();

        Ok(due)
    }

    async fn update_status(&self, id: Uuid, status: JobStatus) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(&id) {
            job.status = status;
            Ok(())
        } else {
            Err(SchedulerError::JobNotFound(id))
        }
    }

    async fn record_result(&self, id: Uuid, result: JobResult) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(&id) {
            job.last_run_at = Some(result.ended_at);
            job.run_count += 1;

            if result.success {
                job.status = JobStatus::Completed;
                job.retry_count = 0;
                // Schedule next run for recurring jobs
                if let Some(next) = job.schedule.next_run(result.ended_at) {
                    job.next_run_at = Some(next);
                    job.status = JobStatus::Scheduled;
                }
            } else {
                job.retry_count += 1;
                if job.retry_count >= job.max_retries {
                    job.status = JobStatus::Failed;
                } else {
                    job.status = JobStatus::Scheduled;
                    // Exponential backoff for retry
                    let delay = std::time::Duration::from_secs(2u64.pow(job.retry_count));
                    job.next_run_at = Some(result.ended_at + Duration::from_std(delay).unwrap());
                }
            }

            job.last_result = Some(result);
            Ok(())
        } else {
            Err(SchedulerError::JobNotFound(id))
        }
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        jobs.remove(&id);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<Job>> {
        let jobs = self.jobs.read().await;
        Ok(jobs.values().cloned().collect())
    }
}

/// Scheduler.
pub struct Scheduler<S: JobStorage, E: JobExecutor> {
    storage: Arc<S>,
    executor: Arc<E>,
    running: AtomicBool,
    executed_count: AtomicU64,
}

impl<S: JobStorage + 'static, E: JobExecutor + 'static> Scheduler<S, E> {
    /// Create a new scheduler.
    pub fn new(storage: Arc<S>, executor: Arc<E>) -> Self {
        Self {
            storage,
            executor,
            running: AtomicBool::new(false),
            executed_count: AtomicU64::new(0),
        }
    }

    /// Schedule a job.
    pub async fn schedule(&self, job: Job) -> Result<Uuid> {
        let id = job.id;
        self.storage.save(job).await?;
        Ok(id)
    }

    /// Cancel a job.
    pub async fn cancel(&self, id: Uuid) -> Result<()> {
        self.storage.update_status(id, JobStatus::Cancelled).await
    }

    /// Pause a job.
    pub async fn pause(&self, id: Uuid) -> Result<()> {
        self.storage.update_status(id, JobStatus::Paused).await
    }

    /// Resume a job.
    pub async fn resume(&self, id: Uuid) -> Result<()> {
        self.storage.update_status(id, JobStatus::Scheduled).await
    }

    /// Run one tick of the scheduler.
    pub async fn tick(&self) -> Result<usize> {
        let jobs = self.storage.get_due_jobs(10).await?;
        let mut executed = 0;

        for job in jobs {
            self.storage
                .update_status(job.id, JobStatus::Running)
                .await?;

            let execution_id = Uuid::new_v4();
            let started_at = Utc::now();

            let result = match self.executor.execute(&job).await {
                Ok(data) => JobResult::success(execution_id, started_at, Some(data)),
                Err(e) => JobResult::failure(execution_id, started_at, e.to_string()),
            };

            self.storage.record_result(job.id, result).await?;
            self.executed_count.fetch_add(1, Ordering::Relaxed);
            executed += 1;
        }

        Ok(executed)
    }

    /// Get executed count.
    pub fn executed_count(&self) -> u64 {
        self.executed_count.load(Ordering::Relaxed)
    }

    /// List all jobs.
    pub async fn list_jobs(&self) -> Result<Vec<Job>> {
        self.storage.list().await
    }

    /// Get a job.
    pub async fn get_job(&self, id: Uuid) -> Result<Option<Job>> {
        self.storage.get(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoExecutor;

    #[async_trait]
    impl JobExecutor for EchoExecutor {
        async fn execute(&self, job: &Job) -> Result<serde_json::Value> {
            Ok(job.payload.clone())
        }
    }

    struct FailingExecutor;

    #[async_trait]
    impl JobExecutor for FailingExecutor {
        async fn execute(&self, _job: &Job) -> Result<serde_json::Value> {
            Err(SchedulerError::ExecutionError("Failed".to_string()))
        }
    }

    #[test]
    fn test_job_creation() {
        let job = Job::new("test-job", Schedule::Once(Utc::now() + Duration::hours(1)))
            .with_description("Test job")
            .with_payload(serde_json::json!({"key": "value"}))
            .with_tag("test");

        assert_eq!(job.name, "test-job");
        assert!(job.description.is_some());
        assert_eq!(job.tags.len(), 1);
    }

    #[test]
    fn test_schedule_once() {
        let future = Utc::now() + Duration::hours(1);
        let schedule = Schedule::Once(future);

        let next = schedule.next_run(Utc::now());
        assert!(next.is_some());
        assert_eq!(next.unwrap(), future);

        // Past time should return None
        let past = Utc::now() - Duration::hours(1);
        let schedule = Schedule::Once(past);
        assert!(schedule.next_run(Utc::now()).is_none());
    }

    #[test]
    fn test_schedule_interval() {
        let schedule = Schedule::Interval(std::time::Duration::from_secs(60));
        let now = Utc::now();
        let next = schedule.next_run(now).unwrap();

        assert!(next > now);
    }

    #[tokio::test]
    async fn test_scheduler_schedule_job() {
        let storage = Arc::new(InMemoryJobStorage::new());
        let executor = Arc::new(EchoExecutor);
        let scheduler = Scheduler::new(storage, executor);

        let job = Job::new("test", Schedule::Once(Utc::now() + Duration::hours(1)));
        let id = scheduler.schedule(job).await.unwrap();

        let retrieved = scheduler.get_job(id).await.unwrap();
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_scheduler_tick() {
        let storage = Arc::new(InMemoryJobStorage::new());
        let executor = Arc::new(EchoExecutor);
        let scheduler = Scheduler::new(storage, executor);

        // Schedule job for immediate execution
        let job = Job::new("test", Schedule::Once(Utc::now() - Duration::seconds(1)));
        scheduler.schedule(job).await.unwrap();

        let executed = scheduler.tick().await.unwrap();
        assert_eq!(executed, 1);
        assert_eq!(scheduler.executed_count(), 1);
    }

    #[tokio::test]
    async fn test_job_result_success() {
        let storage = Arc::new(InMemoryJobStorage::new());
        let executor = Arc::new(EchoExecutor);
        let scheduler = Scheduler::new(storage.clone(), executor);

        let job = Job::new("test", Schedule::Once(Utc::now() - Duration::seconds(1)));
        let id = scheduler.schedule(job).await.unwrap();

        scheduler.tick().await.unwrap();

        let job = scheduler.get_job(id).await.unwrap().unwrap();
        assert!(job.last_result.is_some());
        assert!(job.last_result.unwrap().success);
    }

    #[tokio::test]
    async fn test_job_result_failure() {
        let storage = Arc::new(InMemoryJobStorage::new());
        let executor = Arc::new(FailingExecutor);
        let scheduler = Scheduler::new(storage, executor);

        let job = Job::new("test", Schedule::Once(Utc::now() - Duration::seconds(1)));
        let id = scheduler.schedule(job).await.unwrap();

        scheduler.tick().await.unwrap();

        let job = scheduler.get_job(id).await.unwrap().unwrap();
        assert!(job.last_result.is_some());
        assert!(!job.last_result.unwrap().success);
        assert_eq!(job.retry_count, 1);
    }

    #[tokio::test]
    async fn test_cancel_job() {
        let storage = Arc::new(InMemoryJobStorage::new());
        let executor = Arc::new(EchoExecutor);
        let scheduler = Scheduler::new(storage, executor);

        let job = Job::new("test", Schedule::Once(Utc::now() + Duration::hours(1)));
        let id = scheduler.schedule(job).await.unwrap();

        scheduler.cancel(id).await.unwrap();

        let job = scheduler.get_job(id).await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_pause_resume_job() {
        let storage = Arc::new(InMemoryJobStorage::new());
        let executor = Arc::new(EchoExecutor);
        let scheduler = Scheduler::new(storage, executor);

        let job = Job::new("test", Schedule::Once(Utc::now() + Duration::hours(1)));
        let id = scheduler.schedule(job).await.unwrap();

        scheduler.pause(id).await.unwrap();
        let job = scheduler.get_job(id).await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Paused);

        scheduler.resume(id).await.unwrap();
        let job = scheduler.get_job(id).await.unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Scheduled);
    }
}
