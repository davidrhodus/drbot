//! AI reasoning inspection and debugging.
//!
//! This crate provides:
//! - Reasoning chain visualization
//! - Token attribution
//! - Attention analysis
//! - Debug logging

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Debug errors.
#[derive(Debug, Error)]
pub enum DebugError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),

    #[error("Capture error: {0}")]
    CaptureError(String),
}

/// Result type for debug operations.
pub type Result<T> = std::result::Result<T, DebugError>;

/// Debug session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugSession {
    /// Session identifier.
    pub id: String,
    /// Session name.
    pub name: String,
    /// Events captured.
    pub events: Vec<DebugEvent>,
    /// Started at.
    pub started_at: DateTime<Utc>,
    /// Ended at.
    pub ended_at: Option<DateTime<Utc>>,
    /// Is active.
    pub is_active: bool,
    /// Metadata.
    pub metadata: HashMap<String, String>,
}

/// A debug event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugEvent {
    /// Event identifier.
    pub id: String,
    /// Event type.
    pub event_type: EventType,
    /// Timestamp.
    pub timestamp: DateTime<Utc>,
    /// Duration (if applicable).
    pub duration_ms: Option<u64>,
    /// Data.
    pub data: EventData,
    /// Parent event ID.
    pub parent_id: Option<String>,
    /// Depth in call stack.
    pub depth: usize,
}

/// Event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    /// API call.
    ApiCall,
    /// Token generation.
    TokenGeneration,
    /// Tool call.
    ToolCall,
    /// Reasoning step.
    ReasoningStep,
    /// Context lookup.
    ContextLookup,
    /// Decision point.
    Decision,
    /// Error.
    Error,
    /// Custom event.
    Custom,
}

/// Event data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventData {
    /// API call data.
    ApiCall(ApiCallData),
    /// Token data.
    Token(TokenData),
    /// Tool call data.
    ToolCall(ToolCallData),
    /// Reasoning data.
    Reasoning(ReasoningData),
    /// Context data.
    Context(ContextData),
    /// Decision data.
    Decision(DecisionData),
    /// Error data.
    Error(ErrorData),
    /// Generic data.
    Generic(HashMap<String, String>),
}

/// API call data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCallData {
    /// Provider.
    pub provider: String,
    /// Model.
    pub model: String,
    /// Request size.
    pub request_tokens: usize,
    /// Response size.
    pub response_tokens: usize,
    /// Latency.
    pub latency_ms: u64,
    /// Cost.
    pub cost: f64,
}

/// Token data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    /// Token.
    pub token: String,
    /// Token ID.
    pub token_id: usize,
    /// Log probability.
    pub logprob: f64,
    /// Top alternatives.
    pub top_alternatives: Vec<(String, f64)>,
    /// Position.
    pub position: usize,
}

/// Tool call data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallData {
    /// Tool name.
    pub tool_name: String,
    /// Arguments.
    pub arguments: HashMap<String, String>,
    /// Result.
    pub result: Option<String>,
    /// Success.
    pub success: bool,
}

/// Reasoning data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningData {
    /// Step number.
    pub step: usize,
    /// Reasoning text.
    pub reasoning: String,
    /// Confidence.
    pub confidence: f64,
    /// Contributing factors.
    pub factors: Vec<ReasoningFactor>,
}

/// A factor contributing to reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningFactor {
    /// Factor name.
    pub name: String,
    /// Weight/importance.
    pub weight: f64,
    /// Evidence.
    pub evidence: String,
}

/// Context data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextData {
    /// Source.
    pub source: String,
    /// Relevance score.
    pub relevance: f64,
    /// Content preview.
    pub preview: String,
    /// Tokens used.
    pub tokens: usize,
}

/// Decision data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionData {
    /// Decision point.
    pub decision_point: String,
    /// Options considered.
    pub options: Vec<DecisionOption>,
    /// Chosen option.
    pub chosen: String,
    /// Rationale.
    pub rationale: String,
}

