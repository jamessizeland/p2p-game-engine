//! This is the basic test for setting up rooms and exchanging basic information between them.

mod common;
use common::*;
use iroh::EndpointId;
use p2p_game_engine::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

static PERSISTENT_ROOM_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn test_full_game_lifecycle() -> anyhow::Result<()> {
    let _room_guard = PERSISTENT_ROOM_TEST_LOCK.lock().await;
    // --- SETUP PHASE ---
    let host_name = "HostPlayer";
    let (host_room, ticket_string, _host_id, mut host_events) = setup_test_room(host_name).await?;

    let client_name = "ClientPlayer";
    let (client_room, mut client_events) = join_test_room(client_name, &ticket_string, 3).await?;
    await_peer_ready(&host_room, &client_room.id(), true).await?;

    // --- LOBBY PHASE ---
    let client_id = client_room.id();
    await_lobby_update(&mut host_events, 2).await?;

    println!("Getting player map from host room...");

    // Host can also query the state directly
    let players = await_peer_list_count(&host_room, 2).await?;
    println!("Players: {players}");
    assert_eq!(players.len(), 2);
    assert!(players.contains_key(&client_id));
    assert_eq!(
        players.get(&client_id).unwrap().profile.nickname,
        client_name
    );
    println!("Host direct query successful.");

    // Client should receive enough lobby state to see both players.
    await_lobby_update(&mut client_events, 2).await?;
    // Client can also query the state directly
    let app_state = client_room.get_app_state().await?;
    assert!(matches!(app_state, AppState::Lobby));
    let lobby_snapshot = client_room.snapshot().await?;
    assert_eq!(lobby_snapshot.local_id, client_room.id());
    assert_eq!(lobby_snapshot.host_id, Some(host_room.id()));
    assert!(!lobby_snapshot.is_host);
    assert_eq!(lobby_snapshot.app_state, AppState::Lobby);
    assert_eq!(lobby_snapshot.peers.len(), 2);
    assert!(lobby_snapshot.game_state.is_none());
    println!("Client direct query successful.");

    // --- GAME START ---
    println!("Starting Game");

    // Host starts the game
    host_room.start_game().await?;

    // Client should receive a GameStarted and a GameState Updated event.
    await_game_start(&mut client_events).await?;

    // Query the state directly
    let initial_state = client_room.get_game_state().await?;
    assert_eq!(initial_state, TestGameState { counter: 0 });
    let game_snapshot = client_room.snapshot().await?;
    assert_eq!(game_snapshot.app_state, AppState::InGame);
    assert_eq!(game_snapshot.game_state, Some(TestGameState { counter: 0 }));
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
    let _room_guard = PERSISTENT_ROOM_TEST_LOCK.lock().await;
    let (host_room, ticket_string, _host_id, mut host_events) = setup_test_room("host").await?;
    let (client_room, mut client_events) = join_test_room("client", &ticket_string, 3).await?;
    await_lobby_ready_update(&mut host_events, &client_room.id(), true).await?;
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
    let _room_guard = PERSISTENT_ROOM_TEST_LOCK.lock().await;
    let (host_room, ticket_string, _host_id, mut host_events) = setup_test_room("host").await?;
    let (client_room, mut client_events) = join_test_room("client", &ticket_string, 3).await?;
    await_lobby_ready_update(&mut host_events, &client_room.id(), true).await?;
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
async fn test_action_submission_is_rejected_in_lobby() -> anyhow::Result<()> {
    let _room_guard = PERSISTENT_ROOM_TEST_LOCK.lock().await;
    let (room, _ticket_string, _host_id, _events) = setup_test_room("host").await?;

    let result = room.submit_action(TestGameAction::Increment).await;
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "Cannot submit action from lobby"
    );
    Ok(())
}

#[tokio::test]
async fn test_enter_lobby_defaults_to_not_ready() -> anyhow::Result<()> {
    let _room_guard = PERSISTENT_ROOM_TEST_LOCK.lock().await;
    let (room, mut events) = GameRoom::create(TestGame, None).await?;

    room.enter_lobby("host").await?;
    let event = await_event(&mut events).await?;

    match event {
        UiEvent::Peer(players) => {
            let local_peer = players
                .get(&room.id())
                .expect("local peer should be present");
            assert!(!local_peer.ready);
        }
        _ => panic!("Host received wrong event type"),
    }
    Ok(())
}

