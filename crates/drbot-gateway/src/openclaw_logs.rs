//! OpenClaw `logs.tail` support.
//!
//! OpenClaw's gateway exposes `logs.tail` to stream server logs to the Control UI.
//! drbot doesn't currently persist tracing logs to a rolling file by default, so we
//! keep a small in-memory, file-like buffer and implement cursor/maxBytes semantics
//! compatible with OpenClaw's protocol.

use tokio::sync::Mutex;

const MAX_LOG_BYTES: usize = 5_000_000;

fn now_stamp() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[derive(Debug, Clone)]
pub struct LogsTailResult {
    pub file: String,
    pub cursor: u64,
    pub size: u64,
    pub lines: Vec<String>,
    pub truncated: bool,
    pub reset: bool,
}

#[derive(Debug, Default)]
struct LogState {
    base_cursor: u64,
    bytes: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct OpenclawLogBuffer {
    inner: Mutex<LogState>,
}

impl OpenclawLogBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn push_line(&self, line: &str) {
        let line = line.trim_end_matches(|c| c == '\n' || c == '\r');
        if line.trim().is_empty() {
            return;
        }

        // Prefix with timestamp to keep logs readable when tailing.
        let stamped = format!("[{}] {}\n", now_stamp(), line);
        let mut st = self.inner.lock().await;
        st.bytes.extend_from_slice(stamped.as_bytes());

        if st.bytes.len() <= MAX_LOG_BYTES {
            return;
        }

        // Drop older bytes, then align to next newline so callers don't start mid-line.
        let overflow = st.bytes.len().saturating_sub(MAX_LOG_BYTES);
        let mut drop_len = overflow;
        if drop_len > 0 {
            if let Some(pos) = st
                .bytes
                .get(drop_len..)
                .and_then(|rest| rest.iter().position(|b| *b == b'\n'))
            {
                drop_len = drop_len.saturating_add(pos + 1);
            }
        }
        if drop_len > 0 {
            st.bytes.drain(0..drop_len);
            st.base_cursor = st.base_cursor.saturating_add(drop_len as u64);
        }
    }

    pub async fn tail(
        &self,
        cursor: Option<u64>,
        limit: usize,
        max_bytes: usize,
    ) -> LogsTailResult {
        let max_bytes = max_bytes.clamp(1, 1_000_000) as u64;
        let limit = limit.clamp(1, 5000);

        let st = self.inner.lock().await;
        let size = st.base_cursor.saturating_add(st.bytes.len() as u64);

        let mut reset = false;
        let mut truncated = false;
        let mut start = size.saturating_sub(max_bytes);

        if let Some(cursor) = cursor {
            if cursor > size {
                reset = true;
                truncated = start > 0;
            } else {
                start = cursor;
                if size.saturating_sub(start) > max_bytes {
                    reset = true;
                    truncated = true;
                    start = size.saturating_sub(max_bytes);
                }
            }
        } else {
            truncated = start > 0;
        }

        if start < st.base_cursor {
            reset = true;
            truncated = true;
            start = st.base_cursor;
        }

        if size == 0 || size <= start {
            return LogsTailResult {
                file: "drbot".to_string(),
                cursor: size,
                size,
                lines: Vec::new(),
                truncated,
                reset,
            };
        }

        let offset = start.saturating_sub(st.base_cursor) as usize;
        let slice = &st.bytes[offset..];

        let mut lines = String::from_utf8_lossy(slice)
            .split('\n')
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        // If we started mid-line, drop the first partial line (OpenClaw parity).
        if start > st.base_cursor {
            let prev_idx = offset.saturating_sub(1);
            if st.bytes.get(prev_idx) != Some(&b'\n') && !lines.is_empty() {
                lines.remove(0);
            }
        }

        if lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }

        if lines.len() > limit {
            lines = lines.split_off(lines.len() - limit);
        }

        LogsTailResult {
            file: "drbot".to_string(),
            cursor: size,
            size,
            lines,
            truncated,
            reset,
        }
    }
}
