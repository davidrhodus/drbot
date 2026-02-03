//! AI-powered email intelligence for drbot.
//!
//! Smart email processing and automation.
//!
//! # Features
//!
//! - Email summarization
//! - Reply drafting
//! - Action item extraction
//! - Smart categorization
//! - Thread analysis

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Mail AI result type.
pub type Result<T> = std::result::Result<T, MailError>;

/// Mail AI errors.
#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("Email not found: {0}")]
    NotFound(String),
    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
    #[error("Draft generation failed: {0}")]
    DraftFailed(String),
}

/// An email message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Email {
    /// Email ID.
    pub id: Uuid,
    /// Subject.
    pub subject: String,
    /// From address.
    pub from: String,
    /// To addresses.
    pub to: Vec<String>,
    /// CC addresses.
    pub cc: Vec<String>,
    /// Body content.
    pub body: String,
    /// HTML body.
    pub html_body: Option<String>,
    /// Attachments.
    pub attachments: Vec<Attachment>,
    /// Thread ID.
    pub thread_id: Option<Uuid>,
    /// In reply to.
    pub in_reply_to: Option<Uuid>,
    /// Received at.
    pub received_at: DateTime<Utc>,
    /// Labels/folders.
    pub labels: Vec<String>,
    /// Read status.
    pub is_read: bool,
}

impl Email {
    /// Create a new email.
    pub fn new(subject: &str, from: &str, to: Vec<String>, body: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            subject: subject.to_string(),
            from: from.to_string(),
            to,
            cc: Vec::new(),
            body: body.to_string(),
            html_body: None,
            attachments: Vec::new(),
            thread_id: None,
            in_reply_to: None,
            received_at: Utc::now(),
            labels: Vec::new(),
            is_read: false,
        }
    }
}

/// Email attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// Filename.
    pub filename: String,
    /// MIME type.
    pub mime_type: String,
    /// Size in bytes.
    pub size: usize,
    /// Content ID (for inline).
    pub content_id: Option<String>,
}

/// Email summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailSummary {
    /// Email ID.
    pub email_id: Uuid,
    /// One-line summary.
    pub one_liner: String,
    /// Key points.
    pub key_points: Vec<String>,
    /// Detected sentiment.
    pub sentiment: Sentiment,
    /// Priority assessment.
    pub priority: Priority,
    /// Requires response.
    pub requires_response: bool,
    /// Deadline mentioned.
    pub deadline: Option<DateTime<Utc>>,
}

/// Sentiment analysis result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sentiment {
    Positive,
    Neutral,
    Negative,
    Urgent,
    Formal,
    Casual,
}

/// Email priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Critical,
    High,
    Normal,
    Low,
}

/// Extracted action item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    /// Action ID.
    pub id: Uuid,
    /// Description.
    pub description: String,
    /// Assignee (if detected).
    pub assignee: Option<String>,
    /// Due date (if detected).
    pub due_date: Option<DateTime<Utc>>,
    /// Source email.
    pub source_email: Uuid,
    /// Extracted at.
    pub extracted_at: DateTime<Utc>,
}

/// Draft reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftReply {
    /// Draft ID.
    pub id: Uuid,
    /// Original email ID.
    pub reply_to: Uuid,
    /// Subject.
    pub subject: String,
    /// Body.
    pub body: String,
    /// Tone used.
    pub tone: ReplyTone,
    /// Confidence.
    pub confidence: f32,
    /// Alternative drafts.
    pub alternatives: Vec<String>,
}

/// Reply tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyTone {
    Professional,
    Friendly,
    Formal,
    Brief,
    Detailed,
}

/// Email category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailCategory {
    /// Category name.
    pub name: String,
    /// Confidence.
    pub confidence: f32,
    /// Subcategories.
    pub subcategories: Vec<String>,
}

