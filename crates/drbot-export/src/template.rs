//! Template system for custom export formats.

use crate::{Conversation, ExportError, Result};
use std::collections::HashMap;

/// A simple template engine.
pub struct TemplateEngine {
    templates: HashMap<String, Template>,
}

impl TemplateEngine {
    /// Create a new template engine.
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// Register a template.
    pub fn register(&mut self, name: &str, template: Template) {
        self.templates.insert(name.to_string(), template);
    }

    /// Render using a template.
    pub fn render(&self, name: &str, conversation: &Conversation) -> Result<String> {
        let template = self
            .templates
            .get(name)
            .ok_or_else(|| ExportError::TemplateError(format!("Template not found: {}", name)))?;

        template.render(conversation)
    }

    /// List available templates.
    pub fn list(&self) -> Vec<&str> {
        self.templates.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for TemplateEngine {
    fn default() -> Self {
        let mut engine = Self::new();

        // Register default templates
        engine.register("simple", Template::simple());
        engine.register("detailed", Template::detailed());
        engine.register("minimal", Template::minimal());

        engine
    }
}

/// A template definition.
#[derive(Debug, Clone)]
pub struct Template {
    /// Template name.
    pub name: String,
    /// Template content with placeholders.
    pub content: String,
    /// Message template.
    pub message_template: String,
    /// Header template.
    pub header_template: Option<String>,
    /// Footer template.
    pub footer_template: Option<String>,
}

impl Template {
    /// Create a new template.
    pub fn new(name: &str, content: &str) -> Self {
        Self {
            name: name.to_string(),
            content: content.to_string(),
            message_template: "{{sender}}: {{content}}\n".to_string(),
            header_template: None,
            footer_template: None,
        }
    }

    /// Set message template.
    pub fn with_message_template(mut self, template: &str) -> Self {
        self.message_template = template.to_string();
        self
    }

    /// Set header template.
    pub fn with_header(mut self, header: &str) -> Self {
        self.header_template = Some(header.to_string());
        self
    }

    /// Set footer template.
    pub fn with_footer(mut self, footer: &str) -> Self {
        self.footer_template = Some(footer.to_string());
        self
    }

    /// Render the template with conversation data.
    pub fn render(&self, conversation: &Conversation) -> Result<String> {
        let mut output = String::new();

        // Header
        if let Some(header) = &self.header_template {
            output.push_str(&self.substitute_vars(header, conversation, None));
        }

        // Main content
        let mut messages_content = String::new();
        for message in &conversation.messages {
            let msg_content = self
                .message_template
                .replace("{{sender}}", &message.sender)
                .replace("{{role}}", &message.role)
                .replace("{{content}}", &message.content)
                .replace(
                    "{{timestamp}}",
                    &message.timestamp.format("%Y-%m-%d %H:%M").to_string(),
                );
            messages_content.push_str(&msg_content);
        }

        let main_content =
            self.substitute_vars(&self.content, conversation, Some(&messages_content));
        output.push_str(&main_content);

        // Footer
        if let Some(footer) = &self.footer_template {
            output.push_str(&self.substitute_vars(footer, conversation, None));
        }

        Ok(output)
    }

    /// Substitute variables in template.
    fn substitute_vars(
        &self,
        template: &str,
        conversation: &Conversation,
        messages: Option<&str>,
    ) -> String {
        let mut result = template.to_string();

        result = result.replace("{{title}}", &conversation.title);
        result = result.replace("{{channel}}", &conversation.channel_id);
        result = result.replace("{{participants}}", &conversation.participants.join(", "));
        result = result.replace(
            "{{started_at}}",
            &conversation.started_at.format("%Y-%m-%d %H:%M").to_string(),
        );
        result = result.replace(
            "{{message_count}}",
            &conversation.messages.len().to_string(),
        );

        if let Some(ended) = conversation.ended_at {
            result = result.replace("{{ended_at}}", &ended.format("%Y-%m-%d %H:%M").to_string());
        } else {
            result = result.replace("{{ended_at}}", "ongoing");
        }

        if let Some(msgs) = messages {
            result = result.replace("{{messages}}", msgs);
        }

        result
    }

    /// Simple text template.
    pub fn simple() -> Self {
        Self::new("simple", "{{title}}\n\n{{messages}}")
            .with_message_template("{{sender}}: {{content}}\n\n")
    }

    /// Detailed template with metadata.
    pub fn detailed() -> Self {
        Self::new(
            "detailed",
            r#"# {{title}}

Channel: {{channel}}
Started: {{started_at}}
Participants: {{participants}}
Messages: {{message_count}}

---

{{messages}}"#,
        )
        .with_message_template("[{{timestamp}}] {{sender}} ({{role}})\n{{content}}\n\n")
    }

    /// Minimal template.
    pub fn minimal() -> Self {
        Self::new("minimal", "{{messages}}").with_message_template("{{content}}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExportMessage;
    use chrono::Utc;
    use uuid::Uuid;

    fn test_conversation() -> Conversation {
        let mut conv = Conversation::new(Uuid::new_v4(), "Test Chat", "general");
        conv.participants = vec!["Alice".to_string(), "Bot".to_string()];
        conv.messages.push(ExportMessage {
            id: Uuid::new_v4(),
            sender: "Alice".to_string(),
            role: "user".to_string(),
            content: "Hello!".to_string(),
            timestamp: Utc::now(),
            attachments: vec![],
        });
        conv
    }

    #[test]
    fn test_simple_template() {
        let template = Template::simple();
        let conv = test_conversation();
        let result = template.render(&conv).unwrap();

        assert!(result.contains("Test Chat"));
        assert!(result.contains("Alice: Hello!"));
    }

    #[test]
    fn test_detailed_template() {
        let template = Template::detailed();
        let conv = test_conversation();
        let result = template.render(&conv).unwrap();

        assert!(result.contains("Channel: general"));
        assert!(result.contains("Participants: Alice, Bot"));
    }

    #[test]
    fn test_template_engine() {
        let engine = TemplateEngine::default();
        let conv = test_conversation();

        let simple = engine.render("simple", &conv).unwrap();
        assert!(simple.contains("Hello!"));

        let templates = engine.list();
        assert!(templates.contains(&"simple"));
        assert!(templates.contains(&"detailed"));
    }
}
