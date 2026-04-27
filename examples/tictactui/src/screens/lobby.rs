//! Lobby screen.

use crate::{
    components::{logs, side_panel},
    session::RoomSession,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Paragraph},
};

pub fn render(frame: &mut Frame<'_>, session: &RoomSession) {
    let area = crate::ui::content_area(frame);
    if area.width < 92 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(7),
                Constraint::Length(6),
            ])
            .split(area);

        frame.render_widget(
            Paragraph::new(session.status_line())
                .block(Block::default().title("Lobby").borders(Borders::ALL)),
            rows[0],
        );
        side_panel::render_peers(frame, session, rows[1]);
        side_panel::render_room(frame, session, rows[2]);
        logs::render_preview(frame, session, rows[3]);
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(area);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(7),
            Constraint::Length(7),
        ])
        .split(columns[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(7)])
        .split(columns[1]);

    frame.render_widget(
        Paragraph::new(session.status_line())
            .block(Block::default().title("Lobby").borders(Borders::ALL)),
        left[0],
    );
    side_panel::render_peers(frame, session, left[1]);
    side_panel::render_ticket(frame, session, left[2]);
    side_panel::render_room(frame, session, right[0]);
    logs::render_preview(frame, session, right[1]);
}
