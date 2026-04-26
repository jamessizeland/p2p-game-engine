//! Game Logic
//!
//! This module contains the `GameLogic` trait, which defines the core logic of a turn-based game,
//! including how to apply actions, assign roles, and handle player disconnects and reconnects.
//! It also defines the `ConnectionEffect` enum, which indicates how the game state should be updated
//! in response to player connections and disconnections.

use iroh::EndpointId;
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::HashMap, error::Error, fmt::Debug};

use crate::PeerMap;

/// The effect of a player connection or disconnection on the game state,
/// indicating whether the state or peer list has changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEffect {
    /// No change to the game state or peer list is necessary.
    NoChange,
    /// The game state has changed and needs to be updated for all players.
    StateChanged,
    /// The peer list has changed and needs to be updated for all players.
    PeersChanged,
    /// Both the game state and peer list have changed and need to be updated for all players.
    StateAndPeersChanged,
}

/// Generic Trait for p2p turn based games.
pub trait GameLogic: Debug + Send + Sync + 'static {
    /// Current State of the game
    type GameState: Serialize + DeserializeOwned + Clone + Debug + Send + Sync;
    /// Actions that can be taken in the game
    type GameAction: Serialize + DeserializeOwned + Clone + Debug + Send + Sync;
    /// Roles that can be assigned to players
    type PlayerRole: Serialize + DeserializeOwned + Clone + Debug + Send + Sync;
    /// Game specific reasons for a player to leave the game
    /// Common non-specific reasons are also available via [LeaveReason]
    type PlayerLeaveReason: Serialize + DeserializeOwned + Clone + Debug + Send + Sync;
    /// Errors specific to this game
    type GameError: Error + Send + Sync;

    /// Returns true when a role should be treated as a non-acting observer.
    fn is_observer_role(&self, _role: &Self::PlayerRole) -> bool {
        false
    }

    /// Assigns roles to players at the start of the game.
    fn assign_roles(
        &self,
        players: &PeerMap,
    ) -> Result<HashMap<EndpointId, Self::PlayerRole>, Self::GameError>;

    /// Check that all game specific conditions are met for starting this game.
    fn validate_start(
        &self,
        players: &PeerMap,
        roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> Result<(), Self::GameError>;

    /// Creates the initial game state from the lobby info.
    fn initial_state(
        &self,
        players: &PeerMap,
        roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> Result<Self::GameState, Self::GameError>;

    /// The core game logic: validates and applies an action.
    fn apply_action(
        &self,
        current_state: &mut Self::GameState,
        player_id: &EndpointId,
        action: &Self::GameAction,
    ) -> Result<(), Self::GameError>;

    /// Deal with a player disconnecting from the game.
    fn handle_player_disconnect(
        &self,
        players: &mut PeerMap,
        player_id: &EndpointId,
        current_state: &mut Self::GameState,
    ) -> Result<ConnectionEffect, Self::GameError>;

    // Deal with a player reconnecting to the game.
    fn handle_player_reconnect(
        &self,
        players: &mut PeerMap,
        player_id: &EndpointId,
        current_state: &mut Self::GameState,
    ) -> Result<ConnectionEffect, Self::GameError>;

    /// Deal with a player forfeiting active participation.
    fn handle_player_forfeit(
        &self,
        players: &mut PeerMap,
        player_id: &EndpointId,
        current_state: &mut Self::GameState,
    ) -> Result<ConnectionEffect, Self::GameError>;
}
