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
use tokio::time::{Duration, sleep};

#[tokio::test]
async fn test_host_disconnects_during_game_controlled() -> anyhow::Result<()> {
    // A "controlled" disconnect is when the host explicitly announces they are leaving.

    // --- SETUP PHASE ---
    let (host_room, ticket_string, _host_id, _host_events) = setup_test_room("player1").await?;

    // Use two clients to ensure broadcast works
    let (_client_room1, mut client_events1) = join_test_room("player2", &ticket_string, 3).await?;

    // Wait for lobby to be fully populated for all clients
    await_lobby_update(&mut client_events1, 2).await?;

    // --- GAME START ---
    host_room.start_game().await?;

    // Wait for clients to enter the game
    await_game_start(&mut client_events1).await?;

    // --- HOST LEAVES ---
    println!("Host leaving...");
    host_room
        .announce_leave(&LeaveReason::ApplicationClosed)
        .await?;

    // Give iroh a moment to sync the leave announcement before we drop the host
    sleep(Duration::from_millis(200)).await;

    // Clients should receive announcements that the host player has left and why.
    // First event is HostDisconnected
    // let event1 = await_event(&mut client_events1).await?;
    // assert!(matches!(event1, UiEvent::HostDisconnected));

    // // Second event is the lobby update showing the host is gone
    // let event2 = await_event(&mut client_events1).await?;
    // if let UiEvent::LobbyUpdated(players) = event2 {
    //     assert_eq!(players.len(), 1); // Only 1 client left
    // } else {
    //     panic!("Expected LobbyUpdated, got {event2:?}");
    // }
    for index in 0..3 {
        let event = await_event(&mut client_events1).await?;
        println!("{index}: Client event: {event:?}");
    }

    // drop(host_room);

    Ok(())
}

#[tokio::test]
async fn test_host_disconnects_during_game_uncontrolled() -> anyhow::Result<()> {
    // An "uncontrolled" disconnect is when the host process crashes or is dropped.

    // --- SETUP PHASE ---
    let (host_room, ticket_string, _host_id, _host_events) = setup_test_room("player1").await?;

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

    // --- HOST CRASHES ---
    drop(host_room);

    // Clients should receive announcements that the host player has left and why.
    for mut events in [client_events1, client_events2] {
        // First event is HostDisconnected
        let event1 = await_event(&mut events).await?;
        assert!(matches!(event1, UiEvent::HostDisconnected));

        // Second event is AppStateChanged to Paused
        let event2 = await_event(&mut events).await?;
        assert!(matches!(event2, UiEvent::AppStateChanged(AppState::Paused)));
    }

    // Check state directly
    assert_eq!(client_room1.get_app_state().await?, AppState::Paused);
    assert_eq!(client_room2.get_app_state().await?, AppState::Paused);

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
