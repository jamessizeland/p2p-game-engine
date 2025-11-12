mod common;
use common::*;

use anyhow::Result;
use p2p_game_engine::{GameEvent, GameRoom, Iroh};
use std::time::Duration;
use tempfile::tempdir;
use tokio::sync::mpsc;

async fn await_event(
    event: &mut mpsc::Receiver<GameEvent<TestGame>>,
) -> Result<GameEvent<TestGame>> {
    let duration = Duration::from_secs(2);
    tokio::time::timeout(duration, event.recv())
        .await?
        .ok_or_else(|| anyhow::anyhow!("Timed out waiting for event"))
}

#[tokio::test]
async fn test_full_game_lifecycle() -> anyhow::Result<()> {
    // --- SETUP PHASE ---

    println!("Creating Host Room");
    let host_dir = tempdir()?;
    let host_iroh = Iroh::new(host_dir.path().to_path_buf()).await?;
    let (host_room, ticket) = GameRoom::host(host_iroh, TestGame).await?;
    let (host_handle, mut host_events) = host_room.start_event_loop().await?;
    let ticket_string = ticket.to_string();
    println!("Host Ticket: {}", &ticket_string);

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
    let event = await_event(&mut host_events).await?;

    let client_id = client_room.id;
    match event {
        GameEvent::LobbyUpdated(players) => {
            assert_eq!(players.len(), 1);
            assert!(players.contains_key(&client_id));
            assert_eq!(players.get(&client_id).unwrap().name, client_name);
        }
        _ => panic!("Host received wrong event type"),
    }

    // Host can also query the state directly
    let players = host_room.get_players().await?.unwrap();
    assert_eq!(players.len(), 1);
    assert!(players.contains_key(&client_id));
    assert_eq!(players.get(&client_id).unwrap().name, client_name);
    println!("Host direct query successful.");

    // Client should first receive the lobby update and the initial lobby state
    for _ in 0..2 {
        let event = await_event(&mut client_events).await?;
        match event {
            GameEvent::LobbyUpdated(_) => { /* Good */ }
            GameEvent::AppStateChanged(state) => {
                assert!(matches!(state, p2p_game_engine::AppState::Lobby));
            }
            _ => panic!("Client received wrong event type during lobby phase: {event:?}"),
        }
    }
    // Client can also query the state directly
    let app_state = client_room.get_app_state().await?.unwrap();
    assert!(matches!(app_state, p2p_game_engine::AppState::Lobby));
    println!("Client direct query successful.");

    // --- GAME START ---
    println!("Starting Game");

    // Host starts the game
    host_room.start_game().await?;

    // Client should receive AppStateChanged and StateUpdated
    let mut game_started = false;
    let mut state_received = false;

    for _ in 0..2 {
        let event = await_event(&mut client_events).await?;
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

    // We can also query the state directly now
    let initial_state = client_room.get_game_state().await?.unwrap();
    assert_eq!(initial_state, TestGameState { counter: 0 });
    println!("Client direct query of initial game state successful.");

    // --- ACTION PHASE ---
    println!("Submitting Action");

    // Client submits an action
    client_room.submit_action(TestGameAction::Increment).await?;

    // Client should receive the new state (after host processes and broadcasts it)
    let event = await_event(&mut client_events).await?;

    match event {
        GameEvent::StateUpdated(state) => {
            assert_eq!(state, TestGameState { counter: 1 });
        }
        _ => panic!("Client received wrong event after action"),
    }

    // And we can query the final state
    let final_state = client_room.get_game_state().await?.unwrap();
    assert_eq!(final_state, TestGameState { counter: 1 });
    println!("Client direct query of final game state successful.");

    // --- CLEANUP ---
    host_handle.abort();
    client_handle.abort();
    host_dir.close()?;
    client_dir.close()?;

    Ok(())
}
