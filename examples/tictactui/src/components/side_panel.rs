//! Room side-panel components.

use crate::session::RoomSession;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};

pub fn render(frame: &mut Frame<'_>, session: &RoomSession, area: Rect) {
    if area.height < 18 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(7), Constraint::Min(4)])
            .split(area);

        render_room(frame, session, chunks[0]);
        render_peers(frame, session, chunks[1]);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(6),
            Constraint::Length(7),
        ])
        .split(area);

    render_room(frame, session, chunks[0]);
    render_peers(frame, session, chunks[1]);
    render_ticket(frame, session, chunks[2]);
}

pub fn render_room(frame: &mut Frame<'_>, session: &RoomSession, area: Rect) {
    let host = session
        .snapshot
        .host_id
        .map_or("unknown".to_string(), |id| short_id(&id));
    let info = vec![
        Line::from(format!("You: {}", short_id(&session.snapshot.local_id))),
        Line::from(format!("Role: {}", session.local_role())),
        Line::from(format!("Host: {host}")),
        Line::from(format!(
            "Host offline: {}",
            session.snapshot.host_disconnected
        )),
        Line::from(format!("State: {:?}", session.snapshot.app_state)),
    ];
    frame.render_widget(
        Paragraph::new(info).block(Block::default().title("Room").borders(Borders::ALL)),
        area,
    );
}

pub fn render_peers(frame: &mut Frame<'_>, session: &RoomSession, area: Rect) {
    let mut peers: Vec<_> = session.snapshot.peers.values().collect();
    peers.sort_by_key(|peer| peer.id);
    let peer_items = peers.into_iter().map(|peer| {
        let host_marker = if Some(peer.id) == session.snapshot.host_id {
            " host"
        } else {
            ""
        };
        let ready = if peer.ready { "ready" } else { "not ready" };
        let status = format!("{:?}", peer.status).to_lowercase();
        ListItem::new(format!(
            "{} - {status}, {ready}{host_marker}",
            peer.profile.nickname
        ))
    });
    frame.render_widget(
        List::new(peer_items).block(Block::default().title("Lobby").borders(Borders::ALL)),
        area,
    );
}

pub fn render_ticket(frame: &mut Frame<'_>, session: &RoomSession, area: Rect) {
    let ticket = session.ticket.as_deref().unwrap_or("Joined with ticket");
    frame.render_widget(
        Paragraph::new(ticket)
            .wrap(Wrap { trim: false })
            .block(Block::default().title("Ticket").borders(Borders::ALL)),
        area,
    );
}

fn short_id(id: &iroh::EndpointId) -> String {
    let mut value = id.to_string();
    value.truncate(8);
    value
}
