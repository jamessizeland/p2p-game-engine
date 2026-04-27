//! Application shell for the Tic Tac Toe TUI.

use crate::{game::TicTacToeLogic, session::RoomSession};
use anyhow::{Result, anyhow};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use p2p_game_engine::{AppState, GameRoom};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Top-level application state.
pub struct App {
    pub home: HomeState,
    pub session: Option<RoomSession>,
    pub room_screen: RoomScreen,
    pub chat_popup: Option<ChatPopup>,
    pub should_quit: bool,
    data_path: PathBuf,
}

impl App {
    /// Create the application shell before any room has been opened.
    pub fn new(username: String, data_path: PathBuf) -> Self {
        Self {
            home: HomeState::new(username),
            session: None,
            room_screen: RoomScreen::Lobby,
            chat_popup: None,
            should_quit: false,
            data_path,
        }
    }

    /// Handle room events if a game session is active.
    pub async fn drain_room_events(&mut self) -> Result<()> {
        let mut should_leave = false;
        if let Some(session) = &mut self.session {
            let chat_messages = session.drain_events().await?;
            if !matches!(self.room_screen, RoomScreen::Chat) {
                if let Some(message) = chat_messages.last() {
                    self.chat_popup = Some(ChatPopup::new(message.clone()));
                }
            }
            should_leave = session.should_leave;
        }
        self.advance_room_screen();
        if should_leave {
            self.leave_session("Left game. Choose resume to reenter.")
                .await?;
        }
        if self
            .chat_popup
            .as_ref()
            .is_some_and(|popup| popup.is_expired())
        {
            self.chat_popup = None;
        }
        Ok(())
    }

    /// Handle a terminal key event for either the home screen or room screen.
    pub async fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.session.is_some() {
            if let Some(session) = &mut self.session {
                session.handle_key(key, self.room_screen).await?;
            }
            self.handle_room_shortcut(key).await?;
            if self
                .session
                .as_ref()
                .is_some_and(|session| session.should_leave)
            {
                self.leave_session("Left game. Choose resume to reenter.")
                    .await?;
            }
            return Ok(());
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(());
        }