#[tokio::test]
async fn test_processed_actions_are_not_replayed_after_host_reconnect() -> anyhow::Result<()> {
    let _room_guard = PERSISTENT_ROOM_TEST_LOCK.lock().await;
    let host_temp = tempfile::tempdir()?;
    let host_dir = host_temp.path().to_path_buf();
    let (host_room, ticket_string, host_id, mut host_events) =
        setup_persistent_test_room("host", host_dir.clone()).await?;
    let (client_room, mut client_events) = join_test_room("client", &ticket_string, 3).await?;
    await_lobby_ready_update(&mut host_events, &client_room.id(), true).await?;
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
    await_host_event(&mut client_events, HostEvent::Online).await?;

    client_room.submit_action(TestGameAction::Increment).await?;
    await_counter_state(&mut client_events, 2).await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct HostObserverState {
    started: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum HostObserverAction {}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum HostObserverRole {
    Player,
    Observer,
}

#[derive(Debug, Error)]
enum HostObserverError {
    #[error("no player")]
    NoPlayer,
}

#[derive(Debug, Clone)]
struct HostObserverGame;

impl GameLogic for HostObserverGame {
    type GameState = HostObserverState;
    type GameAction = HostObserverAction;
    type PlayerRole = HostObserverRole;
    type PlayerLeaveReason = ();
    type GameError = HostObserverError;

    fn is_observer_role(&self, role: &Self::PlayerRole) -> bool {
        matches!(role, HostObserverRole::Observer)
    }

    fn assign_roles(
        &self,
        players: &PeerMap,
    ) -> Result<HashMap<EndpointId, Self::PlayerRole>, Self::GameError> {
        Ok(players
            .iter()
            .map(|(id, peer)| {
                let role = if peer.profile.nickname == "player" {
                    HostObserverRole::Player
                } else {
                    HostObserverRole::Observer
                };
                (*id, role)
            })
            .collect())
    }

    fn validate_start(
        &self,
        _players: &PeerMap,
        roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> Result<(), Self::GameError> {
        if roles
            .values()
            .any(|role| matches!(role, HostObserverRole::Player))
        {
            Ok(())
        } else {
            Err(HostObserverError::NoPlayer)
        }
    }

    fn initial_state(
        &self,
        _players: &PeerMap,
        _roles: &HashMap<EndpointId, Self::PlayerRole>,
    ) -> Result<Self::GameState, Self::GameError> {
        Ok(HostObserverState { started: true })
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

    fn handle_player_forfeit(
        &self,
        _players: &mut PeerMap,
        _player_id: &EndpointId,
        _current_state: &mut Self::GameState,
    ) -> Result<ConnectionEffect, Self::GameError> {
        Ok(ConnectionEffect::NoChange)
    }
}

#[tokio::test]
async fn test_readiness_only_blocks_assigned_players() -> anyhow::Result<()> {
    let _room_guard = PERSISTENT_ROOM_TEST_LOCK.lock().await;
    let (host_room, mut host_events) = GameRoom::create(HostObserverGame, None).await?;
    let ticket_string = host_room.ticket().await?.to_string();
    host_room.announce_presence("host-observer").await?;
    tokio::time::timeout(std::time::Duration::from_secs(30), host_events.recv()).await?;

    let (player_room, mut player_events) =
        GameRoom::join(HostObserverGame, &ticket_string, None).await?;
    player_room.announce_presence("player").await?;
    tokio::time::timeout(std::time::Duration::from_secs(30), player_events.recv()).await?;
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            if host_room.get_peer_list().await?.len() == 2 {
                return anyhow::Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await??;

    let result = host_room.start_game().await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Peer player is not ready")
    );

    player_room.set_ready(true).await?;
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let players = host_room.get_peer_list().await?;
            if players
                .get(&player_room.id())
                .is_some_and(|peer| peer.ready)
            {
                return anyhow::Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await??;

    host_room.start_game().await?;
    assert_eq!(host_room.get_app_state().await?, AppState::InGame);
    Ok(())
}

#[tokio::test]
async fn test_start_game_waits_for_lobby_readiness() -> anyhow::Result<()> {
    let _room_guard = PERSISTENT_ROOM_TEST_LOCK.lock().await;
    let (host_room, ticket_string, _host_id, mut host_events) = setup_test_room("host").await?;

    let (client_room, mut client_events) = GameRoom::join(TestGame, &ticket_string, None).await?;
    client_room.announce_presence("client").await?;
    let client_id = client_room.id();

    await_lobby_contains(&mut client_events, &client_id).await?;
    await_lobby_update(&mut host_events, 2).await?;

    let result = host_room.start_game().await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Peer client is not ready")
    );
    assert_eq!(host_room.get_app_state().await?, AppState::Lobby);

    client_room.set_ready(true).await?;
    await_lobby_ready_update(&mut host_events, &client_id, true).await?;

    host_room.start_game().await?;
    await_game_start(&mut client_events).await?;
    Ok(())
}

#[tokio::test]
async fn test_online_host_claim_is_rejected() -> anyhow::Result<()> {
    let _room_guard = PERSISTENT_ROOM_TEST_LOCK.lock().await;
    let (host_room, ticket_string, _host_id, mut host_events) = setup_test_room("host").await?;
    let (client_room, _client_events) = join_test_room("client", &ticket_string, 3).await?;
    await_lobby_update(&mut host_events, 2).await?;

    let result = client_room.claim_host().await;
    assert!(result.is_err());
    assert!(host_room.is_host().await?);
    assert!(!client_room.is_host().await?);
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

    fn handle_player_forfeit(
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
    let _room_guard = PERSISTENT_ROOM_TEST_LOCK.lock().await;
    let (room, mut events) = GameRoom::create(StartBlockedGame, None).await?;
    room.announce_presence("host").await?;
    tokio::time::timeout(std::time::Duration::from_secs(30), events.recv()).await?;

    assert!(room.start_game().await.is_err());
    assert!(room.get_game_state().await.is_err());
    assert_eq!(room.get_app_state().await?, AppState::Lobby);
    Ok(())
}

#[tokio::test]
async fn test_join_rejects_wrong_game_type() -> anyhow::Result<()> {
    let _room_guard = PERSISTENT_ROOM_TEST_LOCK.lock().await;
    let (room, _events) = GameRoom::create(TestGame, None).await?;
    let ticket = room.ticket().await?.to_string();

    let result = GameRoom::join(StartBlockedGame, &ticket, None).await;
    assert!(result.is_err());
    Ok(())
}