/// A decision option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOption {
    /// Option name.
    pub name: String,
    /// Score.
    pub score: f64,
    /// Pros.
    pub pros: Vec<String>,
    /// Cons.
    pub cons: Vec<String>,
}

/// Error data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorData {
    /// Error type.
    pub error_type: String,
    /// Message.
    pub message: String,
    /// Stack trace.
    pub stack_trace: Option<String>,
    /// Recoverable.
    pub recoverable: bool,
}

/// Token attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAttribution {
    /// Output token.
    pub output_token: String,
    /// Position.
    pub position: usize,
    /// Input token attributions.
    pub attributions: Vec<Attribution>,
}

/// Attribution to input.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribution {
    /// Input token.
    pub input_token: String,
    /// Input position.
    pub input_position: usize,
    /// Attribution score.
    pub score: f64,
    /// Source (prompt, context, etc).
    pub source: String,
}

/// Reasoning chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningChain {
    /// Chain identifier.
    pub id: String,
    /// Steps.
    pub steps: Vec<ReasoningStep>,
    /// Conclusion.
    pub conclusion: String,
    /// Overall confidence.
    pub confidence: f64,
}

/// A reasoning step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// Step number.
    pub step: usize,
    /// Type.
    pub step_type: ReasoningStepType,
    /// Content.
    pub content: String,
    /// Inputs.
    pub inputs: Vec<String>,
    /// Output.
    pub output: String,
    /// Confidence.
    pub confidence: f64,
}

/// Reasoning step types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasoningStepType {
    /// Observation.
    Observation,
    /// Inference.
    Inference,
    /// Deduction.
    Deduction,
    /// Hypothesis.
    Hypothesis,
    /// Verification.
    Verification,
    /// Conclusion.
    Conclusion,
}

/// Debug analysis provider.
#[async_trait]
pub trait DebugAnalyzer: Send + Sync {
    /// Analyze token attributions.
    async fn analyze_attributions(
        &self,
        input: &str,
        output: &str,
    ) -> Result<Vec<TokenAttribution>>;

    /// Extract reasoning chain.
    async fn extract_reasoning(&self, response: &str) -> Result<ReasoningChain>;

    /// Analyze decision points.
    async fn analyze_decisions(&self, events: &[DebugEvent]) -> Result<Vec<DecisionData>>;
}

/// The debug engine.
pub struct DebugEngine {
    /// Analyzer.
    analyzer: Arc<dyn DebugAnalyzer>,
    /// Active sessions.
    sessions: Arc<RwLock<HashMap<String, DebugSession>>>,
    /// Global event log.
    event_log: Arc<RwLock<Vec<DebugEvent>>>,
    /// Is debugging enabled.
    enabled: Arc<RwLock<bool>>,
}

impl DebugEngine {
    /// Create a new debug engine.
    pub fn new(analyzer: Arc<dyn DebugAnalyzer>) -> Self {
        Self {
            analyzer,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            event_log: Arc::new(RwLock::new(Vec::new())),
            enabled: Arc::new(RwLock::new(true)),
        }
    }

    /// Enable/disable debugging.
    pub async fn set_enabled(&self, enabled: bool) {
        let mut e = self.enabled.write().await;
        *e = enabled;
    }

    /// Check if debugging is enabled.
    pub async fn is_enabled(&self) -> bool {
        *self.enabled.read().await
    }

    /// Start a debug session.
    pub async fn start_session(&self, name: &str) -> Result<String> {
        let session = DebugSession {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            events: Vec::new(),
            started_at: Utc::now(),
            ended_at: None,
            is_active: true,
            metadata: HashMap::new(),
        };

        let id = session.id.clone();
        let mut sessions = self.sessions.write().await;
        sessions.insert(id.clone(), session);

        Ok(id)
    }

