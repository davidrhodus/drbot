//! OpenClaw usage helpers (best-effort).
//!
//! OpenClaw's Control UI expects `usage.status` and `usage.cost` to work even when
//! the gateway cannot query upstream provider quota APIs (drbot typically uses API
//! keys, not OpenClaw's web/OAuth tokens). To support the dashboards, we persist
//! per-run token usage records and aggregate them for the last N days.

use crate::state::GatewayState;
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Local, NaiveDate, TimeZone};
use drbot_core::message::Message;
use drbot_providers::Usage;
use drbot_providers::{ChatOptions, ChatResponse, ModelInfo, Provider, StreamEvent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_stream::Stream;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageRecord {
    pub ts_ms: u64,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub input: u64,
    pub output: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageCostConfigFile {
    version: u32,
    #[serde(default)]
    providers: HashMap<String, HashMap<String, ModelCost>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelCost {
    input: f64,
    output: f64,
    #[serde(default)]
    cache_read: f64,
    #[serde(default)]
    cache_write: f64,
}

impl ModelCost {
    fn estimate_usd(self, usage: &UsageRecord) -> Option<f64> {
        let total = (usage.input as f64) * self.input
            + (usage.output as f64) * self.output
            + 0.0 * self.cache_read
            + 0.0 * self.cache_write;
        if total.is_finite() {
            Some(total / 1_000_000.0)
        } else {
            None
        }
    }
}

fn builtin_model_cost(model: &str) -> Option<ModelCost> {
    let normalized = model.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    // Heuristics / fallbacks based on OpenClaw's static catalogs. These should be treated as
    // best-effort estimates; users can override costs precisely via `openclaw_costs.json`.
    match normalized.as_str() {
        // GPT-5.* family (OpenClaw / opencode Zen fallbacks)
        "gpt-5.2" | "gpt-5.2-codex" => Some(ModelCost {
            input: 1.75,
            output: 14.0,
            cache_read: 0.175,
            cache_write: 0.0,
        }),
        "gpt-5.1" | "gpt-5.1-codex" => Some(ModelCost {
            input: 1.07,
            output: 8.5,
            cache_read: 0.107,
            cache_write: 0.0,
        }),
        "gpt-5.1-codex-mini" => Some(ModelCost {
            input: 0.25,
            output: 2.0,
            cache_read: 0.025,
            cache_write: 0.0,
        }),
        "gpt-5.1-codex-max" => Some(ModelCost {
            input: 1.25,
            output: 10.0,
            cache_read: 0.125,
            cache_write: 0.0,
        }),

        // Claude Opus 4.5 (OpenClaw / opencode Zen fallbacks)
        "claude-opus-4-5" => Some(ModelCost {
            input: 5.0,
            output: 25.0,
            cache_read: 0.5,
            cache_write: 6.25,
        }),

        // Legacy models (best-effort; pricing may vary by provider).
        "gpt-4o" => Some(ModelCost {
            input: 5.0,
            output: 15.0,
            cache_read: 0.0,
            cache_write: 0.0,
        }),
        "gpt-4o-mini" => Some(ModelCost {
            input: 0.15,
            output: 0.6,
            cache_read: 0.0,
            cache_write: 0.0,
        }),
        "gpt-4-turbo" => Some(ModelCost {
            input: 10.0,
            output: 30.0,
            cache_read: 0.0,
            cache_write: 0.0,
        }),
        "gpt-3.5-turbo" => Some(ModelCost {
            input: 0.5,
            output: 1.5,
            cache_read: 0.0,
            cache_write: 0.0,
        }),
        "claude-3-5-sonnet-20241022" => Some(ModelCost {
            input: 3.0,
            output: 15.0,
            cache_read: 0.0,
            cache_write: 0.0,
        }),
        "claude-3-5-haiku-20241022" => Some(ModelCost {
            input: 1.0,
            output: 5.0,
            cache_read: 0.0,
            cache_write: 0.0,
        }),
        "claude-3-opus-20240229" => Some(ModelCost {
            input: 15.0,
            output: 75.0,
            cache_read: 0.0,
            cache_write: 0.0,
        }),
        _ => {
            // Prefix-based fallbacks for versioned model IDs.
            if normalized.starts_with("gpt-4o-") {
                return Some(ModelCost {
                    input: 5.0,
                    output: 15.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                });
            }
            if normalized.starts_with("gpt-4o-mini-") {
                return Some(ModelCost {
                    input: 0.15,
                    output: 0.6,
                    cache_read: 0.0,
                    cache_write: 0.0,
                });
            }
            if normalized.starts_with("gpt-4-turbo-") {
                return Some(ModelCost {
                    input: 10.0,
                    output: 30.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                });
            }
            if normalized.starts_with("gpt-3.5-turbo-") {
                return Some(ModelCost {
                    input: 0.5,
                    output: 1.5,
                    cache_read: 0.0,
                    cache_write: 0.0,
                });
            }
            if normalized.starts_with("claude-opus-4-5-") {
                return Some(ModelCost {
                    input: 5.0,
                    output: 25.0,
                    cache_read: 0.5,
                    cache_write: 6.25,
                });
            }
            if normalized.starts_with("claude-3-5-sonnet-") {
                return Some(ModelCost {
                    input: 3.0,
                    output: 15.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                });
            }
            if normalized.starts_with("claude-3-5-haiku-") {
                return Some(ModelCost {
                    input: 1.0,
                    output: 5.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                });
            }
            if normalized.starts_with("claude-3-opus-") {
                return Some(ModelCost {
                    input: 15.0,
                    output: 75.0,
                    cache_read: 0.0,
                    cache_write: 0.0,
                });
            }
            None
        }
    }
}

fn usage_records_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn resolve_usage_dir(state: &GatewayState) -> PathBuf {
    crate::openclaw_paths::resolve_openclaw_state_dir(state.config())
        .or_else(drbot_core::Config::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("usage")
}

fn resolve_usage_log_path(state: &GatewayState, date: NaiveDate) -> PathBuf {
    resolve_usage_dir(state).join(format!("{}.jsonl", date.format("%Y-%m-%d")))
}

fn resolve_usage_cost_config_path() -> PathBuf {
    if let Some(dir) = drbot_core::Config::config_dir() {
        return dir.join("openclaw_costs.json");
    }
    PathBuf::from("openclaw_costs.json")
}

fn load_usage_cost_config_best_effort() -> Option<UsageCostConfigFile> {
    let path = resolve_usage_cost_config_path();
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<UsageCostConfigFile>(&raw).ok()
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
}

fn percent_used(used: u64, budget: Option<u64>) -> u64 {
    let Some(budget) = budget else {
        return 0;
    };
    if budget == 0 {
        return 0;
    }
    let pct = ((used as f64) / (budget as f64)) * 100.0;
    pct.round().clamp(0.0, 10_000.0) as u64
}

fn next_local_midnight_ms(now_ms: u64) -> Option<u64> {
    let dt = Local.timestamp_millis_opt(now_ms as i64).single()?;
    let next = dt.date_naive().succ_opt()?.and_hms_opt(0, 0, 0)?;
    let next_dt = Local.from_local_datetime(&next).single()?;
    u64::try_from(next_dt.timestamp_millis()).ok()
}

pub(crate) async fn append_usage_record_best_effort(state: &GatewayState, record: UsageRecord) {
    let date = Local::now().date_naive();
    let path = resolve_usage_log_path(state, date);
    let dir = resolve_usage_dir(state);

    let _guard = usage_records_lock().lock().await;
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        tracing::warn!(error = %e, path = %dir.to_string_lossy(), "openclaw_usage: failed to create usage dir");
        return;
    }
    let line = match serde_json::to_string(&record) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "openclaw_usage: failed to serialize usage record");
            return;
        }
    };

    let mut file = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
    {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(error = %e, path = %path.to_string_lossy(), "openclaw_usage: failed to open usage log");
            return;
        }
    };
    if let Err(e) = file.write_all(line.as_bytes()).await {
        tracing::warn!(error = %e, path = %path.to_string_lossy(), "openclaw_usage: failed to write usage record");
        return;
    }
    if let Err(e) = file.write_all(b"\n").await {
        tracing::warn!(error = %e, path = %path.to_string_lossy(), "openclaw_usage: failed to write usage newline");
        return;
    }
    let _ = file.flush().await;
}