/// Thread analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadAnalysis {
    /// Thread ID.
    pub thread_id: Uuid,
    /// Email count.
    pub email_count: usize,
    /// Participants.
    pub participants: Vec<String>,
    /// Summary.
    pub summary: String,
    /// Key decisions.
    pub decisions: Vec<String>,
    /// Open questions.
    pub open_questions: Vec<String>,
    /// Action items.
    pub action_items: Vec<ActionItem>,
}

/// Mail AI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailAIConfig {
    /// Enable auto-categorization.
    pub auto_categorize: bool,
    /// Enable action extraction.
    pub extract_actions: bool,
    /// Default reply tone.
    pub default_tone: ReplyTone,
    /// Smart priority.
    pub smart_priority: bool,
    /// Important senders (always high priority).
    pub important_senders: Vec<String>,
}

impl Default for MailAIConfig {
    fn default() -> Self {
        Self {
            auto_categorize: true,
            extract_actions: true,
            default_tone: ReplyTone::Professional,
            smart_priority: true,
            important_senders: Vec::new(),
        }
    }
}

/// Trait for email analyzers.
#[async_trait]
pub trait EmailAnalyzer: Send + Sync {
    /// Summarize an email.
    async fn summarize(&self, email: &Email) -> Result<EmailSummary>;
    /// Extract action items.
    async fn extract_actions(&self, email: &Email) -> Result<Vec<ActionItem>>;
    /// Categorize email.
    async fn categorize(&self, email: &Email) -> Result<Vec<EmailCategory>>;
}

/// Trait for reply generators.
#[async_trait]
pub trait ReplyGenerator: Send + Sync {
    /// Generate a draft reply.
    async fn generate_reply(
        &self,
        email: &Email,
        tone: ReplyTone,
        context: Option<&str>,
    ) -> Result<DraftReply>;
}

/// Mail AI engine.
pub struct MailAIEngine<A: EmailAnalyzer, G: ReplyGenerator> {
    config: MailAIConfig,
    analyzer: A,
    generator: G,
    emails: Arc<RwLock<HashMap<Uuid, Email>>>,
    summaries: Arc<RwLock<HashMap<Uuid, EmailSummary>>>,
    action_items: Arc<RwLock<Vec<ActionItem>>>,
}

