use crate::{
    GameLogic, GameRoom, PlayerInfo,
    room::{AppState, PlayerMap, chat::ChatMessage, state::*},
};
use anyhow::{Result, anyhow};
use iroh_blobs::Hash;
use iroh_docs::{ContentStatus, engine::LiveEvent, sync::Entry};
use n0_future::StreamExt as _;
use std::{collections::HashMap, sync::Arc};
use tokio::{sync::mpsc, task::JoinHandle};

/// Public events your library will send to the game UI
#[derive(Debug)]
pub enum GameEvent<G: GameLogic> {
    LobbyUpdated(PlayerMap),
    StateUpdated(G::GameState),
    AppStateChanged(AppState),
    ChatReceived { id: PlayerInfo, msg: ChatMessage },
    HostDisconnected,
    Error(String),
}

impl<G: GameLogic> GameRoom<G> {
    pub(crate) async fn start_event_loop(
        &mut self,
    ) -> Result<(mpsc::Receiver<GameEvent<G>>, JoinHandle<()>)> {
        let mut sub = self.state.doc.subscribe().await?;
        let (sender, receiver) = mpsc::channel(32); // Event channel for the UI

        let state_data = self.state.clone();
        let logic = self.logic.clone();

        let mut pending_entries: HashMap<Hash, Entry> = HashMap::new();

        let task_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Listen for iroh doc events
                    Some(Ok(event)) = sub.next() => {
                        if let Some(entry) = parse_live_event(event, &mut pending_entries) {
                            match process_entry(&entry, &state_data, &logic).await {
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
                    else => break, // Stream finished
                }
            }
        });
        Ok((receiver, task_handle))
    }
}

async fn process_entry<G: GameLogic>(
    entry: &Entry,
    data: &StateData<G>,
    logic: &Arc<G>,
) -> Result<Option<GameEvent<G>>> {
    let is_host = data.is_host().await?;
    // --- HOST-ONLY LOGIC ---
    if is_host {
        if let Some(node_id) = entry.is_join() {
            let node_id = node_id?;
            // A player has joined the game room.
            // Get the PlayerInfo payload
            let player_info: PlayerInfo = match data.parse(&entry).await {
                Ok(info) => info,
                Err(e) => {
                    return Err(anyhow!("Failed to parse PlayerInfo for {}: {e}", &node_id,));
                }
            };
            // Broadcast the new canonical player list
            data.insert_player(node_id, &player_info).await.ok();
        } else if let Some(node_id) = entry.is_action_request() {
            let node_id = node_id?;
            // Ensure we have a state to apply the action to
            let current_state = &mut data.get_game_state().await?;

            match data.parse::<G::GameAction>(&entry).await {
                Ok(action) => {
                    // Apply the game logic and broadcast the new authoritative state
                    match logic.apply_action(current_state, &node_id, &action) {
                        Err(e) => {
                            let player = data.get_player_info(&node_id).await?.unwrap_or_default();
                            return Err(anyhow!("Invalid action from {player}: {e}"));
                        }
                        Ok(()) => data.set_game_state(current_state).await.ok(),
                    };
                }
                Err(e) => {
                    let player = data.get_player_info(&node_id).await?.unwrap_or_default();
                    return Err(anyhow!("Failed to parse GameAction from {player}: {e}",));
                }
            }
        }
    } else {
        // --- CLIENT-ONLY LOGIC ---
        if entry.is_game_state_update() {
            // only the host can update the Game State
            return match data.parse::<G::GameState>(&entry).await {
                Err(e) => Err(anyhow!("Failed to parse GameState: {e}")),
                Ok(state) => Ok(Some(GameEvent::StateUpdated(state))),
            };
        } else if entry.is_app_state_update() {
            // only the host can update the App State
            return match data.parse::<AppState>(&entry).await {
                Err(e) => Err(anyhow!("Failed to parse AppState: {e}")),
                Ok(app_state) => Ok(Some(GameEvent::AppStateChanged(app_state))),
            };
        }
    }

    // --- ALL-PLAYERS LOGIC ---
    if let Some(node_id) = entry.is_chat_message() {
        let node_id = node_id?;
        let player = data.get_player_info(&node_id).await?.unwrap_or_default();
        match data.parse::<ChatMessage>(&entry).await {
            Err(e) => Err(anyhow!("Failed to parse ChatMessage from {player}: {e}")),
            Ok(msg) => Ok(Some(GameEvent::ChatReceived {
                id: player.clone(),
                msg,
            })),
        }
    } else if entry.is_players_update() {
        match data.parse::<PlayerMap>(&entry).await {
            Err(e) => Err(anyhow!("Failed to parse PlayerMap: {e}")),
            Ok(players) => Ok(Some(GameEvent::LobbyUpdated(players))),
        }
    } else {
        Ok(None)
    }
}

/// Output a doc entry when a new one is ready.
fn parse_live_event(event: LiveEvent, pending_entries: &mut HashMap<Hash, Entry>) -> Option<Entry> {
    use ContentStatus::{Complete, Incomplete, Missing};
    match event {
        // TODO maybe add functionality to handle losing and gaining neighbours
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
