//! Ratatui rendering for the Tic Tac Toe showcase.

use crate::{
    app::{App, RoomScreen},
    screens::{game, home, lobby},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

/// Render the full application frame.
pub fn render(frame: &mut Frame<'_>, app: &App) {
    if let Some(session) = &app.session {
        match app.room_screen {
            RoomScreen::Lobby => lobby::render(frame, session),
            RoomScreen::Game => game::render(frame, session),
            RoomScreen::Chat => crate::components::chat::render(frame, session, frame.area()),
            RoomScreen::Logs => crate::components::logs::render_full(frame, session, frame.area()),
        }
        render_footer(frame, app.room_screen);
        if let Some(popup) = &app.chat_popup {
            render_chat_popup(frame, &popup.message);
        }
    } else {
        home::render(frame, &app.home);
    }
}

fn render_footer(frame: &mut Frame<'_>, screen: RoomScreen) {
    let area = frame.area();
    let footer_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(3),
        width: area.width,
        height: 3.min(area.height),
    };
    let current = match screen {
        RoomScreen::Lobby => "Lobby",
        RoomScreen::Game => "Game",
        RoomScreen::Chat => "Chat",
        RoomScreen::Logs => "Logs",
    };
    let text = if area.width < 86 {
        format!("{current} | Alt+L/G/T/E screens | Alt+C copy | Esc leave")
    } else {
        format!(
            "{current} | arrows move | Alt+L/G/T/E screens | Alt+R/U ready | Alt+S start | Alt+F forfeit | Alt+C copy ticket | Alt+Q leave"
        )
    };
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().title("Controls").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        footer_area,
    );
}

fn render_chat_popup(frame: &mut Frame<'_>, message: &str) {
    let area = frame.area();
    let width = area.width.saturating_sub(4).min(64);
    let height = 5.min(area.height);
    let popup_area = Rect {
        x: area.x + area.width.saturating_sub(width + 2),
        y: area.y + 1,
        width,
        height,
    };
    let text = vec![
        Line::from(Span::styled(
            "New chat",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(message.to_string()),
    ];
    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL)),
        popup_area,
    );
}

pub(crate) fn content_area(frame: &Frame<'_>) -> Rect {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());
    chunks[0]
}
