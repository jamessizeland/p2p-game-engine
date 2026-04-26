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
    Reject,
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
    type PlayerLeaveReason = ();

    fn assign_roles(
        &self,
        players: &PeerMap,
    ) -> Result<HashMap<EndpointId, Self::PlayerRole>, Self::GameError> {
        Ok(players
            .keys()
            .map(|id| (*id, TestPlayerRole::Counter))
            .collect())
    }

    fn validate_start(
        &self,
        _players: &PeerMap,
        _roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> Result<(), Self::GameError> {
        Ok(())
    }

    fn initial_state(
        &self,
        _players: &PeerMap,
        _roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> Result<Self::GameState, Self::GameError> {
        Ok(TestGameState { counter: 0 })
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
            TestGameAction::Reject => Err(TestGameError::Unknown),
        }
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
        _player_id: &EndpointId,
        _current_state: &mut Self::GameState,
    ) -> Result<ConnectionEffect, Self::GameError> {
        Ok(ConnectionEffect::NoChange)
    }
}

pub async fn await_event(
    event: &mut mpsc::Receiver<UiEvent<TestGame>>,
) -> anyhow::Result<UiEvent<TestGame>> {
    // Long timeout is to give reconnections time to happen.
    let duration = Duration::from_secs(30);
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
    let ticket_string = host_room.ticket().await?.to_string();
    println!("Host Ticket: {}", &ticket_string);

    println!("Announcing Host Presence");
    host_room.announce_presence(name).await?;
    let event = await_event(&mut host_events).await?;
    println!("Received Host Lobby Update: {event}");
    let host_id = host_room.id();
    match event {
        UiEvent::Peer(players) => {
            assert_eq!(players.len(), 1);
            assert!(players.contains_key(&host_id));
            assert_eq!(players.get(&host_id).unwrap().profile.nickname, name);
        }
        _ => panic!("Host received wrong event type"),
    }
    host_room.set_ready(true).await?;
    Ok((host_room, ticket_string, host_id, host_events))
}

pub async fn setup_persistent_test_room(
    name: &str,
    path: std::path::PathBuf,
) -> anyhow::Result<(
    GameRoom<TestGame>,
    String,
    EndpointId,
    mpsc::Receiver<UiEvent<TestGame>>,
)> {
    println!("Setting up Persistent Host Room");
    let (host_room, mut host_events) = GameRoom::create(TestGame, Some(path)).await?;
    let ticket_string = host_room.ticket().await?.to_string();
    println!("Host Ticket: {}", &ticket_string);

    println!("Announcing Host Presence");
    host_room.announce_presence(name).await?;
    let event = await_event(&mut host_events).await?;
    println!("Received Host Lobby Update: {event}");
    let host_id = host_room.id();
    match event {
        UiEvent::Peer(players) => {
            assert_eq!(players.len(), 1);
            assert!(players.contains_key(&host_id));
        }
        _ => panic!("Host received wrong event type"),
    }
    host_room.set_ready(true).await?;
    Ok((host_room, ticket_string, host_id, host_events))
}

