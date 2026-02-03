//! Smart screen context for AI-assisted workflows.
//!
//! Provides intelligent context extraction based on what the user is looking at.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Smart context extracted from screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartContext {
    /// Context ID.
    pub id: Uuid,
    /// Context type.
    pub context_type: ContextType,
    /// Extracted content.
    pub content: ContextContent,
    /// Confidence score.
    pub confidence: f32,
    /// Suggestions based on context.
    pub suggestions: Vec<ContextSuggestion>,
    /// Related files/resources.
    pub related_resources: Vec<RelatedResource>,
    /// Extracted at.
    pub extracted_at: DateTime<Utc>,
}

/// Context types based on detected activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextType {
    /// Code editing.
    Code,
    /// Document writing.
    Document,
    /// Email composition.
    Email,
    /// Chat/messaging.
    Chat,
    /// Web browsing.
    Web,
    /// Terminal/CLI.
    Terminal,
    /// Spreadsheet.
    Spreadsheet,
    /// Design tool.
    Design,
    /// Meeting/video call.
    Meeting,
    /// File management.
    FileManagement,
    /// Unknown.
    Unknown,
}

/// Extracted context content.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextContent {
    /// Code context.
    Code {
        language: String,
        file_path: Option<String>,
        visible_code: String,
        cursor_line: Option<u32>,
        selected_text: Option<String>,
        project_root: Option<String>,
        imports: Vec<String>,
        current_function: Option<String>,
    },
    /// Document context.
    Document {
        title: Option<String>,
        visible_text: String,
        selected_text: Option<String>,
        format: String,
        word_count: u32,
    },
    /// Email context.
    Email {
        subject: Option<String>,
        recipients: Vec<String>,
        body_preview: String,
        is_reply: bool,
        thread_length: u32,
    },
    /// Web context.
    Web {
        url: String,
        title: String,
        visible_text: String,
        selected_text: Option<String>,
        meta_description: Option<String>,
    },
    /// Terminal context.
    Terminal {
        current_directory: String,
        last_command: Option<String>,
        output_preview: String,
        shell: String,
    },
    /// Generic text context.
    Generic {
        app_name: String,
        visible_text: String,
        selected_text: Option<String>,
    },
}

/// Suggestion based on context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSuggestion {
    /// Suggestion type.
    pub suggestion_type: SuggestionType,
    /// Description.
    pub description: String,
    /// Prompt to execute.
    pub prompt: String,
    /// Confidence.
    pub confidence: f32,
}

/// Suggestion types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionType {
    /// Explain the code/content.
    Explain,
    /// Fix an error.
    Fix,
    /// Improve/refactor.
    Improve,
    /// Generate related code.
    Generate,
    /// Summarize.
    Summarize,
    /// Translate.
    Translate,
    /// Search related.
    Search,
    /// Complete/continue.
    Complete,
}

/// Related resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedResource {
    /// Resource type.
    pub resource_type: ResourceType,
    /// Path or identifier.
    pub path: String,
    /// Name.
    pub name: String,
    /// Relevance score.
    pub relevance: f32,
}

/// Resource types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    File,
    Directory,
    Url,
    Document,
    Conversation,
}

/// App detection patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPattern {
    /// App name patterns.
    pub name_patterns: Vec<String>,
    /// Context type.
    pub context_type: ContextType,
    /// Language hints (for code editors).
    pub language_hints: HashMap<String, String>,
}

/// Smart context extractor.
pub struct SmartContextExtractor {
    app_patterns: Vec<AppPattern>,
    language_extensions: HashMap<String, String>,
}

