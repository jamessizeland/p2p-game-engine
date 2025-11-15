use crate::state::*;
use crate::{GameLogic, GameRoom, PlayerInfo, PlayerMap};
use anyhow::{Result, anyhow};
use iroh::EndpointId;
use iroh_blobs::Hash;
use iroh_docs::{ContentStatus, engine::LiveEvent, sync::Entry};
use n0_future::StreamExt as _;
use std::{collections::HashMap, str::FromStr as _};
use tokio::sync::mpsc;

/// Public events your library will send to the game UI
#[derive(Debug)]
pub enum GameEvent<G: GameLogic> {
    LobbyUpdated(PlayerMap),
    GameStarted(G::GameState, AppState),
    StateUpdated(G::GameState),
    AppStateChanged(AppState),
    ChatReceived(ChatMessage),
    Error(String),
}

impl<G: GameLogic + Clone> GameRoom<G> {
    pub async fn start_event_loop(
        &self,
    ) -> Result<(tokio::task::JoinHandle<()>, mpsc::Receiver<GameEvent<G>>)> {
        let mut sub = self.doc.subscribe().await?;
        let (sender, receiver) = mpsc::channel(32); // Event channel for the UI

        let room = self.clone();

        // host state
        let mut current_players: PlayerMap = HashMap::new();
        let mut current_state: Option<G::GameState> = None;
        let mut pending_entries: HashMap<Hash, Entry> = HashMap::new();

        let task_handle = tokio::spawn(async move {
            use ContentStatus::*;
            use LiveEvent::*;
            while let Some(Ok(event)) = sub.next().await {
                let entry: Option<Entry> = match event {
                    InsertLocal { entry } => Some(entry),
                    InsertRemote {
                        entry,
                        content_status: Complete,
                        ..
                    } => Some(entry),
                    InsertRemote {
                        entry,
                        content_status: Missing | Incomplete,
                        ..
                    } => {
                        // Content is not ready, cache the entry to process later.
                        pending_entries.insert(entry.content_hash(), entry);
                        None
                    }

                    ContentReady { hash } => pending_entries.remove(&hash),
                    _other => None, // Skip other events
                };
                let Some(entry) = entry else {
                    continue;
                };
                match process_entry(&entry, &room, &mut current_players, &mut current_state).await {
                    Ok(None) => {} // No event to send
                    Err(e) => eprintln!("Error processing event: {}", e),
                    Ok(Some(event)) => {
                        if let Err(e) = sender.send(event).await {
                            eprintln!("Error sending event to UI: {}", e);
                        }
                    }
                }
            }
        });
        Ok((task_handle, receiver))
    }
}

async fn process_entry<G: GameLogic>(
    entry: &Entry,
    room: &GameRoom<G>,
    current_players: &mut PlayerMap,
    current_state: &mut Option<G::GameState>,
) -> Result<Option<GameEvent<G>>> {
    let key = entry.key();
    let GameRoom {
        iroh,
        doc,
        author,
        logic,
        is_host,
        ..
    } = room;

    // --- HOST-ONLY LOGIC ---
    if *is_host {
        if key.starts_with(PREFIX_JOIN) {
            // Don't allow new players to be added to the official player list if the game is running.
            // They can still join as observers. We check the AppState from the doc.
            if let Ok(Some(app_state)) = room.get_app_state().await {
                if app_state == AppState::InGame {
                    // The game has started. New joiners are observers and won't be added to the KEY_PLAYERS map.
                    // They will still receive all other events.
                    return Ok(None);
                }
            }
            let node_id = String::from_utf8_lossy(&key[PREFIX_JOIN.len()..]).to_string();

            // Get the PlayerInfo payload
            let player_info: PlayerInfo = match iroh.get_content_as(&entry).await {
                Ok(info) => info,
                Err(e) => {
                    return Err(anyhow!("Failed to parse PlayerInfo for {}: {}", node_id, e));
                }
            };
            let player_id = match EndpointId::from_str(&node_id) {
                Ok(id) => id,
                Err(err) => {
                    return Err(anyhow!("Invalid EndpointId from key {}: {}", node_id, err));
                }
            };

            current_players.insert(player_id, player_info);

            // Broadcast the new canonical player list
            let players_bytes = postcard::to_stdvec(&current_players).unwrap();
            doc.set_bytes(author.clone(), KEY_PLAYERS.to_vec(), players_bytes)
                .await
                .ok();
        } else if key.starts_with(PREFIX_ACTION) {
            let node_id = String::from_utf8_lossy(&key[PREFIX_ACTION.len()..]).to_string();
            let player_id = match EndpointId::from_str(&node_id) {
                Ok(id) => id,
                Err(err) => {
                    return Err(anyhow!("Invalid EndpointId from key {}: {}", node_id, err));
                }
            };

            // Ensure we have a state to apply the action to
            if current_state.is_none() {
                return Err(anyhow!(
                    "Action from {} received before game state is initialized",
                    node_id
                ));
            }

            match iroh.get_content_as::<G::GameAction>(&entry).await {
                Ok(action) => {
                    // Apply the game logic
                    let state_to_update = current_state.as_mut().unwrap(); // Safe due to check
                    match logic.apply_action(state_to_update, &player_id, &action) {
                        Ok(()) => {
                            // Broadcast the new authoritative state
                            let state_bytes = postcard::to_stdvec(state_to_update).unwrap();
                            doc.set_bytes(author.clone(), KEY_GAME_STATE.to_vec(), state_bytes)
                                .await
                                .ok();
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
    if key.starts_with(PREFIX_CHAT) {
        match iroh.get_content_as::<ChatMessage>(&entry).await {
            Ok(msg) => Ok(Some(GameEvent::ChatReceived(msg))),
            Err(e) => Err(anyhow!("Failed to parse ChatMessage: {}", e)),
        }
    } else if key == KEY_PLAYERS {
        match iroh.get_content_as::<PlayerMap>(&entry).await {
            Ok(players) => {
                *current_players = players.clone(); // Update local cache
                Ok(Some(GameEvent::LobbyUpdated(players)))
            }
            Err(e) => Err(anyhow!("Failed to parse PlayerMap: {}", e)),
        }
    } else if key == KEY_GAME_STATE {
        match iroh.get_content_as::<G::GameState>(&entry).await {
            Ok(state) => {
                *current_state = Some(state.clone()); // Update local cache
                Ok(Some(GameEvent::StateUpdated(state)))
            }
            Err(e) => Err(anyhow!("Failed to parse GameState: {}", e)),
        }
    } else if key == KEY_APP_STATE {
        match iroh.get_content_as::<AppState>(&entry).await {
            Ok(app_state) => {
                // Only clients should react to this event by changing state.
                // The host initiates this change and should not react to its own broadcast.
                match is_host {
                    true => Ok(None),
                    false => Ok(Some(GameEvent::AppStateChanged(app_state))),
                }
            }
            Err(e) => Err(anyhow!("Failed to parse AppState: {}", e)),
        }
    } else {
        Ok(None)
    }
}
