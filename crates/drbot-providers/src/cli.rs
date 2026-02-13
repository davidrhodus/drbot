//! CLI-wrapping provider that shells out to external AI CLI tools.

use async_trait::async_trait;
use drbot_core::config::CliProviderConfig;
use drbot_core::message::{Message, Role};
use std::pin::Pin;
use tokio_stream::Stream;

use crate::{ChatOptions, ChatResponse, ModelInfo, Provider, StreamEvent, Usage};

/// Default timeout for CLI execution in seconds.
pub const CLI_DEFAULT_TIMEOUT_SECS: u64 = 120;

/// A provider that wraps external CLI tools (e.g. `claude -p`, `codex exec`).
pub struct CliProvider {
    pub command: String,
    pub args: Vec<String>,
    pub model_flag: String,
    pub default_model: String,
    pub system_flag: Option<String>,
    pub provider_name: String,
    /// If true, send full conversation history via stdin instead of just the last user message.
    pub send_history: bool,
    /// Known models for this provider (used by `models()`).
    pub known_models: Vec<ModelInfo>,
    /// If true, the last element of `args` is "-" meaning prompt should be piped via stdin.
    pub stdin_mode: bool,
    /// Timeout for CLI execution.
    pub timeout: std::time::Duration,
    /// If true, add `--output-format json` and parse structured response (claude-cli).
    pub json_output: bool,
}

impl CliProvider {
    /// Create a provider that wraps `claude -p`.
    pub fn claude_cli() -> Self {
        let provider_name = "claude-cli".to_string();
        let known_models = vec![
            ModelInfo {
                id: "sonnet".into(),
                name: "Claude Sonnet".into(),
                provider: provider_name.clone(),
                context_window: 200_000,
                max_output_tokens: Some(16_384),
            },
            ModelInfo {
                id: "opus".into(),
                name: "Claude Opus".into(),
                provider: provider_name.clone(),
                context_window: 200_000,
                max_output_tokens: Some(32_768),
            },
            ModelInfo {
                id: "haiku".into(),
                name: "Claude Haiku".into(),
                provider: provider_name.clone(),
                context_window: 200_000,
                max_output_tokens: Some(8_192),
            },
        ];
        Self {
            command: "claude".into(),
            args: vec!["-p".into()],
            model_flag: "--model".into(),
            default_model: "sonnet".into(),
            system_flag: Some("--system-prompt".into()),
            provider_name,
            send_history: false,
            known_models,
            stdin_mode: false,
            timeout: std::time::Duration::from_secs(CLI_DEFAULT_TIMEOUT_SECS),
            json_output: true,
        }
    }

    /// Create a provider that wraps `codex exec`.
    pub fn codex_cli() -> Self {
        let provider_name = "codex-cli".to_string();
        let known_models = vec![
            ModelInfo {
                id: "o3".into(),
                name: "O3".into(),
                provider: provider_name.clone(),
                context_window: 200_000,
                max_output_tokens: None,
            },
            ModelInfo {
                id: "o4-mini".into(),
                name: "O4 Mini".into(),
                provider: provider_name.clone(),
                context_window: 200_000,
                max_output_tokens: None,
            },
        ];
        Self {
            command: "codex".into(),
            args: vec!["exec".into(), "--full-auto".into(), "-".into()],
            model_flag: "-m".into(),
            default_model: "o3".into(),
            system_flag: None,
            provider_name,
            send_history: false,
            known_models,
            stdin_mode: true,
            timeout: std::time::Duration::from_secs(CLI_DEFAULT_TIMEOUT_SECS),
            json_output: false,
        }
    }

    /// Create a provider from a user-defined config.
    pub fn from_config(cfg: &CliProviderConfig) -> Self {
        let stdin_mode = cfg.args.last().map(|a| a == "-").unwrap_or(false);
        let default_model = cfg
            .default_model
            .clone()
            .unwrap_or_else(|| "default".into());
        let provider_name = cfg.name.clone();
        let known_models = vec![ModelInfo {
            id: default_model.clone(),
            name: default_model.clone(),
            provider: provider_name.clone(),
            context_window: 200_000,
            max_output_tokens: None,
        }];
        let timeout_secs = cfg.timeout_secs.unwrap_or(CLI_DEFAULT_TIMEOUT_SECS);
        Self {
            command: cfg.command.clone(),
            args: cfg.args.clone(),
            model_flag: cfg.model_flag.clone(),
            default_model,
            system_flag: cfg.system_flag.clone(),
            provider_name,
            send_history: cfg.send_history,
            known_models,
            stdin_mode,
            timeout: std::time::Duration::from_secs(timeout_secs),
            json_output: false,
        }
    }

