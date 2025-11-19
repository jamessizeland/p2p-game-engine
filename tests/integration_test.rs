mod common;
use common::*;

use anyhow::Result;
use p2p_game_engine::AppState;
use p2p_game_engine::GameEvent;
use p2p_game_engine::GameRoom;
use std::path::Path;
use std::time::Duration;
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
    let host_dir = Path::new("./temp/host/");
    tokio::fs::create_dir_all(&host_dir).await?;
    let client_dir = Path::new("./temp/client/");
    tokio::fs::create_dir_all(&client_dir).await?;

    println!("Setting up Host Room");
    let (host_room, mut host_events) = GameRoom::create(TestGame, host_dir.to_path_buf()).await?;
    let ticket_string = host_room.ticket().to_string();
    println!("Host Ticket: {}", &ticket_string);

    let host_name = "HostPlayer";
    println!("Announcing Host Presence");
    host_room.announce_presence(host_name).await?;
    let event = await_event(&mut host_events).await?;
    println!("Received Host Lobby Update: {event:?}");
    let host_id = host_room.id();
    match event {
        GameEvent::LobbyUpdated(players) => {
            assert_eq!(players.len(), 1);
            assert!(players.contains_key(&host_id));
            assert_eq!(players.get(&host_id).unwrap().name, host_name);
        }
        _ => panic!("Host received wrong event type"),
    }

    println!("Setting up Client Room");
    let (client_room, mut client_events) =
        GameRoom::join(TestGame, ticket_string, client_dir.to_path_buf()).await?;

    // --- LOBBY PHASE ---

    // Client announces their presence
    let client_name = "ClientPlayer";
    println!("Announcing Client Presence");
    client_room.announce_presence(client_name).await?;

    // Host should receive the lobby update
    println!("Waiting for Host Lobby Update...");
    let event = await_event(&mut host_events).await?;
    println!("Received Host Lobby Update: {event:?}");
    let client_id = client_room.id();
    match event {
        GameEvent::LobbyUpdated(players) => {
            assert_eq!(players.len(), 2);
            assert!(players.contains_key(&client_id));
            assert!(players.contains_key(&host_id));
            assert_eq!(players.get(&client_id).unwrap().name, client_name);
        }
        _ => panic!("Host received wrong event type"),
    }

    println!("Getting player map from host room...");

    // Host can also query the state directly
    let players = host_room.get_players_list().await?;
    println!("Players: {players}");
    assert_eq!(players.len(), 2);
    assert!(players.contains_key(&client_id));
    assert_eq!(players.get(&client_id).unwrap().name, client_name);
    println!("Host direct query successful.");

    // Client should first receive the lobby update and the initial lobby state
    for _ in 0..3 {
        let event = await_event(&mut client_events).await?;
        println!("event: {event:?}");
        match event {
            GameEvent::LobbyUpdated(_) => { /* Good */ }
            GameEvent::AppStateChanged(AppState::Lobby) => { /* Good */ }
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
        println!("event: {event:?}");
        match event {
            GameEvent::AppStateChanged(AppState::InGame) => { /* Good */ }
            GameEvent::StateUpdated(TestGameState { counter: 0 }) => { /* Good */ }
            _ => panic!("Client received wrong event type, got: {event:?}"),
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

    // Client should receive the new state (after host processes and broadcasts it)
    let event = await_event(&mut client_events).await?;

    match event {
        GameEvent::StateUpdated(TestGameState { counter: 1 }) => { /* Good */ }
        _ => panic!("Client received wrong event after action: {event:?}"),
    }

    // Query the final state
    let final_state = client_room.get_game_state().await?;
    assert_eq!(final_state, TestGameState { counter: 1 });
    println!("Client direct query of final game state successful.");

    Ok(())
}
