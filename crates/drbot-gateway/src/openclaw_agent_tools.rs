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

// ---------------------------------------------------------------------------
// Moltbook typed convenience tools
// ---------------------------------------------------------------------------

pub struct MoltbookPostTool {
    state: GatewayState,
}

impl MoltbookPostTool {
    pub fn new(state: GatewayState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentTool for MoltbookPostTool {
    fn name(&self) -> &str {
        "moltbook.post"
    }

    fn description(&self) -> &str {
        "Create a post on Moltbook (approval-gated)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "Post title." },
                "content": { "type": "string", "description": "Post body (markdown)." },
                "submolt": { "type": "string", "description": "Target submolt name." },
                "dryRun": { "type": "boolean", "description": "If true, return a request preview without sending." }
            },
            "required": ["title", "content", "submolt"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let submolt = args.get("submolt").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let dry_run = args.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);

        if title.is_empty() || content.is_empty() || submolt.is_empty() {
            return Err(AgentError::ToolError("title, content, and submolt are required".to_string()));
        }

        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_MOLTBOOK_WRITE").ok().as_deref() == Some("1");
        let mut allow_write = allow_write_by_env;
        if !dry_run && !allow_write_by_env {
            let approval = ExecApprovalRequestPayload {
                command: format!("moltbook.post in s/{}", submolt),
                cwd: None,
                host: Some("moltbook".to_string()),
                security: Some("integration-http-write".to_string()),
                ask: Some(format!("Allow creating a Moltbook post in s/{}?", submolt)),
                agent_id: Some("default".to_string()),
                resolved_path: Some("https://www.moltbook.com/api/v1/posts".to_string()),
                session_key: None,
            };
            crate::openclaw_exec_approvals::ensure_tool_write_allowed(&self.state, "moltbook.post", approval, 120_000)
                .await
                .map_err(|e| AgentError::ToolError(format!("{}: {}", e.code, e.message)))?;
            allow_write = true;
        }

        match crate::moltbook::moltbook_create_post(&title, &content, &submolt, dry_run, allow_write).await {
            Ok(payload) => serde_json::to_string_pretty(&payload).map_err(|e| AgentError::ToolError(e.to_string())),
            Err(err) => {
                let mut msg = format!("{}: {}", err.code, err.message);
                if let Some(details) = err.details {
                    if let Ok(pretty) = serde_json::to_string_pretty(&details) { msg.push('\n'); msg.push_str(&pretty); }
                }
                Err(AgentError::ToolError(msg))
            }
        }
    }
}

pub struct MoltbookFeedTool;

#[async_trait]
impl AgentTool for MoltbookFeedTool {
    fn name(&self) -> &str {
        "moltbook.feed"
    }

