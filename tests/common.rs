#![allow(dead_code)]

use iroh::EndpointId;
use p2p_game_engine::{GameLogic, PlayerMap};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TestGameError {
    #[error("An unknown error occurred")]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestGameState {
    pub counter: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestGameAction {
    Increment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestPlayerRole {
    Counter,
}

#[derive(Debug, Clone)]
pub struct TestGame;

impl GameLogic for TestGame {
    type GameState = TestGameState;
    type GameAction = TestGameAction;
    type PlayerRole = TestPlayerRole;
    type GameError = TestGameError;

    fn assign_roles(&self, players: &PlayerMap) -> HashMap<EndpointId, Self::PlayerRole> {
        players
            .keys()
            .map(|id| (*id, TestPlayerRole::Counter))
            .collect()
    }

    fn initial_state(&self, _players: &HashMap<EndpointId, Self::PlayerRole>) -> Self::GameState {
        TestGameState { counter: 0 }
    }

    fn apply_action(
        &self,
        current_state: &mut Self::GameState,
        _player_id: &EndpointId,
        action: &Self::GameAction,
    ) -> Result<(), Self::GameError> {
        match action {
            TestGameAction::Increment => {
                current_state.counter += 1;
                Ok(())
            }
        }
    }
    fn start_conditions_met(
        &self,
        _players: &PlayerMap,
        _current_state: &Self::GameState,
    ) -> Result<(), Self::GameError> {
        Ok(())
    }
}
