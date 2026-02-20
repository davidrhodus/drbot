//! AppleScript integration for iMessage.

use std::process::Command;
use tracing::{debug, error};

/// Send a message via AppleScript.
pub fn send_message(recipient: &str, text: &str) -> drbot_core::Result<()> {
    let script = format!(
        r#"
        tell application "Messages"
            set targetService to 1st account whose service type = iMessage
            set targetBuddy to participant "{}" of targetService
            send "{}" to targetBuddy
        end tell
        "#,
        escape_applescript(recipient),
        escape_applescript(text)
    );

    run_osascript(&script)
}

/// Send a message to a specific chat by ID.
pub fn send_to_chat(chat_id: &str, text: &str) -> drbot_core::Result<()> {
    let script = format!(
        r#"
        tell application "Messages"
            set targetChat to chat id "{}"
            send "{}" to targetChat
        end tell
        "#,
        escape_applescript(chat_id),
        escape_applescript(text)
    );

    run_osascript(&script)
}

/// Get list of recent chats.
pub fn get_chats() -> drbot_core::Result<Vec<ChatInfo>> {
    let script = r#"
        tell application "Messages"
            set chatList to {}
            repeat with c in chats
                set chatId to id of c
                set chatName to name of c
                set end of chatList to chatId & "|||" & chatName
            end repeat
            return chatList
        end tell
    "#;

    let output = run_osascript_output(script)?;
    let mut chats = Vec::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.split("|||").collect();
        if parts.len() >= 2 {
            chats.push(ChatInfo {
                id: parts[0].trim().to_string(),
                name: parts[1].trim().to_string(),
            });
        }
    }

    Ok(chats)
}

/// Check if Messages.app is running.
pub fn is_messages_running() -> bool {
    let script = r#"
        tell application "System Events"
            return (name of processes) contains "Messages"
        end tell
    "#;

    match run_osascript_output(script) {
        Ok(output) => output.trim() == "true",
        Err(_) => false,
    }
}

/// Activate Messages.app.
#[allow(dead_code)] // Used by some workflows/channels; not currently referenced in this crate.
pub fn activate_messages() -> drbot_core::Result<()> {
    let script = r#"
        tell application "Messages"
            activate
        end tell
    "#;
    run_osascript(script)
}

/// Chat info from AppleScript.
#[derive(Debug, Clone)]
pub struct ChatInfo {
    pub id: String,
    pub name: String,
}

/// Escape a string for use in AppleScript.
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Run an AppleScript and discard output.
fn run_osascript(script: &str) -> drbot_core::Result<()> {
    debug!("Running AppleScript");

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| drbot_core::Error::Internal(format!("Failed to run osascript: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("AppleScript error: {}", stderr);
        return Err(drbot_core::Error::Internal(format!(
            "AppleScript failed: {}",
            stderr
        )));
    }

    Ok(())
}

/// Run an AppleScript and return output.
fn run_osascript_output(script: &str) -> drbot_core::Result<String> {
    debug!("Running AppleScript with output");

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| drbot_core::Error::Internal(format!("Failed to run osascript: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("AppleScript error: {}", stderr);
        return Err(drbot_core::Error::Internal(format!(
            "AppleScript failed: {}",
            stderr
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_applescript() {
        assert_eq!(escape_applescript("hello"), "hello");
        assert_eq!(escape_applescript("hello\"world"), "hello\\\"world");
        assert_eq!(escape_applescript("line1\nline2"), "line1\\nline2");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_is_messages_running() {
        // Just test that this doesn't panic
        let _ = is_messages_running();
    }
}
