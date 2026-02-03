//! Interrupt and steering controls for drbot.
//!
//! Provides ability to interrupt AI responses and steer generation in real-time.
//!
//! # Features
//!
//! - Stop AI mid-response
//! - Redirect focus during generation
//! - Branch without losing context
//! - Real-time guidance

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, watch, RwLock};
use uuid::Uuid;

/// Interrupt result type.
pub type Result<T> = std::result::Result<T, InterruptError>;

/// Interrupt errors.
#[derive(Debug, thiserror::Error)]
pub enum InterruptError {
    #[error("No active generation")]
    NoActiveGeneration,
    #[error("Already interrupted")]
    AlreadyInterrupted,
    #[error("Cannot steer at this point")]
    CannotSteer,
}

/// Interrupt command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InterruptCommand {
    /// Stop immediately.
    Stop,
    /// Pause generation.
    Pause,
    /// Resume after pause.
    Resume,
    /// Steer in a new direction.
    Steer { guidance: String },
    /// Focus on specific topic.
    Focus { topic: String },
    /// Skip current section.
    Skip,
    /// Regenerate from checkpoint.
    Regenerate { checkpoint_id: Uuid },
    /// Branch conversation.
    Branch { from_position: usize },
}

/// Generation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationState {
    /// Not generating.
    Idle,
    /// Actively generating.
    Generating,
    /// Paused.
    Paused,
    /// Interrupted.
    Interrupted,
    /// Completed.
    Completed,
    /// Error.
    Error,
}

/// Generation checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Checkpoint ID.
    pub id: Uuid,
    /// Position in output (characters).
    pub position: usize,
    /// Content up to this point.
    pub content: String,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Token count.
    pub token_count: u64,
}

impl Checkpoint {
    /// Create a new checkpoint.
    pub fn new(content: &str, position: usize, token_count: u64) -> Self {
        Self {
            id: Uuid::new_v4(),
            position,
            content: content.to_string(),
            timestamp: Utc::now(),
            token_count,
        }
    }
}

/// Active generation info.
#[derive(Debug, Clone)]
pub struct ActiveGeneration {
    /// Generation ID.
    pub id: Uuid,
    /// Session ID.
    pub session_id: String,
    /// Current state.
    pub state: GenerationState,
    /// Content generated so far.
    pub content: String,
    /// Checkpoints.
    pub checkpoints: Vec<Checkpoint>,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Last activity.
    pub last_activity: DateTime<Utc>,
}

impl ActiveGeneration {
    /// Create a new active generation.
    pub fn new(session_id: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            session_id: session_id.to_string(),
            state: GenerationState::Generating,
            content: String::new(),
            checkpoints: Vec::new(),
            started_at: now,
            last_activity: now,
        }
    }

    /// Add content.
    pub fn append(&mut self, text: &str) {
        self.content.push_str(text);
        self.last_activity = Utc::now();
    }

    /// Create checkpoint.
    pub fn checkpoint(&mut self, token_count: u64) -> Checkpoint {
        let cp = Checkpoint::new(&self.content, self.content.len(), token_count);
        self.checkpoints.push(cp.clone());
        cp
    }

    /// Rollback to checkpoint.
    pub fn rollback(&mut self, checkpoint_id: Uuid) -> bool {
        if let Some(cp) = self.checkpoints.iter().find(|c| c.id == checkpoint_id) {
            let content = cp.content.clone();
            let timestamp = cp.timestamp;
            self.content = content;
            self.checkpoints.retain(|c| c.timestamp <= timestamp);
            true
        } else {
            false
        }
    }
}

/// Interrupt event.
#[derive(Debug, Clone)]
pub enum InterruptEvent {
    /// Generation started.
    Started { id: Uuid },
    /// Content added.
    ContentAdded { text: String },
    /// Checkpoint created.
    Checkpoint { checkpoint: Checkpoint },
    /// State changed.
    StateChanged {
        from: GenerationState,
        to: GenerationState,
    },
    /// Interrupted.
    Interrupted { command: InterruptCommand },
    /// Generation completed.
    Completed { content: String },
    /// Error occurred.
    Error { error: String },
}

