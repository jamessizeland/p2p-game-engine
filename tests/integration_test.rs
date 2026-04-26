//! This is the basic test for setting up rooms and exchanging basic information between them.

mod common;
use common::*;
use iroh::EndpointId;
use p2p_game_engine::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[tokio::test]
async fn test_full_game_lifecycle() -> anyhow::Result<()> {
    // --- SETUP PHASE ---
    let host_name = "HostPlayer";
    let (host_room, ticket_string, host_id, mut host_events) = setup_test_room(host_name).await?;

    let client_name = "ClientPlayer";
    let (client_room, mut client_events) = join_test_room(client_name, &ticket_string, 3).await?;

    // --- LOBBY PHASE ---
    // Host should receive the lobby update
    println!("Waiting for Host Lobby Update...");
    let event = await_event(&mut host_events).await?;
    println!("Received Host Lobby Update: {event}");
    let client_id = client_room.id();
    match event {
        UiEvent::Peer(players) => {
            assert_eq!(players.len(), 2);
            assert!(players.contains_key(&client_id));
            assert!(players.contains_key(&host_id));
            assert_eq!(
                players.get(&client_id).unwrap().profile.nickname,
                client_name
            );
        }
        _ => panic!("Host received wrong event type"),
    }

    println!("Getting player map from host room...");

    // Host can also query the state directly
    let players = host_room.get_peer_list().await?;
    println!("Players: {players}");
    assert_eq!(players.len(), 2);
    assert!(players.contains_key(&client_id));
    assert_eq!(
        players.get(&client_id).unwrap().profile.nickname,
        client_name
    );
    println!("Host direct query successful.");

    // Client should first receive the lobby update and the initial lobby state
    for _ in 0..4 {
        let event = await_event(&mut client_events).await?;
        println!("event: {event}");
        match event {
            UiEvent::Peer(_) => { /* Good */ }
            UiEvent::AppState(AppState::Lobby) => { /* Good */ }
            UiEvent::Host(HostEvent::Changed { to }) => {
                assert_eq!(to, host_name);
            }
            other => panic!("Client received wrong event type during lobby phase: {other:?}"),
        }
    }
    // Client can also query the state directly
    let app_state = client_room.get_app_state().await?;
    assert!(matches!(app_state, AppState::Lobby));
    println!("Client direct query successful.");

    // --- GAME START ---
    println!("Starting Game");

    // Host starts the game
    host_room.start_game().await?;

    // Client should receive a GameStarted and a GameState Updated event.
    for _ in 0..2 {
        let event = await_event(&mut client_events).await?;
        println!("event: {event}");
        match event {
            UiEvent::AppState(AppState::InGame) => { /* Good */ }
            UiEvent::GameState(TestGameState { counter: 0 }) => { /* Good */ }
            _ => panic!("Client received wrong event type, got: {event}"),
        }
    }

    // Query the state directly
    let initial_state = client_room.get_game_state().await?;
    assert_eq!(initial_state, TestGameState { counter: 0 });
    println!("Client direct query of initial game state successful.");

    // --- ACTION PHASE ---
    println!("Submitting Action");

    // Client submits an action
    client_room.submit_action(TestGameAction::Increment).await?;

    await_counter_state(&mut client_events, 1).await?;
    let result = await_action_result(&mut client_events, true).await?;
    assert!(result.error.is_none());

    // Query the final state
    let final_state = client_room.get_game_state().await?;
    assert_eq!(final_state, TestGameState { counter: 1 });
    println!("Client direct query of final game state successful.");

    Ok(())
}

#[tokio::test]
async fn test_two_rapid_actions_from_same_peer_are_not_overwritten() -> anyhow::Result<()> {
    let (host_room, ticket_string, _host_id, _host_events) = setup_test_room("host").await?;
    let (client_room, mut client_events) = join_test_room("client", &ticket_string, 3).await?;
    await_lobby_update(&mut client_events, 2).await?;

    host_room.start_game().await?;
    await_game_start(&mut client_events).await?;

    client_room.submit_action(TestGameAction::Increment).await?;
    client_room.submit_action(TestGameAction::Increment).await?;

    await_counter_state(&mut client_events, 2).await?;
    Ok(())
}