    /// End a debug session.
    pub async fn end_session(&self, session_id: &str) -> Result<DebugSession> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| DebugError::SessionNotFound(session_id.to_string()))?;

        session.is_active = false;
        session.ended_at = Some(Utc::now());

        Ok(session.clone())
    }

    /// Log an event.
    pub async fn log_event(
        &self,
        session_id: Option<&str>,
        event_type: EventType,
        data: EventData,
        parent_id: Option<String>,
    ) -> Result<String> {
        if !*self.enabled.read().await {
            return Ok(String::new());
        }

        let event = DebugEvent {
            id: Uuid::new_v4().to_string(),
            event_type,
            timestamp: Utc::now(),
            duration_ms: None,
            data,
            parent_id: parent_id.clone(),
            depth: 0,
        };

        let id = event.id.clone();

        // Add to session if specified
        if let Some(sid) = session_id {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(sid) {
                session.events.push(event.clone());
            }
        }

        // Add to global log
        let mut log = self.event_log.write().await;
        log.push(event);
        if log.len() > 100000 {
            log.drain(0..10000);
        }

        Ok(id)
    }

    /// Log API call.
    pub async fn log_api_call(
        &self,
        session_id: Option<&str>,
        provider: &str,
        model: &str,
        request_tokens: usize,
        response_tokens: usize,
        latency_ms: u64,
        cost: f64,
    ) -> Result<String> {
        self.log_event(
            session_id,
            EventType::ApiCall,
            EventData::ApiCall(ApiCallData {
                provider: provider.to_string(),
                model: model.to_string(),
                request_tokens,
                response_tokens,
                latency_ms,
                cost,
            }),
            None,
        )
        .await
    }

    /// Log tool call.
    pub async fn log_tool_call(
        &self,
        session_id: Option<&str>,
        tool_name: &str,
        arguments: HashMap<String, String>,
        result: Option<String>,
        success: bool,
    ) -> Result<String> {
        self.log_event(
            session_id,
            EventType::ToolCall,
            EventData::ToolCall(ToolCallData {
                tool_name: tool_name.to_string(),
                arguments,
                result,
                success,
            }),
            None,
        )
        .await
    }

    /// Log reasoning step.
    pub async fn log_reasoning(
        &self,
        session_id: Option<&str>,
        step: usize,
        reasoning: &str,
        confidence: f64,
    ) -> Result<String> {
        self.log_event(
            session_id,
            EventType::ReasoningStep,
            EventData::Reasoning(ReasoningData {
                step,
                reasoning: reasoning.to_string(),
                confidence,
                factors: Vec::new(),
            }),
            None,
        )
        .await
    }

    /// Log error.
    pub async fn log_error(
        &self,
        session_id: Option<&str>,
        error_type: &str,
        message: &str,
        recoverable: bool,
    ) -> Result<String> {
        self.log_event(
            session_id,
            EventType::Error,
            EventData::Error(ErrorData {
                error_type: error_type.to_string(),
                message: message.to_string(),
                stack_trace: None,
                recoverable,
            }),
            None,
        )
        .await
    }

    /// Analyze token attributions.
    pub async fn analyze_attributions(
        &self,
        input: &str,
        output: &str,
    ) -> Result<Vec<TokenAttribution>> {
        self.analyzer.analyze_attributions(input, output).await
    }

    /// Extract reasoning chain.
    pub async fn extract_reasoning(&self, response: &str) -> Result<ReasoningChain> {
        self.analyzer.extract_reasoning(response).await
    }

    /// Get session.
    pub async fn get_session(&self, session_id: &str) -> Option<DebugSession> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// Get session events.
    pub async fn get_events(
        &self,
        session_id: &str,
        event_type: Option<EventType>,
    ) -> Result<Vec<DebugEvent>> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| DebugError::SessionNotFound(session_id.to_string()))?;

        let events = match event_type {
            Some(et) => session
                .events
                .iter()
                .filter(|e| e.event_type == et)
                .cloned()
                .collect(),
            None => session.events.clone(),
        };

        Ok(events)
    }

    /// Get global event log.
    pub async fn get_global_log(
        &self,
        limit: usize,
        event_type: Option<EventType>,
    ) -> Vec<DebugEvent> {
        let log = self.event_log.read().await;
        let iter = log.iter().rev();

        match event_type {
            Some(et) => iter
                .filter(|e| e.event_type == et)
                .take(limit)
                .cloned()
                .collect(),
            None => iter.take(limit).cloned().collect(),
        }
    }

    /// Get session summary.
    pub async fn get_summary(&self, session_id: &str) -> Result<SessionSummary> {
        let sessions = self.sessions.read().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| DebugError::SessionNotFound(session_id.to_string()))?;

        let total_events = session.events.len();
        let api_calls = session
            .events
            .iter()
            .filter(|e| e.event_type == EventType::ApiCall)
            .count();
        let tool_calls = session
            .events
            .iter()
            .filter(|e| e.event_type == EventType::ToolCall)
            .count();
        let errors = session
            .events
            .iter()
            .filter(|e| e.event_type == EventType::Error)
            .count();

        let total_latency: u64 = session.events.iter().filter_map(|e| e.duration_ms).sum();

        let total_cost: f64 = session
            .events
            .iter()
            .filter_map(|e| match &e.data {
                EventData::ApiCall(d) => Some(d.cost),
                _ => None,
            })
            .sum();

        let total_tokens: usize = session
            .events
            .iter()
            .filter_map(|e| match &e.data {
                EventData::ApiCall(d) => Some(d.request_tokens + d.response_tokens),
                _ => None,
            })
            .sum();

        Ok(SessionSummary {
            session_id: session_id.to_string(),
            total_events,
            api_calls,
            tool_calls,
            errors,
            total_latency_ms: total_latency,
            total_cost,
            total_tokens,
            duration_ms: session
                .ended_at
                .map(|e| (e - session.started_at).num_milliseconds() as u64)
                .unwrap_or(0),
        })
    }

    /// List active sessions.
    pub async fn list_sessions(&self, include_inactive: bool) -> Vec<DebugSession> {
        let sessions = self.sessions.read().await;
        sessions
            .values()
            .filter(|s| include_inactive || s.is_active)
            .cloned()
            .collect()
    }

    /// Clear old sessions.
    pub async fn clear_inactive_sessions(&self) {
        let mut sessions = self.sessions.write().await;
        sessions.retain(|_, s| s.is_active);
    }
}

