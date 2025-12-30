#![allow(dead_code)]
use iroh::EndpointId;
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::HashMap, error::Error, fmt::Debug};

use crate::PeerMap;

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

    /// Assigns roles to players at the start of the game.
    fn assign_roles(&self, players: &PeerMap) -> HashMap<EndpointId, Self::PlayerRole>;

    /// Creates the initial game state from the lobby info.
    fn initial_state(&self, roles: &HashMap<EndpointId, Self::PlayerRole>) -> Self::GameState;

    /// The core game logic: validates and applies an action.
    fn apply_action(
        &self,
        current_state: &mut Self::GameState,
        player_id: &EndpointId,
        action: &Self::GameAction,
    ) -> Result<(), Self::GameError>;

    /// Check that all game specific conditions are met for starting this game.
    fn start_conditions_met(
        &self,
        players: &PeerMap,
        current_state: &Self::GameState,
    ) -> Result<(), Self::GameError>;

    /// Deal with a player disconnecting from the game.
    fn handle_player_disconnect(
        &self,
        players: &mut PeerMap,
        player_id: &EndpointId,
        current_state: &mut Self::GameState,
    ) -> Result<(), Self::GameError>;

    // Deal with a player reconnecting to the game.
    fn handle_player_reconnect(
        &self,
        players: &mut PeerMap,
        player_id: &EndpointId,
        current_state: &mut Self::GameState,
    ) -> Result<(), Self::GameError>;
}