impl SmartContextExtractor {
    /// Create a new extractor with default patterns.
    pub fn new() -> Self {
        let mut app_patterns = Vec::new();

        // Code editors
        app_patterns.push(AppPattern {
            name_patterns: vec![
                "VS Code".to_string(),
                "Visual Studio Code".to_string(),
                "Code".to_string(),
            ],
            context_type: ContextType::Code,
            language_hints: HashMap::new(),
        });
        app_patterns.push(AppPattern {
            name_patterns: vec![
                "IntelliJ".to_string(),
                "WebStorm".to_string(),
                "PyCharm".to_string(),
                "RustRover".to_string(),
            ],
            context_type: ContextType::Code,
            language_hints: HashMap::new(),
        });

        // Terminals
        app_patterns.push(AppPattern {
            name_patterns: vec![
                "Terminal".to_string(),
                "iTerm".to_string(),
                "Warp".to_string(),
                "Alacritty".to_string(),
            ],
            context_type: ContextType::Terminal,
            language_hints: HashMap::new(),
        });

        // Browsers
        app_patterns.push(AppPattern {
            name_patterns: vec![
                "Chrome".to_string(),
                "Safari".to_string(),
                "Firefox".to_string(),
                "Arc".to_string(),
                "Edge".to_string(),
            ],
            context_type: ContextType::Web,
            language_hints: HashMap::new(),
        });

        // Documents
        app_patterns.push(AppPattern {
            name_patterns: vec![
                "Word".to_string(),
                "Pages".to_string(),
                "Google Docs".to_string(),
                "Notion".to_string(),
            ],
            context_type: ContextType::Document,
            language_hints: HashMap::new(),
        });

        // Email
        app_patterns.push(AppPattern {
            name_patterns: vec![
                "Mail".to_string(),
                "Outlook".to_string(),
                "Gmail".to_string(),
            ],
            context_type: ContextType::Email,
            language_hints: HashMap::new(),
        });

        // Chat
        app_patterns.push(AppPattern {
            name_patterns: vec![
                "Slack".to_string(),
                "Discord".to_string(),
                "Messages".to_string(),
                "WhatsApp".to_string(),
                "Telegram".to_string(),
            ],
            context_type: ContextType::Chat,
            language_hints: HashMap::new(),
        });

        // Language extensions
        let mut language_extensions = HashMap::new();
        language_extensions.insert("rs".to_string(), "rust".to_string());
        language_extensions.insert("py".to_string(), "python".to_string());
        language_extensions.insert("js".to_string(), "javascript".to_string());
        language_extensions.insert("ts".to_string(), "typescript".to_string());
        language_extensions.insert("go".to_string(), "go".to_string());
        language_extensions.insert("java".to_string(), "java".to_string());
        language_extensions.insert("cpp".to_string(), "cpp".to_string());
        language_extensions.insert("c".to_string(), "c".to_string());
        language_extensions.insert("rb".to_string(), "ruby".to_string());
        language_extensions.insert("swift".to_string(), "swift".to_string());
        language_extensions.insert("kt".to_string(), "kotlin".to_string());

        Self {
            app_patterns,
            language_extensions,
        }
    }

    /// Detect context type from app name.
    pub fn detect_context_type(&self, app_name: &str) -> ContextType {
        let app_lower = app_name.to_lowercase();

        for pattern in &self.app_patterns {
            for name in &pattern.name_patterns {
                if app_lower.contains(&name.to_lowercase()) {
                    return pattern.context_type;
                }
            }
        }

        ContextType::Unknown
    }

    /// Detect programming language from file path.
    pub fn detect_language(&self, file_path: &str) -> Option<String> {
        let extension = file_path.rsplit('.').next()?;
        self.language_extensions.get(extension).cloned()
    }