pub(crate) fn record_from_stream(
    state: &GatewayState,
    provider: &str,
    model: Option<String>,
    session_key: Option<String>,
    run_id: Option<String>,
    usage: &Usage,
) -> UsageRecord {
    let model = model.or_else(|| match provider.trim() {
        "anthropic" => state
            .config()
            .providers
            .anthropic
            .as_ref()
            .and_then(|cfg| cfg.default_model.clone())
            .or_else(|| state.config().providers.default_model.clone()),
        "openai" => state
            .config()
            .providers
            .openai
            .as_ref()
            .and_then(|cfg| cfg.default_model.clone())
            .or_else(|| state.config().providers.default_model.clone()),
        "ollama" => state
            .config()
            .providers
            .ollama
            .as_ref()
            .and_then(|cfg| cfg.default_model.clone())
            .or_else(|| state.config().providers.default_model.clone()),
        _ => state.config().providers.default_model.clone(),
    });
    UsageRecord {
        ts_ms: chrono::Utc::now()
            .timestamp_millis()
            .try_into()
            .unwrap_or(0),
        provider: provider.trim().to_string(),
        model,
        input: usage.input_tokens as u64,
        output: usage.output_tokens as u64,
        session_key,
        run_id,
    }
}

async fn read_usage_records_for_date(state: &GatewayState, date: NaiveDate) -> Vec<UsageRecord> {
    let path = resolve_usage_log_path(state, date);
    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut out = Vec::new();
    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<UsageRecord>(trimmed) {
            out.push(rec);
        }
    }
    out
}

