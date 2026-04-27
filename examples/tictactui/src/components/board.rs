//! Tic Tac Toe board component.

use crate::{game::Cell, session::RoomSession};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub fn render(frame: &mut Frame<'_>, session: &RoomSession, area: Rect) {
    let block = Block::default()
        .title(format!("Board - {}", session.status_line()))
        .borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(inner);

    for row in 0..3 {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(34),
                Constraint::Percentage(33),
            ])
            .split(rows[row]);
        for col in 0..3 {
            let idx = row * 3 + col;
            let selected = idx == session.selected_cell;
            let cell = match session.cell(idx) {
                Cell::Empty => (idx + 1).to_string(),
                Cell::Occupied(role) => role.to_string(),
            };
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            let text = Paragraph::new(Line::from(Span::styled(cell, style)))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            frame.render_widget(Clear, cols[col]);
            frame.render_widget(text, cols[col]);
        }
    }
}