    /// Extract smart context from raw screen data.
    pub fn extract(
        &self,
        app_name: &str,
        window_title: &str,
        visible_text: &str,
        selected_text: Option<&str>,
    ) -> SmartContext {
        let context_type = self.detect_context_type(app_name);

        let content = match context_type {
            ContextType::Code => {
                let file_path = self.extract_file_path(window_title);
                let language = file_path
                    .as_ref()
                    .and_then(|p| self.detect_language(p))
                    .unwrap_or_else(|| "unknown".to_string());

                ContextContent::Code {
                    language,
                    file_path,
                    visible_code: visible_text.to_string(),
                    cursor_line: None,
                    selected_text: selected_text.map(|s| s.to_string()),
                    project_root: None,
                    imports: self.extract_imports(visible_text),
                    current_function: self.extract_current_function(visible_text),
                }
            }
            ContextType::Terminal => {
                let (last_command, output) = self.parse_terminal_output(visible_text);
                ContextContent::Terminal {
                    current_directory: self.extract_cwd(visible_text).unwrap_or_default(),
                    last_command,
                    output_preview: output,
                    shell: "bash".to_string(),
                }
            }
            ContextType::Web => ContextContent::Web {
                url: self.extract_url(window_title).unwrap_or_default(),
                title: window_title.to_string(),
                visible_text: visible_text.to_string(),
                selected_text: selected_text.map(|s| s.to_string()),
                meta_description: None,
            },
            ContextType::Email => ContextContent::Email {
                subject: self.extract_email_subject(window_title),
                recipients: Vec::new(),
                body_preview: visible_text.chars().take(500).collect(),
                is_reply: window_title.to_lowercase().starts_with("re:"),
                thread_length: 1,
            },
            ContextType::Document => ContextContent::Document {
                title: Some(window_title.to_string()),
                visible_text: visible_text.to_string(),
                selected_text: selected_text.map(|s| s.to_string()),
                format: "unknown".to_string(),
                word_count: visible_text.split_whitespace().count() as u32,
            },
            _ => ContextContent::Generic {
                app_name: app_name.to_string(),
                visible_text: visible_text.to_string(),
                selected_text: selected_text.map(|s| s.to_string()),
            },
        };

        let suggestions = self.generate_suggestions(&context_type, &content);

        SmartContext {
            id: Uuid::new_v4(),
            context_type,
            content,
            confidence: 0.85,
            suggestions,
            related_resources: Vec::new(),
            extracted_at: Utc::now(),
        }
    }

    fn extract_file_path(&self, title: &str) -> Option<String> {
        // Try to extract file path from window title
        // Patterns like "filename.rs — VS Code" or "project/src/file.py - PyCharm"
        let parts: Vec<&str> = title.split(&['—', '-', '–'][..]).collect();
        if let Some(first) = parts.first() {
            let trimmed = first.trim();
            if trimmed.contains('.') {
                return Some(trimmed.to_string());
            }
        }
        None
    }

