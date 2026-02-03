//! Workflow recording for drbot.
//!
//! Record and replay user workflows.
//!
//! # Features
//!
//! - Action recording
//! - Workflow playback
//! - Step editing
//! - Variable extraction
//! - Conditional branching

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Recorder result type.
pub type Result<T> = std::result::Result<T, RecorderError>;

/// Recorder errors.
#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error("Recording not found: {0}")]
    NotFound(String),
    #[error("Playback failed: {0}")]
    PlaybackFailed(String),
    #[error("Step failed: {0}")]
    StepFailed(String),
    #[error("Not recording")]
    NotRecording,
    #[error("Already recording")]
    AlreadyRecording,
}

/// Recorded action types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    /// Click at position.
    Click { x: i32, y: i32, button: MouseButton },
    /// Type text.
    TypeText { text: String },
    /// Press key.
    KeyPress { key: String, modifiers: Vec<String> },
    /// Open application.
    OpenApp { name: String },
    /// Open URL.
    OpenUrl { url: String },
    /// Run command.
    RunCommand { command: String },
    /// Wait for duration.
    Wait { ms: u64 },
    /// Wait for element.
    WaitFor { selector: String, timeout_ms: u64 },
    /// Screenshot.
    Screenshot { name: String },
    /// AI prompt.
    AiPrompt { prompt: String },
    /// Extract value.
    Extract { selector: String, variable: String },
    /// Conditional.
    Conditional {
        condition: String,
        then_step: usize,
        else_step: Option<usize>,
    },
    /// Loop.
    Loop { times: usize, steps: Vec<usize> },
    /// Custom action.
    Custom {
        name: String,
        params: HashMap<String, String>,
    },
}

/// Mouse buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Recorded step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// Step ID.
    pub id: Uuid,
    /// Step number.
    pub number: usize,
    /// Action.
    pub action: ActionType,
    /// Description.
    pub description: Option<String>,
    /// Timestamp when recorded.
    pub recorded_at: DateTime<Utc>,
    /// Duration (ms).
    pub duration_ms: Option<u64>,
    /// Screenshot before.
    pub screenshot_before: Option<String>,
    /// Screenshot after.
    pub screenshot_after: Option<String>,
    /// Success on last run.
    pub last_success: Option<bool>,
}

impl Step {
    /// Create a new step.
    pub fn new(number: usize, action: ActionType) -> Self {
        Self {
            id: Uuid::new_v4(),
            number,
            action,
            description: None,
            recorded_at: Utc::now(),
            duration_ms: None,
            screenshot_before: None,
            screenshot_after: None,
            last_success: None,
        }
    }
}

/// Recording.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    /// Recording ID.
    pub id: Uuid,
    /// Name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Steps.
    pub steps: Vec<Step>,
    /// Variables.
    pub variables: HashMap<String, String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
    /// Updated at.
    pub updated_at: DateTime<Utc>,
    /// Total duration (ms).
    pub total_duration_ms: u64,
    /// Run count.
    pub run_count: u64,
    /// Success count.
    pub success_count: u64,
    /// Tags.
    pub tags: Vec<String>,
}

impl Recording {
    /// Create a new recording.
    pub fn new(name: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: None,
            steps: Vec::new(),
            variables: HashMap::new(),
            created_at: now,
            updated_at: now,
            total_duration_ms: 0,
            run_count: 0,
            success_count: 0,
            tags: Vec::new(),
        }
    }

    /// Add a step.
    pub fn add_step(&mut self, action: ActionType) {
        let number = self.steps.len() + 1;
        self.steps.push(Step::new(number, action));
        self.updated_at = Utc::now();
    }

    /// Get success rate.
    pub fn success_rate(&self) -> f64 {
        if self.run_count == 0 {
            0.0
        } else {
            self.success_count as f64 / self.run_count as f64
        }
    }
}