pub(crate) async fn usage_cost_summary(state: &GatewayState, days: usize) -> serde_json::Value {
    let days = days.clamp(1, 365);
    let today = Local::now().date_naive();
    let start = today - ChronoDuration::days((days - 1) as i64);

    let cost_cfg = load_usage_cost_config_best_effort();

    let mut by_date: HashMap<String, (u64, u64, f64, u64)> = HashMap::new(); // input, output, cost, missing

    for i in 0..days {
        let date = start + ChronoDuration::days(i as i64);
        let key = date.format("%Y-%m-%d").to_string();
        by_date.insert(key, (0, 0, 0.0, 0));
    }

    for i in 0..days {
        let date = start + ChronoDuration::days(i as i64);
        let date_key = date.format("%Y-%m-%d").to_string();
        let records = read_usage_records_for_date(state, date).await;
        let entry = by_date.entry(date_key).or_insert((0, 0, 0.0, 0));
        for rec in records {
            entry.0 = entry.0.saturating_add(rec.input);
            entry.1 = entry.1.saturating_add(rec.output);

            let mut cost_added = false;
            if let (Some(cfg), Some(model)) = (cost_cfg.as_ref(), rec.model.as_deref()) {
                if let Some(provider_models) = cfg.providers.get(rec.provider.trim()) {
                    if let Some(cost) = provider_models.get(model.trim()) {
                        if let Some(usd) = cost.estimate_usd(&rec) {
                            entry.2 += usd;
                            cost_added = true;
                        }
                    }
                }
            }
            if !cost_added {
                if let Some(model) = rec.model.as_deref() {
                    if let Some(cost) = builtin_model_cost(model) {
                        if let Some(usd) = cost.estimate_usd(&rec) {
                            entry.2 += usd;
                            cost_added = true;
                        }
                    }
                }
            }
            if !cost_added {
                entry.3 = entry.3.saturating_add(1);
            }
        }
    }

    let mut daily: Vec<serde_json::Value> = Vec::new();
    let mut totals_input: u64 = 0;
    let mut totals_output: u64 = 0;
    let mut totals_cost: f64 = 0.0;
    let mut totals_missing: u64 = 0;

    for i in 0..days {
        let d = start + ChronoDuration::days(i as i64);
        let date = d.format("%Y-%m-%d").to_string();
        let (input, output, cost, missing) = by_date.get(&date).copied().unwrap_or((0, 0, 0.0, 0));
        totals_input = totals_input.saturating_add(input);
        totals_output = totals_output.saturating_add(output);
        totals_cost += cost;
        totals_missing = totals_missing.saturating_add(missing);

        daily.push(json!({
            "date": date,
            "input": input,
            "output": output,
            "cacheRead": 0,
            "cacheWrite": 0,
            "totalTokens": input + output,
            "totalCost": cost,
            "missingCostEntries": missing,
        }));
    }

    json!({
        "updatedAt": chrono::Utc::now().timestamp_millis(),
        "days": days,
        "daily": daily,
        "totals": {
            "input": totals_input,
            "output": totals_output,
            "cacheRead": 0,
            "cacheWrite": 0,
            "totalTokens": totals_input + totals_output,
            "totalCost": totals_cost,
            "missingCostEntries": totals_missing,
        },
        "costConfig": {
            "path": resolve_usage_cost_config_path().to_string_lossy(),
            "loaded": cost_cfg.as_ref().map(|c| c.version).is_some(),
        }
    })
}

