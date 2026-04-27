//! Chat screen component.

use crate::session::RoomSession;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub fn render(frame: &mut Frame<'_>, session: &RoomSession, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(3)])
        .split(crate::ui::content_area(frame).intersection(area));

    let items = session
        .chat_log
        .iter()
        .rev()
        .take(chunks[0].height.saturating_sub(2) as usize)
        .rev()
        .map(|line| ListItem::new(line.as_str()));
    frame.render_widget(
        List::new(items).block(Block::default().title("Chat").borders(Borders::ALL)),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(session.chat_input.as_str())
            .block(Block::default().title("Message").borders(Borders::ALL)),
        chunks[1],
    );
}
