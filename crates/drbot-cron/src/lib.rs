//! Scheduled tasks for drbot.
//!
//! This crate provides cron-style scheduling for recurring tasks.
//!
//! # Features
//!
//! - Standard cron expression parsing
//! - Special expressions (@hourly, @daily, etc.)
//! - Async job execution
//! - Job persistence (optional)

mod expression;

pub use expression::CronExpression;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// A scheduled job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique job ID.
    pub id: Uuid,
    /// Job name.
    pub name: String,
    /// Cron expression string.
    pub schedule: String,
    /// Job payload (arbitrary JSON data).
    pub payload: serde_json::Value,
    /// Whether the job is enabled.
    pub enabled: bool,
    /// Last execution time.
    pub last_run: Option<DateTime<Utc>>,
    /// Next scheduled execution time.
    pub next_run: Option<DateTime<Utc>>,
    /// Number of times executed.
    pub run_count: u64,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

impl Job {
    /// Create a new job.
    pub fn new(name: impl Into<String>, schedule: impl Into<String>) -> drbot_core::Result<Self> {
        let schedule = schedule.into();

        // Validate the expression
        CronExpression::parse(&schedule).map_err(|e| drbot_core::Error::InvalidInput(e))?;

        let mut job = Self {
            id: Uuid::new_v4(),
            name: name.into(),
            schedule,
            payload: serde_json::Value::Null,
            enabled: true,
            last_run: None,
            next_run: None,
            run_count: 0,
            created_at: Utc::now(),
        };

        // Calculate initial next_run
        job.calculate_next_run();

        Ok(job)
    }

    /// Set job payload.
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }

    /// Enable or disable the job.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.calculate_next_run();
        } else {
            self.next_run = None;
        }
    }

    /// Calculate the next run time.
    pub fn calculate_next_run(&mut self) {
        if !self.enabled {
            self.next_run = None;
            return;
        }

        if let Ok(expr) = CronExpression::parse(&self.schedule) {
            let after = self.last_run.unwrap_or_else(Utc::now);
            self.next_run = expr.next(&after);
        }
    }

    /// Check if the job should run now.
    pub fn should_run(&self) -> bool {
        if !self.enabled {
            return false;
        }
        match self.next_run {
            Some(next) => Utc::now() >= next,
            None => false,
        }
    }

    /// Mark the job as executed.
    pub fn mark_executed(&mut self) {
        self.last_run = Some(Utc::now());
        self.run_count += 1;
        self.calculate_next_run();
    }
}

/// Event emitted when a job is triggered.
#[derive(Debug, Clone)]
pub struct JobEvent {
    /// Job that was triggered.
    pub job: Job,
    /// Trigger time.
    pub triggered_at: DateTime<Utc>,
}

/// Cron scheduler.
pub struct CronScheduler {
    /// Registered jobs.
    jobs: Arc<RwLock<HashMap<Uuid, Job>>>,
    /// Event sender.
    event_tx: broadcast::Sender<JobEvent>,
    /// Whether the scheduler is running.
    running: Arc<AtomicBool>,
    /// Scheduler task handle.
    task_handle: Option<tokio::task::JoinHandle<()>>,
    /// Tick interval in seconds.
    tick_interval: u64,
}

