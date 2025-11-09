mod common;
use common::*;

use p2p_game_engine::{GameEvent, GameRoom, Iroh}; // Use your crate's name
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_full_game_lifecycle() -> anyhow::Result<()> {
    // --- SETUP PHASE ---

    // Host creates a new room
    println!("Creating Host Room");
    let host_dir = tempdir()?;
    let host_iroh = Iroh::new(host_dir.path().to_path_buf()).await?;
    let (host_room, ticket) = GameRoom::host(host_iroh, TestGame).await?;
    let (host_handle, mut host_events) = host_room.start_event_loop().await?;
    let ticket_string = ticket.to_string();
    println!("Host Ticket: {}", &ticket_string);

    // Client joins the room
    println!("Creating Client Room");
    let client_dir = tempdir()?;
    let client_iroh = Iroh::new(client_dir.path().to_path_buf()).await?;
    let client_room = GameRoom::join(client_iroh, TestGame, ticket_string).await?;
    let (client_handle, mut client_events) = client_room.start_event_loop().await?;

    // --- LOBBY PHASE ---

    // Client announces their presence
    let client_name = "ClientPlayer";
    println!("Announcing Client Presence");
    client_room.announce_presence(client_name).await?;

    // Host should receive the lobby update
    println!("Waiting for Host Lobby Update...");
    let event = tokio::time::timeout(Duration::from_secs(5), host_events.recv())
        .await?
        .expect("Host did not receive event");
    println!("{event:?}");

    let client_id = client_room.id;
    match event {
        GameEvent::LobbyUpdated(players) => {
            assert_eq!(players.len(), 1);
            assert!(players.contains_key(&client_id));
            assert_eq!(players.get(&client_id).unwrap().name, client_name);
        }
        _ => panic!("Host received wrong event type"),
    }

    // --- GAME START ---
    println!("Starting Game");

    // Host starts the game
    host_room.start_game().await?;

    // Client should receive AppStateChanged and StateUpdated
    let mut game_started = false;
    let mut state_received = false;

    for _ in 0..2 {
        let event = tokio::time::timeout(Duration::from_secs(5), client_events.recv())
            .await?
            .expect("Client did not receive event");
        println!("event: {event:?}");
        match event {
            GameEvent::AppStateChanged(state) => {
                println!("app state: {state:?}");
                assert!(matches!(state, p2p_game_engine::AppState::InGame));
                game_started = true;
            }
            GameEvent::StateUpdated(state) => {
                assert_eq!(state, TestGameState { counter: 0 });
                state_received = true;
            }
            _ => panic!("Client received wrong event type"),
        }
    }
    assert!(
        game_started && state_received,
        "Client did not receive all start events"
    );

    // --- ACTION PHASE ---
    println!("Submitting Action");

    // Client submits an action
    client_room.submit_action(TestGameAction::Increment).await?;

    // Client should receive the new state (after host processes and broadcasts it)
    let event = tokio::time::timeout(Duration::from_secs(5), client_events.recv())
        .await?
        .expect("Client did not receive state update");

    match event {
        GameEvent::StateUpdated(state) => {
            assert_eq!(state, TestGameState { counter: 1 });
        }
        _ => panic!("Client received wrong event after action"),
    }

    // (Optional) Host should also receive its own broadcast
    let host_event = tokio::time::timeout(Duration::from_secs(5), host_events.recv())
        .await?
        .expect("Host did not receive state update");

    match host_event {
        GameEvent::StateUpdated(state) => {
            assert_eq!(state, TestGameState { counter: 1 });
        }
        _ => panic!("Host received wrong event after action"),
    }

    // --- CLEANUP ---
    host_handle.abort();
    client_handle.abort();
    host_dir.close()?;
    client_dir.close()?;

    Ok(())
}