    /// Build the command and prompt text without executing. Useful for testing.
    pub fn build_command(
        &self,
        messages: &[Message],
        options: &ChatOptions,
    ) -> (tokio::process::Command, String) {
        let model = options
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());

        let mut cmd = tokio::process::Command::new(&self.command);

        // Add base args, but skip trailing "-" in stdin_mode (it's a sentinel, not a real arg
        // for some CLIs, but for codex it IS a real arg). Keep it.
        for arg in &self.args {
            cmd.arg(arg);
        }

        cmd.arg(&self.model_flag).arg(&model);

        if let Some(ref flag) = self.system_flag {
            if let Some(ref system) = options.system_prompt {
                cmd.arg(flag).arg(system);
            }
        }

        let prompt = if self.send_history {
            self.format_history(messages)
        } else {
            messages
                .iter()
                .rev()
                .find(|m| m.role == Role::User)
                .map(|m| m.text_content())
                .unwrap_or_default()
        };

        if !self.stdin_mode {
            cmd.arg(&prompt);
        }

        (cmd, prompt)
    }

    /// Format full conversation history as role-prefixed text.
    pub fn format_history(&self, messages: &[Message]) -> String {
        let mut out = String::new();
        for msg in messages {
            let prefix = match msg.role {
                Role::System => "System",
                Role::User => "User",
                Role::Assistant => "Assistant",
            };
            let text = msg.text_content();
            if !text.is_empty() {
                out.push_str(prefix);
                out.push_str(": ");
                out.push_str(&text);
                out.push('\n');
            }
        }
        out
    }

    /// Get a human-readable hint for when the CLI command is not found.
    pub fn not_found_hint(&self) -> String {
        match self.command.as_str() {
            "claude" => format!(
                "'claude' CLI not found. Install it with: npm install -g @anthropic-ai/claude-code"
            ),
            "codex" => {
                format!("'codex' CLI not found. Install it with: npm install -g @openai/codex")
            }
            _ => format!(
                "'{}' not found. Make sure it is installed and on your PATH.",
                self.command
            ),
        }
    }

    /// Check if the CLI command exists on PATH. Returns an error with install hint if not found.
    pub fn check_command_exists(&self) -> drbot_core::Result<()> {
        use std::process::Command;
        let result = Command::new("which").arg(&self.command).output();
        match result {
            Ok(output) if output.status.success() => Ok(()),
            _ => Err(drbot_core::Error::Provider(self.not_found_hint())),
        }
    }

    /// Parse JSON output from `claude -p --output-format json`.
    pub fn parse_json_output(
        &self,
        raw: &str,
        fallback_model: &str,
    ) -> drbot_core::Result<ChatResponse> {
        let json: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
            drbot_core::Error::Provider(format!(
                "Failed to parse {} JSON output: {}",
                self.command, e
            ))
        })?;

        if json
            .get("is_error")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let error_msg = json
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(drbot_core::Error::Provider(format!(
                "{}: {}",
                self.command, error_msg
            )));
        }

        let content = json
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Extract actual model from modelUsage keys
        let model = json
            .get("modelUsage")
            .and_then(|v| v.as_object())
            .and_then(|m| m.keys().next())
            .map(|s| s.to_string())
            .unwrap_or_else(|| fallback_model.to_string());

        let usage = json.get("usage").and_then(|u| {
            let input = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let output = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if input > 0 || output > 0 {
                Some(Usage {
                    input_tokens: input,
                    output_tokens: output,
                })
            } else {
                None
            }
        });

        let cost = json.get("total_cost_usd").and_then(|v| v.as_f64());
        if let Some(cost_usd) = cost {
            tracing::info!("{} cost: ${:.6}", self.command, cost_usd);
        }

        Ok(ChatResponse {
            content,
            model,
            usage,
            stop_reason: Some("end_turn".into()),
            tool_uses: vec![],
        })
    }
}