#[tokio::test]
async fn test_invalid_action_returns_action_result() -> anyhow::Result<()> {
    let (host_room, ticket_string, _host_id, _host_events) = setup_test_room("host").await?;
    let (client_room, mut client_events) = join_test_room("client", &ticket_string, 3).await?;
    await_lobby_update(&mut client_events, 2).await?;

    host_room.start_game().await?;
    await_game_start(&mut client_events).await?;

    client_room.submit_action(TestGameAction::Reject).await?;
    let result = await_action_result(&mut client_events, false).await?;
    assert!(result.error.is_some());
    assert_eq!(client_room.get_game_state().await?.counter, 0);
    Ok(())
}

#[tokio::test]
async fn test_processed_actions_are_not_replayed_after_host_reconnect() -> anyhow::Result<()> {
    let host_dir = tempfile::tempdir()?.path().to_path_buf();
    let (host_room, ticket_string, host_id, _host_events) =
        setup_persistent_test_room("host", host_dir.clone()).await?;
    let (client_room, mut client_events) = join_test_room("client", &ticket_string, 3).await?;
    await_lobby_update(&mut client_events, 2).await?;

    host_room.start_game().await?;
    await_game_start(&mut client_events).await?;

    client_room.submit_action(TestGameAction::Increment).await?;
    await_counter_state(&mut client_events, 1).await?;

    drop(host_room);
    await_host_event(&mut client_events, HostEvent::Offline).await?;

    let (reconnected_host, _host_events) =
        GameRoom::join(TestGame, &ticket_string, Some(host_dir)).await?;
    assert_eq!(reconnected_host.id(), host_id);
    assert_eq!(reconnected_host.get_game_state().await?.counter, 1);

    client_room.submit_action(TestGameAction::Increment).await?;
    await_counter_state(&mut client_events, 2).await?;
    Ok(())
}

#[derive(Debug, Error)]
enum StartBlockedError {
    #[error("start blocked")]
    StartBlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct StartBlockedState;

#[derive(Debug, Clone, Serialize, Deserialize)]
enum StartBlockedAction {}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum StartBlockedRole {}

#[derive(Debug, Clone)]
struct StartBlockedGame;

impl GameLogic for StartBlockedGame {
    type GameState = StartBlockedState;
    type GameAction = StartBlockedAction;
    type PlayerRole = StartBlockedRole;
    type PlayerLeaveReason = ();
    type GameError = StartBlockedError;

    fn assign_roles(
        &self,
        _players: &PeerMap,
    ) -> Result<HashMap<EndpointId, Self::PlayerRole>, Self::GameError> {
        Ok(HashMap::new())
    }

    fn validate_start(
        &self,
        _players: &PeerMap,
        _roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> Result<(), Self::GameError> {
        Err(StartBlockedError::StartBlocked)
    }

    fn initial_state(
        &self,
        _players: &PeerMap,
        _roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> Result<Self::GameState, Self::GameError> {
        Ok(StartBlockedState)
    }

    fn apply_action(
        &self,
        _current_state: &mut Self::GameState,
        _player_id: &EndpointId,
        _action: &Self::GameAction,
    ) -> Result<(), Self::GameError> {
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
}

#[tokio::test]
async fn test_validate_start_failure_does_not_publish_partial_state() -> anyhow::Result<()> {
    let (room, mut events) = GameRoom::create(StartBlockedGame, None).await?;
    room.announce_presence("host").await?;
    tokio::time::timeout(std::time::Duration::from_secs(30), events.recv()).await?;

    assert!(room.start_game().await.is_err());
    assert!(room.get_game_state().await.is_err());
    assert_eq!(room.get_app_state().await?, AppState::Lobby);
    Ok(())
}