        if key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char('v')
            || key.code == KeyCode::F(9)
        {
            self.load_ticket_handoff();
            return Ok(());
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab | KeyCode::Down => self.home.next_focus(),
            KeyCode::BackTab | KeyCode::Up => self.home.previous_focus(),
            KeyCode::Left => self.home.previous_action(),
            KeyCode::Right => self.home.next_action(),
            KeyCode::Enter => self.open_selected().await?,
            KeyCode::Backspace => self.home.delete_char(),
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.home.push_char(c);
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_room_shortcut(&mut self, key: KeyEvent) -> Result<()> {
        match (key.modifiers.contains(KeyModifiers::ALT), key.code) {
            (true, KeyCode::Char('l')) | (_, KeyCode::F(1)) => self.room_screen = RoomScreen::Lobby,
            (true, KeyCode::Char('g')) | (_, KeyCode::F(2)) => self.room_screen = RoomScreen::Game,
            (true, KeyCode::Char('t')) | (_, KeyCode::F(3)) => {
                self.room_screen = RoomScreen::Chat;
                self.chat_popup = None;
            }
            (true, KeyCode::Char('e')) | (_, KeyCode::F(4)) => self.room_screen = RoomScreen::Logs,
            (true, KeyCode::Char('c')) | (_, KeyCode::F(9)) => self.copy_ticket().await?,
            _ => {}
        }
        Ok(())
    }

    async fn copy_ticket(&mut self) -> Result<()> {
        let Some(session) = &mut self.session else {
            return Ok(());
        };

        let ticket = match session.room.ticket().await {
            Ok(ticket) => ticket.to_string(),
            Err(err) => {
                session.notice(format!("Could not refresh room ticket: {err}"));
                return Ok(());
            }
        };
        session.ticket = Some(ticket.clone());

        let handoff = write_ticket_handoff(&ticket);
        let clipboard = copy_to_clipboard(&ticket);
        match (clipboard, handoff) {
            (Ok(method), Ok(())) => session.notice(format!(
                "Ticket copied via {method}; also saved for Alt+V/F9 on home"
            )),
            (Ok(method), Err(err)) => session.notice(format!(
                "Ticket copied via {method}; handoff save failed: {err}"
            )),
            (Err(_), Ok(())) => {
                session.notice("Ticket saved for Alt+V/F9 on home; clipboard unavailable")
            }
            (Err(clipboard_err), Err(handoff_err)) => session.notice(format!(
                "Ticket copy failed: {clipboard_err}; handoff save failed: {handoff_err}"
            )),
        }
        Ok(())
    }

    fn load_ticket_handoff(&mut self) {
        match fs::read_to_string(ticket_handoff_path()) {
            Ok(ticket) if !ticket.trim().is_empty() => {
                self.home.join_ticket = ticket.trim().to_string();
                self.home.focus = HomeFocus::Ticket;
                self.home.selected_action = HomeAction::Join;
                self.home.notice = "Loaded copied ticket. Press Enter to join.".to_string();
            }
            _ => {
                self.home.notice = "No copied ticket found yet.".to_string();
            }
        }
    }

    async fn open_selected(&mut self) -> Result<()> {
        match self.home.selected_action {
            HomeAction::Host => self.open_host().await,
            HomeAction::Join => self.open_join().await,
            HomeAction::Resume => self.resume_last().await,
        }
    }

    async fn open_host(&mut self) -> Result<()> {
        let (room, events) = GameRoom::create(TicTacToeLogic, Some(self.data_path.clone())).await?;
        room.enter_lobby(self.home.username.as_str()).await?;
        let ticket = room.ticket().await?.to_string();
        self.home.last_session = Some(LastSession::Host);
        self.home.last_ticket = Some(ticket.clone());
        self.enter_session(room, events, Some(ticket)).await?;
        Ok(())
    }

    async fn open_join(&mut self) -> Result<()> {
        let ticket = self.home.join_ticket.trim().to_string();
        if ticket.is_empty() {
            self.home.notice = "Paste a ticket before joining.".to_string();
            self.home.focus = HomeFocus::Ticket;
            return Ok(());
        }
        let (room, events) =
            GameRoom::join(TicTacToeLogic, &ticket, Some(self.data_path.clone())).await?;
        room.enter_lobby(self.home.username.as_str()).await?;
        self.home.last_session = Some(LastSession::Join);
        self.home.last_ticket = Some(ticket.clone());
        self.enter_session(room, events, None).await?;
        Ok(())
    }

    async fn resume_last(&mut self) -> Result<()> {
        match self.home.last_session {
            Some(LastSession::Host) => {
                let ticket = self
                    .home
                    .last_ticket
                    .clone()
                    .ok_or_else(|| anyhow!("no previous ticket to resume"))?;
                let (room, events) =
                    GameRoom::join(TicTacToeLogic, &ticket, Some(self.data_path.clone())).await?;
                room.enter_lobby(self.home.username.as_str()).await?;
                self.enter_session(room, events, Some(ticket)).await
            }
            Some(LastSession::Join) => {
                let ticket = self
                    .home
                    .last_ticket
                    .clone()
                    .ok_or_else(|| anyhow!("no previous ticket to resume"))?;
                self.home.join_ticket = ticket;
                self.open_join().await
            }
            None => {
                self.home.notice = "No previous game to resume yet.".to_string();
                Ok(())
            }
        }
    }

    async fn enter_session(
        &mut self,
        room: GameRoom<TicTacToeLogic>,
        events: tokio::sync::mpsc::Receiver<p2p_game_engine::UiEvent<TicTacToeLogic>>,
        ticket: Option<String>,
    ) -> Result<()> {
        let session = RoomSession::new(room, events, ticket).await?;
        self.room_screen = room_screen_for(&session);
        self.session = Some(session);
        self.chat_popup = None;
        Ok(())
    }

    async fn leave_session(&mut self, notice: impl Into<String>) -> Result<()> {
        if let Some(session) = self.session.take() {
            session.leave().await?;
        }
        self.room_screen = RoomScreen::Lobby;
        self.chat_popup = None;
        self.home.notice = notice.into();
        self.home.selected_action = HomeAction::Resume;
        Ok(())
    }

    fn advance_room_screen(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        match (session.snapshot.app_state, self.room_screen) {
            (AppState::InGame | AppState::Paused, RoomScreen::Lobby)
                if session.game_state().is_some() =>
            {
                self.room_screen = RoomScreen::Game;
            }
            (AppState::Lobby | AppState::Paused, RoomScreen::Game)
                if session.game_state().is_none() =>
            {
                self.room_screen = RoomScreen::Lobby;
            }
            _ => {}
        }
    }
}

fn room_screen_for(session: &RoomSession) -> RoomScreen {
    match session.snapshot.app_state {
        AppState::InGame | AppState::Paused if session.game_state().is_some() => RoomScreen::Game,
        _ => RoomScreen::Lobby,
    }
}

fn copy_to_clipboard(text: &str) -> io::Result<&'static str> {
    for command in clipboard_commands() {
        if write_to_command(command.program, command.args, text).is_ok() {
            return Ok(command.name);
        }
    }

    let encoded = base64_encode(text.as_bytes());
    let mut stdout = io::stdout();
    write!(stdout, "\x1b]52;c;{encoded}\x07")?;
    stdout.flush()?;
    Ok("terminal")
}

