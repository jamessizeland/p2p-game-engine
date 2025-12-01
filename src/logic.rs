#![allow(dead_code)]
use iroh::EndpointId;
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::HashMap, error::Error, fmt::Debug};

use crate::PeerMap;

/// Generic Trait for p2p turn based games.
pub trait GameLogic: Debug + Send + Sync + 'static {
    type GameState: Serialize + DeserializeOwned + Clone + Debug + Send + Sync;
    type GameAction: Serialize + DeserializeOwned + Clone + Debug + Send + Sync;
    type PlayerRole: Serialize + DeserializeOwned + Clone + Debug + Send + Sync;
    type GameEndReason: Serialize + DeserializeOwned + Clone + Debug + Send + Sync;
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
}
