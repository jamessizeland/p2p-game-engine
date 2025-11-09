#![allow(dead_code)]
use iroh::EndpointId;
use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashMap;

use crate::PlayerMap;

/// Generic Trait for p2p turn based games.
pub trait GameLogic {
    type GameState: Serialize + DeserializeOwned + Clone + Send;
    type GameAction: Serialize + DeserializeOwned + Clone + Send;
    type PlayerRole: Serialize + DeserializeOwned + Clone + Send;
    type GameError: std::error::Error + Send;

    /// Assigns roles to players at the start of the game.
    fn assign_roles(&self, players: &PlayerMap) -> HashMap<EndpointId, Self::PlayerRole>;

    /// Creates the initial game state from the lobby info.
    fn initial_state(&self, players: &HashMap<EndpointId, Self::PlayerRole>) -> Self::GameState;

    /// The core game logic: validates and applies an action.
    fn apply_action(
        &self,
        current_state: &Self::GameState,
        player_id: &EndpointId,
        action: &Self::GameAction,
    ) -> Result<Self::GameState, Self::GameError>;
}
