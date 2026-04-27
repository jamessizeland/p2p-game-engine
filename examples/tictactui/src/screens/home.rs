//! Home screen rendering.

use crate::app::{HomeAction, HomeFocus, HomeState};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub fn render(frame: &mut Frame<'_>, home: &HomeState) {
    let area = centered(frame.area(), 72, 19);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Length(3),
            Constraint::Length(4),
        ])
        .split(area);

    frame.render_widget(
        Paragraph::new("Tic Tac Toe P2P")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL)),
        rows[0],
    );
    field(
        frame,
        rows[1],
        "Name",
        &home.username,
        home.focus == HomeFocus::Name,
    );
    field(
        frame,
        rows[2],
        "Join ticket",
        &home.join_ticket,
        home.focus == HomeFocus::Ticket,
    );
    actions(frame, rows[3], home);
    let status = vec![
        Line::from(home.notice.as_str()),
        Line::from("Alt+V/F9 loads the last copied ticket"),
    ];
    frame.render_widget(
        Paragraph::new(status)
            .wrap(Wrap { trim: true })
            .block(Block::default().title("Status").borders(Borders::ALL)),
        rows[4],
    );
}

fn field(frame: &mut Frame<'_>, area: Rect, title: &str, value: &str, focused: bool) {
    let style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    frame.render_widget(
        Paragraph::new(value)
            .style(style)
            .block(Block::default().title(title).borders(Borders::ALL)),
        area,
    );
}

fn actions(frame: &mut Frame<'_>, area: Rect, home: &HomeState) {
    let labels = [
        (HomeAction::Host, "Host"),
        (HomeAction::Join, "Join"),
        (HomeAction::Resume, "Resume"),
    ];
    let line = Line::from(
        labels
            .into_iter()
            .flat_map(|(action, label)| {
                let selected = home.focus == HomeFocus::Action && home.selected_action == action;
                let style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                [Span::raw("  "), Span::styled(format!(" {label} "), style)]
            })
            .collect::<Vec<_>>(),
    );
    frame.render_widget(
        Paragraph::new(line)
            .alignment(Alignment::Center)
            .block(Block::default().title("Action").borders(Borders::ALL)),
        area,
    );
}

fn centered(area: Rect, max_width: u16, height: u16) -> Rect {
    let width = area.width.min(max_width);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height: area.height.min(height),
    }
}