    fn extract_imports(&self, code: &str) -> Vec<String> {
        let mut imports = Vec::new();
        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("use ")
                || trimmed.starts_with("import ")
                || trimmed.starts_with("from ")
                || trimmed.starts_with("#include")
                || trimmed.starts_with("require")
            {
                imports.push(trimmed.to_string());
            }
        }
        imports.truncate(20);
        imports
    }

    fn extract_current_function(&self, code: &str) -> Option<String> {
        // Simple heuristic: find last function definition
        for line in code.lines().rev() {
            let trimmed = line.trim();
            if trimmed.starts_with("fn ")
                || trimmed.starts_with("def ")
                || trimmed.starts_with("func ")
                || trimmed.starts_with("function ")
                || trimmed.contains("async fn ")
            {
                return Some(trimmed.to_string());
            }
        }
        None
    }

    fn parse_terminal_output(&self, text: &str) -> (Option<String>, String) {
        let lines: Vec<&str> = text.lines().collect();

        // Find last prompt line
        let mut last_command = None;
        let mut output_start = 0;

        for (i, line) in lines.iter().enumerate() {
            if line.contains("$ ") || line.contains("% ") || line.ends_with("> ") {
                last_command = line
                    .split(&['$', '%', '>'][..])
                    .last()
                    .map(|s| s.trim().to_string());
                output_start = i + 1;
            }
        }

        let output: String = lines[output_start..]
            .iter()
            .take(20)
            .map(|s| *s)
            .collect::<Vec<_>>()
            .join("\n");

        (last_command, output)
    }

    fn extract_cwd(&self, text: &str) -> Option<String> {
        // Try to find current directory from prompt
        for line in text.lines() {
            if line.contains("~/") || line.starts_with('/') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                for part in parts {
                    if part.contains('/') && !part.contains("http") {
                        return Some(part.trim_matches(&['[', ']', '(', ')'][..]).to_string());
                    }
                }
            }
        }
        None
    }

    fn extract_url(&self, title: &str) -> Option<String> {
        // URL is often not in title, would need to get from browser
        None
    }

    fn extract_email_subject(&self, title: &str) -> Option<String> {
        // Remove common prefixes
        let subject = title
            .trim_start_matches("Re: ")
            .trim_start_matches("RE: ")
            .trim_start_matches("Fwd: ")
            .trim_start_matches("FW: ");

        if subject.is_empty() {
            None
        } else {
            Some(subject.to_string())
        }
    }

    fn generate_suggestions(
        &self,
        context_type: &ContextType,
        content: &ContextContent,
    ) -> Vec<ContextSuggestion> {
        let mut suggestions = Vec::new();

        match context_type {
            ContextType::Code => {
                suggestions.push(ContextSuggestion {
                    suggestion_type: SuggestionType::Explain,
                    description: "Explain this code".to_string(),
                    prompt: "Explain what this code does".to_string(),
                    confidence: 0.9,
                });
                suggestions.push(ContextSuggestion {
                    suggestion_type: SuggestionType::Improve,
                    description: "Suggest improvements".to_string(),
                    prompt: "How can I improve this code?".to_string(),
                    confidence: 0.85,
                });
                suggestions.push(ContextSuggestion {
                    suggestion_type: SuggestionType::Fix,
                    description: "Find potential bugs".to_string(),
                    prompt: "Are there any bugs or issues in this code?".to_string(),
                    confidence: 0.8,
                });
            }
            ContextType::Document => {
                suggestions.push(ContextSuggestion {
                    suggestion_type: SuggestionType::Summarize,
                    description: "Summarize this document".to_string(),
                    prompt: "Summarize the main points".to_string(),
                    confidence: 0.9,
                });
                suggestions.push(ContextSuggestion {
                    suggestion_type: SuggestionType::Improve,
                    description: "Improve writing".to_string(),
                    prompt: "How can I improve this text?".to_string(),
                    confidence: 0.85,
                });
            }
            ContextType::Email => {
                suggestions.push(ContextSuggestion {
                    suggestion_type: SuggestionType::Complete,
                    description: "Draft a reply".to_string(),
                    prompt: "Help me write a reply to this email".to_string(),
                    confidence: 0.9,
                });
            }
            ContextType::Terminal => {
                suggestions.push(ContextSuggestion {
                    suggestion_type: SuggestionType::Explain,
                    description: "Explain output".to_string(),
                    prompt: "What does this output mean?".to_string(),
                    confidence: 0.85,
                });
                suggestions.push(ContextSuggestion {
                    suggestion_type: SuggestionType::Fix,
                    description: "Fix error".to_string(),
                    prompt: "How do I fix this error?".to_string(),
                    confidence: 0.8,
                });
            }
            _ => {
                suggestions.push(ContextSuggestion {
                    suggestion_type: SuggestionType::Summarize,
                    description: "Help me with this".to_string(),
                    prompt: "Can you help me with what I'm looking at?".to_string(),
                    confidence: 0.7,
                });
            }
        }

        suggestions
    }
}

impl Default for SmartContextExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_type_detection() {
        let extractor = SmartContextExtractor::new();

        assert_eq!(
            extractor.detect_context_type("Visual Studio Code"),
            ContextType::Code
        );
        assert_eq!(
            extractor.detect_context_type("iTerm2"),
            ContextType::Terminal
        );
        assert_eq!(
            extractor.detect_context_type("Google Chrome"),
            ContextType::Web
        );
        assert_eq!(
            extractor.detect_context_type("Random App"),
            ContextType::Unknown
        );
    }

    #[test]
    fn test_language_detection() {
        let extractor = SmartContextExtractor::new();

        assert_eq!(
            extractor.detect_language("main.rs"),
            Some("rust".to_string())
        );
        assert_eq!(
            extractor.detect_language("app.py"),
            Some("python".to_string())
        );
        assert_eq!(
            extractor.detect_language("index.ts"),
            Some("typescript".to_string())
        );
    }

    #[test]
    fn test_smart_context_extraction() {
        let extractor = SmartContextExtractor::new();

        let context = extractor.extract(
            "Visual Studio Code",
            "main.rs — VS Code",
            "fn main() {\n    println!(\"Hello\");\n}",
            Some("println!"),
        );

        assert_eq!(context.context_type, ContextType::Code);
        assert!(!context.suggestions.is_empty());

        if let ContextContent::Code { language, .. } = &context.content {
            assert_eq!(language, "rust");
        } else {
            panic!("Expected Code content");
        }
    }
}