/// Playback result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackResult {
    /// Recording ID.
    pub recording_id: Uuid,
    /// Success.
    pub success: bool,
    /// Steps completed.
    pub steps_completed: usize,
    /// Total steps.
    pub total_steps: usize,
    /// Duration (ms).
    pub duration_ms: u64,
    /// Failed step.
    pub failed_step: Option<usize>,
    /// Error message.
    pub error: Option<String>,
    /// Variables after playback.
    pub variables: HashMap<String, String>,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Completed at.
    pub completed_at: DateTime<Utc>,
}

/// Recorder state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecorderState {
    Idle,
    Recording,
    Paused,
    Playing,
}

/// Recorder event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecorderEvent {
    /// Recording started.
    RecordingStarted(Uuid),
    /// Recording stopped.
    RecordingStopped(Uuid),
    /// Step recorded.
    StepRecorded { recording_id: Uuid, step: Step },
    /// Playback started.
    PlaybackStarted(Uuid),
    /// Playback completed.
    PlaybackCompleted(PlaybackResult),
    /// Step executed.
    StepExecuted {
        recording_id: Uuid,
        step_number: usize,
        success: bool,
    },
}

/// Recorder configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecorderConfig {
    /// Capture screenshots.
    pub capture_screenshots: bool,
    /// Screenshot on failure.
    pub screenshot_on_failure: bool,
    /// Default step delay (ms).
    pub default_step_delay: u64,
    /// Retry failed steps.
    pub retry_failed: bool,
    /// Max retries.
    pub max_retries: usize,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            capture_screenshots: true,
            screenshot_on_failure: true,
            default_step_delay: 100,
            retry_failed: true,
            max_retries: 3,
        }
    }
}

/// Trait for action executors.
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    /// Execute an action.
    async fn execute(
        &self,
        action: &ActionType,
        variables: &HashMap<String, String>,
    ) -> Result<Option<String>>;
}

/// Workflow recorder.
pub struct WorkflowRecorder<E: ActionExecutor> {
    config: RecorderConfig,
    executor: E,
    state: Arc<RwLock<RecorderState>>,
    current_recording: Arc<RwLock<Option<Recording>>>,
    recordings: Arc<RwLock<HashMap<Uuid, Recording>>>,
    event_tx: broadcast::Sender<RecorderEvent>,
}

