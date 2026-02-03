//! Continuous conversation mode for voice interface.
//!
//! Manages multi-turn voice conversations with context.

use crate::{VoiceConfig, VoiceError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

/// Voice conversation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationState {
    /// Idle, waiting for wake word.
    Idle,
    /// Listening for user input.
    Listening,
    /// Processing user input.
    Processing,
    /// AI is responding (speaking).
    Speaking,
    /// Waiting for follow-up.
    WaitingFollowUp,
    /// Conversation ended.
    Ended,
}

/// Voice conversation turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceTurn {
    /// Turn ID.
    pub id: Uuid,
    /// Role (user/assistant).
    pub role: String,
    /// Transcribed/synthesized text.
    pub text: String,
    /// Audio duration in seconds.
    pub duration_secs: f32,
    /// Confidence of transcription.
    pub confidence: Option<f32>,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Voice conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConversation {
    /// Conversation ID.
    pub id: Uuid,
    /// Session ID.
    pub session_id: String,
    /// Conversation turns.
    pub turns: Vec<VoiceTurn>,
    /// Current state.
    pub state: ConversationState,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Last activity.
    pub last_activity: DateTime<Utc>,
}

impl VoiceConversation {
    /// Create a new voice conversation.
    pub fn new(session_id: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            session_id: session_id.to_string(),
            turns: Vec::new(),
            state: ConversationState::Idle,
            started_at: now,
            last_activity: now,
        }
    }

    /// Add a user turn.
    pub fn add_user_turn(&mut self, text: &str, confidence: f32, duration_secs: f32) {
        self.turns.push(VoiceTurn {
            id: Uuid::new_v4(),
            role: "user".to_string(),
            text: text.to_string(),
            duration_secs,
            confidence: Some(confidence),
            timestamp: Utc::now(),
        });
        self.last_activity = Utc::now();
    }

    /// Add an assistant turn.
    pub fn add_assistant_turn(&mut self, text: &str, duration_secs: f32) {
        self.turns.push(VoiceTurn {
            id: Uuid::new_v4(),
            role: "assistant".to_string(),
            text: text.to_string(),
            duration_secs,
            confidence: None,
            timestamp: Utc::now(),
        });
        self.last_activity = Utc::now();
    }

    /// Get conversation history as text.
    pub fn history_text(&self) -> String {
        self.turns
            .iter()
            .map(|t| format!("{}: {}", t.role, t.text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get last N turns.
    pub fn last_turns(&self, n: usize) -> Vec<&VoiceTurn> {
        self.turns.iter().rev().take(n).collect()
    }

    /// Total duration of conversation.
    pub fn total_duration_secs(&self) -> f32 {
        self.turns.iter().map(|t| t.duration_secs).sum()
    }
}

/// Conversation event.
#[derive(Debug, Clone)]
pub enum ConversationEvent {
    /// State changed.
    StateChanged {
        from: ConversationState,
        to: ConversationState,
    },
    /// User spoke.
    UserSpoke { text: String, confidence: f32 },
    /// Assistant speaking.
    AssistantSpeaking { text: String },
    /// Conversation ended.
    Ended { reason: EndReason },
    /// Interrupted.
    Interrupted,
    /// Error occurred.
    Error { error: String },
}

/// Reason for conversation ending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// User said goodbye.
    UserEnded,
    /// Silence timeout.
    Timeout,
    /// System ended.
    SystemEnded,
    /// Error.
    Error,
}

/// Conversation manager configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationConfig {
    /// Silence timeout between turns (seconds).
    pub silence_timeout_secs: u32,
    /// Maximum conversation duration (seconds).
    pub max_duration_secs: u32,
    /// Maximum turns per conversation.
    pub max_turns: usize,
    /// Enable interruption detection.
    pub enable_interruption: bool,
    /// Phrases that end conversation.
    pub end_phrases: Vec<String>,
    /// Enable context carryover.
    pub enable_context: bool,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            silence_timeout_secs: 10,
            max_duration_secs: 300,
            max_turns: 50,
            enable_interruption: true,
            end_phrases: vec![
                "goodbye".to_string(),
                "bye".to_string(),
                "that's all".to_string(),
                "thanks, bye".to_string(),
                "end conversation".to_string(),
            ],
            enable_context: true,
        }
    }
}

/// Voice conversation manager.
pub struct VoiceConversationManager {
    config: ConversationConfig,
    current: Arc<RwLock<Option<VoiceConversation>>>,
    event_sender: broadcast::Sender<ConversationEvent>,
}

impl VoiceConversationManager {
    /// Create a new conversation manager.
    pub fn new(config: ConversationConfig) -> Self {
        let (sender, _) = broadcast::channel(32);
        Self {
            config,
            current: Arc::new(RwLock::new(None)),
            event_sender: sender,
        }
    }

    /// Start a new conversation.
    pub async fn start(&self, session_id: &str) -> Uuid {
        let mut conv = VoiceConversation::new(session_id);
        conv.state = ConversationState::Listening;
        let id = conv.id;

        let _ = self.event_sender.send(ConversationEvent::StateChanged {
            from: ConversationState::Idle,
            to: ConversationState::Listening,
        });

        *self.current.write().await = Some(conv);
        id
    }

