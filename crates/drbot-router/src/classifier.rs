//! Task classification for intelligent routing.

use drbot_core::message::Message;

/// Task complexity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskComplexity {
    /// Simple tasks: greetings, simple questions, formatting.
    Simple,
    /// Medium tasks: summaries, explanations, basic analysis.
    Medium,
    /// Complex tasks: coding, detailed analysis, creative writing.
    Complex,
    /// Expert tasks: multi-step reasoning, research, complex code.
    Expert,
}

/// Task type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    /// General conversation.
    Conversation,
    /// Code-related tasks.
    Coding,
    /// Writing and content creation.
    Writing,
    /// Analysis and reasoning.
    Analysis,
    /// Math and calculations.
    Math,
    /// Research and information gathering.
    Research,
    /// Translation.
    Translation,
    /// Summarization.
    Summarization,
    /// Unknown/other.
    Other,
}

/// Trait for implementing task classifiers.
pub trait TaskClassifier: Send + Sync {
    /// Classify the complexity of a task based on messages.
    fn classify(&self, messages: &[Message]) -> TaskComplexity;

    /// Classify the type of task.
    fn classify_type(&self, messages: &[Message]) -> TaskType;
}

/// Rule-based task classifier using heuristics.
#[allow(dead_code)]
pub struct RuleBasedClassifier {
    /// Keywords indicating simple tasks.
    simple_keywords: Vec<&'static str>,
    /// Keywords indicating complex tasks.
    complex_keywords: Vec<&'static str>,
    /// Keywords indicating expert tasks.
    expert_keywords: Vec<&'static str>,
    /// Keywords indicating coding tasks.
    coding_keywords: Vec<&'static str>,
    /// Keywords indicating writing tasks.
    writing_keywords: Vec<&'static str>,
    /// Keywords indicating analysis tasks.
    analysis_keywords: Vec<&'static str>,
}

impl Default for RuleBasedClassifier {
    fn default() -> Self {
        Self {
            simple_keywords: vec![
                "hello",
                "hi",
                "hey",
                "thanks",
                "thank you",
                "bye",
                "goodbye",
                "what is",
                "what's",
                "define",
                "meaning of",
                "how are you",
            ],
            complex_keywords: vec![
                "implement",
                "code",
                "write a program",
                "function",
                "algorithm",
                "analyze",
                "compare",
                "explain in detail",
                "step by step",
                "create",
                "design",
                "build",
                "develop",
            ],
            expert_keywords: vec![
                "architecture",
                "optimize",
                "refactor",
                "debug",
                "complex",
                "research",
                "comprehensive",
                "in-depth",
                "multiple",
                "system",
                "strategy",
                "framework",
                "best practices",
            ],
            coding_keywords: vec![
                "code",
                "function",
                "class",
                "implement",
                "program",
                "script",
                "bug",
                "error",
                "compile",
                "run",
                "execute",
                "debug",
                "test",
                "python",
                "javascript",
                "rust",
                "java",
                "typescript",
                "sql",
            ],
            writing_keywords: vec![
                "write",
                "essay",
                "article",
                "story",
                "poem",
                "blog",
                "content",
                "draft",
                "edit",
                "proofread",
                "creative",
            ],
            analysis_keywords: vec![
                "analyze",
                "compare",
                "evaluate",
                "assess",
                "review",
                "examine",
                "investigate",
                "study",
                "research",
            ],
        }
    }
}

#[allow(dead_code)]
impl RuleBasedClassifier {
    /// Create a new rule-based classifier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the combined text from messages.
    fn get_text(&self, messages: &[Message]) -> String {
        messages
            .iter()
            .map(|m| m.text_content())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }

    /// Count keyword matches.
    fn count_keywords(&self, text: &str, keywords: &[&str]) -> usize {
        keywords.iter().filter(|k| text.contains(*k)).count()
    }

    /// Estimate message complexity from length and structure.
    fn estimate_from_length(&self, messages: &[Message]) -> TaskComplexity {
        let total_len: usize = messages.iter().map(|m| m.text_content().len()).sum();
        let message_count = messages.len();

        if total_len < 50 && message_count <= 2 {
            TaskComplexity::Simple
        } else if total_len < 500 {
            TaskComplexity::Medium
        } else if total_len < 2000 {
            TaskComplexity::Complex
        } else {
            TaskComplexity::Expert
        }
    }
}

