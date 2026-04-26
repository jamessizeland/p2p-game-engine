//! These tests handle scenarios where either the host disconnects or a peer disconnects.
//!
//! If the host disconnects, the default behaviour is to:
//! - set the game into a Paused state.
//! - inform connected peers.
//! - set the game into a Paused state.
//! - unless the host has quit the game, in which case a new host is elected to continue.
//!
//! If a non-host disconnects, the default behaviour is to:
//! - inform connected peers.
//! - host sets their status to Offline.

mod common;

use common::*;
use p2p_game_engine::*;

async fn get_peer_statuses(room: &GameRoom<TestGame>) -> anyhow::Result<Vec<PeerStatus>> {
    Ok(room
        .get_peer_list()
        .await?
        .iter()
        .map(|p| p.1.status)
        .collect())
}

#[tokio::test]
async fn test_host_disconnects_during_game_controlled() -> anyhow::Result<()> {
    // A "controlled" disconnect is when the host explicitly announces they are leaving.

    // --- SETUP PHASE ---
    let (host_room, ticket_string, _host_id, mut host_events) = setup_test_room("peer1").await?;

    // Use two clients to ensure broadcast works
    let (client_room1, mut client_events1) = join_test_room("peer2", &ticket_string, 3).await?;
    let (client_room2, mut client_events2) = join_test_room("peer3", &ticket_string, 3).await?;

    // Wait for lobby to be fully populated for all clients
    await_lobby_ready_update(&mut host_events, &client_room1.id(), true).await?;
    await_lobby_ready_update(&mut host_events, &client_room2.id(), true).await?;
    await_lobby_update(&mut client_events1, 3).await?;
    await_lobby_update(&mut client_events2, 3).await?;

    // --- GAME START ---
    host_room.start_game().await?;

    // Wait for clients to enter the game
    await_game_start(&mut client_events1).await?;
    await_game_start(&mut client_events2).await?;

    {
        let peer_list = get_peer_statuses(&client_room1).await?;
        assert!(peer_list.len() == 3);
        assert!(!peer_list.contains(&PeerStatus::Offline)); // everyone online
        let peer_list = get_peer_statuses(&client_room2).await?;
        assert!(peer_list.len() == 3);
        assert!(!peer_list.contains(&PeerStatus::Offline)); // everyone online
    }

    // --- HOST LEAVES ---
    println!("Host leaving...");
    host_room
        .announce_leave(&LeaveReason::ApplicationClosed)
        .await?;

    await_host_event(&mut client_events1, HostEvent::Offline).await?;
    await_host_event(&mut client_events2, HostEvent::Offline).await?;

    {
        let peer_list = get_peer_statuses(&client_room1).await?;
        assert!(peer_list.len() == 3);
        assert!(peer_list.contains(&PeerStatus::Offline)); // someone offline
        let peer_list = get_peer_statuses(&client_room2).await?;
        assert!(peer_list.len() == 3);
        assert!(peer_list.contains(&PeerStatus::Offline)); // someone offline
    }

    Ok(())
}

#[tokio::test]
async fn test_host_disconnects_during_game_uncontrolled() -> anyhow::Result<()> {
    // An "uncontrolled" disconnect is when the host process crashes or is dropped.

    // --- SETUP PHASE ---
    let (host_room, ticket_string, host_id, mut host_events) = setup_test_room("peer1").await?;

    let (client_room, mut client_events) = join_test_room("peer2", &ticket_string, 3).await?;

    // Wait for lobby to be fully populated
    await_lobby_ready_update(&mut host_events, &client_room.id(), true).await?;
    await_lobby_update(&mut client_events, 2).await?;

    // --- GAME START ---
    host_room.start_game().await?;

    // Wait for client to enter the game
    await_game_start(&mut client_events).await?;

    // --- HOST CRASHES ---
    drop(host_room);

    // Client should receive an announcement that the host peer has left.
    await_host_event(&mut client_events, HostEvent::Offline).await?;

    // Check state directly
    // The app state should report Paused. This is a synthetic state not held in the document.
    assert_eq!(client_room.get_app_state().await?, AppState::Paused);

    // The host's peer status is inferred to be Offline by clients when the host disconnects.
    // This is a synthetic state change on the client, not a document update, so no `UiEvent::Peer`
    // is emitted. We can verify this by querying the state directly.
    let peers = client_room.get_peer_list().await?;
    let host_status = peers.get(&host_id).unwrap().status;
    assert_eq!(host_status, PeerStatus::Offline);

    Ok(())
}