impl<A: EmailAnalyzer, G: ReplyGenerator> MailAIEngine<A, G> {
    /// Create a new mail AI engine.
    pub fn new(config: MailAIConfig, analyzer: A, generator: G) -> Self {
        Self {
            config,
            analyzer,
            generator,
            emails: Arc::new(RwLock::new(HashMap::new())),
            summaries: Arc::new(RwLock::new(HashMap::new())),
            action_items: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Process an incoming email.
    pub async fn process(&self, email: Email) -> Result<EmailSummary> {
        let email_id = email.id;

        // Store email
        self.emails.write().await.insert(email_id, email.clone());

        // Analyze
        let summary = self.analyzer.summarize(&email).await?;
        self.summaries
            .write()
            .await
            .insert(email_id, summary.clone());

        // Extract actions if enabled
        if self.config.extract_actions {
            let actions = self.analyzer.extract_actions(&email).await?;
            self.action_items.write().await.extend(actions);
        }

        Ok(summary)
    }

    /// Get email summary.
    pub async fn get_summary(&self, email_id: Uuid) -> Option<EmailSummary> {
        self.summaries.read().await.get(&email_id).cloned()
    }

    /// Generate reply draft.
    pub async fn draft_reply(&self, email_id: Uuid, context: Option<&str>) -> Result<DraftReply> {
        let emails = self.emails.read().await;
        let email = emails
            .get(&email_id)
            .ok_or(MailError::NotFound(email_id.to_string()))?;

        self.generator
            .generate_reply(email, self.config.default_tone, context)
            .await
    }

    /// Get all action items.
    pub async fn get_action_items(&self) -> Vec<ActionItem> {
        self.action_items.read().await.clone()
    }

    /// Get pending action items.
    pub async fn get_pending_actions(&self) -> Vec<ActionItem> {
        let now = Utc::now();
        self.action_items
            .read()
            .await
            .iter()
            .filter(|a| a.due_date.map(|d| d > now).unwrap_or(true))
            .cloned()
            .collect()
    }

    /// Analyze a thread.
    pub async fn analyze_thread(&self, thread_id: Uuid) -> Result<ThreadAnalysis> {
        let emails = self.emails.read().await;
        let thread_emails: Vec<_> = emails
            .values()
            .filter(|e| e.thread_id == Some(thread_id))
            .cloned()
            .collect();

        if thread_emails.is_empty() {
            return Err(MailError::NotFound(thread_id.to_string()));
        }

        let mut participants: Vec<String> = thread_emails
            .iter()
            .flat_map(|e| std::iter::once(e.from.clone()).chain(e.to.clone()))
            .collect();
        participants.sort();
        participants.dedup();

        let mut all_actions = Vec::new();
        for email in &thread_emails {
            if let Ok(actions) = self.analyzer.extract_actions(email).await {
                all_actions.extend(actions);
            }
        }

        Ok(ThreadAnalysis {
            thread_id,
            email_count: thread_emails.len(),
            participants,
            summary: format!("Thread with {} emails", thread_emails.len()),
            decisions: Vec::new(),
            open_questions: Vec::new(),
            action_items: all_actions,
        })
    }

    /// Get statistics.
    pub async fn stats(&self) -> MailStats {
        let emails = self.emails.read().await;
        let actions = self.action_items.read().await;

        let unread = emails.values().filter(|e| !e.is_read).count();
        let requires_response = self
            .summaries
            .read()
            .await
            .values()
            .filter(|s| s.requires_response)
            .count();

        MailStats {
            total_emails: emails.len(),
            unread_count: unread,
            requires_response,
            pending_actions: actions.len(),
        }
    }
}

/// Mail statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailStats {
    pub total_emails: usize,
    pub unread_count: usize,
    pub requires_response: usize,
    pub pending_actions: usize,
}

/// Simple email analyzer for testing.
pub struct SimpleAnalyzer;

#[async_trait]
impl EmailAnalyzer for SimpleAnalyzer {
    async fn summarize(&self, email: &Email) -> Result<EmailSummary> {
        let word_count = email.body.split_whitespace().count();
        let body_lower = email.body.to_lowercase();
        let has_question = email.body.contains('?');
        let needs_response = has_question
            || body_lower.contains("please send")
            || body_lower.contains("please provide")
            || body_lower.contains("let me know")
            || body_lower.contains("respond");
        let is_urgent =
            email.subject.to_lowercase().contains("urgent") || body_lower.contains("asap");

        Ok(EmailSummary {
            email_id: email.id,
            one_liner: if word_count > 20 {
                format!(
                    "{}...",
                    email
                        .body
                        .split_whitespace()
                        .take(10)
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            } else {
                email.body.clone()
            },
            key_points: vec![format!("From: {}", email.from)],
            sentiment: if is_urgent {
                Sentiment::Urgent
            } else {
                Sentiment::Neutral
            },
            priority: if is_urgent {
                Priority::High
            } else {
                Priority::Normal
            },
            requires_response: needs_response,
            deadline: None,
        })
    }

    async fn extract_actions(&self, email: &Email) -> Result<Vec<ActionItem>> {
        let mut actions = Vec::new();

        // Simple pattern matching for action items
        for line in email.body.lines() {
            let lower = line.to_lowercase();
            if lower.contains("please")
                || lower.contains("need to")
                || lower.contains("action required")
            {
                actions.push(ActionItem {
                    id: Uuid::new_v4(),
                    description: line.trim().to_string(),
                    assignee: None,
                    due_date: None,
                    source_email: email.id,
                    extracted_at: Utc::now(),
                });
            }
        }

        Ok(actions)
    }

    async fn categorize(&self, email: &Email) -> Result<Vec<EmailCategory>> {
        let mut categories = Vec::new();
        let content = format!("{} {}", email.subject, email.body).to_lowercase();

        if content.contains("meeting") || content.contains("schedule") {
            categories.push(EmailCategory {
                name: "Calendar".to_string(),
                confidence: 0.8,
                subcategories: vec!["Meetings".to_string()],
            });
        }

        if content.contains("invoice") || content.contains("payment") {
            categories.push(EmailCategory {
                name: "Finance".to_string(),
                confidence: 0.85,
                subcategories: vec!["Billing".to_string()],
            });
        }

        if categories.is_empty() {
            categories.push(EmailCategory {
                name: "General".to_string(),
                confidence: 0.5,
                subcategories: Vec::new(),
            });
        }

        Ok(categories)
    }
}

/// Simple reply generator for testing.
pub struct SimpleGenerator;

#[async_trait]
impl ReplyGenerator for SimpleGenerator {
    async fn generate_reply(
        &self,
        email: &Email,
        tone: ReplyTone,
        _context: Option<&str>,
    ) -> Result<DraftReply> {
        let greeting = match tone {
            ReplyTone::Professional => format!(
                "Dear {},",
                email.from.split('@').next().unwrap_or("Sir/Madam")
            ),
            ReplyTone::Friendly => {
                format!("Hi {},", email.from.split('@').next().unwrap_or("there"))
            }
            ReplyTone::Formal => "Dear Sir/Madam,".to_string(),
            ReplyTone::Brief => String::new(),
            ReplyTone::Detailed => format!(
                "Dear {},",
                email.from.split('@').next().unwrap_or("Sir/Madam")
            ),
        };

        let body = format!(
            "{}\n\nThank you for your email regarding \"{}\".\n\n[Your response here]\n\nBest regards",
            greeting,
            email.subject
        );

        Ok(DraftReply {
            id: Uuid::new_v4(),
            reply_to: email.id,
            subject: format!("Re: {}", email.subject),
            body,
            tone,
            confidence: 0.7,
            alternatives: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_email_processing() {
        let engine = MailAIEngine::new(MailAIConfig::default(), SimpleAnalyzer, SimpleGenerator);

        let email = Email::new(
            "Urgent: Project Update Required",
            "boss@example.com",
            vec!["me@example.com".to_string()],
            "Please send the project update by end of day. This is needed ASAP for the board meeting.",
        );

        let summary = engine.process(email).await.unwrap();
        assert_eq!(summary.priority, Priority::High);
        assert!(summary.requires_response);
    }

    #[tokio::test]
    async fn test_action_extraction() {
        let analyzer = SimpleAnalyzer;
        let email = Email::new(
            "Tasks",
            "manager@example.com",
            vec!["team@example.com".to_string()],
            "Please review the document.\nWe need to finalize the budget.\nAction required: Submit report.",
        );

        let actions = analyzer.extract_actions(&email).await.unwrap();
        assert!(!actions.is_empty());
    }

    #[tokio::test]
    async fn test_draft_reply() {
        let engine = MailAIEngine::new(MailAIConfig::default(), SimpleAnalyzer, SimpleGenerator);

        let email = Email::new(
            "Question about project",
            "client@example.com",
            vec!["me@example.com".to_string()],
            "Can you provide an update on the project timeline?",
        );

        let email_id = email.id;
        engine.process(email).await.unwrap();

        let draft = engine.draft_reply(email_id, None).await.unwrap();
        assert!(draft.subject.starts_with("Re:"));
    }

    #[tokio::test]
    async fn test_categorization() {
        let analyzer = SimpleAnalyzer;
        let email = Email::new(
            "Invoice #12345",
            "billing@example.com",
            vec!["me@example.com".to_string()],
            "Please find attached your invoice for payment.",
        );

        let categories = analyzer.categorize(&email).await.unwrap();
        assert!(categories.iter().any(|c| c.name == "Finance"));
    }
}