    fn description(&self) -> &str {
        "Read the Moltbook feed (read-only)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "sort": { "type": "string", "description": "Sort order: hot, new, or top. Default: hot." },
                "limit": { "type": "number", "description": "Number of posts (1-50, default 25)." },
                "submolt": { "type": "string", "description": "Filter by submolt name (optional)." }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let sort = args.get("sort").and_then(|v| v.as_str()).unwrap_or("hot").trim().to_string();
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(25).max(1).min(50);
        let submolt = args.get("submolt").and_then(|v| v.as_str()).map(|s| s.trim().to_string());

        match crate::moltbook::moltbook_get_feed(&sort, limit, submolt.as_deref()).await {
            Ok(payload) => serde_json::to_string_pretty(&payload).map_err(|e| AgentError::ToolError(e.to_string())),
            Err(err) => {
                let mut msg = format!("{}: {}", err.code, err.message);
                if let Some(details) = err.details {
                    if let Ok(pretty) = serde_json::to_string_pretty(&details) { msg.push('\n'); msg.push_str(&pretty); }
                }
                Err(AgentError::ToolError(msg))
            }
        }
    }
}

pub struct MoltbookCommentTool {
    state: GatewayState,
}

impl MoltbookCommentTool {
    pub fn new(state: GatewayState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentTool for MoltbookCommentTool {
    fn name(&self) -> &str {
        "moltbook.comment"
    }

    fn description(&self) -> &str {
        "Comment on a Moltbook post (approval-gated)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "postId": { "type": "string", "description": "ID of the post to comment on." },
                "content": { "type": "string", "description": "Comment body (markdown)." },
                "parentId": { "type": "string", "description": "Parent comment ID for threaded replies (optional)." },
                "dryRun": { "type": "boolean", "description": "If true, return a request preview without sending." }
            },
            "required": ["postId", "content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let post_id = args.get("postId").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let parent_id = args.get("parentId").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
        let dry_run = args.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);

        if post_id.is_empty() || content.is_empty() {
            return Err(AgentError::ToolError("postId and content are required".to_string()));
        }

        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_MOLTBOOK_WRITE").ok().as_deref() == Some("1");
        let mut allow_write = allow_write_by_env;
        if !dry_run && !allow_write_by_env {
            let approval = ExecApprovalRequestPayload {
                command: format!("moltbook.comment on post {}", post_id),
                cwd: None,
                host: Some("moltbook".to_string()),
                security: Some("integration-http-write".to_string()),
                ask: Some(format!("Allow commenting on Moltbook post {}?", post_id)),
                agent_id: Some("default".to_string()),
                resolved_path: Some(format!("https://www.moltbook.com/api/v1/posts/{}/comments", post_id)),
                session_key: None,
            };
            crate::openclaw_exec_approvals::ensure_tool_write_allowed(&self.state, "moltbook.comment", approval, 120_000)
                .await
                .map_err(|e| AgentError::ToolError(format!("{}: {}", e.code, e.message)))?;
            allow_write = true;
        }

        match crate::moltbook::moltbook_create_comment(&post_id, &content, parent_id.as_deref(), dry_run, allow_write).await {
            Ok(payload) => serde_json::to_string_pretty(&payload).map_err(|e| AgentError::ToolError(e.to_string())),
            Err(err) => {
                let mut msg = format!("{}: {}", err.code, err.message);
                if let Some(details) = err.details {
                    if let Ok(pretty) = serde_json::to_string_pretty(&details) { msg.push('\n'); msg.push_str(&pretty); }
                }
                Err(AgentError::ToolError(msg))
            }
        }
    }
}

pub struct MoltbookVoteTool {
    state: GatewayState,
}

impl MoltbookVoteTool {
    pub fn new(state: GatewayState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentTool for MoltbookVoteTool {
    fn name(&self) -> &str {
        "moltbook.vote"
    }

    fn description(&self) -> &str {
        "Upvote or downvote a Moltbook post (approval-gated)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "postId": { "type": "string", "description": "ID of the post to vote on." },
                "direction": { "type": "string", "description": "Vote direction: up or down. Default: up." },
                "dryRun": { "type": "boolean", "description": "If true, return a request preview without sending." }
            },
            "required": ["postId"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let post_id = args.get("postId").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let direction = args.get("direction").and_then(|v| v.as_str()).unwrap_or("up").trim().to_string();
        let dry_run = args.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);

        if post_id.is_empty() {
            return Err(AgentError::ToolError("postId is required".to_string()));
        }

        let suffix = if direction == "down" { "downvote" } else { "upvote" };
        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_MOLTBOOK_WRITE").ok().as_deref() == Some("1");
        let mut allow_write = allow_write_by_env;
        if !dry_run && !allow_write_by_env {
            let approval = ExecApprovalRequestPayload {
                command: format!("moltbook.vote {} post {}", suffix, post_id),
                cwd: None,
                host: Some("moltbook".to_string()),
                security: Some("integration-http-write".to_string()),
                ask: Some(format!("Allow {} Moltbook post {}?", suffix, post_id)),
                agent_id: Some("default".to_string()),
                resolved_path: Some(format!("https://www.moltbook.com/api/v1/posts/{}/{}", post_id, suffix)),
                session_key: None,
            };
            crate::openclaw_exec_approvals::ensure_tool_write_allowed(&self.state, "moltbook.vote", approval, 120_000)
                .await
                .map_err(|e| AgentError::ToolError(format!("{}: {}", e.code, e.message)))?;
            allow_write = true;
        }

        match crate::moltbook::moltbook_vote(&post_id, &direction, dry_run, allow_write).await {
            Ok(payload) => serde_json::to_string_pretty(&payload).map_err(|e| AgentError::ToolError(e.to_string())),
            Err(err) => {
                let mut msg = format!("{}: {}", err.code, err.message);
                if let Some(details) = err.details {
                    if let Ok(pretty) = serde_json::to_string_pretty(&details) { msg.push('\n'); msg.push_str(&pretty); }
                }
                Err(AgentError::ToolError(msg))
            }
        }
    }
}

pub struct MoltbookIdentityTool {
    state: GatewayState,
}

impl MoltbookIdentityTool {
    pub fn new(state: GatewayState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentTool for MoltbookIdentityTool {
    fn name(&self) -> &str {
        "moltbook.identity"
    }

    fn description(&self) -> &str {
        "Get agent profile, status, or generate an identity token."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "description": "Action: profile, status, or token. Default: profile. The token action is write (approval-gated)." },
                "dryRun": { "type": "boolean", "description": "If true (token action only), return a request preview without sending." }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("profile").trim().to_string();
        let dry_run = args.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);

        let is_write = action == "token";
        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_MOLTBOOK_WRITE").ok().as_deref() == Some("1");
        let mut allow_write = allow_write_by_env;
        if is_write && !dry_run && !allow_write_by_env {
            let approval = ExecApprovalRequestPayload {
                command: "moltbook.identity token".to_string(),
                cwd: None,
                host: Some("moltbook".to_string()),
                security: Some("integration-http-write".to_string()),
                ask: Some("Allow generating a Moltbook identity token?".to_string()),
                agent_id: Some("default".to_string()),
                resolved_path: Some("https://www.moltbook.com/api/v1/agents/me/identity-token".to_string()),
                session_key: None,
            };
            crate::openclaw_exec_approvals::ensure_tool_write_allowed(&self.state, "moltbook.identity", approval, 120_000)
                .await
                .map_err(|e| AgentError::ToolError(format!("{}: {}", e.code, e.message)))?;
            allow_write = true;
        }

        match crate::moltbook::moltbook_get_identity(&action, dry_run, allow_write).await {
            Ok(payload) => serde_json::to_string_pretty(&payload).map_err(|e| AgentError::ToolError(e.to_string())),
            Err(err) => {
                let mut msg = format!("{}: {}", err.code, err.message);
                if let Some(details) = err.details {
                    if let Ok(pretty) = serde_json::to_string_pretty(&details) { msg.push('\n'); msg.push_str(&pretty); }
                }
                Err(AgentError::ToolError(msg))
            }
        }
    }
}

pub struct MoltbookSearchTool;

#[async_trait]
impl AgentTool for MoltbookSearchTool {
    fn name(&self) -> &str {
        "moltbook.search"
    }

