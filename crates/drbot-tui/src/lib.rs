//! Terminal UI for drbot.
//!
//! This crate provides a terminal-based chat interface using ratatui.

mod app;
mod gateway_client;
mod openclaw_client;
mod ui;

pub use app::{App, AppConfig, ProviderType};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use drbot_core::Result;
use ratatui::prelude::*;
use std::io;
use std::time::Duration;
use tracing::error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitAction {
    Quit,
    LaunchWizard,
}

/// Run the TUI application.
pub async fn run(config: AppConfig) -> Result<ExitAction> {
    // Setup terminal
    enable_raw_mode().map_err(|e| drbot_core::Error::Internal(e.to_string()))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .map_err(|e| drbot_core::Error::Internal(e.to_string()))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|e| drbot_core::Error::Internal(e.to_string()))?;

    // Create app
    let mut app = App::new(config).await?;

    // Run main loop
    let result = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode().map_err(|e| drbot_core::Error::Internal(e.to_string()))?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .map_err(|e| drbot_core::Error::Internal(e.to_string()))?;
    terminal
        .show_cursor()
        .map_err(|e| drbot_core::Error::Internal(e.to_string()))?;

    if let Err(e) = result {
        error!(error = %e, "TUI error");
        return Err(e);
    }

    Ok(app.exit_action)
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<ExitAction> {
    loop {
        // Draw UI
        terminal
            .draw(|f| ui::draw(f, app))
            .map_err(|e| drbot_core::Error::Internal(e.to_string()))?;

        // Handle events with timeout for async operations
        if event::poll(Duration::from_millis(50))
            .map_err(|e| drbot_core::Error::Internal(e.to_string()))?
        {
            if let Event::Key(key) =
                event::read().map_err(|e| drbot_core::Error::Internal(e.to_string()))?
            {
                // Handle quit
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(ExitAction::Quit);
                }

                // Handle input
                app.handle_key(key).await?;
            }
        }

        // Process any pending async operations
        app.tick().await?;

        // Check if we should quit
        if app.should_quit() {
            return Ok(app.exit_action);
        }
    }
}
