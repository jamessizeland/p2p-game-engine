use crate::{
    GameLogic, GameRoom,
    room::{AppState, PlayerInfo, PlayerMap, chat::ChatMessage, state::*},
};
use anyhow::{Result, anyhow};
use iroh_blobs::Hash;
use iroh_docs::{ContentStatus, engine::LiveEvent, sync::Entry};
use n0_future::StreamExt as _;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

/// Public events your library will send to the game UI
#[derive(Debug)]
pub enum GameEvent<G: GameLogic> {
    LobbyUpdated(PlayerMap),
    GameStarted(G::GameState, AppState),
    StateUpdated(G::GameState),
    AppStateChanged(AppState),
    ChatReceived(ChatMessage),
    HostDisconnected,
    Error(String),
}

impl<G: GameLogic + Clone> GameRoom<G> {
    pub async fn start_event_loop(&mut self) -> Result<mpsc::Receiver<GameEvent<G>>> {
        let mut sub = self.subscribe().await?;
        let (sender, receiver) = mpsc::channel(32); // Event channel for the UI

        let room_clone_for_task = self.clone();
        let room = self.clone();

        // host state
        let mut current_players: PlayerMap = HashMap::new();
        let mut current_state: Option<G::GameState> = None;
        let mut pending_entries: HashMap<Hash, Entry> = HashMap::new();

        // Client state
        let mut last_heartbeat = Instant::now();

        let task_handle = tokio::spawn(async move {
            // If we are the host, start a heartbeat task.
            if room_clone_for_task.is_host().await.unwrap() {
                tokio::spawn(async move {
                    loop {
                        if room_clone_for_task.set_heartbeat().await.is_err() {
                            // Stop if we can't write to the doc
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                });
            }

            let heartbeat_timeout = Duration::from_secs(5);

            loop {
                tokio::select! {
                    // Listen for iroh doc events
                    Some(Ok(event)) = sub.next() => {
                        if let Some(entry) = parse_live_event(event, &mut pending_entries) {

                            // If this is a heartbeat, update our timer
                            if entry.is_heartbeat() {
                                last_heartbeat = Instant::now();
                            }

                            match process_entry(&entry, &room, &mut current_players, &mut current_state).await {
                                Ok(None) => {} // No event to send
                                Err(e) => eprintln!("Error processing event: {}", e),
                                Ok(Some(event)) => {
                                    if sender.send(event).await.is_err() {
                                        break; // Channel closed
                                    }
                                }
                            }
                        }
                    },
                    // Periodically check for heartbeat timeout (clients only)
                    _ = tokio::time::sleep(Duration::from_secs(1)), if !room.is_host().await.unwrap() => {
                        if last_heartbeat.elapsed() > heartbeat_timeout {
                            if sender.send(GameEvent::HostDisconnected).await.is_err() {
                                break; // Channel closed
                            }
                            // TODO
                            // To avoid spamming, we can break or wait for a long time.
                            // For now, we'll stop checking. The app should handle this.
                            break;
                        }
                    },
                    else => break, // Stream finished
                }
            }
        });
        self.event_handle = Some(Arc::new(task_handle));
        Ok(receiver)
    }
}

async fn process_entry<G: GameLogic>(
    entry: &Entry,
    room: &GameRoom<G>,
    current_players: &mut PlayerMap,
    current_state: &mut Option<G::GameState>,
) -> Result<Option<GameEvent<G>>> {
    let is_host = room.is_host().await?;

    // --- HOST-ONLY LOGIC ---
    if is_host {
        if let Some(node_id) = entry.is_join() {
            let node_id = node_id?;
            if let Ok(app_state) = room.get_app_state().await {
                if app_state == AppState::InGame {
                    return Ok(None);
                }
            }
            // Get the PlayerInfo payload
            let player_info: PlayerInfo = match room.parse(&entry).await {
                Ok(info) => info,
                Err(e) => {
                    return Err(anyhow!("Failed to parse PlayerInfo for {}: {e}", &node_id,));
                }
            };
            current_players.insert(node_id, player_info);
            // Broadcast the new canonical player list
            room.set_player_list(&current_players).await.ok();
        } else if let Some(node_id) = entry.is_action_request() {
            let node_id = node_id?;
            // Ensure we have a state to apply the action to
            if current_state.is_none() {
                return Err(anyhow!(
                    "Action from {node_id} received before game state is initialized",
                ));
            }

            match room.parse::<G::GameAction>(&entry).await {
                Ok(action) => {
                    // Apply the game logic
                    let state_to_update = current_state.as_mut().unwrap(); // Safe due to check
                    match room.logic.apply_action(state_to_update, &node_id, &action) {
                        Ok(()) => {
                            // Broadcast the new authoritative state
                            room.set_game_state::<G>(state_to_update).await.ok();
                        }
                        Err(e) => return Err(anyhow!("Invalid action from {}: {}", node_id, e)),
                    }
                }
                Err(e) => {
                    return Err(anyhow!(
                        "Failed to parse GameAction from {}: {}",
                        node_id,
                        e
                    ));
                }
            }
        }
    }

    // --- ALL-PLAYERS LOGIC ---
    if let Some(_node_id) = entry.is_chat_message() {
        match room.parse::<ChatMessage>(&entry).await {
            Ok(msg) => Ok(Some(GameEvent::ChatReceived(msg))),
            Err(e) => Err(anyhow!("Failed to parse ChatMessage: {}", e)),
        }
    } else if entry.is_players_update() {
        match room.parse::<PlayerMap>(&entry).await {
            Ok(players) => {
                *current_players = players.clone(); // Update local cache
                Ok(Some(GameEvent::LobbyUpdated(players)))
            }
            Err(e) => Err(anyhow!("Failed to parse PlayerMap: {}", e)),
        }
    } else if entry.is_game_state_update() {
        match room.parse::<G::GameState>(&entry).await {
            Ok(state) => {
                // If we get the game state and the app is already "InGame", this is the start event.
                if let Ok(app_state) = room.get_app_state().await {
                    if app_state == AppState::InGame && !is_host {
                        *current_state = Some(state.clone());
                        return Ok(Some(GameEvent::GameStarted(state, app_state)));
                    }
                }
                *current_state = Some(state.clone()); // Update local cache
                Ok(Some(GameEvent::StateUpdated(state)))
            }
            Err(e) => Err(anyhow!("Failed to parse GameState: {}", e)),
        }
    } else if entry.is_app_state_update() {
        if !is_host {
            if let Ok(app_state) = room.parse::<AppState>(&entry).await {
                // If we get the app_state and we already have the game state, this is the start event.
                if app_state == AppState::InGame && current_state.is_some() {
                    let state = current_state.as_ref().unwrap().clone();
                    return Ok(Some(GameEvent::GameStarted(state, app_state)));
                }
                return Ok(Some(GameEvent::AppStateChanged(app_state)));
            }
        }
        Ok(None)
    } else if entry.is_heartbeat() {
        Ok(None) // Heartbeat is handled in the outer loop, no event needed
    } else {
        Ok(None)
    }
}

/// Output a doc entry when a new one is ready.
fn parse_live_event(event: LiveEvent, pending_entries: &mut HashMap<Hash, Entry>) -> Option<Entry> {
    use ContentStatus::{Complete, Incomplete, Missing};
    match event {
        LiveEvent::InsertLocal { entry } => Some(entry),
        LiveEvent::InsertRemote {
            entry,
            content_status: Complete,
            ..
        } => Some(entry),
        LiveEvent::InsertRemote {
            entry,
            content_status: Missing | Incomplete,
            ..
        } => {
            pending_entries.insert(entry.content_hash(), entry);
            None
        }
        LiveEvent::ContentReady { hash } => pending_entries.remove(&hash),
        _other => None,
    }
}
