#![allow(dead_code)]

use iroh::EndpointId;
use p2p_game_engine::*;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};
use thiserror::Error;
use tokio::{sync::mpsc, time::sleep};

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

pub async fn await_event(
    event: &mut mpsc::Receiver<UiEvent<TestGame>>,
) -> anyhow::Result<UiEvent<TestGame>> {
    let duration = Duration::from_secs(2);
    tokio::time::timeout(duration, event.recv())
        .await?
        .ok_or_else(|| anyhow::anyhow!("Timed out waiting for event"))
}

pub async fn setup_test_room(
    name: &str,
) -> anyhow::Result<(
    GameRoom<TestGame>,
    String,
    EndpointId,
    mpsc::Receiver<UiEvent<TestGame>>,
)> {
    println!("Setting up Host Room");
    let (host_room, mut host_events) = GameRoom::create(TestGame, None).await?;
    let ticket_string = host_room.ticket().to_string();
    println!("Host Ticket: {}", &ticket_string);

    println!("Announcing Host Presence");
    host_room.announce_presence(name).await?;
    let event = await_event(&mut host_events).await?;
    println!("Received Host Lobby Update: {event}");
    let host_id = host_room.id();
    match event {
        UiEvent::LobbyUpdated(players) => {
            assert_eq!(players.len(), 1);
            assert!(players.contains_key(&host_id));
            assert_eq!(players.get(&host_id).unwrap().name, name);
        }
        _ => panic!("Host received wrong event type"),
    }
    Ok((host_room, ticket_string, host_id, host_events))
}

pub async fn join_test_room(
    name: &str,
    ticket_string: &str,
    mut retries: i32,
) -> anyhow::Result<(GameRoom<TestGame>, mpsc::Receiver<UiEvent<TestGame>>)> {
    println!("Setting up Client Room");
    // Sometimes this fails, so we have a retry mechanic.
    let (client_room, client_events) = loop {
        sleep(Duration::from_secs(1)).await;
        match GameRoom::join(TestGame, &ticket_string, None).await {
            Ok((room, events)) => break (room, events),
            Err(e) => {
                if retries == 0 {
                    panic!("Failed to join room: {e}");
                }
                println!("Failed to join room: {e}. Retrying...");
                retries -= 1;
            }
        }
    };
    client_room.announce_presence(name).await?;
    Ok((client_room, client_events))
}
