//! Agent tool shims used by the OpenClaw-compatible gateway.
//!
//! OpenClaw skills often describe HTTP APIs (Colosseum, Moltbook, etc). drbot's
//! OpenClaw agent runner can expose safe, allowlisted tools for these APIs so
//! models don't need to shell out to curl (and so secrets stay scoped to the
//! correct domains).

use async_trait::async_trait;
use crate::openclaw_exec_approvals::ExecApprovalRequestPayload;
use crate::state::GatewayState;
use drbot_agents::{AgentError, AgentTool, Result};
use drbot_core::message::OutgoingMessage;
use serde_json::{json, Value};

pub struct AgentsListTool;

#[async_trait]
impl AgentTool for AgentsListTool {
    fn name(&self) -> &str {
        "agents_list"
    }

    fn description(&self) -> &str {
        "List available agent ids (drbot currently exposes a single default agent)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
        })
    }

    async fn execute(&self, _args: Value) -> Result<String> {
        let payload = json!({
            "requester": "default",
            "allowAny": false,
            "agents": [{
                "id": "default",
                "name": "drbot",
                "configured": true,
            }],
        });
        serde_json::to_string_pretty(&payload).map_err(|e| AgentError::ToolError(e.to_string()))
    }
}

pub struct ColosseumRequestTool {
    state: GatewayState,
}