pub async fn join_test_room(
    name: &str,
    ticket_string: &str,
    mut retries: i32,
) -> anyhow::Result<(GameRoom<TestGame>, mpsc::Receiver<UiEvent<TestGame>>)> {
    println!("Setting up Client Room");
    // Sometimes this fails, so we have a retry mechanic.
    let (client_room, mut client_events) = loop {
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
    await_lobby_contains(&mut client_events, &client_room.id()).await?;
    client_room.set_ready(true).await?;
    Ok((client_room, client_events))
}

pub async fn await_lobby_contains(
    events: &mut mpsc::Receiver<UiEvent<TestGame>>,
    player_id: &EndpointId,
) -> anyhow::Result<()> {
    loop {
        let event = await_event(events).await?;
        if let UiEvent::Peer(players) = event
            && players.contains_key(player_id)
        {
            return Ok(());
        }
    }
}

/// Wait until we have the expected number of players in the lobby
pub async fn await_lobby_update(
    events: &mut mpsc::Receiver<UiEvent<TestGame>>,
    expected_players: usize,
) -> anyhow::Result<()> {
    loop {
        let event = await_event(events).await?;
        if let UiEvent::Peer(players) = event {
            if players.len() == expected_players {
                return Ok(());
            }
        }
    }
}

pub async fn await_peer_list_count(
    room: &GameRoom<TestGame>,
    expected_players: usize,
) -> anyhow::Result<PeerMap> {
    let duration = Duration::from_secs(30);
    tokio::time::timeout(duration, async {
        loop {
            let players = room.get_peer_list().await?;
            if players.len() == expected_players {
                return Ok(players);
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await?
}

pub async fn await_peer_ready(
    room: &GameRoom<TestGame>,
    player_id: &EndpointId,
    expected_ready: bool,
) -> anyhow::Result<()> {
    let duration = Duration::from_secs(30);
    tokio::time::timeout(duration, async {
        loop {
            let players = room.get_peer_list().await?;
            if players
                .get(player_id)
                .is_some_and(|player| player.ready == expected_ready)
            {
                return Ok(());
            }
            sleep(Duration::from_millis(100)).await;
        }
    })
    .await?
}

/// When a game starts we see two events (in non-deterministic order)
pub async fn await_game_start(
    events: &mut mpsc::Receiver<UiEvent<TestGame>>,
) -> anyhow::Result<()> {
    println!("Waiting for game to start...");
    let mut has_seen_app_state_update = false;
    let mut has_seen_game_state_update = false;
    loop {
        match await_event(events).await? {
            UiEvent::AppState(AppState::InGame) => {
                has_seen_app_state_update = true;
            }
            UiEvent::GameState(..) => has_seen_game_state_update = true,
            _ => {}
        }
        if has_seen_app_state_update && has_seen_game_state_update {
            return Ok(());
        }
    }
}

pub async fn await_counter_state(
    events: &mut mpsc::Receiver<UiEvent<TestGame>>,
    expected_counter: u32,
) -> anyhow::Result<()> {
    loop {
        let event = await_event(events).await?;
        if let UiEvent::GameState(TestGameState { counter }) = event {
            if counter == expected_counter {
                return Ok(());
            }
        }
    }
}

pub async fn await_action_result(
    events: &mut mpsc::Receiver<UiEvent<TestGame>>,
    accepted: bool,
) -> anyhow::Result<ActionResult> {
    loop {
        let event = await_event(events).await?;
        if let UiEvent::ActionResult(result) = event
            && result.accepted == accepted
        {
            return Ok(result);
        }
    }
}

pub async fn await_host_event(
    events: &mut mpsc::Receiver<UiEvent<TestGame>>,
    expected: HostEvent,
) -> anyhow::Result<()> {
    loop {
        let event = await_event(events).await?;
        if let UiEvent::Host(host_event) = event
            && host_event == expected
        {
            return Ok(());
        }
    }
}

/// Wait until a specific player's status is updated in the lobby
pub async fn await_lobby_status_update(
    events: &mut mpsc::Receiver<UiEvent<TestGame>>,
    player_id: &EndpointId,
    expected_status: PeerStatus,
) -> anyhow::Result<()> {
    loop {
        let event = await_event(events).await?;
        if let UiEvent::Peer(players) = event {
            if let Some(player) = players.get(player_id) {
                if player.status == expected_status {
                    return Ok(());
                }
            }
        }
    }
}

/// Wait until a specific player's observer flag is updated.
pub async fn await_lobby_observer_update(
    events: &mut mpsc::Receiver<UiEvent<TestGame>>,
    player_id: &EndpointId,
    expected_observer: bool,
) -> anyhow::Result<()> {
    loop {
        let event = await_event(events).await?;
        if let UiEvent::Peer(players) = event
            && let Some(player) = players.get(player_id)
            && player.is_observer == expected_observer
        {
            return Ok(());
        }
    }
}

pub async fn await_lobby_ready_update(
    events: &mut mpsc::Receiver<UiEvent<TestGame>>,
    player_id: &EndpointId,
    expected_ready: bool,
) -> anyhow::Result<()> {
    loop {
        let event = await_event(events).await?;
        if let UiEvent::Peer(players) = event
            && let Some(player) = players.get(player_id)
            && player.ready == expected_ready
        {
            return Ok(());
        }
    }
}
