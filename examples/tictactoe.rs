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
use p2p_game_engine::{GameEvent, GameLogic, GameRoom, PlayerMap};
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

    fn assign_roles(&self, players: &PlayerMap) -> HashMap<EndpointId, Self::PlayerRole> {
        // The first two players become X and O. Everyone else is an observer.
        let mut roles = HashMap::new();
        let mut player_roles = [PlayerRole::X, PlayerRole::O].into_iter();

        for (player_id, _player_info) in players {
            if let Some(role) = player_roles.next() {
                roles.insert(*player_id, role);
            }
        }

        // Assign remaining players as observers
        for player_id in players.keys() {
            roles.entry(*player_id).or_insert(PlayerRole::Observer);
        }
        roles
    }

    fn initial_state(&self, roles: &HashMap<EndpointId, Self::PlayerRole>) -> Self::GameState {
        // Ensure we have both an X and an O before starting.
        let has_x = roles.values().any(|&r| r == PlayerRole::X);
        let has_o = roles.values().any(|&r| r == PlayerRole::O);

        if !(has_x && has_o) {
            // This is a logic error. The host UI should prevent this.
            panic!("Attempted to start a Tic-Tac-Toe game without two players (X and O).");
        }

        TicTacToeState {
            board: [Cell::Empty; 9],
            status: GameStatus::Ongoing,
            current_turn: PlayerRole::X, // X always starts
            roles: roles.clone(),
        }
    }

    fn start_conditions_met(
        &self,
        players: &PlayerMap,
        current_state: &Self::GameState,
    ) -> std::result::Result<(), Self::GameError> {
        if players.len() < 2 {
            Err(GameError::NotEnoughPlayers)
        } else {
            Ok(())
        }
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
    let data_path = tempfile::tempdir()?.path().to_path_buf();

    // --- Setup Room ---
    let (room, mut events) = match cli.command {
        Commands::Host => {
            let (room, events) = GameRoom::create(TicTacToeLogic, data_path).await?;
            println!("Game hosted! Your ID: {}", room.id());
            println!("Ticket: {}", room.ticket());
            println!("Your role is X. Waiting for player O to join...");
            println!("Once player O has joined, type 'start' to begin the game.");
            (room, events)
        }
        Commands::Join { ticket } => {
            let (room, events) = GameRoom::join(TicTacToeLogic, ticket, data_path).await?;
            println!("Joined game! Your ID: {}", room.id());
            print!("Enter your name: ");
            io::stdout().flush()?;
            let mut name = String::new();
            io::stdin().read_line(&mut name)?;
            room.announce_presence(name.trim()).await?;
            println!("Welcome! Waiting for the host to start the game...");
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
                    GameEvent::LobbyUpdated(players) => {
                        println!("\nLobby updated. Players now: {:?}", players.values().map(|p|&p.name).collect::<Vec<_>>());
                    }
                    GameEvent::StateUpdated(state) => {
                        if let Some(role) = state.roles.get(&room.id()) {
                            println!("\nGame state updated! Your role is: {role}");
                        } else {
                            println!("\nGame state updated! You are an observer.");
                        }
                        print_board(&state);
                    },
                    GameEvent::ChatReceived(msg) => {
                        let from = if msg.from == room.id() { "You".to_string() } else { format!("Player {}", &msg.from.to_string()[..5]) };
                        println!("\n[Chat] {}: {}", from, msg.message);
                    }
                    GameEvent::Error(e) => eprintln!("\nAn error occurred: {}", e),
                    GameEvent::AppStateChanged(app_state) => {
                        println!("\nGame state changed to: {:?}", app_state);
                    }
                    GameEvent::HostDisconnected => {
                        println!("\nGame Host disconnected. The game is over.");
                        break;
                    },
                }
            }
            else => {
                break; // events channel closed
            }
        }
    }
    Ok(())
}