impl ColosseumRequestTool {
    pub fn new(state: GatewayState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentTool for ColosseumRequestTool {
    fn name(&self) -> &str {
        "colosseum.request"
    }

    fn description(&self) -> &str {
        "Call the Colosseum Agent Hackathon API (base URL pinned; auth handled)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "method": { "type": "string", "description": "HTTP method (GET, POST, PUT, PATCH, DELETE). Default: GET." },
                "path": { "type": "string", "description": "API path (e.g. /forum/posts or /my-project)." },
                "query": { "type": "object", "description": "Query parameters (object; string/number/bool/array-of-string values)." },
                "body": { "description": "JSON body (object) or raw JSON string." },
                "timeoutMs": { "type": "number", "description": "Request timeout in milliseconds." },
                "dryRun": { "type": "boolean", "description": "If true, return a request preview without sending." }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let method = args
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .trim()
            .to_string();
        let method_upper = method.trim().to_uppercase();
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if path.is_empty() {
            return Err(AgentError::ToolError("path required".to_string()));
        }

        let query_value = args.get("query").cloned();
        let body_value = args.get("body").cloned();
        let timeout_ms = args.get("timeoutMs").and_then(|v| v.as_u64());
        let dry_run = args.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);

        let query = query_value.as_ref();
        let body = body_value.as_ref();

        let is_write =
            matches!(method_upper.as_str(), "POST" | "PUT" | "PATCH" | "DELETE");
        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_COLOSSEUM_WRITE")
            .ok()
            .as_deref()
            == Some("1");
        let mut allow_write = allow_write_by_env;
        if is_write && !dry_run && !allow_write_by_env {
            let mut url = "https://agents.colosseum.com/api".to_string();
            if path.starts_with('/') {
                url.push_str(&path);
            } else {
                url.push('/');
                url.push_str(&path);
            }
            let approval = ExecApprovalRequestPayload {
                command: format!("colosseum.request {} {}", method_upper, path),
                cwd: None,
                host: Some("colosseum".to_string()),
                security: Some("integration-http-write".to_string()),
                ask: Some(format!(
                    "Allow Colosseum API write request? {} {}",
                    method_upper, path
                )),
                agent_id: Some("default".to_string()),
                resolved_path: Some(url),
                session_key: None,
            };
            crate::openclaw_exec_approvals::ensure_tool_write_allowed(
                &self.state,
                "colosseum.request",
                approval,
                120_000,
            )
            .await
            .map_err(|e| AgentError::ToolError(format!("{}: {}", e.code, e.message)))?;
            allow_write = true;
        }

        let res = crate::colosseum::colosseum_request(
            &method_upper,
            &path,
            query,
            body,
            timeout_ms,
            dry_run,
            allow_write,
        )
        .await;

        match res {
            Ok(payload) => serde_json::to_string_pretty(&payload)
                .map_err(|e| AgentError::ToolError(e.to_string())),
            Err(err) => {
                let mut msg = format!("{}: {}", err.code, err.message);
                if let Some(details) = err.details {
                    if let Ok(pretty) = serde_json::to_string_pretty(&details) {
                        msg.push_str("\n");
                        msg.push_str(&pretty);
                    }
                }
                Err(AgentError::ToolError(msg))
            }
        }
    }
}

pub struct MoltbookRequestTool {
    state: GatewayState,
}

impl MoltbookRequestTool {
    pub fn new(state: GatewayState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentTool for MoltbookRequestTool {
    fn name(&self) -> &str {
        "moltbook.request"
    }

    fn description(&self) -> &str {
        "Call the Moltbook API (base URL pinned; auth handled)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "method": { "type": "string", "description": "HTTP method (GET, POST, PUT, PATCH, DELETE). Default: GET." },
                "path": { "type": "string", "description": "API path (e.g. /posts or /agents/status)." },
                "query": { "type": "object", "description": "Query parameters (object; string/number/bool/array-of-string values)." },
                "body": { "description": "JSON body (object) or raw JSON string." },
                "timeoutMs": { "type": "number", "description": "Request timeout in milliseconds." },
                "dryRun": { "type": "boolean", "description": "If true, return a request preview without sending." }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let method = args
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .trim()
            .to_string();
        let method_upper = method.trim().to_uppercase();
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if path.is_empty() {
            return Err(AgentError::ToolError("path required".to_string()));
        }

        let query_value = args.get("query").cloned();
        let body_value = args.get("body").cloned();
        let timeout_ms = args.get("timeoutMs").and_then(|v| v.as_u64());
        let dry_run = args.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);

        let query = query_value.as_ref();
        let body = body_value.as_ref();

        let is_write =
            matches!(method_upper.as_str(), "POST" | "PUT" | "PATCH" | "DELETE");
        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_MOLTBOOK_WRITE")
            .ok()
            .as_deref()
            == Some("1");
        let mut allow_write = allow_write_by_env;
        if is_write && !dry_run && !allow_write_by_env {
            let mut url = "https://www.moltbook.com/api/v1".to_string();
            if path.starts_with('/') {
                url.push_str(&path);
            } else {
                url.push('/');
                url.push_str(&path);
            }
            let approval = ExecApprovalRequestPayload {
                command: format!("moltbook.request {} {}", method_upper, path),
                cwd: None,
                host: Some("moltbook".to_string()),
                security: Some("integration-http-write".to_string()),
                ask: Some(format!(
                    "Allow Moltbook API write request? {} {}",
                    method_upper, path
                )),
                agent_id: Some("default".to_string()),
                resolved_path: Some(url),
                session_key: None,
            };
            crate::openclaw_exec_approvals::ensure_tool_write_allowed(
                &self.state,
                "moltbook.request",
                approval,
                120_000,
            )
            .await
            .map_err(|e| AgentError::ToolError(format!("{}: {}", e.code, e.message)))?;
            allow_write = true;
        }

        let res = crate::moltbook::moltbook_request(
            &method_upper,
            &path,
            query,
            body,
            timeout_ms,
            dry_run,
            allow_write,
        )
        .await;

        match res {
            Ok(payload) => serde_json::to_string_pretty(&payload)
                .map_err(|e| AgentError::ToolError(e.to_string())),
            Err(err) => {
                let mut msg = format!("{}: {}", err.code, err.message);
                if let Some(details) = err.details {
                    if let Ok(pretty) = serde_json::to_string_pretty(&details) {
                        msg.push_str("\n");
                        msg.push_str(&pretty);
                    }
                }
                Err(AgentError::ToolError(msg))
            }
        }
    }
}

pub struct SendTool {
    state: GatewayState,
}

impl SendTool {
    pub fn new(state: GatewayState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentTool for SendTool {
    fn name(&self) -> &str {
        "send"
    }

    fn description(&self) -> &str {
        "Send an outbound message via a configured drbot channel (approval-gated)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string", "description": "Channel type (telegram, slack, discord, signal, whatsapp, imessage, matrix, webchat). Optional if a default channel is configured." },
                "to": { "type": "string", "description": "Recipient id (channel-specific). For webchat: a client UUID string. You may also use '<channel>:<to>' when channel is omitted." },
                "message": { "type": "string", "description": "Message text." },
                "idempotencyKey": { "type": "string", "description": "Optional idempotency key (recommended for retries)." },
                "approvalTimeoutMs": { "type": "number", "description": "Approval timeout in ms (default 120000)." },
                "dryRun": { "type": "boolean", "description": "If true, return a request preview without sending." }
            },
            "required": ["to", "message"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let mut channel = args
            .get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let mut to = args
            .get("to")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let dry_run = args.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);
        let timeout_ms = args
            .get("approvalTimeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120_000)
            .max(1);

        if to.is_empty() || message.is_empty() {
            return Err(AgentError::ToolError("to and message required".to_string()));
        }

        if channel.is_empty() {
            if let Some((left, right)) = to.split_once(':') {
                if self.state.channel_manager().has_channel(left) {
                    channel = left.trim().to_string();
                    to = right.trim().to_string();
                }
            }
        }
        if channel.is_empty() {
            if let Some(default) = self.state.channel_manager().default_channel() {
                channel = default.to_string();
            }
        }
        if channel.is_empty() {
            return Err(AgentError::ToolError(
                "channel required (or configure a default channel)".to_string(),
            ));
        }

        if dry_run {
            let preview = json!({
                "ok": true,
                "dryRun": true,
                "channel": channel,
                "to": to,
                "message": message,
            });
            return serde_json::to_string_pretty(&preview)
                .map_err(|e| AgentError::ToolError(e.to_string()));
        }

        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_SEND_WRITE")
            .ok()
            .as_deref()
            == Some("1");
        if !allow_write_by_env {
            let approval = ExecApprovalRequestPayload {
                command: format!("send {} {}", channel, to),
                cwd: None,
                host: Some("channels".to_string()),
                security: Some("channel-send".to_string()),
                ask: Some(format!(
                    "Allow sending an outbound message via {} to {}?",
                    channel, to
                )),
                agent_id: Some("default".to_string()),
                resolved_path: None,
                session_key: None,
            };
            crate::openclaw_exec_approvals::ensure_tool_write_allowed(
                &self.state,
                "send",
                approval,
                timeout_ms,
            )
            .await
            .map_err(|e| AgentError::ToolError(format!("{}: {}", e.code, e.message)))?;
        }

        self.state
            .channel_manager()
            .send(&channel, &to, OutgoingMessage::text(message))
            .await
            .map_err(|e| AgentError::ToolError(format!("send failed: {}", e)))?;

        serde_json::to_string_pretty(&json!({ "ok": true }))
            .map_err(|e| AgentError::ToolError(e.to_string()))
    }
}

pub struct PollTool {
    state: GatewayState,
}

impl PollTool {
    pub fn new(state: GatewayState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentTool for PollTool {
    fn name(&self) -> &str {
        "poll"
    }

    fn description(&self) -> &str {
        "Send a poll (text fallback) via a configured drbot channel (approval-gated)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "channel": { "type": "string", "description": "Channel type (optional if a default channel is configured)." },
                "to": { "type": "string", "description": "Recipient id. You may also use '<channel>:<to>' when channel is omitted." },
                "question": { "type": "string", "description": "Poll question." },
                "options": { "type": "array", "items": { "type": "string" }, "description": "Poll options (>=2)." },
                "approvalTimeoutMs": { "type": "number", "description": "Approval timeout in ms (default 120000)." },
                "dryRun": { "type": "boolean", "description": "If true, return a request preview without sending." }
            },
            "required": ["to", "question", "options"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let mut channel = args
            .get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let mut to = args
            .get("to")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let question = args
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let options = args
            .get("options")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let dry_run = args.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);
        let timeout_ms = args
            .get("approvalTimeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120_000)
            .max(1);

        if to.is_empty() || question.is_empty() || options.len() < 2 {
            return Err(AgentError::ToolError(
                "to, question, and options (>=2) required".to_string(),
            ));
        }

        if channel.is_empty() {
            if let Some((left, right)) = to.split_once(':') {
                if self.state.channel_manager().has_channel(left) {
                    channel = left.trim().to_string();
                    to = right.trim().to_string();
                }
            }
        }
        if channel.is_empty() {
            if let Some(default) = self.state.channel_manager().default_channel() {
                channel = default.to_string();
            }
        }
        if channel.is_empty() {
            return Err(AgentError::ToolError(
                "channel required (or configure a default channel)".to_string(),
            ));
        }

        let mut text = String::new();
        text.push_str(question.trim());
        text.push_str("\n\n");
        for (idx, opt) in options.iter().enumerate() {
            let label = opt.as_str().unwrap_or("").trim();
            if label.is_empty() {
                continue;
            }
            text.push_str(&format!("{}. {}\n", idx + 1, label));
        }
        text.push_str("\nReply with the number of your choice.");

        if dry_run {
            let preview = json!({
                "ok": true,
                "dryRun": true,
                "channel": channel,
                "to": to,
                "message": text,
            });
            return serde_json::to_string_pretty(&preview)
                .map_err(|e| AgentError::ToolError(e.to_string()));
        }

        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_SEND_WRITE")
            .ok()
            .as_deref()
            == Some("1");
        if !allow_write_by_env {
            let approval = ExecApprovalRequestPayload {
                command: format!("poll {} {}", channel, to),
                cwd: None,
                host: Some("channels".to_string()),
                security: Some("channel-send".to_string()),
                ask: Some(format!(
                    "Allow sending an outbound poll via {} to {}?",
                    channel, to
                )),
                agent_id: Some("default".to_string()),
                resolved_path: None,
                session_key: None,
            };
            crate::openclaw_exec_approvals::ensure_tool_write_allowed(
                &self.state,
                "poll",
                approval,
                timeout_ms,
            )
            .await
            .map_err(|e| AgentError::ToolError(format!("{}: {}", e.code, e.message)))?;
        }

        self.state
            .channel_manager()
            .send(&channel, &to, OutgoingMessage::text(text))
            .await
            .map_err(|e| AgentError::ToolError(format!("poll failed: {}", e)))?;

        serde_json::to_string_pretty(&json!({ "ok": true }))
            .map_err(|e| AgentError::ToolError(e.to_string()))
    }
}
