//! Tic Tac Toe rules used by the ratatui showcase.

use iroh::EndpointId;
use p2p_game_engine::{ConnectionEffect, GameLogic, PeerMap};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt};
use thiserror::Error;

/// Role assigned to a peer once the game starts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum PlayerRole {
    X,
    O,
    Observer,
}

impl fmt::Display for PlayerRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlayerRole::X => write!(f, "X"),
            PlayerRole::O => write!(f, "O"),
            PlayerRole::Observer => write!(f, "Observer"),
        }
    }
}

/// A single Tic Tac Toe board cell.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Copy)]
pub enum Cell {
    Empty,
    Occupied(PlayerRole),
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Cell::Empty => write!(f, " "),
            Cell::Occupied(role) => write!(f, "{role}"),
        }
    }
}

/// Game-over status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GameStatus {
    Ongoing,
    Win(PlayerRole),
    Draw,
}

/// Host-authored Tic Tac Toe state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TicTacToeState {
    pub board: [Cell; 9],
    pub status: GameStatus,
    pub current_turn: PlayerRole,
    pub roles: HashMap<EndpointId, PlayerRole>,
}

/// Actions peers may submit to the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TicTacToeAction {
    Place(u8),
}

/// Tic Tac Toe rule errors.
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

/// Tic Tac Toe game logic implementation.
#[derive(Debug, Clone)]
pub struct TicTacToeLogic;

impl GameLogic for TicTacToeLogic {
    const GAME_NAME: &'static str = "Tic Tac Toe";
    type GameState = TicTacToeState;
    type GameAction = TicTacToeAction;
    type PlayerRole = PlayerRole;
    type PlayerLeaveReason = ();
    type GameError = GameError;

    fn is_observer_role(&self, role: &Self::PlayerRole) -> bool {
        *role == PlayerRole::Observer
    }

    fn assign_roles(
        &self,
        players: &PeerMap,
    ) -> Result<HashMap<EndpointId, Self::PlayerRole>, Self::GameError> {
        let mut roles = HashMap::new();
        let mut player_roles = [PlayerRole::X, PlayerRole::O].into_iter();
        let mut player_ids: Vec<_> = players.keys().copied().collect();
        player_ids.sort();

        for player_id in player_ids {
            let role = player_roles.next().unwrap_or(PlayerRole::Observer);
            roles.insert(player_id, role);
        }
        Ok(roles)
    }

    fn validate_start(
        &self,
        _players: &PeerMap,
        roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> Result<(), Self::GameError> {
        let has_x = roles.values().any(|&role| role == PlayerRole::X);
        let has_o = roles.values().any(|&role| role == PlayerRole::O);
        if has_x && has_o {
            Ok(())
        } else {
            Err(GameError::NotEnoughPlayers)
        }
    }

    fn initial_state(
        &self,
        _players: &PeerMap,
        roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> Result<Self::GameState, Self::GameError> {
        Ok(TicTacToeState {
            board: [Cell::Empty; 9],
            status: GameStatus::Ongoing,
            current_turn: PlayerRole::X,
            roles: roles.clone(),
        })
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
            TicTacToeAction::Place(cell_idx) => apply_place(state, *player_role, *cell_idx)?,
        }
        Ok(())
    }

    fn handle_player_disconnect(
        &self,
        _players: &mut PeerMap,
        _player_id: &EndpointId,
        _current_state: &mut Self::GameState,
    ) -> Result<ConnectionEffect, Self::GameError> {
        Ok(ConnectionEffect::NoChange)
    }

    fn handle_player_reconnect(
        &self,
        _players: &mut PeerMap,
        _player_id: &EndpointId,
        _current_state: &mut Self::GameState,
    ) -> Result<ConnectionEffect, Self::GameError> {
        Ok(ConnectionEffect::NoChange)
    }

    fn handle_player_forfeit(
        &self,
        _players: &mut PeerMap,
        player_id: &EndpointId,
        current_state: &mut Self::GameState,
    ) -> Result<ConnectionEffect, Self::GameError> {
        current_state.roles.insert(*player_id, PlayerRole::Observer);
        Ok(ConnectionEffect::StateChanged)
    }
}

fn apply_place(
    state: &mut TicTacToeState,
    player_role: PlayerRole,
    cell_idx: u8,
) -> Result<(), GameError> {
    let idx = cell_idx as usize;
    if idx > 8 {
        return Err(GameError::InvalidCell);
    }
    if state.board[idx] != Cell::Empty {
        return Err(GameError::CellOccupied);
    }

    state.board[idx] = Cell::Occupied(player_role);
    if let Some(winner) = winner(&state.board) {
        state.status = GameStatus::Win(winner);
        state.current_turn = PlayerRole::Observer;
    } else if state.board.iter().all(|&cell| cell != Cell::Empty) {
        state.status = GameStatus::Draw;
        state.current_turn = PlayerRole::Observer;
    } else {
        state.current_turn = if state.current_turn == PlayerRole::X {
            PlayerRole::O
        } else {
            PlayerRole::X
        };
    }
    Ok(())
}

fn winner(board: &[Cell; 9]) -> Option<PlayerRole> {
    const WINS: [[usize; 3]; 8] = [
        [0, 1, 2],
        [3, 4, 5],
        [6, 7, 8],
        [0, 3, 6],
        [1, 4, 7],
        [2, 5, 8],
        [0, 4, 8],
        [2, 4, 6],
    ];
    WINS.iter().find_map(|line| match board[line[0]] {
        Cell::Occupied(role)
            if board[line[1]] == board[line[0]] && board[line[2]] == board[line[0]] =>
        {
            Some(role)
        }
        _ => None,
    })
}