#[tokio::test]
async fn test_host_disconnects_during_game_and_reconnects() -> anyhow::Result<()> {
    // During an active game, the host disconnects without reporting they lose or forfeit.
    // the game state should enter an inferred pause, preventing other peers from
    // submitting actions until the host reconnects.
    // --- SETUP PHASE ---
    let host_temp = tempfile::tempdir()?;
    let host_dir = host_temp.path().to_path_buf();
    let (host_room, ticket_string, host_id, mut host_events) =
        setup_persistent_test_room("peer1", host_dir.clone()).await?;

    let (client_room, mut client_events) = join_test_room("peer2", &ticket_string, 3).await?;
    await_lobby_ready_update(&mut host_events, &client_room.id(), true).await?;
    await_lobby_update(&mut client_events, 2).await?;

    assert_eq!(client_room.get_app_state().await?, AppState::Lobby);

    // --- GAME START ---
    host_room.start_game().await?;
    await_game_start(&mut client_events).await?;

    // --- HOST CRASHES ---
    println!("Crashing host...");
    drop(host_room);

    // Client should see the host disconnect.
    await_host_event(&mut client_events, HostEvent::Offline).await?;
    println!("Client detected host disconnection.");

    assert_eq!(client_room.get_app_state().await?, AppState::Paused);

    // --- HOST RECONNECTS ---
    println!("Reconnecting host...");
    let (reconnected_host, _new_host_events) =
        GameRoom::join(TestGame, &ticket_string, Some(host_dir)).await?;

    // The reconnected host should have the same ID and be recognized as host.
    assert_eq!(reconnected_host.id(), host_id);
    assert!(reconnected_host.is_host().await?);
    println!("Host reconnected successfully and is host.");

    // Client should see the host come back online via a NeighborUp event, and unpause their state.
    await_host_event(&mut client_events, HostEvent::Online).await?;
    assert_eq!(client_room.get_app_state().await?, AppState::InGame);
    println!("Client detected host reconnection and unpaused.");
    Ok(())
}

#[tokio::test]
async fn test_peer_disconnects_during_lobby() -> anyhow::Result<()> {
    // A peer leaves the room for any reason, before the game has started.
    // They are reassigned to be an observer, should they rejoin later.
    // (we never fully remove a peer from the PeerMap once they have been registered)

    // --- SETUP PHASE ---
    let (_host_room, ticket_string, _host_id, mut host_events) = setup_test_room("host").await?;

    // Use a persistent client so we can reconnect with the same ID
    let client_dir = tempfile::tempdir()?;
    let (client_room, mut client_events) = GameRoom::join(
        TestGame,
        &ticket_string,
        Some(client_dir.path().to_path_buf()),
    )
    .await?;
    client_room.announce_presence("client").await?;
    let client_id = client_room.id();

    // Wait for lobby
    await_lobby_update(&mut host_events, 2).await?;
    await_lobby_update(&mut client_events, 2).await?;

    // --- CLIENT DISCONNECTS ---
    drop(client_room);

    // Host should see client offline
    await_lobby_status_update(&mut host_events, &client_id, PeerStatus::Offline).await?;

    // --- CLIENT RECONNECTS ---
    let (client_room, _client_events) = GameRoom::join(
        TestGame,
        &ticket_string,
        Some(client_dir.path().to_path_buf()),
    )
    .await?;
    assert_eq!(client_room.id(), client_id);

    // Host should see client online
    await_lobby_status_update(&mut host_events, &client_id, PeerStatus::Online).await?;

    Ok(())
}