/// Interrupt controller.
pub struct InterruptController {
    generation: Arc<RwLock<Option<ActiveGeneration>>>,
    state_sender: watch::Sender<GenerationState>,
    state_receiver: watch::Receiver<GenerationState>,
    event_sender: broadcast::Sender<InterruptEvent>,
    interrupt_sender: broadcast::Sender<InterruptCommand>,
}

impl InterruptController {
    /// Create a new interrupt controller.
    pub fn new() -> Self {
        let (state_sender, state_receiver) = watch::channel(GenerationState::Idle);
        let (event_sender, _) = broadcast::channel(256);
        let (interrupt_sender, _) = broadcast::channel(16);

        Self {
            generation: Arc::new(RwLock::new(None)),
            state_sender,
            state_receiver,
            event_sender,
            interrupt_sender,
        }
    }

    /// Start a new generation.
    pub async fn start_generation(&self, session_id: &str) -> Uuid {
        let gen = ActiveGeneration::new(session_id);
        let id = gen.id;

        *self.generation.write().await = Some(gen);
        let _ = self.state_sender.send(GenerationState::Generating);
        let _ = self.event_sender.send(InterruptEvent::Started { id });

        id
    }

    /// Add content to generation.
    pub async fn add_content(&self, text: &str) {
        let mut gen_lock = self.generation.write().await;
        if let Some(gen) = gen_lock.as_mut() {
            gen.append(text);
            let _ = self.event_sender.send(InterruptEvent::ContentAdded {
                text: text.to_string(),
            });
        }
    }

    /// Create a checkpoint.
    pub async fn create_checkpoint(&self, token_count: u64) -> Option<Checkpoint> {
        let mut gen_lock = self.generation.write().await;
        if let Some(gen) = gen_lock.as_mut() {
            let cp = gen.checkpoint(token_count);
            let _ = self.event_sender.send(InterruptEvent::Checkpoint {
                checkpoint: cp.clone(),
            });
            Some(cp)
        } else {
            None
        }
    }

    /// Get current state.
    pub fn state(&self) -> GenerationState {
        *self.state_receiver.borrow()
    }

    /// Subscribe to state changes.
    pub fn watch_state(&self) -> watch::Receiver<GenerationState> {
        self.state_receiver.clone()
    }