pub(crate) async fn usage_status_summary(state: &GatewayState) -> serde_json::Value {
    // Aggregate records for the last 30 days to build a local "usage window".
    let days = 30usize;
    let today = Local::now().date_naive();
    let start = today - ChronoDuration::days((days - 1) as i64);

    let mut by_provider_day: HashMap<String, (u64, u64)> = HashMap::new();
    let mut by_provider_30d: HashMap<String, (u64, u64)> = HashMap::new();

    for i in 0..days {
        let date = start + ChronoDuration::days(i as i64);
        let records = read_usage_records_for_date(state, date).await;
        for rec in records {
            let provider = rec.provider.trim();
            if provider.is_empty() {
                continue;
            }
            let entry_30 = by_provider_30d
                .entry(provider.to_string())
                .or_insert((0, 0));
            entry_30.0 = entry_30.0.saturating_add(rec.input);
            entry_30.1 = entry_30.1.saturating_add(rec.output);

            if date == today {
                let entry_day = by_provider_day
                    .entry(provider.to_string())
                    .or_insert((0, 0));
                entry_day.0 = entry_day.0.saturating_add(rec.input);
                entry_day.1 = entry_day.1.saturating_add(rec.output);
            }
        }
    }

    let daily_budget = env_u64("DRBOT_OPENCLAW_USAGE_DAILY_TOKEN_BUDGET");
    let window_budget = env_u64("DRBOT_OPENCLAW_USAGE_30D_TOKEN_BUDGET");
    let reset_at = next_local_midnight_ms(
        chrono::Utc::now()
            .timestamp_millis()
            .try_into()
            .unwrap_or(0),
    );

    let mut providers: Vec<serde_json::Value> = Vec::new();
    let mut keys: Vec<String> = by_provider_30d.keys().cloned().collect();
    keys.sort();

    if keys.is_empty() {
        // Keep the Control UI happy even when no records exist yet.
        keys.push(
            state
                .provider()
                .map(|p| p.name().to_string())
                .unwrap_or_else(|| "provider".to_string()),
        );
    }

    for key in keys {
        let (day_in, day_out) = by_provider_day.get(&key).copied().unwrap_or((0, 0));
        let (win_in, win_out) = by_provider_30d.get(&key).copied().unwrap_or((0, 0));
        let day_total = day_in.saturating_add(day_out);
        let win_total = win_in.saturating_add(win_out);

        providers.push(json!({
            "provider": key,
            "displayName": key,
            "windows": [
                {
                    "label": "Day",
                    "usedPercent": percent_used(day_total, daily_budget),
                    "resetAt": reset_at,
                    "inputTokens": day_in,
                    "outputTokens": day_out,
                    "totalTokens": day_total,
                },
                {
                    "label": "30d",
                    "usedPercent": percent_used(win_total, window_budget),
                    "resetAt": null,
                    "inputTokens": win_in,
                    "outputTokens": win_out,
                    "totalTokens": win_total,
                }
            ]
        }));
    }

    json!({
        "updatedAt": chrono::Utc::now().timestamp_millis(),
        "providers": providers,
    })
}

pub(crate) struct UsageLoggingProvider {
    state: GatewayState,
    inner: Arc<dyn Provider>,
    session_key: Option<String>,
    run_id: Option<String>,
}

impl UsageLoggingProvider {
    pub(crate) fn new(
        state: GatewayState,
        inner: Arc<dyn Provider>,
        session_key: Option<String>,
        run_id: Option<String>,
    ) -> Self {
        Self {
            state,
            inner,
            session_key,
            run_id,
        }
    }
}

