//! Terminal input bridge.

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use std::time::Duration;
use tokio::sync::mpsc;

/// Terminal input events consumed by the application shell.
pub enum InputEvent {
    Key(KeyEvent),
    Paste(String),
}

/// Spawn a blocking keyboard reader and forward key events into Tokio.
pub fn spawn_input_task() -> mpsc::Receiver<InputEvent> {
    let (sender, receiver) = mpsc::channel(64);
    std::thread::spawn(move || {
        loop {
            let Ok(has_event) = event::poll(Duration::from_millis(100)) else {
                break;
            };
            if !has_event {
                continue;
            }
            let input = match event::read() {
                Ok(Event::Key(key)) => InputEvent::Key(key),
                Ok(Event::Paste(text)) => InputEvent::Paste(text),
                _ => {
                    continue;
                }
            };
            if sender.blocking_send(input).is_err() {
                break;
            }
        }
    });
    receiver
}

/// Configure the terminal for ratatui rendering.
pub fn enter_terminal() -> Result<ratatui::DefaultTerminal> {
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableBracketedPaste
    )?;
    Ok(ratatui::init())
}

/// Restore terminal state after the app exits.
pub fn leave_terminal() -> Result<()> {
    ratatui::restore();
    crossterm::execute!(
        std::io::stdout(),
        crossterm::event::DisableBracketedPaste,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    crossterm::terminal::disable_raw_mode()?;
    Ok(())
}