fn write_ticket_handoff(ticket: &str) -> io::Result<()> {
    fs::write(ticket_handoff_path(), ticket)
}

fn ticket_handoff_path() -> PathBuf {
    std::env::temp_dir().join("tictactui-ticket.txt")
}

struct ClipboardCommand {
    name: &'static str,
    program: &'static str,
    args: &'static [&'static str],
}

fn clipboard_commands() -> &'static [ClipboardCommand] {
    &[
        ClipboardCommand {
            name: "wl-copy",
            program: "wl-copy",
            args: &[],
        },
        ClipboardCommand {
            name: "xclip",
            program: "xclip",
            args: &["-selection", "clipboard"],
        },
        ClipboardCommand {
            name: "xsel",
            program: "xsel",
            args: &["--clipboard", "--input"],
        },
        ClipboardCommand {
            name: "pbcopy",
            program: "pbcopy",
            args: &[],
        },
        ClipboardCommand {
            name: "clip.exe",
            program: "clip.exe",
            args: &[],
        },
        ClipboardCommand {
            name: "termux-clipboard-set",
            program: "termux-clipboard-set",
            args: &[],
        },
    ]
}

fn write_to_command(program: &str, args: &[&str], text: &str) -> io::Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }

    let status = child.wait()?;
    status.success().then_some(()).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("{program} exited without copying"),
        )
    })
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let value = ((first as u32) << 16) | ((second as u32) << 8) | third as u32;

        encoded.push(TABLE[((value >> 18) & 0x3f) as usize] as char);
        encoded.push(TABLE[((value >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            encoded.push(TABLE[((value >> 6) & 0x3f) as usize] as char);
        } else {
            encoded.push('=');
        }
        if chunk.len() > 2 {
            encoded.push(TABLE[(value & 0x3f) as usize] as char);
        } else {
            encoded.push('=');
        }
    }

    encoded
}

/// The focused room screen after entering a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomScreen {
    Lobby,
    Game,
    Chat,
    Logs,
}

/// Short-lived chat preview shown outside the chat screen.
pub struct ChatPopup {
    pub message: String,
    expires_at: Instant,
}

impl ChatPopup {
    fn new(message: String) -> Self {
        Self {
            message,
            expires_at: Instant::now() + Duration::from_secs(3),
        }
    }

    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

/// Editable home-screen fields and selection state.
pub struct HomeState {
    pub username: String,
    pub join_ticket: String,
    pub focus: HomeFocus,
    pub selected_action: HomeAction,
    pub last_session: Option<LastSession>,
    pub last_ticket: Option<String>,
    pub notice: String,
}

impl HomeState {
    fn new(username: String) -> Self {
        Self {
            username,
            join_ticket: String::new(),
            focus: HomeFocus::Name,
            selected_action: HomeAction::Host,
            last_session: None,
            last_ticket: None,
            notice: "Set a name, then host or join a room.".to_string(),
        }
    }

    fn push_char(&mut self, c: char) {
        match self.focus {
            HomeFocus::Name => self.username.push(c),
            HomeFocus::Ticket => self.join_ticket.push(c),
            HomeFocus::Action => {}
        }
    }

    fn delete_char(&mut self) {
        match self.focus {
            HomeFocus::Name => {
                self.username.pop();
            }
            HomeFocus::Ticket => {
                self.join_ticket.pop();
            }
            HomeFocus::Action => {}
        }
    }

    fn next_focus(&mut self) {
        self.focus = match self.focus {
            HomeFocus::Name => HomeFocus::Ticket,
            HomeFocus::Ticket => HomeFocus::Action,
            HomeFocus::Action => HomeFocus::Name,
        };
    }

    fn previous_focus(&mut self) {
        self.focus = match self.focus {
            HomeFocus::Name => HomeFocus::Action,
            HomeFocus::Ticket => HomeFocus::Name,
            HomeFocus::Action => HomeFocus::Ticket,
        };
    }

    fn next_action(&mut self) {
        self.selected_action = match self.selected_action {
            HomeAction::Host => HomeAction::Join,
            HomeAction::Join => HomeAction::Resume,
            HomeAction::Resume => HomeAction::Host,
        };
        self.focus = HomeFocus::Action;
    }

    fn previous_action(&mut self) {
        self.selected_action = match self.selected_action {
            HomeAction::Host => HomeAction::Resume,
            HomeAction::Join => HomeAction::Host,
            HomeAction::Resume => HomeAction::Join,
        };
        self.focus = HomeFocus::Action;
    }
}

/// Home screen focus target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeFocus {
    Name,
    Ticket,
    Action,
}

/// Action selected on the home screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeAction {
    Host,
    Join,
    Resume,
}

/// Last room entry mode, used to reconnect from the home screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastSession {
    Host,
    Join,
}