#[async_trait]
impl Provider for UsageLoggingProvider {
    async fn chat(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<ChatResponse> {
        let res = self.inner.chat(messages, options).await?;
        if let Some(usage) = res.usage.as_ref() {
            let record = record_from_stream(
                &self.state,
                self.inner.name(),
                Some(res.model.clone()),
                self.session_key.clone(),
                self.run_id.clone(),
                usage,
            );
            append_usage_record_best_effort(&self.state, record).await;
        }
        Ok(res)
    }

    async fn stream(
        &self,
        messages: &[Message],
        options: ChatOptions,
    ) -> drbot_core::Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>> {
        self.inner.stream(messages, options).await
    }

    fn models(&self) -> Vec<ModelInfo> {
        self.inner.models()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drbot_core::Config;
    use futures::stream;
    use uuid::Uuid;

    fn test_state() -> GatewayState {
        let mut cfg = Config::default();
        let base = std::env::temp_dir().join(format!("drbot-usage-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        cfg.storage.database_path = base.join("drbot.db");
        cfg.storage.media_path = base.join("media");
        GatewayState::new(cfg)
    }

    #[tokio::test]
    async fn usage_cost_summary_aggregates_records() {
        let state = test_state();
        let today = Local::now().date_naive();
        let path = resolve_usage_log_path(&state, today);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(
            &path,
            r#"{"tsMs":1,"provider":"openai","model":"x","input":10,"output":5}
{"tsMs":2,"provider":"openai","model":"x","input":3,"output":7}
"#,
        )
        .await
        .unwrap();

        let summary = usage_cost_summary(&state, 1).await;
        let totals = summary.get("totals").unwrap();
        assert_eq!(totals.get("input").and_then(|v| v.as_u64()), Some(13));
        assert_eq!(totals.get("output").and_then(|v| v.as_u64()), Some(12));
        assert_eq!(totals.get("totalTokens").and_then(|v| v.as_u64()), Some(25));
    }

    #[tokio::test]
    async fn append_usage_record_and_status_summary_work() {
        let state = test_state();
        let usage = Usage {
            input_tokens: 11,
            output_tokens: 7,
        };
        let record = record_from_stream(
            &state,
            "openai",
            Some("dummy-model".to_string()),
            Some("main".to_string()),
            Some("run-1".to_string()),
            &usage,
        );
        append_usage_record_best_effort(&state, record).await;

        let cost = usage_cost_summary(&state, 1).await;
        let totals = cost.get("totals").unwrap();
        assert_eq!(totals.get("input").and_then(|v| v.as_u64()), Some(11));
        assert_eq!(totals.get("output").and_then(|v| v.as_u64()), Some(7));

        let status = usage_status_summary(&state).await;
        let providers = status
            .get("providers")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(
            providers
                .iter()
                .any(|p| p.get("provider").and_then(|v| v.as_str()) == Some("openai")),
            "expected status summary to include openai provider"
        );
    }

    #[derive(Debug)]
    struct DummyProvider;

    #[async_trait]
    impl Provider for DummyProvider {
        async fn chat(
            &self,
            _messages: &[Message],
            options: ChatOptions,
        ) -> drbot_core::Result<ChatResponse> {
            Ok(ChatResponse {
                content: "ok".to_string(),
                model: options.model.unwrap_or_else(|| "dummy-model".to_string()),
                usage: Some(Usage {
                    input_tokens: 4,
                    output_tokens: 2,
                }),
                stop_reason: Some("stop".to_string()),
                tool_uses: Vec::new(),
            })
        }

        async fn stream(
            &self,
            _messages: &[Message],
            _options: ChatOptions,
        ) -> drbot_core::Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>> {
            Ok(Box::pin(stream::empty()))
        }

        fn models(&self) -> Vec<ModelInfo> {
            Vec::new()
        }

        fn name(&self) -> &str {
            "dummy"
        }
    }

    #[tokio::test]
    async fn usage_logging_provider_appends_chat_usage() {
        let state = test_state();
        let wrapped = UsageLoggingProvider::new(
            state.clone(),
            Arc::new(DummyProvider),
            Some("main".to_string()),
            Some("wrapped-1".to_string()),
        );

        let _ = wrapped
            .chat(
                &[Message::user("hi")],
                ChatOptions {
                    model: Some("dummy-model".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        let summary = usage_cost_summary(&state, 1).await;
        let totals = summary.get("totals").unwrap();
        assert_eq!(totals.get("input").and_then(|v| v.as_u64()), Some(4));
        assert_eq!(totals.get("output").and_then(|v| v.as_u64()), Some(2));
    }
}