    /// Subscribe to events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<InterruptEvent> {
        self.event_sender.subscribe()
    }

    /// Subscribe to interrupt commands (for generation loop).
    pub fn subscribe_interrupts(&self) -> broadcast::Receiver<InterruptCommand> {
        self.interrupt_sender.subscribe()
    }

    /// Send an interrupt command.
    pub async fn interrupt(&self, command: InterruptCommand) -> Result<()> {
        let gen_lock = self.generation.read().await;
        if gen_lock.is_none() {
            return Err(InterruptError::NoActiveGeneration);
        }
        drop(gen_lock);

        let _ = self.interrupt_sender.send(command.clone());
        let _ = self.event_sender.send(InterruptEvent::Interrupted {
            command: command.clone(),
        });

        // Handle state changes
        match &command {
            InterruptCommand::Stop => {
                self.set_state(GenerationState::Interrupted).await;
            }
            InterruptCommand::Pause => {
                self.set_state(GenerationState::Paused).await;
            }
            InterruptCommand::Resume => {
                self.set_state(GenerationState::Generating).await;
            }
            _ => {}
        }

        Ok(())
    }

    /// Stop generation.
    pub async fn stop(&self) -> Result<String> {
        self.interrupt(InterruptCommand::Stop).await?;

        let gen_lock = self.generation.read().await;
        Ok(gen_lock
            .as_ref()
            .map(|g| g.content.clone())
            .unwrap_or_default())
    }

    /// Pause generation.
    pub async fn pause(&self) -> Result<()> {
        self.interrupt(InterruptCommand::Pause).await
    }

    /// Resume generation.
    pub async fn resume(&self) -> Result<()> {
        self.interrupt(InterruptCommand::Resume).await
    }

    /// Steer generation.
    pub async fn steer(&self, guidance: &str) -> Result<()> {
        self.interrupt(InterruptCommand::Steer {
            guidance: guidance.to_string(),
        })
        .await
    }

    /// Focus on topic.
    pub async fn focus(&self, topic: &str) -> Result<()> {
        self.interrupt(InterruptCommand::Focus {
            topic: topic.to_string(),
        })
        .await
    }

    /// Regenerate from checkpoint.
    pub async fn regenerate(&self, checkpoint_id: Uuid) -> Result<()> {
        let mut gen_lock = self.generation.write().await;
        if let Some(gen) = gen_lock.as_mut() {
            if gen.rollback(checkpoint_id) {
                drop(gen_lock);
                self.interrupt(InterruptCommand::Regenerate { checkpoint_id })
                    .await?;
                self.set_state(GenerationState::Generating).await;
                Ok(())
            } else {
                Err(InterruptError::CannotSteer)
            }
        } else {
            Err(InterruptError::NoActiveGeneration)
        }
    }

    /// Complete generation.
    pub async fn complete(&self) -> String {
        let gen_lock = self.generation.read().await;
        let content = gen_lock
            .as_ref()
            .map(|g| g.content.clone())
            .unwrap_or_default();

        let _ = self.event_sender.send(InterruptEvent::Completed {
            content: content.clone(),
        });
        let _ = self.state_sender.send(GenerationState::Completed);

        content
    }

    /// End generation and cleanup.
    pub async fn end(&self) {
        *self.generation.write().await = None;
        let _ = self.state_sender.send(GenerationState::Idle);
    }

    async fn set_state(&self, state: GenerationState) {
        let from = *self.state_receiver.borrow();
        let _ = self.state_sender.send(state);
        let _ = self
            .event_sender
            .send(InterruptEvent::StateChanged { from, to: state });
    }

    /// Get current generation info.
    pub async fn current(&self) -> Option<ActiveGeneration> {
        self.generation.read().await.clone()
    }

    /// Get checkpoints.
    pub async fn checkpoints(&self) -> Vec<Checkpoint> {
        self.generation
            .read()
            .await
            .as_ref()
            .map(|g| g.checkpoints.clone())
            .unwrap_or_default()
    }
}

impl Default for InterruptController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_interrupt_controller() {
        let controller = InterruptController::new();

        let id = controller.start_generation("session1").await;
        assert_eq!(controller.state(), GenerationState::Generating);

        controller.add_content("Hello ").await;
        controller.add_content("World").await;

        let content = controller.stop().await.unwrap();
        assert_eq!(content, "Hello World");
        assert_eq!(controller.state(), GenerationState::Interrupted);
    }

    #[tokio::test]
    async fn test_checkpoints() {
        let controller = InterruptController::new();

        controller.start_generation("session1").await;
        controller.add_content("Part 1. ").await;

        let cp = controller.create_checkpoint(10).await.unwrap();

        controller.add_content("Part 2. ").await;

        controller.regenerate(cp.id).await.unwrap();

        let current = controller.current().await.unwrap();
        assert_eq!(current.content, "Part 1. ");
    }

    #[tokio::test]
    async fn test_pause_resume() {
        let controller = InterruptController::new();

        controller.start_generation("session1").await;

        controller.pause().await.unwrap();
        assert_eq!(controller.state(), GenerationState::Paused);

        controller.resume().await.unwrap();
        assert_eq!(controller.state(), GenerationState::Generating);
    }
}