    fn description(&self) -> &str {
        "Search Moltbook posts, agents, and submolts (read-only)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query." },
                "limit": { "type": "number", "description": "Max results (default 25)." }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(25).max(1).min(50);

        if query.is_empty() {
            return Err(AgentError::ToolError("query is required".to_string()));
        }

        match crate::moltbook::moltbook_search(&query, limit).await {
            Ok(payload) => serde_json::to_string_pretty(&payload).map_err(|e| AgentError::ToolError(e.to_string())),
            Err(err) => {
                let mut msg = format!("{}: {}", err.code, err.message);
                if let Some(details) = err.details {
                    if let Ok(pretty) = serde_json::to_string_pretty(&details) { msg.push('\n'); msg.push_str(&pretty); }
                }
                Err(AgentError::ToolError(msg))
            }
        }
    }
}

pub struct MoltbookFollowTool {
    state: GatewayState,
}

impl MoltbookFollowTool {
    pub fn new(state: GatewayState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentTool for MoltbookFollowTool {
    fn name(&self) -> &str {
        "moltbook.follow"
    }

    fn description(&self) -> &str {
        "Follow or unfollow a Moltbook agent (approval-gated)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent": { "type": "string", "description": "Agent name to follow/unfollow." },
                "unfollow": { "type": "boolean", "description": "If true, unfollow instead of follow. Default: false." },
                "dryRun": { "type": "boolean", "description": "If true, return a request preview without sending." }
            },
            "required": ["agent"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let agent = args.get("agent").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let unfollow = args.get("unfollow").and_then(|v| v.as_bool()).unwrap_or(false);
        let dry_run = args.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);

        if agent.is_empty() {
            return Err(AgentError::ToolError("agent is required".to_string()));
        }

        let action_label = if unfollow { "unfollow" } else { "follow" };
        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_MOLTBOOK_WRITE").ok().as_deref() == Some("1");
        let mut allow_write = allow_write_by_env;
        if !dry_run && !allow_write_by_env {
            let approval = ExecApprovalRequestPayload {
                command: format!("moltbook.follow {} {}", action_label, agent),
                cwd: None,
                host: Some("moltbook".to_string()),
                security: Some("integration-http-write".to_string()),
                ask: Some(format!("Allow {} Moltbook agent {}?", action_label, agent)),
                agent_id: Some("default".to_string()),
                resolved_path: Some(format!("https://www.moltbook.com/api/v1/agents/{}/follow", agent)),
                session_key: None,
            };
            crate::openclaw_exec_approvals::ensure_tool_write_allowed(&self.state, "moltbook.follow", approval, 120_000)
                .await
                .map_err(|e| AgentError::ToolError(format!("{}: {}", e.code, e.message)))?;
            allow_write = true;
        }

        match crate::moltbook::moltbook_follow(&agent, unfollow, dry_run, allow_write).await {
            Ok(payload) => serde_json::to_string_pretty(&payload).map_err(|e| AgentError::ToolError(e.to_string())),
            Err(err) => {
                let mut msg = format!("{}: {}", err.code, err.message);
                if let Some(details) = err.details {
                    if let Ok(pretty) = serde_json::to_string_pretty(&details) { msg.push('\n'); msg.push_str(&pretty); }
                }
                Err(AgentError::ToolError(msg))
            }
        }
    }
}

pub struct MoltbookSubscribeTool {
    state: GatewayState,
}

impl MoltbookSubscribeTool {
    pub fn new(state: GatewayState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentTool for MoltbookSubscribeTool {
    fn name(&self) -> &str {
        "moltbook.subscribe"
    }

    fn description(&self) -> &str {
        "Subscribe or unsubscribe from a Moltbook submolt (approval-gated)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "submolt": { "type": "string", "description": "Submolt name to subscribe/unsubscribe." },
                "unsubscribe": { "type": "boolean", "description": "If true, unsubscribe instead of subscribe. Default: false." },
                "dryRun": { "type": "boolean", "description": "If true, return a request preview without sending." }
            },
            "required": ["submolt"]
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let submolt = args.get("submolt").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let unsubscribe = args.get("unsubscribe").and_then(|v| v.as_bool()).unwrap_or(false);
        let dry_run = args.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);

        if submolt.is_empty() {
            return Err(AgentError::ToolError("submolt is required".to_string()));
        }

        let action_label = if unsubscribe { "unsubscribe" } else { "subscribe" };
        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_MOLTBOOK_WRITE").ok().as_deref() == Some("1");
        let mut allow_write = allow_write_by_env;
        if !dry_run && !allow_write_by_env {
            let approval = ExecApprovalRequestPayload {
                command: format!("moltbook.subscribe {} s/{}", action_label, submolt),
                cwd: None,
                host: Some("moltbook".to_string()),
                security: Some("integration-http-write".to_string()),
                ask: Some(format!("Allow {} Moltbook submolt s/{}?", action_label, submolt)),
                agent_id: Some("default".to_string()),
                resolved_path: Some(format!("https://www.moltbook.com/api/v1/submolts/{}/subscribe", submolt)),
                session_key: None,
            };
            crate::openclaw_exec_approvals::ensure_tool_write_allowed(&self.state, "moltbook.subscribe", approval, 120_000)
                .await
                .map_err(|e| AgentError::ToolError(format!("{}: {}", e.code, e.message)))?;
            allow_write = true;
        }

        match crate::moltbook::moltbook_subscribe(&submolt, unsubscribe, dry_run, allow_write).await {
            Ok(payload) => serde_json::to_string_pretty(&payload).map_err(|e| AgentError::ToolError(e.to_string())),
            Err(err) => {
                let mut msg = format!("{}: {}", err.code, err.message);
                if let Some(details) = err.details {
                    if let Ok(pretty) = serde_json::to_string_pretty(&details) { msg.push('\n'); msg.push_str(&pretty); }
                }
                Err(AgentError::ToolError(msg))
            }
        }
    }
}

pub struct MoltbookDmTool {
    state: GatewayState,
}

impl MoltbookDmTool {
    pub fn new(state: GatewayState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentTool for MoltbookDmTool {
    fn name(&self) -> &str {
        "moltbook.dm"
    }

    fn description(&self) -> &str {
        "Check for or send Moltbook direct messages (sends are approval-gated)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "description": "Action: check or send. Default: check." },
                "to": { "type": "string", "description": "Recipient agent name (required for send)." },
                "message": { "type": "string", "description": "Message text (required for send)." },
                "dryRun": { "type": "boolean", "description": "If true (send action only), return a request preview without sending." }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("check").trim().to_string();
        let to = args.get("to").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
        let message = args.get("message").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
        let dry_run = args.get("dryRun").and_then(|v| v.as_bool()).unwrap_or(false);

        let is_write = action == "send";
        let allow_write_by_env = std::env::var("DRBOT_OPENCLAW_MOLTBOOK_WRITE").ok().as_deref() == Some("1");
        let mut allow_write = allow_write_by_env;
        if is_write && !dry_run && !allow_write_by_env {
            let to_label = to.as_deref().unwrap_or("?");
            let approval = ExecApprovalRequestPayload {
                command: format!("moltbook.dm send to {}", to_label),
                cwd: None,
                host: Some("moltbook".to_string()),
                security: Some("integration-http-write".to_string()),
                ask: Some(format!("Allow sending a Moltbook DM to {}?", to_label)),
                agent_id: Some("default".to_string()),
                resolved_path: Some("https://www.moltbook.com/api/v1/agents/dm/send".to_string()),
                session_key: None,
            };
            crate::openclaw_exec_approvals::ensure_tool_write_allowed(&self.state, "moltbook.dm", approval, 120_000)
                .await
                .map_err(|e| AgentError::ToolError(format!("{}: {}", e.code, e.message)))?;
            allow_write = true;
        }

        match crate::moltbook::moltbook_dm(&action, to.as_deref(), message.as_deref(), dry_run, allow_write).await {
            Ok(payload) => serde_json::to_string_pretty(&payload).map_err(|e| AgentError::ToolError(e.to_string())),
            Err(err) => {
                let mut msg = format!("{}: {}", err.code, err.message);
                if let Some(details) = err.details {
                    if let Ok(pretty) = serde_json::to_string_pretty(&details) { msg.push('\n'); msg.push_str(&pretty); }
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