/// Session summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Session ID.
    pub session_id: String,
    /// Total events.
    pub total_events: usize,
    /// API calls.
    pub api_calls: usize,
    /// Tool calls.
    pub tool_calls: usize,
    /// Errors.
    pub errors: usize,
    /// Total latency.
    pub total_latency_ms: u64,
    /// Total cost.
    pub total_cost: f64,
    /// Total tokens.
    pub total_tokens: usize,
    /// Session duration.
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAnalyzer;

    #[async_trait]
    impl DebugAnalyzer for MockAnalyzer {
        async fn analyze_attributions(
            &self,
            input: &str,
            output: &str,
        ) -> Result<Vec<TokenAttribution>> {
            let output_tokens: Vec<&str> = output.split_whitespace().collect();
            let input_tokens: Vec<&str> = input.split_whitespace().collect();

            Ok(output_tokens
                .iter()
                .enumerate()
                .map(|(i, token)| TokenAttribution {
                    output_token: token.to_string(),
                    position: i,
                    attributions: input_tokens
                        .iter()
                        .enumerate()
                        .map(|(j, inp)| Attribution {
                            input_token: inp.to_string(),
                            input_position: j,
                            score: 1.0 / (input_tokens.len() as f64),
                            source: "prompt".to_string(),
                        })
                        .collect(),
                })
                .collect())
        }

        async fn extract_reasoning(&self, response: &str) -> Result<ReasoningChain> {
            Ok(ReasoningChain {
                id: Uuid::new_v4().to_string(),
                steps: vec![
                    ReasoningStep {
                        step: 1,
                        step_type: ReasoningStepType::Observation,
                        content: "Observed input".to_string(),
                        inputs: vec!["input".to_string()],
                        output: "observation".to_string(),
                        confidence: 0.9,
                    },
                    ReasoningStep {
                        step: 2,
                        step_type: ReasoningStepType::Conclusion,
                        content: response.to_string(),
                        inputs: vec!["observation".to_string()],
                        output: response.to_string(),
                        confidence: 0.85,
                    },
                ],
                conclusion: response.to_string(),
                confidence: 0.85,
            })
        }

        async fn analyze_decisions(&self, _events: &[DebugEvent]) -> Result<Vec<DecisionData>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_start_session() {
        let analyzer = Arc::new(MockAnalyzer);
        let engine = DebugEngine::new(analyzer);

        let session_id = engine.start_session("Test Session").await.unwrap();
        assert!(!session_id.is_empty());

        let session = engine.get_session(&session_id).await.unwrap();
        assert!(session.is_active);
    }

    #[tokio::test]
    async fn test_log_events() {
        let analyzer = Arc::new(MockAnalyzer);
        let engine = DebugEngine::new(analyzer);

        let session_id = engine.start_session("Test").await.unwrap();

        engine
            .log_api_call(
                Some(&session_id),
                "anthropic",
                "claude-3",
                100,
                200,
                500,
                0.01,
            )
            .await
            .unwrap();

        engine
            .log_tool_call(
                Some(&session_id),
                "search",
                HashMap::from([("query".to_string(), "test".to_string())]),
                Some("results".to_string()),
                true,
            )
            .await
            .unwrap();

        let events = engine.get_events(&session_id, None).await.unwrap();
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn test_session_summary() {
        let analyzer = Arc::new(MockAnalyzer);
        let engine = DebugEngine::new(analyzer);

        let session_id = engine.start_session("Summary Test").await.unwrap();

        engine
            .log_api_call(Some(&session_id), "openai", "gpt-4", 50, 100, 200, 0.005)
            .await
            .unwrap();
        engine
            .log_api_call(Some(&session_id), "openai", "gpt-4", 60, 120, 250, 0.006)
            .await
            .unwrap();
        engine
            .log_error(Some(&session_id), "timeout", "Request timed out", true)
            .await
            .unwrap();

        let summary = engine.get_summary(&session_id).await.unwrap();
        assert_eq!(summary.api_calls, 2);
        assert_eq!(summary.errors, 1);
        assert!(summary.total_cost > 0.0);
    }

    #[tokio::test]
    async fn test_analyze_attributions() {
        let analyzer = Arc::new(MockAnalyzer);
        let engine = DebugEngine::new(analyzer);

        let attributions = engine
            .analyze_attributions(
                "What is the capital of France?",
                "The capital of France is Paris.",
            )
            .await
            .unwrap();

        assert!(!attributions.is_empty());
        assert!(!attributions[0].attributions.is_empty());
    }

    #[tokio::test]
    async fn test_extract_reasoning() {
        let analyzer = Arc::new(MockAnalyzer);
        let engine = DebugEngine::new(analyzer);

        let chain = engine
            .extract_reasoning("Paris is the capital")
            .await
            .unwrap();

        assert!(!chain.steps.is_empty());
        assert_eq!(chain.conclusion, "Paris is the capital");
    }

    #[tokio::test]
    async fn test_disable_debugging() {
        let analyzer = Arc::new(MockAnalyzer);
        let engine = DebugEngine::new(analyzer);

        engine.set_enabled(false).await;
        assert!(!engine.is_enabled().await);

        // Events should not be logged when disabled
        let event_id = engine.log_error(None, "test", "test", false).await.unwrap();
        assert!(event_id.is_empty());
    }
}