#[async_trait]
impl Provider for CliProvider {
    async fn chat(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<ChatResponse> {
        let model = options
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());
        let (mut cmd, prompt) = self.build_command(messages, &options);

        // For chat(), use JSON output format to get structured response with usage stats
        if self.json_output {
            cmd.arg("--output-format").arg("json");
        }

        if self.stdin_mode {
            cmd.stdin(std::process::Stdio::piped());
        } else {
            cmd.stdin(std::process::Stdio::null());
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let command_name = self.command.clone();
        let timeout = self.timeout;

        let result = tokio::time::timeout(timeout, async {
            let mut child = cmd.spawn().map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    drbot_core::Error::Provider(self.not_found_hint())
                } else {
                    drbot_core::Error::Provider(format!("Failed to run {}: {}", command_name, e))
                }
            })?;

            if self.stdin_mode {
                if let Some(mut stdin) = child.stdin.take() {
                    use tokio::io::AsyncWriteExt;
                    let _ = stdin.write_all(prompt.as_bytes()).await;
                    drop(stdin);
                }
            }

            let output = child.wait_with_output().await.map_err(|e| {
                drbot_core::Error::Provider(format!("{} failed: {}", command_name, e))
            })?;

            Ok::<_, drbot_core::Error>((output, command_name.clone()))
        })
        .await;

        let (output, _) = match result {
            Ok(inner) => inner?,
            Err(_) => {
                return Err(drbot_core::Error::Timeout(format!(
                    "{} timed out after {}s",
                    self.command,
                    timeout.as_secs()
                )));
            }
        };

        let stderr_text = String::from_utf8_lossy(&output.stderr);
        if !stderr_text.trim().is_empty() {
            if output.status.success() {
                for line in stderr_text.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        tracing::warn!("{} stderr: {}", self.command, trimmed);
                    }
                }
            } else {
                return Err(drbot_core::Error::Provider(format!(
                    "{} exited with {}: {}",
                    self.command,
                    output.status,
                    stderr_text.trim()
                )));
            }
        } else if !output.status.success() {
            return Err(drbot_core::Error::Provider(format!(
                "{} exited with {}",
                self.command, output.status
            )));
        }

        let raw_stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        if self.json_output {
            self.parse_json_output(&raw_stdout, &model)
        } else {
            Ok(ChatResponse {
                content: raw_stdout,
                model,
                usage: None,
                stop_reason: Some("end_turn".into()),
                tool_uses: vec![],
            })
        }
    }

    async fn stream(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>> {
        let model = options
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());
        let (mut cmd, prompt) = self.build_command(messages, &options);

        // For streaming with JSON-capable CLIs, use stream-json for incremental deltas + usage
        let json_stream = self.json_output;
        if json_stream {
            cmd.arg("--output-format").arg("stream-json");
            cmd.arg("--include-partial-messages");
        }

        if self.stdin_mode {
            cmd.stdin(std::process::Stdio::piped());
        } else {
            cmd.stdin(std::process::Stdio::null());
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                drbot_core::Error::Provider(self.not_found_hint())
            } else {
                drbot_core::Error::Provider(format!("Failed to run {}: {}", self.command, e))
            }
        })?;

        if self.stdin_mode {
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(prompt.as_bytes()).await;
                drop(stdin);
            }
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| drbot_core::Error::Provider("Failed to capture stdout".into()))?;
        let stderr = child.stderr.take();

        let command_name = self.command.clone();
        let timeout = self.timeout;

        Ok(Box::pin(async_stream::stream! {
            use tokio::io::{AsyncBufReadExt, BufReader};

            let stdout_reader = BufReader::new(stdout);
            let mut stdout_lines = stdout_reader.lines();

            // Spawn a task to collect stderr lines concurrently
            let stderr_handle = stderr.map(|se| {
                tokio::spawn(async move {
                    let reader = BufReader::new(se);
                    let mut lines = reader.lines();
                    let mut collected = Vec::new();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let trimmed = line.trim().to_string();
                        if !trimmed.is_empty() {
                            collected.push(trimmed);
                        }
                    }
                    collected
                })
            });

            let mut timed_out = false;
            let mut _got_result = false;
            let mut final_usage: Option<Usage> = None;
            let mut started = false;
            let deadline = tokio::time::Instant::now() + timeout;

            // For plain text mode, track newlines between lines
            let mut first_text_line = true;

            loop {
                let line_result = tokio::time::timeout_at(deadline, stdout_lines.next_line()).await;
                match line_result {
                    Ok(Ok(Some(line))) => {
                        if json_stream {
                            // Parse NDJSON line
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                                let msg_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                match msg_type {
                                    "system" => {
                                        // Init message — extract model if available
                                        let m = json.get("model")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or(&model)
                                            .to_string();
                                        if !started {
                                            started = true;
                                            yield StreamEvent::Start { model: m };
                                        }
                                    }
                                    "stream_event" => {
                                        if !started {
                                            started = true;
                                            yield StreamEvent::Start { model: model.clone() };
                                        }
                                        // Extract text delta: event.delta.text
                                        if let Some(event) = json.get("event") {
                                            let delta_type = event
                                                .get("delta")
                                                .and_then(|d| d.get("type"))
                                                .and_then(|v| v.as_str());
                                            if delta_type == Some("text_delta") {
                                                if let Some(text) = event
                                                    .get("delta")
                                                    .and_then(|d| d.get("text"))
                                                    .and_then(|v| v.as_str())
                                                {
                                                    if !text.is_empty() {
                                                        yield StreamEvent::Delta { content: text.to_string() };
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    "result" => {
                                        _got_result = true;
                                        // Extract usage from result
                                        final_usage = json.get("usage").and_then(|u| {
                                            let input = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                            let output = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                            if input > 0 || output > 0 {
                                                Some(Usage { input_tokens: input, output_tokens: output })
                                            } else {
                                                None
                                            }
                                        });

                                        if let Some(cost) = json.get("total_cost_usd").and_then(|v| v.as_f64()) {
                                            tracing::info!("{} cost: ${:.6}", command_name, cost);
                                        }

                                        let is_error = json.get("is_error").and_then(|v| v.as_bool()).unwrap_or(false);
                                        if is_error {
                                            let err_msg = json.get("result").and_then(|v| v.as_str()).unwrap_or("unknown error");
                                            yield StreamEvent::Error { message: format!("{}: {}", command_name, err_msg) };
                                        }
                                    }
                                    // "assistant", "user" — skip, we get content from stream_event deltas
                                    _ => {}
                                }
                            }
                            // Silently skip lines that aren't valid JSON
                        } else {
                            // Plain text mode
                            if !started {
                                started = true;
                                yield StreamEvent::Start { model: model.clone() };
                            }
                            if first_text_line {
                                first_text_line = false;
                            } else {
                                yield StreamEvent::Delta { content: "\n".into() };
                            }
                            yield StreamEvent::Delta { content: line };
                        }
                    }
                    Ok(Ok(None)) => break, // EOF
                    Ok(Err(e)) => {
                        yield StreamEvent::Error {
                            message: format!("{} stdout read error: {}", command_name, e),
                        };
                        break;
                    }
                    Err(_) => {
                        timed_out = true;
                        yield StreamEvent::Error {
                            message: format!("{} timed out after {}s", command_name, timeout.as_secs()),
                        };
                        let _ = child.kill().await;
                        break;
                    }
                }
            }

            // Ensure we emitted Start even if no lines were received
            if !started {
                yield StreamEvent::Start { model: model.clone() };
            }

            // Emit stderr lines as errors
            if let Some(handle) = stderr_handle {
                if let Ok(stderr_lines) = handle.await {
                    for line in stderr_lines {
                        yield StreamEvent::Error { message: format!("{} stderr: {}", command_name, line) };
                    }
                }
            }

            if !timed_out {
                let status = child.wait().await;
                if let Ok(exit) = &status {
                    if !exit.success() {
                        yield StreamEvent::Error {
                            message: format!("{} exited with {}", command_name, exit),
                        };
                    }
                }
            }

            yield StreamEvent::Stop {
                reason: if timed_out { "timeout".into() } else { "end_turn".into() },
                usage: final_usage,
            };
        }))
    }

    fn models(&self) -> Vec<ModelInfo> {
        self.known_models.clone()
    }

    fn name(&self) -> &str {
        &self.provider_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChatOptions, StreamEvent};
    use drbot_core::message::Message;
    use futures::StreamExt;

    #[test]
    fn claude_cli_basic_command() {
        let provider = CliProvider::claude_cli();
        let messages = vec![Message::user("What is 2+2?")];
        let options = ChatOptions::default();

        let (cmd, prompt) = provider.build_command(&messages, &options);
        let prog = cmd.as_std().get_program().to_str().unwrap().to_string();
        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();

        assert_eq!(prog, "claude");
        assert_eq!(args, vec!["-p", "--model", "sonnet", "What is 2+2?"]);
        assert_eq!(prompt, "What is 2+2?");
    }

    #[test]
    fn claude_cli_with_model_override() {
        let provider = CliProvider::claude_cli();
        let messages = vec![Message::user("Hello")];
        let options = ChatOptions {
            model: Some("opus".into()),
            ..Default::default()
        };

        let (cmd, _prompt) = provider.build_command(&messages, &options);
        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();

        assert_eq!(args, vec!["-p", "--model", "opus", "Hello"]);
    }

    #[test]
    fn claude_cli_with_system_prompt() {
        let provider = CliProvider::claude_cli();
        let messages = vec![Message::user("Hi")];
        let options = ChatOptions {
            system_prompt: Some("You are a pirate.".into()),
            ..Default::default()
        };

        let (cmd, _prompt) = provider.build_command(&messages, &options);
        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();

        assert_eq!(
            args,
            vec![
                "-p",
                "--model",
                "sonnet",
                "--system-prompt",
                "You are a pirate.",
                "Hi"
            ]
        );
    }

    #[test]
    fn codex_cli_stdin_mode() {
        let provider = CliProvider::codex_cli();
        assert!(provider.stdin_mode);

        let messages = vec![Message::user("Fix the bug")];
        let options = ChatOptions::default();

        let (cmd, prompt) = provider.build_command(&messages, &options);
        let prog = cmd.as_std().get_program().to_str().unwrap().to_string();
        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();

        // In stdin_mode, the prompt is NOT appended as an arg
        assert_eq!(prog, "codex");
        assert_eq!(args, vec!["exec", "--full-auto", "-", "-m", "o3"]);
        assert_eq!(prompt, "Fix the bug");
    }

    #[test]
    fn codex_cli_no_system_flag() {
        let provider = CliProvider::codex_cli();
        let messages = vec![Message::user("Hello")];
        let options = ChatOptions {
            system_prompt: Some("Be helpful".into()),
            ..Default::default()
        };

        let (cmd, _prompt) = provider.build_command(&messages, &options);
        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();

        // system_flag is None, so --system-prompt should NOT appear
        assert!(!args.contains(&"--system-prompt".to_string()));
        assert!(!args.contains(&"Be helpful".to_string()));
    }

    #[test]
    fn history_mode_formats_messages() {
        let mut provider = CliProvider::claude_cli();
        provider.send_history = true;

        let messages = vec![
            Message::user("What is 2+2?"),
            Message::assistant("4"),
            Message::user("And 3+3?"),
        ];

        let history = provider.format_history(&messages);
        assert_eq!(
            history,
            "User: What is 2+2?\nAssistant: 4\nUser: And 3+3?\n"
        );
    }

    #[test]
    fn history_mode_uses_full_history_as_prompt() {
        let mut provider = CliProvider::claude_cli();
        provider.send_history = true;

        let messages = vec![
            Message::user("Hello"),
            Message::assistant("Hi there!"),
            Message::user("How are you?"),
        ];
        let options = ChatOptions::default();

        let (_cmd, prompt) = provider.build_command(&messages, &options);
        assert!(prompt.contains("User: Hello"));
        assert!(prompt.contains("Assistant: Hi there!"));
        assert!(prompt.contains("User: How are you?"));
    }

    #[test]
    fn from_config_basic() {
        let cfg = CliProviderConfig {
            name: "my-llm".into(),
            command: "my-llm-cli".into(),
            args: vec!["run".into()],
            model_flag: "--model".into(),
            default_model: Some("llama3".into()),
            system_flag: Some("--system".into()),
            send_history: false,
            timeout_secs: None,
        };

        let provider = CliProvider::from_config(&cfg);
        assert_eq!(provider.provider_name, "my-llm");
        assert_eq!(provider.command, "my-llm-cli");
        assert_eq!(provider.default_model, "llama3");
        assert!(!provider.stdin_mode);
        assert!(!provider.send_history);
    }

    #[test]
    fn from_config_stdin_mode_detection() {
        let cfg = CliProviderConfig {
            name: "custom".into(),
            command: "custom-ai".into(),
            args: vec!["chat".into(), "-".into()],
            model_flag: "-m".into(),
            default_model: None,
            system_flag: None,
            send_history: true,
            timeout_secs: None,
        };

        let provider = CliProvider::from_config(&cfg);
        assert!(provider.stdin_mode);
        assert!(provider.send_history);
        assert_eq!(provider.default_model, "default");
    }

    #[test]
    fn models_returns_known_models() {
        let claude = CliProvider::claude_cli();
        let models = claude.models();
        assert_eq!(models.len(), 3);
        assert!(models.iter().any(|m| m.id == "sonnet"));
        assert!(models.iter().any(|m| m.id == "opus"));
        assert!(models.iter().any(|m| m.id == "haiku"));

        let codex = CliProvider::codex_cli();
        let models = codex.models();
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.id == "o3"));
        assert!(models.iter().any(|m| m.id == "o4-mini"));
    }

    #[test]
    fn not_found_hints() {
        let claude = CliProvider::claude_cli();
        assert!(claude.not_found_hint().contains("npm install"));
        assert!(claude
            .not_found_hint()
            .contains("@anthropic-ai/claude-code"));

        let codex = CliProvider::codex_cli();
        assert!(codex.not_found_hint().contains("npm install"));
        assert!(codex.not_found_hint().contains("@openai/codex"));

        let custom = CliProvider::from_config(&CliProviderConfig {
            name: "custom".into(),
            command: "my-tool".into(),
            args: vec![],
            model_flag: "--model".into(),
            default_model: None,
            system_flag: None,
            send_history: false,
            timeout_secs: None,
        });
        assert!(custom.not_found_hint().contains("my-tool"));
        assert!(custom.not_found_hint().contains("PATH"));
    }

    #[test]
    fn provider_name() {
        assert_eq!(CliProvider::claude_cli().name(), "claude-cli");
        assert_eq!(CliProvider::codex_cli().name(), "codex-cli");
    }

    #[test]
    fn last_user_message_is_used() {
        let provider = CliProvider::claude_cli();
        let messages = vec![
            Message::user("First question"),
            Message::assistant("First answer"),
            Message::user("Second question"),
        ];
        let options = ChatOptions::default();
        let (_cmd, prompt) = provider.build_command(&messages, &options);
        assert_eq!(prompt, "Second question");
    }

    #[test]
    fn default_timeout() {
        let claude = CliProvider::claude_cli();
        assert_eq!(
            claude.timeout,
            std::time::Duration::from_secs(CLI_DEFAULT_TIMEOUT_SECS)
        );

        let codex = CliProvider::codex_cli();
        assert_eq!(
            codex.timeout,
            std::time::Duration::from_secs(CLI_DEFAULT_TIMEOUT_SECS)
        );
    }

    #[test]
    fn custom_timeout_from_config() {
        let cfg = CliProviderConfig {
            name: "slow-llm".into(),
            command: "slow-llm".into(),
            args: vec![],
            model_flag: "--model".into(),
            default_model: None,
            system_flag: None,
            send_history: false,
            timeout_secs: Some(300),
        };
        let provider = CliProvider::from_config(&cfg);
        assert_eq!(provider.timeout, std::time::Duration::from_secs(300));
    }

    #[test]
    fn config_timeout_defaults_when_none() {
        let cfg = CliProviderConfig {
            name: "fast-llm".into(),
            command: "fast-llm".into(),
            args: vec![],
            model_flag: "--model".into(),
            default_model: None,
            system_flag: None,
            send_history: false,
            timeout_secs: None,
        };
        let provider = CliProvider::from_config(&cfg);
        assert_eq!(
            provider.timeout,
            std::time::Duration::from_secs(CLI_DEFAULT_TIMEOUT_SECS)
        );
    }

    #[tokio::test]
    async fn chat_timeout_fires() {
        let mut provider = CliProvider::claude_cli();
        provider.command = "bash".into();
        provider.args = vec!["-c".into(), "sleep 10".into()];
        provider.model_flag = String::new();
        provider.default_model = String::new();
        provider.system_flag = None;
        provider.stdin_mode = true; // avoid appending prompt as arg
        provider.timeout = std::time::Duration::from_millis(100);

        let messages = vec![Message::user("ignored")];
        let options = ChatOptions::default();
        let result = provider.chat(&messages, options).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("timed out"),
            "expected timeout error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn stream_timeout_fires() {
        let mut provider = CliProvider::claude_cli();
        provider.command = "bash".into();
        provider.args = vec!["-c".into(), "sleep 10".into()];
        provider.model_flag = String::new();
        provider.default_model = String::new();
        provider.system_flag = None;
        provider.stdin_mode = true;
        provider.timeout = std::time::Duration::from_millis(100);

        let messages = vec![Message::user("ignored")];
        let options = ChatOptions::default();
        let stream = provider.stream(&messages, options).await.unwrap();

        let events: Vec<StreamEvent> = stream.collect().await;
        let has_timeout_error = events
            .iter()
            .any(|e| matches!(e, StreamEvent::Error { message } if message.contains("timed out")));
        assert!(
            has_timeout_error,
            "expected timeout error in stream events: {:?}",
            events
        );
    }

    #[tokio::test]
    async fn chat_stderr_on_success_is_warning() {
        // Use a command that writes to stderr but exits 0
        let mut provider = CliProvider::claude_cli();
        provider.command = "bash".into();
        provider.args = vec![
            "-c".into(),
            "echo 'hello' && echo 'warning: something' >&2".into(),
        ];
        provider.model_flag = String::new();
        provider.default_model = String::new();
        provider.system_flag = None;
        provider.stdin_mode = false;
        provider.json_output = false; // plain text output for this test

        let messages = vec![Message::user("ignored")];
        let options = ChatOptions::default();
        let result = provider.chat(&messages, options).await;

        // Should succeed despite stderr output
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, "hello");
    }

    #[tokio::test]
    async fn stream_stderr_emitted_as_error_events() {
        let mut provider = CliProvider::claude_cli();
        provider.command = "bash".into();
        provider.args = vec![
            "-c".into(),
            "echo 'output' && echo 'stderr warning' >&2".into(),
        ];
        provider.model_flag = String::new();
        provider.default_model = String::new();
        provider.system_flag = None;
        provider.stdin_mode = false;
        provider.json_output = false;

        let messages = vec![Message::user("ignored")];
        let options = ChatOptions::default();
        let stream = provider.stream(&messages, options).await.unwrap();

        let events: Vec<StreamEvent> = stream.collect().await;

        let has_output = events
            .iter()
            .any(|e| matches!(e, StreamEvent::Delta { content } if content == "output"));
        assert!(has_output, "expected 'output' delta, got: {:?}", events);

        let has_stderr = events.iter().any(
            |e| matches!(e, StreamEvent::Error { message } if message.contains("stderr warning")),
        );
        assert!(has_stderr, "expected stderr error event, got: {:?}", events);
    }

    #[test]
    fn claude_cli_has_json_output() {
        let provider = CliProvider::claude_cli();
        assert!(provider.json_output);
    }

    #[test]
    fn codex_cli_no_json_output() {
        let provider = CliProvider::codex_cli();
        assert!(!provider.json_output);
    }

    #[test]
    fn parse_json_output_success() {
        let provider = CliProvider::claude_cli();
        let json = r#"{
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "result": "The answer is 42.",
            "total_cost_usd": 0.003,
            "usage": {
                "input_tokens": 15,
                "output_tokens": 8
            },
            "modelUsage": {
                "claude-sonnet-4-5-20250929": {
                    "inputTokens": 15,
                    "outputTokens": 8
                }
            }
        }"#;

        let response = provider.parse_json_output(json, "sonnet").unwrap();
        assert_eq!(response.content, "The answer is 42.");
        assert_eq!(response.model, "claude-sonnet-4-5-20250929");
        let usage = response.usage.unwrap();
        assert_eq!(usage.input_tokens, 15);
        assert_eq!(usage.output_tokens, 8);
    }

    #[test]
    fn parse_json_output_error() {
        let provider = CliProvider::claude_cli();
        let json = r#"{
            "type": "result",
            "subtype": "error_during_execution",
            "is_error": true,
            "result": "Something went wrong"
        }"#;

        let result = provider.parse_json_output(json, "sonnet");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Something went wrong"));
    }

    #[test]
    fn parse_json_output_no_usage() {
        let provider = CliProvider::claude_cli();
        let json = r#"{
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "result": "Hello"
        }"#;

        let response = provider.parse_json_output(json, "sonnet").unwrap();
        assert_eq!(response.content, "Hello");
        assert_eq!(response.model, "sonnet"); // fallback since no modelUsage
        assert!(response.usage.is_none());
    }

    #[test]
    fn parse_json_output_invalid_json() {
        let provider = CliProvider::claude_cli();
        let result = provider.parse_json_output("not json at all", "sonnet");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("parse"));
    }

    #[test]
    fn check_command_exists_for_known_binary() {
        let mut provider = CliProvider::claude_cli();
        provider.command = "bash".into(); // bash always exists
        assert!(provider.check_command_exists().is_ok());
    }

    #[test]
    fn check_command_exists_for_missing_binary() {
        let mut provider = CliProvider::claude_cli();
        provider.command = "nonexistent_binary_that_does_not_exist_12345".into();
        let result = provider.check_command_exists();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn chat_json_output_parses_response() {
        // Use a bash command that outputs JSON in the claude format
        let mut provider = CliProvider::claude_cli();
        provider.command = "bash".into();
        provider.args = vec!["-c".into(), r#"echo '{"type":"result","subtype":"success","is_error":false,"result":"hello from json","usage":{"input_tokens":5,"output_tokens":3},"modelUsage":{"test-model":{}}}'"#.into()];
        provider.model_flag = String::new();
        provider.default_model = String::new();
        provider.system_flag = None;
        provider.stdin_mode = true;

        let messages = vec![Message::user("test")];
        let options = ChatOptions::default();
        let response = provider.chat(&messages, options).await.unwrap();

        assert_eq!(response.content, "hello from json");
        assert_eq!(response.model, "test-model");
        let usage = response.usage.unwrap();
        assert_eq!(usage.input_tokens, 5);
        assert_eq!(usage.output_tokens, 3);
    }

    #[tokio::test]
    async fn stream_json_parses_deltas_and_usage() {
        // Simulate claude --output-format stream-json --include-partial-messages
        let mut provider = CliProvider::claude_cli();
        provider.command = "bash".into();
        provider.args = vec!["-c".into(), concat!(
            r#"echo '{"type":"system","model":"claude-sonnet-4-5-20250929","tools":[]}'"#, " && ",
            r#"echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}'"#, " && ",
            r#"echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}}'"#, " && ",
            r#"echo '{"type":"result","subtype":"success","is_error":false,"result":"Hello world","total_cost_usd":0.005,"usage":{"input_tokens":10,"output_tokens":5},"modelUsage":{"claude-sonnet-4-5-20250929":{}}}'"#
        ).into()];
        provider.model_flag = String::new();
        provider.default_model = String::new();
        provider.system_flag = None;
        provider.stdin_mode = true;

        let messages = vec![Message::user("test")];
        let options = ChatOptions::default();
        let stream = provider.stream(&messages, options).await.unwrap();
        let events: Vec<StreamEvent> = stream.collect().await;

        // Should have Start with model from system init
        let start = events
            .iter()
            .find(|e| matches!(e, StreamEvent::Start { .. }));
        assert!(start.is_some(), "expected Start event, got: {:?}", events);
        if let Some(StreamEvent::Start { model }) = start {
            assert_eq!(model, "claude-sonnet-4-5-20250929");
        }

        // Should have two Delta events: "Hello" and " world"
        let deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::Delta { content } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["Hello", " world"]);

        // Should have Stop with usage
        let stop = events
            .iter()
            .find(|e| matches!(e, StreamEvent::Stop { .. }));
        assert!(stop.is_some(), "expected Stop event, got: {:?}", events);
        if let Some(StreamEvent::Stop { usage, reason, .. }) = stop {
            assert_eq!(reason, "end_turn");
            let u = usage.as_ref().expect("expected usage in Stop event");
            assert_eq!(u.input_tokens, 10);
            assert_eq!(u.output_tokens, 5);
        }
    }

    #[tokio::test]
    async fn stream_json_error_result() {
        let mut provider = CliProvider::claude_cli();
        provider.command = "bash".into();
        provider.args = vec!["-c".into(), concat!(
            r#"echo '{"type":"system","model":"test-model","tools":[]}'"#, " && ",
            r#"echo '{"type":"result","subtype":"error_during_execution","is_error":true,"result":"Something failed","usage":{"input_tokens":5,"output_tokens":0}}'"#
        ).into()];
        provider.model_flag = String::new();
        provider.default_model = String::new();
        provider.system_flag = None;
        provider.stdin_mode = true;

        let messages = vec![Message::user("test")];
        let options = ChatOptions::default();
        let stream = provider.stream(&messages, options).await.unwrap();
        let events: Vec<StreamEvent> = stream.collect().await;

        let has_error = events.iter().any(
            |e| matches!(e, StreamEvent::Error { message } if message.contains("Something failed")),
        );
        assert!(has_error, "expected error event, got: {:?}", events);
    }

    #[tokio::test]
    async fn stream_json_skips_non_text_deltas() {
        let mut provider = CliProvider::claude_cli();
        provider.command = "bash".into();
        provider.args = vec!["-c".into(), concat!(
            r#"echo '{"type":"system","model":"test-model","tools":[]}'"#, " && ",
            r#"echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}'"#, " && ",
            r#"echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hi"}}}'"#, " && ",
            r#"echo '{"type":"stream_event","event":{"type":"content_block_stop","index":0}}'"#, " && ",
            r#"echo '{"type":"result","subtype":"success","is_error":false,"result":"Hi","usage":{"input_tokens":3,"output_tokens":1}}'"#
        ).into()];
        provider.model_flag = String::new();
        provider.default_model = String::new();
        provider.system_flag = None;
        provider.stdin_mode = true;

        let messages = vec![Message::user("test")];
        let options = ChatOptions::default();
        let stream = provider.stream(&messages, options).await.unwrap();
        let events: Vec<StreamEvent> = stream.collect().await;

        let deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::Delta { content } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["Hi"]);
    }

    #[tokio::test]
    async fn stream_plain_text_still_works() {
        let mut provider = CliProvider::codex_cli();
        provider.command = "bash".into();
        provider.args = vec!["-c".into(), "printf 'line1\\nline2\\nline3\\n'".into()];
        provider.model_flag = String::new();
        provider.default_model = "test".into();
        provider.system_flag = None;
        provider.stdin_mode = true;

        let messages = vec![Message::user("test")];
        let options = ChatOptions::default();
        let stream = provider.stream(&messages, options).await.unwrap();
        let events: Vec<StreamEvent> = stream.collect().await;

        let deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::Delta { content } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["line1", "\n", "line2", "\n", "line3"]);

        let stop = events
            .iter()
            .find(|e| matches!(e, StreamEvent::Stop { .. }));
        if let Some(StreamEvent::Stop { usage, .. }) = stop {
            assert!(usage.is_none());
        }
    }
}
