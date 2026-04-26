//! # Tic Tac Toe example
//!
//! From the CLI either create a new game or enter a ticket to join an existing game lobby.
//! Then select your moves one by one until a winner is declared.
//!
//! ## Host a game
//!
//! ```sh
//! cargo run --example tictactoe host
//! ```
//!
//! ## Join a game
//!
//! ```sh
//! cargo run --example tictactoe join <ticket>
//! ```
#![allow(unused)]

use anyhow::Result;
use clap::Parser;
use iroh::EndpointId;
use p2p_game_engine::{ConnectionEffect, GameLogic, GameRoom, HostEvent, PeerMap, UiEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::io::{self, Write};
use thiserror::Error;
use tokio_util::io::ReaderStream;

// --- CLI Setup ---

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser, Debug)]
enum Commands {
    /// Host a new game
    Host,
    /// Join an existing game
    Join {
        /// The ticket to join the game
        ticket: String,
    },
}

// --- Game Logic Definition ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum PlayerRole {
    X,
    O,
    Observer,
}

impl fmt::Display for PlayerRole {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PlayerRole::X => write!(f, "X"),
            PlayerRole::O => write!(f, "O"),
            PlayerRole::Observer => write!(f, "Observer"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum Cell {
    Empty,
    Occupied(PlayerRole),
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Cell::Empty => write!(f, " "),
            Cell::Occupied(role) => write!(f, "{}", role),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GameStatus {
    Ongoing,
    Win(PlayerRole),
    Draw,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TicTacToeState {
    board: [Cell; 9],
    status: GameStatus,
    current_turn: PlayerRole,
    roles: HashMap<EndpointId, PlayerRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TicTacToeAction {
    Place(u8), // 0-8
}

#[derive(Debug, Error)]
pub enum GameError {
    #[error("Not your turn")]
    NotYourTurn,
    #[error("Cell is already occupied")]
    CellOccupied,
    #[error("Invalid cell number")]
    InvalidCell,
    #[error("Game is already over")]
    GameOver,
    #[error("You are not a player in this game")]
    NotAPlayer,
    #[error("Not enough players to start a game")]
    NotEnoughPlayers,
}

#[derive(Debug, Clone)]
pub struct TicTacToeLogic;

impl GameLogic for TicTacToeLogic {
    type GameState = TicTacToeState;
    type GameAction = TicTacToeAction;
    type PlayerRole = PlayerRole;
    type GameError = GameError;
    type PlayerLeaveReason = ();

    fn is_observer_role(&self, role: &Self::PlayerRole) -> bool {
        *role == PlayerRole::Observer
    }

    fn assign_roles(
        &self,
        players: &PeerMap,
    ) -> std::result::Result<HashMap<EndpointId, Self::PlayerRole>, Self::GameError> {
        // The first two players become X and O. Everyone else is an observer.
        let mut roles = HashMap::new();
        let mut player_roles = [PlayerRole::X, PlayerRole::O].into_iter();
        let mut player_ids: Vec<_> = players.keys().copied().collect();
        player_ids.sort();

        for player_id in player_ids {
            if let Some(role) = player_roles.next() {
                roles.insert(player_id, role);
            } else {
                roles.insert(player_id, PlayerRole::Observer);
            }
        }
        Ok(roles)
    }

    fn validate_start(
        &self,
        _players: &PeerMap,
        roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> std::result::Result<(), Self::GameError> {
        // Ensure we have both an X and an O before starting.
        let has_x = roles.values().any(|&r| r == PlayerRole::X);
        let has_o = roles.values().any(|&r| r == PlayerRole::O);

        if !(has_x && has_o) {
            return Err(GameError::NotEnoughPlayers);
        }
        Ok(())
    }

    fn initial_state(
        &self,
        _players: &PeerMap,
        roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> std::result::Result<Self::GameState, Self::GameError> {
        Ok(TicTacToeState {
            board: [Cell::Empty; 9],
            status: GameStatus::Ongoing,
            current_turn: PlayerRole::X, // X always starts
            roles: roles.clone(),
        })
    }

    fn handle_player_disconnect(
        &self,
        players: &mut PeerMap,
        player_id: &EndpointId,
        current_state: &mut Self::GameState,
    ) -> std::result::Result<ConnectionEffect, Self::GameError> {
        // TODO add disconnect behaviour.
        Ok(ConnectionEffect::NoChange)
    }

    fn handle_player_reconnect(
        &self,
        _players: &mut PeerMap,
        _player_id: &EndpointId,
        _current_state: &mut Self::GameState,
    ) -> std::result::Result<ConnectionEffect, Self::GameError> {
        // TODO add reconnect behaviour.
        Ok(ConnectionEffect::NoChange)
    }

    fn handle_player_forfeit(
        &self,
        _players: &mut PeerMap,
        player_id: &EndpointId,
        current_state: &mut Self::GameState,
    ) -> std::result::Result<ConnectionEffect, Self::GameError> {
        current_state.roles.insert(*player_id, PlayerRole::Observer);
        Ok(ConnectionEffect::StateChanged)
    }

    fn apply_action(
        &self,
        state: &mut Self::GameState,
        player_id: &EndpointId,
        action: &Self::GameAction,
    ) -> Result<(), Self::GameError> {
        if state.status != GameStatus::Ongoing {
            return Err(GameError::GameOver);
        }

        let player_role = state.roles.get(player_id).ok_or(GameError::NotAPlayer)?;

        if *player_role != state.current_turn {
            return Err(GameError::NotYourTurn);
        }

        match action {
            TicTacToeAction::Place(cell_idx) => {
                let idx = *cell_idx as usize;
                if idx > 8 {
                    return Err(GameError::InvalidCell);
                }
                if state.board[idx] != Cell::Empty {
                    return Err(GameError::CellOccupied);
                }

                state.board[idx] = Cell::Occupied(*player_role);

                // Check for win condition
                const WIN_CONDITIONS: [[usize; 3]; 8] = [
                    [0, 1, 2],
                    [3, 4, 5],
                    [6, 7, 8], // rows
                    [0, 3, 6],
                    [1, 4, 7],
                    [2, 5, 8], // columns
                    [0, 4, 8],
                    [2, 4, 6], // diagonals
                ];

                for &condition in &WIN_CONDITIONS {
                    if state.board[condition[0]] == state.board[condition[1]]
                        && state.board[condition[1]] == state.board[condition[2]]
                        && state.board[condition[0]] != Cell::Empty
                    {
                        state.status = GameStatus::Win(*player_role);
                        state.current_turn = PlayerRole::Observer; // No more turns
                        return Ok(());
                    }
                }

                // Check for draw condition (no empty cells left)
                if state.board.iter().all(|&c| c != Cell::Empty) {
                    state.status = GameStatus::Draw;
                    state.current_turn = PlayerRole::Observer; // No more turns
                    return Ok(());
                }

                // Switch turns
                state.current_turn = if state.current_turn == PlayerRole::X {
                    PlayerRole::O
                } else {
                    PlayerRole::X
                };
            }
        }
        Ok(())
    }
}

fn print_board(state: &TicTacToeState) {
    println!("\n-------------");
    for r in 0..3 {
        print!("| ");
        for c in 0..3 {
            print!("{} | ", state.board[r * 3 + c]);
        }
        println!("\n-------------");
    }

    match &state.status {
        GameStatus::Ongoing => println!("Turn: {}", state.current_turn),
        GameStatus::Win(winner) => println!("Winner: {}!", winner),
        GameStatus::Draw => println!("It's a draw!"),
    }
    println!();
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let data_dir = tempfile::tempdir()?;
    let data_path = data_dir.path().to_path_buf();

    // --- Setup Room ---
    let (room, mut events) = match cli.command {
        Commands::Host => {
            let (room, events) = GameRoom::create(TicTacToeLogic, Some(data_path)).await?;
            println!("Game hosted! Your ID: {}", room.id());
            println!("Ticket: {}", room.ticket().await?);
            print!("Enter your name: ");
            io::stdout().flush()?;
            let mut name = String::new();
            io::stdin().read_line(&mut name)?;
            room.enter_lobby(name.trim()).await?;
            println!("Welcome {name}! Waiting for player O to join...");
            println!("Type 'ready' when you are ready to play.");
            println!("Once player O has joined, type 'start' to begin the game.");
            (room, events)
        }
        Commands::Join { ticket } => {
            let (room, events) = GameRoom::join(TicTacToeLogic, &ticket, Some(data_path)).await?;
            println!("Joined game! Your ID: {}", room.id());
            print!("Enter your name: ");
            io::stdout().flush()?;
            let mut name = String::new();
            io::stdin().read_line(&mut name)?;
            room.enter_lobby(name.trim()).await?;
            println!("Welcome {name}! Type 'ready' when you are ready to play.");
            println!("Waiting for the host to start the game...");
            (room, events)
        }
    };

    // --- Event Loop ---
    let mut stdin = ReaderStream::new(tokio::io::stdin());

    loop {
        print!("> ");
        io::stdout().flush()?;

        tokio::select! {
            // Handle user input
            Some(Ok(input)) = futures::StreamExt::next(&mut stdin) => {
                let line = String::from_utf8(input.to_vec())?.trim().to_string();

                if line.is_empty() { continue; }

                if line == "ready" {
                    room.set_ready(true).await?;
                    println!("You are ready.");
                    continue;
                }

                if line == "unready" {
                    room.set_ready(false).await?;
                    println!("You are not ready.");
                    continue;
                }

                if room.is_host().await? && line == "start" {
                    println!("Starting game...");
                    if let Err(e) = room.start_game().await {
                        eprintln!("Failed to start game: {}", e);
                    }
                    continue;
                }

                if let Ok(num) = line.parse::<u8>() {
                    if (1..=9).contains(&num) {
                        println!("Submitting move: {}", num);
                        let action = TicTacToeAction::Place(num - 1);
                        if let Err(e) = room.submit_action(action).await {
                             eprintln!("Invalid move: {}", e);
                        }
                    } else {
                        eprintln!("Invalid move. Enter a number from 1-9.");
                    }
                } else {
                    // Treat as chat
                    if let Err(e) = room.send_chat(&line).await {
                        eprintln!("Failed to send chat: {}", e);
                    }
                }
            }

            // Handle game events
            Some(event) = events.recv() => {
                match event {
                    UiEvent::Peer(players) => {
                        let player_list: Vec<&String> = players.values().map(|p|&p.profile.nickname).collect();
                        println!("\nPeers updated. Players now: {player_list:?}");
                    }
                    UiEvent::GameState(state) => {
                        if let Some(role) = state.roles.get(&room.id()) {
                            println!("\nGame state updated! Your role is: {role}");
                        } else {
                            println!("\nGame state updated! You are an observer.");
                        }
                        print_board(&state);
                    },
                    UiEvent::Chat{sender, msg} => {
                        let from = if msg.from == room.id() { "You".to_string() } else { format!("Player {}", &msg.from.to_string()[..5]) };
                        println!("\n[Chat] {sender}: {}\n{}", msg.message, msg.from);
                    }
                    UiEvent::ActionResult(result) => {
                        if !result.accepted {
                            eprintln!("\nAction rejected: {}", result.error.unwrap_or_else(|| "unknown error".to_string()));
                        }
                    }
                    UiEvent::Error(e) => eprintln!("\nAn error occurred: {e}"),
                    UiEvent::AppState(app_state) => {
                        println!("\nGame state changed to: {app_state:?}");
                    }
                    UiEvent::Host(HostEvent::Offline) => {
                        println!("\nGame Host disconnected. The game is paused");
                        break;
                    },
                    UiEvent::Host(HostEvent::Online) => {
                        println!("\nGame Host reconnected. The game is unpaused")
                    }
                    UiEvent::Host(HostEvent::Changed { to }) => {
                        println!("\nGame Host assigned to: {to}")
                    }
                }
            }
            else => {
                break; // events channel closed
            }
        }
    }
    Ok(())
}