    /// End the current conversation.
    pub async fn end(&self, reason: EndReason) {
        let mut current = self.current.write().await;
        if let Some(conv) = current.as_mut() {
            conv.state = ConversationState::Ended;
            let _ = self.event_sender.send(ConversationEvent::Ended { reason });
        }
        *current = None;
    }

    /// Get the current conversation.
    pub async fn current(&self) -> Option<VoiceConversation> {
        self.current.read().await.clone()
    }

    /// Subscribe to conversation events.
    pub fn subscribe(&self) -> broadcast::Receiver<ConversationEvent> {
        self.event_sender.subscribe()
    }

    /// Process user speech.
    pub async fn user_spoke(&self, text: &str, confidence: f32, duration_secs: f32) -> bool {
        let mut current = self.current.write().await;
        if let Some(conv) = current.as_mut() {
            // Check for end phrases
            let text_lower = text.to_lowercase();
            for phrase in &self.config.end_phrases {
                if text_lower.contains(phrase) {
                    conv.state = ConversationState::Ended;
                    let _ = self.event_sender.send(ConversationEvent::Ended {
                        reason: EndReason::UserEnded,
                    });
                    return false;
                }
            }

            conv.add_user_turn(text, confidence, duration_secs);
            conv.state = ConversationState::Processing;

            let _ = self.event_sender.send(ConversationEvent::UserSpoke {
                text: text.to_string(),
                confidence,
            });

            true
        } else {
            false
        }
    }

    /// Record assistant response.
    pub async fn assistant_speaking(&self, text: &str, duration_secs: f32) {
        let mut current = self.current.write().await;
        if let Some(conv) = current.as_mut() {
            conv.add_assistant_turn(text, duration_secs);
            conv.state = ConversationState::Speaking;

            let _ = self
                .event_sender
                .send(ConversationEvent::AssistantSpeaking {
                    text: text.to_string(),
                });
        }
    }

    /// Mark assistant done speaking.
    pub async fn assistant_done(&self) {
        let mut current = self.current.write().await;
        if let Some(conv) = current.as_mut() {
            let from = conv.state;
            conv.state = ConversationState::WaitingFollowUp;

            let _ = self.event_sender.send(ConversationEvent::StateChanged {
                from,
                to: ConversationState::WaitingFollowUp,
            });
        }
    }

    /// Handle interruption.
    pub async fn interrupted(&self) {
        if !self.config.enable_interruption {
            return;
        }

        let mut current = self.current.write().await;
        if let Some(conv) = current.as_mut() {
            if conv.state == ConversationState::Speaking {
                conv.state = ConversationState::Listening;
                let _ = self.event_sender.send(ConversationEvent::Interrupted);
            }
        }
    }

    /// Check if conversation should timeout.
    pub async fn check_timeout(&self) -> bool {
        let current = self.current.read().await;
        if let Some(conv) = current.as_ref() {
            let elapsed = Utc::now()
                .signed_duration_since(conv.last_activity)
                .num_seconds() as u32;

            if elapsed > self.config.silence_timeout_secs {
                return true;
            }

            let total = Utc::now()
                .signed_duration_since(conv.started_at)
                .num_seconds() as u32;

            if total > self.config.max_duration_secs {
                return true;
            }

            if conv.turns.len() >= self.config.max_turns {
                return true;
            }
        }

        false
    }

    /// Get conversation history for context.
    pub async fn get_context(&self) -> Option<String> {
        if !self.config.enable_context {
            return None;
        }

        self.current.read().await.as_ref().map(|c| c.history_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_voice_conversation() {
        let mut conv = VoiceConversation::new("session1");

        conv.add_user_turn("Hello", 0.95, 1.5);
        conv.add_assistant_turn("Hi there!", 2.0);

        assert_eq!(conv.turns.len(), 2);
        assert_eq!(conv.total_duration_secs(), 3.5);
    }

    #[tokio::test]
    async fn test_conversation_manager() {
        let config = ConversationConfig::default();
        let manager = VoiceConversationManager::new(config);

        let id = manager.start("session1").await;
        assert!(manager.current().await.is_some());

        let continued = manager.user_spoke("Hello there", 0.9, 1.0).await;
        assert!(continued);

        manager.assistant_speaking("Hi! How can I help?", 2.0).await;
        manager.assistant_done().await;

        // Test end phrase
        let continued = manager.user_spoke("goodbye", 0.9, 0.5).await;
        assert!(!continued);
    }

    #[tokio::test]
    async fn test_context() {
        let config = ConversationConfig::default();
        let manager = VoiceConversationManager::new(config);

        manager.start("session1").await;
        manager.user_spoke("What's the weather?", 0.9, 1.5).await;
        manager.assistant_speaking("It's sunny today.", 2.0).await;

        let context = manager.get_context().await;
        assert!(context.is_some());
        assert!(context.unwrap().contains("weather"));
    }
}
