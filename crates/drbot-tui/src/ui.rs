//! TUI rendering.

use crate::app::{App, MessageRole, OpenclawTab, Overlay};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

/// Draw the complete UI.
pub fn draw(f: &mut Frame, app: &App) {
    // Create main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(1),    // Messages
            Constraint::Length(3), // Input
            Constraint::Length(1), // Status
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_messages(f, app, chunks[1]);
    draw_input(f, app, chunks[2]);
    draw_status(f, app, chunks[3]);

    draw_overlay(f, app);
}

fn sanitize_for_tui(input: &str) -> String {
    let stripped = strip_ansi_sequences(input);
    let mut out = String::with_capacity(stripped.len());
    for ch in stripped.chars() {
        match ch {
            '\n' => out.push('\n'),
            '\r' => out.push('\n'),
            '\t' => out.push_str("    "),
            c if c.is_control() => {
                // Drop other control characters to avoid terminal corruption.
            }
            c => out.push(c),
        }
    }
    out
}

fn strip_ansi_sequences(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            out.push(ch);
            continue;
        }

        match chars.peek().copied() {
            Some('[') => {
                // CSI: ESC [ ... <final byte>
                chars.next();
                while let Some(c) = chars.next() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                // OSC: ESC ] ... BEL or ESC \\
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            Some('P') | Some('X') | Some('^') | Some('_') => {
                // DCS/SOS/PM/APC: ESC <char> ... ESC \\
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            Some(_) => {
                // Single-character escape sequence.
                chars.next();
            }
            None => {}
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::{sanitize_for_tui, strip_ansi_sequences};

    #[test]
    fn strip_ansi_sequences_removes_csi_and_osc() {
        let input = "\u{1b}[31mred\u{1b}[0m \u{1b}]0;title\u{7}ok";
        let stripped = strip_ansi_sequences(input);
        assert_eq!(stripped, "red ok");
    }

    #[test]
    fn sanitize_for_tui_drops_controls_and_normalizes() {
        let input = "a\r\nb\tc\u{7}";
        let sanitized = sanitize_for_tui(input);
        assert_eq!(sanitized, "a\n\nb    c");
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            "drbot ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Terminal UI", Style::default().fg(Color::White)),
    ])];

    if let Some(url) = app.config.gateway_url.as_deref() {
        let status_label = if app.config.gateway_running {
            "running"
        } else {
            "not running"
        };
        let status_style = if app.config.gateway_running {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let (provider_label, provider_style) = match (
            app.active_provider.as_deref(),
            app.active_provider_status.as_deref(),
        ) {
            (Some(name), Some(status)) if status.contains("unreachable") => (
                format!("{} (unreachable)", name),
                Style::default().fg(Color::Red),
            ),
            (Some(name), Some(_status)) => (name.to_string(), Style::default().fg(Color::Green)),
            (Some(name), None) => (name.to_string(), Style::default().fg(Color::White)),
            (None, _) => (
                "(no provider)".to_string(),
                Style::default().fg(Color::DarkGray),
            ),
        };
        let model = app.model.as_deref().unwrap_or("(default)");
        let session = app
            .session_id
            .map(|id| {
                let s = id.to_string();
                let short = s.get(0..8).unwrap_or(&s);
                format!("{}...", short)
            })
            .unwrap_or_else(|| "(none)".to_string());

        let (tools_label, tools_style) = if app.tool_cfg.enabled {
            ("on", Style::default().fg(Color::Green))
        } else {
            ("off", Style::default().fg(Color::DarkGray))
        };
        let (approve_label, approve_style) = if app.tool_cfg.auto_approve {
            ("auto", Style::default().fg(Color::Green))
        } else {
            ("ask", Style::default().fg(Color::DarkGray))
        };

        lines.push(Line::from(vec![
            Span::styled("Gateway: ", Style::default().fg(Color::DarkGray)),
            Span::styled(url, Style::default().fg(Color::White)),
            Span::raw(" "),
            Span::styled(format!("({})", status_label), status_style),
            Span::raw(" | "),
            Span::styled("Provider: ", Style::default().fg(Color::DarkGray)),
            Span::styled(provider_label, provider_style),
            Span::raw(" | "),
            Span::styled("Model: ", Style::default().fg(Color::DarkGray)),
            Span::styled(model, Style::default().fg(Color::White)),
            Span::raw(" | "),
            Span::styled("Session: ", Style::default().fg(Color::DarkGray)),
            Span::styled(session, Style::default().fg(Color::White)),
            Span::raw(" | "),
            Span::styled("Tools: ", Style::default().fg(Color::DarkGray)),
            Span::styled(tools_label, tools_style),
            Span::raw(" | "),
            Span::styled("Approve: ", Style::default().fg(Color::DarkGray)),
            Span::styled(approve_label, approve_style),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "Gateway: (unknown)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let header = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .alignment(Alignment::Center);

    f.render_widget(header, area);
}

fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;

    let mut lines: Vec<Line> = Vec::new();

    for msg in app.visible_messages(inner_height * 2) {
        let (prefix, style) = match msg.role {
            MessageRole::User => ("You: ", Style::default().fg(Color::Green)),
            MessageRole::Assistant => ("AI: ", Style::default().fg(Color::Cyan)),
            MessageRole::System => (
                "System: ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::ITALIC),
            ),
        };

        // Add prefix line
        lines.push(Line::from(Span::styled(
            prefix,
            style.add_modifier(Modifier::BOLD),
        )));

        // Add content lines (wrapped)
        let sanitized = sanitize_for_tui(&msg.content);
        for line in sanitized.lines() {
            lines.push(Line::from(Span::styled(format!("  {}", line), style)));
        }

        // Add empty line between messages
        lines.push(Line::from(""));
    }

    // Add streaming content if any
    if app.is_loading && !app.streaming_content.is_empty() {
        lines.push(Line::from(Span::styled(
            "AI: ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        let sanitized = sanitize_for_tui(&app.streaming_content);
        for line in sanitized.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", line),
                Style::default().fg(Color::Cyan),
            )));
        }
    }

    let messages = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(" Chat ", Style::default().fg(Color::White))),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(messages, area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let input_style = if app.is_loading {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let input = Paragraph::new(app.input.as_str()).style(input_style).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(
                if app.is_loading {
                    " Waiting... "
                } else {
                    " Message "
                },
                Style::default().fg(Color::White),
            )),
    );

    f.render_widget(input, area);

    // Show cursor
    if !app.is_loading && app.overlay.is_none() {
        f.set_cursor_position((area.x + 1 + app.cursor_pos as u16, area.y + 1));
    }
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let status_style = if app.is_loading {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let shortcuts = " Ctrl+P: Provider | Ctrl+M: Model | Ctrl+O: Sessions | Ctrl+D: OpenClaw | Ctrl+K: Skills | Ctrl+T: Tools | Ctrl+Y: AutoApprove | Esc: Stop | Ctrl+C: Quit | Enter: Send | /help | /wizard ";

    let status_text = sanitize_for_tui(&app.status).replace('\n', " ");
    let status = Paragraph::new(Line::from(vec![
        Span::styled(status_text, status_style),
        Span::raw(" | "),
        Span::styled(shortcuts, Style::default().fg(Color::DarkGray)),
    ]));

    f.render_widget(status, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    let vertical = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1]);

    vertical[1]
}

fn overlay_window_slice(total: usize, selected: usize, max_items: usize) -> (usize, usize) {
    if total <= max_items {
        return (0, total);
    }
    let mut start = 0usize;
    if selected >= max_items {
        start = selected.saturating_sub(max_items / 2);
    }
    if start + max_items > total {
        start = total - max_items;
    }
    (start, (start + max_items).min(total))
}

fn draw_overlay(f: &mut Frame, app: &App) {
    let Some(overlay) = app.overlay.as_ref() else {
        return;
    };

    match overlay {
        Overlay::ProviderPicker(picker) => {
            let area = centered_rect(70, 70, f.area());
            f.render_widget(Clear, area);

            let title = " Provider ";
            let hint = "Up/Down: move   Enter: select   Esc: close";

            let max_items = area.height.saturating_sub(4) as usize;
            let (start, end) =
                overlay_window_slice(picker.providers.len(), picker.selected, max_items.max(1));

            let mut lines: Vec<Line> = Vec::new();
            for (i, prov) in picker.providers.iter().enumerate().take(end).skip(start) {
                let selected = i == picker.selected;
                let style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                lines.push(Line::from(Span::styled(
                    format!(
                        "{:>2}. {} ({})  models: {}",
                        i + 1,
                        prov.name,
                        prov.status,
                        prov.models.len()
                    ),
                    style,
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            )));

            let widget = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::White))
                        .title(Span::styled(title, Style::default().fg(Color::White))),
                )
                .wrap(Wrap { trim: true });

            f.render_widget(widget, area);
        }
        Overlay::ModelPicker(picker) => {
            let area = centered_rect(75, 75, f.area());
            f.render_widget(Clear, area);

            let title = " Model ";
            let hint = "Up/Down: move   Enter: select   Esc: close";

            let max_items = area.height.saturating_sub(4) as usize;
            let (start, end) =
                overlay_window_slice(picker.models.len(), picker.selected, max_items.max(1));

            let mut lines: Vec<Line> = Vec::new();
            for (i, model) in picker.models.iter().enumerate().take(end).skip(start) {
                let selected = i == picker.selected;
                let style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let label = if model.id == "(default)" {
                    model.name.clone()
                } else {
                    format!("{}  {}", model.id, model.name)
                };

                lines.push(Line::from(Span::styled(
                    format!("{:>2}. {}", i + 1, label),
                    style,
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            )));

            let widget = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::White))
                        .title(Span::styled(title, Style::default().fg(Color::White))),
                )
                .wrap(Wrap { trim: true });

            f.render_widget(widget, area);
        }
        Overlay::SessionPicker(picker) => {
            let area = centered_rect(85, 75, f.area());
            f.render_widget(Clear, area);

            let title = " Sessions ";
            let hint = "Up/Down: move   Enter: open   Esc: close";

            let max_items = area.height.saturating_sub(4) as usize;
            let (start, end) =
                overlay_window_slice(picker.sessions.len(), picker.selected, max_items.max(1));

            let mut lines: Vec<Line> = Vec::new();
            for (i, s) in picker.sessions.iter().enumerate().take(end).skip(start) {
                let selected = i == picker.selected;
                let style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let id = s.id.to_string();
                let short = id.get(0..8).unwrap_or(&id);
                let title = s.title.clone().unwrap_or_else(|| "(untitled)".to_string());
                let provider = s
                    .provider
                    .clone()
                    .unwrap_or_else(|| "(default)".to_string());
                let model = s.model.clone().unwrap_or_else(|| "(default)".to_string());

                lines.push(Line::from(Span::styled(
                    format!(
                        "{:>2}. {}  {}  msgs:{}  provider:{}  model:{}  {}",
                        i + 1,
                        short,
                        s.state,
                        s.message_count,
                        provider,
                        model,
                        title
                    ),
                    style,
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            )));

            let widget = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::White))
                        .title(Span::styled(title, Style::default().fg(Color::White))),
                )
                .wrap(Wrap { trim: true });

            f.render_widget(widget, area);
        }
        Overlay::ToolApproval(approval) => {
            let area = centered_rect(85, 65, f.area());
            f.render_widget(Clear, area);

            let title = " Approve Tool ";
            let hint = if approval.call.tool == "bash" {
                "Y/Enter: approve   T: trust command   A: approve+auto-approve   N/Esc: deny"
            } else {
                "Y/Enter: approve   A: approve+auto-approve   N/Esc: deny"
            };

            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(Span::styled(
                format!("Tool: {}", approval.call.tool),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));
            if !approval.reason.trim().is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("Reason: {}", approval.reason.trim()),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            lines.push(Line::from(""));

            match approval.call.tool.as_str() {
                "bash" => {
                    let command = approval
                        .call
                        .args
                        .get("command")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let cwd = approval
                        .call
                        .args
                        .get("cwd")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if !cwd.trim().is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!("cwd: {}", cwd.trim()),
                            Style::default().fg(Color::White),
                        )));
                    }
                    lines.push(Line::from(Span::styled(
                        format!("command: {}", command),
                        Style::default().fg(Color::White),
                    )));
                }
                "read_file" => {
                    let path = approval
                        .call
                        .args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    lines.push(Line::from(Span::styled(
                        format!("path: {}", path),
                        Style::default().fg(Color::White),
                    )));
                }
                "write_file" => {
                    let path = approval
                        .call
                        .args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let bytes = approval
                        .call
                        .args
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(|s| s.len())
                        .unwrap_or(0);
                    lines.push(Line::from(Span::styled(
                        format!("path: {}", path),
                        Style::default().fg(Color::White),
                    )));
                    lines.push(Line::from(Span::styled(
                        format!("bytes: {}", bytes),
                        Style::default().fg(Color::White),
                    )));
                }
                "list_dir" | "list_directory" => {
                    let path = approval
                        .call
                        .args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or(".");
                    lines.push(Line::from(Span::styled(
                        format!("path: {}", path),
                        Style::default().fg(Color::White),
                    )));
                }
                "search" => {
                    let pattern = approval
                        .call
                        .args
                        .get("pattern")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let path = approval
                        .call
                        .args
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or(".");
                    lines.push(Line::from(Span::styled(
                        format!("pattern: {}", pattern),
                        Style::default().fg(Color::White),
                    )));
                    lines.push(Line::from(Span::styled(
                        format!("path: {}", path),
                        Style::default().fg(Color::White),
                    )));
                }
                "apply_patch" => {
                    let bytes = approval
                        .call
                        .args
                        .get("patch")
                        .and_then(|v| v.as_str())
                        .map(|s| s.len())
                        .unwrap_or(0);
                    lines.push(Line::from(Span::styled(
                        format!("patch bytes: {}", bytes),
                        Style::default().fg(Color::White),
                    )));
                }
                _ => {
                    if let Ok(pretty) = serde_json::to_string_pretty(&approval.call.args) {
                        for line in pretty.lines().take(12) {
                            lines.push(Line::from(Span::styled(
                                line.to_string(),
                                Style::default().fg(Color::White),
                            )));
                        }
                    }
                }
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            )));

            let widget = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::White))
                        .title(Span::styled(title, Style::default().fg(Color::White))),
                )
                .wrap(Wrap { trim: false });

            f.render_widget(widget, area);
        }
        Overlay::Skills(skills) => {
            let area = centered_rect(92, 85, f.area());
            f.render_widget(Clear, area);

            let title = " Skills ";
            let hint = "Up/Down: move   Enter: toggle   E: enable   D: disable   R: refresh   A: add   Esc: close";

            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(Span::styled(
                format!("Workspace: {}", skills.workspace_dir),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(Span::styled(
                format!("Managed: {}", skills.managed_dir),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(Span::styled(
                format!("Snapshot: {}", skills.snapshot_version),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));

            let max_items = area.height.saturating_sub(8) as usize;
            let (start, end) =
                overlay_window_slice(skills.skills.len(), skills.selected, max_items.max(1));

            for (i, entry) in skills.skills.iter().enumerate().take(end).skip(start) {
                let selected = i == skills.selected;
                let style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let desc_style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                let mut flags: Vec<String> = Vec::new();
                if entry.disabled {
                    flags.push("disabled".to_string());
                } else {
                    flags.push("enabled".to_string());
                }
                if entry.blocked_by_allowlist {
                    flags.push("blocked".to_string());
                }
                if entry.eligible {
                    flags.push("eligible".to_string());
                } else {
                    flags.push("ineligible".to_string());
                }
                if !entry.missing.bins.is_empty()
                    || !entry.missing.any_bins.is_empty()
                    || !entry.missing.env.is_empty()
                    || !entry.missing.config.is_empty()
                    || !entry.missing.os.is_empty()
                {
                    let mut missing = Vec::new();
                    if !entry.missing.bins.is_empty() || !entry.missing.any_bins.is_empty() {
                        missing.push("bins");
                    }
                    if !entry.missing.env.is_empty() {
                        missing.push("env");
                    }
                    if !entry.missing.config.is_empty() {
                        missing.push("config");
                    }
                    if !entry.missing.os.is_empty() {
                        missing.push("os");
                    }
                    flags.push(format!("missing:{}", missing.join(",")));
                }

                lines.push(Line::from(Span::styled(
                    format!(
                        "{:>2}. {} ({}) [{}]",
                        i + 1,
                        entry.name,
                        entry.skill_key,
                        flags.join(", ")
                    ),
                    style,
                )));
                if !entry.description.trim().is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("    {}", entry.description.trim()),
                        desc_style,
                    )));
                }
            }

            lines.push(Line::from(""));
            if let Some(entry) = skills.skills.get(skills.selected) {
                lines.push(Line::from(Span::styled(
                    format!("Selected: {} ({})", entry.name, entry.skill_key),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )));
                let source = entry.source.trim();
                if !source.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("Source: {}", source),
                        Style::default().fg(Color::DarkGray),
                    )));
                }

                let mut req_parts = Vec::new();
                if !entry.requirements.bins.is_empty() {
                    req_parts.push(format!("bins({})", entry.requirements.bins.len()));
                }
                if !entry.requirements.any_bins.is_empty() {
                    req_parts.push(format!(
                        "any_bins({})",
                        entry.requirements.any_bins.len()
                    ));
                }
                if !entry.requirements.env.is_empty() {
                    req_parts.push(format!("env({})", entry.requirements.env.len()));
                }
                if !entry.requirements.config.is_empty() {
                    req_parts.push(format!("config({})", entry.requirements.config.len()));
                }
                if !entry.requirements.os.is_empty() {
                    req_parts.push(format!("os({})", entry.requirements.os.len()));
                }
                if !req_parts.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("Requires: {}", req_parts.join(", ")),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "No skill selected.",
                    Style::default().fg(Color::DarkGray),
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            )));

            let widget = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::White))
                        .title(Span::styled(title, Style::default().fg(Color::White))),
                )
                .wrap(Wrap { trim: true });

            f.render_widget(widget, area);
        }
        Overlay::SkillsAdd(add) => {
            let area = centered_rect(70, 40, f.area());
            f.render_widget(Clear, area);

            let title = " Add Skill ";
            let hint = "Enter: save   Esc: close";

            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(Span::styled(
                "Format: <skillKey> <url>",
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));

            let mut display = add.input.clone();
            let cursor = add.cursor_pos.min(display.len());
            display.insert(cursor, '|');
            lines.push(Line::from(Span::styled(
                format!("> {}", display),
                Style::default().fg(Color::White),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            )));

            let widget = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::White))
                        .title(Span::styled(title, Style::default().fg(Color::White))),
                )
                .wrap(Wrap { trim: true });

            f.render_widget(widget, area);
        }
        Overlay::FirstRun(fr) => {
            let area = centered_rect(92, 80, f.area());
            f.render_widget(Clear, area);

            let title = " Setup ";
            let hint = "W: run wizard   P/Enter: provider picker   R: refresh   Esc: close";

            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(Span::styled(
                "No usable providers detected.",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Cost-savers (recommended):",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "  1) Install `claude` or `codex` CLI (gateway can use them without an API key).",
                Style::default().fg(Color::White),
            )));
            lines.push(Line::from(Span::styled(
                "  2) Or run Ollama locally and configure its URL/model.",
                Style::default().fg(Color::White),
            )));
            lines.push(Line::from(Span::styled(
                "  3) Or configure API keys for Anthropic/OpenAI.",
                Style::default().fg(Color::White),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Detected providers:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));

            let max_items = area.height.saturating_sub(12) as usize;
            let (start, end) = overlay_window_slice(fr.providers.len(), 0, max_items.max(1));
            for prov in fr.providers.iter().take(end).skip(start) {
                let style = if prov.status.starts_with("unavailable") {
                    Style::default().fg(Color::DarkGray)
                } else if prov.status.contains("unreachable") {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::White)
                };
                lines.push(Line::from(Span::styled(
                    format!("- {}  {}", prov.name, prov.status),
                    style,
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            )));

            let widget = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::White))
                        .title(Span::styled(title, Style::default().fg(Color::White))),
                )
                .wrap(Wrap { trim: true });

            f.render_widget(widget, area);
        }
        Overlay::Openclaw(oc) => {
            let area = centered_rect(96, 90, f.area());
            f.render_widget(Clear, area);

            let title = " OpenClaw ";
            let hint =
                "1/2/3 or O/L/E: tabs  Left/Right: tabs  Up/Down: scroll  R: refresh  Esc: close";

            let mut lines: Vec<Line> = Vec::new();

            let header = match app.openclaw_hello.as_ref() {
                Some(hello) => format!(
                    "Connected: server v{}  protocol:{}  conn:{}",
                    hello.server.version, hello.protocol, hello.server.conn_id
                ),
                None => "Not connected".to_string(),
            };
            lines.push(Line::from(Span::styled(
                header,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));

            let tabs = [
                ("Overview", OpenclawTab::Overview),
                ("Logs", OpenclawTab::Logs),
                ("Events", OpenclawTab::Events),
            ];
            let mut tab_spans: Vec<Span> = Vec::new();
            for (i, (label, tab)) in tabs.iter().enumerate() {
                if i > 0 {
                    tab_spans.push(Span::raw("  "));
                }
                let selected = *tab == oc.tab;
                let style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                tab_spans.push(Span::styled(format!("[{}] {}", i + 1, label), style));
            }
            lines.push(Line::from(tab_spans));
            lines.push(Line::from(""));

            let max_body = area.height.saturating_sub(6) as usize;
            match oc.tab {
                OpenclawTab::Overview => {
                    if let Some(hello) = app.openclaw_hello.as_ref() {
                        lines.push(Line::from(Span::styled(
                            format!(
                                "Uptime: {}ms  Presence: {}",
                                hello.snapshot.uptime_ms,
                                hello.snapshot.presence.len()
                            ),
                            Style::default().fg(Color::DarkGray),
                        )));
                        if let Some(path) = hello.snapshot.config_path.as_deref() {
                            lines.push(Line::from(Span::styled(
                                format!("Config: {}", path),
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                        if let Some(dir) = hello.snapshot.state_dir.as_deref() {
                            lines.push(Line::from(Span::styled(
                                format!("State: {}", dir),
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                        lines.push(Line::from(""));
                    }

                    if let Some(health) = app.openclaw_health.as_ref() {
                        let pretty = serde_json::to_string_pretty(health)
                            .unwrap_or_else(|_| health.to_string());
                        let health_lines: Vec<&str> = pretty.lines().collect();
                        let total = health_lines.len();
                        let scroll = oc.scroll.min(total);
                        let end = total.saturating_sub(scroll);
                        let start = end.saturating_sub(max_body.max(1));
                        for line in health_lines.iter().take(end).skip(start) {
                            lines.push(Line::from(Span::styled(
                                line.to_string(),
                                Style::default().fg(Color::White),
                            )));
                        }
                        if start > 0 {
                            lines.insert(
                                3,
                                Line::from(Span::styled(
                                    "(scroll up for more)",
                                    Style::default().fg(Color::DarkGray),
                                )),
                            );
                        } else if total > max_body {
                            lines.push(Line::from(Span::styled(
                                "(scroll up for more)",
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                    } else {
                        lines.push(Line::from(Span::styled(
                            "No health snapshot yet. Press R to fetch.",
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
                OpenclawTab::Logs => {
                    let total = app.openclaw_logs.len();
                    let scroll = oc.scroll.min(total);
                    let end = total.saturating_sub(scroll);
                    let start = end.saturating_sub(max_body.max(1));
                    if total == 0 {
                        lines.push(Line::from(Span::styled(
                            "No logs yet. Press R to fetch.",
                            Style::default().fg(Color::DarkGray),
                        )));
                    } else {
                        for line in app.openclaw_logs.iter().take(end).skip(start) {
                            let sanitized = sanitize_for_tui(line).replace('\n', " ");
                            lines.push(Line::from(Span::styled(
                                sanitized,
                                Style::default().fg(Color::White),
                            )));
                        }
                    }
                }
                OpenclawTab::Events => {
                    let all: Vec<&String> = app.openclaw_events.iter().collect();
                    let total = all.len();
                    let scroll = oc.scroll.min(total);
                    let end = total.saturating_sub(scroll);
                    let start = end.saturating_sub(max_body.max(1));
                    if total == 0 {
                        lines.push(Line::from(Span::styled(
                            "No events yet. (Tick events are hidden.)",
                            Style::default().fg(Color::DarkGray),
                        )));
                    } else {
                        for line in all.iter().take(end).skip(start) {
                            let sanitized = sanitize_for_tui(line).replace('\n', " ");
                            lines.push(Line::from(Span::styled(
                                sanitized,
                                Style::default().fg(Color::White),
                            )));
                        }
                    }
                }
            }

            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            )));

            let widget = Paragraph::new(Text::from(lines))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::White))
                        .title(Span::styled(title, Style::default().fg(Color::White))),
                )
                .wrap(Wrap { trim: false });

            f.render_widget(widget, area);
        }
    }
}
