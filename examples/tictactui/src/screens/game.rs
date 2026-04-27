//! Game screen.

use crate::{
    components::{board, side_panel},
    session::RoomSession,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

pub fn render(frame: &mut Frame<'_>, session: &RoomSession) {
    let area = crate::ui::content_area(frame);
    if area.width < 92 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        board::render(frame, session, chunks[0]);
        side_panel::render(frame, session, chunks[1]);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(area);

    board::render(frame, session, chunks[0]);
    side_panel::render(frame, session, chunks[1]);
}