#[tokio::test]
async fn test_peer_disconnects_during_game() -> anyhow::Result<()> {
    // A peer leaves the room without registering a loss or forfeit.
    // They will be marked as offline by the host and the game will continue until
    // it is their turn to act.

    // --- SETUP PHASE ---
    let (host_room, ticket_string, _host_id, mut host_events) = setup_test_room("host").await?;

    // Use a persistent client
    let client_dir = tempfile::tempdir()?;
    let (client_room, mut client_events) = GameRoom::join(
        TestGame,
        &ticket_string,
        Some(client_dir.path().to_path_buf()),
    )
    .await?;
    client_room.announce_presence("client").await?;
    let client_id = client_room.id();

    // Wait for lobby
    await_lobby_update(&mut host_events, 2).await?;
    await_lobby_update(&mut client_events, 2).await?;
    client_room.set_ready(true).await?;
    await_lobby_ready_update(&mut host_events, &client_id, true).await?;

    // --- GAME START ---
    host_room.start_game().await?;
    await_game_start(&mut client_events).await?;

    // --- CLIENT DISCONNECTS ---
    drop(client_room);

    // Host should see client offline
    await_lobby_status_update(&mut host_events, &client_id, PeerStatus::Offline).await?;

    // --- CLIENT RECONNECTS ---
    let (client_room, _client_events) = GameRoom::join(
        TestGame,
        &ticket_string,
        Some(client_dir.path().to_path_buf()),
    )
    .await?;
    assert_eq!(client_room.id(), client_id);

    // Host should see client online
    await_lobby_status_update(&mut host_events, &client_id, PeerStatus::Online).await?;

    // Client should receive current game state
    let state = client_room.get_game_state().await?;
    assert_eq!(state.counter, 0);

    // Client should be able to continue playing (submit action)
    client_room.submit_action(TestGameAction::Increment).await?;

    await_counter_state(&mut host_events, 1).await?;

    Ok(())
}
#[tokio::test]
async fn test_client_peer_forfeits() -> anyhow::Result<()> {
    // Non-host peer loses or chooses to forfeit.
    // In this scenario they should be switched to being an observer and can continue
    // to stay subscribed to the game state but no-longer act.

    let (host_room, ticket_string, _host_id, mut host_events) = setup_test_room("host").await?;
    let (client_room, mut client_events) = join_test_room("client", &ticket_string, 3).await?;
    let client_id = client_room.id();

    await_lobby_ready_update(&mut host_events, &client_id, true).await?;
    await_lobby_update(&mut client_events, 2).await?;

    host_room.start_game().await?;
    await_game_start(&mut client_events).await?;

    client_room.forfeit().await?;
    await_lobby_observer_update(&mut host_events, &client_id, true).await?;

    client_room.submit_action(TestGameAction::Increment).await?;
    let result = await_action_result(&mut client_events, false).await?;
    assert_eq!(result.error.as_deref(), Some("Peer is an observer"));

    Ok(())
}

#[tokio::test]
async fn test_host_forfeits() -> anyhow::Result<()> {
    // During an active game, the hosting peer loses or chooses to forfeit.
    // In this scenario the game should be able to continue without them needing to stay online.
    // They will be switched to being an observer, and will elect a new host to take over if they
    // go offline.

    let (host_room, ticket_string, host_id, mut host_events) = setup_test_room("host").await?;
    let (client_room1, mut client_events1) = join_test_room("client1", &ticket_string, 3).await?;
    let (client_room2, mut client_events2) = join_test_room("client2", &ticket_string, 3).await?;

    await_lobby_ready_update(&mut host_events, &client_room1.id(), true).await?;
    await_lobby_ready_update(&mut host_events, &client_room2.id(), true).await?;

    host_room.start_game().await?;
    await_game_start(&mut client_events1).await?;
    await_game_start(&mut client_events2).await?;

    let expected_host = [client_room1.id(), client_room2.id()]
        .into_iter()
        .min_by_key(|id| id.to_string())
        .unwrap();
    let expected_name = if expected_host == client_room1.id() {
        "client1"
    } else {
        "client2"
    };

    host_room.forfeit().await?;
    await_lobby_observer_update(&mut host_events, &host_id, true).await?;
    await_host_event(
        &mut client_events1,
        HostEvent::Changed {
            to: expected_name.to_string(),
        },
    )
    .await?;
    await_host_event(
        &mut client_events2,
        HostEvent::Changed {
            to: expected_name.to_string(),
        },
    )
    .await?;

    assert_eq!(
        client_room1.is_host().await?,
        expected_host == client_room1.id()
    );
    assert_eq!(
        client_room2.is_host().await?,
        expected_host == client_room2.id()
    );

    let new_host_room = if expected_host == client_room1.id() {
        &client_room1
    } else {
        &client_room2
    };
    new_host_room
        .submit_action(TestGameAction::Increment)
        .await?;
    await_counter_state(&mut host_events, 1).await?;

    Ok(())
}
