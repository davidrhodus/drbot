//! TUI rendering.

use crate::app::{App, MessageRole};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
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

    draw_header(f, chunks[0]);
    draw_messages(f, app, chunks[1]);
    draw_input(f, app, chunks[2]);
    draw_status(f, app, chunks[3]);
}

fn draw_header(f: &mut Frame, area: Rect) {
    let header = Paragraph::new(vec![Line::from(vec![
        Span::styled(
            "drbot ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("Terminal UI", Style::default().fg(Color::White)),
    ])])
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
        for line in msg.content.lines() {
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
        for line in app.streaming_content.lines() {
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
    if !app.is_loading {
        f.set_cursor_position((area.x + 1 + app.cursor_pos as u16, area.y + 1));
    }
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let status_style = if app.is_loading {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let shortcuts = " ESC: Quit | Enter: Send | /help: Commands ";

    let status = Paragraph::new(Line::from(vec![
        Span::styled(&app.status, status_style),
        Span::raw(" | "),
        Span::styled(shortcuts, Style::default().fg(Color::DarkGray)),
    ]));

    f.render_widget(status, area);
}