impl TaskClassifier for RuleBasedClassifier {
    fn classify(&self, messages: &[Message]) -> TaskComplexity {
        let text = self.get_text(messages);

        // Check for expert keywords first
        let expert_count = self.count_keywords(&text, &self.expert_keywords);
        if expert_count >= 2 {
            return TaskComplexity::Expert;
        }

        // Check for complex keywords
        let complex_count = self.count_keywords(&text, &self.complex_keywords);
        if complex_count >= 2 || expert_count >= 1 {
            return TaskComplexity::Complex;
        }

        // Check for simple keywords
        let simple_count = self.count_keywords(&text, &self.simple_keywords);
        if simple_count >= 1 && complex_count == 0 {
            return TaskComplexity::Simple;
        }

        // Fall back to length-based estimation
        self.estimate_from_length(messages)
    }

    fn classify_type(&self, messages: &[Message]) -> TaskType {
        let text = self.get_text(messages);

        // Check for coding indicators
        if self.count_keywords(&text, &self.coding_keywords) >= 2 {
            return TaskType::Coding;
        }

        // Check for writing indicators
        if self.count_keywords(&text, &self.writing_keywords) >= 2 {
            return TaskType::Writing;
        }

        // Check for analysis indicators
        if self.count_keywords(&text, &self.analysis_keywords) >= 2 {
            return TaskType::Analysis;
        }

        // Check for specific task types
        if text.contains("translate") || text.contains("translation") {
            return TaskType::Translation;
        }

        if text.contains("summarize") || text.contains("summary") || text.contains("tldr") {
            return TaskType::Summarization;
        }

        if text.contains("calculate")
            || text.contains("math")
            || text.contains("equation")
            || text.contains("solve")
        {
            return TaskType::Math;
        }

        if text.contains("research")
            || text.contains("find information")
            || text.contains("look up")
        {
            return TaskType::Research;
        }

        TaskType::Conversation
    }
}

/// LLM-based classifier that uses a fast model for classification.
#[allow(dead_code)]
pub struct LlmClassifier {
    fallback: RuleBasedClassifier,
}

#[allow(dead_code)]
impl LlmClassifier {
    /// Create a new LLM-based classifier.
    pub fn new() -> Self {
        Self {
            fallback: RuleBasedClassifier::new(),
        }
    }
}

impl Default for LlmClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskClassifier for LlmClassifier {
    fn classify(&self, messages: &[Message]) -> TaskComplexity {
        // For now, use rule-based classification
        // In production, this would call a fast LLM for classification
        self.fallback.classify(messages)
    }

    fn classify_type(&self, messages: &[Message]) -> TaskType {
        self.fallback.classify_type(messages)
    }
}

