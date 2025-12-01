//! This is the basic test for setting up rooms and exchanging basic information between them.

mod common;
use common::*;
use p2p_game_engine::*;

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

    // Client should receive the new state (after host processes and broadcasts it)
    let event = await_event(&mut client_events).await?;

    match event {
        UiEvent::GameState(TestGameState { counter: 1 }) => { /* Good */ }
        _ => panic!("Client received wrong event after action: {event}"),
    }

    // Query the final state
    let final_state = client_room.get_game_state().await?;
    assert_eq!(final_state, TestGameState { counter: 1 });
    println!("Client direct query of final game state successful.");

    Ok(())
}
