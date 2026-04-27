//! Active room session state for the Tic Tac Toe TUI.

use crate::{
    app::RoomScreen,
    game::{Cell, GameStatus, TicTacToeAction, TicTacToeLogic, TicTacToeState},
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use p2p_game_engine::{AppState, GameRoom, HostEvent, LeaveReason, RoomSnapshot, UiEvent};
use tokio::sync::mpsc;

/// Runtime state for a joined or hosted room.
pub struct RoomSession {
    pub room: GameRoom<TicTacToeLogic>,
    pub events: mpsc::Receiver<UiEvent<TicTacToeLogic>>,
    pub snapshot: RoomSnapshot<TicTacToeLogic>,
    pub ticket: Option<String>,
    pub selected_cell: usize,
    pub chat_input: String,
    pub chat_log: Vec<String>,
    pub notices: Vec<String>,
    pub should_leave: bool,
}

impl RoomSession {
    /// Create a new session after the room has been entered.
    pub async fn new(
        room: GameRoom<TicTacToeLogic>,
        events: mpsc::Receiver<UiEvent<TicTacToeLogic>>,
        ticket: Option<String>,
    ) -> Result<Self> {
        let snapshot = room.snapshot().await?;
        Ok(Self {
            room,
            events,
            snapshot,
            ticket,
            selected_cell: 0,
            chat_input: String::new(),
            chat_log: Vec::new(),
            notices: vec!["Entered room".to_string()],
            should_leave: false,
        })
    }

    /// Refresh the aggregate state used by the renderer.
    pub async fn refresh(&mut self) -> Result<()> {
        self.snapshot = self.room.snapshot().await?;
        Ok(())
    }

    /// Apply all pending room events without blocking the app shell.
    pub async fn drain_events(&mut self) -> Result<Vec<String>> {
        let mut chat_messages = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            if let Some(message) = self.handle_room_event(event).await? {
                chat_messages.push(message);
            }
        }
        Ok(chat_messages)
    }

    /// Handle an input key while focused on the active room.
    pub async fn handle_key(&mut self, key: KeyEvent, screen: RoomScreen) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_leave = true;
            return Ok(());
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('q') => self.should_leave = true,
                _ => {}
            }
            return Ok(());
        }

        match (key.modifiers.contains(KeyModifiers::ALT), key.code) {
            (true, KeyCode::Char('r')) | (_, KeyCode::F(5)) => self.set_ready(true).await?,
            (true, KeyCode::Char('u')) | (_, KeyCode::F(6)) => self.set_ready(false).await?,
            (true, KeyCode::Char('s')) | (_, KeyCode::F(7)) => self.start_game().await?,
            (true, KeyCode::Char('f')) | (_, KeyCode::F(8)) => self.forfeit().await?,
            (true, KeyCode::Char('q')) | (_, KeyCode::F(10)) => self.should_leave = true,
            _ => {}
        }

        match (screen, key.code) {
            (_, KeyCode::Esc) => self.should_leave = true,
            (RoomScreen::Game, KeyCode::Left) => self.move_selection(-1, 0),
            (RoomScreen::Game, KeyCode::Right) => self.move_selection(1, 0),
            (RoomScreen::Game, KeyCode::Up) => self.move_selection(0, -1),
            (RoomScreen::Game, KeyCode::Down) => self.move_selection(0, 1),
            (RoomScreen::Game, KeyCode::Enter) => self.submit_move().await?,
            (RoomScreen::Game, KeyCode::Char(c)) => {
                if let Some(idx) = c.to_digit(10).and_then(|n| n.checked_sub(1)) {
                    if idx < 9 {
                        self.selected_cell = idx as usize;
                    }
                }
            }
            (RoomScreen::Chat, KeyCode::Enter) => self.submit_chat().await?,
            (RoomScreen::Chat, KeyCode::Backspace) => {
                self.chat_input.pop();
            }
            (RoomScreen::Chat, KeyCode::Char(c))
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.chat_input.push(c);
            }
            _ => {}
        }
        Ok(())
    }

    /// Latest game state, if the game has started.
    pub fn game_state(&self) -> Option<&TicTacToeState> {
        self.snapshot.game_state.as_ref()
    }

    /// Cell contents for rendering an empty lobby board.
    pub fn cell(&self, idx: usize) -> Cell {
        self.game_state()
            .map_or(Cell::Empty, |state| state.board[idx])
    }

    /// Local peer's role, if known.
    pub fn local_role(&self) -> String {
        self.game_state()
            .and_then(|state| state.roles.get(&self.snapshot.local_id))
            .map_or_else(|| "Observer".to_string(), ToString::to_string)
    }

    /// Human-readable status line.
    pub fn status_line(&self) -> String {
        match self.game_state().map(|state| &state.status) {
            Some(GameStatus::Ongoing) => self
                .game_state()
                .map_or("Waiting for game state".to_string(), |state| {
                    format!("Turn: {}", state.current_turn)
                }),
            Some(GameStatus::Win(role)) => format!("{role} wins"),
            Some(GameStatus::Draw) => "Draw".to_string(),
            None => match self.snapshot.app_state {
                AppState::Lobby => "Waiting in lobby".to_string(),
                AppState::Paused => "Paused; host offline".to_string(),
                AppState::InGame => "Waiting for game state".to_string(),
                AppState::Finished => "Finished".to_string(),
            },
        }
    }

    async fn handle_room_event(
        &mut self,
        event: UiEvent<TicTacToeLogic>,
    ) -> Result<Option<String>> {
        let mut chat_message = None;
        match event {
            UiEvent::Peer(_) => self.notice("Lobby updated"),
            UiEvent::GameState(_) => self.notice("Game state updated"),
            UiEvent::AppState(state) => self.notice(format!("Room is now {state:?}")),
            UiEvent::Chat { sender, msg } => {
                let sender = if msg.is_from(&self.snapshot.local_id) {
                    "You".to_string()
                } else {
                    sender
                };
                let line = format!("{sender}: {}", msg.message);
                self.chat_log.push(line.clone());
                chat_message = Some(line);
            }
            UiEvent::ActionResult(result) => {
                if !result.accepted {
                    self.notice(format!(
                        "Action rejected: {}",
                        result.error.unwrap_or_else(|| "unknown error".to_string())
                    ));
                }
            }
            UiEvent::Host(HostEvent::Online) => self.notice("Host reconnected"),
            UiEvent::Host(HostEvent::Offline) => self.notice("Host disconnected; game paused"),
            UiEvent::Host(HostEvent::Changed { to }) => {
                self.notice(format!("Host changed to {to}"))
            }
            UiEvent::Error(error) => self.notice(format!("Error: {error}")),
        }
        self.refresh().await?;
        Ok(chat_message)
    }

    fn move_selection(&mut self, dx: isize, dy: isize) {
        let row = self.selected_cell / 3;
        let col = self.selected_cell % 3;
        let next_row = (row as isize + dy).clamp(0, 2) as usize;
        let next_col = (col as isize + dx).clamp(0, 2) as usize;
        self.selected_cell = next_row * 3 + next_col;
    }

    async fn submit_chat(&mut self) -> Result<()> {
        if self.chat_input.trim().is_empty() {
            Ok(())
        } else {
            let message = std::mem::take(&mut self.chat_input);
            self.room.send_chat(message.trim()).await?;
            Ok(())
        }
    }

    async fn submit_move(&mut self) -> Result<()> {
        match self
            .room
            .submit_action(TicTacToeAction::Place(self.selected_cell as u8))
            .await
        {
            Ok(()) => self.notice(format!("Submitted cell {}", self.selected_cell + 1)),
            Err(err) => self.notice(format!("Move not sent: {err}")),
        }
        Ok(())
    }

    async fn set_ready(&mut self, ready: bool) -> Result<()> {
        match self.room.set_ready(ready).await {
            Ok(()) => {
                self.notice(if ready { "Ready" } else { "Not ready" });
                self.refresh().await?;
            }
            Err(err) => self.notice(format!("Could not update ready state: {err}")),
        }
        Ok(())
    }

    async fn start_game(&mut self) -> Result<()> {
        if !self.snapshot.is_host {
            self.notice("Only the host can start");
            return Ok(());
        }
        match self.room.start_game().await {
            Ok(()) => self.notice("Starting game"),
            Err(err) => self.notice(format!("Start failed: {err}")),
        }
        Ok(())
    }

    async fn forfeit(&mut self) -> Result<()> {
        match self.room.forfeit().await {
            Ok(()) => self.notice("Forfeited active play"),
            Err(err) => self.notice(format!("Forfeit failed: {err}")),
        }
        Ok(())
    }

    pub fn notice(&mut self, message: impl Into<String>) {
        self.notices.push(message.into());
        let excess = self.notices.len().saturating_sub(6);
        if excess > 0 {
            self.notices.drain(0..excess);
        }
    }

    pub async fn leave(self) -> Result<()> {
        self.room
            .announce_leave(&LeaveReason::ApplicationClosed)
            .await
    }
}