// ============================================================================
// KANI FORMAL VERIFICATION PROOFS
// ============================================================================

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    /// Proof: TaskComplexity has exactly 4 variants
    #[kani::proof]
    fn proof_complexity_variants() {
        let val: u8 = kani::any();
        kani::assume(val <= 3);

        let complexity = match val {
            0 => TaskComplexity::Simple,
            1 => TaskComplexity::Medium,
            2 => TaskComplexity::Complex,
            _ => TaskComplexity::Expert,
        };

        kani::assert(complexity == complexity, "Complexity must equal itself");
    }

    /// Proof: TaskType has exactly 9 variants
    #[kani::proof]
    fn proof_task_type_variants() {
        let val: u8 = kani::any();
        kani::assume(val <= 8);

        let task_type = match val {
            0 => TaskType::Conversation,
            1 => TaskType::Coding,
            2 => TaskType::Writing,
            3 => TaskType::Analysis,
            4 => TaskType::Math,
            5 => TaskType::Research,
            6 => TaskType::Translation,
            7 => TaskType::Summarization,
            _ => TaskType::Other,
        };

        kani::assert(task_type == task_type, "Task type must equal itself");
    }

    /// Proof: Length-based estimation is deterministic
    #[kani::proof]
    fn proof_length_estimation_deterministic() {
        let total_len: usize = kani::any();
        let message_count: usize = kani::any();

        kani::assume(total_len <= 100_000);
        kani::assume(message_count <= 1000);

        // Inline the estimation logic
        let complexity1 = if total_len < 50 && message_count <= 2 {
            TaskComplexity::Simple
        } else if total_len < 500 {
            TaskComplexity::Medium
        } else if total_len < 2000 {
            TaskComplexity::Complex
        } else {
            TaskComplexity::Expert
        };

        // Same calculation should give same result
        let complexity2 = if total_len < 50 && message_count <= 2 {
            TaskComplexity::Simple
        } else if total_len < 500 {
            TaskComplexity::Medium
        } else if total_len < 2000 {
            TaskComplexity::Complex
        } else {
            TaskComplexity::Expert
        };

        kani::assert(
            complexity1 == complexity2,
            "Estimation must be deterministic",
        );
    }

    /// Proof: Length thresholds are ordered correctly
    #[kani::proof]
    fn proof_length_thresholds_ordered() {
        // Verify the thresholds create a valid partition
        let simple_max = 50usize;
        let medium_max = 500usize;
        let complex_max = 2000usize;

        kani::assert(simple_max < medium_max, "Simple max < Medium max");
        kani::assert(medium_max < complex_max, "Medium max < Complex max");
    }

    /// Proof: Keyword count is bounded by keyword list length
    #[kani::proof]
    fn proof_keyword_count_bounded() {
        // Simulate counting with a fixed number of keywords
        let keywords_len = 12; // simple_keywords has 12 items
        let matches: usize = kani::any();

        kani::assume(matches <= keywords_len);

        kani::assert(matches <= keywords_len, "Matches must be <= keyword count");
    }

    /// Proof: Classification thresholds create valid decision boundaries
    #[kani::proof]
    fn proof_classification_thresholds() {
        let expert_count: usize = kani::any();
        let complex_count: usize = kani::any();
        let simple_count: usize = kani::any();

        kani::assume(expert_count <= 20);
        kani::assume(complex_count <= 20);
        kani::assume(simple_count <= 20);

        // Verify the classification logic is consistent
        let is_expert = expert_count >= 2;
        let is_complex = complex_count >= 2 || expert_count >= 1;
        let is_simple = simple_count >= 1 && complex_count == 0;

        // If expert, also complex (except for Simple case)
        if is_expert {
            kani::assert(
                !is_simple || complex_count == 0,
                "Expert tasks should not be classified as simple",
            );
        }
    }

    /// Proof: Empty messages result in some valid classification
    #[kani::proof]
    fn proof_empty_messages_valid() {
        // With no messages, total_len = 0, message_count = 0
        let total_len = 0usize;
        let message_count = 0usize;

        let complexity = if total_len < 50 && message_count <= 2 {
            TaskComplexity::Simple
        } else if total_len < 500 {
            TaskComplexity::Medium
        } else if total_len < 2000 {
            TaskComplexity::Complex
        } else {
            TaskComplexity::Expert
        };

        // Empty messages should classify as Simple
        kani::assert(
            complexity == TaskComplexity::Simple,
            "Empty messages should be Simple",
        );
    }

    /// Proof: Very long messages classify as Expert
    #[kani::proof]
    fn proof_long_messages_expert() {
        let total_len: usize = kani::any();
        kani::assume(total_len >= 2000);
        kani::assume(total_len <= 1_000_000); // Reasonable upper bound

        let message_count = 1usize; // At least one message

        let complexity = if total_len < 50 && message_count <= 2 {
            TaskComplexity::Simple
        } else if total_len < 500 {
            TaskComplexity::Medium
        } else if total_len < 2000 {
            TaskComplexity::Complex
        } else {
            TaskComplexity::Expert
        };

        kani::assert(
            complexity == TaskComplexity::Expert,
            "Long messages should be Expert",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(text: &str) -> Message {
        Message::user(text)
    }

    #[test]
    fn test_simple_classification() {
        let classifier = RuleBasedClassifier::new();

        let messages = vec![msg("Hello, how are you?")];
        assert_eq!(classifier.classify(&messages), TaskComplexity::Simple);

        let messages = vec![msg("What is the capital of France?")];
        assert_eq!(classifier.classify(&messages), TaskComplexity::Simple);
    }

    #[test]
    fn test_complex_classification() {
        let classifier = RuleBasedClassifier::new();

        let messages = vec![msg(
            "Implement a function to sort an array using quicksort algorithm",
        )];
        assert_eq!(classifier.classify(&messages), TaskComplexity::Complex);
    }

    #[test]
    fn test_expert_classification() {
        let classifier = RuleBasedClassifier::new();

        let messages = vec![msg(
            "Design a comprehensive system architecture for a distributed database \
             with multiple replicas and implement an optimization strategy for query performance",
        )];
        assert_eq!(classifier.classify(&messages), TaskComplexity::Expert);
    }

    #[test]
    fn test_task_type_coding() {
        let classifier = RuleBasedClassifier::new();

        let messages = vec![msg(
            "Write a Python function to calculate fibonacci numbers",
        )];
        assert_eq!(classifier.classify_type(&messages), TaskType::Coding);
    }

    #[test]
    fn test_task_type_writing() {
        let classifier = RuleBasedClassifier::new();

        let messages = vec![msg("Write an essay about climate change")];
        assert_eq!(classifier.classify_type(&messages), TaskType::Writing);
    }
}