impl<E: ActionExecutor> WorkflowRecorder<E> {
    /// Create a new recorder.
    pub fn new(config: RecorderConfig, executor: E) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        Self {
            config,
            executor,
            state: Arc::new(RwLock::new(RecorderState::Idle)),
            current_recording: Arc::new(RwLock::new(None)),
            recordings: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
        }
    }

    /// Subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<RecorderEvent> {
        self.event_tx.subscribe()
    }

    /// Get current state.
    pub async fn state(&self) -> RecorderState {
        *self.state.read().await
    }

    /// Start recording.
    pub async fn start_recording(&self, name: &str) -> Result<Uuid> {
        let mut state = self.state.write().await;
        if *state != RecorderState::Idle {
            return Err(RecorderError::AlreadyRecording);
        }

        let recording = Recording::new(name);
        let id = recording.id;

        *self.current_recording.write().await = Some(recording);
        *state = RecorderState::Recording;

        let _ = self.event_tx.send(RecorderEvent::RecordingStarted(id));

        Ok(id)
    }

    /// Record an action.
    pub async fn record_action(&self, action: ActionType) -> Result<Step> {
        let state = self.state.read().await;
        if *state != RecorderState::Recording {
            return Err(RecorderError::NotRecording);
        }
        drop(state);

        let mut current = self.current_recording.write().await;
        let recording = current.as_mut().ok_or(RecorderError::NotRecording)?;

        let number = recording.steps.len() + 1;
        let step = Step::new(number, action);

        recording.steps.push(step.clone());
        recording.updated_at = Utc::now();

        let _ = self.event_tx.send(RecorderEvent::StepRecorded {
            recording_id: recording.id,
            step: step.clone(),
        });

        Ok(step)
    }

    /// Pause recording.
    pub async fn pause_recording(&self) -> Result<()> {
        let mut state = self.state.write().await;
        if *state != RecorderState::Recording {
            return Err(RecorderError::NotRecording);
        }
        *state = RecorderState::Paused;
        Ok(())
    }

    /// Resume recording.
    pub async fn resume_recording(&self) -> Result<()> {
        let mut state = self.state.write().await;
        if *state != RecorderState::Paused {
            return Err(RecorderError::NotRecording);
        }
        *state = RecorderState::Recording;
        Ok(())
    }

    /// Stop recording.
    pub async fn stop_recording(&self) -> Result<Recording> {
        let mut state = self.state.write().await;
        if *state != RecorderState::Recording && *state != RecorderState::Paused {
            return Err(RecorderError::NotRecording);
        }

        let recording = self
            .current_recording
            .write()
            .await
            .take()
            .ok_or(RecorderError::NotRecording)?;

        // Calculate total duration
        let total_duration: u64 = recording.steps.iter().filter_map(|s| s.duration_ms).sum();

        let mut final_recording = recording;
        final_recording.total_duration_ms = total_duration;

        // Store recording
        self.recordings
            .write()
            .await
            .insert(final_recording.id, final_recording.clone());

        *state = RecorderState::Idle;

        let _ = self
            .event_tx
            .send(RecorderEvent::RecordingStopped(final_recording.id));

        Ok(final_recording)
    }

    /// Play a recording.
    pub async fn play(
        &self,
        id: Uuid,
        variables: Option<HashMap<String, String>>,
    ) -> Result<PlaybackResult> {
        let mut state = self.state.write().await;
        if *state != RecorderState::Idle {
            return Err(RecorderError::PlaybackFailed(
                "Recorder is busy".to_string(),
            ));
        }

        let recording = self
            .recordings
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| RecorderError::NotFound(id.to_string()))?;

        *state = RecorderState::Playing;
        drop(state);

        let _ = self.event_tx.send(RecorderEvent::PlaybackStarted(id));

        let started_at = Utc::now();
        let mut current_variables = variables.unwrap_or_default();

        // Merge recording variables
        for (k, v) in &recording.variables {
            current_variables
                .entry(k.clone())
                .or_insert_with(|| v.clone());
        }

        let mut steps_completed = 0;
        let mut failed_step = None;
        let mut error = None;

        for (i, step) in recording.steps.iter().enumerate() {
            let step_result = self.execute_step(step, &mut current_variables).await;

            let _ = self.event_tx.send(RecorderEvent::StepExecuted {
                recording_id: id,
                step_number: i + 1,
                success: step_result.is_ok(),
            });

            match step_result {
                Ok(output) => {
                    steps_completed += 1;
                    if let Some(value) = output {
                        // Store extracted value if action was Extract
                        if let ActionType::Extract { variable, .. } = &step.action {
                            current_variables.insert(variable.clone(), value);
                        }
                    }
                }
                Err(e) => {
                    failed_step = Some(i + 1);
                    error = Some(e.to_string());
                    break;
                }
            }

            // Add delay between steps
            if self.config.default_step_delay > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(
                    self.config.default_step_delay,
                ))
                .await;
            }
        }

        let completed_at = Utc::now();
        let success = failed_step.is_none();

        // Update recording stats
        {
            let mut recordings = self.recordings.write().await;
            if let Some(rec) = recordings.get_mut(&id) {
                rec.run_count += 1;
                if success {
                    rec.success_count += 1;
                }
            }
        }

        *self.state.write().await = RecorderState::Idle;

        let result = PlaybackResult {
            recording_id: id,
            success,
            steps_completed,
            total_steps: recording.steps.len(),
            duration_ms: (completed_at - started_at).num_milliseconds() as u64,
            failed_step,
            error,
            variables: current_variables,
            started_at,
            completed_at,
        };

        let _ = self
            .event_tx
            .send(RecorderEvent::PlaybackCompleted(result.clone()));

        Ok(result)
    }

    async fn execute_step(
        &self,
        step: &Step,
        variables: &mut HashMap<String, String>,
    ) -> Result<Option<String>> {
        let mut retries = 0;

        loop {
            match self.executor.execute(&step.action, variables).await {
                Ok(output) => return Ok(output),
                Err(e) => {
                    if self.config.retry_failed && retries < self.config.max_retries {
                        retries += 1;
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    } else {
                        return Err(RecorderError::StepFailed(e.to_string()));
                    }
                }
            }
        }
    }

    /// Get a recording.
    pub async fn get(&self, id: Uuid) -> Option<Recording> {
        self.recordings.read().await.get(&id).cloned()
    }

    /// List all recordings.
    pub async fn list(&self) -> Vec<Recording> {
        self.recordings.read().await.values().cloned().collect()
    }

    /// Delete a recording.
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        self.recordings
            .write()
            .await
            .remove(&id)
            .ok_or_else(|| RecorderError::NotFound(id.to_string()))?;
        Ok(())
    }

    /// Edit a step.
    pub async fn edit_step(
        &self,
        recording_id: Uuid,
        step_number: usize,
        action: ActionType,
    ) -> Result<()> {
        let mut recordings = self.recordings.write().await;
        let recording = recordings
            .get_mut(&recording_id)
            .ok_or_else(|| RecorderError::NotFound(recording_id.to_string()))?;

        if step_number == 0 || step_number > recording.steps.len() {
            return Err(RecorderError::StepFailed("Invalid step number".to_string()));
        }

        recording.steps[step_number - 1].action = action;
        recording.updated_at = Utc::now();

        Ok(())
    }

    /// Add variable to recording.
    pub async fn set_variable(&self, recording_id: Uuid, name: &str, value: &str) -> Result<()> {
        let mut recordings = self.recordings.write().await;
        let recording = recordings
            .get_mut(&recording_id)
            .ok_or_else(|| RecorderError::NotFound(recording_id.to_string()))?;

        recording
            .variables
            .insert(name.to_string(), value.to_string());
        recording.updated_at = Utc::now();

        Ok(())
    }

    /// Get statistics.
    pub async fn stats(&self) -> RecorderStats {
        let recordings = self.recordings.read().await;

        let total_runs: u64 = recordings.values().map(|r| r.run_count).sum();
        let total_successes: u64 = recordings.values().map(|r| r.success_count).sum();

        RecorderStats {
            total_recordings: recordings.len(),
            total_runs,
            total_successes,
            avg_success_rate: if total_runs > 0 {
                total_successes as f64 / total_runs as f64
            } else {
                0.0
            },
        }
    }
}

