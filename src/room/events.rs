use crate::state::*;
use crate::{GameLogic, GameRoom, PlayerInfo, PlayerMap};
use anyhow::Result;
use iroh::EndpointId;
use iroh_docs::engine::LiveEvent;
use n0_future::StreamExt as _;
use std::{collections::HashMap, str::FromStr as _};
use tokio::sync::mpsc;

/// Public events your library will send to the game UI
#[derive(Debug)]
pub enum GameEvent<G: GameLogic> {
    LobbyUpdated(PlayerMap),
    GameStarted(G::GameState),
    StateUpdated(G::GameState),
    AppStateChanged(AppState),
    ChatReceived(ChatMessage),
    Error(String),
}

impl<G: GameLogic + Send + Sync + 'static> GameRoom<G> {
    pub async fn start_event_loop(
        &self,
    ) -> Result<(tokio::task::JoinHandle<()>, mpsc::Receiver<GameEvent<G>>)> {
        let mut sub = self.doc.subscribe().await?;
        let (sender, receiver) = mpsc::channel(32); // Event channel for the UI

        let doc = self.doc.clone();
        let author = self.author.clone();
        let logic = self.logic.clone();
        let is_host = self.is_host;
        let iroh = self.iroh.clone();

        // host state
        let mut current_players: PlayerMap = HashMap::new();
        let mut current_state: Option<G::GameState> = None;

        let task_handle = tokio::spawn(async move {
            while let Some(Ok(event)) = sub.next().await {
                let entry = match event {
                    LiveEvent::InsertLocal { entry } => entry,
                    LiveEvent::InsertRemote { entry, .. } => entry,
                    _ => continue, // Skip events without an entry
                };
                let key = entry.key();

                // --- HOST-ONLY LOGIC ---
                if is_host {
                    if key.starts_with(PREFIX_JOIN) {
                        let node_id =
                            String::from_utf8_lossy(&key[PREFIX_JOIN.len()..]).to_string();

                        // Get the PlayerInfo payload
                        let player_info: PlayerInfo = match iroh.get_content_as(&entry).await {
                            Ok(info) => info,
                            Err(e) => {
                                eprintln!("Failed to parse PlayerInfo for {}: {}", node_id, e);
                                continue;
                            }
                        };
                        let player_id = match EndpointId::from_str(&node_id) {
                            Ok(id) => id,
                            Err(err) => {
                                eprintln!("Invalid EndpointId from key {}: {}", node_id, err);
                                continue;
                            }
                        };

                        current_players.insert(player_id, player_info);

                        // Broadcast the new canonical player list
                        let players_bytes = postcard::to_stdvec(&current_players).unwrap();
                        doc.set_bytes(author.clone(), KEY_PLAYERS.to_vec(), players_bytes)
                            .await
                            .ok();
                    } else if key.starts_with(PREFIX_ACTION) {
                        let node_id =
                            String::from_utf8_lossy(&key[PREFIX_ACTION.len()..]).to_string();
                        let player_id = match EndpointId::from_str(&node_id) {
                            Ok(id) => id,
                            Err(err) => {
                                eprintln!("Invalid EndpointId from key {}: {}", node_id, err);
                                continue;
                            }
                        };

                        // Ensure we have a state to apply the action to
                        if current_state.is_none() {
                            // This might happen if an action arrives before host has started game
                            eprintln!(
                                "Action from {} received before game state is initialized",
                                node_id
                            );
                            continue;
                        }

                        match iroh.get_content_as::<G::GameAction>(&entry).await {
                            Ok(action) => {
                                // Apply the game logic
                                let state_to_use = current_state.as_ref().unwrap(); // Safe due to check
                                match logic.apply_action(state_to_use, &player_id, &action) {
                                    Ok(new_state) => {
                                        // Broadcast the new authoritative state
                                        let state_bytes = postcard::to_stdvec(&new_state).unwrap();
                                        doc.set_bytes(
                                            author.clone(),
                                            KEY_GAME_STATE.to_vec(),
                                            state_bytes,
                                        )
                                        .await
                                        .ok();
                                    }
                                    Err(e) => {
                                        eprintln!("Invalid action from {}: {}", node_id, e);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to parse GameAction from {}: {}", node_id, e)
                            }
                        }
                    }
                }

                // --- ALL-PLAYERS LOGIC ---
                if key.starts_with(PREFIX_CHAT) {
                    match iroh.get_content_as::<ChatMessage>(&entry).await {
                        Ok(msg) => {
                            sender.send(GameEvent::ChatReceived(msg)).await.ok();
                        }
                        Err(e) => eprintln!("Failed to parse ChatMessage: {}", e),
                    }
                } else if key == KEY_PLAYERS {
                    match iroh.get_content_as::<PlayerMap>(&entry).await {
                        Ok(players) => {
                            current_players = players.clone(); // Update local cache
                            sender.send(GameEvent::LobbyUpdated(players)).await.ok();
                        }
                        Err(e) => eprintln!("Failed to parse PlayerMap: {}", e),
                    }
                } else if key == KEY_GAME_STATE {
                    match iroh.get_content_as::<G::GameState>(&entry).await {
                        Ok(state) => {
                            current_state = Some(state.clone()); // Update local cache
                            sender.send(GameEvent::StateUpdated(state)).await.ok();
                        }
                        Err(e) => eprintln!("Failed to parse GameState: {}", e),
                    }
                } else if key == KEY_APP_STATE {
                    match iroh.get_content_as::<AppState>(&entry).await {
                        Ok(app_state) => {
                            // If the state is InGame and we're the host, we need to create
                            // the initial state.
                            sender
                                .send(GameEvent::AppStateChanged(app_state))
                                .await
                                .ok();
                        }
                        Err(e) => eprintln!("Failed to parse AppState: {}", e),
                    }
                }
            }
        });

        Ok((task_handle, receiver))
    }
}