impl CronScheduler {
    /// Create a new scheduler.
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            running: Arc::new(AtomicBool::new(false)),
            task_handle: None,
            tick_interval: 60, // Check every minute by default
        }
    }

    /// Set the tick interval in seconds.
    pub fn with_tick_interval(mut self, seconds: u64) -> Self {
        self.tick_interval = seconds;
        self
    }

    /// Add a job to the scheduler.
    pub async fn add_job(&self, job: Job) -> Uuid {
        let id = job.id;
        self.jobs.write().await.insert(id, job);
        info!(job_id = %id, "Added job to scheduler");
        id
    }

    /// Remove a job from the scheduler.
    pub async fn remove_job(&self, id: Uuid) -> Option<Job> {
        let job = self.jobs.write().await.remove(&id);
        if job.is_some() {
            info!(job_id = %id, "Removed job from scheduler");
        }
        job
    }

    /// Get a job by ID.
    pub async fn get_job(&self, id: Uuid) -> Option<Job> {
        self.jobs.read().await.get(&id).cloned()
    }

    /// Get all jobs.
    pub async fn list_jobs(&self) -> Vec<Job> {
        self.jobs.read().await.values().cloned().collect()
    }

    /// Enable or disable a job.
    pub async fn set_job_enabled(&self, id: Uuid, enabled: bool) -> Option<()> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(&id) {
            job.set_enabled(enabled);
            Some(())
        } else {
            None
        }
    }

    /// Subscribe to job events.
    pub fn subscribe(&self) -> broadcast::Receiver<JobEvent> {
        self.event_tx.subscribe()
    }

    /// Start the scheduler.
    pub fn start(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            warn!("Scheduler already running");
            return;
        }

        info!("Starting cron scheduler");
        self.running.store(true, Ordering::SeqCst);

        let jobs = self.jobs.clone();
        let event_tx = self.event_tx.clone();
        let running = self.running.clone();
        let tick_interval = self.tick_interval;

        let handle = tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                // Check for jobs to run
                let mut jobs_lock = jobs.write().await;
                let now = Utc::now();

                for job in jobs_lock.values_mut() {
                    if job.should_run() {
                        debug!(job_id = %job.id, name = %job.name, "Triggering job");

                        let event = JobEvent {
                            job: job.clone(),
                            triggered_at: now,
                        };

                        job.mark_executed();

                        if event_tx.send(event).is_err() {
                            // No receivers, that's okay
                        }
                    }
                }

                drop(jobs_lock);

                // Sleep until next tick
                tokio::time::sleep(std::time::Duration::from_secs(tick_interval)).await;
            }

            info!("Cron scheduler stopped");
        });

        self.task_handle = Some(handle);
    }

    /// Stop the scheduler.
    pub fn stop(&mut self) {
        info!("Stopping cron scheduler");
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.task_handle.take() {
            handle.abort();
        }
    }

    /// Check if the scheduler is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Default for CronScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CronScheduler {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_creation() {
        let job = Job::new("test-job", "0 * * * *").unwrap();
        assert_eq!(job.name, "test-job");
        assert_eq!(job.schedule, "0 * * * *");
        assert!(job.enabled);
        assert!(job.next_run.is_some());
    }

    #[test]
    fn test_job_with_payload() {
        let job = Job::new("test-job", "@hourly")
            .unwrap()
            .with_payload(serde_json::json!({"key": "value"}));

        assert_eq!(job.payload["key"], "value");
    }

    #[test]
    fn test_job_invalid_schedule() {
        let result = Job::new("test-job", "invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_job_disable_enable() {
        let mut job = Job::new("test-job", "0 * * * *").unwrap();
        assert!(job.next_run.is_some());

        job.set_enabled(false);
        assert!(!job.enabled);
        assert!(job.next_run.is_none());

        job.set_enabled(true);
        assert!(job.enabled);
        assert!(job.next_run.is_some());
    }

    #[tokio::test]
    async fn test_scheduler_add_remove() {
        let scheduler = CronScheduler::new();

        let job = Job::new("test-job", "@hourly").unwrap();
        let id = job.id;

        scheduler.add_job(job).await;

        assert!(scheduler.get_job(id).await.is_some());

        scheduler.remove_job(id).await;

        assert!(scheduler.get_job(id).await.is_none());
    }

    #[tokio::test]
    async fn test_scheduler_list_jobs() {
        let scheduler = CronScheduler::new();

        scheduler
            .add_job(Job::new("job1", "@hourly").unwrap())
            .await;
        scheduler.add_job(Job::new("job2", "@daily").unwrap()).await;

        let jobs = scheduler.list_jobs().await;
        assert_eq!(jobs.len(), 2);
    }

    #[test]
    fn test_scheduler_creation() {
        let scheduler = CronScheduler::new();
        assert!(!scheduler.is_running());
    }
}
