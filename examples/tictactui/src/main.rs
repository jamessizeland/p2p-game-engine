//! Ratatui Tic Tac Toe showcase for the peer-to-peer game engine.

mod app;
mod game;
mod screens {
    pub mod game;
    pub mod home;
    pub mod lobby;
}
mod input;
mod session;
mod ui;
mod components {
    pub mod board;
    pub mod chat;
    pub mod logs;
    pub mod side_panel;
}

use anyhow::Result;
use app::App;
use input::InputEvent;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let data_dir = tempfile::tempdir()?;
    let data_path = data_dir.path().to_path_buf();

    let mut terminal = input::enter_terminal()?;
    let result = run(App::new(default_name(), data_path), &mut terminal).await;
    input::leave_terminal()?;
    result
}

async fn run(app: App, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
    let mut app = app;
    let mut input_rx = input::spawn_input_task();
    let mut tick = tokio::time::interval(Duration::from_millis(50));

    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;
        if app.should_quit {
            break;
        }

        tokio::select! {
            Some(input) = input_rx.recv() => {
                match input {
                    InputEvent::Key(key) => app.handle_key(key).await?,
                    InputEvent::Paste(text) => app.handle_paste(&text),
                }
            }
            _ = tick.tick() => {
                app.drain_room_events().await?;
            }
            else => break,
        }
    }
    Ok(())
}

fn default_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "player".to_string())
}