/// Recorder statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecorderStats {
    pub total_recordings: usize,
    pub total_runs: u64,
    pub total_successes: u64,
    pub avg_success_rate: f64,
}

/// Mock executor for testing.
pub struct MockExecutor;

#[async_trait]
impl ActionExecutor for MockExecutor {
    async fn execute(
        &self,
        action: &ActionType,
        _variables: &HashMap<String, String>,
    ) -> Result<Option<String>> {
        // Simulate execution
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        match action {
            ActionType::Extract { selector, .. } => {
                Ok(Some(format!("extracted_from_{}", selector)))
            }
            ActionType::Wait { ms } => {
                tokio::time::sleep(tokio::time::Duration::from_millis(*ms)).await;
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_start_stop_recording() {
        let recorder = WorkflowRecorder::new(RecorderConfig::default(), MockExecutor);

        let _id = recorder.start_recording("Test Recording").await.unwrap();
        assert_eq!(recorder.state().await, RecorderState::Recording);

        recorder
            .record_action(ActionType::Click {
                x: 100,
                y: 200,
                button: MouseButton::Left,
            })
            .await
            .unwrap();
        recorder
            .record_action(ActionType::TypeText {
                text: "Hello".to_string(),
            })
            .await
            .unwrap();

        let recording = recorder.stop_recording().await.unwrap();
        assert_eq!(recorder.state().await, RecorderState::Idle);
        assert_eq!(recording.steps.len(), 2);
    }

    #[tokio::test]
    async fn test_playback() {
        let recorder = WorkflowRecorder::new(RecorderConfig::default(), MockExecutor);

        recorder.start_recording("Test").await.unwrap();
        recorder
            .record_action(ActionType::Click {
                x: 0,
                y: 0,
                button: MouseButton::Left,
            })
            .await
            .unwrap();
        recorder
            .record_action(ActionType::Wait { ms: 10 })
            .await
            .unwrap();
        let recording = recorder.stop_recording().await.unwrap();

        let result = recorder.play(recording.id, None).await.unwrap();
        assert!(result.success);
        assert_eq!(result.steps_completed, 2);
    }

    #[tokio::test]
    async fn test_variables() {
        let recorder = WorkflowRecorder::new(RecorderConfig::default(), MockExecutor);

        recorder.start_recording("With Variables").await.unwrap();
        recorder
            .record_action(ActionType::Extract {
                selector: ".email".to_string(),
                variable: "user_email".to_string(),
            })
            .await
            .unwrap();
        let recording = recorder.stop_recording().await.unwrap();

        let result = recorder.play(recording.id, None).await.unwrap();
        assert!(result.variables.contains_key("user_email"));
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let recorder = WorkflowRecorder::new(RecorderConfig::default(), MockExecutor);

        recorder.start_recording("Pausable").await.unwrap();
        recorder
            .record_action(ActionType::Click {
                x: 0,
                y: 0,
                button: MouseButton::Left,
            })
            .await
            .unwrap();

        recorder.pause_recording().await.unwrap();
        assert_eq!(recorder.state().await, RecorderState::Paused);

        recorder.resume_recording().await.unwrap();
        assert_eq!(recorder.state().await, RecorderState::Recording);

        recorder
            .record_action(ActionType::Click {
                x: 10,
                y: 10,
                button: MouseButton::Left,
            })
            .await
            .unwrap();
        let recording = recorder.stop_recording().await.unwrap();

        assert_eq!(recording.steps.len(), 2);
    }

    #[tokio::test]
    async fn test_edit_step() {
        let recorder = WorkflowRecorder::new(RecorderConfig::default(), MockExecutor);

        recorder.start_recording("Editable").await.unwrap();
        recorder
            .record_action(ActionType::TypeText {
                text: "old".to_string(),
            })
            .await
            .unwrap();
        let recording = recorder.stop_recording().await.unwrap();

        recorder
            .edit_step(
                recording.id,
                1,
                ActionType::TypeText {
                    text: "new".to_string(),
                },
            )
            .await
            .unwrap();

        let updated = recorder.get(recording.id).await.unwrap();
        if let ActionType::TypeText { text } = &updated.steps[0].action {
            assert_eq!(text, "new");
        }
    }

    #[tokio::test]
    async fn test_list_recordings() {
        let recorder = WorkflowRecorder::new(RecorderConfig::default(), MockExecutor);

        recorder.start_recording("Recording 1").await.unwrap();
        recorder.stop_recording().await.unwrap();

        recorder.start_recording("Recording 2").await.unwrap();
        recorder.stop_recording().await.unwrap();

        let recordings = recorder.list().await;
        assert_eq!(recordings.len(), 2);
    }

    #[tokio::test]
    async fn test_stats() {
        let recorder = WorkflowRecorder::new(RecorderConfig::default(), MockExecutor);

        recorder.start_recording("Stats Test").await.unwrap();
        recorder
            .record_action(ActionType::Click {
                x: 0,
                y: 0,
                button: MouseButton::Left,
            })
            .await
            .unwrap();
        let recording = recorder.stop_recording().await.unwrap();

        recorder.play(recording.id, None).await.unwrap();
        recorder.play(recording.id, None).await.unwrap();

        let stats = recorder.stats().await;
        assert_eq!(stats.total_recordings, 1);
        assert_eq!(stats.total_runs, 2);
    }
}
