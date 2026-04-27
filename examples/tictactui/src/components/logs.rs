//! Event log components.

use crate::session::RoomSession;
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, List, ListItem},
};

pub fn render_full(frame: &mut Frame<'_>, session: &RoomSession, area: Rect) {
    render(
        frame,
        session,
        crate::ui::content_area(frame).intersection(area),
        "Events",
    );
}

pub fn render_preview(frame: &mut Frame<'_>, session: &RoomSession, area: Rect) {
    render(frame, session, area, "Events");
}

fn render(frame: &mut Frame<'_>, session: &RoomSession, area: Rect, title: &'static str) {
    let items = session
        .notices
        .iter()
        .rev()
        .take(area.height.saturating_sub(2) as usize)
        .rev()
        .map(|line| ListItem::new(line.as_str()));
    frame.render_widget(
        List::new(items).block(Block::default().title(title).borders(Borders::ALL)),
        area,
    );
}
