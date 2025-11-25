//! These tests handle scenarios where either the host disconnects or a player disconnects.
//!
//! If the host disconnects, the default behaviour is to:
//! - set the game into a Paused state.
//! - inform connected players.
//! - set the game into a Paused state.
//! - unless the host has quit the game, in which case a new host is elected to continue.
//!
//! If a non-host disconnects, the default behaviour is to:
//! - inform connected players.
//! - tbc...

mod common;

use common::*;
use p2p_game_engine::*;

async fn get_player_statuses(room: &GameRoom<TestGame>) -> anyhow::Result<Vec<PlayerStatus>> {
    Ok(room
        .get_players_list()
        .await?
        .iter()
        .map(|p| p.1.status)
        .collect())
}

#[tokio::test]
async fn test_host_disconnects_during_game_controlled() -> anyhow::Result<()> {
    // A "controlled" disconnect is when the host explicitly announces they are leaving.c

    // --- SETUP PHASE ---
    let (host_room, ticket_string, _host_id, _host_events) = setup_test_room("player1").await?;

    // Use two clients to ensure broadcast works
    let (client_room1, mut client_events1) = join_test_room("player2", &ticket_string, 3).await?;
    let (client_room2, mut client_events2) = join_test_room("player3", &ticket_string, 3).await?;

    // Wait for lobby to be fully populated for all clients
    await_lobby_update(&mut client_events1, 3).await?;
    await_lobby_update(&mut client_events2, 3).await?;

    // --- GAME START ---
    host_room.start_game().await?;

    // Wait for clients to enter the game
    await_game_start(&mut client_events1).await?;
    await_game_start(&mut client_events2).await?;

    {
        let player_list = get_player_statuses(&client_room1).await?;
        assert!(player_list.len() == 3);
        assert!(!player_list.contains(&PlayerStatus::Offline)); // everyone online
        let player_list = get_player_statuses(&client_room2).await?;
        assert!(player_list.len() == 3);
        assert!(!player_list.contains(&PlayerStatus::Offline)); // everyone online
    }

    // --- HOST LEAVES ---
    println!("Host leaving...");
    host_room.announce_leave(&LeaveReason::Forfeit).await?;
    drop(host_room);

    assert!(matches!(
        await_event(&mut client_events1).await?,
        UiEvent::HostDisconnected
    ));
    assert!(matches!(
        await_event(&mut client_events2).await?,
        UiEvent::HostDisconnected
    ));

    {
        let player_list = get_player_statuses(&client_room1).await?;
        assert!(player_list.len() == 3);
        assert!(player_list.contains(&PlayerStatus::Offline)); // someone offline
        let player_list = get_player_statuses(&client_room2).await?;
        assert!(player_list.len() == 3);
        assert!(player_list.contains(&PlayerStatus::Offline)); // someone offline
    }

    Ok(())
}

#[tokio::test]
async fn test_host_disconnects_during_game_uncontrolled() -> anyhow::Result<()> {
    // An "uncontrolled" disconnect is when the host process crashes or is dropped.

    // --- SETUP PHASE ---
    let (host_room, ticket_string, host_id, _host_events) = setup_test_room("player1").await?;

    let (client_room1, mut client_events1) = join_test_room("player2", &ticket_string, 3).await?;
    let (_client_room2, mut client_events2) = join_test_room("player3", &ticket_string, 3).await?;

    // Wait for lobby to be fully populated for all clients
    await_lobby_update(&mut client_events1, 3).await?;
    await_lobby_update(&mut client_events2, 3).await?;

    // --- GAME START ---
    host_room.start_game().await?;

    // Wait for clients to enter the game
    await_game_start(&mut client_events1).await?;
    await_game_start(&mut client_events2).await?;

    // --- HOST CRASHES ---
    drop(host_room);

    // Clients should receive announcements that the host player has left and why.
    // We only need to check one client for this test.
    let event = await_event(&mut client_events1).await?;
    assert!(matches!(event, UiEvent::HostDisconnected));

    // Check state directly
    // The app state should remain InGame. The UI is responsible for pausing.
    assert_eq!(client_room1.get_app_state().await?, AppState::InGame);

    // The host's player status should be updated to Offline by the other client,
    // which then syncs to this client. We'll wait for that lobby update.
    await_lobby_status_update(&mut client_events2, &host_id, PlayerStatus::Offline).await?;

    Ok(())
}

#[tokio::test]
async fn test_host_disconnects_during_game_and_reconnects() -> anyhow::Result<()> {
    // This requires more complex logic for host election and state reconciliation.
    // We'll leave this as a todo for now.
    todo!()
}

#[tokio::test]
async fn test_player_disconnects() -> anyhow::Result<()> {
    // This is the next step after handling host disconnections.
    todo!()
}
